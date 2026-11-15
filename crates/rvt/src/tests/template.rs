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
        "Came in from the bridge's export: 3 levels, 3 phases, 2 line patterns, 0 line styles, 3 fill patterns, 1 text type, \
         0 dimension types, 0 object styles, 0 parameters, 0 view filters, 0 view templates, 0 title blocks, 0 sheets and 2 families"
    );
    let all = read.skipped.join("\n");
    assert!(all.contains("Family Stair (in-place) is not exported"), "{all}");
    for missing in ["line weights", "tick mark", "labels", "overrides"] {
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

/// The office's template with its line styles, object styles, dimension types and project parameters too.
fn styled() -> Value {
    let mut v = office();
    v["line_styles"] = json!([
        {"name": "Office Demolished", "pen": 3, "color": [255, 0, 0], "pattern": "Office Dash"},
        {"name": "Office Axis", "pen": 1, "color": [0, 0, 0], "pattern": null},
    ]);
    v["object_styles"] = json!([
        {"category": "OST_Walls", "projection": 2, "cut": 5},
        {"category": "OST_Doors", "projection": 1, "cut": 3},
        {"category": "OST_PipeCurves", "projection": 6, "cut": null},
        {"category": "OST_Views", "projection": 1, "cut": 1},
        {"category": "OST_Grids", "projection": 1, "cut": null},
    ]);
    v["dimension_types"] = json!([
        {"name": "Office 2mm", "font": "Arial", "text_size": 2.0, "bold": false, "italic": true, "underline": false,
         "width_factor": 0.8, "color": [0, 0, 200], "line_pen": 2, "text_offset": 1.2, "witness_gap": 1.0,
         "witness_extension": 2.0, "line_extension": 0.5},
    ]);
    v["parameters"] = json!([
        {"name": "Fire Rating", "kind": "text", "guid": "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10", "scope": "type",
         "group": "identity-data", "categories": ["OST_Walls", "OST_Doors"]},
        {"name": "Clear Height", "kind": "length", "scope": "instance", "categories": ["OST_Rooms", "OST_Views"]},
        {"name": "Sheet Issue", "kind": "text", "scope": "instance", "categories": ["OST_Sheets", "OST_Views"]},
    ]);
    v
}

#[test]
fn line_styles_object_styles_dimension_types_and_project_parameters_come_in() {
    let dir = dir("styled");
    let read = crate::exchange_template_path(&bundle(&dir, &styled())).unwrap();
    let m = &read.model;
    m.check().unwrap();
    // A line style: its pen, its colour and its pattern, which came in before it.
    let demolished = serde_json::to_value(&m.line_styles["Office Demolished"]).unwrap();
    assert_eq!((demolished["pen"].clone(), demolished["pattern"].clone()), (json!(3), json!("Office Dash")));
    assert_eq!(demolished["color"], json!([255, 0, 0]));
    assert!(m.line_styles.contains_key("Office Axis"));
    // Object styles, each Revit category on the category BIMCraft has for it; one given no cut pen keeps its own.
    let mut s = bimcraft_core::engine::Session::new();
    s.reset(m.clone(), None);
    let styles = s.execute("object_styles", &json!({})).unwrap();
    let of = |category: &str| {
        let row = styles["categories"].as_array().unwrap().iter().find(|c| c["category"] == category).unwrap().clone();
        (row["projection"].as_u64().unwrap(), row["cut"].as_u64().unwrap())
    };
    assert_eq!((of("wall"), of("door")), ((2, 5), (1, 3)));
    assert_eq!(of("pipe").0, 6);
    assert_eq!(of("floor"), (4, 10), "a category the template says nothing of keeps what a project starts with");
    // A dimension type's text and lines.
    let dims = s.execute("dimension_type", &json!({})).unwrap().to_string();
    assert!(dims.contains("Office 2mm"), "{dims}");
    let office = serde_json::to_value(&m.dimension_types["Office 2mm"]).unwrap();
    assert_eq!((office["text_size"].clone(), office["italic"].clone(), office["line_pen"].clone()), (json!(2.0), json!(true), json!(2)));
    assert_eq!((office["width_factor"].clone(), office["witness_extension"].clone()), (json!(0.8), json!(2.0)));
    // Project parameters: a shared one keeps its GUID, each is its kind and scope on the categories BIMCraft has.
    let fire = serde_json::to_value(&m.parameters["Fire Rating"]).unwrap();
    assert_eq!((fire["kind"].clone(), fire["scope"].clone()), (json!("text"), json!("type")));
    assert_eq!(
        m.library.guid(&bimcraft_core::library::Kind::Parameter, "Fire Rating").map(|g| g.to_string()).as_deref(),
        Some("0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10")
    );
    let listed = s.execute("parameter_list", &json!({})).unwrap().to_string();
    assert!(listed.contains("Fire Rating") && listed.contains("Clear Height"), "{listed}");
    assert!(!m.parameters.contains_key("Sheet Issue"), "none of its categories is one a parameter here is for");
    // The notes: what came in, and what had nowhere to go.
    assert_eq!(
        read.skipped[0],
        "Came in from the bridge's export: 3 levels, 3 phases, 2 line patterns, 2 line styles, 3 fill patterns, 1 text type, \
         1 dimension type, 3 object styles, 2 parameters, 0 view filters, 0 view templates, 0 title blocks, 0 sheets and 2 families"
    );
    let all = read.skipped.join("\n");
    assert!(read.skipped.iter().any(|n| n.starts_with("1 parameter left out, ") && n.contains("`Sheet Issue`")), "{all}");
    assert!(read.skipped.iter().any(|n| n.starts_with("2 object styles left out, ") && n.contains("`OST_Views`")), "{all}");
    assert!(all.contains("Clear Height") && all.contains("OST_Views"), "a category dropped from a parameter is said: {all}");
    for done in ["object styles", "dimension types", "project and shared parameters"] {
        assert!(!read.skipped.last().unwrap().contains(done), "{done} is exported now: {}", read.skipped.last().unwrap());
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn styles_and_parameters_a_command_refuses_are_left_out_by_name() {
    let dir = dir("styled-refused");
    let mut v = styled();
    v["line_styles"].as_array_mut().unwrap().extend([
        json!({"name": "Odd Pen", "pen": 99, "color": [0, 0, 0]}),
        json!({"name": "Lost Pattern", "pen": 2, "color": [0, 0, 0], "pattern": "Nowhere"}),
        json!({"name": 7}),
    ]);
    v["object_styles"].as_array_mut().unwrap().extend([json!({"category": "OST_Floors", "projection": 40, "cut": 2}), json!("x")]);
    v["dimension_types"].as_array_mut().unwrap().push(json!({"name": "Huge", "text_size": 1.0e9}));
    v["parameters"].as_array_mut().unwrap().extend([
        json!({"name": "Fire Rating", "kind": "length", "scope": "instance", "categories": ["OST_Walls"]}),
        json!({"name": "", "kind": "text", "categories": ["OST_Walls"]}),
        json!({"name": "Odd\nName, \"quoted\"", "kind": "text", "guid": "not-a-guid", "categories": ["OST_Walls"]}),
        json!(5),
    ]);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let m = &read.model;
    m.check().unwrap();
    let all = read.skipped.join("\n");
    assert!(!m.line_styles.contains_key("Odd Pen"), "{all}");
    assert!(read.skipped.iter().any(|n| n.starts_with("1 line style left out, ") && n.contains("`Odd Pen`")), "{all}");
    // A style whose pattern did not come in is drawn solid, and said.
    assert!(m.line_styles.contains_key("Lost Pattern"));
    assert!(all.contains("Lost Pattern") && all.contains("Nowhere"), "{all}");
    assert!(read.skipped.iter().any(|n| n.contains("left out") && n.contains("`OST_Floors`")), "{all}");
    assert!(!m.dimension_types.contains_key("Huge") && m.dimension_types.contains_key("Office 2mm"));
    // A second parameter of a name is kept apart, as the import of a table keeps it, and the first is as it was.
    assert_eq!(serde_json::to_value(&m.parameters["Fire Rating"]).unwrap()["kind"], "text");
    assert!(m.parameters.len() >= 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_project_parameter_is_for_every_mep_category_revit_has_as_the_category_bimcraft_has() {
    let dir = dir("mep");
    // Runs are their own categories; everything a family makes (fittings, accessories, equipment, devices) is a component.
    let runs = [
        ("OST_PipeCurves", "pipe"),
        ("OST_FlexPipeCurves", "pipe"),
        ("OST_PlaceHolderPipes", "pipe"),
        ("OST_DuctCurves", "duct"),
        ("OST_FlexDuctCurves", "duct"),
        ("OST_PlaceHolderDucts", "duct"),
        ("OST_Conduit", "conduit"),
        ("OST_CableTray", "cabletray"),
        ("OST_Wire", "wire"),
    ];
    let made = [
        "OST_PipeFitting",
        "OST_PipeAccessory",
        "OST_PlumbingFixtures",
        "OST_PlumbingEquipment",
        "OST_Sprinklers",
        "OST_MechanicalEquipment",
        "OST_MechanicalControlDevices",
        "OST_DuctFitting",
        "OST_DuctAccessory",
        "OST_DuctTerminal",
        "OST_ElectricalEquipment",
        "OST_ElectricalFixtures",
        "OST_LightingFixtures",
        "OST_LightingDevices",
        "OST_ConduitFitting",
        "OST_CableTrayFitting",
        "OST_DataDevices",
        "OST_TelephoneDevices",
        "OST_CommunicationDevices",
        "OST_FireAlarmDevices",
        "OST_SecurityDevices",
        "OST_NurseCallDevices",
        "OST_AudioVisualDevices",
        "OST_FireProtection",
        "OST_SpecialityEquipment",
        "OST_GenericModel",
    ];
    let mut v = office();
    let mut parameters: Vec<Value> =
        runs.iter().map(|(ost, _)| json!({"name": format!("On {ost}"), "kind": "text", "categories": [ost]})).collect();
    parameters.extend(made.iter().map(|ost| json!({"name": format!("On {ost}"), "kind": "text", "categories": [ost]})));
    // What BIMCraft's parameters have no category for: said, never guessed.
    parameters.push(json!({"name": "On systems", "kind": "text", "categories": ["OST_PipingSystem", "OST_DuctSystem", "OST_ElectricalCircuit", "OST_PipeInsulations"]}));
    v["parameters"] = json!(parameters);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let m = &read.model;
    for (ost, ours) in runs {
        let p = serde_json::to_value(&m.parameters[&format!("On {ost}")]).unwrap();
        assert_eq!(p["categories"], json!([ours]), "{ost}");
    }
    for ost in made {
        let p = serde_json::to_value(&m.parameters[&format!("On {ost}")]).unwrap();
        assert_eq!(p["categories"], json!(["component"]), "{ost}");
    }
    assert!(!m.parameters.contains_key("On systems"));
    let all = read.skipped.join("\n");
    assert!(read.skipped.iter().any(|n| n.starts_with("1 parameter left out, ") && n.contains("`On systems`")), "{all}");
    let _ = std::fs::remove_dir_all(dir);
}

/// The office's template with its view filters, view templates, title block and sheets.
fn drawn() -> Value {
    let mut v = styled();
    v["filters"] = json!([
        {"name": "Fire Walls", "categories": ["OST_Walls", "OST_Doors"], "logic": "and",
         "rules": [{"field": "Fire Rating", "op": "contains", "value": "EI"}]},
        {"name": "All Pipes", "categories": ["OST_PipeCurves", "OST_FlexPipeCurves", "OST_PipeFitting"], "logic": "and", "rules": []},
    ]);
    v["view_templates"] = json!([
        {"name": "Office Plan", "kind": "plan", "scale": 50, "detail": "fine", "visual_style": "hidden_line",
         "hide": ["OST_Furniture", "OST_Grids", "OST_DuctFitting", "OST_Furniture"],
         "filters": [{"filter": "Fire Walls", "hidden": false}, {"filter": "All Pipes", "hidden": true}, {"filter": "Gone", "hidden": true}],
         "phase": "Fit Out", "controls": ["categories", "filters", "scale", "detail", "nonsense"]},
        {"name": "Office 3D", "kind": "3d", "detail": "medium", "visual_style": "shaded", "hide": [], "filters": []},
    ]);
    v["title_blocks"] = json!([
        {"name": "Office A1", "width": 180.0, "height": 60.0,
         "lines": [{"points": [[0.0, 0.0], [180.0, 0.0], [180.0, 60.0], [0.0, 60.0]], "closed": true}, {"points": [[0.0, 30.0], [180.0, 30.0]]}],
         "texts": [{"at": [5.0, 40.0], "text": "OFFICE", "size": 5.0}, {"at": [5.0, 10.0], "text": "Drawn by", "size": 2.0, "align": "left"}]},
    ]);
    v["sheets"] = json!([
        {"number": "A-101", "name": "Ground Floor", "width": 841.0, "height": 594.0, "title_block": "Office A1"},
        {"number": "A-102", "name": "First Floor", "width": 841.0, "height": 594.0, "title_block": "Unknown Block"},
    ]);
    v
}

#[test]
fn view_filters_view_templates_title_blocks_and_sheets_come_in() {
    let dir = dir("drawn");
    let read = crate::exchange_template_path(&bundle(&dir, &drawn())).unwrap();
    let m = &read.model;
    m.check().unwrap();
    let all = read.skipped.join("\n");
    // A filter: its categories as BIMCraft's, once each, and its rule on the project parameter that came in before it.
    let fire = serde_json::to_value(&m.view_filters["Fire Walls"]).unwrap();
    assert_eq!(fire["categories"], json!(["door", "wall"]), "{fire}");
    assert_eq!(fire["rules"][0]["field"], "Fire Rating");
    assert_eq!(serde_json::to_value(&m.view_filters["All Pipes"]).unwrap()["categories"], json!(["component", "pipe"]));
    // A view template: its kind, scale, detail and style, what it hides, the filters it applies, and what it controls.
    let mut s = bimcraft_core::engine::Session::new();
    s.reset(m.clone(), None);
    let plan = s.execute("template", &json!({"name": "Office Plan"})).unwrap().to_string();
    for part in ["\"plan\"", "furniture", "grid", "Fire Walls", "All Pipes", "Fit Out"] {
        assert!(plan.contains(part), "{part}: {plan}");
    }
    assert!(!plan.contains("Gone"), "a filter that did not come in is not applied: {plan}");
    assert!(m.view_templates.contains_key("Office 3D"));
    assert!(all.contains("Office Plan") && all.contains("OST_DuctFitting"), "a hidden category with none here is said: {all}");
    // A title block: its box, lines and texts; and the sheets, of their size, drawn with it where it came in.
    let block = &m.title_blocks["Office A1"];
    assert_eq!(serde_json::to_value(block).unwrap()["texts"][0]["text"], "OFFICE");
    let sheets: Vec<(String, String, String, String)> = m
        .elements
        .values()
        .filter_map(|e| {
            if let Element::Sheet(s) = e { Some((s.number.clone(), s.name.clone(), s.size.clone(), s.title_block.clone())) } else { None }
        })
        .collect();
    assert!(sheets.contains(&("A-101".into(), "Ground Floor".into(), "841 X 594".into(), "Office A1".into())), "{sheets:?}");
    let second = sheets.iter().find(|s| s.0 == "A-102").unwrap_or_else(|| panic!("{sheets:?}"));
    assert_eq!(second.3, "", "a title block that did not come in leaves the sheet the built-in one");
    assert!(all.contains("A-102") && all.contains("Unknown Block"), "{all}");
    assert!(read.skipped[0].contains("2 view filters, 2 view templates, 1 title block, 2 sheets and 2 families"), "{}", read.skipped[0]);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn filters_and_templates_bimcraft_cannot_say_are_left_out_with_why() {
    let dir = dir("drawn-refused");
    let mut v = drawn();
    v["filters"].as_array_mut().unwrap().extend([
        json!({"name": "Either", "categories": ["OST_Walls"], "logic": "or", "rules": [{"field": "Fire Rating", "op": "equals", "value": "EI60"}]}),
        json!({"name": "In Feet", "categories": ["OST_Walls"], "logic": "and", "rules": [{"field": "Length", "op": "greater", "value": 3.0, "internal": true}]}),
        json!({"name": "No Field", "categories": ["OST_Walls"], "logic": "and", "rules": [{"field": "Nowhere", "op": "equals", "value": "x"}]}),
        json!({"name": "Only Views", "categories": ["OST_Views"], "logic": "and", "rules": []}),
        json!(3),
    ]);
    v["view_templates"]
        .as_array_mut()
        .unwrap()
        .extend([json!({"name": "Office Schedule", "kind": "schedule"}), json!({"name": "Odd Scale", "kind": "plan", "scale": -4})]);
    v["title_blocks"].as_array_mut().unwrap().push(json!({"name": "Huge", "width": 1.0e9, "height": 1.0}));
    v["sheets"].as_array_mut().unwrap().push(json!({"number": "A-101", "name": "Twice", "width": 841.0, "height": 594.0}));
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let m = &read.model;
    m.check().unwrap();
    let all = read.skipped.join("\n");
    for (name, why) in [("Either", "OR"), ("In Feet", "internal units"), ("No Field", "Nowhere"), ("Only Views", "categories")] {
        assert!(!m.view_filters.contains_key(name), "{name}");
        assert!(
            read.skipped
                .iter()
                .any(|n| n.contains("view filter") && n.contains("left out") && n.contains(&format!("`{name}`")) && n.contains(why)),
            "{name}: {all}"
        );
    }
    assert!(m.view_filters.contains_key("Fire Walls"));
    assert!(!m.view_templates.contains_key("Office Schedule") && !m.view_templates.contains_key("Odd Scale"));
    assert!(read.skipped.iter().any(|n| n.contains("view template") && n.contains("`Office Schedule`") && n.contains("schedule")), "{all}");
    assert!(m.view_templates.contains_key("Office Plan"));
    assert!(!m.title_blocks.contains_key("Huge") && m.title_blocks.contains_key("Office A1"));
    assert_eq!(m.elements.values().filter(|e| matches!(e, Element::Sheet(_))).count(), 2, "the sheet numbered twice is left out");
    let _ = std::fs::remove_dir_all(dir);
}

/// What the first run inside Revit showed: a filter of a hydraulic template is by the pipes' system type, which Revit
/// shows by a localized name and applies to pipes and to what families make alike.
#[test]
fn a_filter_by_system_type_comes_in_by_the_parameters_public_name_for_the_runs_that_have_a_system() {
    let dir = dir("system-filter");
    let mut v = drawn();
    v["filters"] = json!([
        {"name": "ESGOTO COMUM", "categories": ["OST_PlaceHolderPipes", "OST_PipeFitting", "OST_PipeCurves", "OST_PlumbingFixtures"],
         "logic": "and", "rules": [{"field": "Tipo de sistema", "builtin": "RBS_PIPING_SYSTEM_TYPE_PARAM", "op": "equals", "value": "ESGOTO COMUM"}]},
        {"name": "Pipes 20", "categories": ["OST_PipeCurves"], "logic": "and",
         "rules": [{"field": "Diâmetro", "builtin": "RBS_PIPE_DIAMETER_PARAM", "op": "equals", "value": 0.06561679790026247, "internal": true},
                   {"field": "Comprimento", "builtin": "CURVE_ELEM_LENGTH", "op": "greater", "value": 1.0, "internal": true}]},
        {"name": "Odd", "categories": ["OST_PipeCurves"], "logic": "and",
         "rules": [{"field": "Coisa", "builtin": "SOMETHING_ELSE", "op": "equals", "value": "x"}]},
    ]);
    v["view_templates"] = json!([]);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let m = &read.model;
    m.check().unwrap();
    let all = read.skipped.join("\n");
    let filter = serde_json::to_value(&m.view_filters["ESGOTO COMUM"]).unwrap();
    assert_eq!(filter["categories"], json!(["pipe"]), "{all}");
    assert_eq!(filter["rules"][0]["field"], json!("System"), "{filter}");
    assert_eq!(filter["rules"][0]["value"], json!("ESGOTO COMUM"));
    assert!(
        read.skipped.iter().any(|n| n.contains("ESGOTO COMUM") && n.contains("component") && n.contains("System")),
        "the components it no longer picks are said: {all}"
    );
    let sized = serde_json::to_value(&m.view_filters["Pipes 20"]).unwrap();
    assert_eq!(sized["rules"][0]["field"], json!("Diameter"), "{all}");
    assert_eq!(sized["rules"][0]["value"], json!(20.0), "feet are millimetres here");
    assert_eq!(sized["rules"][1], json!({"field": "Length", "op": "greater", "value": 304.8}), "{sized}");
    assert!(!m.view_filters.contains_key("Odd"), "a built-in parameter BIMCraft has no field for is still the command's refusal");
    let _ = std::fs::remove_dir_all(dir);
}

/// A note of several lines in a title block (Revit breaks them with a carriage return) comes in as a text for each line,
/// one under the other, since a title block's text here is one line.
#[test]
fn a_title_blocks_note_of_several_lines_comes_in_as_a_text_for_each_line() {
    let dir = dir("title-lines");
    let mut v = drawn();
    v["sheets"] = json!([]);
    v["title_blocks"] = json!([{"name": "Notes A3", "width": 420.0, "height": 297.0, "lines": [],
        "texts": [{"at": [10.0, 200.0], "text": "GENERAL NOTES\rOpen the model\r\rSet the coordinates", "size": 5.0, "align": "left"}]}]);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let all = read.skipped.join("\n");
    let block = serde_json::to_value(&read.model.title_blocks["Notes A3"]).unwrap_or_else(|_| panic!("{all}"));
    let texts = block["texts"].as_array().unwrap();
    let said: Vec<&str> = texts.iter().map(|t| t["text"].as_str().unwrap()).collect();
    assert_eq!(said, ["GENERAL NOTES", "Open the model", "Set the coordinates"], "{all}");
    let ys: Vec<f64> = texts.iter().map(|t| t["at"][1].as_f64().unwrap()).collect();
    assert_eq!(ys, [200.0, 192.5, 177.5], "each line a line and a half of its size lower, a blank one keeping its place");
    let _ = std::fs::remove_dir_all(dir);
}

/// A hydraulic template applies some fifty filters to a view, more than a template here holds.
#[test]
fn a_view_template_of_more_filters_than_one_holds_keeps_the_ones_that_hide_first() {
    let dir = dir("many-filters");
    let mut v = drawn();
    let names: Vec<String> = (0..40).map(|i| format!("Filter {i}")).collect();
    v["filters"] = names.iter().map(|n| json!({"name": n, "categories": ["OST_Walls"], "logic": "and", "rules": []})).collect();
    let applied: Vec<Value> = names.iter().enumerate().map(|(i, n)| json!({"filter": n, "hidden": i >= 36})).collect();
    v["view_templates"] = json!([{"name": "Busy Plan", "kind": "plan", "hide": [], "filters": applied}]);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let all = read.skipped.join(
        "
",
    );
    read.model.check().unwrap();
    assert!(read.model.view_templates.contains_key("Busy Plan"), "{all}");
    let mut s = bimcraft_core::engine::Session::new();
    s.reset(read.model.clone(), None);
    let template = s.execute("template", &json!({"name": "Busy Plan"})).unwrap().to_string();
    let kept = names.iter().filter(|n| template.contains(&format!("\"{n}\""))).count();
    assert_eq!(kept, 32, "{template}");
    for hiding in &names[36..] {
        assert!(template.contains(&format!("\"{hiding}\"")), "every filter that hides is kept: {hiding} in {template}");
    }
    assert!(read.skipped.iter().any(|n| n.contains("Busy Plan") && n.contains("8 filters more")), "{all}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A project that imported drawings holds hundreds of line patterns with no segments: one note says them all.
#[test]
fn entries_left_out_for_one_reason_are_one_note() {
    let dir = dir("one-note");
    let mut v = office();
    v["line_patterns"] =
        json!([{"name": "IMPORT-A", "segments": []}, {"name": "IMPORT-B", "segments": []}, {"name": "IMPORT-C", "segments": []}]);
    let read = crate::exchange_template_path(&bundle(&dir, &v)).unwrap();
    let about: Vec<&String> = read.skipped.iter().filter(|n| n.contains("line pattern")).filter(|n| n.contains("left out")).collect();
    assert_eq!(about.len(), 1, "{about:?}");
    assert!(about[0].starts_with("3 line patterns left out, ") && about[0].contains("`IMPORT-A`, `IMPORT-B`, `IMPORT-C`"), "{about:?}");
    let _ = std::fs::remove_dir_all(dir);
}

/// What the whole hydraulic template showed: Revit lets two shared parameters have one name (a maker's `Finish` and the
/// office's), and a family share one named as a project parameter is; here one name is one shared parameter. The family
/// still comes in: the parameter keeps its values, untied from its definition or under another name, and the note says so.
#[test]
fn a_family_whose_shared_parameter_clashes_by_name_comes_in_with_it_untied_or_renamed() {
    let dir = dir("shared-clash");
    let mut v = office();
    v["parameters"] = json!([{"name": "Rating", "kind": "text", "guid": "7b1d2c3e-4f50-4a61-8b72-9c83ad94be05", "scope": "instance",
                              "categories": ["OST_Furniture"]}]);
    let path = bundle(&dir, &v);
    let with = |name: &str, parameter: Value, value: Value| {
        let mut f = family(name);
        let named = parameter["name"].as_str().unwrap().to_string();
        f["parameters"].as_array_mut().unwrap().push(parameter);
        f["types"][0]["values"][named] = value;
        f
    };
    let finish = |guid: &str| json!({"name": "Finish", "kind": "text", "shared": guid});
    let desk = with("Office Desk", finish("11111111-1111-4111-8111-111111111111"), json!("Oak"));
    let chair = with("Office Chair", finish("22222222-2222-4222-8222-222222222222"), json!("Steel"));
    std::fs::write(dir.join("families/0000_Office_Desk.rfa.json"), desk.to_string()).unwrap();
    std::fs::write(dir.join("families/0001_Office_Chair.rfa.json"), chair.to_string()).unwrap();
    let lamp =
        with("Office Lamp", json!({"name": "Rating", "kind": "text", "shared": "33333333-3333-4333-8333-333333333333"}), json!("IP44"));
    std::fs::write(dir.join("families/0002_Office_Lamp.rfa.json"), lamp.to_string()).unwrap();
    v["families"].as_array_mut().unwrap().push(json!("families/0002_Office_Lamp.rfa.json"));
    std::fs::write(&path, v.to_string()).unwrap();

    let read = crate::exchange_template_path(&path).unwrap();
    let all = read.skipped.join("\n");
    let m = &read.model;
    m.check().unwrap();
    for name in ["Office Desk", "Office Chair", "Office Lamp"] {
        assert!(m.families.contains_key(name), "{name}: {all}");
    }
    let of = |family: &str, name: &str| m.families[family].parameters.iter().find(|p| p.name == name).cloned();
    assert!(of("Office Desk", "Finish").unwrap().shared.is_some(), "the first to share the name keeps it");
    let untied = of("Office Chair", "Finish").unwrap_or_else(|| panic!("{all}"));
    assert!(untied.shared.is_none(), "the second keeps the parameter, untied");
    assert_eq!(serde_json::to_value(&m.families["Office Chair"].types["1400"]).unwrap()["Finish"], json!("Steel"), "{all}");
    assert!(of("Office Lamp", "Rating").is_none(), "{all}");
    let renamed = of("Office Lamp", "Rating 2").unwrap_or_else(|| panic!("{all}"));
    assert!(renamed.shared.is_none(), "renamed, it is untied too: the same definition under two names is refused as well");
    assert_eq!(serde_json::to_value(&m.families["Office Lamp"].types["1400"]).unwrap()["Rating 2"], json!("IP44"), "{all}");
    assert!(read.skipped.iter().any(|n| n.contains("Office Chair") && n.contains("`Finish`") && n.contains("untied")), "{all}");
    assert!(read.skipped.iter().any(|n| n.contains("Office Lamp") && n.contains("`Rating 2`")), "{all}");
    let _ = std::fs::remove_dir_all(dir);
}

/// A formula the family command refuses in the project (it says a parameter the family lost to the most a family holds)
/// leaves the family in, the parameter keeping each type's value.
#[test]
fn a_family_whose_formula_is_refused_in_the_project_comes_in_without_the_formula() {
    let dir = dir("formula-refused");
    let v = office();
    let path = bundle(&dir, &v);
    let mut desk = family("Office Desk");
    desk["parameters"].as_array_mut().unwrap().push(json!({"name": "Half", "kind": "length", "formula": "Top / 2"}));
    desk["types"][0]["values"]["Half"] = json!(700.0);
    let mut read = crate::exchange_family(desk.to_string().as_bytes(), "desk.rfa.json").unwrap();
    // As the reader would hand it over had `Gone` been left behind.
    for p in read.family["parameters"].as_array_mut().unwrap() {
        if p["name"] == "Half" {
            p["formula"] = json!("Gone / 2");
        }
    }
    let said = super::super::template::mend(
        &mut read.family,
        "family: parameter `Half`: formula `Gone / 2`: the following is not a valid parameter: Gone.",
    );
    assert!(said.is_some_and(|n| n.contains("`Half`") && n.contains("Gone / 2")));
    let half = read.family["parameters"].as_array().unwrap().iter().find(|p| p["name"] == "Half").unwrap();
    assert!(half.get("formula").is_none(), "{half}");
    let mut s = bimcraft_core::engine::Session::new();
    s.execute("family", &read.family).unwrap();
    let _ = (std::fs::remove_dir_all(dir), path);
}
