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

fn read_i32_le(data: &[u8], offset: usize) -> Option<i32> {
    let bytes = data.get(offset..offset + 4)?;
    Some(i32::from_le_bytes(bytes.try_into().ok()?))
}
