use std::path::{Path, PathBuf};

/// RPM-LED profile parsed from the `<RevLedProfileWheel>` section of a .pws file.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct RevLedProfile {
    /// Per-LED RPM activation thresholds (one per LED, up to 9).
    pub rpm_thresholds: Vec<u32>,
    /// ColorIndex per LED (see `led::color_index_to_rgb`).
    pub colors: Vec<u8>,
    /// RPM above which the strip flashes.
    pub flash_threshold: u32,
    /// Flash period in milliseconds.
    pub flash_period_ms: u32,
}

/// One button's constant-color config from the `<ButtonLedProfile>` section.
/// Colors use the 0–15 scale from FanaLab's JSON (4-bit per channel).
/// Scale to 0–255 with `val * 255 / 15` before converting to RGB565.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ButtonLedEntry {
    pub r: u8,          // 0–15
    pub g: u8,          // 0–15
    pub b: u8,          // 0–15
    pub brightness: u8, // 0–15; scale to 0–7 with `val / 2` for the intensity report
}

/// Tuning values parsed from a .pws file.
/// All fields use display units (same as the FanaLab JSON).
/// Wire encoding differs for some params — see `wire_*` helpers below.
#[derive(Debug, Clone)]
pub struct PwsProfile {
    #[allow(dead_code)]
    pub name: String,
    pub game: String,
    pub car: String,
    pub path: PathBuf,

    /// BaseType from the <Device> tag (e.g. 12 = CS DD+). Preserved for future
    /// LED/display matching — profiles carry wheel-specific LED sections.
    #[allow(dead_code)]
    pub base_type: Option<u8>,
    /// WheelType from the <Device> tag (e.g. 19 = specific rim model). Used to
    /// select the correct RevLedProfile / ButtonLedProfile / ITMProfile section
    /// when LED control is implemented.
    #[allow(dead_code)]
    pub wheel_type: Option<u8>,
    /// Rev LED strip profile from <RevLedProfileWheel>.
    #[allow(dead_code)]
    pub rev_led: Option<RevLedProfile>,
    /// Per-button constant colors from <ButtonLedProfile> (up to 12 buttons).
    /// None if the profile has no button LED section (wheels without button LEDs).
    #[allow(dead_code)]
    pub button_led: Option<Vec<ButtonLedEntry>>,

    /// True for Fanatec App recommended profiles (game-wide fallback).
    pub recommended: bool,

    pub sen: u8, // raw wire byte — sent to device as-is
    pub ff: u8,
    pub ffs: u8,
    pub ndp: u8,
    pub nfr: u8,
    pub nin: u8,
    pub int_: u8,
    pub fei: u8,
    pub for_: u8, // display 0–120; wire = display / 10
    pub spr: u8,
    pub dpr: u8,
    pub bli: u8, // 101 = OFF
    pub sho: u8, // display 0–100; wire = display / 10
    pub brf: u8, // display 0–100; wire = raw display value (NOT /10)
    pub ful: u8, // 0 = OFF
    pub dri: u8,
    pub acp: u8, // JSON key "APM"
}

impl PwsProfile {
    /// Wire byte for FOR/SPR/DPR/SHO/BRF (display ÷ 10).
    pub fn wire_div10(display: u8) -> u8 {
        display / 10
    }

