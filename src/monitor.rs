use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::config;
use crate::games::{self, CarDetected};
use crate::hid::{self, REPORT_SIZE};
use crate::profile::PwsProfile;
use crate::tuning::{
    build_full_write_report, build_request_report, is_tuning_report, parse_tuning_report,
};

pub fn run_monitor(config: &config::Config, profiles: &[PwsProfile]) -> ! {
    if profiles.is_empty() {
        eprintln!("warning: no profiles loaded — auto-apply will do nothing");
        eprintln!("  Set [profiles] path in fanatec-tuner.toml to a directory with .pws files.");
    } else {
        println!("{} profile(s) loaded", profiles.len());
    }

    // Open device handles once. probe_tuning_collection sends the wake toggle
    // (CMD_TOGGLE_ADVANCED 0x06) to bring the base into advanced mode.
    let devices = crate::open_devices();
    let mut dev_idx = match crate::probe_tuning_collection(&devices) {
        Some((idx, _)) => idx,
        None => {
            eprintln!("error: no HID collection responded to the tuning request.");
            std::process::exit(1);
        }
    };
    let col = crate::collection_label(&devices[dev_idx].device_path);
    println!(
        "Using {}  ({}, PID 0x{:04X})",
        col, devices[dev_idx].product_name, devices[dev_idx].product_id
    );
    println!("Monitoring — press Ctrl-C to stop\n");

    let interval = Duration::from_secs(config.monitor.scan_interval_secs());
    let mut last_car: Option<CarDetected> = None;

    loop {
        let current = games::detect_car();

        if current != last_car {
            match &current {
                None => {
                    if last_car.is_some() {
                        println!("[{}] Game exited — wheel unchanged", now_hms());
                    }
                }
                Some(detected) => {
                    print!("[{}] {} / {} → ", now_hms(), detected.game, detected.car);
                    match find_matching_profile(profiles, &detected.game, &detected.car) {
                        None => println!("no matching profile"),
                        Some(prof) => {
                            println!("applying {}", prof.path.display());
                            if !do_apply(&devices[dev_idx], prof) {
                                // Write failed — advanced mode may have been lost.
                                // Re-probe with the existing handles (sends toggle to recover).
                                eprintln!("  re-probing after write failure…");
                                match crate::probe_tuning_collection(&devices) {
                                    Some((idx, _)) => {
                                        dev_idx = idx;
                                        do_apply(&devices[dev_idx], prof);
                                    }
                                    None => {
                                        eprintln!("  error: device not responding after re-probe")
                                    }
                                }
                            }
                        }
                    }
                }
            }
            last_car = current;
        }

        std::thread::sleep(interval);
    }
}

/// Reads the current device state, applies the profile, then reads back and
/// prints a diff. Returns false only if the write itself failed (the caller
/// should re-probe in that case); readback failures are non-fatal.
fn do_apply(dev: &hid::FanatecDevice, prof: &PwsProfile) -> bool {
    let before_buf = match get_current_state(dev) {
        Some(b) => b,
        None => {
            eprintln!("  error: could not read device state before apply");
            return false;
        }
    };
    let before = parse_tuning_report(&before_buf);

    let params = prof.write_params();
    let write_buf = build_full_write_report(&before_buf, &params);

    if let Err(e) = hid::write_report(dev, &write_buf) {
        eprintln!("  error: write failed: {}", e);
        return false;
    }

    // Advanced mode stays on after a write — just send a bare request for readback.
    std::thread::sleep(Duration::from_millis(500));
    let request = build_request_report();
    if let Err(e) = hid::write_report(dev, &request) {
        eprintln!("  warning: could not request readback: {}", e);
        return true; // write succeeded; readback failure is not a write error
    }

    match read_tuning_report(dev) {
        Some(after_buf) => {
            let after = parse_tuning_report(&after_buf);
            crate::print_diff(&before, &after);
        }
        None => eprintln!("  warning: could not read back tuning values after apply"),
    }
    true
}

/// Returns the current device tuning state. Advanced mode is already ON from
/// the probe at startup — sends only a values request, never the toggle.
fn get_current_state(dev: &hid::FanatecDevice) -> Option<[u8; REPORT_SIZE]> {
    let request = build_request_report();
    hid::write_report(dev, &request).ok()?;
    read_tuning_report(dev)
}

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

fn find_matching_profile<'a>(
    profiles: &'a [PwsProfile],
    game: &str,
    car: &str,
) -> Option<&'a PwsProfile> {
    let ng = normalize(game);
    let nc = normalize(car);

    // 1. Exact game + exact car
    if let Some(p) = profiles
        .iter()
        .find(|p| normalize(&p.game) == ng && normalize(&p.car) == nc)
    {
        return Some(p);
    }

    // 2. Game prefix + exact car
    if let Some(p) = profiles
        .iter()
        .find(|p| normalize(&p.game).starts_with(&ng) && normalize(&p.car) == nc)
    {
        return Some(p);
    }

    // 3. Game prefix + profile car contains detected car
    if let Some(p) = profiles
        .iter()
        .find(|p| normalize(&p.game).starts_with(&ng) && normalize(&p.car).contains(&nc))
    {
        return Some(p);
    }

    // 4. Game prefix + detected car contains profile car (reverse)
    profiles
        .iter()
        .find(|p| normalize(&p.game).starts_with(&ng) && nc.contains(&normalize(&p.car)))
}

fn normalize(s: &str) -> String {
    s.to_lowercase()
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn now_hms() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
