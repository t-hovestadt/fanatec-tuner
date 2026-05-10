// iRacing shared memory: Local\IRSDKMemMapFileName
//
// Header layout (all little-endian i32):
//   offset  4 — status; bit 0 = irsdk_stConnected
//   offset 16 — sessionInfoLen (byte length of YAML blob)
//   offset 20 — sessionInfoOffset (byte offset of YAML blob)
//
// The YAML contains DriverInfo.DriverCarIdx and
// DriverInfo.Drivers[N].CarScreenName for each driver.

#[cfg(windows)]
use super::SharedMem;

const MAP_NAME: &str = "Local\\IRSDKMemMapFileName";

#[cfg(windows)]
pub fn car_name() -> Option<String> {
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
    parse_driver_car_name(yaml)
}

#[cfg(not(windows))]
pub fn car_name() -> Option<String> {
    None
}

fn parse_driver_car_name(yaml: &str) -> Option<String> {
    // Parse DriverCarIdx value
    let prefix = "DriverCarIdx:";
    let idx_pos = yaml.find(prefix)?;
    let after_key = yaml[idx_pos + prefix.len()..].trim_start();
    let car_idx_str = after_key.split(|c: char| !c.is_ascii_digit()).next()?;
    let car_idx: u32 = car_idx_str.parse().ok()?;

    // Find the Drivers block entry for this idx (exact match — not a prefix of a larger number)
    let needle = format!("CarIdx: {}", car_idx);
    let entry_pos = find_exact_car_idx(yaml, &needle)?;

    // Find CarScreenName: after this entry
    let after_entry = &yaml[entry_pos..];
    let name_prefix = "CarScreenName:";
    let name_pos = after_entry.find(name_prefix)?;
    let after_name = &after_entry[name_pos + name_prefix.len()..];

    let name = after_name.lines().next()?.trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
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

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(i32::from_le_bytes(bytes.try_into().ok()?))
}