    /// Builds a 64-byte write report directly from this profile's values.
    /// Does not require reading the current device state first.
    #[allow(dead_code)]
    pub fn to_write_report(&self) -> [u8; crate::hid::REPORT_SIZE] {
        use crate::tuning::{
            ADDR_ACP, ADDR_BLI, ADDR_BRF, ADDR_DPR, ADDR_DRI, ADDR_FEI, ADDR_FF, ADDR_FFS,
            ADDR_FOR, ADDR_FUL, ADDR_INT, ADDR_NDP, ADDR_NFR, ADDR_NIN, ADDR_SEN, ADDR_SHO,
            ADDR_SPR, CMD_WRITE_PARAM, REPORT_ID, TUNING_MARKER,
        };
        let mut buf = [0u8; crate::hid::REPORT_SIZE];
        buf[0] = REPORT_ID;
        buf[1] = TUNING_MARKER;
        buf[2] = CMD_WRITE_PARAM;
        buf[3] = 0x01; // devId — must not be 0x81 (device silently ignores writes)
        buf[4] = 0x01; // UserSetupIndex = slot 1
        buf[ADDR_SEN + 2] = self.sen;
        buf[ADDR_FF + 2] = self.ff;
        buf[ADDR_FFS + 2] = self.ffs;
        buf[ADDR_NDP + 2] = self.ndp;
        buf[ADDR_NFR + 2] = self.nfr;
        buf[ADDR_NIN + 2] = self.nin;
        buf[ADDR_INT + 2] = self.int_;
        buf[ADDR_FEI + 2] = self.fei;
        buf[ADDR_FOR + 2] = Self::wire_div10(self.for_);
        buf[ADDR_SPR + 2] = Self::wire_div10(self.spr);
        buf[ADDR_DPR + 2] = Self::wire_div10(self.dpr);
        buf[ADDR_BLI + 2] = self.bli;
        buf[ADDR_SHO + 2] = Self::wire_div10(self.sho);
        buf[ADDR_BRF + 2] = Self::wire_div10(self.brf);
        buf[ADDR_FUL + 2] = self.ful;
        buf[ADDR_DRI + 2] = self.dri;
        buf[ADDR_ACP + 2] = self.acp;
        buf
    }

    /// Returns all tuning values as `(ADDR_*, wire_value)` pairs for use with
    /// `build_fb_write_report` (overlay at `buf[addr+1]`) or
    /// `build_full_write_report` (overlay at `buf[addr+2]`).
    pub fn to_params(&self) -> Vec<(usize, u8)> {
        use crate::tuning::{
            ADDR_ACP, ADDR_BLI, ADDR_BRF, ADDR_DPR, ADDR_DRI, ADDR_FEI, ADDR_FF, ADDR_FFS,
            ADDR_FOR, ADDR_FUL, ADDR_INT, ADDR_NDP, ADDR_NFR, ADDR_NIN, ADDR_SEN, ADDR_SHO,
            ADDR_SPR,
        };
        vec![
            (ADDR_SEN, self.sen),
            (ADDR_FF, self.ff),
            (ADDR_FFS, self.ffs),
            (ADDR_NDP, self.ndp),
            (ADDR_NFR, self.nfr),
            (ADDR_NIN, self.nin),
            (ADDR_INT, self.int_),
            (ADDR_FEI, self.fei),
            (ADDR_FOR, Self::wire_div10(self.for_)),
            (ADDR_SPR, Self::wire_div10(self.spr)),
            (ADDR_DPR, Self::wire_div10(self.dpr)),
            (ADDR_BLI, self.bli),
            (ADDR_SHO, Self::wire_div10(self.sho)),
            (ADDR_BRF, self.brf), // BRF is sent as the raw display value, not /10
            (ADDR_FUL, self.ful),
            (ADDR_DRI, self.dri),
            (ADDR_ACP, self.acp),
        ]
    }
}

// ── XML helpers ──────────────────────────────────────────────────────────────

/// Extract a numeric attribute from a named XML tag.
/// Finds the first `<tag_name ` occurrence and returns the parsed u8 value of `attr`.
fn xml_attr_u8(text: &str, tag_name: &str, attr: &str) -> Option<u8> {
    let needle = format!("<{} ", tag_name);
    let tag_start = text.find(&needle)?;
    let tag_end = text[tag_start..].find('>')?;
    let tag_text = &text[tag_start..tag_start + tag_end + 1];
    let attr_needle = format!("{}=\"", attr);
    let start = tag_text.find(&attr_needle)? + attr_needle.len();
    let end = tag_text[start..].find('"')?;
    tag_text[start..start + end].parse().ok()
}

/// Extract the text content of a simple `<tag>value</tag>` element (first occurrence).
fn xml_tag_value<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)?;
    Some(text[start..start + end].trim())
}

fn xml_tag_u32(text: &str, tag: &str) -> Option<u32> {
    xml_tag_value(text, tag)?.parse().ok()
}

fn xml_tag_i32(text: &str, tag: &str) -> Option<i32> {
    xml_tag_value(text, tag)?.parse().ok()
}

// ── Section JSON helper ───────────────────────────────────────────────────────

