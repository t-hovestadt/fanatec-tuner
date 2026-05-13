mod carlist;
mod config;
mod display;
mod games;
mod hid;
mod led;
mod monitor;
mod profile;
mod sdk;
mod tuning;

use clap::{Parser, Subcommand};
use hid::REPORT_SIZE;
use tuning::{
    build_fb_write_report, build_full_write_report, build_request_report, build_select_slot_report,
    build_wake_report, is_tuning_report, parse_tuning_report, ADDR_NDP,
};

#[derive(Parser)]
#[command(
    name = "fanatec-tuner",
    about = "Read and apply Fanatec wheel tuning profiles"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Apply a .pws profile to the connected wheel base
    Apply {
        /// Path to the .pws file
        path: std::path::PathBuf,
    },
    /// List available profiles from the configured profiles directory
    List,
    /// Scan the profiles directory and show game/car mapping (verbose)
    Scan,
    /// Dump HID caps and probe write/read strategies on all collections
    Diag,
    /// Watch for game/car changes and auto-apply matching profiles
    Monitor,
    /// Systematically test 8 toggle+write sequences to find the correct write protocol
    WriteTest,
    /// Test LED and display control without a game running
    LedTest,
    /// Persistent-handle write stress: 3 writes on the same open handles, hex-dumped
    WriteStress,
    /// Load the Fanatec SDK DLL, read tuning state, set a test param, verify, restore
    SdkTest,
    /// Passively capture HID reports from col03/col01 (read-only, coexists with Fanatec App)
    Sniff {
        /// Print all 64 bytes per report instead of the first 32
        #[arg(long)]
        verbose: bool,
        /// Decode reports into human-readable fields (tuning, LED, protobuf, col01 state)
        #[arg(long)]
        decode: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    // Load config (fanatec-tuner.toml next to the exe, or CWD)
    let config_path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("fanatec-tuner.toml")))
        .unwrap_or_else(|| std::path::PathBuf::from("fanatec-tuner.toml"));
    let cfg = match config::load(&config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("warning: {}", e);
            config::Config::default()
        }
    };

    match cli.command {
        None => cmd_read(),
        Some(Command::Apply { path }) => cmd_apply(&path),
        Some(Command::List) => cmd_list(&cfg, false),
        Some(Command::Scan) => cmd_list(&cfg, true),
        Some(Command::Diag) => cmd_diag(),
        Some(Command::Monitor) => cmd_monitor(&cfg),
        Some(Command::WriteTest) => cmd_write_test(),
        Some(Command::LedTest) => cmd_led_test(),
        Some(Command::WriteStress) => cmd_write_stress(&cfg),
        Some(Command::SdkTest) => cmd_sdk_test(),
        Some(Command::Sniff { verbose, decode }) => cmd_sniff(verbose, decode),
    }
}

// ---------------------------------------------------------------------------
// read — default command (no subcommand)
// ---------------------------------------------------------------------------

fn cmd_read() {
    let devices = open_devices();
    match probe_tuning_collection(&devices) {
        Some((idx, buf)) => {
            let col = collection_label(&devices[idx].device_path);
            println!(
                "Using {} for tuning commands  ({}, PID 0x{:04X})",
                col, devices[idx].product_name, devices[idx].product_id
            );
            print_profile(&parse_tuning_report(&buf));
        }
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            eprintln!("Tried all {} collection(s).", devices.len());
            std::process::exit(1);
        }
    }
}

// ---------------------------------------------------------------------------
// apply — parse .pws and write all params to the base
// ---------------------------------------------------------------------------

fn cmd_apply(pws_path: &std::path::Path) {
    let prof = match profile::parse_pws(pws_path) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    println!(
        "Applying profile: {} / {}  ({})",
        prof.game,
        prof.car,
        pws_path.display()
    );

    let devices = open_devices();
    let (idx, before_buf) = match probe_tuning_collection(&devices) {
        Some(r) => r,
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            std::process::exit(1);
        }
    };
    let dev = &devices[idx];
    let col = collection_label(&dev.device_path);
    println!(
        "Using {}  ({}, PID 0x{:04X})\n",
        col, dev.product_name, dev.product_id
    );

    let before = parse_tuning_report(&before_buf);
    let col01 = find_col01(&devices);

    match apply_write(dev, col01, &prof) {
        Some(after_buf) => print_diff(&before, &parse_tuning_report(&after_buf)),
        None => println!("applied (unverified — readback failed)"),
    }
}

// ---------------------------------------------------------------------------
// monitor — poll game shared memory and auto-apply profiles
// ---------------------------------------------------------------------------

fn cmd_monitor(cfg: &config::Config) {
    let profiles_dir = cfg.profiles.path.as_deref().unwrap_or("profiles");
    let profiles = profile::scan_profiles(std::path::Path::new(profiles_dir));
    monitor::run_monitor(cfg, &profiles);
}

// ---------------------------------------------------------------------------
// list / scan
// ---------------------------------------------------------------------------

fn cmd_list(cfg: &config::Config, verbose: bool) {
    let profiles_dir = cfg.profiles.path.as_deref().unwrap_or("profiles");
    let base_dir = std::path::Path::new(profiles_dir);

    if !base_dir.exists() {
        eprintln!(
            "error: profiles directory not found: {}",
            base_dir.display()
        );
        eprintln!("Set [profiles] path in fanatec-tuner.toml.");
        std::process::exit(1);
    }

    let profiles = profile::scan_profiles(base_dir);
    if profiles.is_empty() {
        println!("No .pws profiles found in {}", base_dir.display());
        return;
    }

    // Group by game for display
    let mut by_game: std::collections::BTreeMap<&str, Vec<&profile::PwsProfile>> =
        std::collections::BTreeMap::new();
    for p in &profiles {
        by_game.entry(p.game.as_str()).or_default().push(p);
    }

    println!("{} profile(s) in {}\n", profiles.len(), base_dir.display());
    for (game, cars) in &by_game {
        println!("{}  ({} profiles)", game, cars.len());
        let mut sorted = cars.to_vec();
        sorted.sort_by(|a, b| a.car.cmp(&b.car));
        for p in &sorted {
            if verbose {
                println!("  {:<40}  {}", p.car, p.path.display());
            } else {
                println!("  {}", p.car);
            }
        }
        println!();
    }
}

// ---------------------------------------------------------------------------
// shared helpers
// ---------------------------------------------------------------------------

pub(crate) fn open_devices() -> Vec<hid::FanatecDevice> {
    let devices = match hid::enumerate_fanatec() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: failed to enumerate HID devices: {}", e);
            std::process::exit(1);
        }
    };

    if devices.is_empty() {
        eprintln!("No Fanatec devices found (VID 0x0EB7).");
        eprintln!("Check that the device is connected and powered on.");
        std::process::exit(1);
    }

    println!("Fanatec devices found:");
    for (i, dev) in devices.iter().enumerate() {
        println!(
            "  [{}] {} (PID 0x{:04X})  {}",
            i, dev.product_name, dev.product_id, dev.device_path
        );
    }
    println!();
    devices
}

