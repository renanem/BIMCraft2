//! A family exported from Revit by the bridge (`tools/revit_bridge`), read into the params of the `family` command: every
//! parameter with its kind, instance or type, shared GUID and formula, each type's values, and a mesh for each type.

use bimcraft_core::engine::Session;
use serde_json::{Value, json};

/// A unit tetrahedron scaled by `size` mm: four triangles.
fn solid(size: f64) -> Value {
    let (o, x, y, z) = ([0.0, 0.0, 0.0], [size, 0.0, 0.0], [0.0, size, 0.0], [0.0, 0.0, size]);
    json!([[o, y, x], [o, x, z], [o, z, y], [x, y, z]])
}

/// What the bridge writes of a bench: two types, type and instance parameters, a formula, a shared one, and geometry.
fn bench() -> Value {
    json!({
        "format": "bimcraft-revit-family",
        "version": 1,
        "name": "Park Bench",
        "category": "OST_Furniture",
        "options": {"work_plane_based": true, "always_vertical": false, "shared": true},
        "parameters": [
            {"name": "Seat Width", "kind": "length", "group": "dimensions"},
            {"name": "Half", "kind": "length", "formula": "Seat Width / 2"},
            {"name": "Lean", "kind": "angle", "instance": true},
            {"name": "Has Back", "kind": "yesno"},
            {"name": "Slats", "kind": "integer"},
            {"name": "Finish", "kind": "text", "shared": "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10", "group": "identity-data"},
            {"name": "Face", "kind": "material"},
            {"name": "Power", "kind": "other", "unit": "W"},
        ],
        "types": [
            {"name": "1200", "values": {"Seat Width": 1200.0, "Half": 600.0, "Lean": 12.5, "Has Back": true, "Slats": 7, "Finish": "Oiled", "Face": "Oak", "Power": 60.0}, "triangles": solid(1200.0)},
            {"name": "1800", "values": {"Seat Width": 1800.0, "Half": 900.0, "Lean": 12.5, "Has Back": false, "Slats": 9, "Finish": "Painted", "Face": "Oak", "Power": 75.0}, "triangles": solid(1800.0)},
        ],
        "notes": ["Family Legs (nested) is not exported"],
    })
}

fn read(v: &Value) -> crate::FamilyFile {
    crate::exchange_family(v.to_string().as_bytes(), "File Name").unwrap()
}

fn parameter<'a>(family: &'a Value, name: &str) -> &'a Value {
    family["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap_or_else(|| panic!("no parameter {name}"))
}

/// Loads what was read through the `family` command, places one component of each type and checks the model.
fn loaded(read: &crate::FamilyFile) -> bimcraft_core::family::Family {
    let mut s = Session::new();
    s.execute("family", &read.family).unwrap_or_else(|e| panic!("the family command refuses what was read: {e}"));
    let family = s.model.families.get(&read.name).cloned().unwrap_or_else(|| panic!("no family {}", read.name));
    for (i, t) in family.types.keys().take(3).enumerate() {
        s.execute("component", &json!({"family": read.name, "type": t, "at": [2000.0 * i as f64, 0.0]}))
            .unwrap_or_else(|e| panic!("a component of type {t} is refused: {e}"));
    }
    s.model.check().unwrap();
    family
}

#[test]
fn a_family_from_the_bridge_comes_in_with_instance_and_type_parameters_formulas_and_a_mesh_for_each_type() {
    let got = read(&bench());
    let f = &got.family;
    assert_eq!((got.name.as_str(), f["category"].as_str()), ("Park Bench", Some("furniture")), "named as the export says");
    // Type and instance parameters, each of its kind, under its heading.
    assert_eq!(parameter(f, "Seat Width"), &json!({"name": "Seat Width", "kind": "length", "value": 1200.0, "group": "dimensions"}));
    assert_eq!(parameter(f, "Lean"), &json!({"name": "Lean", "kind": "angle", "value": 12.5, "instance": true}));
    assert_eq!(parameter(f, "Has Back")["kind"], "yesno");
    assert_eq!(parameter(f, "Slats")["kind"], "integer");
    assert_eq!(parameter(f, "Finish")["shared"], "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10");
    assert_eq!(parameter(f, "Finish")["group"], "identity-data");
    // A formula is kept as written, and what it works out is no type's to set.
    assert_eq!(parameter(f, "Half")["formula"], "Seat Width / 2");
    // A material is its name as text, and a data type with no kind a number in the family's unit.
    assert_eq!((parameter(f, "Face")["kind"].as_str(), f["types"]["1200"]["Face"].as_str()), (Some("text"), Some("Oak")));
    assert_eq!((parameter(f, "Power")["kind"].as_str(), f["types"]["1800"]["Power"].as_f64()), (Some("number"), Some(75.0)));
    // Each type sets its type parameters, and which mesh is its own; never an instance parameter or a formula's.
    assert_eq!(
        f["types"]["1800"],
        json!({"Seat Width": 1800.0, "Has Back": false, "Slats": 9.0, "Finish": "Painted", "Face": "Oak", "Power": 75.0, "Imported Type": 1.0})
    );
    assert_eq!(f["types"]["1200"]["Imported Type"], 0.0);
    assert_eq!(f["options"], json!({"work_plane_based": true, "always_vertical": false, "shared": true}));
    let forms = f["forms"].as_array().unwrap();
    assert_eq!(forms.len(), 2);
    assert_eq!((forms[0]["visible"].as_str(), forms[1]["visible"].as_str()), (Some("Imported Type = 0"), Some("Imported Type = 1")));
    assert_eq!(forms[1]["mesh"], solid(1800.0));

    let family = loaded(&got);
    assert_eq!(family.parameters.len(), 9);
    assert!(family.parameters.iter().any(|p| p.name == "Lean" && p.instance));
    assert_eq!(family.forms.len(), 2);
    // The notes: what came in, what the export itself said, and what a mesh is not.
    assert!(got.notes[0].starts_with("2 types, 8 parameters (1 instance, 1 formula) and the geometry of 2 types read"), "{}", got.notes[0]);
    let all = got.notes.join("\n");
    assert!(all.contains("Family Legs (nested) is not exported"), "{all}");
    assert!(all.contains("Power (W)"), "{all}");
    for missing in ["mesh", "constraints", "connectors", "materials"] {
        assert!(all.contains(missing), "the notes name {missing}: {all}");
    }
}

