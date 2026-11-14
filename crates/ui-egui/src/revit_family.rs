//! Import Revit Family (Insert > Revit Family): a `.rfa`, or a family the bridge exported from Revit (`.rfa.json`, with
//! instance parameters, formulas and a mesh for each type), read by the reader the desktop app attaches, shown before anything
//! changes: the family's name (which may be changed), its category and types, a row for each parameter with its kind, its
//! first type's value and whether it is shared, and the reader's notes of what the file did not give or what was left out.
//! Import runs the `family` command with what was read, as one undo step, and hands the family to the Component tool.
//!
//! The reader, `bimcraft-rvt`, is Apache-2.0 only (docs/rvt-provenance.md), so this crate never depends on it: a window
//! without one (every test, a build with `--no-default-features`) has no button, and says why when asked all the same. What
//! the reader answers is hostile like any file: the `family` command checks it, and a refusal stays on the screen.

use std::path::Path;
use std::sync::Arc;

use bimcraft_i18n::msg;
use egui::{Align, Context, Grid, Id, Label, Layout, RichText, ScrollArea, TextEdit};
use serde_json::Value;

use crate::lang::{t, tf};
use crate::popup::Modal;
use crate::style::pal;
use crate::{BimApp, Status};

/// A family read from a Revit file: the params of the `family` command that loads it, and the reader's notes, the first
/// saying what was read and the rest what was left out or changed and why.
#[derive(Clone, Debug, PartialEq)]
pub struct FamilyRead {
    pub name: String,
    pub family: Value,
    pub notes: Vec<String>,
}

/// Reads a Revit family file, or says why not.
pub type FamilyReader = Arc<dyn Fn(&Path) -> Result<FamilyRead, String> + Send + Sync>;

/// The screen, while it is up.
pub(crate) struct FamilyBox {
    file: String,
    /// The name the family joins the project under: the reader's, until changed here.
    pub(crate) name: String,
    read: FamilyRead,
    /// Why the `family` command refused it, when it did.
    pub(crate) refused: Option<String>,
    copied: bool,
}

/// A parameter's kind as the screen names it.
fn kind_name(kind: &str) -> &'static str {
    match kind {
        "length" => msg!("Length"),
        "number" => msg!("Number"),
        "integer" => msg!("Integer"),
        "angle" => msg!("Angle"),
        "area" => msg!("Area"),
        "volume" => msg!("Volume"),
        "yesno" => msg!("Yes/No"),
        "material" => msg!("Material"),
        "url" => msg!("URL"),
        "familytype" => msg!("Family Type"),
        _ => msg!("Text"),
    }
}

/// A family's category as the screen names it.
fn category_name(category: &str) -> &'static str {
    match category {
        "furniture" => msg!("Furniture"),
        "casework" => msg!("Casework"),
        "plumbing" => msg!("Plumbing Fixtures"),
        "pipefitting" => msg!("Pipe Fittings"),
        "pipeaccessory" => msg!("Pipe Accessories"),
        "equipment" => msg!("Electrical Equipment"),
        "outlet" => msg!("Electrical Fixtures"),
        "switch" => msg!("Lighting Devices"),
        "lighting" => msg!("Lighting Fixtures"),
        "airterminal" => msg!("Air Terminals"),
        "dataoutlet" => msg!("Data Devices"),
        "voiceoutlet" => msg!("Telephone Devices"),
        "door" => msg!("Doors"),
        "window" => msg!("Windows"),
        _ => msg!("Generic Models"),
    }
}

/// A number as short as it reads: no decimals it does not have, three at most.
fn short(x: f64) -> String {
    let rounded = (x * 1000.0).round() / 1000.0 + 0.0;
    format!("{rounded}")
}

/// One row of the table: a parameter's name, its kind, its value as shown, and a shared one's GUID.
struct Row {
    name: String,
    kind: &'static str,
    value: String,
    shared: Option<String>,
}

