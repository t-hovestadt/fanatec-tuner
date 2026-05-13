use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::carlist;
use crate::config;
use crate::games::{self, CarDetected};
use crate::hid;
use crate::led;
use crate::profile::{self, PwsProfile};
use crate::tuning;

/// Commands sent from the monitor thread to the LED thread.
enum LedCmd {
    /// A new car was detected; apply its profile and start animating.
    CarChanged(Box<PwsProfile>),
    /// The game exited; clear LEDs and stop animating.
    GameExited,
}

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

    // Spawn the LED thread.  It owns the col03/col01 handles for their lifetime.
    let (led_tx, led_rx) = mpsc::channel::<LedCmd>();
    {
        let col03_for_led = col03_path.clone();
        let col01_for_led = col01_path.clone();
        std::thread::spawn(move || led_thread(col03_for_led, col01_for_led, led_rx));
    }

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
                        led_tx.send(LedCmd::CarChanged(Box::new(prof.clone()))).ok();
                    }
                }
            }
            last_car = current;
        } else if last_car.is_some() {
            // Game gone, but we had a known car — start or continue grace period.
            let since = gone_since.get_or_insert_with(Instant::now);
            if since.elapsed() >= grace {
                println!("[{}] Game exited — wheel unchanged", now_hms());
                led_tx.send(LedCmd::GameExited).ok();
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

/// Owns the HID handles for the process lifetime and drives rev LED animation
/// at ~30 Hz.  Receives `LedCmd` messages from the monitor thread via channel.
fn led_thread(col03_path: String, col01_path: Option<String>, rx: mpsc::Receiver<LedCmd>) {
    let frame = Duration::from_millis(33); // ≈30 Hz

    // Persistent handles — reopen col03 on write failure.
    let mut col03 = hid::open_device_by_path(&col03_path).ok();
    let col01 = col01_path
        .as_deref()
        .and_then(|p| hid::open_device_by_path(p).ok());

    // Animation state (populated on CarChanged).
    let mut palette = [0u16; 9];
    let mut rpm_thresholds: Vec<u32> = Vec::new();
    let mut flash_threshold: u32 = 0;
    let mut flash_period_ms: u32 = 500;
    let mut active = false;

    // Blink state.
    let mut blink_on = true;
    let mut last_flip = Instant::now();

    // Dirty tracking — only write when LED state changes.
    let mut last_leds = [0u16; 9];

    loop {
        let tick_start = Instant::now();

        // ── 1. Drain command channel (non-blocking) ───────────────────────────
        loop {
            match rx.try_recv() {
                Ok(LedCmd::CarChanged(prof)) => {
                    // Apply tuning + static LED config.
                    if let Some(ref d) = col03 {
                        match crate::apply_write(d, col01.as_ref(), &prof) {
                            Some(_) => {}
                            None => eprintln!("  applied (unverified)"),
                        }
                        send_led_config(d, &prof);
                    }
                    // Extract animation palette from profile.
                    if let Some(ref rev) = prof.rev_led {
                        for (i, &ci) in rev.colors.iter().take(9).enumerate() {
                            palette[i] = led::color_index_to_rgb565(ci);
                        }
                        rpm_thresholds = rev.rpm_thresholds.clone();
                        flash_threshold = rev.flash_threshold;
                        flash_period_ms = rev.flash_period_ms.max(50);
                        active = true;
                    } else {
                        active = false;
                    }
                    last_leds = [0u16; 9]; // force first-frame write
                }
                Ok(LedCmd::GameExited) => {
                    active = false;
                    if let Some(ref d) = col03 {
                        let off = led::build_rev_led_report(&[0u16; 9]);
                        let _ = hid::write_report(d, &off);
                    }
                    last_leds = [0u16; 9];
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // ── 2. Animate rev LEDs ───────────────────────────────────────────────
        if active {
            // Advance blink phase.
            let half = Duration::from_millis(flash_period_ms as u64 / 2);
            if last_flip.elapsed() >= half {
                blink_on = !blink_on;
                last_flip = Instant::now();
            }

            if let Some(t) = games::iracing::live_telemetry() {
                let new_leds = led::compute_rev_leds(
                    t.rpm,
                    &rpm_thresholds,
                    &palette,
                    flash_threshold,
                    blink_on,
                );
                if new_leds != last_leds {
                    let report = led::build_rev_led_report(&new_leds);
                    if let Some(ref d) = col03 {
                        if hid::write_report(d, &report).is_err() {
                            eprintln!("[led] write failed — reopening handle");
                            col03 = hid::open_device_by_path(&col03_path).ok();
                        } else {
                            last_leds = new_leds;
                        }
                    }
                }
            }
        }

        // ── 3. Sleep remainder of frame ───────────────────────────────────────
        let elapsed = tick_start.elapsed();
        if elapsed < frame {
            std::thread::sleep(frame - elapsed);
        }
    }
}

/// Send rev LED colors, clear flag LEDs, send button colors + intensities,
/// then SAVE.  All writes are non-fatal — log and continue on failure.
fn send_led_config(col03: &hid::FanatecDevice, prof: &PwsProfile) {
    // ── Rev LEDs ────────────────────────────────────────────────────────────
    if let Some(ref rev) = prof.rev_led {
        if !rev.colors.is_empty() {
            let rgb565: Vec<u16> = rev
                .colors
                .iter()
                .map(|&ci| led::color_index_to_rgb565(ci))
                .collect();
            let report = led::build_rev_led_report(&rgb565);
            if hid::write_report(col03, &report).is_ok() {
                println!("  [led] rev: {} colors", rgb565.len());
            } else {
                eprintln!("  [led] rev write failed (non-fatal)");
            }
        }
    }

    // ── Flag LEDs — clear to off; firmware overrides when flags trigger ─────
    {
        let report = led::build_flag_led_report(&[]);
        let _ = hid::write_report(col03, &report);
    }

    // ── Button LEDs ──────────────────────────────────────────────────────────
    if let Some(ref buttons) = prof.button_led {
        if !buttons.is_empty() {
            // 4-bit (0–15) channel values → RGB565
            let colors: Vec<u16> = buttons
                .iter()
                .map(|b| {
                    let r = (b.r as u32 * 255 / 15) as u8;
                    let g = (b.g as u32 * 255 / 15) as u8;
                    let b8 = (b.b as u32 * 255 / 15) as u8;
                    led::rgb_to_rgb565(r, g, b8)
                })
                .collect();
            // Stage colors (commit=false), then intensities (commit=true).
            let color_report = led::build_button_color_report(&colors, false);
            if hid::write_report(col03, &color_report).is_ok() {
                // Scale Brigthness 0–15 → intensity 0–7.
                let intensities: Vec<u8> =
                    buttons.iter().map(|b| (b.brightness / 2).min(7)).collect();
                let intensity_report = led::build_button_intensity_report(&intensities, true);
                if hid::write_report(col03, &intensity_report).is_ok() {
                    println!("  [led] buttons: {} sent", buttons.len());
                } else {
                    eprintln!("  [led] button intensity write failed (non-fatal)");
                }
            } else {
                eprintln!("  [led] button color write failed (non-fatal)");
            }
        }
    }

    // ── SAVE — persist LED config to firmware flash ──────────────────────────
    let save = tuning::build_save_report();
    if hid::write_report(col03, &save).is_ok() {
        println!("  [led] saved");
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