#[test]
fn a_family_of_one_type_has_one_mesh_and_no_selector_and_one_without_geometry_says_so() {
    let mut one = bench();
    one["types"].as_array_mut().unwrap().truncate(1);
    let got = read(&one);
    assert!(got.family["parameters"].as_array().unwrap().iter().all(|p| p["name"] != "Imported Type"));
    assert_eq!(got.family["forms"].as_array().unwrap().len(), 1);
    assert!(got.family["forms"][0].get("visible").is_none());
    loaded(&got);

    let mut bare = bench();
    for t in bare["types"].as_array_mut().unwrap() {
        t["triangles"] = json!([]);
    }
    let got = read(&bare);
    assert!(got.family.get("forms").is_none());
    assert!(got.notes.iter().any(|n| n.contains("no geometry")), "{:?}", got.notes);
    loaded(&got);
}

#[test]
fn a_formula_bimcraft_cannot_read_is_left_out_and_its_parameter_keeps_each_types_value() {
    let mut v = bench();
    v["parameters"][1]["formula"] = json!("Seat Width +* (");
    v["parameters"][4]["formula"] = json!("Nowhere * 2");
    let got = read(&v);
    let f = &got.family;
    for name in ["Half", "Slats"] {
        assert!(parameter(f, name).get("formula").is_none(), "{name}");
    }
    assert_eq!((f["types"]["1200"]["Half"].as_f64(), f["types"]["1800"]["Half"].as_f64()), (Some(600.0), Some(900.0)));
    assert_eq!(f["types"]["1800"]["Slats"], 9.0);
    let note = got.notes.iter().find(|n| n.contains("formula") && n.contains("left out")).unwrap_or_else(|| panic!("{:?}", got.notes));
    assert!(
        note.starts_with("2 formulas left out") && note.contains("Half = Seat Width +* (") && note.contains("Slats = Nowhere * 2"),
        "{note}"
    );
    assert!(got.notes[0].contains("(1 instance, 0 formulas)"), "{}", got.notes[0]);
    loaded(&got);
}

#[test]
fn a_revit_category_is_told_by_its_built_in_name() {
    for (ost, category) in [
        ("OST_Furniture", "furniture"),
        ("OST_Casework", "casework"),
        ("OST_PlumbingFixtures", "plumbing"),
        ("OST_PipeFitting", "pipefitting"),
        ("OST_PipeAccessory", "pipeaccessory"),
        ("OST_ElectricalEquipment", "equipment"),
        ("OST_ElectricalFixtures", "outlet"),
        ("OST_LightingDevices", "switch"),
        ("OST_LightingFixtures", "lighting"),
        ("OST_DuctTerminal", "airterminal"),
        ("OST_DataDevices", "dataoutlet"),
        ("OST_TelephoneDevices", "voiceoutlet"),
        ("OST_GenericModel", "generic"),
    ] {
        let mut v = bench();
        v["category"] = json!(ost);
        let got = read(&v);
        assert_eq!(got.family["category"], category, "{ost}");
        assert!(!got.notes.iter().any(|n| n.contains("category")), "{ost}: {:?}", got.notes);
    }
    // A category with none here, and a door without the Width and Height a door here makes its opening of.
    for (ost, why) in [("OST_StructConnections", "no family category"), ("OST_Doors", "Width")] {
        let mut v = bench();
        v["category"] = json!(ost);
        let got = read(&v);
        assert_eq!(got.family["category"], "generic", "{ost}");
        assert!(got.notes.iter().any(|n| n.contains(ost) && n.contains(why)), "{ost}: {:?}", got.notes);
        loaded(&got);
    }
    let mut door = bench();
    door["category"] = json!("OST_Doors");
    door["parameters"] = json!([{"name": "Width", "kind": "length"}, {"name": "Height", "kind": "length"}]);
    door["types"] = json!([{"name": "0.9 x 2.1", "values": {"Width": 900.0, "Height": 2100.0}, "triangles": solid(900.0)}]);
    assert_eq!(read(&door).family["category"], "door");
}