impl FamilyBox {
    fn category(&self) -> &'static str {
        category_name(self.read.family["category"].as_str().unwrap_or(""))
    }

    fn types(&self) -> Vec<&str> {
        self.read.family["types"].as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default()
    }

    /// `2 types: A, B`, the first few named.
    fn types_line(&self) -> String {
        let types = self.types();
        let mut names = types.iter().take(8).copied().collect::<Vec<_>>().join(", ");
        if types.len() > 8 {
            names.push_str(", …");
        }
        match types.len() {
            1 => tf("1 type: {}", &[&names]),
            n => tf("{} types: {}", &[&n.to_string(), &names]),
        }
    }

    /// The parameters, each with its value in the first type that sets it, else its own, in the project's `units`.
    fn rows(&self, units: bimcraft_core::units::Units) -> Vec<Row> {
        let types = self.read.family["types"].as_object();
        let parameters = self.read.family["parameters"].as_array().map(Vec::as_slice).unwrap_or_default();
        parameters
            .iter()
            .map(|p| {
                let name = p["name"].as_str().unwrap_or("").to_string();
                let kind = p["kind"].as_str().unwrap_or("length");
                let value = types.and_then(|t| t.values().find_map(|set| set.get(&name))).unwrap_or(&p["value"]);
                let value = match (kind, value) {
                    (_, Value::Bool(b)) => t(if *b { msg!("Yes") } else { msg!("No") }).to_string(),
                    (_, Value::String(s)) => s.chars().take(80).collect(),
                    ("length", v) => v.as_f64().map(|mm| units.length(mm)).unwrap_or_default(),
                    ("area", v) => v.as_f64().map(|a| units.area(a)).map(|(a, unit)| format!("{} {unit}", short(a))).unwrap_or_default(),
                    ("volume", v) => v.as_f64().map(|mm3| format!("{} m³", short(mm3 / 1e9))).unwrap_or_default(),
                    ("angle", v) => v.as_f64().map(|a| format!("{}°", short(a))).unwrap_or_default(),
                    (_, v) => v.as_f64().map(short).unwrap_or_default(),
                };
                Row { name, kind: kind_name(kind), value, shared: p["shared"].as_str().map(str::to_string) }
            })
            .collect()
    }

    /// Why the name can't be imported under, when it can't.
    fn name_refused(&self, taken: bool) -> Option<String> {
        let name = self.name.trim();
        if name.is_empty() {
            Some(t("A family needs a name.").to_string())
        } else if taken {
            Some(tf("The project has a family named {} already: give this one another name.", &[name]))
        } else {
            None
        }
    }

    /// The screen in plain text, in millimetres: for pasting into an issue or a file.
    pub(crate) fn text(&self) -> String {
        let mut lines = vec![self.file.clone(), format!("{} · {}", self.name.trim(), t(self.category())), self.types_line(), String::new()];
        for row in self.rows(bimcraft_core::units::Units::Millimetres) {
            let mut line = format!("{} · {} · {}", row.name, t(row.kind), row.value);
            if row.kind == "Length" {
                line.push_str(" mm");
            }
            if let Some(guid) = row.shared {
                line.push_str(&format!(" · {} {guid}", t("Shared")));
            }
            lines.push(line);
        }
        lines.push(String::new());
        lines.extend(self.read.notes.iter().map(|note| format!("• {note}")));
        lines.join("\n") + "\n"
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
}

impl BimApp {
    /// Insert > Revit Family's button: asks for a `.rfa` and reads it.
    pub(crate) fn revit_family_dialog(&mut self, ctx: &Context) {
        // A `.rfa`, or what tools/revit_bridge exported of one from Revit (`.rfa.json`).
        let dialog = rfd::AsyncFileDialog::new().add_filter(t("Revit family"), &["rfa", "json"]);
        self.ask_file(ctx, dialog, false, |app, path| app.import_revit_family(&path));
    }

