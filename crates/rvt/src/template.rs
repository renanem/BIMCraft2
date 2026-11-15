//! A template (or a project) exported from Revit by the bridge (`tools/revit_bridge`), as a BIMCraft project to save as a
//! template (`.bimt`).
//!
//! Like `exchange.rs`, this reads no Revit format: the bridge asks Revit's public API and writes JSON of BIMCraft's own,
//! `bimcraft-revit-template` version 1, beside the families it exports. Each thing it lists is made here the way a user
//! would make it, by running its command on a new project (`level`, `phase`, `line_pattern`, `line_style`, `fill_pattern`,
//! `text_type`, `dimension_type`, `object_styles`, `parameter_import`, `family`), so the model's own rules decide what comes in: what a command refuses is left out, by name, with the
//! command's reason, and the rest comes in.
//!
//! ```json
//! {"format": "bimcraft-revit-template", "version": 1, "name": "Office",
//!  "levels": [{"name": "Ground", "elevation": 0.0}],
//!  "phases": ["Existing", "New Construction"],
//!  "line_patterns": [{"name": "Dash", "segments": [{"dash": 6.0}, {"space": 3.0}, "dot"]}],
//!  "fill_patterns": [{"name": "Hatch", "target": "drafting", "solid": false,
//!                     "lines": [{"angle": 45.0, "spacing": 3.0, "origin": [0.0, 0.0], "shift": 0.0, "dashes": [2.0, -1.0]}]}],
//!  "text_types": [{"name": "2mm", "font": "Arial", "size": 2.0, "bold": false, "italic": false, "underline": false,
//!                  "width_factor": 1.0, "color": [0, 0, 0], "border": false, "border_offset": 1.0}],
//!  "line_styles": [{"name": "Demolished", "pen": 3, "color": [255, 0, 0], "pattern": "Dash"}],
//!  "object_styles": [{"category": "OST_Walls", "projection": 2, "cut": 5}],
//!  "dimension_types": [{"name": "2mm", "font": "Arial", "text_size": 2.0, "bold": false, "italic": false,
//!                       "underline": false, "width_factor": 1.0, "color": [0, 0, 0], "line_pen": 1, "text_offset": 1.0,
//!                       "witness_gap": 1.0, "witness_extension": 2.0, "line_extension": 0.0}],
//!  "parameters": [{"name": "Fire Rating", "kind": "text", "guid": "0f3c9a52-…", "scope": "type",
//!                  "group": "identity-data", "categories": ["OST_Walls", "OST_Doors"]}],
//!  "families": ["families/0000_Desk.rfa.json"],
//!  "notes": ["what the bridge itself could not export"]}
//! ```
//!
//! Lengths are millimetres (a pattern's and a text type's on paper, a level's and a model pattern's in the building),
//! angles degrees. A pen is Revit's line weight, 1 to 16. A category is the `BuiltInCategory` name of Revit's public API
//! (`OST_Walls`), told here as the category BIMCraft has for it; one it has none for is left out and said. A family is a
//! path under the template file's own folder, in `exchange.rs`'s format.

use std::path::{Component, Path};
use std::sync::Arc;

use bimcraft_core::engine::Session;
use bimcraft_core::import::Imported;
use bimcraft_core::model::Element;
use serde_json::{Map, Value, json};

/// The export's name of itself, and the version this reader reads.
pub const FORMAT: &str = "bimcraft-revit-template";
pub const VERSION: u64 = 1;
/// The largest template file read, in bytes, and the most entries of one kind.
pub const MAX_EXPORT: u64 = 64 << 20;
pub const MAX_ENTRIES: usize = 10_000;
/// The most names a note lists.
const MAX_LISTED: usize = 12;

/// What was left out, grouped by kind and reason, in the order met.
#[derive(Default)]
struct Notes {
    left: Vec<(&'static str, String, Vec<String>)>,
}

impl Notes {
    fn left_out(&mut self, what: &'static str, why: &str, who: &str) {
        // A command's error opens with its name: the reason is what follows.
        let why = why.split_once(": ").filter(|(c, _)| !c.contains(' ')).map_or(why, |(_, rest)| rest);
        let why: String = why.chars().take(220).collect();
        let who: String = who.chars().take(60).collect();
        match self.left.iter_mut().find(|(w, y, _)| *w == what && *y == why) {
            Some((_, _, names)) => names.push(who),
            None => self.left.push((what, why, vec![who])),
        }
    }

