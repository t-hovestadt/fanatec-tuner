use crate::hid::REPORT_SIZE;

// Wire byte addresses (verified against hid-fanatecff/hid-ftec.h)
const ADDR_SLOT: usize = 0x02;
const ADDR_SEN: usize = 0x03;
const ADDR_FF: usize = 0x04;
const ADDR_SHO: usize = 0x05;
const ADDR_BLI: usize = 0x06;
const ADDR_FFS: usize = 0x07;
const ADDR_DRI: usize = 0x09;
const ADDR_FOR: usize = 0x0a;
const ADDR_SPR: usize = 0x0b;
const ADDR_DPR: usize = 0x0c;
const ADDR_NDP: usize = 0x0d;
const ADDR_NFR: usize = 0x0e;
const ADDR_BRF: usize = 0x10;
const ADDR_FEI: usize = 0x11;
const ADDR_ACP: usize = 0x13;
const ADDR_INT: usize = 0x14;
const ADDR_NIN: usize = 0x15;
const ADDR_FUL: usize = 0x16;

pub const REPORT_ID: u8 = 0xFF;
pub const TUNING_MARKER: u8 = 0x03;

pub const CMD_REQUEST_VALUES: u8 = 0x04;
#[allow(dead_code)]
pub const CMD_SELECT_SLOT: u8 = 0x01;
#[allow(dead_code)]
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
    /// Degrees if decodable, None for large-range ambiguity
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

    /// FullForce, 0–100 (CSL DD only)
    pub ful: u8,
}

/// Decodes the SEN wire byte to degrees.
///
/// For bases with max rotation ≤ 1090°: degrees = val * 10.
/// For large-range bases (DD+, max > 1090°): three regions encoded in a single byte:
///   0x8A–0xEC → 90° to 1070°
///   0xED–0xFF → 1080° to 1260°
///   0x00–0x89 → 1270° to 2640° (byte wrapped past 0xFF)
///
/// Since we don't query max_range from the device in this PoC, we decode both
/// interpretations and return the large-range result when the byte is ≥ 0x8A,
/// falling back to the small-range formula otherwise.
pub fn decode_sen(raw: u8) -> Option<u32> {
    // For bases with max_range > 1090 (e.g. DD, DD+, DD1, DD2):
    if raw >= 0x8a && raw < 0xed {
        // Mid range: 90° to 1070°
        Some(90 + 10 * (raw - 0x8a) as u32)
    } else if raw >= 0xed {
        // High range: 1080° to 1260°
        Some(1080 + 10 * (raw - 0xed) as u32)
    } else if raw == 0x00 {
        // 0x00 often means AUTO (full range) in the driver
        None
    } else {
        // Wrapped range: 1270°+ (byte overflowed past 0xFF)
        let offset = (0x100u32 + raw as u32).wrapping_sub(0xed);
        Some(1080 + 10 * offset)
    }
}

/// Returns true if the 64-byte buffer looks like a tuning report.
pub fn is_tuning_report(buf: &[u8; REPORT_SIZE]) -> bool {
    buf[0] == REPORT_ID && buf[1] == TUNING_MARKER
}

pub fn parse_tuning_report(buf: &[u8; REPORT_SIZE]) -> TuningProfile {
    TuningProfile {
        slot: buf[ADDR_SLOT],
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