    /// Reads the Revit family at `path` and puts up the screen that shows it; a refusal is said in the status box. Nothing
    /// joins the project until Import.
    pub(crate) fn import_revit_family(&mut self, path: &Path) {
        let file = file_name(path);
        let Some(reader) = self.revit_family.clone() else {
            let template = msg!(
                "{}: this build of BIMCraft was made without Revit support (`--no-default-features`); build with the default `rvt` feature to read Revit files"
            );
            self.status = Status { text: tf(template, &[&file]), error: true };
            return;
        };
        // The reader is hostile-input code: whatever it does, the window says something. The entry it reads is small, so it
        // is read here, not on a thread.
        let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| reader(path))).unwrap_or_else(|p| {
            let why = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned());
            Err(match why {
                Some(why) => format!("the Revit reader stopped on this file ({why})"),
                None => "the Revit reader stopped on this file".to_string(),
            })
        });
        match read {
            Ok(read) => {
                self.status = Status::default();
                self.revit_family_box = Some(FamilyBox { file, name: read.name.clone(), read, refused: None, copied: false });
            }
            Err(e) => self.status = Status { text: tf("Couldn't read {}: {}", &[&file, &e]), error: true },
        }
    }

    /// Whether the screen is up, which holds the keys as every dialog does.
    pub(crate) fn revit_family_open(&self) -> bool {
        self.revit_family_box.is_some()
    }

    /// Import: the `family` command with what was read, under the name on the screen. Whether it went through; a refusal
    /// stays on the screen.
    fn finish_revit_family(&mut self) -> bool {
        let Some(b) = self.revit_family_box.as_ref() else { return false };
        let name = b.name.trim().to_string();
        let mut family = b.read.family.clone();
        family["name"] = Value::String(name.clone());
        let file = b.file.clone();
        match self.exec("family", &family) {
            Ok(_) => {
                let Some(made) = self.session.model.families.get(&name) else { return false };
                let (types, parameters) = (made.types.len(), made.parameters.len());
                let first = made.types.keys().next().cloned().unwrap_or_default();
                self.component = Some((name.clone(), first));
                let text = tf(
                    "Imported the family {} from {}: {}, {}.",
                    &[
                        &name,
                        &file,
                        &if types == 1 { t("1 type").to_string() } else { tf("{} types", &[&types.to_string()]) },
                        &if parameters == 1 { t("1 parameter").to_string() } else { tf("{} parameters", &[&parameters.to_string()]) },
                    ],
                );
                self.status = Status { text, error: false };
                true
            }
            Err(e) => {
                if let Some(b) = self.revit_family_box.as_mut() {
                    b.refused = Some(e);
                }
                false
            }
        }
    }

    /// The screen itself.
    pub(crate) fn revit_family_window(&mut self, ctx: &Context) {
        let units = self.session.model.units;
        let Some(b) = self.revit_family_box.as_mut() else { return };
        let taken = self.session.model.families.contains_key(b.name.trim());
        let name_refused = b.name_refused(taken);
        let rows = b.rows(units);
        let (mut import, mut cancel, mut copy) = (false, false, false);
        let height = (ctx.content_rect().height() * 0.6).max(200.0);
        let r = Modal::new(Id::new("revit-family"), t("Import Revit Family")).width(640.0).show(ctx, |ui| {
            crate::popup::body(ui, |ui| {
                ui.label(RichText::new(&b.file).color(pal().muted));
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(t("Name"));
                    ui.add(TextEdit::singleline(&mut b.name).desired_width(300.0).char_limit(bimcraft_core::model::MAX_TEXT));
                    ui.label(RichText::new(t(b.category())).color(pal().muted));
                });
                if let Some(why) = &name_refused {
                    ui.label(RichText::new(why).color(pal().error));
                }
                ui.label(b.types_line());
                ui.add_space(6.0);
                ScrollArea::vertical().max_height(height).show(ui, |ui| {
                    if rows.is_empty() {
                        ui.label(t("The file lists no parameters."));
                    }
                    Grid::new("revit-family-parameters").num_columns(4).striped(true).spacing([18.0, 4.0]).show(ui, |ui| {
                        if !rows.is_empty() {
                            for head in [msg!("Parameter"), msg!("Data Type"), msg!("Value"), ""] {
                                ui.label(RichText::new(t(head)).strong());
                            }
                            ui.end_row();
                        }
                        for row in &rows {
                            ui.label(&row.name);
                            ui.label(t(row.kind));
                            ui.label(&row.value);
                            match &row.shared {
                                Some(guid) => ui.label(RichText::new(t("Shared")).color(pal().muted)).on_hover_text(guid),
                                None => ui.label(""),
                            };
                            ui.end_row();
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new(t("Noted")).strong());
                    for note in &b.read.notes {
                        ui.add(Label::new(format!("• {note}")).wrap());
                    }
                });
                if let Some(why) = &b.refused {
                    ui.add_space(6.0);
                    ui.add(Label::new(RichText::new(why).color(pal().error)).wrap());
                }
            });
            crate::popup::footer(ui, |ui| {
                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    cancel = ui.button(t("Cancel")).clicked();
                    import = ui.add_enabled(name_refused.is_none(), egui::Button::new(t("Import"))).clicked();
                    copy = ui.button(t(if b.copied { msg!("Copied") } else { msg!("Copy Report") })).clicked();
                });
            });
        });
        if copy {
            ctx.copy_text(b.text());
            b.copied = true;
            ctx.request_repaint();
        }
        if cancel || r.should_close() || (import && self.finish_revit_family()) {
            self.revit_family_box = None;
        }
    }
}