/// Return the text between the first `<JSON>…</JSON>` tags inside a named
/// XML section.  E.g. `extract_section_json(text, "RevLedProfileWheel")`.
fn extract_section_json<'a>(text: &'a str, section: &str) -> Option<&'a str> {
    let open = format!("<{}>", section);
    let section_pos = text.find(&open)?;
    let json_rel = text[section_pos..].find("<JSON>")?;
    let json_start = section_pos + json_rel + "<JSON>".len();
    let json_len = text[json_start..].find("</JSON>")?;
    Some(&text[json_start..json_start + json_len])
}

// ── RevLedProfile parser ──────────────────────────────────────────────────────

/// Parse the `<RevLedProfileWheel>` section from .pws XML text.
///
/// Actual .pws JSON structure (Maurice format v4.x):
/// ```json
/// { "RevLedGearsForward": [ { "ColorRaw": { "Colors": [ {"ColorIndex": 2}, … ] },
///                              "RPMRawThreshold": [3500, …], … } ] }
/// ```
/// Colors are objects `{"ColorIndex": N}`, not plain integers.
/// Falls back to a top-level `ColorRaw` if `RevLedGearsForward` is absent.
fn parse_rev_led(text: &str) -> Option<RevLedProfile> {
    let json_str = extract_section_json(text, "RevLedProfileWheel")?;
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Per-gear array: use gear-0 entry as the representative color set.
    // Fall back to the top-level object for older / unknown formats.
    let gear: &serde_json::Value = v
        .get("RevLedGearsForward")
        .and_then(|x| x.as_array())
        .and_then(|arr| arr.first())
        .unwrap_or(&v);

    let colors: Vec<u8> = gear
        .get("ColorRaw")
        .and_then(|cr| cr.get("Colors"))
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    // New format: {"ColorIndex": N}
                    item.get("ColorIndex")
                        .and_then(|x| x.as_u64())
                        .map(|n| n as u8)
                        // Old format: plain integer
                        .or_else(|| item.as_u64().map(|n| n as u8))
                })
                .collect()
        })
        .unwrap_or_default();

    let rpm_thresholds = gear
        .get("RPMRawThreshold")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();

    let flash_threshold = gear
        .get("RPMFlashRawThreshold")
        .and_then(|x| x.as_u64())
        .unwrap_or(0) as u32;

    let flash_period_ms = gear
        .get("PeriodFlashRaw")
        .and_then(|x| x.as_u64())
        .unwrap_or(0) as u32;

    Some(RevLedProfile {
        rpm_thresholds,
        colors,
        flash_threshold,
        flash_period_ms,
    })
}

// ── ButtonLedProfile parser ───────────────────────────────────────────────────

/// Parse the `<ButtonLedProfile>` section from .pws XML text.
///
/// JSON structure (Maurice format v4.x):
/// ```json
/// { "LastUsed": "BMW",
///   "ButtonLedsBMW": [ { "ConstantColor": { "Red":0,"Green":15,"Blue":0,"Brigthness":12 },
///                         … }, … ] }
/// ```
/// Colors are 0–15 (4-bit); Brigthness is 0–15.
/// Returns the `ConstantColor` per button for the wheel named by `LastUsed`.
fn parse_button_led(text: &str) -> Option<Vec<ButtonLedEntry>> {
    let json_str = extract_section_json(text, "ButtonLedProfile")?;
    let v: serde_json::Value = serde_json::from_str(json_str).ok()?;

    // Find the active wheel array: ButtonLeds{LastUsed}
    let last_used = v.get("LastUsed").and_then(|x| x.as_str()).unwrap_or("");
    let key = format!("ButtonLeds{}", last_used);
    let arr = v
        .get(&key)
        .or_else(|| {
            // Fallback: first ButtonLeds* array found
            v.as_object()?.values().find(|val| val.is_array())
        })
        .and_then(|x| x.as_array())?;

    let entries: Vec<ButtonLedEntry> = arr
        .iter()
        .filter_map(|entry| {
            let cc = entry.get("ConstantColor")?;
            Some(ButtonLedEntry {
                r: cc.get("Red").and_then(|x| x.as_u64()).unwrap_or(0) as u8,
                g: cc.get("Green").and_then(|x| x.as_u64()).unwrap_or(0) as u8,
                b: cc.get("Blue").and_then(|x| x.as_u64()).unwrap_or(0) as u8,
                brightness: cc.get("Brigthness").and_then(|x| x.as_u64()).unwrap_or(7) as u8,
            })
        })
        .collect();

    if entries.is_empty() {
        None
    } else {
        Some(entries)
    }
}

