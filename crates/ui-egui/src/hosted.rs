//! An embedding app owns document identity, persistence and save acknowledgement.
use crate::{WordApp, theme};

#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub accent: egui::Color32,
    pub panel: egui::Color32,
    pub canvas: egui::Color32,
    pub text: egui::Color32,
    pub muted: egui::Color32,
    pub border: egui::Color32,
}
impl Palette {
    pub fn from_json(value: &Value) -> Option<Self> {
        fn color(v: &Value, key: &str) -> Option<egui::Color32> {
            let a = v[key].as_array()?;
            if a.len() != 3 {
                return None;
            }
            Some(egui::Color32::from_rgb(u8::try_from(a[0].as_u64()?).ok()?, u8::try_from(a[1].as_u64()?).ok()?, u8::try_from(a[2].as_u64()?).ok()?))
        }
        Some(Self {
            dark: value["dark"].as_bool()?,
            accent: color(value, "accent")?,
            panel: color(value, "panel")?,
            canvas: color(value, "canvas")?,
            text: color(value, "text")?,
            muted: color(value, "muted")?,
            border: color(value, "border")?,
        })
    }
}
use egui::RichText;
use serde_json::{Value, json};

pub struct HostedDocument {
    pub title: String,
    pub revision: u64,
    pub busy: bool,
    pub opened: bool,
    pub message: String,
    pub dispatch: Box<dyn Fn(&str)>,
}

pub fn action_for(command: &str) -> Option<&'static str> {
    Some(match command {
        "file.new" => "new",
        "file.open" => "open",
        "file.save" => "save",
        "file.saveAs" => "saveAs",
        "file.exportPdf" | "file.print" | "ui.print" => "pdf",
        "file.close" => "close",
        _ => return None,
    })
}

pub fn command(app: &mut WordApp, id: &str) -> Option<Result<Value, String>> {
    let host = app.hosted.as_ref()?;
    if id == "file.autosave" {
        return Some(Err("Automatisches Speichern ist für diesen Writer noch nicht aktiviert.".into()));
    }
    let action = action_for(id)?;
    if host.busy {
        return Some(Err("Bitte den laufenden Vorgang abwarten.".into()));
    }
    (host.dispatch)(action);
    // Host acknowledgement alone clears dirty. Never report an asynchronous save as saved.
    Some(Ok(json!({"pending":"host", "action":action})))
}

pub fn install_theme(ctx: &egui::Context, enabled: bool) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("nalvra_hosted"), enabled));
    if enabled {
        let t = theme::Tokens::get(ctx);
        let theme = if t.dark { egui::Theme::Dark } else { egui::Theme::Light };
        ctx.options_mut(|o| o.theme_preference = if t.dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
        let mut style = (*ctx.style_of(theme)).clone();
        style.visuals.override_text_color = Some(t.text);
        style.visuals.panel_fill = t.panel;
        style.visuals.window_fill = t.panel;
        style.visuals.selection.bg_fill = t.checked;
        style.visuals.selection.stroke.color = t.accent_text;
        style.visuals.widgets.inactive.bg_fill = t.input;
        style.visuals.widgets.hovered.bg_fill = t.hover;
        style.visuals.widgets.active.bg_fill = t.pressed;
        ctx.set_style_of(theme, style);
    }
}

fn dispatch(app: &mut WordApp, action: &str) {
    if let Some(host) = &app.hosted
        && !host.busy
    {
        (host.dispatch)(action);
    }
}

pub fn title(app: &mut WordApp, ui: &mut egui::Ui) {
    let t = theme::Tokens::get(ui.ctx());
    egui::Panel::top("nalvra_document_header")
        .exact_size(48.0)
        .frame(egui::Frame::NONE.fill(t.title_bar).inner_margin(egui::Margin::symmetric(14, 8)))
        .show(ui, |ui| {
            ui.horizontal_centered(|ui| {
                ui.label(RichText::new("WRITER").font(theme::medium(12.0)).color(t.accent_text));
                ui.add_space(14.0);
                let Some(host) = app.hosted.as_ref() else { return };
                ui.label(RichText::new(&host.title).font(theme::semibold(13.0)).color(t.text));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let busy = app.hosted.as_ref().is_some_and(|h| h.busy);
                    if ui.add_enabled(!busy, egui::Button::new("Speichern")).clicked() {
                        let _ = app.run("file.save", json!({}));
                    }
                    if ui.add_enabled(!busy, egui::Button::new("Öffnen")).clicked() {
                        let _ = app.run("file.open", json!({}));
                    }
                    let state = if busy {
                        "Wird gespeichert / geladen …".into()
                    } else if app.session.dirty {
                        "Ungespeicherte Änderungen".into()
                    } else {
                        app.hosted.as_ref().map(|h| h.message.clone()).unwrap_or_default()
                    };
                    ui.label(RichText::new(state).font(theme::regular(11.0)).color(if app.session.dirty { t.accent_text } else { t.text_dim }));
                });
            });
        });
}

