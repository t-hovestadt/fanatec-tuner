mod config;
mod hid;
mod profile;
mod tuning;

use clap::{Parser, Subcommand};
use hid::REPORT_SIZE;
use tuning::{build_request_report, build_write_report, is_tuning_report, parse_tuning_report};

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
            eprintln!(
                "Tried all {} collection(s). Close FanaLab and retry.",
                devices.len()
            );
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
        eprintln!("If FanaLab is running with exclusive access, close it and retry.");
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

/// Tries each collection in turn. Returns device index + raw 64-byte tuning
/// report from the first collection that accepts a write and responds.
fn probe_tuning_collection(devices: &[hid::FanatecDevice]) -> Option<(usize, [u8; REPORT_SIZE])> {
    let request = build_request_report();
    for (idx, dev) in devices.iter().enumerate() {
        let col = collection_label(&dev.device_path);
        print!("  Probing {} ... ", col);

        if let Err(e) = hid::write_report(dev, &request) {
            println!("write failed ({})", e);
            continue;
        }

        let mut buf = [0u8; REPORT_SIZE];
        let mut got_tuning = false;
        for _ in 0..10 {
            match hid::read_report(dev, &mut buf, 200) {
                Ok(()) if is_tuning_report(&buf) => {
                    got_tuning = true;
                    break;
                }
                Ok(()) => {}
                Err(e) => {
                    println!("read failed ({})", e);
                    break;
                }
            }
        }

        if got_tuning {
            println!("OK");
            return Some((idx, buf));
        } else {
            println!("no tuning response");
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
