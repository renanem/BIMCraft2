//! File > New > Project from Template…: the New Project dialog, as Revit's asks for a template file. The templates it lists
//! are the `.bimt` files saved or browsed to, remembered beside the recent files; OK runs `new {template}`.

use std::path::{Path, PathBuf};

use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bimcraft-newproject-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A template on disk: a project with a Roof level, a wall and a text type of its own.
fn office(dir: &Path) -> PathBuf {
    let mut s = Session::new();
    s.script(
        r#"level {"name": "Roof", "elevation": 6000}
wall {"points": [[0, 0], [4000, 0]]}
text_type {"name": "Office 3mm", "size": 3.0}"#,
    )
    .unwrap();
    let path = dir.join("Office.bimt");
    s.execute("save", &json!({"path": path})).unwrap();
    path
}

fn has_level(h: &Harness<BimApp>, name: &str) -> bool {
    h.state().session.model.elements.values().any(|e| matches!(e, bimcraft_core::model::Element::Level(l) if l.name == name))
}

fn open_dialog(h: &mut Harness<BimApp>) {
    file_menu(h, "New", "Project from Template…");
}

#[test]
fn a_new_project_from_a_template_starts_as_the_template_is_and_is_unsaved() {
    let dir = scratch("from");
    let template = office(&dir);
    let mut h = app("");
    h.state_mut().keep_templates_in(dir.join("templates.json"));
    h.state_mut().note_template(&template);
    open_dialog(&mut h);
    // The dialog names the templates it knows by their files, the newest picked, and <None> for an empty project.
    assert!(h.query_by_label("New Project").is_some());
    assert!(h.query_by_label("Template file").is_some());
    assert_eq!(h.state().new_project.as_ref().unwrap().pick, Some(0));
    assert!(!has_level(&h, "Roof"), "nothing happens until OK");
    press(&mut h, "OK");
    let app = h.state();
    assert!(app.new_project.is_none());
    assert!(has_level(&h, "Roof"));
    assert!(app.session.model.text_types.contains_key("Office 3mm"));
    assert_eq!(app.session.model.elements.values().filter(|e| e.category() == "wall").count(), 1);
    assert!(app.session.path.is_none(), "saving it asks where: the template is not the project's file");
    assert_eq!(app.session.model.name, "Project");
    assert!(!app.status.error, "{}", app.status.text);
    assert_eq!(app.status.text, "New project from the template Office.bimt.");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn without_a_template_the_dialog_makes_an_empty_project_and_cancel_makes_none() {
    let mut h = app(BOX);
    open_dialog(&mut h);
    assert_eq!(h.state().new_project.as_ref().unwrap().pick, None, "no template is known: <None>");
    press(&mut h, "Cancel");
    assert!(h.state().new_project.is_none());
    assert_eq!(h.state().session.model.elements.values().filter(|e| e.category() == "wall").count(), 4, "the project stays");
    // The project's walls are not saved: New asks first, as it always does, and only then the dialog comes.
    h.state_mut().saved = std::sync::Arc::clone(&h.state().session.model);
    open_dialog(&mut h);
    press(&mut h, "OK");
    assert_eq!(h.state().session.model.elements.values().filter(|e| e.category() == "wall").count(), 0);
    assert_eq!(h.state().session.model.elements.len(), 2, "Level 1 and Level 2");
}

#[test]
fn a_template_saved_or_browsed_to_is_remembered_and_one_that_is_gone_is_forgotten_and_said() {
    let dir = scratch("kept");
    let list = dir.join("templates.json");
    let mut h = app(BOX);
    h.state_mut().keep_templates_in(list.clone());
    // Save As a `.bimt` makes it a template the dialog offers; a project (`.bimc`) is not one.
    let mine = dir.join("Mine.bimt");
    assert!(h.state_mut().save_to(&mine));
    assert!(h.state_mut().save_to(&dir.join("job.bimc")));
    let abs = |p: &Path| std::path::absolute(p).unwrap();
    assert_eq!(h.state().project_templates.paths(), [abs(&mine)]);
    // Browsing to one adds it, newest first, and picks it.
    let office = office(&dir);
    open_dialog(&mut h);
    h.state_mut().browsed_template(&office);
    assert_eq!(h.state().project_templates.paths(), [abs(&office), abs(&mine)]);
    assert_eq!(h.state().new_project.as_ref().unwrap().pick, Some(0));
    press(&mut h, "Cancel");
    // The list is in its file: another window finds it.
    let mut other = app("");
    other.state_mut().keep_templates_in(list);
    assert_eq!(other.state().project_templates.paths(), [abs(&office), abs(&mine)]);
    // A template whose file is gone can't start a project: it is said, forgotten, and the project open stays.
    std::fs::remove_file(&office).unwrap();
    other.state_mut().session.script(BOX).unwrap();
    other.state_mut().saved = std::sync::Arc::clone(&other.state().session.model);
    open_dialog(&mut other);
    press(&mut other, "OK");
    let app = other.state();
    assert!(app.status.error && app.status.text.contains("Office.bimt"), "{}", app.status.text);
    assert_eq!(app.project_templates.paths(), [abs(&mine)]);
    assert_eq!(app.session.model.elements.values().filter(|e| e.category() == "wall").count(), 4);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_window_told_no_place_keeps_no_templates_and_the_start_page_offers_the_dialog() {
    let dir = scratch("none");
    let template = office(&dir);
    let mut h = app("");
    h.state_mut().note_template(&template);
    assert!(h.state().project_templates.paths().is_empty(), "a test's window never writes the user's list");
    h.state_mut().show_start_page();
    h.run();
    press(&mut h, "New from Template");
    assert!(h.state().new_project.is_some());
    let _ = std::fs::remove_dir_all(dir);
}

/// What the dialog looks like: `BIMCRAFT_NEW_PROJECT_DUMP=<folder>` writes it light and dark.
#[test]
fn new_project_screenshots() {
    let Ok(folder) = std::env::var("BIMCRAFT_NEW_PROJECT_DUMP") else { return };
    std::fs::create_dir_all(&folder).unwrap();
    let dir = scratch("shots");
    let template = office(&dir);
    for (dark, look) in [(false, "light"), (true, "dark")] {
        let mut h = app(BOX);
        if dark {
            h.ctx.set_theme(egui::Theme::Dark);
        }
        h.state_mut().keep_templates_in(dir.join("templates.json"));
        h.state_mut().note_template(&template);
        h.state_mut().saved = std::sync::Arc::clone(&h.state().session.model);
        open_dialog(&mut h);
        h.render().unwrap().save(Path::new(&folder).join(format!("new-project-{look}.png"))).unwrap();
    }
    let _ = std::fs::remove_dir_all(dir);
}
