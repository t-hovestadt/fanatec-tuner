#![allow(dead_code)]

use std::collections::HashMap;

use crate::car_leds::LedStage;
use crate::hid::{self, FanatecDevice, REPORT_SIZE};

// ── Col03 LED report header ──────────────────────────────────────────────────
const LED_REPORT_ID: u8 = 0xFF;
const LED_MARKER: u8 = 0x01;

pub const SUBCMD_REV: u8 = 0x00;
pub const SUBCMD_FLAG: u8 = 0x01;
pub const SUBCMD_BTN_COLORS: u8 = 0x02;
pub const SUBCMD_BTN_INTENSITIES: u8 = 0x03;

const HEADER: usize = 3; // [0xFF, 0x01, subcmd]
const BTN_COLOR_COMMIT_OFFSET: usize = 27;
const BTN_INTENSITY_COMMIT_OFFSET: usize = 18;
const MAX_REV_LEDS: usize = 9;
const MAX_FLAG_LEDS: usize = 6;
const MAX_BTN_LEDS: usize = 12;
pub const INTENSITY_PAYLOAD_SIZE: usize = 16;

/// Convert 24-bit RGB to 16-bit BGR565 (Fanatec wire format).
/// Blue in bits 15–11, green in 10–5, red in 4–0.
/// Packed big-endian in the HID report (high byte first).
pub const fn rgb_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16 & 0x1F;
    let g6 = (g >> 2) as u16 & 0x3F;
    let b5 = (b >> 3) as u16 & 0x1F;
    (b5 << 11) | (g6 << 5) | r5
}

/// Convert 24-bit RGB to 15-bit RGB555 for the Podium Button Module Rally (BMR).
/// Layout: 0RRRRRGGGGGBBBBB — bit 15 always 0.
pub const fn rgb_to_rgb555(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r as u16 >> 3) & 0x1F;
    let g5 = (g as u16 >> 3) & 0x1F;
    let b5 = (b as u16 >> 3) & 0x1F;
    (r5 << 10) | (g5 << 5) | b5
}

fn led_header(subcmd: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = LED_REPORT_ID;
    buf[1] = LED_MARKER;
    buf[2] = subcmd;
    buf
}

fn pack_colors(buf: &mut [u8; REPORT_SIZE], colors: &[u16], max: usize) {
    for (i, &c) in colors.iter().take(max).enumerate() {
        let off = HEADER + i * 2;
        buf[off] = (c >> 8) as u8;
        buf[off + 1] = (c & 0xFF) as u8;
    }
}

/// Build a Rev LED report (subcmd 0x00). Up to 9 RGB565 colors; 0x0000 = off.
pub fn build_rev_led_report(colors: &[u16]) -> [u8; REPORT_SIZE] {
    let mut buf = led_header(SUBCMD_REV);
    pack_colors(&mut buf, colors, MAX_REV_LEDS);
    buf
}

/// Build a Flag LED report (subcmd 0x01). Up to 6 RGB565 colors.
pub fn build_flag_led_report(colors: &[u16]) -> [u8; REPORT_SIZE] {
    let mut buf = led_header(SUBCMD_FLAG);
    pack_colors(&mut buf, colors, MAX_FLAG_LEDS);
    buf
}

/// Build a Button LED color report (subcmd 0x02). Up to 12 RGB565 colors.
/// `commit=false` stages; `commit=true` stages and applies immediately.
/// When sending both colors and intensities: send colors with commit=false,
/// then intensities with commit=true.
pub fn build_button_color_report(colors: &[u16], commit: bool) -> [u8; REPORT_SIZE] {
    let mut buf = led_header(SUBCMD_BTN_COLORS);
    pack_colors(&mut buf, colors, MAX_BTN_LEDS);
    buf[BTN_COLOR_COMMIT_OFFSET] = if commit { 0x01 } else { 0x00 };
    buf
}

/// Build a Button LED intensity report (subcmd 0x03).
/// `intensities`: up to 16 bytes, each 0–7 (3-bit).
/// `commit=false` stages; `commit=true` applies.
pub fn build_button_intensity_report(intensities: &[u8], commit: bool) -> [u8; REPORT_SIZE] {
    let mut buf = led_header(SUBCMD_BTN_INTENSITIES);
    for (i, &v) in intensities.iter().take(INTENSITY_PAYLOAD_SIZE).enumerate() {
        buf[HEADER + i] = v & 0x07;
    }
    buf[BTN_INTENSITY_COMMIT_OFFSET] = if commit { 0x01 } else { 0x00 };
    buf
}

