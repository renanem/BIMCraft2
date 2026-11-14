//! A Revit family file (`.rfa`) as a BIMCraft family: its types and their parameters, read from its `PartAtom` entry.
//!
//! The entry is the Atom document Revit keeps in every family file, and the one its public API writes out
//! (`Application.ExtractPartAtomFromFamilyFile`): the family's category and options, and for each type its type
//! parameters, each with its name, data type, unit, value and, shared, its GUID. That is all of a family this reader
//! takes: its geometry, instance parameters, formulas and connectors are in the file's partitions, which have no reader
//! here, so the notes say they are missing and that the bridge through Revit's API (`tools/revit_bridge`) brings them.
//!
//! What comes back is the params of the `family` command, checked here against the family's own rules
//! (`Family::check`), so loading it never fails on what the file said: a parameter or a type the rules refuse is left
//! out, by name, with the rule.

use std::collections::BTreeMap;
use std::path::Path;

use bimcraft_core::family::{Family, MAX_PARAMETERS, MAX_TYPES, MAX_URL, nameable};
use bimcraft_core::model::{MAX_COORD, MAX_TEXT};
use serde_json::{Map, Value, json};

use crate::atom::{Event, attribute, scan};
use crate::error::{Error, Result};

/// A family read from a file: the params of the `family` command that loads it, and what was left out and why.
#[derive(Debug, Clone, PartialEq)]
pub struct FamilyFile {
    pub name: String,
    /// What `Session::execute("family", …)` takes: `name`, `category`, `parameters`, `types` and `options`.
    pub family: Value,
    /// First what was read, then each thing left out or changed, as an import's notes are.
    pub notes: Vec<String>,
}

/// The most names a note lists; it says how many there were.
const MAX_LISTED: usize = 12;

/// A type parameter's element under a part, or an option's under the family's features, as the entry holds it.
#[derive(Debug, Clone, Default)]
struct Raw {
    /// The name Revit shows: `displayName`, else the element's own.
    name: String,
    /// `typeOfParameter`: Revit's name of the data type, none for a choice among words (a host, a part type).
    data_type: Option<String>,
    units: Option<String>,
    /// `type="shared"` with `id`, a GUID.
    shared: Option<String>,
    text: String,
}

/// The entry, as far as a family is read from it.
#[derive(Debug, Default)]
struct Entry {
    category: Option<String>,
    options: Vec<Raw>,
    /// Each type: its title and its parameters, in the entry's order.
    parts: Vec<(String, Vec<Raw>)>,
}

fn entry(xml: &[u8]) -> Result<Entry> {
    let mut e = Entry::default();
    // The element being read, opened under a part or under a group of the features.
    let mut open: Option<Raw> = None;
    scan(xml, &mut |event| match event {
        Event::Open { name, tag, parents, empty } => {
            let raw = || Raw {
                name: attribute(tag, "displayName").filter(|n| !n.trim().is_empty()).unwrap_or_else(|| name.to_string()),
                data_type: attribute(tag, "typeOfParameter"),
                units: attribute(tag, "units"),
                shared: attribute(tag, "type").filter(|t| t == "shared").and_then(|_| attribute(tag, "id")),
                text: String::new(),
            };
            match parents {
                ["entry"] if name == "category" => {
                    if let Some(term) = attribute(tag, "term") {
                        e.category.get_or_insert(term);
                    }
                }
                ["entry", "A:family"] if name == "A:part" && !empty => e.parts.push((String::new(), Vec::new())),
                ["entry", "A:family", "A:part"] if name != "title" && !empty => open = Some(raw()),
                ["entry", "A:features", "A:feature", "A:group"] if name != "A:title" && !empty => open = Some(raw()),
                _ => {}
            }
        }
        Event::Close { name, text, parents } => match parents {
            ["entry", "category"] if name == "term" => {
                e.category.get_or_insert_with(|| text.to_string());
            }
            ["entry", "A:family", "A:part"] => {
                let Some((title, parameters)) = e.parts.last_mut() else { return };
                if name == "title" {
                    *title = text.to_string();
                } else if let Some(mut raw) = open.take() {
                    raw.text = text.to_string();
                    parameters.push(raw);
                }
            }
            ["entry", "A:features", "A:feature", "A:group"] => {
                if let Some(mut raw) = open.take() {
                    raw.text = text.to_string();
                    e.options.push(raw);
                }
            }
            _ => {}
        },
    })?;
    Ok(e)
}