/// Working collection path cached within the process lifetime.
static TUNING_COLLECTION: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// Tries each collection in turn. Returns device index + raw 64-byte tuning
/// report from the first collection that responds to a READ request.
/// Probes collections without toggling — device should naturally stay in 0x01 mode.
pub(crate) fn probe_tuning_collection(
    devices: &[hid::FanatecDevice],
) -> Option<(usize, [u8; REPORT_SIZE])> {
    let request = build_request_report();

    // If we already found the working collection this process, put it first.
    let cached = TUNING_COLLECTION.get().cloned();

    let sorted: Vec<(usize, &hid::FanatecDevice)> = {
        let mut v: Vec<(usize, &hid::FanatecDevice)> = devices.iter().enumerate().collect();
        if let Some(ref path) = cached {
            v.sort_by_key(|(_, d)| if &d.device_path == path { 0 } else { 1 });
        }
        v
    };

    for (idx, dev) in sorted {
        let col = collection_label(&dev.device_path);
        print!("  Probing {} ... ", col);

        drain_stale_reports(dev);

        if hid::write_report(dev, &request).is_err() {
            println!("skip (write rejected)");
            continue;
        }

        if let Some(buf) = read_tuning_report(dev) {
            println!("OK");
            let _ = TUNING_COLLECTION.set(dev.device_path.clone());
            return Some((idx, buf));
        }

        // Retry once after a short settle.
        print!("(retry) ");
        std::thread::sleep(std::time::Duration::from_millis(500));
        if hid::write_report(dev, &request).is_ok() {
            if let Some(buf) = read_tuning_report(dev) {
                println!("OK");
                let _ = TUNING_COLLECTION.set(dev.device_path.clone());
                return Some((idx, buf));
            }
        }

        println!("no tuning response");
    }
    None
}

/// Sends a request and polls for a tuning report; used for readback after apply.
fn read_tuning_report(dev: &hid::FanatecDevice) -> Option<[u8; REPORT_SIZE]> {
    let mut buf = [0u8; REPORT_SIZE];
    for _ in 0..10 {
        match hid::read_report(dev, &mut buf, 200) {
            Ok(()) if is_tuning_report(&buf) => return Some(buf),
            Ok(()) => {}
            Err(_) => break,
        }
    }
    None
}

/// Find the col01 HID collection (display / ACK endpoint) among opened devices.
pub(crate) fn find_col01(devices: &[hid::FanatecDevice]) -> Option<&hid::FanatecDevice> {
    devices
        .iter()
        .find(|d| d.device_path.to_lowercase().contains("col01"))
}

// ACK burst packets sent on col01 after every tuning write (FanaBridge protocol).
const ACK_ON: [u8; 8] = [0x01, 0xF8, 0x09, 0x01, 0x06, 0xFF, 0x02, 0x00];
const ACK_OFF: [u8; 8] = [0x01, 0xF8, 0x09, 0x01, 0x06, 0x00, 0x00, 0x00];

/// Send 4 ON/OFF pulses on col01 after a tuning write (matches FanaBridge behaviour).
fn send_ack_burst(col01: &hid::FanatecDevice) {
    for _ in 0..4 {
        let _ = hid::write_raw(col01, &ACK_ON);
        std::thread::sleep(std::time::Duration::from_millis(1));
        let _ = hid::write_raw(col01, &ACK_OFF);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Drain any stale input reports buffered before a fresh readback request.
fn drain_stale_reports(dev: &hid::FanatecDevice) {
    let mut buf = [0u8; REPORT_SIZE];
    while hid::read_report(dev, &mut buf, 50).is_ok() {}
}

/// Writes a profile to the device via raw HID and reads back for verification.
///
/// If attempt 0 sees devId=0x81 (write-rejected mode), retries once:
/// open a fresh handle, sleep 200ms, drain, re-read, write (no toggle).
pub(crate) fn apply_write(
    col03: &hid::FanatecDevice,
    col01: Option<&hid::FanatecDevice>,
    prof: &profile::PwsProfile,
) -> Option<[u8; REPORT_SIZE]> {
    let request = build_request_report();
    let params = prof.to_params();

    println!(
        "  [apply] profile: SEN=0x{:02X} FF={} NDP={} NFR={} FEI={}",
        prof.sen, prof.ff, prof.ndp, prof.nfr, prof.fei
    );

    // ── attempt 0 ────────────────────────────────────────────────────────────
    drain_stale_reports(col03);
    let _ = hid::write_report(col03, &request);
    let base0 = read_tuning_report(col03)?;
    let dev_id0 = base0[2];
    let write_buf0 = build_fb_write_report(&base0, &params);
    if hid::write_report(col03, &write_buf0).is_err() {
        eprintln!("error: HID write failed");
        return None;
    }
    if let Some(c01) = col01 {
        send_ack_burst(c01);
    }
    drain_stale_reports(col03);
    let _ = hid::write_report(col03, &request);
    if let Some(after0) = read_tuning_report(col03) {
        let matched0 = params.iter().filter(|&&(a, v)| after0[a] == v).count();
        println!(
            "  [apply] attempt 0: devId=0x{:02X} match={}/{}",
            dev_id0,
            matched0,
            params.len()
        );
        if let Some(&(addr, val)) = params.iter().find(|&&(a, v)| after0[a] != v) {
            println!(
                "  [apply] first mismatch: addr=0x{:02X} expected=0x{:02X} got=0x{:02X}",
                addr, val, after0[addr]
            );
        }
        if matched0 == params.len() || dev_id0 != 0x81 {
            return Some(after0);
        }
    }

    // ── retry: fresh handle + 200ms + drain (no toggle) ─────────────────────
    let fresh = match hid::open_device_by_path(&col03.device_path) {
        Ok(d) => d,
        Err(_) => return None,
    };
    std::thread::sleep(std::time::Duration::from_millis(200));
    drain_stale_reports(&fresh);
    let _ = hid::write_report(&fresh, &request);
    if let Some(base1) = read_tuning_report(&fresh) {
        let dev_id1 = base1[2];
        let write_buf1 = build_fb_write_report(&base1, &params);
        if hid::write_report(&fresh, &write_buf1).is_ok() {
            if let Some(c01) = col01 {
                send_ack_burst(c01);
            }
            drain_stale_reports(&fresh);
            let _ = hid::write_report(&fresh, &request);
            if let Some(after1) = read_tuning_report(&fresh) {
                let matched1 = params.iter().filter(|&&(a, v)| after1[a] == v).count();
                println!(
                    "  [apply] retry (reopen): devId=0x{:02X} match={}/{}",
                    dev_id1,
                    matched1,
                    params.len()
                );
                return Some(after1);
            }
        }
    }

    None
}

/// Extracts the "col01" / "col02" label from a HID device path.
/// Paths look like: \\?\hid#vid_0eb7&pid_0020&col02#6&...
pub(crate) fn collection_label(path: &str) -> String {
    let lower = path.to_lowercase();
    if let Some(amp) = lower.find("&col") {
        let start = amp + 1; // skip '&', point at 'c'
        let end = lower[start..]
            .find('#')
            .map(|i| start + i)
            .unwrap_or(start + 5);
        return path[start..end.min(start + 5)].to_string();
    }
    "collection".to_string()
}

// ---------------------------------------------------------------------------
// sdk-test — verify Fanatec SDK DLL: read, set test param, verify, restore
// ---------------------------------------------------------------------------

fn cmd_sdk_test() {
    let lib = match sdk::load_sdk() {
        Some(l) => l,
        None => {
            eprintln!("error: Fanatec SDK DLL not found");
            eprintln!(
                "  Copy EndorFanatecSdk64_VS2019.dll next to fanatec-tuner.exe, or install\n  \
                 the Fanatec wheel software so the DLL is at the default path."
            );
            std::process::exit(1);
        }
    };

    let devices = open_devices();
    let (idx, _) = match probe_tuning_collection(&devices) {
        Some(r) => r,
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            std::process::exit(1);
        }
    };
    let pid = devices[idx].product_id;
    println!("Connecting SDK to PID 0x{:04X}...", pid);

    let dev = match lib.connect(pid) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Initial read.
    let initial = match dev.read_tuning() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("error: FSTmDataReportRead failed: {}", e);
            std::process::exit(1);
        }
    };
    println!("read  [0..24]: {}", hex_str(&initial[..24]));
    println!("\nInitial state:");
    print_profile(&parse_tuning_report(&initial));

    // Test: nudge FF by ±10.
    let original_ff = initial[tuning::ADDR_FF] as i32;
    let test_ff = if original_ff >= 10 {
        original_ff - 10
    } else {
        original_ff + 10
    };
    println!(
        "\nSetting FF {} → {} via FSTmDataSet...",
        original_ff, test_ff
    );

    if let Err(e) = dev.set_param(sdk::SDK_PARAM_FF, test_ff) {
        eprintln!("  FSTmDataSet failed: {}", e);
    } else if let Err(e) = dev.save() {
        eprintln!("  FSTmDataSave failed: {}", e);
    } else {
        std::thread::sleep(std::time::Duration::from_millis(200));
        match dev.read_tuning() {
            Err(e) => eprintln!("  readback failed: {}", e),
            Ok(after) => {
                println!("after [0..24]: {}", hex_str(&after[..24]));
                let got = after[tuning::ADDR_FF] as i32;
                if got == test_ff {
                    println!("  FF = {} ✓", got);
                } else {
                    println!(
                        "  FF = {} (expected {}  — param ID may be wrong)",
                        got, test_ff
                    );
                }
                print_diff(&parse_tuning_report(&initial), &parse_tuning_report(&after));
            }
        }
    }

    // Restore.
    println!("\nRestoring FF to {}...", original_ff);
    let _ = dev.set_param(sdk::SDK_PARAM_FF, original_ff);
    let _ = dev.save();
    std::thread::sleep(std::time::Duration::from_millis(200));
    match dev.read_tuning() {
        Ok(final_buf) => println!(
            "Final FF: {}  ({})",
            final_buf[tuning::ADDR_FF],
            if final_buf[tuning::ADDR_FF] as i32 == original_ff {
                "restored"
            } else {
                "MISMATCH"
            }
        ),
        Err(e) => eprintln!("  final readback failed: {}", e),
    }
}

