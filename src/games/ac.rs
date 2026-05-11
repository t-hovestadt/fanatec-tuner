// AC / ACC / AC EVO shared memory: Local\acpmf_static / Local\acevo_pmf_static
//
// Static info struct (leading layout identical across all three games):
//   offset  0 — smVersion   wchar_t[15] = 30 bytes
//   offset 30 — acVersion   wchar_t[15] = 30 bytes
//   offset 60 — numberOfSessions  i32
//   offset 64 — numCars           i32
//   offset 68 — carModel    wchar_t[33] = 66 bytes  ← what we read

#[cfg(windows)]
use super::SharedMem;

pub enum AcVariant {
    Evo,
    Ac1,
}

const EVO_STATIC: &str = "Local\\acevo_pmf_static";
const AC1_STATIC: &str = "Local\\acpmf_static";

/// Returns `(variant, car_path, display_name)`.
/// `car_path` is the raw carModel identifier (e.g. "ks_bmw_m4_gt3") used for
/// XML lookup; `display_name` has underscores replaced with spaces.
#[cfg(windows)]
pub fn car_name() -> Option<(AcVariant, String, String)> {
    if let Some((path, name)) = try_open(EVO_STATIC) {
        return Some((AcVariant::Evo, path, name));
    }
    if let Some((path, name)) = try_open(AC1_STATIC) {
        return Some((AcVariant::Ac1, path, name));
    }
    None
}

#[cfg(not(windows))]
pub fn car_name() -> Option<(AcVariant, String, String)> {
    None
}

#[cfg(windows)]
fn try_open(map_name: &str) -> Option<(String, String)> {
    let mem = SharedMem::open(map_name)?;
    read_car_model(mem.bytes())
}

fn read_car_model(data: &[u8]) -> Option<(String, String)> {
    // carModel is wchar_t[33] at offset 68 = 66 bytes of LE UTF-16
    if data.len() < 68 + 66 {
        return None;
    }
    let raw = &data[68..134];
    let wide: Vec<u16> = raw
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let s = String::from_utf16_lossy(&wide);
    let car_path = s.trim_end_matches('\0').trim().to_lowercase().to_string();
    if car_path.is_empty() {
        return None;
    }
    let display = car_path.replace('_', " ");
    Some((car_path, display))
}
