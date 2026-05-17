use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::car_leds;
use crate::carlist;
use crate::config;
use crate::display;
use crate::games::{self, CarDetected};
use crate::hid;
use crate::itm;
use crate::led;
use crate::profile::{self, PwsProfile};
use crate::tuning;
use crate::wheel;

/// Commands sent from the monitor thread to the LED thread.
enum LedCmd {
    /// A new car was detected; apply its profile and start animating.
    /// The second field is the iRacing `carPath` (used to look up the
    /// LED profile from `car_led_profiles.json`). `None` for non-iRacing games.
    CarChanged(Box<PwsProfile>, Option<String>),
    /// The game exited; clear LEDs and stop animating.
    GameExited,
    /// fanatec-tuner is shutting down (Ctrl-C); clear all LEDs then return.
    Shutdown,
}

/// How long to wait for iRacing's process to reappear after it disappears
/// during a session transition before declaring the game exited.
const GAME_GONE_GRACE_SECS: u64 = 30;

// ── iRacing SessionFlags bitfield ────────────────────────────────────────────
const FLAG_CHECKERED: u32 = 0x0000_0001;
const FLAG_WHITE: u32 = 0x0000_0002;
const FLAG_GREEN: u32 = 0x0000_0004;
const FLAG_YELLOW: u32 = 0x0000_0008;
const FLAG_RED: u32 = 0x0000_0010;
const FLAG_BLUE: u32 = 0x0000_0020;
const FLAG_DEBRIS: u32 = 0x0000_0040;
const FLAG_YELLOW_WAVING: u32 = 0x0000_0100;
const FLAG_ONE_TO_GREEN: u32 = 0x0000_0200;
const FLAG_GREEN_HELD: u32 = 0x0000_0400;
const FLAG_CAUTION: u32 = 0x0000_4000;
const FLAG_CAUTION_WAVING: u32 = 0x0000_8000;
const FLAG_BLACK: u32 = 0x0001_0000;
const FLAG_DISQUALIFY: u32 = 0x0002_0000;
const FLAG_FURLED: u32 = 0x0008_0000; // meatball — mechanical issue
const FLAG_REPAIR: u32 = 0x0010_0000;

// ── Flag LED colors (BGR565, computed at compile time) ────────────────────────
const FLAG_COLOR_YELLOW: u16 = led::rgb_to_rgb565(255, 255, 0);
const FLAG_COLOR_BLUE: u16 = led::rgb_to_rgb565(0, 0, 255);
const FLAG_COLOR_GREEN: u16 = led::rgb_to_rgb565(0, 255, 0);
const FLAG_COLOR_RED: u16 = led::rgb_to_rgb565(255, 0, 0);
const FLAG_COLOR_WHITE: u16 = led::rgb_to_rgb565(255, 255, 255);
const FLAG_COLOR_ORANGE: u16 = led::rgb_to_rgb565(255, 165, 0);

