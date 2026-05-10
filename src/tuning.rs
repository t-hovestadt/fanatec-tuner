use crate::hid::REPORT_SIZE;

// Wire byte addresses — public so profile.rs can build write reports directly.
// Verified against hid-fanatecff/hid-ftec.h.
pub const ADDR_SLOT: usize = 0x02;
pub const ADDR_SEN: usize = 0x03;
pub const ADDR_FF: usize = 0x04;
pub const ADDR_SHO: usize = 0x05;
pub const ADDR_BLI: usize = 0x06;
pub const ADDR_FFS: usize = 0x07;
pub const ADDR_DRI: usize = 0x09;
pub const ADDR_FOR: usize = 0x0a;
pub const ADDR_SPR: usize = 0x0b;
pub const ADDR_DPR: usize = 0x0c;
pub const ADDR_NDP: usize = 0x0d;
pub const ADDR_NFR: usize = 0x0e;
pub const ADDR_BRF: usize = 0x10;
pub const ADDR_FEI: usize = 0x11;
pub const ADDR_ACP: usize = 0x13;
pub const ADDR_INT: usize = 0x14;
pub const ADDR_NIN: usize = 0x15;
pub const ADDR_FUL: usize = 0x16;

pub const REPORT_ID: u8 = 0xFF;
pub const TUNING_MARKER: u8 = 0x03;

pub const CMD_REQUEST_VALUES: u8 = 0x04;
pub const CMD_SELECT_SLOT: u8 = 0x01;
pub const CMD_WRITE_PARAM: u8 = 0x00;
#[allow(dead_code)]
pub const CMD_TOGGLE_ADVANCED: u8 = 0x06;

/// Parsed tuning values, all in display units.
///
/// FOR/SPR/DPR/SHO: wire byte × 10 = display value (0–100/120)
/// SEN: decoded to degrees where possible (see decode_sen)
/// BLI: 101 wire = OFF display
/// FFS: 0 = LINEAR, 1 = PEAK
pub struct TuningProfile {
    pub slot: u8,

    pub sen_raw: u8,
    /// Degrees if decodable, None for the AUTO/unknown case
    pub sen_degrees: Option<u32>,

    pub ff: u8,
    /// Wheel vibration motor, 0–100 (wire × 10)
    pub sho: u8,
    /// Brake Level Indicator, 0–100 or 101 = OFF
    pub bli: u8,
    /// Force Feedback Scaling: 0 = LINEAR, 1 = PEAK
    pub ffs: u8,
    /// Drift Mode (signed, -5 to 3), only on CSL Elite
    pub dri: i8,
    /// Force Effect Strength, 0–120 display (wire × 10)
    pub for_: u8,
    /// Spring Effect Strength, 0–120 display (wire × 10)
    pub spr: u8,
    /// Damper Effect Strength, 0–120 display (wire × 10)
    pub dpr: u8,
    /// Natural Damping, 0–100 (DD+ only)
    pub ndp: u8,
    /// Natural Friction, 0–100 (DD+ only)
    pub nfr: u8,
    /// Brake Force (load-cell), 0–100
    pub brf: u8,
    /// Force Effect Intensity, 0–100
    pub fei: u8,
    /// Analogue Paddles, 1–4
    pub acp: u8,
    /// FFB Interpolation Filter, 0–20 (DD+ only)
    pub int_: u8,
    /// Natural Inertia, 0–100 (DD+ only)
    pub nin: u8,
    /// FullForce, 0–100 (CSL DD / DD+ only)
    pub ful: u8,
}

/// Decodes the SEN wire byte to degrees.
///
/// For bases with max rotation ≤ 1090° (CSL Elite, CS V2/V2.5):
///   degrees = val × 10.  Range: wire 0x09–0x6C → 90°–1080°.
///
/// For large-range bases (DD+, DD1, DD2, max > 1090°) one byte covers three
/// sub-ranges:
///   0x8A–0xEC → 90°–1070°   (`90 + 10 × (val − 0x8A)`)
///   0xED–0xFF → 1080°–1260° (`1080 + 10 × (val − 0xED)`)
///   0x00–0x89 → 1270°–2640° (byte wrapped past 0xFF; same formula with
///                              0x100 added to the offset)
///
/// Because we don't query max_range at runtime, values in 0x8A–0xFF are
/// decoded using the large-range formula (correct for DD+), while values in
/// 0x01–0x89 could be either interpretation. 0x00 = AUTO.
pub fn decode_sen(raw: u8) -> Option<u32> {
    if (0x8a..0xed).contains(&raw) {
        Some(90 + 10 * (raw - 0x8a) as u32)
    } else if raw >= 0xed {
        Some(1080 + 10 * (raw - 0xed) as u32)
    } else if raw == 0x00 {
        None // AUTO
    } else {
        // Wrapped range: 1270°+ on large-range bases
        let offset = (0x100u32 + raw as u32).wrapping_sub(0xed);
        Some(1080 + 10 * offset)
    }
}

/// Returns true if the 64-byte buffer is a tuning report.
pub fn is_tuning_report(buf: &[u8; REPORT_SIZE]) -> bool {
    buf[0] == REPORT_ID && buf[1] == TUNING_MARKER
}

pub fn parse_tuning_report(buf: &[u8; REPORT_SIZE]) -> TuningProfile {
    TuningProfile {
        // Bits 0–3 = slot (1–5). Bit 7 = advanced_mode flag (masked out).
        slot: buf[ADDR_SLOT] & 0x0f,
        sen_raw: buf[ADDR_SEN],
        sen_degrees: decode_sen(buf[ADDR_SEN]),
        ff: buf[ADDR_FF],
        sho: buf[ADDR_SHO].saturating_mul(10),
        bli: buf[ADDR_BLI],
        ffs: buf[ADDR_FFS],
        dri: buf[ADDR_DRI] as i8,
        for_: buf[ADDR_FOR].saturating_mul(10),
        spr: buf[ADDR_SPR].saturating_mul(10),
        dpr: buf[ADDR_DPR].saturating_mul(10),
        ndp: buf[ADDR_NDP],
        nfr: buf[ADDR_NFR],
        brf: buf[ADDR_BRF],
        fei: buf[ADDR_FEI],
        acp: buf[ADDR_ACP],
        int_: buf[ADDR_INT],
        nin: buf[ADDR_NIN],
        ful: buf[ADDR_FUL],
    }
}

/// Builds a 64-byte advanced-mode toggle report (CMD 0x06).
///
/// The DD+ ignores tuning requests until this command is sent once per
/// session. Send it, wait ~500 ms, then send build_request_report().
pub fn build_wake_report() -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = REPORT_ID;
    buf[1] = TUNING_MARKER;
    buf[2] = CMD_TOGGLE_ADVANCED;
    buf
}

/// Builds a 64-byte report that requests the current tuning values.
pub fn build_request_report() -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = REPORT_ID;
    buf[1] = TUNING_MARKER;
    buf[2] = CMD_REQUEST_VALUES;
    buf
}

/// Builds a 64-byte report that selects a tuning slot (1–5).
#[allow(dead_code)]
pub fn build_select_slot_report(slot: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = REPORT_ID;
    buf[1] = TUNING_MARKER;
    buf[2] = CMD_SELECT_SLOT;
    buf[3] = slot;
    buf
}

/// Builds a 64-byte report that writes a single parameter byte.
/// `addr` is one of the ADDR_* constants. `value` is the wire-encoded byte.
pub fn build_write_report(addr: usize, value: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = REPORT_ID;
    buf[1] = TUNING_MARKER;
    buf[2] = CMD_WRITE_PARAM;
    buf[addr] = value;
    buf
}