/// The BIMCraft category of a family whose Revit category is shown as `label`, in English: the names of Revit's
/// categories as its interface and its public API's `BuiltInCategory` documentation give them.
fn category(label: &str) -> Option<&'static str> {
    Some(match label {
        "Furniture" | "Furniture Systems" => "furniture",
        "Casework" => "casework",
        "Plumbing Fixtures" => "plumbing",
        "Pipe Fittings" => "pipefitting",
        "Pipe Accessories" => "pipeaccessory",
        "Electrical Equipment" => "equipment",
        "Electrical Fixtures" => "outlet",
        "Lighting Devices" => "switch",
        "Lighting Fixtures" => "lighting",
        "Air Terminals" => "airterminal",
        "Data Devices" => "dataoutlet",
        "Telephone Devices" => "voiceoutlet",
        "Doors" => "door",
        "Windows" => "window",
        "Generic Models" => "generic",
        _ => return None,
    })
}

/// How a value of Revit's data type `data_type` is read: its kind here, and the factor from the file's `units` to the
/// model's (millimetres, mm², mm³, degrees).
enum Reading {
    Number {
        kind: &'static str,
        factor: f64,
    },
    Whole,
    YesNo,
    /// Yes or no in the language the file was saved in: the word that is yes.
    YesIs(String),
    /// Text of at most this many characters.
    Words {
        kind: &'static str,
        most: usize,
    },
    /// A data type with no kind here: a number in the file's unit where its value is one, else text.
    Unknown,
}

/// Revit names a data type in the language the file was saved in (`Length`, `Comprimento`, `Länge`), so only the English
/// names are told by name: any other is told by its unit (a unit of length makes a length, whatever it is called), and a
/// yes or no by its name being its two answers (`Sim/Não`) and `text` one of them.
fn reading(data_type: Option<&str>, units: Option<&str>, text: &str) -> std::result::Result<Reading, String> {
    let unit = units.unwrap_or("").trim();
    // The unit's symbol without its power: `m` of `m²`, `m2` and `m^3`.
    let base = unit.trim_end_matches(['²', '³', '2', '3', '^']);
    let scale = |power: i32, what: &str| -> std::result::Result<f64, String> {
        let mm = millimetres(base).ok_or_else(|| format!("`{unit}` is no unit of {what} this reader knows"))?;
        Ok(mm.powi(power))
    };
    Ok(match data_type {
        Some("Length") => Reading::Number { kind: "length", factor: scale(1, "length")? },
        Some("Area") => Reading::Number { kind: "area", factor: scale(2, "area")? },
        Some("Volume") => Reading::Number { kind: "volume", factor: scale(3, "volume")? },
        Some("Angle") => Reading::Number {
            kind: "angle",
            factor: match unit {
                "°" | "deg" | "" => 1.0,
                "rad" => 180.0 / std::f64::consts::PI,
                "grad" | "gon" => 0.9,
                _ => return Err(format!("`{unit}` is no unit of angle this reader knows")),
            },
        },
        Some("Number") => Reading::Number { kind: "number", factor: 1.0 },
        Some("Integer") => Reading::Whole,
        Some("Yes/No") => Reading::YesNo,
        Some("Text") | Some("Multiline Text") => Reading::Words { kind: "text", most: MAX_TEXT },
        // A material's name: the file holds no material, so it is text.
        Some("Material") => Reading::Words { kind: "text", most: MAX_TEXT },
        Some("URL") => Reading::Words { kind: "url", most: MAX_URL },
        // A choice among Revit's own words (a host, a part type) has no data type: text.
        None => Reading::Words { kind: "text", most: MAX_TEXT },
        Some(other) => {
            let answers = other.split_once('/').filter(|(yes, no)| unit.is_empty() && (text.trim() == *yes || text.trim() == *no));
            if let Some(mm) = millimetres(unit) {
                Reading::Number { kind: "length", factor: mm }
            } else if let (true, Some(mm)) = (unit.ends_with(['²', '2']), millimetres(base)) {
                Reading::Number { kind: "area", factor: mm.powi(2) }
            } else if let (true, Some(mm)) = (unit.ends_with(['³', '3']), millimetres(base)) {
                Reading::Number { kind: "volume", factor: mm.powi(3) }
            } else if unit == "°" {
                Reading::Number { kind: "angle", factor: 1.0 }
            } else if let Some((yes, _)) = answers {
                Reading::YesIs(yes.to_string())
            } else {
                Reading::Unknown
            }
        }
    })
}

