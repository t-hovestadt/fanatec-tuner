mod config;
mod hid;
mod profile;
mod tuning;

use clap::{Parser, Subcommand};
use hid::REPORT_SIZE;
use tuning::{
    build_request_report, build_wake_report, build_write_report, is_tuning_report,
    parse_tuning_report,
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

    // Write order: everything except FF first, then FF last.
    // This avoids a sudden force spike if FF is currently 0.
    let writes: &[(usize, u8)] = &[
        (tuning::ADDR_SEN, prof.sen),
        (tuning::ADDR_FFS, prof.ffs),
        (tuning::ADDR_NDP, prof.ndp),
        (tuning::ADDR_NFR, prof.nfr),
        (tuning::ADDR_NIN, prof.nin),
        (tuning::ADDR_INT, prof.int_),
        (tuning::ADDR_FEI, prof.fei),
        (tuning::ADDR_FOR, profile::PwsProfile::wire_div10(prof.for_)),
        (tuning::ADDR_SPR, profile::PwsProfile::wire_div10(prof.spr)),
        (tuning::ADDR_DPR, profile::PwsProfile::wire_div10(prof.dpr)),
        (tuning::ADDR_BLI, prof.bli),
        (tuning::ADDR_SHO, profile::PwsProfile::wire_div10(prof.sho)),
        (tuning::ADDR_BRF, profile::PwsProfile::wire_div10(prof.brf)),
        (tuning::ADDR_FUL, prof.ful),
        (tuning::ADDR_DRI, prof.dri),
        (tuning::ADDR_ACP, prof.acp),
        (tuning::ADDR_FF, prof.ff), // FF last
    ];

    for &(addr, wire_val) in writes {
        let report = build_write_report(addr, wire_val);
        if let Err(e) = hid::write_report(dev, &report) {
            eprintln!("error: write to offset 0x{:02X} failed: {}", addr, e);
            std::process::exit(1);
        }
    }

    // Re-read to get the after snapshot
    let request = build_request_report();
    if let Err(e) = hid::write_report(dev, &request) {
        eprintln!("error: could not request readback: {}", e);
        std::process::exit(1);
    }
    let after_buf = match read_tuning_report(dev) {
        Some(b) => b,
        None => {
            eprintln!("warning: could not read back tuning values after apply");
            return;
        }
    };
    let after = parse_tuning_report(&after_buf);

    print_diff(&before, &after);
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

fn open_devices() -> Vec<hid::FanatecDevice> {
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
/// report from the first collection that accepts writes and responds.
///
/// Collections that reject the first write (e.g. col01 → ERROR_INVALID_FUNCTION)
/// are skipped immediately — no point waking a non-tuning endpoint.
///
/// The DD+ advanced-mode toggle (CMD 0x06) is a flip, not an enable, so the
/// device may already be in advanced mode when we arrive. Three attempts:
///   Attempt 1: wake → 500 ms → request+poll
///   Attempt 2: wake → 500 ms → request+poll   (corrects if attempt 1 toggled it off)
///   Attempt 3: 1000 ms extra → wake → 500 ms → request+poll  (longer settle)
fn probe_tuning_collection(devices: &[hid::FanatecDevice]) -> Option<(usize, [u8; REPORT_SIZE])> {
    let wake = build_wake_report();
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

        // Bail immediately if the collection refuses writes.
        if hid::write_report(dev, &wake).is_err() {
            println!("skip (write rejected)");
            continue;
        }

        // Attempt 1: wake already sent above; wait + request + poll.
        if let Some(buf) = request_and_poll(dev, &request) {
            println!("OK");
            let _ = TUNING_COLLECTION.set(dev.device_path.clone());
            return Some((idx, buf));
        }

        // Attempt 2: toggle again (corrects if attempt 1 landed us in wrong state).
        print!("(retry 2) ");
        if hid::write_report(dev, &wake).is_ok() {
            if let Some(buf) = request_and_poll(dev, &request) {
                println!("OK");
                let _ = TUNING_COLLECTION.set(dev.device_path.clone());
                return Some((idx, buf));
            }
        }

        // Attempt 3: give the base 1 s extra to settle then try once more.
        print!("(retry 3) ");
        std::thread::sleep(std::time::Duration::from_millis(1000));
        if hid::write_report(dev, &wake).is_ok() {
            if let Some(buf) = request_and_poll(dev, &request) {
                println!("OK");
                let _ = TUNING_COLLECTION.set(dev.device_path.clone());
                return Some((idx, buf));
            }
        }

        println!("no tuning response");
    }
    None
}

/// Sends a request report, then polls for a tuning reply (up to 10 × 200 ms).
fn request_and_poll(
    dev: &hid::FanatecDevice,
    request: &[u8; REPORT_SIZE],
) -> Option<[u8; REPORT_SIZE]> {
    std::thread::sleep(std::time::Duration::from_millis(500));
    if hid::write_report(dev, request).is_err() {
        return None;
    }
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

/// Extracts the "col01" / "col02" label from a HID device path.
/// Paths look like: \\?\hid#vid_0eb7&pid_0020&col02#6&...
fn collection_label(path: &str) -> String {
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

fn print_diff(before: &tuning::TuningProfile, after: &tuning::TuningProfile) {
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
