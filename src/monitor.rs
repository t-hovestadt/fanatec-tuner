use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::carlist;
use crate::config;
use crate::games::{self, CarDetected};
use crate::hid;
use crate::profile::{self, PwsProfile};

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
    // OneFanatec/xml/ is the live copy updated by the Fanatec App.
    if let Some(ref base) = config.fanatec_app.resolve() {
        xml_candidates.push(base.join("xml"));
    }
    #[cfg(windows)]
    xml_candidates.push(PathBuf::from(
        r"C:\Program Files\Fanatec\FanatecService\Service\xml",
    ));
    if let Some(ref p) = config.xml.path {
        xml_candidates.push(PathBuf::from(p));
    }

    let (xml_cars_ir, xml_prof_ir) = carlist::load_for_game(&xml_candidates, "iRacing");
    let (xml_cars_ac, xml_prof_ac) = carlist::load_for_game(&xml_candidates, "AC");

    // Extend user profiles with Fanatec App recommended profiles as a fallback.
    let mut all_profiles: Vec<PwsProfile> = profiles.to_vec();
    if let Some(ref base) = config.fanatec_app.resolve() {
        let recommended = profile::scan_recommended_profiles(&base.join("settings"));
        if !recommended.is_empty() {
            println!(
                "{} recommended profile(s) loaded from OneFanatec",
                recommended.len()
            );
        }
        all_profiles.extend(recommended);
    }

    // Enumerate to find device paths — handles open briefly then drop.
    // No probe is performed; a fresh handle is opened on each write instead.
    let (col03_path, col01_path, dev_label, dev_name, dev_pid) = {
        let mut attempt = 0u32;
        loop {
            match hid::enumerate_fanatec() {
                Ok(devs) if !devs.is_empty() => {
                    if let Some(col03) = devs
                        .iter()
                        .find(|d| crate::collection_label(&d.device_path) == "col03")
                    {
                        let col03_path = col03.device_path.clone();
                        let col01_path = devs
                            .iter()
                            .find(|d| crate::collection_label(&d.device_path) == "col01")
                            .map(|d| d.device_path.clone());
                        let label = crate::collection_label(&col03.device_path);
                        let name = col03.product_name.clone();
                        let pid = col03.product_id;
                        // devs drops here — all handles closed
                        break (col03_path, col01_path, label, name, pid);
                    }
                }
                _ => {}
            }
            attempt += 1;
            if attempt >= 12 {
                eprintln!(
                    "[monitor] device not found after 60 s — \
                     is the wheel base connected and powered on?"
                );
                std::process::exit(1);
            }
            eprintln!(
                "[monitor] probe failed, retrying in 5 s... ({}/12)",
                attempt
            );
            std::thread::sleep(Duration::from_secs(5));
        }
    };
    println!("Using {}  ({}, PID 0x{:04X})", dev_label, dev_name, dev_pid);
    if col01_path.is_some() {
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
                    match find_matching_profile(&all_profiles, detected, xml) {
                        None => println!("no matching profile"),
                        Some(prof) => {
                            if prof.recommended {
                                println!(
                                    "no per-car profile — using Fanatec recommended FFB \
                                     (FF={}, NDP={})",
                                    prof.ff, prof.ndp
                                );
                            } else {
                                println!("applying {}", prof.path.display());
                            }
                            if !do_apply_paths(&col03_path, col01_path.as_deref(), prof) {
                                eprintln!(
                                    "  error: HID write failed \
                                     (device may have been replugged)"
                                );
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

/// Opens fresh HID handles, writes the profile, then closes them.
/// Returns false only if col03 cannot be opened at all.
fn do_apply_paths(col03_path: &str, col01_path: Option<&str>, prof: &PwsProfile) -> bool {
    let col03 = match hid::open_device_by_path(col03_path) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  error: could not open col03: {}", e);
            return false;
        }
    };
    let col01 = col01_path.and_then(|p| hid::open_device_by_path(p).ok());
    match crate::apply_write(&col03, col01.as_ref(), prof) {
        Some(_) => true,
        None => {
            eprintln!("  applied (unverified)");
            true
        }
    }
    // col03 and col01 drop here → CloseHandle
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
    if let Some(p) = fuzzy_match(profiles, &detected.game, &detected.car) {
        return Some(p);
    }

    // 4. Recommended fallback: game match only (car="" so fuzzy steps skip these)
    let ng = normalize(&detected.game);
    profiles
        .iter()
        .find(|p| p.recommended && normalize(&p.game) == ng)
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