    fn said(&self) -> Vec<String> {
        self.left
            .iter()
            .map(|(what, why, names)| {
                let mut listed = names.iter().take(MAX_LISTED).map(|n| format!("`{n}`")).collect::<Vec<_>>().join(", ");
                if names.len() > MAX_LISTED {
                    listed.push_str(&format!(" and {} more", names.len() - MAX_LISTED));
                }
                format!("{} left out, {why}: {listed}", count(names.len(), what))
            })
            .collect()
    }
}

/// `3 line patterns`, `1 family`, `2 families`.
fn count(n: usize, one: &str) -> String {
    let many = match one.strip_suffix('y') {
        Some(stem) => format!("{stem}ies"),
        None => format!("{one}s"),
    };
    format!("{n} {}", if n == 1 { one.to_string() } else { many })
}

/// The entries of the list `key`, the first [`MAX_ENTRIES`] of them.
fn entries<'a>(v: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    v.get(key).and_then(Value::as_array).into_iter().flatten().take(MAX_ENTRIES)
}

/// An entry's name: text, once trimmed not empty.
fn name(v: &Value) -> Option<&str> {
    v.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty())
}

/// `v`'s fields named in `keys`, where it has them: what a command is given, and no field it does not know.
fn picked(v: &Value, keys: &[&str]) -> Map<String, Value> {
    keys.iter().filter_map(|k| v.get(*k).filter(|x| !x.is_null()).map(|x| (k.to_string(), x.clone()))).collect()
}

/// The ids of the project's levels, lowest first.
fn level_ids(s: &Session) -> Vec<(bimcraft_core::model::Id, String)> {
    let mut all: Vec<(f64, bimcraft_core::model::Id, String)> = s
        .model
        .elements
        .iter()
        .filter_map(|(id, e)| if let Element::Level(l) = e { Some((l.elevation, *id, l.name.clone())) } else { None })
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    all.into_iter().map(|(_, id, name)| (id, name)).collect()
}

/// The template's levels in place of the two a project starts with: those are named out of the way, the template's are
/// added, and the starters are deleted once one of the template's is there; with none, they get their names back.
fn levels(s: &mut Session, v: &Value, notes: &mut Notes) -> usize {
    let wanted: Vec<&Value> = entries(v, "levels").collect();
    if wanted.is_empty() {
        return 0;
    }
    let starters = level_ids(s);
    for (i, (id, _)) in starters.iter().enumerate() {
        let _ = s.execute("modify", &json!({"id": id, "name": format!("BIMCraft starter level {}", i + 1)}));
    }
    let mut made = 0;
    for level in wanted {
        let shown = level.to_string();
        let Some(named) = name(level) else {
            notes.left_out("level", "it has no name", &shown);
            continue;
        };
        let Some(elevation) = level.get("elevation").and_then(Value::as_f64) else {
            notes.left_out("level", "it has no elevation", named);
            continue;
        };
        match s.execute("level", &json!({"name": named, "elevation": elevation})) {
            Ok(_) => made += 1,
            Err(why) => notes.left_out("level", &why, named),
        }
    }
    if made > 0 {
        let ids: Vec<_> = starters.iter().map(|(id, _)| *id).collect();
        if s.execute("delete", &json!({ "ids": ids })).is_err() {
            for (id, _) in &starters {
                let _ = s.execute("delete", &json!({"ids": [id]}));
            }
        }
    }
    // A starter still there (no level of the template's came in, or it could not be deleted) has its own name again.
    let still: Vec<_> = level_ids(s).into_iter().map(|(id, _)| id).collect();
    for (id, was) in starters.iter().filter(|(id, _)| still.contains(id)) {
        let _ = s.execute("modify", &json!({"id": id, "name": was}));
    }
    made
}