/// Blink period for flashing flag LEDs (2.5 Hz).
const FLAG_FLASH_PERIOD_MS: u64 = 400;

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

    // Detect wheel capabilities before spawning the LED thread.
    let wheel_caps = wheel::detect(&col03_path);

    println!("Monitoring — press Ctrl-C to stop\n");

    // Spawn the LED thread.  It owns the col03/col01 handles for their lifetime.
    let (led_tx, led_rx) = mpsc::channel::<LedCmd>();
    {
        let col03_for_led = col03_path.clone();
        let col01_for_led = col01_path.clone();
        let caps_for_led = wheel_caps.clone();
        std::thread::spawn(move || led_thread(col03_for_led, col01_for_led, led_rx, caps_for_led));
    }

    // Register Ctrl-C handler — sends Shutdown so the LED thread clears all LEDs
    // before we exit.  Sleeps 300 ms to give the thread time (~150 ms) to finish.
    {
        let led_tx_ctrlc = led_tx.clone();
        ctrlc::set_handler(move || {
            let _ = led_tx_ctrlc.send(LedCmd::Shutdown);
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::process::exit(0);
        })
        .expect("Error setting Ctrl-C handler");
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
                        led_tx
                            .send(LedCmd::CarChanged(
                                Box::new(prof.clone()),
                                detected.car_path.clone(),
                            ))
                            .ok();
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
fn led_thread(
    col03_path: String,
    col01_path: Option<String>,
    rx: mpsc::Receiver<LedCmd>,
    caps: wheel::WheelCaps,
) {
    let frame = Duration::from_millis(33); // ≈30 Hz

    // Load car LED profiles once — embedded in the binary via include_str!.
    // Used to decide whether to send LED commands for a given car.
    let car_led_profiles = car_leds::load_car_led_profiles();

    // Persistent handles — reopen col03 on write failure.
    let mut col03 = hid::open_device_by_path(&col03_path).ok();
    let col01 = col01_path
        .as_deref()
        .and_then(|p| hid::open_device_by_path(p).ok());

    // Animation state (populated on CarChanged).
    //
    // gear_rpms: per-gear thresholds + redline from Lovely Car Data.
    // Key = gear string ("R", "N", "1", ...).
    let mut gear_rpms: HashMap<String, (Vec<f64>, f64)> = HashMap::new();
    // palette: pre-converted RGB565 per LED position (built from CarLedProfile stages).
    let mut palette = [0u16; 9];
    // flash_color_rgb565: color shown for all lit LEDs during the blink-on phase.
    let mut flash_color_rgb565: u16 = 0;
    // flash_period_ms: full on+off blink period in milliseconds.
    let mut flash_period_ms: u32 = 500;
    // Fallback: static .pws thresholds used only when gear_rpms is empty.
    let mut static_thresholds: Vec<f64> = Vec::new();
    let mut static_flash_threshold: f64 = 0.0;
    let mut active = false;

    // Blink state.
    let mut blink_on = true;
    let mut last_flip = Instant::now();

    // Dirty tracking — only write when LED state changes.
    let mut last_leds = [0u16; 9];
    // Gear display state (col01 7-segment).
    let mut last_gear: i32 = i32::MIN; // force first-frame write
                                       // Flag LED state (6 LEDs on col03).
    let mut last_flag_leds = [0u16; 6];
    let mut flag_blink_on = true;
    let mut flag_last_flip = Instant::now();

    // ITM OLED state.
    let mut itm_active = false;
    let mut itm_heartbeat_counter: u32 = 0; // fires every 4 frames  (~8 Hz)
    let mut itm_telemetry_counter: u32 = 0; // fires every 15 frames (~2 Hz)

    loop {
        let tick_start = Instant::now();

        // ── 1. Drain command channel (non-blocking) ───────────────────────────
        loop {
            match rx.try_recv() {
                Ok(LedCmd::CarChanged(prof, car_path)) => {
                    // Find the Lovely Car Data LED profile (verified = pattern != "none").
                    let led_profile_opt = car_path
                        .as_deref()
                        .and_then(|cp| car_leds::find_led_profile(&car_led_profiles, cp))
                        .filter(|p| p.pattern != "none");
                    let has_verified_leds = led_profile_opt.is_some();

                    // Apply tuning always; send LED firmware config only when verified.
                    if let Some(ref d) = col03 {
                        // Clear previous car's LED state before applying new profile.
                        led::clear_all_leds(d);
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        match crate::apply_write(d, col01.as_ref(), &prof) {
                            Some(_) => {}
                            None => eprintln!("  applied (unverified)"),
                        }
                        if has_verified_leds {
                            send_led_config(d, &prof, &caps);
                        } else {
                            println!("  [led] no verified LED data — tuning only");
                        }
                    }

                    // Reset animation state for the new car.
                    gear_rpms.clear();
                    static_thresholds.clear();
                    static_flash_threshold = 0.0;
                    flash_color_rgb565 = 0;
                    active = false;

                    if let Some(lp) = led_profile_opt {
                        // Build stage-based palette and flash color from Lovely Car Data.
                        palette = led::build_palette_from_stages(&lp.stages, &lp.colors);
                        flash_color_rgb565 = led::hex_to_rgb565(&lp.flash_color);
                        flash_period_ms = if lp.flash_hz > 0 {
                            (1000u32 / lp.flash_hz as u32).max(50)
                        } else {
                            500
                        };

                        if let Some(ref gr) = lp.gear_rpms {
                            // Priority 1 — Lovely Car Data per-gear thresholds.
                            for (gear, grpms) in gr {
                                gear_rpms.insert(
                                    gear.clone(),
                                    (grpms.thresholds.clone(), grpms.redline),
                                );
                            }
                            active = true;
                        } else if let Some(ref rev) = prof.rev_led {
                            // Priority 2 — .pws static thresholds (keep Lovely palette).
                            static_thresholds =
                                rev.rpm_thresholds.iter().map(|&x| x as f64).collect();
                            static_flash_threshold = rev.flash_threshold as f64;
                            flash_period_ms = rev.flash_period_ms.max(50);
                            active = true;
                        }
                    }

                    last_leds = [0u16; 9]; // force first-frame write
                    last_gear = i32::MIN; // force gear display update on next frame
                    last_flag_leds = [0u16; 6]; // force flag update on next frame

                    // ── ITM OLED init ──────────────────────────────────────────
                    if caps.itm_oled {
                        if let Some(ref d) = col03 {
                            let _ = hid::write_report(d, &itm::build_itm_enable());
                            std::thread::sleep(Duration::from_millis(50));
                            let _ = hid::write_report(d, &itm::build_itm_page_select(0x01));
                            std::thread::sleep(Duration::from_millis(50));
                            for pkt in itm::page1_layout() {
                                let _ = hid::write_report(d, &pkt);
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            let _ = hid::write_report(d, &itm::page1_field_text());
                            itm_active = true;
                            itm_heartbeat_counter = 0;
                            itm_telemetry_counter = 0;
                            println!("  [itm] OLED enabled — Page 1");
                        }
                    }
                }
                Ok(LedCmd::GameExited) => {
                    active = false;
                    if let Some(ref d) = col03 {
                        led::clear_all_leds(d);
                    }
                    // Clear 7-segment display on game exit.
                    if caps.seg_display {
                        if let Some(ref col01_dev) = col01 {
                            let blank = display::build_display_report(
                                display::SEG_BLANK,
                                display::SEG_BLANK,
                                display::SEG_BLANK,
                            );
                            let _ = hid::write_raw(col01_dev, &blank);
                        }
                    }
                    last_leds = [0u16; 9];
                    last_gear = i32::MIN;
                    last_flag_leds = [0u16; 6];

                    // Disable OLED on game exit.
                    if caps.itm_oled && itm_active {
                        if let Some(ref d) = col03 {
                            let _ = hid::write_report(d, &itm::build_itm_disable());
                        }
                        itm_active = false;
                        println!("  [itm] OLED disabled");
                    }
                }
                Ok(LedCmd::Shutdown) => {
                    if let Some(ref d) = col03 {
                        led::clear_all_leds(d);
                    }
                    // Clear 7-segment display on shutdown.
                    if caps.seg_display {
                        if let Some(ref col01_dev) = col01 {
                            let blank = display::build_display_report(
                                display::SEG_BLANK,
                                display::SEG_BLANK,
                                display::SEG_BLANK,
                            );
                            let _ = hid::write_raw(col01_dev, &blank);
                        }
                    }
                    // Disable OLED on shutdown.
                    if caps.itm_oled && itm_active {
                        if let Some(ref d) = col03 {
                            let _ = hid::write_report(d, &itm::build_itm_disable());
                        }
                    }
                    return;
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        // ── 2. Animate rev LEDs ───────────────────────────────────────────────
        if active {
            // Advance rev LED blink phase.
            let half = Duration::from_millis(flash_period_ms as u64 / 2);
            if last_flip.elapsed() >= half {
                blink_on = !blink_on;
                last_flip = Instant::now();
            }
            // Advance flag LED blink phase (independent rate from rev LEDs).
            let flag_half = Duration::from_millis(FLAG_FLASH_PERIOD_MS / 2);
            if flag_last_flip.elapsed() >= flag_half {
                flag_blink_on = !flag_blink_on;
                flag_last_flip = Instant::now();
            }

            if let Some(t) = games::iracing::live_telemetry() {
                // Select per-gear thresholds from Lovely Car Data, or fall back to
                // static .pws thresholds when gear_rpms is empty.
                let gear_str: String = match t.gear {
                    -1 => "R".to_string(),
                    0 => "N".to_string(),
                    g => g.to_string(),
                };
                let (thresholds, flash_thr): (&[f64], f64) = if !gear_rpms.is_empty() {
                    gear_rpms
                        .get(&gear_str)
                        .or_else(|| gear_rpms.get("N"))
                        .map(|(thr, rl)| (thr.as_slice(), *rl))
                        .unwrap_or((&[], 0.0))
                } else {
                    (static_thresholds.as_slice(), static_flash_threshold)
                };

                // ── Rev LEDs ──────────────────────────────────────────────────
                if caps.rev_leds {
                    let new_leds = led::compute_rev_leds(
                        t.rpm,
                        thresholds,
                        &palette,
                        flash_thr,
                        flash_color_rgb565,
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

                // ── Gear display (col01 7-segment) ────────────────────────────
                if caps.seg_display && t.gear != last_gear {
                    if let Some(ref col01_dev) = col01 {
                        let report = display::build_gear_display(t.gear as i8);
                        let _ = hid::write_raw(col01_dev, &report);
                        last_gear = t.gear;
                    }
                }

                // ── Flag LEDs (6 flag LEDs on col03) ──────────────────────────
                if caps.flag_leds {
                    let new_flag_leds = match flag_color(t.session_flags) {
                        Some((color, flash)) => {
                            if flash && !flag_blink_on {
                                [0u16; 6]
                            } else {
                                [color; 6]
                            }
                        }
                        None => [0u16; 6],
                    };
                    if new_flag_leds != last_flag_leds {
                        let report = led::build_flag_led_report(&new_flag_leds);
                        if let Some(ref d) = col03 {
                            let _ = hid::write_report(d, &report);
                        }
                        last_flag_leds = new_flag_leds;
                    }
                }
            }
        }

        // ── 3. ITM OLED heartbeat + telemetry ────────────────────────────────
        if caps.itm_oled && itm_active {
            itm_heartbeat_counter += 1;
            itm_telemetry_counter += 1;

            // Heartbeat at ~8 Hz (every 4 frames × 33 ms ≈ 132 ms).
            if itm_heartbeat_counter >= 4 {
                itm_heartbeat_counter = 0;
                if let Some(ref d) = col03 {
                    let _ = hid::write_report(d, &itm::build_itm_heartbeat(0x0B));
                }
            }

            // Telemetry at ~2 Hz (every 15 frames × 33 ms ≈ 495 ms).
            if itm_telemetry_counter >= 15 {
                itm_telemetry_counter = 0;
                if let Some(t) = games::iracing::live_telemetry() {
                    let speed_mph = (t.speed * 2.23694_f32) as i32;
                    let mut values = [0i32; 14];
                    values[0] = t.gear;
                    values[1] = speed_mph;
                    if let Some(ref d) = col03 {
                        let _ = hid::write_report(d, &itm::build_itm_telemetry(&values));
                    }
                }
            }
        }

        // ── 4. Sleep remainder of frame ───────────────────────────────────────
        let elapsed = tick_start.elapsed();
        if elapsed < frame {
            std::thread::sleep(frame - elapsed);
        }
    }
}

/// Send rev LED colors, clear flag LEDs, send button colors + intensities,
/// then SAVE.  All writes are non-fatal — log and continue on failure.
fn send_led_config(col03: &hid::FanatecDevice, prof: &PwsProfile, caps: &wheel::WheelCaps) {
    // ── Rev LEDs ────────────────────────────────────────────────────────────
    if caps.rev_leds {
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
    }

    // ── Flag LEDs — clear to off; firmware overrides when flags trigger ─────
    if caps.flag_leds {
        let report = led::build_flag_led_report(&[]);
        let _ = hid::write_report(col03, &report);
    }

    // ── Button LEDs ──────────────────────────────────────────────────────────
    if caps.button_leds {
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
    } // end if caps.button_leds

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

/// Map an iRacing `SessionFlags` bitfield to a flag LED color and flash mode.
///
/// Returns `(color_rgb565, should_flash)` for the highest-priority active flag,
/// or `None` when no relevant flag is set (LEDs off).  Priority order mirrors
/// real racing convention: driver penalties > red > yellow > blue > white >
/// checkered > green.
fn flag_color(session_flags: u32) -> Option<(u16, bool)> {
    // Driver-specific penalty flags — highest priority.
    if session_flags & (FLAG_BLACK | FLAG_DISQUALIFY) != 0 {
        return Some((FLAG_COLOR_ORANGE, true)); // black flag shown as flashing orange
    }
    if session_flags & (FLAG_FURLED | FLAG_REPAIR) != 0 {
        return Some((FLAG_COLOR_ORANGE, true)); // meatball / repair flag
    }
    // Session flags.
    if session_flags & FLAG_RED != 0 {
        return Some((FLAG_COLOR_RED, false)); // solid — session stopped
    }
    if session_flags
        & (FLAG_YELLOW | FLAG_CAUTION | FLAG_CAUTION_WAVING | FLAG_YELLOW_WAVING | FLAG_DEBRIS)
        != 0
    {
        return Some((FLAG_COLOR_YELLOW, true)); // flashing yellow
    }
    if session_flags & FLAG_BLUE != 0 {
        return Some((FLAG_COLOR_BLUE, true)); // flashing blue — let faster car pass
    }
    if session_flags & FLAG_WHITE != 0 {
        return Some((FLAG_COLOR_WHITE, false)); // solid — final lap
    }
    if session_flags & FLAG_CHECKERED != 0 {
        return Some((FLAG_COLOR_WHITE, true)); // flashing white — finish
    }
    if session_flags & (FLAG_GREEN | FLAG_GREEN_HELD | FLAG_ONE_TO_GREEN) != 0 {
        return Some((FLAG_COLOR_GREEN, false)); // solid — go / formation
    }
    None // no relevant flag active
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
