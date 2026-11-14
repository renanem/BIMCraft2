//! Revit files (`.rvt`, `.rfa`) on the command line: `info` reads one with `bimcraft-rvt`, which is Apache-2.0 only (see
//! docs/rvt-provenance.md) and so sits behind the `rvt` feature, and `open` imports one into the session as a new project.

use bimcraft_core::engine::Session;
use serde_json::Value;

/// Whether a path names a Revit file (a project, a family, a template or a family template), or a template the bridge
/// exported from Revit (`.rte.json`), which opens as a project does.
pub fn is_revit(path: &str) -> bool {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "rvt" | "rfa" | "rte" | "rft") || is_template_export(path)
}

/// Whether a path names a template (or a project) exported from Revit by tools/revit_bridge.
pub fn is_template_export(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".rte.json") || lower.ends_with(".rvt.json")
}

/// Whether a path names a Revit family (or a family template), or one the bridge exported from Revit (`.rfa.json`): either is
/// loaded into a project, never opened as one.
pub fn is_family(path: &str) -> bool {
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
    matches!(ext.as_str(), "rfa" | "rft") || is_export(path)
}

/// Whether a path names a family exported from Revit by tools/revit_bridge.
fn is_export(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".rfa.json")
}

#[cfg(not(feature = "rvt"))]
pub fn load_family(_: &mut Session, path: &str) -> Result<Value, String> {
    Err(format!("{path}: {WITHOUT}"))
}

/// Loads a Revit family into the session's project through the `family` command: from a `.rfa`, its types and their
/// parameters, as the file's PartAtom entry lists them; from the bridge's export (`.rfa.json`), every parameter with its
/// formula and a mesh for each type. The reply says what came in and, in `skipped`, what was left out and why.
#[cfg(feature = "rvt")]
pub fn load_family(s: &mut Session, path: &str) -> Result<Value, String> {
    use serde_json::json;

    let file = std::path::Path::new(path);
    let read = if is_export(path) { bimcraft_rvt::exchange_family_path(file) } else { bimcraft_rvt::family_path(file) };
    let read = read.map_err(|e| format!("{path}: {e}"))?;
    s.execute("family", &read.family).map_err(|e| format!("{path}: {e}"))?;
    let count = |key: &str| read.family[key].as_object().map(|o| o.len()).or_else(|| read.family[key].as_array().map(Vec::len));
    Ok(json!({
        "path": path,
        "family": read.name,
        "category": read.family["category"],
        "types": count("types"),
        "parameters": count("parameters"),
        "forms": count("forms").unwrap_or(0),
        "skipped": read.notes,
    }))
}

#[cfg(not(feature = "rvt"))]
const WITHOUT: &str = "this build of bimcraft-cli was made without Revit support (`--no-default-features`); build with the default `rvt` feature to read Revit files";

#[cfg(not(feature = "rvt"))]
pub fn info(path: &str) -> Result<Value, String> {
    Err(format!("{path}: {WITHOUT}"))
}

#[cfg(not(feature = "rvt"))]
pub fn info_with_options(path: &str, _include_detail: bool) -> Result<Value, String> {
    Err(format!("{path}: {WITHOUT}"))
}

#[cfg(not(feature = "rvt"))]
pub fn open(_: &mut Session, path: &str) -> Result<Value, String> {
    Err(format!("{path}: {WITHOUT}"))
}

/// Imports a Revit project as the session's project, the way `open` imports an IFC file: a new project (saving it asks where).
/// The reply says what came in and, in `skipped`, what was left out and why.
#[cfg(feature = "rvt")]
pub fn open(s: &mut Session, path: &str) -> Result<Value, String> {
    use serde_json::json;
    use std::collections::BTreeMap;

    let stem = std::path::Path::new(path).file_stem().map_or_else(|| "Project".into(), |s| s.to_string_lossy().into_owned());
    // What the bridge exported of a template is JSON of BIMCraft's own; a Revit file is read by the reader.
    let i = if is_template_export(path) {
        bimcraft_rvt::exchange_template_path(std::path::Path::new(path))
    } else {
        bimcraft_rvt::import_path(std::path::Path::new(path), &stem)
    };
    let i = i.map_err(|e| format!("{path}: {e}"))?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in i.model.elements.values() {
        *counts.entry(e.category()).or_default() += 1;
    }
    let reply = json!({"path": path, "name": i.model.name, "format": i.schema, "elements": counts, "skipped": i.skipped, "moved": i.moved});
    s.reset(i.model, None);
    Ok(reply)
}

/// The release, the last save, the levels and the elements by category of a Revit file. The last saver's name and the path
/// it was saved to are in the file and shown here; nothing writes them into a project or an IFC file.
#[cfg(feature = "rvt")]
pub fn info(path: &str) -> Result<Value, String> {
    info_with_options(path, false)
}