// ---------------------------------------------------------------------------
// sniff — passive read-only HID traffic capture
// ---------------------------------------------------------------------------

fn cmd_sniff(verbose: bool, decode: bool) {
    // Enumerate to get device paths, then immediately drop the write handles.
    let devices = match hid::enumerate_fanatec() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };
    if devices.is_empty() {
        eprintln!("No Fanatec devices found (VID 0x0EB7). Connect the wheel base first.");
        std::process::exit(1);
    }

    let col03_path = devices
        .iter()
        .find(|d| collection_label(&d.device_path) == "col03")
        .map(|d| d.device_path.clone());
    let col01_path = devices
        .iter()
        .find(|d| collection_label(&d.device_path) == "col01")
        .map(|d| d.device_path.clone());
    drop(devices); // release write handles before reopening read-only

    let col03_path = match col03_path {
        Some(p) => p,
        None => {
            eprintln!("error: col03 not found.");
            std::process::exit(1);
        }
    };

    let start = std::time::Instant::now();
    let lock = std::sync::Arc::new(std::sync::Mutex::new(()));
    let mut threads = Vec::new();

    match hid::open_device_readonly(&col03_path) {
        Ok(dev) => {
            println!(
                "Sniffing col03 (read-only){}...",
                if decode { " [decode]" } else { "" }
            );
            let lk = lock.clone();
            threads.push(std::thread::spawn(move || {
                sniff_loop(dev, "col03", verbose, decode, start, lk);
            }));
        }
        Err(e) => {
            eprintln!(
                "error: cannot open col03 read-only: {}\n\
                 Tip: start fanatec-tuner sniff BEFORE opening the Fanatec App.",
                e
            );
            std::process::exit(1);
        }
    }

    if let Some(path) = col01_path {
        match hid::open_device_readonly(&path) {
            Ok(dev) => {
                println!(
                    "Sniffing col01 (read-only){}...",
                    if decode { " [decode]" } else { "" }
                );
                let lk = lock.clone();
                threads.push(std::thread::spawn(move || {
                    sniff_loop(dev, "col01", verbose, decode, start, lk);
                }));
            }
            Err(e) => eprintln!("warning: cannot open col01 read-only: {}", e),
        }
    }

    println!("Press Ctrl+C to stop.\n");
    for t in threads {
        let _ = t.join();
    }
}

fn sniff_loop(
    dev: hid::FanatecDevice,
    label: &'static str,
    verbose: bool,
    decode: bool,
    start: std::time::Instant,
    lock: std::sync::Arc<std::sync::Mutex<()>>,
) {
    use std::collections::HashMap;

    let mut buf = [0u8; REPORT_SIZE];
    let mut prev_buf = [0u8; REPORT_SIZE];
    // Count reports by type byte (buf[1]) for rate statistics.
    let mut counts: HashMap<u8, u32> = HashMap::new();
    let mut last_stats = std::time::Instant::now();
    const STATS_SECS: u64 = 10;

    loop {
        match hid::read_report(&dev, &mut buf, 100) {
            Ok(()) => {
                let elapsed = start.elapsed();
                let changed = buf != prev_buf;
                *counts.entry(buf[1]).or_insert(0) += 1;

                let line = if decode {
                    sniff_format_decoded(label, &buf, &prev_buf, elapsed)
                } else {
                    let mins = elapsed.as_secs() / 60;
                    let secs = elapsed.as_secs() % 60;
                    let ms = elapsed.subsec_millis();
                    let n = if verbose { REPORT_SIZE } else { 32 };
                    let hex: String = buf[..n]
                        .iter()
                        .map(|b| format!("{:02X}", b))
                        .collect::<Vec<_>>()
                        .join(" ");
                    let marker = if changed { "*" } else { " " };
                    format!(
                        "[{:02}:{:02}.{:03}]{} IN  {}: {}",
                        mins, secs, ms, marker, label, hex
                    )
                };

                {
                    let _guard = lock.lock().unwrap();
                    println!("{}", line);
                }

                prev_buf = buf;

                // Rate stats every STATS_SECS seconds.
                if last_stats.elapsed().as_secs() >= STATS_SECS {
                    sniff_print_stats(&lock, label, &counts, last_stats.elapsed().as_secs_f64());
                    counts.clear();
                    last_stats = std::time::Instant::now();
                }
            }
            Err(hid::HidError::Timeout) => {
                // Still check the stats timer on quiet periods.
                if last_stats.elapsed().as_secs() >= STATS_SECS && !counts.is_empty() {
                    sniff_print_stats(&lock, label, &counts, last_stats.elapsed().as_secs_f64());
                    counts.clear();
                    last_stats = std::time::Instant::now();
                }
            }
            Err(_) => break,
        }
    }
}

fn sniff_print_stats(
    lock: &std::sync::Arc<std::sync::Mutex<()>>,
    label: &str,
    counts: &std::collections::HashMap<u8, u32>,
    elapsed_secs: f64,
) {
    let mut parts: Vec<String> = counts
        .iter()
        .map(|(&k, &v)| format!("FF{:02X}={:.1}/s", k, v as f64 / elapsed_secs))
        .collect();
    parts.sort();
    let _guard = lock.lock().unwrap();
    println!("[rate] {}: {}", label, parts.join("  "));
}

