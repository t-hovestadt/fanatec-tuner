#![allow(dead_code)]

use crate::hid::REPORT_SIZE;

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
pub fn rgb_to_rgb565(r: u8, g: u8, b: u8) -> u16 {
    let r5 = (r >> 3) as u16 & 0x1F;
    let g6 = (g >> 2) as u16 & 0x3F;
    let b5 = (b >> 3) as u16 & 0x1F;
    (b5 << 11) | (g6 << 5) | r5
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
