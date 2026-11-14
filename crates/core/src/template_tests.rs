//! A new project from a template (`new {template}`), as Revit's New > Project asks for a template file: the project starts
//! as the template is, unsaved, under its own name and identity, and the template's file is never written.
use std::path::PathBuf;

use serde_json::json;

use crate::engine::Session;
use crate::model::{Element, Model};

fn has_level(m: &Model, name: &str) -> bool {
    m.elements.values().any(|e| matches!(e, Element::Level(l) if l.name == name))
}

fn dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bimcraft-template-test-{}", crate::ids::Guid::new()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An office's template, saved as `office.bimt`: a third level, a wall, a family, a text type and a view template of its own.
fn office(dir: &std::path::Path) -> (PathBuf, Session) {
    let mut s = Session::new();
    s.execute("new", &json!({"name": "Office Template"})).unwrap();
    s.execute("level", &json!({"name": "Roof", "elevation": 6000})).unwrap();
    s.execute("wall", &json!({"points": [[0, 0], [4000, 0]]})).unwrap();
    s.execute(
        "family",
        &json!({"name": "Office Desk", "category": "furniture", "parameters": [{"name": "Top", "kind": "length", "value": 1400}],
            "types": {"1400": {"Top": 1400}, "1800": {"Top": 1800}}}),
    )
    .unwrap();
    s.execute("text_type", &json!({"name": "Office 3mm", "size": 3.0})).unwrap();
    s.execute("template", &json!({"name": "Office Working", "kind": "plan", "scale": 50})).unwrap();
    let path = dir.join("office.bimt");
    s.execute("save", &json!({"path": path})).unwrap();
    (path, s)
}

#[test]
fn a_new_project_from_a_template_starts_as_the_template_is_unsaved_under_its_own_name_and_identity() {
    let dir = dir();
    let (template, made) = office(&dir);
    let before = std::fs::read(&template).unwrap();

    let mut s = Session::new();
    s.execute("wall", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    let reply = s.execute("new", &json!({"template": template, "name": "Tower A"})).unwrap();
    assert_eq!(reply["name"], "Tower A");
    assert_eq!(reply["template"], json!(template));
    // What the template holds is the project's: its levels and elements, its families and types, its view templates.
    let m = &s.model;
    assert_eq!(m.name, "Tower A");
    assert_eq!(m.elements.len(), made.model.elements.len());
    assert!(has_level(m, "Roof"));
    assert_eq!(m.elements.values().filter(|e| e.category() == "wall").count(), 1);
    assert_eq!(m.families["Office Desk"].types.len(), 2);
    assert!(m.text_types.contains_key("Office 3mm"));
    assert!(m.view_templates.contains_key("Office Working"));
    m.check().unwrap();
    // It is a project of its own: no file yet, no history, and an identity that is not the template's.
    assert!(s.path.is_none(), "saving it asks where: the template is never saved over");
    assert!(s.history().is_empty());
    assert_ne!(m.uid, made.model.uid);
    // The template's file is as it was, and saving the project writes another.
    let project = dir.join("tower.bimc");
    s.execute("save", &json!({"path": project})).unwrap();
    assert_eq!(std::fs::read(&template).unwrap(), before);
    assert_eq!(s.path.as_deref(), Some(project.as_path()));
    // Two projects of one template are two projects.
    let mut other = Session::new();
    other.execute("new", &json!({"template": template})).unwrap();
    assert_eq!(other.model.name, "Project", "without a name, a new project's");
    assert_ne!(other.model.uid, s.model.uid);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_template_keeps_its_language_unless_told_and_a_project_file_serves_as_one() {
    let dir = dir();
    let mut s = Session::new();
    s.execute("new", &json!({"language": "pt"})).unwrap();
    let path = dir.join("modelo.bimt");
    s.execute("save", &json!({"path": path})).unwrap();
    let mut n = Session::new();
    let reply = n.execute("new", &json!({"template": path})).unwrap();
    assert_eq!((reply["language"].as_str(), reply["name"].as_str()), (Some("pt"), Some("Projeto")));
    assert!(has_level(&n.model, "Nível 1"));
    n.execute("new", &json!({"template": path, "language": "en"})).unwrap();
    assert_eq!(n.model.language.code(), "en");
    assert!(has_level(&n.model, "Nível 1"), "what the template named keeps its name");
    // A project (`.bimc`) is a template too, as Revit takes a project for one.
    let project = dir.join("base.bimc");
    s.execute("save", &json!({"path": project})).unwrap();
    n.execute("new", &json!({"template": project})).unwrap();
    assert!(n.path.is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_template_that_is_not_there_or_is_no_project_is_refused_and_the_project_open_stays() {
    let dir = dir();
    let mut s = Session::new();
    s.execute("new", &json!({"name": "Mine"})).unwrap();
    s.execute("wall", &json!({"points": [[0, 0], [1000, 0]]})).unwrap();
    let elements = s.model.elements.len();
    let missing = dir.join("nowhere.bimt");
    let e = s.execute("new", &json!({"template": missing})).unwrap_err();
    assert!(e.contains("nowhere.bimt"), "{e}");
    let damaged = dir.join("damaged.bimt");
    std::fs::write(&damaged, b"{\"format\": 1, \"name\": ").unwrap();
    let e = s.execute("new", &json!({"template": damaged})).unwrap_err();
    assert!(e.contains("damaged.bimt") && e.contains("not a BIMCraft project"), "{e}");
    for name in ["plan.ifc", "notes.txt", "chair.bimf", "house.rte"] {
        let other = dir.join(name);
        std::fs::write(&other, b"x").unwrap();
        let e = s.execute("new", &json!({"template": other})).unwrap_err();
        assert!(e.contains(".bimt") && e.contains(name), "{name}: {e}");
    }
    for bad in [json!({"template": 7}), json!({"template": ""}), json!({"template": damaged, "language": "xx"})] {
        assert!(s.execute("new", &bad).is_err(), "{bad}");
    }
    assert_eq!((s.model.name.as_str(), s.model.elements.len()), ("Mine", elements), "the project open is as it was");
    let _ = std::fs::remove_dir_all(dir);
}