#[test]
fn what_the_rules_refuse_is_left_out_by_name_and_geometry_that_is_no_mesh_leaves_the_parameters() {
    let mut v = bench();
    let params = v["parameters"].as_array_mut().unwrap();
    params.push(json!({"name": "Seat Width", "kind": "length"}));
    params.push(json!({"name": "d1 (max)", "kind": "length"}));
    params.push(json!({"name": "Odd", "kind": "flavour"}));
    params.push(json!({"name": "Wide", "kind": "length"}));
    v["types"][0]["values"]["d1 (max)"] = json!(40.0);
    v["types"][0]["values"]["Odd"] = json!(1.0);
    v["types"][0]["values"]["Wide"] = json!(1.0e30);
    v["types"][1]["triangles"] = json!([[[0, 0, 0], [1, 0, 0]], "x"]);
    let got = read(&v);
    let f = &got.family;
    let all = got.notes.join("\n");
    assert_eq!(f["types"]["1200"]["d1 _max_"], 40.0, "a name the rules refuse is rewritten");
    assert!(all.contains("d1 (max) is d1 _max_"), "{all}");
    for name in ["Odd", "Wide"] {
        assert!(f["parameters"].as_array().unwrap().iter().all(|p| p["name"] != name), "{name} is left out");
        assert!(got.notes.iter().any(|n| n.contains("left out") && n.contains(&format!("`{name}`"))), "{name}: {all}");
    }
    assert_eq!(f["parameters"].as_array().unwrap().iter().filter(|p| p["name"] == "Seat Width").count(), 1);
    assert!(f.get("forms").is_none(), "one type's geometry is no mesh, so none comes in");
    assert!(got.notes.iter().any(|n| n.contains("geometry") && n.contains("left out") && n.contains("1800")), "{all}");
    loaded(&got);
}

#[test]
fn a_file_that_is_not_the_bridges_or_is_of_a_newer_version_says_so() {
    let e = |bytes: &[u8]| crate::exchange_family(bytes, "X").unwrap_err().to_string();
    assert!(e(b"not json").contains("not a family exported by tools/revit_bridge"), "{}", e(b"not json"));
    assert!(e(br#"{"name": "a BIMCraft family"}"#).contains("bimcraft-revit-family"));
    assert!(e(br#"[1, 2]"#).contains("bimcraft-revit-family"));
    let mut newer = bench();
    newer["version"] = json!(2);
    let said = e(newer.to_string().as_bytes());
    assert!(said.contains("version 2") && said.contains("newer"), "{said}");
    // A field this version does not know is passed over, as a file's is.
    let mut more = bench();
    more["later"] = json!({"x": 1});
    more["parameters"][0]["later"] = json!(true);
    assert_eq!(read(&more).family["types"]["1200"]["Seat Width"], 1200.0);
    // With no name of its own, it takes the file's.
    let mut unnamed = bench();
    unnamed.as_object_mut().unwrap().remove("name");
    assert_eq!(read(&unnamed).name, "File Name");
}

#[test]
fn a_damaged_export_never_panics_and_what_comes_back_still_loads() {
    let good = bench().to_string().into_bytes();
    let mut seed = 0x1234_5678_9abc_def1u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut came_back = 0;
    for _ in 0..1500 {
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
        if let Ok(got) = crate::exchange_family(&bad, "Bench") {
            came_back += 1;
            loaded(&got);
        }
    }
    assert!(came_back > 100, "the sweep reaches the reader: {came_back}");
    // Values of the wrong shape, names of any length, more than a family holds.
    let long = "x".repeat(4000);
    let many: Vec<Value> = (0..90).map(|i| json!({"name": format!("P{i}"), "kind": "length"})).collect();
    let odd = json!({
        "format": "bimcraft-revit-family", "version": 1, "name": long, "category": 7,
        "options": {"work_plane_based": "yes", "shared": null},
        "parameters": many,
        "types": [
            {"name": "A", "values": {"P0": "wide", "P1": null, "P2": [1], "P3": -1.0e9, "P4": true}, "triangles": solid(1.0e12)},
            {"name": "", "values": 3}, {"name": long, "triangles": 5}, {"name": "A"}, 7,
        ],
        "notes": [long, 3],
    });
    let got = read(&odd);
    assert!(got.notes.iter().any(|n| n.contains("a family holds")), "{:?}", got.notes);
    loaded(&got);
}
