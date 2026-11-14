//! A template (or a project) exported from Revit by the bridge, read into a model through the commands a user would run:
//! its levels, phases, line patterns, fill patterns, text types and the families exported beside it.

use bimcraft_core::model::{Element, Model};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bimcraft-rvt-template-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("families")).unwrap();
    dir
}

fn family(name: &str) -> Value {
    json!({
        "format": "bimcraft-revit-family", "version": 1, "name": name, "category": "OST_Furniture",
        "parameters": [{"name": "Top", "kind": "length"}],
        "types": [{"name": "1400", "values": {"Top": 1400.0}, "triangles": [[[0, 0, 0], [1400, 0, 0], [0, 700, 0]]]}],
    })
}

/// What the bridge writes of an office's template.
fn office() -> Value {
    json!({
        "format": "bimcraft-revit-template",
        "version": 1,
        "name": "Office",
        "levels": [{"name": "Roof", "elevation": 6200.0}, {"name": "Ground", "elevation": 0.0}, {"name": "First", "elevation": 3100.0}],
        "phases": ["Survey", "Demolition", "Fit Out"],
        "line_patterns": [
            {"name": "Office Dash", "segments": [{"dash": 6.0}, {"space": 3.0}]},
            {"name": "Office Chain", "segments": [{"dash": 12.0}, {"space": 2.0}, "dot", {"space": 2.0}]},
        ],
        "fill_patterns": [
            {"name": "Office Hatch", "target": "drafting", "lines": [{"angle": 45.0, "spacing": 3.0, "origin": [0.0, 0.0], "shift": 0.0}]},
            {"name": "Office Tile", "target": "model", "lines": [
                {"angle": 0.0, "spacing": 600.0, "origin": [0.0, 0.0], "shift": 0.0},
                {"angle": 90.0, "spacing": 600.0, "origin": [0.0, 0.0], "shift": 0.0, "dashes": [300.0, -300.0]},
            ]},
            {"name": "Office Solid", "target": "drafting", "solid": true, "lines": []},
        ],
        "text_types": [
            {"name": "Office 2mm", "font": "Arial", "size": 2.0, "bold": true, "italic": false, "underline": false,
             "width_factor": 0.9, "color": [200, 30, 30], "border": true, "border_offset": 1.5},
        ],
        "families": ["families/0000_Office_Desk.rfa.json", "families/0001_Office_Chair.rfa.json"],
        "notes": ["Family Stair (in-place) is not exported"],
    })
}

/// The bundle on disk: the template's file and its families'.
fn bundle(dir: &Path, template: &Value) -> PathBuf {
    std::fs::write(dir.join("families/0000_Office_Desk.rfa.json"), family("Office Desk").to_string()).unwrap();
    std::fs::write(dir.join("families/0001_Office_Chair.rfa.json"), family("Office Chair").to_string()).unwrap();
    let path = dir.join("Office.rte.json");
    std::fs::write(&path, template.to_string()).unwrap();
    path
}

fn levels(m: &Model) -> Vec<(String, f64)> {
    let mut all: Vec<(String, f64)> =
        m.elements.values().filter_map(|e| if let Element::Level(l) = e { Some((l.name.clone(), l.elevation)) } else { None }).collect();
    all.sort_by(|a, b| a.1.total_cmp(&b.1));
    all
}