// ---------------------------------------------------------------------------
// sniff decode helpers
// ---------------------------------------------------------------------------

fn sniff_format_decoded(
    label: &str,
    buf: &[u8; REPORT_SIZE],
    prev: &[u8; REPORT_SIZE],
    elapsed: std::time::Duration,
) -> String {
    let mins = elapsed.as_secs() / 60;
    let secs = elapsed.as_secs() % 60;
    let ms = elapsed.subsec_millis();
    let ts = format!("[{:02}:{:02}.{:03}]", mins, secs, ms);
    let marker = if buf != prev { "*" } else { " " };

    if label == "col01" {
        return sniff_decode_col01(&ts, marker, buf, prev);
    }

    match (buf[0], buf[1]) {
        (0xFF, 0x03) => sniff_decode_ff03(&ts, marker, buf, prev),
        (0xFF, 0x01) => sniff_decode_ff01(&ts, marker, buf),
        (0xFF, 0x08) => sniff_decode_ff08(&ts, marker, buf),
        (0xFF, 0x10) => sniff_decode_ff10(&ts, marker, buf, prev),
        _ => {
            // Never silently drop — show the actual type byte and full payload.
            let type_byte = buf[1];
            let hex: String = buf[..REPORT_SIZE]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{}{} col03 FF{:02X}-UNK: {}", ts, marker, type_byte, hex)
        }
    }
}

/// Decode a Fanatec tuning response report (FF 03, buf[2] & 0x7F == 0x01).
fn sniff_decode_ff03(
    ts: &str,
    marker: &str,
    buf: &[u8; REPORT_SIZE],
    prev: &[u8; REPORT_SIZE],
) -> String {
    if !is_tuning_report(buf) {
        // Write command or unknown FF03 sub-type — just show the header bytes.
        let hex: String = buf[..16]
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect::<Vec<_>>()
            .join(" ");
        return format!("{}{} col03 FF03-cmd: {}", ts, marker, hex);
    }

    let tp = parse_tuning_report(buf);
    let sen_str = match tp.sen_degrees {
        Some(d) => format!("{}deg", d),
        None => "AUTO".to_string(),
    };

    // Append '*' to a value string when the underlying wire byte changed.
    let chg = |addr: usize| if buf[addr] != prev[addr] { "*" } else { "" };

    format!(
        "{}{} col03 TUNING  slot={} SEN={}{} FF={}{} NDP={}{} NFR={}{} INT={}{} NIN={}{} FUL={}{}  \
         SHO={}{} BLI={}{} FFS={}{} FOR={}{} SPR={}{} DPR={}{} BRF={}{} FEI={}{}",
        ts,
        marker,
        tp.slot,
        sen_str,
        chg(tuning::ADDR_SEN),
        tp.ff,
        chg(tuning::ADDR_FF),
        tp.ndp,
        chg(ADDR_NDP),
        tp.nfr,
        chg(tuning::ADDR_NFR),
        tp.int_,
        chg(tuning::ADDR_INT),
        tp.nin,
        chg(tuning::ADDR_NIN),
        tp.ful,
        chg(tuning::ADDR_FUL),
        tp.sho,
        chg(tuning::ADDR_SHO),
        tp.bli,
        chg(tuning::ADDR_BLI),
        tp.ffs,
        chg(tuning::ADDR_FFS),
        tp.for_,
        chg(tuning::ADDR_FOR),
        tp.spr,
        chg(tuning::ADDR_SPR),
        tp.dpr,
        chg(tuning::ADDR_DPR),
        tp.brf,
        chg(tuning::ADDR_BRF),
        tp.fei,
        chg(tuning::ADDR_FEI),
    )
}

/// Decode an LED report (FF 01): subcmd 0x00=rev, 0x01=flag, 0x02=btn-color, 0x03=btn-intensity.
fn sniff_decode_ff01(ts: &str, marker: &str, buf: &[u8; REPORT_SIZE]) -> String {
    match buf[2] {
        0x00 => {
            // Rev LEDs: byte 3 = count, then pairs of u16 RGB565 big-endian.
            let count = (buf[3] as usize).min((REPORT_SIZE - 4) / 2);
            let colors: Vec<String> = (0..count)
                .map(|i| {
                    let hi = buf[4 + i * 2] as u16;
                    let lo = buf[5 + i * 2] as u16;
                    format!("{:04X}", (hi << 8) | lo)
                })
                .collect();
            format!(
                "{}{} col03 LED-REV  {} colors: {}",
                ts,
                marker,
                count,
                colors.join(" ")
            )
        }
        0x01 => {
            let hex: String = buf[3..11]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{}{} col03 LED-FLAG  {}", ts, marker, hex)
        }
        0x02 => {
            let commit = buf[3];
            format!("{}{} col03 LED-BTN-COLOR   commit={}", ts, marker, commit)
        }
        0x03 => {
            let commit = buf[3];
            format!("{}{} col03 LED-BTN-INTENS  commit={}", ts, marker, commit)
        }
        _ => {
            let hex: String = buf[..12]
                .iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{}{} col03 FF01-??: {}", ts, marker, hex)
        }
    }
}

/// Decode an FF 08 hardware-status report — show full raw hex.
fn sniff_decode_ff08(ts: &str, marker: &str, buf: &[u8; REPORT_SIZE]) -> String {
    let hex: String = buf[..REPORT_SIZE]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    format!("{}{} col03 HW-STATUS: {}", ts, marker, hex)
}

/// Attempt protobuf field extraction from an FF 10 input report.
/// Payload starts at buf[2]; leading zero padding is skipped.
fn sniff_decode_ff10(
    ts: &str,
    marker: &str,
    buf: &[u8; REPORT_SIZE],
    prev: &[u8; REPORT_SIZE],
) -> String {
    // Count non-zero payload bytes to report as the protobuf length.
    let payload = &buf[2..];
    let pb_len = payload.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);

    let hex: String = buf[..REPORT_SIZE]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");

    let fields = sniff_parse_protobuf(payload);

    if fields.is_empty() {
        return format!(
            "{}{} col03 PROTO len={}: {} [no pb fields]",
            ts, marker, pb_len, hex
        );
    }

    // Build a map of previous field values for change annotation.
    let prev_fields: std::collections::HashMap<u32, u64> = if prev[0] == 0xFF && prev[1] == 0x10 {
        sniff_parse_protobuf(&prev[2..]).into_iter().collect()
    } else {
        std::collections::HashMap::new()
    };

    let parts: Vec<String> = fields
        .iter()
        .map(|&(fnum, val)| {
            let changed = prev_fields.get(&fnum).is_none_or(|&pv| pv != val);
            if changed {
                format!("f{}={}*", fnum, val)
            } else {
                format!("f{}={}", fnum, val)
            }
        })
        .collect();

    // buf[2] == 0x16 is the sub-type that carries live tuning feedback (e.g. FF=27).
    let subtype_tag = if buf[2] == 0x16 { " [TUNING]" } else { "" };
    format!(
        "{}{} col03 PROTO{} len={}: {}  [{}]",
        ts,
        marker,
        subtype_tag,
        pb_len,
        hex,
        parts.join("  ")
    )
}

