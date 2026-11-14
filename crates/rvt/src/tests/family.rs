//! A family file's types and parameters, read from its `PartAtom` entry and loaded through the `family` command.

use super::*;
use crate::fixture;
use bimcraft_core::engine::Session;
use serde_json::{Value, json};

/// A `PartAtom` entry written here, in the layout Revit's own show: the family's options under `A:features`, and under
/// `A:family` a part for each type, an element for each of its type parameters.
fn atom(category: &str, options: &str, parts: &[(&str, &str)]) -> Vec<u8> {
    let parts: String = parts.iter().map(|(title, body)| format!(r#"<A:part type="user"><title>{title}</title>{body}</A:part>"#)).collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><entry xmlns="http://www.w3.org/2005/Atom" xmlns:A="urn:schemas-autodesk-com:partatom"><title>0123</title><updated>2026-01-02T03:04:05Z</updated><category><term>{category}</term><scheme>adsk:revit:grouping</scheme></category><A:features><A:feature><A:title>Family Parameters</A:title><A:group><A:title>Other</A:title>{options}</A:group></A:feature></A:features><A:family type="user"><A:variationCount>{}</A:variationCount>{parts}</A:family></entry>"#,
        parts.matches("<A:part").count()
    )
    .into_bytes()
}

const BENCH: &str = r#"<Seat_Width displayName="Seat Width" type="custom" typeOfParameter="Length" units="mm">1200</Seat_Width><Depth type="custom" typeOfParameter="Length" units="in">20</Depth><Slats type="custom" typeOfParameter="Integer">7</Slats><Lean type="custom" typeOfParameter="Angle" units="°">12.5</Lean><Has_Back displayName="Has Back" type="custom" typeOfParameter="Yes/No">Yes</Has_Back><Finish type="shared" id="0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10" typeOfParameter="Text">Oiled &amp; waxed</Finish><Type_Comments displayName="Type Comments" type="system" typeOfParameter="Text">Outdoor</Type_Comments><Default_Elevation displayName="Default Elevation" type="system" typeOfParameter="Length" units="mm">150</Default_Elevation>"#;

const LONG_BENCH: &str = r#"<Seat_Width displayName="Seat Width" type="custom" typeOfParameter="Length" units="mm">1800</Seat_Width><Depth type="custom" typeOfParameter="Length" units="in">20</Depth><Slats type="custom" typeOfParameter="Integer">9</Slats><Lean type="custom" typeOfParameter="Angle" units="°">12.5</Lean><Has_Back displayName="Has Back" type="custom" typeOfParameter="Yes/No">No</Has_Back><Finish type="shared" id="0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10" typeOfParameter="Text">Painted</Finish><Type_Comments displayName="Type Comments" type="system" typeOfParameter="Text">Outdoor</Type_Comments><Default_Elevation displayName="Default Elevation" type="system" typeOfParameter="Length" units="mm">150</Default_Elevation>"#;

const OPENING: &str = r#"<Width type="system" typeOfParameter="Length" units="mm">900</Width><Height type="system" typeOfParameter="Length" units="mm">2100</Height>"#;

const OPTIONS: &str = r#"<Work_Plane_Based displayName="Work Plane-Based" type="system" typeOfParameter="Yes/No">Yes</Work_Plane_Based><Always_vertical displayName="Always vertical" type="system" typeOfParameter="Yes/No">No</Always_vertical><Shared type="system" typeOfParameter="Yes/No">Yes</Shared>"#;

fn parameter<'a>(family: &'a Value, name: &str) -> &'a Value {
    family["parameters"].as_array().unwrap().iter().find(|p| p["name"] == name).unwrap_or_else(|| panic!("no parameter {name}"))
}

/// Loads what was read as the desktop does, through the `family` command, and gives the family the project then holds.
fn loaded(read: &crate::FamilyFile) -> bimcraft_core::family::Family {
    let mut s = Session::new();
    s.execute("family", &read.family).unwrap_or_else(|e| panic!("the family command refuses what was read: {e}"));
    s.model.check().unwrap();
    s.model.families.get(&read.name).cloned().unwrap_or_else(|| panic!("no family {}", read.name))
}