/// The template's phases in place of the two a project starts with, the same way: in the template's order.
fn phases(s: &mut Session, v: &Value, notes: &mut Notes) -> usize {
    let wanted: Vec<&Value> = entries(v, "phases").collect();
    if wanted.is_empty() {
        return 0;
    }
    let starters = s.model.phases.clone();
    let aside: Vec<String> = (1..=starters.len()).map(|i| format!("BIMCraft starter phase {i}")).collect();
    for (was, temp) in starters.iter().zip(&aside) {
        let _ = s.execute("phase", &json!({"name": was, "rename": temp}));
    }
    let mut made: Vec<String> = Vec::new();
    for phase in wanted {
        let Some(named) = phase.as_str().map(str::trim).filter(|n| !n.is_empty()) else {
            notes.left_out("phase", "it has no name", &phase.to_string());
            continue;
        };
        if made.iter().any(|m| m == named) {
            notes.left_out("phase", "another phase of the template has its name", named);
            continue;
        }
        match s.execute("phase", &json!({ "name": named })) {
            Ok(_) => made.push(named.to_string()),
            Err(why) => notes.left_out("phase", &why, named),
        }
    }
    for (was, temp) in starters.iter().zip(&aside) {
        let gone = match made.first() {
            Some(first) => s.execute("phase", &json!({"name": temp, "delete": true, "into": first})).is_ok(),
            None => false,
        };
        if !gone {
            let _ = s.execute("phase", &json!({"name": temp, "rename": was}));
        }
    }
    made.len()
}

/// One command for each entry of `key`, its params what `params` makes of the entry: how many went through.
fn each(
    s: &mut Session,
    v: &Value,
    key: &str,
    command: &str,
    what: &'static str,
    notes: &mut Notes,
    params: impl Fn(&Value) -> Map<String, Value>,
) -> usize {
    let mut made = 0;
    let wanted: Vec<&Value> = entries(v, key).collect();
    for entry in wanted {
        let Some(named) = name(entry) else {
            notes.left_out(what, "it has no name", &entry.to_string());
            continue;
        };
        let mut p = params(entry);
        p.insert("name".into(), json!(named));
        match s.execute(command, &Value::Object(p)) {
            Ok(_) => made += 1,
            Err(why) => notes.left_out(what, &why, named),
        }
    }
    made
}

/// The category BIMCraft has for Revit's `ost`, a `BuiltInCategory` name of its public API: what object styles and project
/// parameters are told by.
fn category(ost: &str) -> Option<&'static str> {
    Some(match ost {
        "OST_Walls" => "wall",
        "OST_Floors" => "floor",
        "OST_Ceilings" => "ceiling",
        "OST_Roofs" => "roof",
        "OST_Doors" => "door",
        "OST_Windows" => "window",
        "OST_Rooms" => "room",
        "OST_Columns" => "column",
        "OST_StructuralColumns" => "structuralcolumn",
        "OST_StructuralFoundation" => "footing",
        "OST_StructuralFraming" => "beam",
        "OST_PipeCurves" => "pipe",
        "OST_DuctCurves" => "duct",
        "OST_Conduit" => "conduit",
        "OST_CableTray" => "cabletray",
        "OST_Wire" => "wire",
        "OST_Rebar" => "rebar",
        "OST_Furniture" => "furniture",
        "OST_Casework" => "casework",
        "OST_PlumbingFixtures" => "plumbing",
        "OST_PipeFitting" => "pipefitting",
        "OST_PipeAccessory" => "pipeaccessory",
        "OST_ElectricalEquipment" => "equipment",
        "OST_ElectricalFixtures" => "outlet",
        "OST_LightingDevices" => "switch",
        "OST_LightingFixtures" => "lighting",
        "OST_DuctTerminal" => "airterminal",
        "OST_DataDevices" => "dataoutlet",
        "OST_TelephoneDevices" => "voiceoutlet",
        "OST_GenericModel" => "generic",
        "OST_Stairs" => "stair",
        "OST_Ramps" => "ramp",
        "OST_StairsRailing" => "railing",
        "OST_Mass" => "mass",
        "OST_Site" => "site",
        "OST_ShaftOpening" => "shaft",
        "OST_RoomSeparationLines" => "roomseparator",
        _ => return None,
    })
}