/// Decode a col01 report: game state flags, steering axis, and device status.
fn sniff_decode_col01(
    ts: &str,
    marker: &str,
    buf: &[u8; REPORT_SIZE],
    prev: &[u8; REPORT_SIZE],
) -> String {
    let game_flags = buf[3];
    let steering = i16::from_le_bytes([buf[16], buf[17]]);
    let device_status = u32::from_le_bytes([buf[30], buf[31], buf[32], buf[33]]);

    let flags_chg = if game_flags != prev[3] { "*" } else { "" };
    let steer_prev = i16::from_le_bytes([prev[16], prev[17]]);
    let steer_chg = if steering != steer_prev { "*" } else { "" };
    let status_prev = u32::from_le_bytes([prev[30], prev[31], prev[32], prev[33]]);
    let status_chg = if device_status != status_prev {
        "*"
    } else {
        ""
    };

    // Best-effort mapping of observed flag values to human-readable state names.
    let state_name = match game_flags {
        0x00 => "idle",
        0x01 => "game",
        0x02 => "in-session",
        0x03 => "on-track",
        _ => "?",
    };

    format!(
        "{}{} col01  state={}/{:02X}{}  steer={}{}  status={:08X}{}",
        ts,
        marker,
        state_name,
        game_flags,
        flags_chg,
        steering,
        steer_chg,
        device_status,
        status_chg,
    )
}

/// Read a protobuf varint from `data` at `*pos`, advancing `*pos`. Returns None on truncation.
fn sniff_read_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0u32;
    loop {
        if *pos >= data.len() {
            return None;
        }
        let b = data[*pos];
        *pos += 1;
        result |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
}

/// Parse protobuf fields from a byte slice.
/// Returns `(field_number, value)` pairs for varint (wire type 0) fields only.
/// Non-varint fields are consumed and skipped; leading zero padding is ignored.
fn sniff_parse_protobuf(data: &[u8]) -> Vec<(u32, u64)> {
    let mut fields = Vec::new();
    let mut pos = 0;
    // Skip leading zero bytes (HID report padding).
    while pos < data.len() && data[pos] == 0 {
        pos += 1;
    }
    while pos < data.len() {
        let tag = match sniff_read_varint(data, &mut pos) {
            Some(t) if t != 0 => t,
            _ => break,
        };
        let field_number = (tag >> 3) as u32;
        match tag & 0x7 {
            0 => {
                // varint
                match sniff_read_varint(data, &mut pos) {
                    Some(val) => fields.push((field_number, val)),
                    None => break,
                }
            }
            1 => {
                // 64-bit fixed
                if pos + 8 > data.len() {
                    break;
                }
                pos += 8;
            }
            2 => {
                // length-delimited
                match sniff_read_varint(data, &mut pos) {
                    Some(len) => {
                        let skip = len as usize;
                        if pos + skip > data.len() {
                            break;
                        }
                        pos += skip;
                    }
                    None => break,
                }
            }
            5 => {
                // 32-bit fixed
                if pos + 4 > data.len() {
                    break;
                }
                pos += 4;
            }
            _ => break, // unknown wire type — stop parsing
        }
    }
    fields
}

// ---------------------------------------------------------------------------
// led-test — exercise Rev LEDs, display, and button LEDs
// ---------------------------------------------------------------------------

fn cmd_led_test() {
    use std::time::Duration;

    let devices = open_devices();

    // col03 — LED + tuning endpoint (probe confirms it responds)
    let (idx, _) = match probe_tuning_collection(&devices) {
        Some(r) => r,
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            std::process::exit(1);
        }
    };
    let col03 = &devices[idx];

    // col01 — display endpoint
    let col01 = find_col01(&devices);
    if col01.is_none() {
        println!("warning: col01 not found — display test will be skipped");
    }

    println!("LED test — watch the wheel!\n");

    // ── a. All 9 Rev LEDs green ──────────────────────────────────────────────
    println!("[1/6] Rev LEDs: all green");
    let green = led::rgb_to_rgb565(0, 255, 0);
    let _ = hid::write_report(col03, &led::build_rev_led_report(&[green; 9]));
    std::thread::sleep(Duration::from_secs(2));

    // ── b. Rev LED sweep: green / yellow / red ───────────────────────────────
    println!("[2/6] Rev LEDs: green-yellow-red sweep");
    let yellow = led::rgb_to_rgb565(255, 255, 0);
    let red = led::rgb_to_rgb565(255, 0, 0);
    let sweep = [green, green, green, yellow, yellow, yellow, red, red, red];
    let _ = hid::write_report(col03, &led::build_rev_led_report(&sweep));
    std::thread::sleep(Duration::from_secs(2));

    // ── c. Rev LEDs off ──────────────────────────────────────────────────────
    let _ = hid::write_report(col03, &led::build_rev_led_report(&[0u16; 9]));

    // ── d. Display ───────────────────────────────────────────────────────────
    if let Some(c01) = col01 {
        println!("[3/6] Display: 123");
        let _ = hid::write_raw(
            c01,
            &display::build_display_report(
                display::digit_to_segment(1),
                display::digit_to_segment(2),
                display::digit_to_segment(3),
            ),
        );
        std::thread::sleep(Duration::from_secs(2));

        println!("[4/6] Display: gear 5");
        let _ = hid::write_raw(c01, &display::build_gear_display(5));
        std::thread::sleep(Duration::from_secs(2));

        println!("      Display: clear");
        let _ = hid::write_raw(
            c01,
            &display::build_display_report(
                display::SEG_BLANK,
                display::SEG_BLANK,
                display::SEG_BLANK,
            ),
        );
    } else {
        println!("[3/6] Display: skipped (col01 not found)");
        println!("[4/6] Display: skipped");
    }

    // ── e. Button LEDs: all white, intensity 4 ───────────────────────────────
    println!("[5/6] Button LEDs: all white (intensity 4)");
    let white = led::rgb_to_rgb565(255, 255, 255);
    let _ = hid::write_report(col03, &led::build_button_color_report(&[white; 12], false));
    let _ = hid::write_report(col03, &led::build_button_intensity_report(&[4u8; 16], true));
    std::thread::sleep(Duration::from_secs(2));

    // ── f. Button LEDs off ───────────────────────────────────────────────────
    println!("[6/6] Button LEDs: off");
    let _ = hid::write_report(col03, &led::build_button_color_report(&[0u16; 12], false));
    let _ = hid::write_report(col03, &led::build_button_intensity_report(&[0u8; 16], true));

    println!("\nLED test complete — did you see the LEDs change?");
}

// ---------------------------------------------------------------------------
// write-stress — persistent-handle multi-write test
// ---------------------------------------------------------------------------