fn number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|x| x.is_finite())
}

/// Whether a data type is a length: named so in English, or measured in a unit of length.
fn length_like(data_type: Option<&str>, units: Option<&str>) -> bool {
    data_type == Some("Length") || millimetres(units.unwrap_or("").trim()).is_some()
}

/// Millimetres per `unit`, a length's symbol.
fn millimetres(unit: &str) -> Option<f64> {
    Some(match unit {
        "mm" | "мм" => 1.0,
        "cm" | "см" => 10.0,
        "dm" | "дм" => 100.0,
        "m" | "м" => 1000.0,
        "in" | "\"" => 25.4,
        "ft" | "'" => 304.8,
        _ => return None,
    })
}

/// A length whose text names its unit, in millimetres: a number and a unit's symbol (`4 cm`), or feet and inches as
/// Revit writes them where a project shows fractional inches (`1' - 10"`, `0' - 1 1/2"`, `6 1/4"`, `3'`).
fn written_length(text: &str) -> Option<f64> {
    let text = text.trim();
    if let Some(at) = text.find(|c: char| c.is_alphabetic()) {
        let (x, unit) = text.split_at(at);
        return Some(number(x)? * millimetres(unit.trim())?);
    }
    if !text.ends_with(['\'', '"']) {
        return None;
    }
    let (negative, text) = match text.strip_prefix('-') {
        Some(rest) => (true, rest.trim_start()),
        None => (false, text),
    };
    let (feet, inches) = match text.split_once('\'') {
        Some((feet, rest)) => (number(feet)?, rest.trim().trim_start_matches('-').trim()),
        None => (0.0, text),
    };
    let inches = match inches.strip_suffix('"') {
        // Whole inches, a fraction, or both: `10`, `1/2`, `1 1/2`.
        Some(inches) => inches.split_whitespace().try_fold(0.0, |sum, part| {
            let x = match part.split_once('/') {
                Some((n, d)) => number(n)? / number(d).filter(|d| *d != 0.0)?,
                None => number(part)?,
            };
            Some(sum + x)
        })?,
        None if inches.is_empty() => 0.0,
        None => return None,
    };
    let mm = feet * 304.8 + inches * 25.4;
    Some(if negative { -mm } else { mm }).filter(|x| x.is_finite())
}

fn yes_no(text: &str) -> Option<bool> {
    match text.trim() {
        "Yes" | "yes" | "1" | "true" | "True" => Some(true),
        "No" | "no" | "0" | "false" | "False" => Some(false),
        _ => None,
    }
}

fn words(text: &str, most: usize) -> std::result::Result<Value, String> {
    if text.chars().count() > most {
        return Err(format!("its text is longer than {most} characters"));
    }
    if text.chars().any(char::is_control) {
        return Err("its text runs over more than one line".into());
    }
    Ok(json!(text))
}