/// The line styles: each its pen, colour and pattern. One whose pattern did not come in is drawn solid, and said.
fn line_styles(s: &mut Session, v: &Value, notes: &mut Notes, said: &mut Vec<String>) -> usize {
    let mut made = 0;
    let wanted: Vec<&Value> = entries(v, "line_styles").collect();
    for entry in wanted {
        let Some(named) = name(entry) else {
            notes.left_out("line style", "it has no name", &entry.to_string());
            continue;
        };
        let mut p = picked(entry, &["pen", "color", "pattern"]);
        p.insert("name".into(), json!(named));
        let pattern = entry.get("pattern").and_then(Value::as_str).map(str::to_string);
        match s.execute("line_style", &Value::Object(p.clone())) {
            Ok(_) => made += 1,
            Err(why) => {
                p.remove("pattern");
                match (pattern, s.execute("line_style", &Value::Object(p))) {
                    (Some(pattern), Ok(_)) => {
                        made += 1;
                        let shown: String = pattern.chars().take(60).collect();
                        said.push(format!("Line style {named} is drawn solid: its pattern `{shown}` did not come in"));
                    }
                    _ => notes.left_out("line style", &why, named),
                }
            }
        }
    }
    made
}

/// The object styles: the pen each category draws in where it is seen and where it is cut, one command a category so
/// that one the command refuses leaves the others.
fn object_styles(s: &mut Session, v: &Value, notes: &mut Notes) -> usize {
    let mut made = 0;
    let wanted: Vec<&Value> = entries(v, "object_styles").collect();
    for entry in wanted {
        let Some(ost) = entry.get("category").and_then(Value::as_str) else {
            notes.left_out("object style", "it names no category", &entry.to_string());
            continue;
        };
        let Some(ours) = category(ost) else {
            notes.left_out("object style", "BIMCraft has no category for Revit's", ost);
            continue;
        };
        let pens = picked(entry, &["projection", "cut"]);
        if pens.is_empty() {
            continue;
        }
        match s.execute("object_styles", &json!({"categories": {ours: pens}})) {
            Ok(_) => made += 1,
            Err(why) => notes.left_out("object style", &why, ost),
        }
    }
    made
}

/// A cell of the parameters' table: on one line, quoted.
fn cell(text: &str) -> String {
    let flat: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(bimcraft_core::model::MAX_TEXT).collect();
    format!("\"{}\"", flat.replace('"', "\"\""))
}