// ── Game ID mapping ───────────────────────────────────────────────────────────

fn major_minor_to_game(major: u8, minor: u8) -> &'static str {
    match (major, minor) {
        (5, 0) => "iRacing",
        (0, 0) => "AC",
        (0, 1) => "ACC",
        (0, 2) => "AC EVO",
        (1, 1) => "AMS2",
        (4, 10) => "F1 24",
        (4, 11) => "F1 25",
        (10, 0) => "RaceRoom",
        (11, 1) => "rFactor 2",
        (11, 2) => "LMU",
        _ => "Unknown",
    }
}

// ── Filename-based game/car extraction ───────────────────────────────────────

/// Extracts `(game, car)` from a .pws filename stem.
/// Pattern: `{Game} {CarName} - {FF} I {Torque}Nm`
/// Example: `"iRacing Acura ARX-06 GTP - 54 I 15Nm"` → `("iRacing", "Acura ARX-06 GTP")`
pub fn car_name_from_stem(stem: &str) -> Option<(String, String)> {
    // Find " - " followed by digits and " I "
    let sep = stem.find(" - ")?;
    let prefix = &stem[..sep];
    // game = first whitespace-delimited word
    let space = prefix.find(' ')?;
    let game = prefix[..space].to_string();
    let car = prefix[space + 1..].trim().to_string();
    if car.is_empty() {
        return None;
    }
    Some((game, car))
}

// ── .pws parsers ─────────────────────────────────────────────────────────────

/// Parse a Maurice-format .pws file (`<TuningMenuProfile><JSON>…</JSON>`).
fn parse_maurice_pws(path: &Path, text: &str) -> Result<PwsProfile, String> {
    let tmp_start = text
        .find("<TuningMenuProfile>")
        .ok_or_else(|| format!("{}: missing <TuningMenuProfile>", path.display()))?;
    let json_tag = text[tmp_start..]
        .find("<JSON>")
        .ok_or_else(|| format!("{}: missing <JSON> in TuningMenuProfile", path.display()))?;
    let json_start = tmp_start + json_tag + "<JSON>".len();
    let json_end = text[json_start..]
        .find("</JSON>")
        .ok_or_else(|| format!("{}: missing </JSON>", path.display()))?;
    let json_str = &text[json_start..json_start + json_end];

    let v: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| format!("{}: JSON parse error: {}", path.display(), e))?;

    let get_u8 = |key: &str| -> u8 {
        v.get(key)
            .and_then(|x| x.as_i64())
            .unwrap_or(0)
            .clamp(0, 255) as u8
    };

    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let (game, car) =
        car_name_from_stem(&stem).unwrap_or_else(|| ("unknown".to_string(), stem.clone()));

    Ok(PwsProfile {
        name: stem,
        game,
        car,
        path: path.to_path_buf(),
        base_type: xml_attr_u8(text, "Device", "BaseType"),
        wheel_type: xml_attr_u8(text, "Device", "WheelType"),
        rev_led: parse_rev_led(text),
        button_led: parse_button_led(text),
        recommended: false,
        sen: get_u8("SEN"),
        ff: get_u8("FF"),
        ffs: get_u8("FFS"),
        ndp: get_u8("NDP"),
        nfr: get_u8("NFR"),
        nin: get_u8("NIN"),
        int_: get_u8("INT"),
        fei: get_u8("FEI"),
        for_: get_u8("FOR"),
        spr: get_u8("SPR"),
        dpr: get_u8("DPR"),
        bli: get_u8("BLI"),
        sho: get_u8("SHO"),
        brf: get_u8("BRF"),
        ful: get_u8("FUL"),
        dri: get_u8("DRI"),
        acp: get_u8("APM"),
    })
}

