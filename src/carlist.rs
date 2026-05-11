use std::collections::HashMap;
use std::path::Path;

/// carPath → display name  (from CarsList_*.xml)
pub type CarNames = HashMap<String, String>;
/// carPath → .pws filename  (from ProfileCarsList_*.xml)
pub type ProfileMap = HashMap<String, String>;

/// Parse `CarsList_*.xml` — returns carPath → display name.
pub fn parse_car_names(path: &Path) -> CarNames {
    let mut map = CarNames::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            if let Some((tag, car_name, _)) = parse_tag(line) {
                map.insert(tag, car_name);
            }
        }
    }
    map
}

/// Parse `ProfileCarsList_*.xml` — returns carPath → .pws filename.
pub fn parse_profile_map(path: &Path) -> ProfileMap {
    let mut map = ProfileMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            if let Some((tag, _, Some(profile))) = parse_tag(line) {
                map.insert(tag, profile);
            }
        }
    }
    map
}

/// Load both XML files for a game from `xml_dir`.
/// `suffix` matches the filename suffix: "iRacing", "AC", "ACC", etc.
pub fn load_for_game(xml_dir: &Path, suffix: &str) -> (CarNames, ProfileMap) {
    let cars = parse_car_names(&xml_dir.join(format!("CarsList_{}.xml", suffix)));
    let profiles = parse_profile_map(&xml_dir.join(format!("ProfileCarsList_{}.xml", suffix)));
    (cars, profiles)
}

/// Parse one line of a Fanatec car-list XML.
///
/// Both formats use self-closing tags:
///   CarsList:        <tagname CarName="display name"/>
///   ProfileCarsList: <tagname Profile="filename.pws" CarName="display name" />
///
/// Returns `(tag, car_name, profile_filename)`.
fn parse_tag(line: &str) -> Option<(String, String, Option<String>)> {
    let lt = line.find('<')? + 1;
    let rest = &line[lt..];
    // Skip XML declaration, root elements, closing tags — they all start with
    // non-lowercase or '?' or '/'.
    if !rest.starts_with(|c: char| c.is_ascii_lowercase()) {
        return None;
    }
    let end = rest.find([' ', '/', '>'])?;
    let tag = rest[..end].to_string();
    if tag.is_empty() {
        return None;
    }
    let car_name = attr(line, "CarName")?;
    let profile = attr(line, "Profile");
    Some((tag, car_name, profile))
}

fn attr(line: &str, name: &str) -> Option<String> {
    let needle = format!("{}=\"", name);
    let start = line.find(&needle)? + needle.len();
    let end = line[start..].find('"')?;
    Some(line[start..start + end].to_string())
}