/// The project parameters, through the import a table of them goes through (`parameter_import`), which keeps a shared
/// parameter's GUID and keeps apart two of one name: the table is written to a file of its own and read back.
fn parameters(s: &mut Session, v: &Value, notes: &mut Notes, said: &mut Vec<String>) -> usize {
    let mut rows = vec!["GUID,Name,Kind,Categories,Group,Description,Instance/Type,User Modifiable".to_string()];
    let wanted: Vec<&Value> = entries(v, "parameters").collect();
    for entry in wanted {
        let Some(named) = name(entry) else {
            notes.left_out("parameter", "it has no name", &entry.to_string());
            continue;
        };
        let given: Vec<&str> = entries(entry, "categories").filter_map(Value::as_str).collect();
        let ours: Vec<&str> = given.iter().filter_map(|c| category(c)).collect();
        if ours.is_empty() {
            notes.left_out("parameter", "none of its categories is one BIMCraft has", named);
            continue;
        }
        let dropped: Vec<&str> = given.iter().filter(|c| category(c).is_none()).copied().collect();
        if !dropped.is_empty() {
            let listed: String = dropped.iter().take(8).copied().collect::<Vec<_>>().join(", ").chars().take(300).collect();
            said.push(format!("Parameter {named} is not for these categories, which BIMCraft has none for: {listed}"));
        }
        let text = |key: &str| entry.get(key).and_then(Value::as_str).unwrap_or("");
        // A data type with no kind here is a number, as the table's own reader takes it.
        let kind = match text("kind") {
            "material" | "url" => "text",
            "other" | "" => "number",
            kind => kind,
        };
        let scope = if text("scope") == "type" { "Type" } else { "Instance" };
        rows.push(
            [cell(text("guid")), cell(named), cell(kind), cell(&ours.join("|")), cell(text("group")), cell(""), cell(scope), cell("yes")]
                .join(","),
        );
    }
    if rows.len() < 2 {
        return 0;
    }
    let before = s.model.parameters.len();
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let file = std::env::temp_dir().join(format!("bimcraft-template-parameters-{}-{unique}.csv", std::process::id()));
    let written = std::fs::write(&file, rows.join("\n")).map_err(|e| e.to_string());
    let reply = written.and_then(|()| s.execute("parameter_import", &json!({"path": file, "skip_conflicts": true})));
    let _ = std::fs::remove_file(&file);
    match reply {
        Ok(reply) => {
            for row in reply.get("rows").and_then(Value::as_array).into_iter().flatten() {
                let who = row.get("name").and_then(Value::as_str).unwrap_or("");
                match row.get("status").and_then(Value::as_str) {
                    Some("conflict" | "skip") => notes.left_out("parameter", "it conflicts with one the project has", who),
                    _ => {
                        let own: Vec<&str> =
                            row.get("notes").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).collect();
                        if !own.is_empty() {
                            let text: String = own.join("; ").chars().take(300).collect();
                            said.push(format!("Parameter {who}: {text}"));
                        }
                    }
                }
            }
            for left in reply.get("left_out").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str).take(50) {
                said.push(format!("Parameters: {}", left.chars().take(300).collect::<String>()));
            }
        }
        Err(why) => notes.left_out("parameter", &why, "all of them"),
    }
    s.model.parameters.len().saturating_sub(before)
}

/// A fill pattern's params: its target and its lines, each with the fields the command knows; a solid one has no lines.
fn fill_pattern(entry: &Value) -> Map<String, Value> {
    let mut p = picked(entry, &["target"]);
    let solid = entry.get("solid").and_then(Value::as_bool).unwrap_or(false);
    let lines: Vec<Value> = if solid {
        Vec::new()
    } else {
        entries(entry, "lines").map(|l| Value::Object(picked(l, &["angle", "spacing", "origin", "shift", "dashes"]))).collect()
    };
    p.insert("lines".into(), Value::Array(lines));
    p
}

/// The families the template lists, each a file under the template's own folder, loaded with the `family` command.
fn families(s: &mut Session, v: &Value, folder: &Path, notes: &mut Notes, said: &mut Vec<String>) -> usize {
    let mut made = 0;
    let wanted: Vec<&Value> = entries(v, "families").collect();
    for entry in wanted {
        let Some(relative) = entry.as_str().filter(|p| !p.trim().is_empty()) else {
            notes.left_out("family", "it is no path", &entry.to_string());
            continue;
        };
        let shown = Path::new(relative).file_name().map_or_else(|| relative.to_string(), |n| n.to_string_lossy().into_owned());
        // Only a file under the export's own folder: a path that climbs out of it, or starts elsewhere, is never read.
        if !Path::new(relative).components().all(|c| matches!(c, Component::Normal(_))) {
            notes.left_out("family", "its path is not under the export's folder", &shown);
            continue;
        }
        let read = match crate::exchange_family_path(&folder.join(relative)) {
            Ok(read) => read,
            Err(why) => {
                notes.left_out("family", &why, &shown);
                continue;
            }
        };
        match s.execute("family", &read.family) {
            Ok(_) => {
                made += 1;
                // What the family's own import had to say, less its first note (what came in) and its last (what a mesh is).
                let own = read.notes.get(1..read.notes.len().saturating_sub(1)).unwrap_or_default();
                if !own.is_empty() {
                    let text: String = own.join("; ").chars().take(600).collect();
                    said.push(format!("Family {}: {text}", read.name));
                }
            }
            Err(why) => notes.left_out("family", &why, &read.name),
        }
    }
    made
}

