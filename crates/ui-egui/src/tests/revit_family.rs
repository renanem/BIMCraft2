//! Import Revit Family: the screen that shows what a `.rfa` gives (its types, its parameters and what the file left out) before
//! the family joins the project through the `family` command. No test reads a real file: the reader is a closure.

use std::path::Path;
use std::sync::Mutex;

use super::*;
use crate::revit_family::{FamilyRead, FamilyReader};

/// What a reader gives of a bench: two types, three parameters (one shared), and two notes.
fn bench() -> FamilyRead {
    FamilyRead {
        name: "Park Bench".into(),
        family: json!({
            "name": "Park Bench",
            "category": "furniture",
            "parameters": [
                {"name": "Seat Width", "kind": "length", "value": 1200.0},
                {"name": "Has Back", "kind": "yesno", "value": true},
                {"name": "Finish", "kind": "text", "value": "Oiled", "shared": "0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10"},
            ],
            "types": {
                "1200": {"Seat Width": 1200.0, "Has Back": true, "Finish": "Oiled"},
                "1800": {"Seat Width": 1800.0, "Has Back": false, "Finish": "Painted"},
            },
        }),
        notes: vec![
            "2 types and 3 parameters read from the file's PartAtom entry".into(),
            "1 parameter left out, `2 furlongs` is no length the model holds: `Reach`".into(),
            "A family file is read for its types and their parameters only: its geometry is not read".into(),
        ],
    }
}

/// The app on a room of four walls, with a reader that answers `answer()` and remembers what it was asked for.
fn rig(answer: impl Fn() -> Result<FamilyRead, String> + Send + Sync + 'static) -> (Harness<'static, BimApp>, Arc<Mutex<Vec<String>>>) {
    let asked: Arc<Mutex<Vec<String>>> = Arc::default();
    let log = Arc::clone(&asked);
    let reader: FamilyReader = Arc::new(move |path: &Path| {
        log.lock().unwrap().push(path.to_string_lossy().into_owned());
        answer()
    });
    let mut h = app(BOX);
    h.state_mut().revit_family = Some(reader);
    (h, asked)
}

fn read(h: &mut Harness<BimApp>, path: &str) {
    h.state_mut().import_revit_family(Path::new(path));
    h.run();
}

fn shown(h: &Harness<BimApp>, label: &str) -> bool {
    h.query_by_label(label).is_some()
}

#[test]
fn the_insert_tab_offers_revit_family_only_to_a_window_with_a_reader() {
    let mut h = app(BOX);
    press(&mut h, "Insert");
    assert!(!shown(&h, "Revit Family"), "a build without the reader has no button");
    let (mut h, _) = rig(|| Ok(bench()));
    press(&mut h, "Insert");
    assert!(shown(&h, "Revit Family"));
}

#[test]
fn a_family_read_is_shown_with_its_types_parameters_and_notes_and_joins_the_project_on_import() {
    let (mut h, asked) = rig(|| Ok(bench()));
    read(&mut h, "/library/Park Bench.rfa");
    assert_eq!(*asked.lock().unwrap(), ["/library/Park Bench.rfa"]);
    // The screen: the file, the family's category and types, a row for each parameter, and every note.
    assert!(shown(&h, "Import Revit Family") && shown(&h, "Park Bench.rfa"));
    assert!(shown(&h, "2 types: 1200, 1800"));
    for label in ["Seat Width", "Has Back", "Finish", "Length", "Yes/No", "Text", "1200", "Yes", "Oiled", "Shared"] {
        assert!(shown(&h, label), "{label} is shown");
    }
    assert!(shown(&h, "• 1 parameter left out, `2 furlongs` is no length the model holds: `Reach`"));
    assert!(shown(&h, "• A family file is read for its types and their parameters only: its geometry is not read"));
    // Nothing has joined the project yet, and the keys are held as under any dialog.
    assert!(!h.state().session.model.families.contains_key("Park Bench"));
    assert!(h.state().revit_family_open());

    press(&mut h, "Import");
    let app = h.state();
    let family = &app.session.model.families["Park Bench"];
    assert_eq!(family.types.keys().collect::<Vec<_>>(), ["1200", "1800"]);
    assert_eq!(family.types["1800"]["Seat Width"], 1800.0);
    assert_eq!(family.parameters.len(), 3);
    assert!(family.parameters.iter().any(|p| p.name == "Finish" && p.shared.is_some()));
    // The Component tool takes it, as Load Family's does, and the status box says what came in.
    assert_eq!(app.component, Some(("Park Bench".to_string(), "1200".to_string())));
    assert!(!app.status.error, "{}", app.status.text);
    assert_eq!(app.status.text, "Imported the family Park Bench from Park Bench.rfa: 2 types, 3 parameters.");
    assert!(!app.revit_family_open());
    // One undo step takes it back.
    h.state_mut().session.execute("undo", &json!({})).unwrap();
    assert!(!h.state().session.model.families.contains_key("Park Bench"));
}

