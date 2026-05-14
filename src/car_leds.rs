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

/// Generic fallback profile used when no car-specific entry is found.
///
/// Green → Yellow → Red sequential left-to-right, flash red — the most
/// common pattern across the iRacing car list.
pub fn default_led_profile() -> CarLedProfile {
    let mut colors = HashMap::new();
    colors.insert("stage1".to_string(), "#00FF00".to_string());
    colors.insert("stage2".to_string(), "#FFFF00".to_string());
    colors.insert("stage3".to_string(), "#FF0000".to_string());

    CarLedProfile {
        car_id: "_default".to_string(),
        iracing_car_paths: Vec::new(),
        pattern: "left_right".to_string(),
        led_count: 9,
        colors,
        stages: vec![
            LedStage {
                leds: vec![1],
                color: "stage1".to_string(),
            },
            LedStage {
                leds: vec![2],
                color: "stage1".to_string(),
            },
            LedStage {
                leds: vec![3],
                color: "stage1".to_string(),
            },
            LedStage {
                leds: vec![4],
                color: "stage2".to_string(),
            },
            LedStage {
                leds: vec![5],
                color: "stage2".to_string(),
            },
            LedStage {
                leds: vec![6],
                color: "stage2".to_string(),
            },
            LedStage {
                leds: vec![7],
                color: "stage3".to_string(),
            },
            LedStage {
                leds: vec![8],
                color: "stage3".to_string(),
            },
            LedStage {
                leds: vec![9],
                color: "stage3".to_string(),
            },
        ],
        flash_color: "#FF0000".to_string(),
        flash_hz: 15,
        pit_limiter_color: Some("#FFFF00".to_string()),
        pit_limiter_flash: Some(true),
        notes: Some("Generic fallback: green→yellow→red L→R, flash red".to_string()),
    }
}
