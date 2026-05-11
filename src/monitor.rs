use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::carlist;
use crate::config;
use crate::games::{self, CarDetected};
use crate::hid;
use crate::profile::PwsProfile;

/// How long to wait for iRacing's process to reappear after it disappears
/// during a session transition before declaring the game exited.
const GAME_GONE_GRACE_SECS: u64 = 30;

pub fn run_monitor(config: &config::Config, profiles: &[PwsProfile]) -> ! {
    if profiles.is_empty() {
        eprintln!("warning: no profiles loaded — auto-apply will do nothing");
        eprintln!("  Set [profiles] path in fanatec-tuner.toml to a directory with .pws files.");
    } else {
        println!("{} profile(s) loaded", profiles.len());
    }

    // Build XML candidate directories in priority order:
    //   1. profiles/xml/  (local override, checked first)
    //   2. Fanatec App default install path (Windows)
    //   3. Custom path from config [xml] path (if set)
    let profiles_dir = config.profiles.path.as_deref().unwrap_or("profiles");
    let mut xml_candidates: Vec<PathBuf> = vec![PathBuf::from(profiles_dir).join("xml")];
    #[cfg(windows)]
    xml_candidates.push(PathBuf::from(
        r"C:\Program Files\Fanatec\FanatecService\Service\xml",
    ));
    if let Some(ref p) = config.xml.path {
        xml_candidates.push(PathBuf::from(p));
    }

    let (xml_cars_ir, xml_prof_ir) = carlist::load_for_game(&xml_candidates, "iRacing");
    let (xml_cars_ac, xml_prof_ac) = carlist::load_for_game(&xml_candidates, "AC");

    let devices = crate::open_devices();
    let col01 = crate::find_col01(&devices);
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
    if col01.is_some() {
        println!("col01 (display/ACK) found");
    }
    println!("Monitoring — press Ctrl-C to stop\n");

    let interval = Duration::from_secs(config.monitor.scan_interval_secs());
    let grace = Duration::from_secs(GAME_GONE_GRACE_SECS);

    let mut last_car: Option<CarDetected> = None;
    // Set when the game process disappears; cleared when it comes back or grace expires.
    let mut gone_since: Option<Instant> = None;

    loop {
        let current = games::detect_car();

        if current.is_some() {
            // Game is present — reset the gone timer regardless.
            gone_since = None;

            if current != last_car {
                if let Some(detected) = current.as_ref() {
                    print!("[{}] {} / {} → ", now_hms(), detected.game, detected.car);
                    let xml = if detected.game.starts_with("iRacing") {
                        (&xml_cars_ir, &xml_prof_ir)
                    } else {
                        (&xml_cars_ac, &xml_prof_ac)
                    };
                    match find_matching_profile(profiles, detected, xml) {
                        None => println!("no matching profile"),
                        Some(prof) => {
                            println!("applying {}", prof.path.display());
                            if !do_apply(&devices[dev_idx], col01, prof) {
                                // Write failed — re-probe with existing handles to recover
                                // advanced mode, then retry once.
                                eprintln!("  re-probing after write failure…");
                                match crate::probe_tuning_collection(&devices) {
                                    Some((idx, _)) => {
                                        dev_idx = idx;
                                        do_apply(&devices[dev_idx], col01, prof);
                                    }
                                    None => {
                                        eprintln!("  error: device not responding after re-probe")
                                    }
                                }
                            }
                        }
                    }
                }
                last_car = current;
            }
        } else if last_car.is_some() {
            // Game gone, but we had a known car — start or continue grace period.
            let since = gone_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= grace {
                println!("[{}] Game exited — wheel unchanged", now_hms());
                last_car = None;
                gone_since = None;
            }
            // else: still within grace window; keep last_car so a returning game
            // with a different car triggers an apply rather than looking like a
            // fresh start.
        } else {
            // No game and no previous game — nothing to track.
            gone_since = None;
        }

        std::thread::sleep(interval);
    }
}

/// Applies the profile to the device. Returns false only on a hard HID write
/// error (caller re-probes); readback failure is non-fatal.
fn do_apply(
    col03: &hid::FanatecDevice,
    col01: Option<&hid::FanatecDevice>,
    prof: &PwsProfile,
) -> bool {
    match crate::apply_write(col03, col01, prof) {
        Some(_) => true,
        None => {
            eprintln!("  applied (unverified)");
            true
        }
    }
}

fn find_matching_profile<'a>(
    profiles: &'a [PwsProfile],
    detected: &CarDetected,
    xml: (&carlist::CarNames, &carlist::ProfileMap),
) -> Option<&'a PwsProfile> {
    let (xml_cars, xml_profiles) = xml;

    // 1. XML ProfileCarsList exact match: carPath → .pws filename
    if let Some(car_path) = &detected.car_path {
        if let Some(pws_filename) = xml_profiles.get(car_path.as_str()) {
            if let Some(p) = profiles.iter().find(|p| {
                p.path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == pws_filename)
                    .unwrap_or(false)
            }) {
                return Some(p);
            }
        }
    }

    // 2. XML CarsList: carPath → display name → fuzzy match
    if let Some(car_path) = &detected.car_path {
        if let Some(xml_name) = xml_cars.get(car_path.as_str()) {
            if let Some(p) = fuzzy_match(profiles, &detected.game, xml_name) {
                return Some(p);
            }
        }
    }

    // 3. Filename-based fuzzy matching on detected car display name
    fuzzy_match(profiles, &detected.game, &detected.car)
}

fn fuzzy_match<'a>(profiles: &'a [PwsProfile], game: &str, car: &str) -> Option<&'a PwsProfile> {
    let ng = normalize(game);
    let nc = normalize(car);

    // Exact game + exact car
    if let Some(p) = profiles
        .iter()
        .find(|p| normalize(&p.game) == ng && normalize(&p.car) == nc)
    {
        return Some(p);
    }

    // Game prefix + exact car
    if let Some(p) = profiles
        .iter()
        .find(|p| normalize(&p.game).starts_with(&ng) && normalize(&p.car) == nc)
    {
        return Some(p);
    }

    // Game prefix + profile car contains detected car
    if let Some(p) = profiles
        .iter()
        .find(|p| normalize(&p.game).starts_with(&ng) && normalize(&p.car).contains(&nc))
    {
        return Some(p);
    }

    // Game prefix + detected car contains profile car (reverse)
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