#[test]
fn cancel_imports_nothing_and_the_name_may_be_changed_before_importing() {
    let (mut h, _) = rig(|| Ok(bench()));
    read(&mut h, "/library/Park Bench.rfa");
    press(&mut h, "Cancel");
    assert!(!h.state().revit_family_open());
    assert!(!h.state().session.model.families.contains_key("Park Bench"));

    read(&mut h, "/library/Park Bench.rfa");
    h.state_mut().revit_family_box.as_mut().unwrap().name = "Garden Bench".into();
    h.run();
    press(&mut h, "Import");
    assert!(h.state().session.model.families.contains_key("Garden Bench"));
    assert!(!h.state().session.model.families.contains_key("Park Bench"));
}

#[test]
fn a_name_the_project_has_already_or_none_is_refused_until_it_is_changed() {
    let (mut h, _) = rig(|| Ok(bench()));
    h.state_mut().session.execute("family", &bench().family).unwrap();
    let before = h.state().session.model.families["Park Bench"].clone();
    read(&mut h, "/library/Park Bench.rfa");
    assert!(shown(&h, "The project has a family named Park Bench already: give this one another name."));
    assert!(is_disabled(&h, Role::Button, "Import"));
    h.state_mut().revit_family_box.as_mut().unwrap().name = "  ".into();
    h.run();
    assert!(shown(&h, "A family needs a name."));
    assert!(is_disabled(&h, Role::Button, "Import"));
    h.state_mut().revit_family_box.as_mut().unwrap().name = "Park Bench 2".into();
    h.run();
    assert!(!is_disabled(&h, Role::Button, "Import"));
    press(&mut h, "Import");
    assert_eq!(h.state().session.model.families["Park Bench"], before, "the one it had is as it was");
    assert!(h.state().session.model.families.contains_key("Park Bench 2"));
}

#[test]
fn a_refusal_a_reader_that_panics_and_a_window_without_one_are_said_in_the_status_box() {
    let (mut h, _) = rig(|| Err("has no PartAtom entry, which every family file holds: it is not a Revit family".into()));
    read(&mut h, "/library/Odd.rfa");
    let status = &h.state().status;
    assert!(status.error && status.text.starts_with("Couldn't read Odd.rfa: has no PartAtom entry"), "{}", status.text);
    assert!(!h.state().revit_family_open());

    let (mut h, _) = rig(|| panic!("out of its depth"));
    read(&mut h, "/library/Odd.rfa");
    let status = &h.state().status;
    assert!(status.error && status.text.contains("Odd.rfa") && status.text.contains("out of its depth"), "{}", status.text);

    let mut h = app(BOX);
    read(&mut h, "/library/Odd.rfa");
    let status = &h.state().status;
    assert!(status.error && status.text.contains("without Revit support"), "{}", status.text);
}

#[test]
fn a_family_the_command_refuses_stays_on_the_screen_with_the_reason() {
    // A reader's answer is hostile too: a parameter of no kind there is.
    let mut odd = bench();
    odd.family["parameters"][0]["kind"] = json!("flavour");
    let (mut h, _) = rig(move || Ok(odd.clone()));
    read(&mut h, "/library/Park Bench.rfa");
    press(&mut h, "Import");
    assert!(h.state().revit_family_open(), "the screen stays, to read why");
    assert!(h.state().revit_family_box.as_ref().unwrap().refused.as_deref().is_some_and(|e| e.contains("flavour")));
    assert!(!h.state().session.model.families.contains_key("Park Bench"));
}

#[test]
fn the_report_copies_as_plain_text_with_every_parameter_and_note() {
    let (mut h, _) = rig(|| Ok(bench()));
    read(&mut h, "/library/Park Bench.rfa");
    let text = h.state().revit_family_box.as_ref().unwrap().text();
    for line in [
        "Park Bench.rfa",
        "Park Bench · Furniture",
        "2 types: 1200, 1800",
        "Seat Width · Length · 1200 mm",
        "Finish · Text · Oiled · Shared 0f3c9a52-7d41-4e7a-9b1e-2c5d8e6f7a10",
        "• 1 parameter left out, `2 furlongs` is no length the model holds: `Reach`",
    ] {
        assert!(text.lines().any(|l| l == line), "{line} is a line of:\n{text}");
    }
    press(&mut h, "Copy Report");
    assert!(shown(&h, "Copied"));
}

/// What the screen looks like, for a person to look at: `BIMCRAFT_REVIT_FAMILY_DUMP=<folder>` writes it light and dark, and
/// with a name the project has already.
#[test]
fn revit_family_screenshots() {
    let Ok(folder) = std::env::var("BIMCRAFT_REVIT_FAMILY_DUMP") else { return };
    std::fs::create_dir_all(&folder).unwrap();
    for (dark, look) in [(false, "light"), (true, "dark")] {
        let (mut h, _) = rig(|| Ok(bench()));
        if dark {
            h.ctx.set_theme(egui::Theme::Dark);
        }
        press(&mut h, "Insert");
        read(&mut h, "/library/Park Bench.rfa");
        h.render().unwrap().save(Path::new(&folder).join(format!("revit-family-{look}.png"))).unwrap();
        h.state_mut().session.execute("family", &bench().family).unwrap();
        h.run();
        h.render().unwrap().save(Path::new(&folder).join(format!("revit-family-taken-{look}.png"))).unwrap();
    }
}