#[test]
fn a_template_from_the_bridge_comes_in_with_its_levels_phases_patterns_text_types_and_families() {
    let dir = dir("whole");
    let read = crate::exchange_template_path(&bundle(&dir, &office())).unwrap();
    let m = &read.model;
    m.check().unwrap();
    assert_eq!(m.name, "Office");
    assert_eq!(read.schema, "Revit template (bridge)");
    // The template's levels are the project's, and no starter level is left beside them.
    assert_eq!(levels(m), [("Ground".to_string(), 0.0), ("First".to_string(), 3100.0), ("Roof".to_string(), 6200.0)]);
    // Its phases, in its order, in place of the two a project starts with.
    assert_eq!(m.phases, ["Survey", "Demolition", "Fit Out"]);
    // Line patterns, each segment as long as it was on paper.
    let chain = serde_json::to_value(&m.line_patterns["Office Chain"]).unwrap();
    assert_eq!(chain, json!([{"dash": 12.0}, {"space": 2.0}, "dot", {"space": 2.0}]));
    assert_eq!(m.line_patterns["Office Dash"].0.len(), 2);
    // Fill patterns: drafting and model, whole lines and dashed, and a solid one.
    let tile = &m.fill_patterns["Office Tile"];
    assert_eq!((serde_json::to_value(tile.target).unwrap(), tile.lines.len()), (json!("model"), 2));
    assert_eq!(serde_json::to_value(&tile.lines[1]).unwrap()["dashes"], json!([300.0, -300.0]));
    assert_eq!(m.fill_patterns["Office Hatch"].lines.len(), 1);
    assert!(m.fill_patterns["Office Solid"].lines.is_empty());
    // A text type with its font, size and style.
    let text = &m.text_types["Office 2mm"];
    assert_eq!((text.font.as_str(), text.size, text.bold, text.border), ("Arial", 2.0, true, true));
    assert_eq!((text.width_factor, text.color, text.border_offset), (0.9, [200, 30, 30], 1.5));
    // The families exported beside it, loaded as Insert > Revit Family loads one.
    assert_eq!(m.families["Office Desk"].types["1400"]["Top"], 1400.0);
    assert_eq!(m.families["Office Chair"].forms.len(), 1);
    // The notes: what came in, what the bridge said, and what a template holds that this does not bring yet.
    assert_eq!(
        read.skipped[0],
        "Came in from the bridge's export: 3 levels, 3 phases, 2 line patterns, 3 fill patterns, 1 text type and 2 families"
    );
    let all = read.skipped.join("\n");
    assert!(all.contains("Family Stair (in-place) is not exported"), "{all}");
    for missing in ["view templates", "filters", "object styles", "dimension types", "parameters", "sheets", "title blocks"] {
        assert!(all.contains(missing), "the notes name {missing}: {all}");
    }
    // Saved as a template and started from, it is a project like any other.
    let saved = dir.join("Office.bimt");
    let mut s = bimcraft_core::engine::Session::new();
    s.reset(read.model.clone(), None);
    s.execute("save", &json!({"path": saved})).unwrap();
    let mut n = bimcraft_core::engine::Session::new();
    n.execute("new", &json!({"template": saved, "name": "Tower"})).unwrap();
    assert_eq!(n.model.phases, ["Survey", "Demolition", "Fit Out"]);
    assert!(n.model.families.contains_key("Office Desk"));
    n.execute("component", &json!({"family": "Office Desk", "type": "1400", "at": [0, 0]})).unwrap();
    n.model.check().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_template_of_one_level_or_none_and_names_the_project_starts_with_come_in_right() {
    let dir = dir("few");
    // One level and one phase: the starters they do not replace go.
    let mut one = office();
    one["levels"] = json!([{"name": "Level 2", "elevation": 500.0}]);
    one["phases"] = json!(["New Construction"]);
    let read = crate::exchange_template_path(&bundle(&dir, &one)).unwrap();
    assert_eq!(levels(&read.model), [("Level 2".to_string(), 500.0)]);
    assert_eq!(read.model.phases, ["New Construction"]);
    read.model.check().unwrap();
    // Names a new project has already, in another order, and none at all: what a project starts with stays.
    let mut swapped = office();
    swapped["levels"] = json!([{"name": "Level 2", "elevation": 0.0}, {"name": "Level 1", "elevation": 2800.0}]);
    swapped["phases"] = json!(["New Construction", "Existing"]);
    let read = crate::exchange_template_path(&bundle(&dir, &swapped)).unwrap();
    assert_eq!(levels(&read.model), [("Level 2".to_string(), 0.0), ("Level 1".to_string(), 2800.0)]);
    assert_eq!(read.model.phases, ["New Construction", "Existing"]);
    let mut bare = office();
    for key in ["levels", "phases", "line_patterns", "fill_patterns", "text_types", "families"] {
        bare.as_object_mut().unwrap().remove(key);
    }
    let read = crate::exchange_template_path(&bundle(&dir, &bare)).unwrap();
    assert_eq!(levels(&read.model).len(), 2);
    assert_eq!(read.model.phases.len(), 2);
    assert!(read.skipped[0].contains("0 levels") && read.skipped[0].contains("0 families"), "{}", read.skipped[0]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn what_a_command_refuses_is_left_out_by_name_with_its_reason_and_the_rest_comes_in() {
    let dir = dir("refused");
    let mut v = office();
    v["line_patterns"].as_array_mut().unwrap().extend([
        json!({"name": "Too Long", "segments": [{"dash": 5000.0}, {"space": 1.0}]}),
        json!({"name": "No Segments", "segments": []}),
        json!({"name": "", "segments": [{"dash": 1.0}, {"space": 1.0}]}),
        json!("not a pattern"),
    ]);
    v["fill_patterns"]
        .as_array_mut()
        .unwrap()
        .push(json!({"name": "No Gap", "target": "drafting", "lines": [{"angle": 0.0, "spacing": 0.0}]}));
    v["text_types"].as_array_mut().unwrap().push(json!({"name": "Huge", "font": "Arial", "size": 1.0e9}));
    v["levels"]
        .as_array_mut()
        .unwrap()
        .extend([json!({"name": "Sky", "elevation": 1.0e30}), json!({"name": "First", "elevation": 9000.0})]);
    v["phases"].as_array_mut().unwrap().extend([json!("Survey"), json!(7)]);
    v["families"] = json!(["families/0000_Office_Desk.rfa.json", "families/missing.rfa.json", "../outside.rfa.json", "C:/abs.rfa.json", 5]);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let m = &read.model;
    m.check().unwrap();
    let all = read.skipped.join("\n");
    for (count, what, name) in [
        (1, "line pattern", "Too Long"),
        (1, "line pattern", "No Segments"),
        (1, "fill pattern", "No Gap"),
        (1, "text type", "Huge"),
        (1, "level", "Sky"),
        (1, "family", "missing.rfa.json"),
    ] {
        let note = read.skipped.iter().find(|n| n.contains(&format!("`{name}`"))).unwrap_or_else(|| panic!("{name}: {all}"));
        assert!(note.starts_with(&format!("{count} {what} left out, ")), "{note}");
    }
    assert!(!m.line_patterns.contains_key("Too Long") && m.line_patterns.contains_key("Office Dash"));
    assert!(!m.fill_patterns.contains_key("No Gap") && m.fill_patterns.contains_key("Office Tile"));
    assert!(!m.text_types.contains_key("Huge") && m.text_types.contains_key("Office 2mm"));
    // A path that leaves the export's folder is never read.
    assert!(read.skipped.iter().any(|n| n.contains("left out") && n.contains("outside.rfa.json") && n.contains("folder")), "{all}");
    assert!(read.skipped.iter().any(|n| n.contains("abs.rfa.json") && n.contains("folder")), "{all}");
    assert!(m.families.contains_key("Office Desk"));
    assert_eq!(levels(m).len(), 3, "the level named twice and the one out of range are left out");
    assert_eq!(m.phases, ["Survey", "Demolition", "Fit Out"]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_file_that_is_not_the_bridges_template_or_is_of_a_newer_version_says_so() {
    let dir = dir("not");
    let write = |text: &str| {
        let path = dir.join("x.rte.json");
        std::fs::write(&path, text).unwrap();
        path
    };
    let e = crate::exchange_template_path(&write("not json")).err().unwrap();
    assert!(e.contains("not a template exported by tools/revit_bridge"), "{e}");
    let e = crate::exchange_template_path(&write(&family("Desk").to_string())).err().unwrap();
    assert!(e.contains("bimcraft-revit-template"), "{e}");
    let mut newer = office();
    newer["version"] = json!(9);
    let e = crate::exchange_template_path(&write(&newer.to_string())).err().unwrap();
    assert!(e.contains("version 9") && e.contains("newer"), "{e}");
    assert!(crate::exchange_template_path(&dir.join("nowhere.rte.json")).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_damaged_template_never_panics_and_what_comes_back_passes_the_models_rules() {
    let dir = dir("sweep");
    let path = bundle(&dir, &office());
    let good = office().to_string().into_bytes();
    let mut seed = 0x0bad_5eed_1234_5678u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut came_back = 0;
    for _ in 0..400 {
        let mut bad = good.clone();
        for _ in 0..1 + next() % 3 {
            let at = (next() % bad.len() as u64) as usize;
            bad[at] = match next() % 6 {
                0 => b'9',
                1 => b'e',
                2 => b'-',
                3 => b'0',
                4 => b' ',
                _ => b'a' + (next() % 26) as u8,
            };
        }
        std::fs::write(&path, &bad).unwrap();
        if let Ok(read) = crate::exchange_template_path(&path) {
            came_back += 1;
            read.model.check().unwrap();
        }
    }
    assert!(came_back > 50, "the sweep reaches the reader: {came_back}");
    let _ = std::fs::remove_dir_all(dir);
}
