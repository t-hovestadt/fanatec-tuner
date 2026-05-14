#![allow(dead_code)]

use serde::Deserialize;
use std::collections::HashMap;

/// LED activation stage: which Fanatec LEDs (1–9) light up and in what color.
/// Color is a key into the parent profile's `colors` map.
#[derive(Debug, Deserialize)]
pub struct LedStage {
    /// Which LEDs (1–9, left to right on the Fanatec strip) activate in this stage.
    pub leds: Vec<u8>,
    /// Key into the profile's `colors` map (e.g. "stage1" → "#00FF00").
    pub color: String,
}

/// Per-gear RPM thresholds for the 9 Fanatec LED positions.
///
/// Sourced from Lovely Car Data (github.com/Lovely-Sim-Racing/lovely-car-data),
/// CC BY-NC-SA 4.0. Each gear key ("R", "N", "1"–"N") maps to per-LED thresholds
/// and the redline RPM at which the flash activates for that gear.
#[derive(Debug, Deserialize)]
pub struct GearRpms {
    /// RPM activation threshold for each of the 9 Fanatec LED positions (left to right).
    /// For `outside_center` pattern, positions 1 and 9 share the same threshold,
    /// positions 2 and 8 share the same threshold, and so on (symmetric).
    pub thresholds: Vec<f64>,
    /// RPM at which the shift flash activates for this gear.
    pub redline: f64,
}

/// Per-car LED shift light pattern sourced from iRacing user manuals.
///
/// RPM thresholds are **not** stored here — they come from iRacing session YAML
/// (`SLFirstRPM`, `SLLastRPM`, `SLShiftRPM`, `SLBlinkRPM`) at runtime and are
/// distributed evenly across stages.
#[derive(Debug, Deserialize)]
pub struct CarLedProfile {
    /// Human-readable name for the car.
    pub car_id: String,
    /// iRacing `CarPath` strings (lowercase) that match this car.
    /// Checked with substring containment against the session YAML value.
    pub iracing_car_paths: Vec<String>,
    /// Fill pattern: `"left_right"`, `"outside_center"`, or `"all_at_once"`.
    pub pattern: String,
    /// Number of shift LEDs on the real car (we always map to 9 Fanatec LEDs).
    pub led_count: u8,
    /// Named color definitions: `"stage1"` → `"#RRGGBB"`.
    pub colors: HashMap<String, String>,
    /// Ordered LED activation stages. Stages fire sequentially as RPM rises
    /// from `SLFirstRPM` to `SLLastRPM`.
    pub stages: Vec<LedStage>,
    /// Color of all LEDs when RPM ≥ `SLShiftRPM` (flash state).
    pub flash_color: String,
    /// Flash frequency in Hz (typically 10–15).
    pub flash_hz: u8,
    /// LED color when the pit speed limiter is active. `None` = LEDs off.
    pub pit_limiter_color: Option<String>,
    /// Whether the pit limiter LEDs flash (`true`) or hold solid (`false`).
    pub pit_limiter_flash: Option<bool>,
    /// Per-gear RPM thresholds for each of the 9 Fanatec LED positions.
    ///
    /// Key is the gear string ("R", "N", "1", "2", …). `None` for cars without
    /// verified per-gear data (legacy entries, oval/dirt cars with `pattern = "none"`).
    pub gear_rpms: Option<HashMap<String, GearRpms>>,
    /// Source notes from the iRacing user manual.
    pub notes: Option<String>,
}

/// Load all per-car LED profiles from the embedded JSON database.
pub fn load_car_led_profiles() -> Vec<CarLedProfile> {
    let json = include_str!("../data/car_led_profiles.json");
    serde_json::from_str(json).expect("Failed to parse car_led_profiles.json")
}

/// Find the LED profile for a given iRacing `CarPath` string.
///
/// Matching uses case-insensitive substring containment so that minor
/// variant suffixes in the live session YAML don't break the lookup.
pub fn find_led_profile<'a>(
    profiles: &'a [CarLedProfile],
    car_path: &str,
) -> Option<&'a CarLedProfile> {
    let car_path_lower = car_path.to_lowercase();
    profiles.iter().find(|p| {
        p.iracing_car_paths
            .iter()
            .any(|cp| car_path_lower.contains(&cp.to_lowercase()))
    })
}

/// Fallback profile used when no car-specific entry is found.
///
/// Returns a "none" pattern profile — no LED commands are sent for unknown cars.
/// Only cars with verified LED data (sourced from Lovely Car Data) get LED output.
pub fn default_led_profile() -> CarLedProfile {
    CarLedProfile {
        car_id: "_default".to_string(),
        iracing_car_paths: Vec::new(),
        pattern: "none".to_string(),
        led_count: 0,
        colors: HashMap::new(),
        stages: Vec::new(),
        flash_color: "#000000".to_string(),
        flash_hz: 0,
        pit_limiter_color: None,
        pit_limiter_flash: None,
        gear_rpms: None,
        notes: Some("No verified LED data — tuning only".to_string()),
    }
}