/// `raw`'s value as the model holds one of its kind, and the kind.
fn value(raw: &Raw) -> std::result::Result<(&'static str, Value), String> {
    let shown: String = raw.text.chars().take(40).collect();
    let (data_type, units) = (raw.data_type.as_deref(), raw.units.as_deref());
    // A length written with its unit, or in feet and inches, says its unit itself, whatever the element's `units` say.
    let written = written_length(&raw.text);
    let read = match (reading(data_type, units, &raw.text), written) {
        (Ok(Reading::Number { kind: "length", .. }) | Err(_), Some(mm)) if data_type.is_some() && length_like(data_type, units) => {
            return if mm.abs() <= MAX_COORD { Ok(("length", json!(mm))) } else { Err(format!("`{shown}` is no length the model holds")) };
        }
        (read, _) => read?,
    };
    match read {
        Reading::Number { kind, factor } => {
            let x = number(&raw.text).map(|x| x * factor).filter(|x| x.is_finite());
            // A length is a place in the model's space; an area or a volume, one's square or cube.
            let most = match kind {
                "length" => MAX_COORD,
                "area" => MAX_COORD * MAX_COORD,
                _ => f64::MAX,
            };
            match x {
                Some(x) if x.abs() <= most => Ok((kind, json!(x))),
                _ => Err(format!("`{shown}` is no {kind} the model holds")),
            }
        }
        Reading::Whole => number(&raw.text)
            .filter(|x| x.fract() == 0.0 && x.abs() <= bimcraft_core::family::MAX_INTEGER)
            .map(|x| ("integer", json!(x)))
            .ok_or_else(|| format!("`{shown}` is no whole number")),
        Reading::YesNo => yes_no(&raw.text).map(|b| ("yesno", json!(b))).ok_or_else(|| format!("`{shown}` is neither Yes nor No")),
        Reading::YesIs(yes) => Ok(("yesno", json!(raw.text.trim() == yes))),
        Reading::Words { kind, most } => words(&raw.text, most).map(|v| (kind, v)),
        Reading::Unknown => match number(&raw.text) {
            Some(x) => Ok(("number", json!(x))),
            None => words(&raw.text, MAX_TEXT).map(|v| ("text", v)),
        },
    }
}

/// A name the family's rules take (`nameable`), made of `name`: what they refuse becomes `_`, and it is cut to their length.
fn named(name: &str) -> String {
    let mut out: String =
        name.trim().chars().take(MAX_TEXT).map(|c| if c.is_alphanumeric() || c == '_' || c == ' ' { c } else { '_' }).collect();
    if !out.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
        out.insert(0, '_');
        out = out.chars().take(MAX_TEXT).collect();
    }
    out.trim_end().to_string()
}

/// `names` as a note lists them: the first few, in backticks, and how many more.
fn listed(names: &[String]) -> String {
    let cut = |n: &String| -> String {
        let short: String = n.chars().take(60).collect();
        format!("`{short}`")
    };
    let mut out = names.iter().take(MAX_LISTED).map(cut).collect::<Vec<_>>().join(", ");
    if names.len() > MAX_LISTED {
        out.push_str(&format!(" and {} more", names.len() - MAX_LISTED));
    }
    out
}

fn count(n: usize, one: &str) -> String {
    format!("{n} {one}{}", if n == 1 { "" } else { "s" })
}

/// Whether a family of `category` with these parameters and types passes the family's own rules.
fn passes(category: &str, parameters: &[Value], types: &Map<String, Value>) -> std::result::Result<(), String> {
    let family: Family =
        serde_json::from_value(json!({"category": category, "parameters": parameters, "types": types})).map_err(|e| e.to_string())?;
    family.check(&|_| true)
}