fn cmd_write_stress(cfg: &config::Config) {
    use std::time::Duration;

    let profiles_dir = cfg.profiles.path.as_deref().unwrap_or("profiles");

    // Prefer profiles/CS DD+/iRacing/; fall back to all iRacing profiles under profiles/.
    let iracing_dir = std::path::Path::new(profiles_dir)
        .join("CS DD+")
        .join("iRacing");
    let mut all = if iracing_dir.exists() {
        profile::scan_profiles(&iracing_dir)
    } else {
        profile::scan_profiles(std::path::Path::new(profiles_dir))
            .into_iter()
            .filter(|p| p.game.to_lowercase().contains("iracing"))
            .collect()
    };
    all.sort_by(|a, b| a.car.cmp(&b.car));
    all.dedup_by(|a, b| a.path == b.path);

    if all.len() < 2 {
        eprintln!(
            "error: need at least 2 iRacing profiles — found {} in {}",
            all.len(),
            profiles_dir
        );
        std::process::exit(1);
    }

    // Three writes: A → B → A using the same open handles throughout.
    let pa = &all[0];
    let pb = &all[1];
    let sequence: [&profile::PwsProfile; 3] = [pa, pb, pa];

    let devices = open_devices();
    let col01 = find_col01(&devices);
    let (idx, initial_buf) = match probe_tuning_collection(&devices) {
        Some(r) => r,
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            std::process::exit(1);
        }
    };
    let col03 = &devices[idx];
    let col = collection_label(&col03.device_path);
    println!(
        "Using {}  ({}, PID 0x{:04X})",
        col, col03.product_name, col03.product_id
    );
    println!("\nInitial state:");
    print_profile(&parse_tuning_report(&initial_buf));

    let request = build_request_report();
    let mut success_count = 0usize;

    for (i, prof) in sequence.iter().enumerate() {
        println!(
            "\n--- Write {}/3: {} ({}) ---",
            i + 1,
            prof.car,
            prof.path.file_name().unwrap_or_default().to_string_lossy()
        );

        // Pre-read using the already-open handle.
        drain_stale_reports(col03);
        let _ = hid::write_report(col03, &request);
        let base = match read_tuning_report(col03) {
            Some(b) => b,
            None => {
                eprintln!("  pre-read failed — skipping");
                continue;
            }
        };
        println!("  read  [0..24]: {}", hex_str(&base[..24]));

        let write_buf = build_full_write_report(&base, &prof.to_params());
        println!("  write [0..24]: {}", hex_str(&write_buf[..24]));

        if hid::write_report(col03, &write_buf).is_err() {
            eprintln!("  HID write FAILED");
            continue;
        }
        if let Some(c01) = col01 {
            send_ack_burst(c01);
        }

        // Readback on the same handle.
        drain_stale_reports(col03);
        let _ = hid::write_report(col03, &request);
        match read_tuning_report(col03) {
            None => eprintln!("  readback failed"),
            Some(after_buf) => {
                println!("  after [0..24]: {}", hex_str(&after_buf[..24]));
                print_diff(
                    &parse_tuning_report(&base),
                    &parse_tuning_report(&after_buf),
                );
                let params = prof.to_params();
                let matching = params
                    .iter()
                    .filter(|&&(addr, val)| after_buf[addr] == val)
                    .count();
                if matching == params.len() {
                    println!("  RESULT: all {} params match target", params.len());
                    success_count += 1;
                } else {
                    println!(
                        "  RESULT: {}/{} params match target",
                        matching,
                        params.len()
                    );
                }
            }
        }

        if i < sequence.len() - 1 {
            std::thread::sleep(Duration::from_secs(2));
        }
    }

    println!("\n=== Write-stress summary ===");
    println!("  {}/3 writes verified successfully", success_count);
}

// ---------------------------------------------------------------------------
// write-test — systematic toggle+write protocol diagnostics
// ---------------------------------------------------------------------------

