// iRacing shared memory: Local\IRSDKMemMapFileName
//
// Header layout (all little-endian i32):
//   offset  4 — status; bit 0 = irsdk_stConnected
//   offset 16 — sessionInfoLen (byte length of YAML blob)
//   offset 20 — sessionInfoOffset (byte offset of YAML blob)
//   offset 24 — numVars (count of live-telemetry variable headers)
//   offset 28 — varHeaderOffset (byte offset of variable header array)
//   offset 52 — varBuf[0].bufOffset (byte offset of live-data buffer)
//
// Variable header (144 bytes each, starting at varHeaderOffset):
//   [0..4]   type: 1=bool, 2=int, 3=bitField, 4=float
//   [4..8]   offset into live-data buffer
//   [16..48] name (char[32], null-terminated)
//
// The YAML contains DriverInfo.DriverCarIdx and
// DriverInfo.Drivers[N].CarScreenName for each driver.

#[cfg(windows)]
use super::SharedMem;

const MAP_NAME: &str = "Local\\IRSDKMemMapFileName";

/// Live telemetry snapshot read from iRacing shared memory.
#[allow(dead_code)]
pub struct LiveTelemetry {
    pub rpm: f32,
    pub gear: i32, // -1 = R, 0 = N, 1+ = forward
    pub session_flags: u32,
    pub is_on_track: bool,
    pub speed: f32, // m/s — multiply by 2.23694 for MPH
}

/// Returns `(car_path, car_screen_name)` for the player's car.
/// `car_path` is the iRacing internal identifier (e.g. "bmwm4gt3") used for
/// XML matching; `car_screen_name` is the display name shown in-game.
#[cfg(windows)]
pub fn car_info() -> Option<(String, String)> {
    let mem = SharedMem::open(MAP_NAME)?;
    let data = mem.bytes();

    if data.len() < 24 {
        return None;
    }

    let status = read_i32_le(data, 4)?;
    if status & 1 == 0 {
        return None; // not in session
    }

    let session_len = read_i32_le(data, 16)? as usize;
    let session_off = read_i32_le(data, 20)? as usize;

    let end = session_off.checked_add(session_len)?;
    if end > data.len() {
        return None;
    }

    let yaml = std::str::from_utf8(&data[session_off..end]).ok()?;
    parse_driver_info(yaml)
}

#[cfg(not(windows))]
pub fn car_info() -> Option<(String, String)> {
    None
}

fn parse_driver_info(yaml: &str) -> Option<(String, String)> {
    // Parse DriverCarIdx value
    let prefix = "DriverCarIdx:";
    let idx_pos = yaml.find(prefix)?;
    let after_key = yaml[idx_pos + prefix.len()..].trim_start();
    let car_idx_str = after_key.split(|c: char| !c.is_ascii_digit()).next()?;
    let car_idx: u32 = car_idx_str.parse().ok()?;

    // Find the Drivers block entry for this idx (exact match)
    let needle = format!("CarIdx: {}", car_idx);
    let entry_pos = find_exact_car_idx(yaml, &needle)?;

    // Limit the search to this driver's block (before the next "- CarIdx:").
    let block_end = yaml[entry_pos + 1..]
        .find("- CarIdx:")
        .map(|p| entry_pos + 1 + p)
        .unwrap_or(yaml.len());
    let entry = &yaml[entry_pos..block_end];

    let car_name = yaml_value(entry, "CarScreenName")?;
    let car_path = yaml_value(entry, "CarPath").unwrap_or_default();

    Some((car_path, car_name))
}