/// The family in a `PartAtom` entry's XML, named `name`.
pub fn family(xml: &[u8], name: &str) -> Result<FamilyFile> {
    let e = entry(xml)?;
    let name: String = name.trim().chars().take(MAX_TEXT).collect();
    let name = if name.is_empty() { "Family".to_string() } else { name };
    let mut notes = Vec::new();
    // Groups of names left out or changed for one reason, in the order met.
    let mut said: Vec<(String, Vec<String>)> = Vec::new();
    let mut say = |why: String, who: &str| match said.iter_mut().find(|(w, _)| *w == why) {
        Some((_, names)) => names.push(who.to_string()),
        None => said.push((why, vec![who.to_string()])),
    };

    // The types: the first of a name is the type; one with no name, or one too many, is left out.
    let mut parts: Vec<(String, &Vec<Raw>)> = Vec::new();
    for (title, raws) in &e.parts {
        let title = title.trim();
        if title.is_empty() || title.chars().count() > MAX_TEXT {
            say(format!("type left out, a type needs a name of up to {MAX_TEXT} characters"), title);
        } else if parts.iter().any(|(t, _)| t == title) {
            say("type left out, another type of the file has its name".into(), title);
        } else if parts.len() >= MAX_TYPES {
            say(format!("type left out, a family holds {MAX_TYPES} types"), title);
        } else {
            parts.push((title.to_string(), raws));
        }
    }

    // Default Elevation is the family's here, not a type's: the first type's, said where the types differ.
    const ELEVATION: &str = "Default Elevation";
    let elevations: Vec<f64> = parts
        .iter()
        .filter_map(|(_, raws)| raws.iter().find(|r| r.name == ELEVATION))
        .filter_map(|r| value(r).ok().filter(|(kind, _)| *kind == "length").and_then(|(_, v)| v.as_f64()))
        .collect();
    let elevation = elevations.first().copied();

    // The parameters, in the order the types name them: each one's kind and each type's value.
    struct Read {
        name: String,
        kind: &'static str,
        shared: Option<String>,
        values: BTreeMap<String, Value>,
    }
    let mut read: Vec<Read> = Vec::new();
    let mut refused: Vec<String> = Vec::new();
    let mut unknown: Vec<String> = Vec::new();
    for (title, raws) in &parts {
        for raw in raws.iter().filter(|r| r.name != ELEVATION) {
            if refused.contains(&raw.name) {
                continue;
            }
            let found = read.iter().position(|r| r.name == raw.name);
            match (value(raw), found) {
                (Ok((kind, v)), Some(at)) if read[at].kind == kind => {
                    read[at].values.entry(title.clone()).or_insert(v);
                }
                (Ok((kind, v)), None) => {
                    // A number of a data type not known is in the file's unit, which the note names; words are text.
                    let unknown_type = matches!(reading(raw.data_type.as_deref(), raw.units.as_deref(), &raw.text), Ok(Reading::Unknown));
                    if unknown_type && kind == "number" {
                        let unit = raw.units.as_deref().map(|u| format!(", {u}")).unwrap_or_default();
                        unknown.push(format!("{} ({}{unit})", raw.name, raw.data_type.as_deref().unwrap_or("")));
                    }
                    let shared = raw.shared.clone().filter(|g| is_guid(g));
                    read.push(Read { name: raw.name.clone(), kind, shared, values: BTreeMap::from([(title.clone(), v)]) });
                }
                (Ok(_), Some(at)) => {
                    read.remove(at);
                    refused.push(raw.name.clone());
                    say("parameter left out, its types hold values of different kinds".into(), &raw.name);
                }
                (Err(why), at) => {
                    if let Some(at) = at {
                        read.remove(at);
                    }
                    refused.push(raw.name.clone());
                    say(format!("parameter left out, {why}"), &raw.name);
                }
            }
        }
    }

    // Named as the family's rules allow, and no two alike.
    let mut parameters: Vec<Value> = Vec::new();
    let mut types: Map<String, Value> = parts.iter().map(|(t, _)| (t.clone(), json!({}))).collect();
    if types.is_empty() {
        notes.push(format!("The file lists no type: one was made, `{name}`, since a family needs a type"));
        types.insert(name.clone(), json!({}));
    }
    let mut renamed: Vec<String> = Vec::new();
    let mut over: Vec<String> = Vec::new();
    for r in read {
        let own = if nameable(&r.name) { r.name.clone() } else { named(&r.name) };
        if !nameable(&own) || parameters.iter().any(|p| p["name"] == own.as_str()) {
            say("parameter left out, its name is another's once written as a name here is".into(), &r.name);
            continue;
        }
        if parameters.len() >= MAX_PARAMETERS {
            over.push(r.name);
            continue;
        }
        let first = parts.iter().find_map(|(t, _)| r.values.get(t)).cloned().unwrap_or(Value::Null);
        let mut p = json!({"name": own, "kind": r.kind, "value": first});
        if let Some(guid) = &r.shared {
            p["shared"] = json!(guid);
        }
        // The family's own rules have the last word: what they refuse of this parameter leaves it out.
        let mut trial_types = types.clone();
        for (t, v) in &r.values {
            if let Some(Value::Object(set)) = trial_types.get_mut(t) {
                set.insert(own.clone(), v.clone());
            }
        }
        parameters.push(p);
        match passes("generic", &parameters, &trial_types) {
            Ok(()) => {
                types = trial_types;
                if own != r.name {
                    renamed.push(format!("{} is {own}", r.name));
                }
            }
            Err(why) => {
                parameters.pop();
                let why: String = why.chars().take(200).collect();
                say(format!("parameter left out, {why}"), &r.name);
            }
        }
    }

    // The category: Revit's name of it in English, where BIMCraft has one and the family passes as one of it.
    let label = e.category.as_deref().unwrap_or("").trim();
    let shown: String = label.chars().take(60).collect();
    let category = match category(label) {
        Some(c) => match passes(c, &parameters, &types) {
            Ok(()) => c,
            Err(why) => {
                let why: String = why.chars().take(200).collect();
                notes.push(format!("Its category is `{shown}`, and it came in as a generic family: {why}"));
                "generic"
            }
        },
        None if label.is_empty() => {
            notes.push("The file names no category: it came in as a generic family".into());
            "generic"
        }
        None => {
            notes.push(format!(
                "Its category is `{shown}`, which BIMCraft has no family category for (or names in another language than \
                 English): it came in as a generic family"
            ));
            "generic"
        }
    };

    // The family's options, where the entry gives them in English as Yes or No.
    let mut options = Map::new();
    for raw in &e.options {
        let key = match raw.name.as_str() {
            "Work Plane-Based" => "work_plane_based",
            "Always vertical" => "always_vertical",
            "Cut with Voids When Loaded" => "cut_with_voids_when_loaded",
            "Shared" => "shared",
            _ => continue,
        };
        if let Some(b) = yes_no(&raw.text) {
            options.insert(key.into(), json!(b));
        }
    }
    if let Some(mm) = elevation.filter(|mm| mm.abs() <= MAX_COORD) {
        options.insert("default_elevation".into(), json!(mm));
        if elevations.iter().any(|x| (x - mm).abs() > 1e-6) {
            notes.push(format!(
                "Default Elevation differs between its types and is the family's here: the first type's, {mm} mm, was taken"
            ));
        }
    }

    for (why, names) in &said {
        let (what, why) = why.split_once(' ').unwrap_or((why, ""));
        notes.push(format!("{} {why}: {}", count(names.len(), what), listed(names)));
    }
    if !over.is_empty() {
        notes.push(format!("{} left out, a family holds {MAX_PARAMETERS} parameters: {}", count(over.len(), "parameter"), listed(&over)));
    }
    if !renamed.is_empty() {
        notes.push(format!(
            "{} renamed, a name here holds letters, digits, `_` and spaces: {}",
            count(renamed.len(), "parameter"),
            listed(&renamed)
        ));
    }
    if !unknown.is_empty() {
        notes.push(format!(
            "{} of data types BIMCraft has no kind for came in as numbers in the file's units: {}",
            count(unknown.len(), "parameter"),
            listed(&unknown)
        ));
    }
    notes.push(
        "A family file is read for its types and their parameters only: its geometry, reference planes and dimensions, its \
         instance parameters, formulas, connectors, materials, nested families and plan symbols are not read, and a \
         material a parameter names came in as text. Export the family from Revit with tools/revit_bridge for those"
            .into(),
    );
    notes.insert(
        0,
        format!("{} and {} read from the file's PartAtom entry", count(parts.len(), "type"), count(parameters.len(), "parameter")),
    );

    let mut family = json!({"name": name, "category": category, "parameters": parameters, "types": types});
    if !options.is_empty() {
        family["options"] = Value::Object(options);
    }
    Ok(FamilyFile { name, family, notes })
}

/// Whether `text` is written as a GUID: 8-4-4-4-12 hexadecimal digits.
fn is_guid(text: &str) -> bool {
    let groups: Vec<&str> = text.split('-').collect();
    groups.len() == 5 && groups.iter().zip([8, 4, 4, 4, 12]).all(|(g, n)| g.len() == n && g.chars().all(|c| c.is_ascii_hexdigit()))
}

/// The family in the Revit family file at `path`, named after the file.
pub fn family_path(path: &Path) -> std::result::Result<FamilyFile, String> {
    let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut revit = crate::Revit::open(path, crate::Limits::default()).map_err(|e| e.to_string())?;
    revit.family(&name).map_err(|e| e.to_string())
}

impl crate::Revit {
    /// The family this file holds, named `name`: an error when it has no `PartAtom` entry, as a project has none.
    pub fn family(&mut self, name: &str) -> Result<FamilyFile> {
        if self.stream_len("PartAtom").is_none() {
            return Err(Error::Format("has no PartAtom entry, which every family file holds: it is not a Revit family".into()));
        }
        family(&self.read_stream("PartAtom")?, name)
    }
}