/// Map a ColorIndex (from .pws profiles / CurrentSettings.xml) to (R, G, B).
/// Values 5 and 6 are unconfirmed — assumed Magenta and Cyan respectively.
pub fn color_index_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        0 => (0, 0, 0),
        1 => (255, 0, 0),
        2 => (0, 255, 0),
        3 => (0, 0, 255),
        4 => (255, 255, 0),
        5 => (255, 0, 255), // Magenta — unconfirmed
        6 => (0, 255, 255), // Cyan — unconfirmed
        7 => (255, 255, 255),
        12 => (255, 165, 0),
        _ => (0, 0, 0),
    }
}

/// Convert a ColorIndex to RGB565 via color_index_to_rgb.
pub fn color_index_to_rgb565(index: u8) -> u16 {
    let (r, g, b) = color_index_to_rgb(index);
    rgb_to_rgb565(r, g, b)
}

/// Parse a `"#RRGGBB"` hex color string to RGB565.
/// Returns 0 (off) on any parse error.
pub fn hex_to_rgb565(hex: &str) -> u16 {
    let h = hex.trim_start_matches('#');
    if h.len() != 6 {
        return 0;
    }
    let r = u8::from_str_radix(&h[0..2], 16).unwrap_or(0);
    let g = u8::from_str_radix(&h[2..4], 16).unwrap_or(0);
    let b = u8::from_str_radix(&h[4..6], 16).unwrap_or(0);
    rgb_to_rgb565(r, g, b)
}

/// Build a 9-element RGB565 palette from a `CarLedProfile`'s stages and colors maps.
///
/// Each stage specifies which Fanatec LED positions (1–9) activate and a color key.
/// The color key is resolved from `colors` (e.g. `"stage1"` → `"#00FF00"`) and
/// converted to RGB565 via [`hex_to_rgb565`].  Positions not covered by any stage
/// remain `0x0000` (off).
pub fn build_palette_from_stages(
    stages: &[LedStage],
    colors: &HashMap<String, String>,
) -> [u16; 9] {
    let mut palette = [0u16; 9];
    for stage in stages {
        let rgb565 = colors
            .get(&stage.color)
            .map(|hex| hex_to_rgb565(hex))
            .unwrap_or(0);
        for &led_pos in &stage.leds {
            // LED positions are 1-based; guard against out-of-range values.
            if let Some(slot) = palette.get_mut((led_pos as usize).saturating_sub(1)) {
                *slot = rgb565;
            }
        }
    }
    palette
}

/// Clear all wheel LEDs to off.
///
/// Sends zeroed rev, flag, and button LED reports to the wheel base.
/// Double-sends with a 50 ms gap for reliability.
/// Non-fatal — errors are silently ignored (best-effort cleanup).
pub fn clear_all_leds(dev: &FanatecDevice) {
    send_led_off(dev);
    std::thread::sleep(std::time::Duration::from_millis(50));
    send_led_off(dev);
    eprintln!("[led] all LEDs cleared");
}

fn send_led_off(dev: &FanatecDevice) {
    let _ = hid::write_report(dev, &build_rev_led_report(&[]));
    let _ = hid::write_report(dev, &build_flag_led_report(&[]));
    let _ = hid::write_report(dev, &build_button_color_report(&[], false));
    let _ = hid::write_report(dev, &build_button_intensity_report(&[], true));
}

/// Compute rev LED colors for the current RPM using per-LED thresholds.
///
/// Algorithm: LED `i` lights when `rpm >= rpm_thresholds[i]`.
///
/// Below `flash_threshold`: each lit LED shows its `palette` color.
/// At or above `flash_threshold` (blinking):
///   - `blink_on = true`  → all lit LEDs show `flash_color`
///   - `blink_on = false` → all LEDs off
///
/// `palette` must be pre-converted RGB565 values (one per LED position).
/// `flash_color` is the solid RGB565 color shown during the blink-on phase;
/// pass `0` to keep the palette colors during flash (legacy behaviour).
/// Returns 9 RGB565 values; `0x0000` = off.
pub fn compute_rev_leds(
    rpm: f32,
    rpm_thresholds: &[f64],
    palette: &[u16],
    flash_threshold: f64,
    flash_color: u16,
    blink_on: bool,
) -> [u16; 9] {
    let blinking = flash_threshold > 0.0 && rpm >= flash_threshold as f32;
    let mut out = [0u16; 9];
    for (i, slot) in out.iter_mut().enumerate() {
        let thr = rpm_thresholds.get(i).copied().unwrap_or(f64::MAX);
        if rpm >= thr as f32 {
            *slot = if blinking {
                if blink_on {
                    flash_color
                } else {
                    0
                }
            } else {
                palette.get(i).copied().unwrap_or(0)
            };
        }
    }
    out
}
