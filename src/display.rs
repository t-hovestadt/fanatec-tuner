#![allow(dead_code)]

/// Build an 8-byte col01 display report.
///
/// Report format (matches Linux hid-fanatecff ftec_set_display and FanaBridge DisplayEncoder):
///   [0x01, 0xF8, 0x09, 0x01, 0x02, seg1, seg2, seg3]
///
/// Seven-segment bit layout:
///   seg 0   ─── top
///   seg 5 │     │ seg 1   top-left / top-right
///   seg 6   ─── middle
///   seg 4 │     │ seg 2   bottom-left / bottom-right
///   seg 3   ─── bottom     • seg 7 = dot
pub fn build_display_report(seg1: u8, seg2: u8, seg3: u8) -> [u8; 8] {
    [0x01, 0xF8, 0x09, 0x01, 0x02, seg1, seg2, seg3]
}

// ── 7-segment constants ──────────────────────────────────────────────────────
pub const SEG_BLANK: u8 = 0x00;
pub const SEG_DOT: u8 = 0x80;
pub const SEG_DASH: u8 = 0x40;

const SEG_0: u8 = 0x3F;
const SEG_1: u8 = 0x06;
const SEG_2: u8 = 0x5B;
const SEG_3: u8 = 0x4F;
const SEG_4: u8 = 0x66;
const SEG_5: u8 = 0x6D;
const SEG_6: u8 = 0x7D;
const SEG_7: u8 = 0x07;
const SEG_8: u8 = 0x7F;
const SEG_9: u8 = 0x6F;

// Letters (7-segment approximations, matching SevenSegment.cs)
const SEG_A: u8 = 0x77;
const SEG_B: u8 = 0x7C;
const SEG_C: u8 = 0x58;
const SEG_D: u8 = 0x5E;
const SEG_E: u8 = 0x79;
const SEG_F: u8 = 0x71;
const SEG_G: u8 = 0x3D;
const SEG_H: u8 = 0x76;
const SEG_I: u8 = 0x06;
const SEG_J: u8 = 0x0E;
const SEG_K: u8 = 0x75;
const SEG_L: u8 = 0x38;
const SEG_M: u8 = 0x37;
const SEG_N: u8 = 0x54; // lowercase n
const SEG_O: u8 = 0x5C;
const SEG_P: u8 = 0x73;
const SEG_Q: u8 = 0x67;
const SEG_R: u8 = 0x50; // lowercase r (also used for Reverse gear)
const SEG_S: u8 = 0x6D;
const SEG_T: u8 = 0x78;
const SEG_U: u8 = 0x3E;
const SEG_V: u8 = 0x18;
const SEG_W: u8 = 0x7E;
const SEG_X: u8 = 0x76;
const SEG_Y: u8 = 0x6E;
const SEG_Z: u8 = 0x5B;

/// Return the 7-segment byte for a decimal digit (0–9).
pub fn digit_to_segment(d: u8) -> u8 {
    match d {
        0 => SEG_0,
        1 => SEG_1,
        2 => SEG_2,
        3 => SEG_3,
        4 => SEG_4,
        5 => SEG_5,
        6 => SEG_6,
        7 => SEG_7,
        8 => SEG_8,
        9 => SEG_9,
        _ => SEG_BLANK,
    }
}

/// Return the 7-segment byte for an ASCII character.
pub fn char_to_segment(ch: char) -> u8 {
    match ch.to_ascii_uppercase() {
        '0' => SEG_0,
        '1' => SEG_1,
        '2' => SEG_2,
        '3' => SEG_3,
        '4' => SEG_4,
        '5' => SEG_5,
        '6' => SEG_6,
        '7' => SEG_7,
        '8' => SEG_8,
        '9' => SEG_9,
        'A' => SEG_A,
        'B' => SEG_B,
        'C' => SEG_C,
        'D' => SEG_D,
        'E' => SEG_E,
        'F' => SEG_F,
        'G' => SEG_G,
        'H' => SEG_H,
        'I' => SEG_I,
        'J' => SEG_J,
        'K' => SEG_K,
        'L' => SEG_L,
        'M' => SEG_M,
        'N' => SEG_N,
        'O' => SEG_O,
        'P' => SEG_P,
        'Q' => SEG_Q,
        'R' => SEG_R,
        'S' => SEG_S,
        'T' => SEG_T,
        'U' => SEG_U,
        'V' => SEG_V,
        'W' => SEG_W,
        'X' => SEG_X,
        'Y' => SEG_Y,
        'Z' => SEG_Z,
        '-' => SEG_DASH,
        '.' | ',' => SEG_DOT,
        _ => SEG_BLANK,
    }
}

/// Display a gear number: -1=R (reverse), 0=N (neutral), 1–9.
/// The gear segment is centred in the middle digit; left and right are blank.
pub fn build_gear_display(gear: i8) -> [u8; 8] {
    let seg = match gear {
        -1 => SEG_R,
        0 => SEG_N,
        1..=9 => digit_to_segment(gear as u8),
        _ => SEG_N,
    };
    build_display_report(SEG_BLANK, seg, SEG_BLANK)
}

/// Display a speed value (0–999) across all three digits.
pub fn build_speed_display(speed: u16) -> [u8; 8] {
    let s = speed.min(999);
    build_display_report(
        digit_to_segment((s / 100) as u8),
        digit_to_segment(((s / 10) % 10) as u8),
        digit_to_segment((s % 10) as u8),
    )
}