/// The template in the bridge's export at `path`, as a new project and the notes of what was left out: what the desktop
/// opens unsaved, to be saved as a `.bimt`.
pub fn exchange_template_path(path: &Path) -> Result<Imported, String> {
    let not = |why: String| format!("is not a template exported by tools/revit_bridge: {why}");
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_EXPORT {
        return Err(not(format!("it is {size} bytes, over the limit of {MAX_EXPORT}")));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_slice(&bytes).map_err(|e| not(format!("it is not JSON ({e})")))?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(not(format!("its `format` is not `{FORMAT}`")));
    }
    match v.get("version").and_then(Value::as_u64) {
        Some(n) if (1..=VERSION).contains(&n) => {}
        Some(n) => {
            return Err(format!(
                "is version {n} of the bridge's export, newer than this BIMCraft reads ({VERSION}): update BIMCraft, or export with the bridge it came with"
            ));
        }
        None => return Err(not("it has no `version`".into())),
    }
    // `Office.rte.json` is named Office.
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = stem.strip_suffix(".rte").or_else(|| stem.strip_suffix(".rvt")).unwrap_or(&stem).to_string();
    let named: String = name(&v).unwrap_or(&stem).chars().take(bimcraft_core::model::MAX_TEXT).collect();
    let named = if named.trim().is_empty() { "Project".to_string() } else { named.trim().to_string() };
    let folder = path.parent().unwrap_or(Path::new("."));

    let mut s = Session::new();
    s.execute("new", &json!({ "name": named }))?;
    let mut notes = Notes::default();
    let mut said: Vec<String> = Vec::new();
    let came = [
        (levels(&mut s, &v, &mut notes), "level"),
        (phases(&mut s, &v, &mut notes), "phase"),
        (each(&mut s, &v, "line_patterns", "line_pattern", "line pattern", &mut notes, |e| picked(e, &["segments"])), "line pattern"),
        (line_styles(&mut s, &v, &mut notes, &mut said), "line style"),
        (each(&mut s, &v, "fill_patterns", "fill_pattern", "fill pattern", &mut notes, fill_pattern), "fill pattern"),
        (
            each(&mut s, &v, "text_types", "text_type", "text type", &mut notes, |e| {
                picked(e, &["font", "size", "bold", "italic", "underline", "width_factor", "color", "border", "border_offset"])
            }),
            "text type",
        ),
        (
            each(&mut s, &v, "dimension_types", "dimension_type", "dimension type", &mut notes, |e| {
                picked(
                    e,
                    &[
                        "font",
                        "text_size",
                        "bold",
                        "italic",
                        "underline",
                        "width_factor",
                        "color",
                        "line_pen",
                        "text_offset",
                        "witness_gap",
                        "witness_extension",
                        "line_extension",
                    ],
                )
            }),
            "dimension type",
        ),
        (object_styles(&mut s, &v, &mut notes), "object style"),
        (parameters(&mut s, &v, &mut notes, &mut said), "parameter"),
        (families(&mut s, &v, folder, &mut notes, &mut said), "family"),
    ];
    let listed: Vec<String> = came.iter().map(|(n, what)| count(*n, what)).collect();
    let (last, first) = listed.split_last().map_or((String::new(), String::new()), |(l, f)| (l.clone(), f.join(", ")));
    let mut skipped = vec![format!("Came in from the bridge's export: {first} and {last}")];
    skipped.extend(notes.said());
    skipped.extend(said);
    // What the bridge itself said it could not export.
    skipped.extend(entries(&v, "notes").filter_map(Value::as_str).take(100).map(|n| n.chars().take(500).collect::<String>()));
    skipped.push(
        "The bridge does not export these yet, so the project has BIMCraft's own: view templates, filters, line weights \
         (Revit's API does not give them), a dimension type's tick mark, sheets and title blocks, and the views and elements \
         of the model"
            .into(),
    );
    // The commands kept the model checked; a last check stands behind what is handed on.
    s.model.check()?;
    let model = Arc::unwrap_or_clone(s.model);
    Ok(Imported { model, schema: "Revit template (bridge)".into(), skipped, moved: None })
}