fn yaml_value(text: &str, key: &str) -> Option<String> {
    let prefix = format!("{}:", key);
    let pos = text.find(&prefix)?;
    let after = &text[pos + prefix.len()..];
    let value = after.lines().next()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Finds the byte position of `needle` in `yaml` where the character immediately
/// following is not an ASCII digit (prevents CarIdx: 1 matching CarIdx: 10).
fn find_exact_car_idx(yaml: &str, needle: &str) -> Option<usize> {
    let mut start = 0;
    loop {
        let pos = yaml[start..].find(needle)?;
        let abs = start + pos;
        let next = yaml[abs + needle.len()..].chars().next().unwrap_or('\n');
        if !next.is_ascii_digit() {
            return Some(abs);
        }
        start = abs + 1;
    }
}

/// Read live telemetry variables from iRacing shared memory.
/// Returns None if iRacing is not connected or the data cannot be read.
#[cfg(windows)]
pub fn live_telemetry() -> Option<LiveTelemetry> {
    let mem = SharedMem::open(MAP_NAME)?;
    let data = mem.bytes();

    if data.len() < 56 {
        return None;
    }

    // Bit 0 of status must be set (irsdk_stConnected).
    let status = read_i32_le(data, 4)?;
    if status & 1 == 0 {
        return None;
    }

    let num_vars = read_i32_le(data, 24)? as usize;
    let var_hdr_off = read_i32_le(data, 28)? as usize;
    // varBuf[0].bufOffset is at offset 52 (48 + 4).
    let buf_off = read_i32_le(data, 52)? as usize;

    let rpm = find_var_f32(data, num_vars, var_hdr_off, buf_off, "RPM")?;
    let gear = find_var_i32(data, num_vars, var_hdr_off, buf_off, "Gear")?;
    let session_flags =
        find_var_u32(data, num_vars, var_hdr_off, buf_off, "SessionFlags").unwrap_or(0);
    let is_on_track =
        find_var_bool(data, num_vars, var_hdr_off, buf_off, "IsOnTrack").unwrap_or(false);
    let speed = find_var_f32(data, num_vars, var_hdr_off, buf_off, "Speed").unwrap_or(0.0);

    Some(LiveTelemetry {
        rpm,
        gear,
        session_flags,
        is_on_track,
        speed,
    })
}

#[cfg(not(windows))]
pub fn live_telemetry() -> Option<LiveTelemetry> {
    None
}

// ── Variable-table helpers ────────────────────────────────────────────────────

// Each irsdk_varHeader is 144 bytes:
//   [0..4]   type (i32): 1=bool, 2=int, 3=bitField, 4=float
//   [4..8]   offset (i32): byte offset into the live-data buffer
//   [16..48] name (char[32], null-terminated)

const VAR_HDR_SIZE: usize = 144;
const VAR_HDR_NAME_OFF: usize = 16;
const VAR_TYPE_BOOL: i32 = 1;
const VAR_TYPE_INT: i32 = 2;
const VAR_TYPE_BITFIELD: i32 = 3;
const VAR_TYPE_FLOAT: i32 = 4;

/// Look up a variable by name and return its (type, data_offset) pair.
fn find_var(data: &[u8], num_vars: usize, var_hdr_off: usize, name: &str) -> Option<(i32, usize)> {
    let name_bytes = name.as_bytes();
    for i in 0..num_vars {
        let hdr = var_hdr_off + i * VAR_HDR_SIZE;
        let name_slice = data.get(hdr + VAR_HDR_NAME_OFF..hdr + VAR_HDR_NAME_OFF + 32)?;
        // Compare null-terminated name.
        let var_name = name_slice.split(|&b| b == 0).next().unwrap_or(&[]);
        if var_name == name_bytes {
            let var_type = read_i32_le(data, hdr)?;
            let var_off = read_i32_le(data, hdr + 4)? as usize;
            return Some((var_type, var_off));
        }
    }
    None
}

fn find_var_f32(
    data: &[u8],
    num_vars: usize,
    var_hdr_off: usize,
    buf_off: usize,
    name: &str,
) -> Option<f32> {
    let (vtype, voff) = find_var(data, num_vars, var_hdr_off, name)?;
    if vtype != VAR_TYPE_FLOAT {
        return None;
    }
    let off = buf_off + voff;
    let bytes = data.get(off..off + 4)?;
    Some(f32::from_le_bytes(bytes.try_into().ok()?))
}

fn find_var_i32(
    data: &[u8],
    num_vars: usize,
    var_hdr_off: usize,
    buf_off: usize,
    name: &str,
) -> Option<i32> {
    let (vtype, voff) = find_var(data, num_vars, var_hdr_off, name)?;
    if vtype != VAR_TYPE_INT {
        return None;
    }
    read_i32_le(data, buf_off + voff)
}

fn find_var_u32(
    data: &[u8],
    num_vars: usize,
    var_hdr_off: usize,
    buf_off: usize,
    name: &str,
) -> Option<u32> {
    let (vtype, voff) = find_var(data, num_vars, var_hdr_off, name)?;
    if vtype != VAR_TYPE_BITFIELD {
        return None;
    }
    let off = buf_off + voff;
    let bytes = data.get(off..off + 4)?;
    Some(u32::from_le_bytes(bytes.try_into().ok()?))
}

fn find_var_bool(
    data: &[u8],
    num_vars: usize,
    var_hdr_off: usize,
    buf_off: usize,
    name: &str,
) -> Option<bool> {
    let (vtype, voff) = find_var(data, num_vars, var_hdr_off, name)?;
    if vtype != VAR_TYPE_BOOL {
        return None;
    }
    Some(data.get(buf_off + voff).copied().unwrap_or(0) != 0)
}

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(i32::from_le_bytes(bytes.try_into().ok()?))
}