pub fn files(app: &mut WordApp, ui: &mut egui::Ui) {
    let t = theme::Tokens::get(ui.ctx());
    egui::Panel::left("nalvra_file_menu")
        .exact_size(225.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.title_bar).inner_margin(egui::Margin::same(16)))
        .show(ui, |ui| {
            ui.label(RichText::new("DOKUMENT").color(t.accent_text).font(theme::medium(12.0)));
            ui.add_space(16.0);
            if ui.button("← Zurück zum Schreiben").clicked() {
                app.ui.backstage = false;
                app.canvas.want_focus = true;
            }
            ui.add_space(20.0);
            for (label, action) in [
                ("Neues Dokument", "new"),
                ("Öffnen …", "open"),
                ("Speichern", "save"),
                ("Speichern unter …", "saveAs"),
                ("Gespeicherten Stand öffnen", "reopen"),
                ("PDF exportieren", "pdf"),
                ("DOCX herunterladen", "download"),
                ("Dokument schließen", "close"),
            ] {
                let host = app.hosted.as_ref();
                let enabled = host.is_some_and(|h| !h.busy && (h.opened || matches!(action, "open" | "new")));
                if ui.add_enabled(enabled, egui::Button::new(label).min_size(egui::vec2(193.0, 36.0))).clicked() {
                    app.ui.backstage = false;
                    dispatch(app, action);
                }
                ui.add_space(4.0);
            }
        });
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(34))).show(ui, |ui| {
        ui.heading("Ihr Dokument in NALVRA ONE");
        ui.add_space(18.0);
        if let Some(host) = &app.hosted {
            ui.label(RichText::new(&host.title).font(theme::semibold(22.0)));
            ui.add_space(12.0);
            ui.label(format!("Gespeicherte Version {}", host.revision));
            ui.label(&host.message);
        }
        ui.add_space(24.0);
        ui.label("Öffnen, Speichern und PDF verwenden die Dokumentablage und Berechtigungen von NALVRA ONE.");
        ui.label("Speichern unter legt eine weitere Dokumentkopie an. Das bisherige Dokument bleibt erhalten.");
        ui.add_space(16.0);
        ui.label("Strg+S  Speichern    ·    Strg+O  Öffnen    ·    Strg+N  Neu    ·    F12  Speichern unter");
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::RefCell, rc::Rc};
    #[test]
    fn hosted_saves_dispatch_without_clearing_dirty_or_writing_a_file() {
        let actions = Rc::new(RefCell::new(Vec::new()));
        let captured = actions.clone();
        let mut app = WordApp::new(wordcraft_engine::Session::new(wordcraft_doc::Document::new()), Default::default());
        app.hosted = Some(HostedDocument {
            title: "Entwurf".into(),
            revision: 1,
            busy: false,
            opened: true,
            message: "Bereit".into(),
            dispatch: Box::new(move |a| captured.borrow_mut().push(a.to_string())),
        });
        app.session.dirty = true;
        for id in ["file.save", "file.saveAs", "file.open", "file.new", "file.exportPdf", "ui.print", "file.close"] {
            assert_eq!(app.run(id, json!({})).unwrap()["pending"], "host");
            assert!(app.session.dirty);
            assert!(app.dialog.is_none(), "host owns the save/discard dialog");
        }
        assert_eq!(*actions.borrow(), vec!["save", "saveAs", "open", "new", "pdf", "pdf", "close"]);
        app.hosted.as_mut().unwrap().busy = true;
        assert!(app.run("file.save", json!({})).is_err());
        assert_eq!(actions.borrow().len(), 7);
    }
}
