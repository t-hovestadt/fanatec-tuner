//! Wheel capability detection and output gating.
//!
//! Each Fanatec wheel/hub has different output capabilities.
//! This module detects what is connected and exposes capability flags.

use crate::hid;

/// What outputs this wheel supports.
#[derive(Debug, Clone)]
pub struct WheelCaps {
    pub name: &'static str,
    pub id: u8,
    pub rev_leds: bool,      // 9 RGB rev LEDs
    pub flag_leds: bool,     // 6 RGB flag LEDs
    pub button_leds: bool,   // 12 RGB button LEDs
    pub button_rgb555: bool, // true for BMR — uses RGB555 instead of RGB565
    pub seg_display: bool,   // 3-digit 7-segment (col01)
    pub itm_oled: bool,      // ITM OLED display (FF 05)
    #[allow(dead_code)]
    pub led_protocol: LedProtocol, // which collection drives LEDs
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LedProtocol {
    Col03, // Modern wheels — LEDs via col03 (FF 01)
    Col01, // Legacy wheels — LEDs via col01
    None,  // No LED support
}

/// Known wheel database. Source: FanaBridge + Fanatec App observation.
const KNOWN_WHEELS: &[WheelCaps] = &[
    // ── Modern wheels (col03 LEDs) ──────────────────────────────────────────
    WheelCaps {
        name: "Formula V2",
        id: 10,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    WheelCaps {
        name: "Formula V2.5",
        id: 11,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    WheelCaps {
        name: "F1 Esports V2",
        id: 19,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    WheelCaps {
        name: "Podium BMW M4 GT3",
        id: 20,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    WheelCaps {
        name: "McLaren GT3 V2",
        id: 21,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    WheelCaps {
        name: "GT Extreme",
        id: 22,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    WheelCaps {
        name: "ClubSport Universal Hub v2.5",
        id: 23,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    // ── Podium Hub ──────────────────────────────────────────────────────────
    WheelCaps {
        name: "Podium Hub",
        id: 12,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    // Synthetic ID for Podium Hub + BME module combo.
    WheelCaps {
        name: "Podium Hub + BME",
        id: 120,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: true,
        led_protocol: LedProtocol::Col03,
    },
    // Synthetic ID for Podium Hub + BMR (Button Module Rally) combo.
    // BMR uses RGB555 encoding instead of RGB565.
    WheelCaps {
        name: "Podium Hub + BMR",
        id: 121,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: true,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col03,
    },
    // ── Legacy wheels (col01 LEDs) ───────────────────────────────────────────
    WheelCaps {
        name: "CSW RS",
        id: 2,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col01,
    },
    WheelCaps {
        name: "Formula Black",
        id: 3,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col01,
    },
    WheelCaps {
        name: "Formula Carbon",
        id: 4,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col01,
    },
    WheelCaps {
        name: "BMW GT2",
        id: 13,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col01,
    },
    WheelCaps {
        name: "Porsche GT3 Cup",
        id: 16,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col01,
    },
    WheelCaps {
        name: "ClubSport RS",
        id: 17,
        rev_leds: true,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: true,
        itm_oled: false,
        led_protocol: LedProtocol::Col01,
    },
    // ── No LEDs ──────────────────────────────────────────────────────────────
    WheelCaps {
        name: "CSL Elite",
        id: 6,
        rev_leds: false,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: false,
        itm_oled: false,
        led_protocol: LedProtocol::None,
    },
    WheelCaps {
        name: "WRC",
        id: 9,
        rev_leds: false,
        flag_leds: false,
        button_leds: false,
        button_rgb555: false,
        seg_display: false,
        itm_oled: false,
        led_protocol: LedProtocol::None,
    },
];

/// Look up capabilities by wheel ID.
pub fn caps_for_id(id: u8) -> Option<&'static WheelCaps> {
    KNOWN_WHEELS.iter().find(|w| w.id == id)
}

/// Default capabilities when detection fails — assume full support so the
/// user does not lose functionality. Unsupported commands are silently ignored
/// by the firmware.
pub fn default_caps() -> WheelCaps {
    WheelCaps {
        name: "Unknown (all outputs enabled)",
        id: 0,
        rev_leds: true,
        flag_leds: true,
        button_leds: true,
        button_rgb555: false,
        seg_display: true,
        itm_oled: true,
        led_protocol: LedProtocol::Col03,
    }
}

/// Re-detect the wheel after a USB disconnect/reconnect.
///
/// Sleeps 2 seconds for USB re-enumeration, then probes protobuf reports.
/// Logs the swap (old → new name) and any gained/lost capability changes.
pub fn redetect(col03_path: &str, old: &WheelCaps) -> WheelCaps {
    println!("[wheel] Re-probing after disconnect...");
    std::thread::sleep(std::time::Duration::from_secs(2));

    let new = detect(col03_path);

    if new.id != old.id {
        println!(
            "[wheel] Swapped: {} (ID {}) → {} (ID {})",
            old.name, old.id, new.name, new.id
        );
    } else {
        println!("[wheel] Reconnected: {} (ID {})", new.name, new.id);
    }

    let checks: &[(&str, bool, bool)] = &[
        ("rev-LEDs", old.rev_leds, new.rev_leds),
        ("flag-LEDs", old.flag_leds, new.flag_leds),
        ("button-LEDs", old.button_leds, new.button_leds),
        ("7-segment", old.seg_display, new.seg_display),
        ("ITM-OLED", old.itm_oled, new.itm_oled),
    ];
    for &(name, was, now) in checks {
        if was != now {
            println!("[wheel]   {} {}", if now { "+" } else { "-" }, name);
        }
    }

    new
}

/// Attempt to detect the connected wheel type by reading col03 input reports
/// and parsing protobuf fields for the wheel ID.
///
/// Reads up to 100 input reports (50 ms timeout each).
/// Returns the detected `WheelCaps`, or `default_caps()` if detection fails.
pub fn detect(col03_path: &str) -> WheelCaps {
    let dev = match hid::open_device_by_path(col03_path) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("[wheel] cannot open col03 for detection — using defaults");
            return default_caps();
        }
    };

    println!("[wheel] reading device reports...");

    let mut buf = [0u8; hid::REPORT_SIZE];
    let mut best_wheel_id: Option<u8> = None;
    let mut best_module_id: Option<u8> = None;

    // Read up to 100 reports (~2 seconds at typical report rate).
    for _ in 0..100 {
        if hid::read_report(&dev, &mut buf, 50).is_err() {
            continue;
        }
        if buf[0] == 0xFF && buf[1] == 0x10 {
            // Protobuf report — parse fields looking for wheel / module IDs.
            let fields = parse_protobuf_fields(&buf[2..]);
            for &(fnum, val) in &fields {
                // Wheel type candidates: small values (1–30) in fields 2–5.
                if (2..=5).contains(&fnum)
                    && val > 0
                    && val <= 30
                    && caps_for_id(val as u8).is_some()
                    && best_wheel_id.is_none()
                {
                    best_wheel_id = Some(val as u8);
                }
                // Module detection: values in fields 6–10.
                if (6..=10).contains(&fnum) && val > 0 && val <= 50 {
                    best_module_id = Some(val as u8);
                }
            }
            if best_wheel_id.is_some() {
                break;
            }
        }
    }

    // Handle drops the handle — led_thread opens its own.
    drop(dev);

    if let Some(wid) = best_wheel_id {
        // Podium Hub (ID 12) with a module detected → assume BME.
        if wid == 12 && best_module_id.is_some() {
            let caps = caps_for_id(120).cloned().unwrap_or_else(default_caps);
            println!(
                "[wheel] Detected: {} (hub={}, module={:?})",
                caps.name, wid, best_module_id
            );
            print_caps(&caps);
            return caps;
        }
        if let Some(caps) = caps_for_id(wid) {
            println!("[wheel] Detected: {} (ID {})", caps.name, wid);
            print_caps(caps);
            return caps.clone();
        }
    }

    println!("[wheel] Could not auto-detect wheel type — enabling all outputs");
    println!("[wheel] Run `fanatec-tuner sniff --decode` to identify your wheel ID");
    let caps = default_caps();
    print_caps(&caps);
    caps
}

fn print_caps(caps: &WheelCaps) {
    let mut features: Vec<&str> = Vec::new();
    if caps.rev_leds {
        features.push("rev-LEDs");
    }
    if caps.flag_leds {
        features.push("flag-LEDs");
    }
    if caps.button_leds {
        features.push("button-LEDs");
    }
    if caps.seg_display {
        features.push("7-segment");
    }
    if caps.itm_oled {
        features.push("ITM-OLED");
    }
    if features.is_empty() {
        println!("[wheel] Capabilities: tuning only (no LEDs/display)");
    } else {
        println!("[wheel] Capabilities: {}", features.join(" + "));
    }
}

// ── Protobuf helpers ──────────────────────────────────────────────────────────

/// Minimal protobuf varint field parser — returns `(field_number, value)` pairs
/// for varint (wire type 0) fields. Non-varint fields are consumed and skipped.
fn parse_protobuf_fields(payload: &[u8]) -> Vec<(u32, u64)> {
    let mut fields = Vec::new();
    let mut pos = 0;
    // Skip leading zero padding.
    while pos < payload.len() && payload[pos] == 0 {
        pos += 1;
    }
    while pos < payload.len() {
        let (tag, new_pos) = read_varint(payload, pos);
        pos = new_pos;
        if tag == 0 {
            break;
        }
        let field_num = (tag >> 3) as u32;
        let wire_type = tag & 0x07;
        match wire_type {
            0 => {
                // varint value
                let (val, new_pos) = read_varint(payload, pos);
                pos = new_pos;
                fields.push((field_num, val));
            }
            2 => {
                // length-delimited: consume length then skip bytes
                let (len, new_pos) = read_varint(payload, pos);
                pos = new_pos.saturating_add(len as usize).min(payload.len());
            }
            _ => break,
        }
    }
    fields
}

fn read_varint(data: &[u8], mut pos: usize) -> (u64, usize) {
    let mut result = 0u64;
    let mut shift = 0u32;
    while pos < data.len() {
        let b = data[pos];
        pos += 1;
        result |= ((b & 0x7F) as u64) << shift;
        if b & 0x80 == 0 {
            return (result, pos);
        }
        shift += 7;
        if shift >= 64 {
            break;
        }
    }
    (0, pos)
}
