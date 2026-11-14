//! A family exported from Revit by the bridge (`tools/revit_bridge`), as a BIMCraft family.
//!
//! The bridge runs inside Revit and asks its public API (`FamilyManager`: every parameter with its data type, whether it
//! is an instance's, its shared GUID and its formula; each type's values; each type's geometry triangulated) and writes
//! what it is told as JSON of BIMCraft's own, `bimcraft-revit-family` version 1, in the model's units. Nothing here reads
//! Revit's format: this module reads that JSON, which is hostile like any file, into the params of the `family` command,
//! checked against the family's own rules as it is built (`Family::check`), so a parameter, a formula or a mesh the rules
//! refuse is left out, by name, with the rule, and the rest comes in.
//!
//! ```json
//! {"format": "bimcraft-revit-family", "version": 1, "name": "Park Bench", "category": "OST_Furniture",
//!  "options": {"work_plane_based": false, "always_vertical": true, "shared": false, "cut_with_voids_when_loaded": false},
//!  "parameters": [{"name": "Seat Width", "kind": "length", "instance": false, "shared": null, "formula": null,
//!                  "reporting": false, "group": "dimensions", "unit": null}],
//!  "types": [{"name": "1200", "values": {"Seat Width": 1200.0}, "triangles": [[[0, 0, 0], [1, 0, 0], [0, 1, 0]]]}],
//!  "notes": ["what the bridge itself could not export"]}
//! ```
//!
//! A parameter's `kind` is one of `length` (mm), `area` (mm²), `volume` (mm³), `angle` (degrees), `number`, `integer`,
//! `yesno`, `text`, `url`, `material` (its name) and `other` (a number in `unit`, the family's own display unit).

use std::collections::BTreeMap;

use bimcraft_core::family::{Family, MAX_INTEGER, MAX_PARAMETERS, MAX_TYPES, MAX_URL, nameable, renamed_in_formula};
use bimcraft_core::model::{MAX_COORD, MAX_TEXT};
use serde_json::{Map, Value, json};

use crate::error::{Error, Result};
use crate::family::{FamilyFile, count, is_guid, listed, named};

/// The export's name of itself, and the version this reader reads.
pub const FORMAT: &str = "bimcraft-revit-family";
pub const VERSION: u64 = 1;
/// The largest export read, in bytes.
pub const MAX_EXPORT: usize = 256 << 20;
/// The parameter that says which type's mesh a component shows, where a family has more than one type.
const SELECTOR: &str = "Imported Type";
const GROUPS: [&str; 14] = [
    "constraints",
    "construction",
    "dimensions",
    "graphics",
    "identity-data",
    "materials-and-finishes",
    "text",
    "visibility",
    "electrical",
    "mechanical",
    "plumbing",
    "structural",
    "data",
    "other",
];

/// The BIMCraft category of a family of Revit's category `ost`, a `BuiltInCategory` name of its public API.
fn category(ost: &str) -> Option<&'static str> {
    Some(match ost {
        "OST_Furniture" | "OST_FurnitureSystems" => "furniture",
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
        "OST_Doors" => "door",
        "OST_Windows" => "window",
        "OST_GenericModel" => "generic",
        _ => return None,
    })
}

/// `v` as the model holds a value of `kind`, or why it is none.
fn value(kind: &str, v: &Value) -> std::result::Result<Value, String> {
    let shown: String = v.to_string().chars().take(40).collect();
    let number = |most: f64, what: &str| {
        v.as_f64()
            .filter(|x| x.is_finite() && x.abs() <= most)
            .map(|x| json!(x))
            .ok_or_else(|| format!("{shown} is no {what} the model holds"))
    };
    let words = |most: usize| match v {
        Value::String(t) if t.chars().count() <= most && !t.chars().any(char::is_control) => Ok(json!(t)),
        Value::String(_) => Err(format!("its text is on more than one line or longer than {most} characters")),
        _ => Err(format!("{shown} is no text")),
    };
    match kind {
        "length" => number(MAX_COORD, "length"),
        "area" => number(MAX_COORD * MAX_COORD, "area"),
        "volume" | "angle" | "number" => number(f64::MAX, "number"),
        "integer" => v
            .as_f64()
            .filter(|x| x.fract() == 0.0 && x.abs() <= MAX_INTEGER)
            .map(|x| json!(x))
            .ok_or_else(|| format!("{shown} is no whole number")),
        "yesno" => match v {
            Value::Bool(b) => Ok(json!(b)),
            v => v.as_f64().filter(|x| x.is_finite()).map(|x| json!(x != 0.0)).ok_or_else(|| format!("{shown} is neither yes nor no")),
        },
        "url" => words(MAX_URL),
        _ => words(MAX_TEXT),
    }
}

