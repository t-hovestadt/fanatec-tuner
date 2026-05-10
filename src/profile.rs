use std::path::{Path, PathBuf};

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
    pub brf: u8, // display 0–100; wire = display / 10
    pub ful: u8, // 0 = OFF
    pub dri: u8,
    pub acp: u8, // JSON key "APM"
}

impl PwsProfile {
    /// Wire byte for FOR/SPR/DPR/SHO/BRF (display ÷ 10).
    pub fn wire_div10(display: u8) -> u8 {
        display / 10
    }
}

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

/// Parses a .pws file and returns its `TuningMenuProfile` JSON as a `PwsProfile`.
///
/// The file is XML with a `<TuningMenuProfile><JSON>…</JSON>` section.
/// We locate that text span directly without an XML crate.
pub fn parse_pws(path: &Path) -> Result<PwsProfile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;

    // Locate <TuningMenuProfile> … <JSON> … </JSON>
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
        acp: get_u8("APM"), // FanaLab JSON key for Analogue Paddles
    })
}

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
