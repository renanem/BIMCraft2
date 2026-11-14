//! File > New > Project from Template…, and the start page's New from Template: the New Project dialog, as Revit's asks
//! for a template file. It lists the templates the window knows (`.bimt` files saved here or browsed to, newest first, kept
//! in a file of their own beside the recent files) and <None>, an empty project; Browse… adds one; OK runs `new`, with
//! `template` when one is picked. The project that comes of a template is unsaved: saving it asks where.

use std::path::{Path, PathBuf};

use bimcraft_core::files;
use egui::{Align, ComboBox, Context, Id, Layout, RichText};
use serde_json::json;

use crate::lang::{t, tf};
use crate::popup::Modal;
use crate::recent::Recent;
use crate::style::pal;
use crate::{BimApp, Status};

/// What the templates' file is called, beside the recent files'.
const FILE: &str = "templates.json";

/// Where the templates are kept, given where the favorites are: beside them, in a file of their own.
pub fn beside(favorites: &Path) -> PathBuf {
    favorites.with_file_name(FILE)
}

/// The dialog, while it is up.
#[derive(Default)]
pub(crate) struct NewProject {
    /// The template picked, by its place in the list; none, an empty project.
    pub(crate) pick: Option<usize>,
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
}

impl BimApp {
    /// Keep the project templates in the file at `path`, starting from what it holds. Until the app is told where, it keeps
    /// none: a window made in a test never reads or writes the user's own.
    pub fn keep_templates_in(&mut self, path: PathBuf) {
        self.project_templates = Recent::load(&path);
        self.project_templates_path = Some(path);
    }

    /// `path` is a template just saved or browsed to: the newest of the list.
    pub fn note_template(&mut self, path: &Path) {
        if self.project_templates_path.is_some() {
            self.project_templates.push(path);
            self.keep_templates();
        }
    }

    fn keep_templates(&mut self) {
        let Some(path) = &self.project_templates_path else { return };
        if let Err(e) = self.project_templates.save(path)
            && self.status.text.is_empty()
        {
            self.status = Status { text: tf("Couldn't save the setting: {}", &[&e]), error: true };
        }
    }

    /// The dialog comes up, the template used last picked.
    pub(crate) fn open_new_project(&mut self) {
        let pick = (!self.project_templates.paths().is_empty()).then_some(0);
        self.new_project = Some(NewProject { pick });
    }

    /// Browse…'s answer: the file is a template the dialog knows from now on, and the one picked.
    pub(crate) fn browsed_template(&mut self, path: &Path) {
        if self.project_templates_path.is_some() {
            self.note_template(path);
        } else {
            // A window that keeps no list still takes the file for this dialog.
            self.project_templates.push(path);
        }
        if let Some(d) = self.new_project.as_mut() {
            d.pick = Some(0);
        }
    }

    /// OK: a new project, of the template picked or empty, in the interface's language where it is empty.
    fn finish_new_project(&mut self, pick: Option<usize>) {
        let template = pick.and_then(|i| self.project_templates.paths().get(i).cloned());
        let params = match &template {
            Some(path) => json!({"template": path.to_string_lossy()}),
            None => json!({"language": self.lang.project().code()}),
        };
        match self.exec("new", &params) {
            Ok(_) => {
                self.retire_autosave();
                self.reopened();
                self.autosave_document();
                self.status = match &template {
                    Some(path) => {
                        self.note_template(path);
                        Status { text: tf("New project from the template {}.", &[&file_name(path)]), error: false }
                    }
                    None => Status::default(),
                };
            }
            Err(e) => {
                // A template whose file is gone is forgotten: it would fail the same way next time.
                if let Some(path) = template.filter(|p| !p.exists()) {
                    self.project_templates.remove(&path);
                    self.keep_templates();
                }
                self.status = Status { text: e, error: true };
            }
        }
    }

    /// The dialog itself.
    pub(crate) fn new_project_window(&mut self, ctx: &Context) {
        let Some(d) = self.new_project.as_mut() else { return };
        let labels = self.project_templates.labels();
        let paths = self.project_templates.paths();
        // A list that shrank under the dialog leaves it on <None>.
        if d.pick.is_some_and(|i| i >= labels.len()) {
            d.pick = None;
        }
        let none = t("<None>");
        let (mut ok, mut cancel, mut browse) = (false, false, false);
        let r = Modal::new(Id::new("new-project"), t("New Project")).width(460.0).show(ctx, |ui| {
            crate::popup::body(ui, |ui| {
                ui.label(RichText::new(t("Template file")).strong());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let shown = d.pick.and_then(|i| labels.get(i)).map_or(none, String::as_str);
                    ComboBox::from_id_salt("new-project-template").selected_text(shown).width(300.0).truncate().show_ui(ui, |ui| {
                        ui.selectable_value(&mut d.pick, None, none);
                        for (i, label) in labels.iter().enumerate() {
                            let row = ui.selectable_value(&mut d.pick, Some(i), label);
                            if let Some(path) = paths.get(i) {
                                row.on_hover_text(path.to_string_lossy());
                            }
                        }
                    });
                    browse = ui.button(t("Browse…")).clicked();
                });
                ui.add_space(6.0);
                let what = match d.pick.and_then(|i| paths.get(i)) {
                    Some(path) => path.to_string_lossy().into_owned(),
                    None => t("An empty project, with two levels.").to_string(),
                };
                ui.add(egui::Label::new(RichText::new(what).color(pal().muted)).wrap());
            });
            crate::popup::footer(ui, |ui| {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    cancel = ui.button(t("Cancel")).clicked();
                    ok = ui.button(t("OK")).clicked();
                });
            });
        });
        let pick = d.pick;
        if browse {
            let dialog = rfd::AsyncFileDialog::new().add_filter(t("BIMCraft template"), &[files::TEMPLATE, files::PROJECT]);
            self.ask_file(ctx, dialog, false, |app, path| app.browsed_template(&path));
        }
        if ok {
            self.new_project = None;
            self.finish_new_project(pick);
        } else if cancel || r.should_close() {
            self.new_project = None;
        }
    }
}