#[test]
fn a_family_comes_in_with_its_types_and_each_parameters_name_kind_unit_value_and_shared_guid() {
    let read = crate::family(&atom("Furniture", OPTIONS, &[("1200", BENCH), ("1800", LONG_BENCH)]), "Park Bench").unwrap();
    assert_eq!(read.name, "Park Bench");
    let f = &read.family;
    assert_eq!((f["name"].as_str(), f["category"].as_str()), (Some("Park Bench"), Some("furniture")));
    // A parameter is named as Revit shows it, of the kind its data type is, and none is an instance parameter: the entry
    // lists types' values only.
    for (name, kind) in [("Seat Width", "length"), ("Depth", "length"), ("Slats", "integer"), ("Lean", "angle"), ("Has Back", "yesno")] {
        let p = parameter(f, name);
        assert_eq!(p["kind"], kind, "{name}");
        assert!(p.get("instance").is_none() && p.get("shared").is_none(), "{name}");
    }
    assert_eq!(parameter(f, "Finish")["shared"], "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10");
    assert_eq!(parameter(f, "Type Comments")["kind"], "text", "a built-in type parameter comes in as the family's own");
    // Each type's values, lengths in millimetres whatever the file's unit was.
    assert_eq!(
        f["types"]["1200"],
        json!({"Seat Width": 1200.0, "Depth": 508.0, "Slats": 7.0, "Lean": 12.5, "Has Back": true, "Finish": "Oiled & waxed", "Type Comments": "Outdoor"})
    );
    assert_eq!(f["types"]["1800"]["Seat Width"], 1800.0);
    assert_eq!(f["types"]["1800"]["Has Back"], false);
    assert_eq!(f["types"]["1800"]["Finish"], "Painted");
    // The family's options, and Default Elevation, which every type agrees on.
    assert_eq!(f["options"], json!({"work_plane_based": true, "always_vertical": false, "shared": true, "default_elevation": 150.0}));
    assert!(f["parameters"].as_array().unwrap().iter().all(|p| p["name"] != "Default Elevation"));

    let family = loaded(&read);
    assert_eq!(family.types.keys().collect::<Vec<_>>(), ["1200", "1800"]);
    assert_eq!(family.parameters.len(), 7);
    assert!((family.default_elevation - 150.0).abs() < 1e-9);
    assert!(family.work_plane_based && !family.always_vertical && family.shared);
    assert!(family.forms.is_empty(), "no geometry is read");
}

#[test]
fn the_report_says_what_a_family_file_does_not_give() {
    let read = crate::family(&atom("Furniture", "", &[("A", BENCH)]), "Bench").unwrap();
    let all = read.notes.join("\n");
    for missing in ["geometry", "instance parameters", "formulas", "connectors", "materials"] {
        assert!(all.contains(missing), "the notes name {missing}: {all}");
    }
    assert!(all.contains("tools/revit_bridge"), "and where the rest comes from: {all}");
    assert!(read.notes[0].starts_with("1 type and 7 parameters read"), "{}", read.notes[0]);
}

#[test]
fn what_bimcraft_has_no_kind_or_unit_for_is_kept_as_a_number_or_left_out_and_said() {
    let body = r#"<Power type="custom" typeOfParameter="Wattage" units="W">60</Power><Reach type="custom" typeOfParameter="Length" units="furlong">2</Reach><Face type="custom" typeOfParameter="Material">Oak</Face><Price type="custom" typeOfParameter="Currency">abc</Price><Wide type="custom" typeOfParameter="Length" units="mm">1e999</Wide><On type="custom" typeOfParameter="Yes/No">Perhaps</On><Area type="custom" typeOfParameter="Area" units="m²">2.5</Area><Bulk type="custom" typeOfParameter="Volume" units="m³">0.5</Bulk><Web type="custom" typeOfParameter="URL">https://example.org/bench</Web>"#;
    let read = crate::family(&atom("Furniture", "", &[("A", body)]), "Lamp").unwrap();
    let f = &read.family;
    let all = read.notes.join("\n");
    // A data type with no kind here is a number in the file's unit, which the note names.
    assert_eq!((parameter(f, "Power")["kind"].as_str(), f["types"]["A"]["Power"].as_f64()), (Some("number"), Some(60.0)));
    assert!(all.contains("Power (Wattage, W)"), "{all}");
    // A material is its name as text: the file holds no material.
    assert_eq!((parameter(f, "Face")["kind"].as_str(), f["types"]["A"]["Face"].as_str()), (Some("text"), Some("Oak")));
    // Areas and volumes are the model's: mm² and mm³.
    assert_eq!(f["types"]["A"]["Area"], 2.5e6);
    assert_eq!(f["types"]["A"]["Bulk"], 0.5e9);
    assert_eq!(parameter(f, "Web")["kind"], "url");
    // A unit not known, a value that is no number of its kind, and a number out of range are left out, by name, with why.
    // A data type with no kind whose value is no number is text.
    assert_eq!((parameter(f, "Price")["kind"].as_str(), f["types"]["A"]["Price"].as_str()), (Some("text"), Some("abc")));
    for (name, why) in [("Reach", "furlong"), ("Wide", "1e999"), ("On", "Perhaps")] {
        assert!(f["parameters"].as_array().unwrap().iter().all(|p| p["name"] != name), "{name} is left out");
        assert!(read.notes.iter().any(|n| n.contains("left out") && n.contains(name) && n.contains(why)), "{name}: {all}");
    }
    loaded(&read);
}