#[cfg(feature = "rvt")]
pub fn info_with_options(path: &str, include_detail: bool) -> Result<Value, String> {
    use bimcraft_rvt::inventory::category_name;
    use bimcraft_rvt::{Limits, Revit};
    use serde_json::json;

    let failed = |e: bimcraft_rvt::Error| format!("{path}: {e}");
    let mut revit = Revit::open(std::path::Path::new(path), Limits::default()).map_err(failed)?;
    let basic = revit.basic_file_info().map_err(failed)?;
    let partition_streams: Vec<String> =
        revit.stream_names().into_iter().filter(|n| n.starts_with("Partitions/")).map(String::from).collect();
    // The Atom entry is a bonus: a file whose entry is damaged still shows the rest.
    let atom = revit.document_atom().ok().flatten();
    let mm = |feet: f64| (feet * 304.8 * 1000.0).round() / 1000.0;
    let elements = if include_detail {
        match revit.detailed_inventory() {
            Ok(det) => {
                let inv = det.inventory;
                let categories: Vec<Value> = inv
                    .counts()
                    .iter()
                    .map(|c| json!({"category": c.category, "name": category_name(c.category), "elements": c.elements, "placed": c.placed}))
                    .collect();
                let detail_json: Vec<Value> = det
                    .elements_detail
                    .iter()
                    .filter(|e| e.placed)
                    .map(|e| {
                        json!({
                            "id": e.id,
                            "category": e.category,
                            "category_name": category_name(e.category),
                            "class_tag": e.class_tag,
                            "class_name": e.class_name,
                            "placed": e.placed,
                            "level_id": e.level_id,
                            "bbox_min_mm": e.bbox_min_mm,
                            "bbox_max_mm": e.bbox_max_mm,
                            "location_mm": e.location_mm,
                        })
                    })
                    .collect();
                json!({
                    "declared": inv.declared,
                    "total": inv.elements.len(),
                    "placed": inv.elements.iter().filter(|e| e.placed).count(),
                    "partitions_count": partition_streams.len(),
                    "categories": categories,
                    "levels": inv.levels.iter().map(|l| json!({"id": l.id, "name": l.name, "elevation": mm(l.elevation_feet), "building_story": l.building_story})).collect::<Vec<_>>(),
                    "unread_levels": inv.unread_levels,
                    "owned_levels": inv.owned_levels,
                    "elements_detail_placed_count": detail_json.len(),
                    "elements_detail": detail_json,
                })
            }
            Err(e) => json!({"error": e.to_string()}),
        }
    } else {
        match revit.inventory() {
            Ok(inv) => {
                let categories: Vec<Value> = inv
                    .counts()
                    .iter()
                    .map(|c| json!({"category": c.category, "name": category_name(c.category), "elements": c.elements, "placed": c.placed}))
                    .collect();
                json!({
                    "declared": inv.declared,
                    "total": inv.elements.len(),
                    "placed": inv.elements.iter().filter(|e| e.placed).count(),
                    "categories": categories,
                    "levels": inv.levels.iter().map(|l| json!({"id": l.id, "name": l.name, "elevation": mm(l.elevation_feet), "building_story": l.building_story})).collect::<Vec<_>>(),
                    "unread_levels": inv.unread_levels,
                    "owned_levels": inv.owned_levels,
                })
            }
            // A family or a release the reader does not know: say why, and still show the rest.
            Err(e) => json!({"error": e.to_string()}),
        }
    };
    Ok(json!({
        "revit": {
            "release": basic.release,
            "build": basic.build,
            "document": basic.guid,
            "locale": basic.locale,
            "worksharing": basic.worksharing,
            "title": atom.as_ref().and_then(|a| a.title.clone()),
            "saved": atom.as_ref().and_then(|a| a.updated.clone()),
            "omniclass": atom.as_ref().and_then(|a| a.omniclass.clone()),
        },
        "saved_by": basic.username,
        "saved_to": basic.last_save_path,
        "elements": elements,
        "note": "saved_by is the file's Username line, which is set only when worksharing is on; saved_to is the path it was last saved to, on the saving machine. Both are shown here and written nowhere else",
    }))
}

#[cfg(test)]
#[cfg(feature = "rvt")]
pub mod tests {
    use bimcraft_rvt::project_fixture::{
        Block, KIND_MATERIAL, Lev, Mat, Project, Rec, TypeRec, layers_bytes, line_bytes, name_bytes, orientation_bytes,
    };

    #[test]
    fn a_project_template_imports_supported_elements_and_reports_its_limits() {
        let path = house("Office.RTE");
        let mut session = bimcraft_core::engine::Session::new();
        let result = super::open(&mut session, &path).unwrap();
        assert!(
            result["skipped"].as_array().unwrap().iter().any(|note| note.as_str().unwrap().contains("not an entire template conversion"))
        );
        assert_eq!(session.model.elements.values().filter(|e| e.category() == "wall").count(), 1);
        assert!(session.path.is_none());
        session.model.check().unwrap();
        std::fs::remove_file(path).unwrap();
    }

    /// A Revit project on disk, in a folder of its own: levels at 0 and 10 ft and one wall 20 ft long, 10 ft tall, of a type with a
    /// brick layer over a concrete one.
    pub fn house(name: &str) -> String {
        const WALLS: i64 = -2_000_011;
        let mut p = Project::new(2024);
        p.levels = vec![Lev::new(10, "Level 1", 0.0), Lev::new(11, "Level 2", 10.0)];
        p.mats = vec![Mat { id: 700, name: "Brick".into() }, Mat { id: 701, name: "Concrete".into() }];
        p.types = vec![TypeRec::new(500, WALLS), TypeRec::new(700, -2_000_700), TypeRec::new(701, -2_000_700)];
        p.types[1].kind = KIND_MATERIAL;
        p.types[2].kind = KIND_MATERIAL;
        let mut named = name_bytes("Brick wall");
        named.extend(layers_bytes(&[(0.25, Some(700), 4), (0.5, Some(701), 1)], 2024));
        let mut line = line_bytes(0.0, 20.0, [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]);
        line.extend(orientation_bytes(0, 1, false, true));
        p.data = vec![Block { id: 500, payload: named }, Block { id: 100, payload: line }];
        p.recs.push(Rec::new(100, WALLS).at([0.0, -0.375, 0.0, 20.0, 0.375, 10.0], &[10, 11, 500, 100]));
        let dir = std::env::temp_dir().join(format!("bimcraft-cli-rvt-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, p.file()).unwrap();
        path.to_string_lossy().into_owned()
    }
}
