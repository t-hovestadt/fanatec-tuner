//! ITM OLED display protocol for Fanatec col03 (64-byte HID reports).
//!
//! All commands use the framing `[FF 05 subcmd param ...]` with zero-padding
//! to REPORT_SIZE (64 bytes).

use crate::hid::REPORT_SIZE;

fn base(subcmd: u8, param: u8) -> [u8; REPORT_SIZE] {
    let mut buf = [0u8; REPORT_SIZE];
    buf[0] = 0xFF;
    buf[1] = 0x05;
    buf[2] = subcmd;
    buf[3] = param;
    buf
}

/// Enable the OLED display.
pub fn build_itm_enable() -> [u8; REPORT_SIZE] {
    let mut buf = base(0x02, 0x01);
    buf[4] = 0x01;
    buf
}

/// Disable the OLED display.
pub fn build_itm_disable() -> [u8; REPORT_SIZE] {
    base(0x02, 0x00)
}

/// Heartbeat — keeps display alive.  Call at ~8 Hz (every 4 frames at 30 Hz).
/// `field_count` = number of active fields (0x0B for Page 1).
pub fn build_itm_heartbeat(field_count: u8) -> [u8; REPORT_SIZE] {
    let mut buf = base(0x04, 0x02);
    buf[4] = field_count;
    buf
}

/// Select a display page (1–6).
pub fn build_itm_page_select(page: u8) -> [u8; REPORT_SIZE] {
    let mut buf = base(0x04, 0x03);
    buf[4] = page;
    buf
}

/// Send a page layout packet.  `entries` is copied starting at byte 4.
pub fn build_itm_page_layout(entries: &[u8]) -> [u8; REPORT_SIZE] {
    let mut buf = base(0x01, 0x03);
    let n = entries.len().min(REPORT_SIZE - 4);
    buf[4..4 + n].copy_from_slice(&entries[..n]);
    buf
}

/// Send field-text labels.  `entries` is copied starting at byte 4.
pub fn build_itm_field_text(entries: &[u8]) -> [u8; REPORT_SIZE] {
    let mut buf = base(0x03, 0x03);
    let n = entries.len().min(REPORT_SIZE - 4);
    buf[4..4 + n].copy_from_slice(&entries[..n]);
    buf
}

/// Send 14 signed 24-bit little-endian telemetry values, packed at offset 4.
pub fn build_itm_telemetry(values: &[i32; 14]) -> [u8; REPORT_SIZE] {
    let mut buf = base(0x1D, 0x00);
    for (i, &val) in values.iter().enumerate() {
        let off = 4 + i * 3;
        if off + 2 >= REPORT_SIZE {
            break;
        }
        let u = val as u32;
        buf[off] = (u & 0xFF) as u8;
        buf[off + 1] = ((u >> 8) & 0xFF) as u8;
        buf[off + 2] = ((u >> 16) & 0xFF) as u8;
    }
    buf
}

// ── Page 1 helpers ────────────────────────────────────────────────────────────

/// Returns the two 64-byte layout packets for Page 1 (gear + speed display).
pub fn page1_layout() -> Vec<[u8; REPORT_SIZE]> {
    // Packet 1 — field positions
    #[rustfmt::skip]
    let pkt1_entries: &[u8] = &[
        0x00, 0x01, 0x00, 0x34,             // field 0x00 GEAR, large
        0x03, 0x01, 0x04, 0x00, 0x12,       // field 0x01 SPEED, medium
        0x03, 0x82, 0xF9, 0x01, 0x32,       // field 0x82 custom, small
        0x03, 0x83, 0xF5, 0x01, 0x32,       // field 0x83 custom, small
        0x03, 0x04, 0xFD, 0x01, 0x2A,       // field 0x04 lap/eng map, medium
        0x03, 0x05, 0xFE, 0x01, 0x0A,       // field 0x05 position, small
    ];
    // Packet 2 — widget type definitions
    #[rustfmt::skip]
    let pkt2_entries: &[u8] = &[
        0x00, 0x01, 0x00, 0x02, 0x00, 0x00,
        0x03, 0x01, 0x04, 0x00, 0x01, 0x05,
        0x03, 0x02, 0xF9, 0x01, 0x01, 0x00,
        0x03, 0x03, 0xF5, 0x01, 0x01, 0x00,
        0x03, 0x04, 0xFD, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00,
        0x03, 0x05, 0xFE, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00,
    ];
    vec![
        build_itm_page_layout(pkt1_entries),
        build_itm_page_layout(pkt2_entries),
    ]
}

/// Single field-text report for Page 1.
pub fn page1_field_text() -> [u8; REPORT_SIZE] {
    #[rustfmt::skip]
    let entries: &[u8] = &[
        0x82, 0x00, 0x00, 0x02, 0x2F, 0x30,        // field 0x82: "/0"
        0x03, 0x83, 0x00, 0x00, 0x02, 0x2F, 0x31,  // field 0x83: "/1"
    ];
    build_itm_field_text(entries)
}