/// Parse a Fanatec App recommended .pws file (`<TuningMenu>` with direct child tags).
/// The `<SEN>` value is in display degrees; all other values are display units.
fn parse_recommended_pws_text(path: &Path, text: &str) -> Result<PwsProfile, String> {
    let get_u32 = |tag: &str| xml_tag_u32(text, tag).unwrap_or(0);
    let get_u8 = |tag: &str| get_u32(tag).clamp(0, 255) as u8;

    // SEN may be display degrees (e.g. 1080) or already a wire byte (≤ 255).
    let sen_raw = get_u32("SEN");
    let sen = if sen_raw > 255 {
        crate::tuning::encode_sen_degrees(sen_raw)
    } else {
        sen_raw as u8
    };

    // DRI is signed: clamp to i8 range, then reinterpret as u8 for the wire.
    let dri = (xml_tag_i32(text, "DRI").unwrap_or(0).clamp(-128, 127) as i8) as u8;

    let major = xml_attr_u8(text, "Game", "MajorId").unwrap_or(255);
    let minor = xml_attr_u8(text, "Game", "MinorId").unwrap_or(255);
    let game = major_minor_to_game(major, minor).to_string();

    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(PwsProfile {
        name: stem,
        game,
        car: String::new(), // game-wide; not matched by car fuzzy logic
        path: path.to_path_buf(),
        base_type: xml_attr_u8(text, "Device", "BaseType"),
        wheel_type: xml_attr_u8(text, "Device", "WheelType"),
        rev_led: None,
        button_led: None,
        recommended: false, // caller (scan_recommended_profiles) sets true
        sen,
        ff: get_u8("FF"),
        ffs: get_u8("FFS"),
        ndp: get_u8("NDP"),
        nfr: get_u8("NFR"),
        nin: get_u8("NIN"),
        int_: get_u8("INT"),
        fei: get_u8("FEI"),
        for_: get_u8("FOR"),
        spr: get_u8("SPR"),
        dpr: get_u8("DPR"),
        bli: get_u8("ABS"), // <ABS> in recommended format = BLI
        sho: get_u8("SHO"),
        brf: get_u8("BRF"),
        ful: get_u8("FUL"),
        dri,
        acp: get_u8("APM"),
    })
}

/// Parse a .pws file, auto-detecting the format:
/// - `<TuningMenuProfile><JSON>` — Maurice's format
/// - `<TuningMenu>` with direct child tags — Fanatec App recommended format
pub fn parse_pws(path: &Path) -> Result<PwsProfile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    if text.contains("<TuningMenuProfile>") {
        parse_maurice_pws(path, &text)
    } else if text.contains("<TuningMenu>") {
        parse_recommended_pws_text(path, &text)
    } else {
        Err(format!("{}: unrecognized .pws format", path.display()))
    }
}

// ── Profile scanners ──────────────────────────────────────────────────────────

/// Walks `base_dir` recursively and parses every `.pws` file found.
/// Files that fail to parse are skipped with a warning to stderr.
pub fn scan_profiles(base_dir: &Path) -> Vec<PwsProfile> {
    let mut profiles = Vec::new();
    walk_dir(base_dir, &mut profiles);
    profiles
}

fn walk_dir(dir: &Path, out: &mut Vec<PwsProfile>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("warning: cannot read {}: {}", dir.display(), e);
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, out);
        } else if path
            .extension()
            .map(|e| e.eq_ignore_ascii_case("pws"))
            .unwrap_or(false)
        {
            match parse_pws(&path) {
                Ok(p) => out.push(p),
                Err(e) => eprintln!("warning: skipping {}: {}", path.display(), e),
            }
        }
    }
}

/// Walk `settings_dir/{MajorId}_{MinorId}/` and collect recommended .pws files.
/// Each profile is marked `recommended = true` and has `car = ""` (game-wide).
pub fn scan_recommended_profiles(settings_dir: &Path) -> Vec<PwsProfile> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(settings_dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for subdir_entry in entries.flatten() {
        let subdir = subdir_entry.path();
        if !subdir.is_dir() {
            continue;
        }
        let pws_entries = match std::fs::read_dir(&subdir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for pws_entry in pws_entries.flatten() {
            let path = pws_entry.path();
            if path
                .extension()
                .map(|e| e.eq_ignore_ascii_case("pws"))
                .unwrap_or(false)
            {
                match parse_pws(&path) {
                    Ok(mut p) => {
                        if p.car.is_empty() {
                            p.recommended = true;
                        }
                        out.push(p);
                    }
                    Err(e) => eprintln!("warning: skipping recommended {}: {}", path.display(), e),
                }
            }
        }
    }
    out
}