fn cmd_write_test() {
    use std::time::Duration;

    let devices = open_devices();
    let (idx, initial_buf) = match probe_tuning_collection(&devices) {
        Some(r) => r,
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            std::process::exit(1);
        }
    };
    let dev = &devices[idx];
    let col = collection_label(&dev.device_path);
    println!(
        "Using {}  ({}, PID 0x{:04X})\n",
        col, dev.product_name, dev.product_id
    );

    let original_ndp = initial_buf[ADDR_NDP];
    let test_val: u8 = if original_ndp >= 10 {
        original_ndp - 10
    } else {
        original_ndp + 10
    };
    println!(
        "NDP original = {}  test value = {}\n",
        original_ndp, test_val
    );

    // Each test returns Some(ndp_after) or None on readback failure.
    let mut results: Vec<(&str, Option<u8>)> = Vec::new();

    // Helper: build a write buffer for NDP = val, based on whatever buf holds now.
    let make_write =
        |base: &[u8; REPORT_SIZE], val: u8| build_full_write_report(base, &[(ADDR_NDP, val)]);

    // Helper: send CMD_REQUEST + poll for tuning report, return buffer.
    let do_request = |dev: &hid::FanatecDevice| -> Option<[u8; REPORT_SIZE]> {
        let req = build_request_report();
        if hid::write_report(dev, &req).is_err() {
            return None;
        }
        read_tuning_report(dev)
    };

    // Helper: read NDP from a fresh request.
    let read_ndp =
        |dev: &hid::FanatecDevice| -> Option<u8> { do_request(dev).map(|b| b[ADDR_NDP]) };

    // Helper: best-effort restore using toggle → write(original) → read.
    let restore = |dev: &hid::FanatecDevice, base: &[u8; REPORT_SIZE]| {
        let wb = make_write(base, original_ndp);
        let wake = build_wake_report();
        let _ = hid::write_report(dev, &wake);
        std::thread::sleep(Duration::from_millis(500));
        let _ = hid::write_report(dev, &wb);
        std::thread::sleep(Duration::from_millis(200));
        // Confirm restore.
        if let Some(v) = read_ndp(dev) {
            if v != original_ndp {
                eprintln!("  [restore] NDP still {} after restore attempt", v);
            }
        }
    };

    // We track the current on-device buffer so each write uses a fresh base.
    let mut current_buf = initial_buf;

    // --- Test A: write → 200ms → toggle → 200ms → request → read ----------
    {
        println!("Test A: write → 200ms → toggle → 200ms → request → read");
        let wb = make_write(&current_buf, test_val);
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let wake = build_wake_report();
        let ok_t = hid::write_report(dev, &wake).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w && ok_t { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("A: write→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test B: toggle → 500ms → write → 200ms → toggle → 200ms → request → read
    {
        println!("Test B: toggle → 500ms → write → 200ms → toggle → 200ms → request → read");
        let wb = make_write(&current_buf, test_val);
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let wake = build_wake_report();
        let _ = hid::write_report(dev, &wake);
        std::thread::sleep(Duration::from_millis(500));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let wake2 = build_wake_report();
        let ok_t2 = hid::write_report(dev, &wake2).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w && ok_t2 { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("B: toggle→write→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test C: toggle → 500ms → write → 200ms → request → read (no post-write toggle)
    {
        println!("Test C: toggle → 500ms → write → 200ms → request → read");
        let wb = make_write(&current_buf, test_val);
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let wake = build_wake_report();
        let _ = hid::write_report(dev, &wake);
        std::thread::sleep(Duration::from_millis(500));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("C: toggle→write→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test D: write with buf[3]=0x01 (slot-byte override) → toggle → read
    {
        println!("Test D: write (buf[3]=0x01 slot override) → toggle → 200ms → read");
        let mut wb = make_write(&current_buf, test_val);
        wb[3] = 0x01; // force slot byte to 1
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        let wake = build_wake_report();
        let ok_t = hid::write_report(dev, &wake).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w && ok_t { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("D: write(slot=1)→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test E: write with buf[3]=0x81 → toggle → read
    {
        println!("Test E: write (buf[3]=0x81) → toggle → 200ms → read");
        let mut wb = make_write(&current_buf, test_val);
        wb[3] = 0x81;
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        let wake = build_wake_report();
        let ok_t = hid::write_report(dev, &wake).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w && ok_t { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("E: write(buf[3]=0x81)→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test F: TUNING_MENU init sequence then write → toggle → read
    {
        println!("Test F: [FF 08 01 FF]+[FF 08 02]+[FF 03 02] → write → toggle → 200ms → read");
        let init_cmds: [[u8; REPORT_SIZE]; 3] = {
            let mut a = [[0u8; REPORT_SIZE]; 3];
            a[0][0] = 0xFF;
            a[0][1] = 0x08;
            a[0][2] = 0x01;
            a[0][3] = 0xFF;
            a[1][0] = 0xFF;
            a[1][1] = 0x08;
            a[1][2] = 0x02;
            a[2][0] = 0xFF;
            a[2][1] = 0x03;
            a[2][2] = 0x02;
            a
        };
        let mut init_ok = true;
        for cmd in &init_cmds {
            if hid::write_raw(dev, cmd).is_err() {
                init_ok = false;
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let v = if init_ok {
            std::thread::sleep(Duration::from_millis(200));
            let wb = make_write(&current_buf, test_val);
            println!("  write[0..10]: {}", hex_str(&wb[..10]));
            let ok_w = hid::write_report(dev, &wb).is_ok();
            let wake = build_wake_report();
            let ok_t = hid::write_report(dev, &wake).is_ok();
            std::thread::sleep(Duration::from_millis(200));
            if ok_w && ok_t {
                read_ndp(dev)
            } else {
                None
            }
        } else {
            None
        };
        println!("  → NDP after: {:?}", v);
        results.push(("F: init+write→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test G: select_slot(1) → 200ms → write → toggle → 200ms → read
    {
        println!("Test G: select_slot(1) → 200ms → write → toggle → 200ms → read");
        let slot_rpt = build_select_slot_report(1);
        let _ = hid::write_report(dev, &slot_rpt);
        std::thread::sleep(Duration::from_millis(200));
        let wb = make_write(&current_buf, test_val);
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        let wake = build_wake_report();
        let ok_t = hid::write_report(dev, &wake).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w && ok_t { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("G: slot(1)→write→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
    }

    // --- Test H: toggle → toggle → 500ms → write → 200ms → toggle → 200ms → read
    {
        println!("Test H: toggle → toggle → 500ms → write → 200ms → toggle → 200ms → read");
        let wake = build_wake_report();
        let _ = hid::write_report(dev, &wake);
        std::thread::sleep(Duration::from_millis(100));
        let _ = hid::write_report(dev, &wake);
        std::thread::sleep(Duration::from_millis(500));
        let wb = make_write(&current_buf, test_val);
        println!("  write[0..10]: {}", hex_str(&wb[..10]));
        let ok_w = hid::write_report(dev, &wb).is_ok();
        std::thread::sleep(Duration::from_millis(200));
        let _ = hid::write_report(dev, &wake);
        std::thread::sleep(Duration::from_millis(200));
        let v = if ok_w { read_ndp(dev) } else { None };
        println!("  → NDP after: {:?}", v);
        results.push(("H: 2×toggle→write→toggle→read", v));
        if let Some(b) = do_request(dev) {
            current_buf = b;
        }
        restore(dev, &current_buf);
        std::thread::sleep(Duration::from_millis(2000));
    }

    // --- Summary table
    println!("\n=== Write-test summary ===");
    println!(
        "  (original NDP = {}  target = {})\n",
        original_ndp, test_val
    );
    println!("  Test  Description                          NDP after  Changed?");
    println!("  ----  -----------------------------------  ---------  --------");
    for (label, v) in &results {
        let ndp_str = match v {
            Some(n) => format!("{}", n),
            None => "N/A".to_string(),
        };
        let changed = match v {
            Some(n) if *n == test_val => "YES ***",
            Some(_) => "no",
            None => "readback fail",
        };
        println!(
            "  {:4}  {:<35}  {:>9}  {}",
            &label[..1],
            &label[3..],
            ndp_str,
            changed
        );
    }
    println!();
}

// ---------------------------------------------------------------------------
// diag
// ---------------------------------------------------------------------------

fn cmd_diag() {
    let devices = open_devices();
    let req64 = build_request_report();
    // 65-byte variant: 0x00 report-ID prefix + 64-byte payload
    let mut req65 = [0u8; REPORT_SIZE + 1];
    req65[1..].copy_from_slice(&req64);

    for dev in &devices {
        let col = collection_label(&dev.device_path);
        println!(
            "--- {} ({}, PID 0x{:04X}) ---",
            col, dev.product_name, dev.product_id
        );

        match hid::get_hid_caps(dev) {
            Some(c) => println!(
                "  caps: in={}  out={}  feat={}",
                c.input_report_len, c.output_report_len, c.feature_report_len
            ),
            None => println!("  caps: unavailable"),
        }

        // [1] WriteFile 64 bytes + ReadFile 64 bytes (current production approach)
        diag_write_read(dev, &req64, "WriteFile 64 bytes [FF 03 04 ...]", 64);

        // [2] WriteFile 65 bytes (0x00 report-ID prefix) + ReadFile 64 bytes
        diag_write_read(dev, &req65, "WriteFile 65 bytes [00 FF 03 04 ...]", 64);

        // [3] WriteFile 65 bytes + ReadFile 65 bytes
        diag_write_read(dev, &req65, "WriteFile 65 bytes / ReadFile 65 bytes", 65);

        // [4] HidD_SetFeature 64 bytes (ID=0xFF) + HidD_GetFeature 64 bytes
        print!("  [4] SetFeature 64 bytes [FF 03 04 ...] ... ");
        match hid::set_feature(dev, &req64) {
            Err(e) => println!("FAILED ({})", e),
            Ok(()) => {
                println!("OK");
                let mut fbuf = [0xFFu8; REPORT_SIZE]; // byte 0 = desired report ID
                match hid::get_feature(dev, &mut fbuf) {
                    Err(e) => println!("    GetFeature FAILED ({})", e),
                    Ok(()) => println!("    GetFeature: {}", hex_str(&fbuf[..16])),
                }
            }
        }

        // [5] HidD_SetFeature 65 bytes (ID=0x00) + HidD_GetFeature 65 bytes
        print!("  [5] SetFeature 65 bytes [00 FF 03 04 ...] ... ");
        match hid::set_feature(dev, &req65) {
            Err(e) => println!("FAILED ({})", e),
            Ok(()) => {
                println!("OK");
                let mut fbuf = [0u8; REPORT_SIZE + 1];
                match hid::get_feature(dev, &mut fbuf) {
                    Err(e) => println!("    GetFeature FAILED ({})", e),
                    Ok(()) => println!("    GetFeature: {}", hex_str(&fbuf[..17])),
                }
            }
        }

        // [6] HidD_SetOutputReport (control endpoint, like Linux hid_hw_request)
        print!("  [6] SetOutputReport 64 bytes [FF 03 04 ...] ... ");
        match hid::set_output_report(dev, &req64) {
            Err(e) => println!("FAILED ({})", e),
            Ok(()) => {
                println!("write OK");
                diag_poll_read(dev, 64);
            }
        }

        // [7] Toggle advanced mode (0xFF, 0x03, 0x06) → 500ms → tuning request
        print!("  [7] AdvancedMode toggle [FF 03 06] + 500ms + request [FF 03 04] ... ");
        let mut adv = [0u8; REPORT_SIZE];
        adv[0] = 0xFF;
        adv[1] = 0x03;
        adv[2] = 0x06;
        match hid::write_raw(dev, &adv) {
            Err(e) => println!("toggle FAILED ({})", e),
            Ok(()) => {
                std::thread::sleep(std::time::Duration::from_millis(500));
                match hid::write_raw(dev, &req64) {
                    Err(e) => println!("request FAILED ({})", e),
                    Ok(()) => {
                        println!("writes OK");
                        diag_poll_read(dev, 64);
                    }
                }
            }
        }

        // [8] Read-only — no write, check for periodic tuning updates from the device
        println!("  [8] Read-only (no write), 20 × 200ms ...");
        diag_poll_read(dev, 64);

        // [9] TUNING_MENU init sequence (from hid-ftecff.c lines 1184-1195), then request.
        // Linux driver sends three 8-byte output reports on probe before any tuning request.
        // We pad each to REPORT_SIZE bytes since Windows WriteFile needs the full report.
        println!("  [9] TUNING_MENU init [FF 08 01 FF]+[FF 08 02]+[FF 03 02] → request ...");
        let init_cmds: [[u8; REPORT_SIZE]; 3] = {
            let mut a = [[0u8; REPORT_SIZE]; 3];
            // cmd A: 0xFF 0x08 0x01 0xFF 0x00 ...
            a[0][0] = 0xFF;
            a[0][1] = 0x08;
            a[0][2] = 0x01;
            a[0][3] = 0xFF;
            // cmd B: 0xFF 0x08 0x02 0x00 ...
            a[1][0] = 0xFF;
            a[1][1] = 0x08;
            a[1][2] = 0x02;
            // cmd C: 0xFF 0x03 0x02 0x00 ...
            a[2][0] = 0xFF;
            a[2][1] = 0x03;
            a[2][2] = 0x02;
            a
        };
        let mut init_ok = true;
        for (i, cmd) in init_cmds.iter().enumerate() {
            if let Err(e) = hid::write_raw(dev, cmd) {
                println!("    init cmd {} FAILED ({})", i + 1, e);
                init_ok = false;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        if init_ok {
            std::thread::sleep(std::time::Duration::from_millis(200));
            match hid::write_raw(dev, &req64) {
                Err(e) => println!("    request FAILED ({})", e),
                Ok(()) => diag_poll_read(dev, 64),
            }
        }

        println!();
    }
}

fn diag_poll_read(dev: &hid::FanatecDevice, read_len: usize) {
    let mut rbuf = vec![0u8; read_len];
    let mut got_tuning = false;
    for _ in 0..20 {
        match hid::read_raw(dev, &mut rbuf, 200) {
            Ok(()) => {
                let is_tuning = rbuf[0] == 0xFF && rbuf.get(1) == Some(&0x03);
                if is_tuning {
                    println!("      → TUNING: {}", hex_str(&rbuf[..read_len.min(24)]));
                    got_tuning = true;
                    break;
                } else {
                    println!("      other: {}", hex_str(&rbuf[..read_len.min(8)]));
                }
            }
            Err(hid::HidError::Timeout) => {}
            Err(e) => {
                println!("      read error: {}", e);
                break;
            }
        }
    }
    if !got_tuning {
        println!("      → no tuning report (20 × 200ms)");
    }
}

fn diag_write_read(dev: &hid::FanatecDevice, wbuf: &[u8], label: &str, read_len: usize) {
    let attempt = match read_len {
        64 if wbuf.len() == 64 => "[1]",
        64 if wbuf.len() == 65 => "[2]",
        _ => "[3]",
    };
    print!("  {} {} ... ", attempt, label);
    match hid::write_raw(dev, wbuf) {
        Err(e) => {
            println!("WRITE FAILED ({})", e);
            return;
        }
        Ok(()) => println!("write OK"),
    }
    let mut rbuf = vec![0u8; read_len];
    let mut got_tuning = false;
    for _ in 0..20 {
        match hid::read_raw(dev, &mut rbuf, 200) {
            Ok(()) => {
                let marker = rbuf[0] == 0xFF && rbuf.get(1) == Some(&0x03);
                if marker {
                    println!("      → TUNING: {}", hex_str(&rbuf[..read_len.min(24)]));
                    got_tuning = true;
                    break;
                } else {
                    println!("      other: {}", hex_str(&rbuf[..read_len.min(8)]));
                }
            }
            Err(hid::HidError::Timeout) => {}
            Err(e) => {
                println!("      read error: {}", e);
                break;
            }
        }
    }
    if !got_tuning {
        println!("      → no tuning report (20 × 200ms)");
    }
}

fn hex_str(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// display helpers
// ---------------------------------------------------------------------------

fn print_profile(p: &tuning::TuningProfile) {
    println!();
    println!("Slot:  {}", p.slot);

    match p.sen_degrees {
        None => println!("SEN:   AUTO      (raw 0x{:02X})", p.sen_raw),
        Some(deg) if deg > 2520 => {
            println!("SEN:   {}° (approx, raw 0x{:02X})", deg, p.sen_raw)
        }
        Some(deg) => println!("SEN:   {}°  (raw 0x{:02X})", deg, p.sen_raw),
    }

    println!("FF:    {}", p.ff);

    let ffs_label = match p.ffs {
        0 => "LINEAR",
        1 => "PEAK",
        _ => "?",
    };
    println!("FFS:   {} ({})", ffs_label, p.ffs);

    println!("NDP:   {}", p.ndp);
    println!("NFR:   {}", p.nfr);
    println!("NIN:   {}", p.nin);
    println!("INT:   {}", p.int_);
    println!("FEI:   {}", p.fei);

    println!("FOR:   {} (wire {})", p.for_, p.for_ / 10);
    println!("SPR:   {} (wire {})", p.spr, p.spr / 10);
    println!("DPR:   {} (wire {})", p.dpr, p.dpr / 10);

    let bli_label = if p.bli == 101 {
        "OFF".to_string()
    } else {
        p.bli.to_string()
    };
    println!("BLI:   {}", bli_label);

    println!("SHO:   {} (wire {})", p.sho, p.sho / 10);
    println!("BRF:   {}", p.brf);
    println!("FUL:   {}", p.ful);
    println!("ACP:   {}", p.acp);
    println!("DRI:   {}", p.dri);
}

pub(crate) fn print_diff(before: &tuning::TuningProfile, after: &tuning::TuningProfile) {
    println!("Parameter  Before → After");
    println!("-----------+--------------");

    macro_rules! row {
        ($label:expr, $b:expr, $a:expr) => {
            if $b != $a {
                println!("  {:8}  {:>5} → {}", $label, $b, $a);
            } else {
                println!("  {:8}  {:>5}   (unchanged)", $label, $b);
            }
        };
    }

    row!("SEN raw", before.sen_raw, after.sen_raw);
    row!("FF", before.ff, after.ff);
    row!("FFS", before.ffs, after.ffs);
    row!("NDP", before.ndp, after.ndp);
    row!("NFR", before.nfr, after.nfr);
    row!("NIN", before.nin, after.nin);
    row!("INT", before.int_, after.int_);
    row!("FEI", before.fei, after.fei);
    row!("FOR", before.for_, after.for_);
    row!("SPR", before.spr, after.spr);
    row!("DPR", before.dpr, after.dpr);
    row!("BLI", before.bli, after.bli);
    row!("SHO", before.sho, after.sho);
    row!("BRF", before.brf, after.brf);
    row!("FUL", before.ful, after.ful);
    row!("ACP", before.acp, after.acp);
    row!("DRI", before.dri as u8, after.dri as u8);
}