/// A type's triangles as a mesh, each corner a point of the model's space; why not, when they are none.
fn mesh(triangles: &Value) -> std::result::Result<Vec<[[f64; 3]; 3]>, String> {
    let Some(triangles) = triangles.as_array() else { return Err("its `triangles` is no list".into()) };
    let point = |p: &Value| -> Option<[f64; 3]> {
        let p = p.as_array().filter(|p| p.len() == 3)?;
        let mut out = [0.0; 3];
        for (o, c) in out.iter_mut().zip(p) {
            *o = c.as_f64().filter(|x| x.is_finite() && x.abs() <= MAX_COORD)?;
        }
        Some(out)
    };
    triangles
        .iter()
        .map(|t| {
            let t = t.as_array().filter(|t| t.len() == 3)?;
            Some([point(&t[0])?, point(&t[1])?, point(&t[2])?])
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "a triangle of it is not three points of three numbers within the model's space".to_string())
}

/// Whether a family of `category` with these parameters, types and forms passes the family's own rules.
fn passes(category: &str, parameters: &[Value], types: &Map<String, Value>, forms: &[Value]) -> std::result::Result<(), String> {
    let family: Family = serde_json::from_value(json!({"category": category, "parameters": parameters, "types": types, "forms": forms}))
        .map_err(|e| e.to_string())?;
    family.check(&|_| true)
}

fn cut(text: &str, most: usize) -> String {
    text.chars().take(most).collect()
}

/// The family in an export of the bridge, named as the export says, else `file_name`.
pub fn exchange_family(bytes: &[u8], file_name: &str) -> Result<FamilyFile> {
    let not = |why: String| Error::Format(format!("is not a family exported by tools/revit_bridge: {why}"));
    if bytes.len() > MAX_EXPORT {
        return Err(not(format!("it is {} bytes, over the limit of {MAX_EXPORT}", bytes.len())));
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|e| not(format!("it is not JSON ({e})")))?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(not(format!("its `format` is not `{FORMAT}`")));
    }
    match v.get("version").and_then(Value::as_u64) {
        Some(n) if (1..=VERSION).contains(&n) => {}
        Some(n) => {
            return Err(Error::Format(format!(
                "is version {n} of the bridge's export, newer than this BIMCraft reads ({VERSION}): update BIMCraft, or export with the bridge it came with"
            )));
        }
        None => return Err(not("it has no `version`".into())),
    }
    let name = v.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).unwrap_or(file_name);
    let name = cut(name.trim(), MAX_TEXT).trim().to_string();
    let name = if name.is_empty() { "Family".to_string() } else { name };

    let mut notes: Vec<String> = Vec::new();
    // Groups of names left out for one reason, in the order met.
    let mut said: Vec<(String, Vec<String>)> = Vec::new();
    let mut say = |why: String, who: &str| match said.iter_mut().find(|(w, _)| *w == why) {
        Some((_, names)) => names.push(who.to_string()),
        None => said.push((why, vec![who.to_string()])),
    };

    // The types: an object with a name, the first of that name.
    let empty = Vec::new();
    let mut parts: Vec<(String, &Value)> = Vec::new();
    for t in v.get("types").and_then(Value::as_array).unwrap_or(&empty) {
        let title = t.get("name").and_then(Value::as_str).unwrap_or("").trim();
        if title.is_empty() || title.chars().count() > MAX_TEXT {
            say(format!("type left out, a type needs a name of up to {MAX_TEXT} characters"), title);
        } else if parts.iter().any(|(n, _)| n == title) {
            say("type left out, another type of the export has its name".into(), title);
        } else if parts.len() >= MAX_TYPES {
            say(format!("type left out, a family holds {MAX_TYPES} types"), title);
        } else {
            parts.push((title.to_string(), t));
        }
    }
    let mut types: Map<String, Value> = parts.iter().map(|(t, _)| (t.clone(), json!({}))).collect();
    if types.is_empty() {
        notes.push(format!("The export lists no type: one was made, `{name}`, since a family needs a type"));
        types.insert(name.clone(), json!({}));
    }

    // The geometry: a mesh for each type that has one, or none at all where a type's is no mesh.
    let mut meshes: Vec<(usize, Vec<[[f64; 3]; 3]>)> = Vec::new();
    let mut no_geometry: Option<String> = None;
    for (at, (title, t)) in parts.iter().enumerate() {
        match t.get("triangles").map(mesh) {
            None => {}
            Some(Ok(m)) if m.is_empty() => {}
            Some(Ok(m)) => meshes.push((at, m)),
            Some(Err(why)) => {
                no_geometry.get_or_insert(format!("The geometry is left out, type `{}`: {why}", cut(title, 60)));
            }
        }
    }
    if no_geometry.is_some() {
        meshes.clear();
    }
    // More than one type: a number says which type's mesh a component shows, and takes a parameter's place.
    let selects = parts.len() > 1 && !meshes.is_empty();
    let room = MAX_PARAMETERS - usize::from(selects);

    // The parameters, each tried against the family's rules as it is added.
    let mut parameters: Vec<Value> = Vec::new();
    // What each was called in the export, where it is named otherwise here, for the formulas that say it.
    let mut renamed: Vec<(String, String)> = Vec::new();
    let mut formulas: Vec<(String, String)> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    let mut over: Vec<String> = Vec::new();
    let mut instances = 0;
    for p in v.get("parameters").and_then(Value::as_array).unwrap_or(&empty) {
        let Some(given) = p.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()) else {
            say("parameter left out, it has no name".into(), "");
            continue;
        };
        let kind = match p.get("kind").and_then(Value::as_str).unwrap_or("") {
            k @ ("length" | "area" | "volume" | "angle" | "number" | "integer" | "yesno" | "text" | "url") => k,
            // A material's name: the export holds no material, so it is text.
            "material" => "text",
            "other" => "number",
            other => {
                say(format!("parameter left out, its kind `{}` is none BIMCraft has", cut(other, 40)), given);
                continue;
            }
        };
        let own = if nameable(given) { given.to_string() } else { named(given) };
        if !nameable(&own) || own == SELECTOR || parameters.iter().any(|q| q["name"] == own.as_str()) {
            say("parameter left out, its name is another's once written as a name here is".into(), given);
            continue;
        }
        // Each type's value of it, as the model holds one of its kind.
        let mut values: BTreeMap<&str, Value> = BTreeMap::new();
        let mut refused = None;
        for (title, t) in &parts {
            match t.get("values").and_then(|vs| vs.get(given)) {
                None | Some(Value::Null) => {}
                Some(raw) => match value(kind, raw) {
                    Ok(v) => {
                        values.insert(title, v);
                    }
                    Err(why) => refused = Some(why),
                },
            }
        }
        if let Some(why) = refused {
            say(format!("parameter left out, {why}"), given);
            continue;
        }
        if parameters.len() >= room {
            over.push(given.to_string());
            continue;
        }
        let instance = p.get("instance").and_then(Value::as_bool).unwrap_or(false);
        let first = parts.iter().find_map(|(t, _)| values.get(t.as_str())).cloned().unwrap_or(Value::Null);
        let mut made = json!({"name": own, "kind": kind, "value": first});
        if instance {
            made["instance"] = json!(true);
        }
        if let Some(guid) = p.get("shared").and_then(Value::as_str).filter(|g| is_guid(g)) {
            made["shared"] = json!(guid);
        }
        if let Some(group) = p.get("group").and_then(Value::as_str).filter(|g| GROUPS.contains(g)) {
            made["group"] = json!(group);
        }
        let mut trial = types.clone();
        if !instance {
            for (t, v) in &values {
                if let Some(Value::Object(set)) = trial.get_mut(*t) {
                    set.insert(own.clone(), v.clone());
                }
            }
        }
        // A reporting parameter reads a dimension, which a mesh has none of: where the rules refuse it as one, it is its value.
        let reporting = p.get("reporting").and_then(Value::as_bool).unwrap_or(false);
        let mut tries = vec![made.clone()];
        if reporting {
            let mut with = made.clone();
            with["reporting"] = json!(true);
            tries.insert(0, with);
        }
        let mut why = String::new();
        let mut added = false;
        for candidate in tries {
            // A reporting parameter is set by no type.
            let set_by_types = candidate.get("reporting").is_none();
            parameters.push(candidate);
            match passes("generic", &parameters, if set_by_types { &trial } else { &types }, &[]) {
                Ok(()) => {
                    if set_by_types {
                        types = trial.clone();
                    }
                    added = true;
                    break;
                }
                Err(e) => {
                    parameters.pop();
                    why = cut(&e, 200);
                }
            }
        }
        if !added {
            say(format!("parameter left out, {why}"), given);
            continue;
        }
        instances += usize::from(instance);
        if own != given {
            renamed.push((given.to_string(), own.clone()));
        }
        if p.get("kind").and_then(Value::as_str) == Some("other") {
            let unit = p.get("unit").and_then(Value::as_str).map(|u| cut(u, 40)).filter(|u| !u.trim().is_empty());
            unknown.push(format!("{given} ({})", unit.unwrap_or_else(|| "no unit".into())));
        }
        if let Some(formula) = p.get("formula").and_then(Value::as_str).filter(|f| !f.trim().is_empty()) {
            formulas.push((own, formula.to_string()));
        }
    }

    // The formulas, once every parameter they may name is there: each kept as written where the rules take it, and then
    // no type sets its parameter; where they do not, the parameter keeps each type's value.
    let mut refused_formulas: Vec<String> = Vec::new();
    let mut kept_formulas = 0;
    for (own, formula) in &formulas {
        let written = renamed.iter().fold(formula.clone(), |f, (from, to)| renamed_in_formula(&f, from, to));
        let Some(at) = parameters.iter().position(|p| p["name"] == own.as_str()) else { continue };
        let mut trial_parameters = parameters.clone();
        trial_parameters[at]["formula"] = json!(written);
        if let Some(p) = trial_parameters[at].as_object_mut() {
            p.remove("reporting");
        }
        let mut trial = types.clone();
        for set in trial.values_mut().filter_map(Value::as_object_mut) {
            set.remove(own);
        }
        match passes("generic", &trial_parameters, &trial, &[]) {
            Ok(()) => {
                (parameters, types) = (trial_parameters, trial);
                kept_formulas += 1;
            }
            Err(_) => refused_formulas.push(format!("{own} = {}", cut(formula, 80))),
        }
    }
    let read_parameters = parameters.len();

    // The geometry: one mesh, or one for each type, shown where the selector says the type is.
    let mut forms: Vec<Value> = Vec::new();
    if !meshes.is_empty() {
        let mut trial_parameters = parameters.clone();
        let mut trial = types.clone();
        if selects {
            trial_parameters.push(json!({"name": SELECTOR, "kind": "number", "value": 0.0}));
            for (at, (title, _)) in parts.iter().enumerate() {
                if let Some(Value::Object(set)) = trial.get_mut(title) {
                    set.insert(SELECTOR.into(), json!(at as f64));
                }
            }
        }
        let trial_forms: Vec<Value> = meshes
            .iter()
            .map(|(at, m)| if selects { json!({"mesh": m, "visible": format!("{SELECTOR} = {at}")}) } else { json!({"mesh": m}) })
            .collect();
        match passes("generic", &trial_parameters, &trial, &trial_forms) {
            Ok(()) => (parameters, types, forms) = (trial_parameters, trial, trial_forms),
            Err(why) => no_geometry = Some(format!("The geometry is left out: {}", cut(&why, 200))),
        }
    }

    // The category: Revit's built-in name of it, where BIMCraft has one and the family passes as one of it.
    let ost = cut(v.get("category").and_then(Value::as_str).unwrap_or("").trim(), 60);
    let category = match category(&ost) {
        Some(c) => match passes(c, &parameters, &types, &forms) {
            Ok(()) => c,
            Err(why) => {
                notes.push(format!("Its category is `{ost}`, and it came in as a generic family: {}", cut(&why, 200)));
                "generic"
            }
        },
        None if ost.is_empty() => {
            notes.push("The export names no category: it came in as a generic family".into());
            "generic"
        }
        None => {
            notes.push(format!("Its category is `{ost}`, which BIMCraft has no family category for: it came in as a generic family"));
            "generic"
        }
    };

    let mut options = Map::new();
    for key in ["work_plane_based", "always_vertical", "cut_with_voids_when_loaded", "shared"] {
        if let Some(b) = v.get("options").and_then(|o| o.get(key)).and_then(Value::as_bool) {
            options.insert(key.into(), json!(b));
        }
    }

    for (why, names) in &said {
        let (what, why) = why.split_once(' ').unwrap_or((why, ""));
        notes.push(format!("{} {why}: {}", count(names.len(), what), listed(names)));
    }
    if !over.is_empty() {
        notes.push(format!("{} left out, a family holds {MAX_PARAMETERS} parameters: {}", count(over.len(), "parameter"), listed(&over)));
    }
    if !refused_formulas.is_empty() {
        notes.push(format!(
            "{} left out, BIMCraft does not read them as written, and their parameters keep each type's value: {}",
            count(refused_formulas.len(), "formula"),
            listed(&refused_formulas)
        ));
    }
    if !renamed.is_empty() {
        let pairs: Vec<String> = renamed.iter().map(|(from, to)| format!("{from} is {to}")).collect();
        notes.push(format!(
            "{} renamed, a name here holds letters, digits, `_` and spaces: {}",
            count(pairs.len(), "parameter"),
            listed(&pairs)
        ));
    }
    if !unknown.is_empty() {
        notes.push(format!(
            "{} of data types BIMCraft has no kind for came in as numbers in the family's units: {}",
            count(unknown.len(), "parameter"),
            listed(&unknown)
        ));
    }
    match &no_geometry {
        Some(why) => notes.push(why.clone()),
        None if forms.is_empty() => notes.push("The export holds no geometry: the family came in with its parameters only".into()),
        None if forms.len() < parts.len() => {
            notes.push(format!("{} of its {} types have geometry: a component of the others shows none", forms.len(), parts.len()));
        }
        None => {}
    }
    // What the bridge itself said it could not export.
    for note in v.get("notes").and_then(Value::as_array).unwrap_or(&empty).iter().filter_map(Value::as_str).take(100) {
        notes.push(cut(note, 500));
    }
    notes.push(
        "The geometry is a mesh of each type as Revit built it, not its forms: changing a parameter does not reshape it, and a \
         formula is kept as written, so read it over. Reference planes, dimensions and constraints, connectors, materials, \
         nested families and plan symbols are not converted"
            .into(),
    );
    let formulas_said = format!("{}, {}", count(instances, "instance").replace("instances", "instance"), count(kept_formulas, "formula"));
    let geometry = if forms.is_empty() { String::new() } else { format!(" and the geometry of {}", count(forms.len(), "type")) };
    notes.insert(
        0,
        format!(
            "{}, {} ({formulas_said}){geometry} read from the bridge's export",
            count(parts.len(), "type"),
            count(read_parameters, "parameter")
        ),
    );

    let mut family = json!({"name": name, "category": category, "parameters": parameters, "types": types});
    if !forms.is_empty() {
        family["forms"] = Value::Array(forms);
    }
    if !options.is_empty() {
        family["options"] = Value::Object(options);
    }
    Ok(FamilyFile { name, family, notes })
}

/// The family in the bridge's export at `path`, named as the export says, else after the file.
pub fn exchange_family_path(path: &std::path::Path) -> std::result::Result<FamilyFile, String> {
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_EXPORT as u64 {
        return Err(format!("is not a family exported by tools/revit_bridge: it is {size} bytes, over the limit of {MAX_EXPORT}"));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    // `Bench.rfa.json` is named Bench.
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let stem = stem.strip_suffix(".rfa").unwrap_or(&stem).to_string();
    exchange_family(&bytes, &stem).map_err(|e| e.to_string())
}