#[test]
fn a_length_written_with_its_unit_or_in_feet_and_inches_is_read_in_millimetres() {
    let one = |text: &str| {
        let body = format!(r#"<L type="custom" typeOfParameter="Length">{}</L>"#, text.replace('"', "&quot;"));
        let read = crate::family(&atom("Furniture", "", &[("A", &body)]), "X").unwrap();
        read.family["types"]["A"]["L"].as_f64()
    };
    for (text, mm) in [
        ("4 cm", 40.0),
        ("1.5m", 1500.0),
        (r#"1' - 10""#, 558.8),
        (r#"0' - 1 1/2""#, 38.1),
        (r#"0' - 0 1/4""#, 6.35),
        (r#"6 1/4""#, 158.75),
        (r#"3/8""#, 9.525),
        ("3'", 914.4),
        (r#"-1' - 6""#, -457.2),
    ] {
        let got = one(text).unwrap_or_else(|| panic!("{text} is read"));
        assert!((got - mm).abs() < 1e-9, "{text}: {got}");
    }
    // What is no length is left out: no unit at all, a unit not known, a fraction over nothing, words.
    for text in ["12", "4 furlongs", r#"1/0""#, r#"1' - x""#, "wide", "'", "1e400 m"] {
        assert_eq!(one(text), None, "{text}");
    }
}

#[test]
fn a_data_type_named_in_another_language_is_told_by_its_unit_and_a_yes_or_no_by_its_answers() {
    // Revit names the data type in the language it saved the file in: Portuguese, German, Russian, Japanese here.
    let body = r#"<Largura type="custom" typeOfParameter="Comprimento" units="mm">900</Largura><Raio type="custom" typeOfParameter="Comprimento" units="cm">4 cm</Raio><Breite type="custom" typeOfParameter="Länge" units="mm">12</Breite><Dlina type="custom" typeOfParameter="Длина" units="мм">7</Dlina><Area_util displayName="Area util" type="custom" typeOfParameter="Área" units="m²">2</Area_util><Giro type="custom" typeOfParameter="Ângulo" units="°">45</Giro><Visivel type="custom" typeOfParameter="Sim/Não">Sim</Visivel><Oculto type="custom" typeOfParameter="Sim/Não">Não</Oculto><Hai type="custom" typeOfParameter="はい/いいえ">いいえ</Hai><Acabamento type="custom" typeOfParameter="Material">Carvalho</Acabamento><Nota type="custom" typeOfParameter="Texto">Sim</Nota><Peso type="custom" typeOfParameter="Massa por comprimento de unidade" units="kg/m">3.5</Peso><Razao type="custom" typeOfParameter="Entrada/Saída">2</Razao>"#;
    let read = crate::family(&atom("Mobiliário", "", &[("A", body)]), "Banco").unwrap();
    let (f, t) = (&read.family, &read.family["types"]["A"]);
    for (name, kind, value) in [
        ("Largura", "length", json!(900.0)),
        ("Raio", "length", json!(40.0)),
        ("Breite", "length", json!(12.0)),
        ("Dlina", "length", json!(7.0)),
        ("Area util", "area", json!(2.0e6)),
        ("Giro", "angle", json!(45.0)),
        ("Visivel", "yesno", json!(true)),
        ("Oculto", "yesno", json!(false)),
        ("Hai", "yesno", json!(false)),
        ("Acabamento", "text", json!("Carvalho")),
        // Text that happens to say yes stays text: its data type names no two answers.
        ("Nota", "text", json!("Sim")),
        ("Peso", "number", json!(3.5)),
        // A name with a stroke whose value is neither of its halves is no yes or no.
        ("Razao", "number", json!(2.0)),
    ] {
        assert_eq!((parameter(f, name)["kind"].as_str(), &t[name]), (Some(kind), &value), "{name}");
    }
    let unknown = read.notes.iter().find(|n| n.contains("no kind for")).unwrap();
    assert!(unknown.starts_with("2 parameters") && unknown.contains("Peso (Massa por comprimento de unidade, kg/m)"), "{unknown}");
    loaded(&read);
}

#[test]
fn a_category_bimcraft_has_none_for_or_named_in_another_language_is_generic_and_said() {
    for (label, category) in [
        ("Doors", "door"),
        ("Windows", "window"),
        ("Casework", "casework"),
        ("Plumbing Fixtures", "plumbing"),
        ("Lighting Fixtures", "lighting"),
        ("Electrical Equipment", "equipment"),
        ("Pipe Fittings", "pipefitting"),
        ("Generic Models", "generic"),
    ] {
        let read = crate::family(&atom(label, "", &[("A", OPENING)]), "X").unwrap();
        assert_eq!(read.family["category"], category, "{label}");
        assert!(!read.notes.iter().any(|n| n.contains("category")), "{label}: nothing to say");
        loaded(&read);
    }
    // A door or a window here makes an opening of its Width and Height: a family without them is generic, and told why.
    let read = crate::family(&atom("Doors", "", &[("A", BENCH)]), "X").unwrap();
    assert_eq!(read.family["category"], "generic");
    assert!(read.notes.iter().any(|n| n.contains("`Doors`") && n.contains("Width")), "{:?}", read.notes);
    loaded(&read);
    for label in ["Structural Connections", "Mobiliário"] {
        let read = crate::family(&atom(label, "", &[("A", BENCH)]), "X").unwrap();
        assert_eq!(read.family["category"], "generic", "{label}");
        assert!(read.notes.iter().any(|n| n.contains(label) && n.contains("generic")), "{label}: {:?}", read.notes);
        loaded(&read);
    }
}

#[test]
fn more_parameters_or_types_than_a_family_holds_are_left_out_and_counted() {
    let many: String = (0..80).map(|i| format!(r#"<P{i} type="custom" typeOfParameter="Length" units="mm">{i}</P{i}>"#)).collect();
    let read = crate::family(&atom("Furniture", "", &[("A", &many)]), "Many").unwrap();
    let kept = read.family["parameters"].as_array().unwrap().len();
    assert_eq!(kept, bimcraft_core::family::MAX_PARAMETERS);
    assert!(
        read.notes.iter().any(|n| n.starts_with("16 parameters left out") && n.contains("`P64`") && n.contains("and 4 more")),
        "{:?}",
        read.notes
    );
    loaded(&read);
}

#[test]
fn a_type_that_lacks_a_parameter_takes_the_first_types_value_and_two_types_of_one_name_are_one() {
    let wide = r#"<W type="custom" typeOfParameter="Length" units="mm">10</W><H type="custom" typeOfParameter="Length" units="mm">20</H>"#;
    let narrow = r#"<W type="custom" typeOfParameter="Length" units="mm">5</W>"#;
    let read = crate::family(&atom("Furniture", "", &[("A", wide), ("B", narrow), ("A", narrow)]), "X").unwrap();
    assert_eq!(read.family["types"], json!({"A": {"W": 10.0, "H": 20.0}, "B": {"W": 5.0}}));
    assert_eq!(parameter(&read.family, "H")["value"], 20.0);
    assert!(read.notes.iter().any(|n| n.contains("1 type left out") && n.contains("`A`")), "{:?}", read.notes);
    loaded(&read);
}

#[test]
fn an_entry_with_no_types_or_none_at_all_says_so() {
    let read = crate::family(&atom("Furniture", "", &[]), "Bare").unwrap();
    // A family needs a type, so one is made, named after the family, and said.
    assert_eq!(read.family["types"], json!({"Bare": {}}));
    assert!(read.notes[0].starts_with("0 types and 0 parameters read"), "{:?}", read.notes);
    assert!(read.notes.iter().any(|n| n.contains("lists no type") && n.contains("`Bare`")), "{:?}", read.notes);
    loaded(&read);
    // A family file without the entry, and a project, which is no family.
    let basic = fixture::basic_file_info(&fixture::Basic::default());
    let r = &mut Revit::from_bytes(container(&[("BasicFileInfo", &basic)]), Limits::default()).unwrap();
    let e = r.family("X").unwrap_err().to_string();
    assert!(e.contains("PartAtom"), "{e}");
    let with = container(&[("BasicFileInfo", &basic), ("PartAtom", &atom("Furniture", "", &[("A", BENCH)]))]);
    let read = Revit::from_bytes(with, Limits::default()).unwrap().family("Bench").unwrap();
    assert_eq!(read.family["types"]["A"]["Seat Width"], 1200.0);
}

#[test]
fn a_damaged_entry_never_panics_and_what_comes_back_still_loads() {
    let good = atom("Furniture", OPTIONS, &[("1200", BENCH), ("1800", LONG_BENCH)]);
    let mut seed = 0x9e37_79b9_7f4a_7c15u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let mut came_back = 0;
    for round in 0..600 {
        let mut bad = good.clone();
        for _ in 0..1 + next() % 6 {
            let at = (next() % bad.len() as u64) as usize;
            bad[at] = match next() % 5 {
                0 => b'<',
                1 => b'"',
                2 => b'9',
                3 => b'&',
                _ => next() as u8,
            };
        }
        if round % 9 == 0 {
            bad.truncate(bad.len() - (next() % 300) as usize);
        }
        if let Ok(read) = crate::family(&bad, "Bench") {
            came_back += 1;
            loaded(&read);
        }
    }
    assert!(came_back > 50, "the sweep reaches the reader: {came_back}");
    // Names and values as long and as odd as a file may hold.
    let long = "x".repeat(5000);
    let odd = format!(
        r#"<{long} type="custom" typeOfParameter="Length" units="mm">1</{long}><A displayName="" type="custom" typeOfParameter="Text">{long}</A><B displayName="a&#10;b" type="custom" typeOfParameter="Text">t</B><C type="shared" id="not-a-guid" typeOfParameter="Text">t</C><D type="custom" typeOfParameter="Length" units="mm">-1e12</D>"#
    );
    let read = crate::family(&atom(&long, "", &[(&long, &odd), ("", &odd), ("T", &odd)]), &long).unwrap();
    loaded(&read);
    assert!(parameter(&read.family, "C").get("shared").is_none(), "a GUID that is none makes a family parameter");
}

#[test]
fn every_family_of_the_sample_folders_reads_and_loads() {
    let Some(folders) = std::env::var_os("BIMCRAFT_RFA_SAMPLES") else { return };
    let mut files = Vec::new();
    let mut dirs: Vec<PathBuf> = std::env::split_paths(&folders).collect();
    while let Some(dir) = dirs.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("rfa")) {
                files.push(path);
            }
        }
    }
    let (mut read_ok, mut parameters, mut tally) = (0, 0, std::collections::BTreeMap::<String, usize>::new());
    for path in &files {
        match crate::family_path(path) {
            Ok(read) => {
                read_ok += 1;
                parameters += read.family["parameters"].as_array().map_or(0, Vec::len);
                loaded(&read);
                // The data types met that have no kind here, each with its unit.
                for note in read.notes.iter().filter(|n| n.contains("no kind for")) {
                    for kind in note.split('(').skip(1).filter_map(|rest| rest.split(')').next()) {
                        *tally.entry(format!("data type with no kind: {kind}")).or_default() += 1;
                    }
                }
                for note in read.notes.iter().skip(1) {
                    *tally.entry(note.split('(').next().unwrap_or("").chars().take(90).collect()).or_default() += 1;
                }
            }
            Err(e) => *tally.entry(format!("refused: {}", e.chars().take(90).collect::<String>())).or_default() += 1,
        }
    }
    println!("{read_ok} of {} families read, {parameters} parameters", files.len());
    for (note, n) in tally {
        println!("{n:5}  {note}");
    }
    assert!(read_ok * 10 >= files.len() * 9, "{read_ok} of {} families read", files.len());
}
