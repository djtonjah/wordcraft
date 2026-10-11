//! Bounded, same-origin host bridge for the isolated NALVRA Writer prototype.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::cell::RefCell;
use wasm_bindgen::prelude::*;
use wordcraft_ui_egui::WordApp;

const MAX_MESSAGE: usize = 32 * 1024 * 1024;
thread_local! {
    static REQUESTS: RefCell<Vec<Value>> = const { RefCell::new(Vec::new()) };
    static CONTEXT: RefCell<Option<egui::Context>> = const { RefCell::new(None) };
}

#[wasm_bindgen]
pub fn nalvra_request(payload: &str) -> Result<(), JsValue> {
    if payload.len() > MAX_MESSAGE {
        return Err(JsValue::from_str("Dokument zu groß"));
    }
    let request: Value = serde_json::from_str(payload).map_err(|e| JsValue::from_str(&e.to_string()))?;
    REQUESTS.with(|queue| {
        let mut queue = queue.borrow_mut();
        if queue.len() >= 8 {
            return Err(JsValue::from_str("Zu viele Anfragen"));
        }
        queue.push(request);
        Ok(())
    })?;
    CONTEXT.with(|ctx| {
        if let Some(ctx) = ctx.borrow().as_ref() {
            ctx.request_repaint();
        }
    });
    Ok(())
}

pub fn emit(value: Value) {
    if let Some(window) = web_sys::window()
        && let Ok(Some(parent)) = window.parent()
        && let Ok(origin) = window.location().origin()
    {
        let payload = json!({"channel":"nalvra-wordcraft-v1", "payload":value}).to_string();
        if let Err(e) = parent.post_message(&JsValue::from_str(&payload), &origin) {
            log::warn!("host response failed: {e:?}");
        }
    }
}

pub fn register(ctx: &egui::Context) {
    CONTEXT.with(|slot| *slot.borrow_mut() = Some(ctx.clone()));
    emit(json!({"event":"ready"}));
}

pub fn process(app: &mut WordApp) {
    let requests = REQUESTS.with(|queue| std::mem::take(&mut *queue.borrow_mut()));
    for request in requests {
        let result = execute(app, &request);
        emit(match result {
            Ok(value) => json!({"id":request["id"],"ok":true,"result":value}),
            Err(error) => json!({"id":request["id"],"ok":false,"error":error}),
        });
    }
}

fn execute(app: &mut WordApp, request: &Value) -> Result<Value, String> {
    match request["op"].as_str().unwrap_or("") {
        "host" => {
            if let Some(palette) = wordcraft_ui_egui::hosted::Palette::from_json(&request["theme"]) {
                CONTEXT.with(|ctx| {
                    if let Some(ctx) = ctx.borrow().as_ref() {
                        ctx.data_mut(|d| d.insert_temp(egui::Id::new("hosted_palette"), palette));
                    }
                });
            }
            if let Some(host) = app.hosted.as_mut() {
                host.title = request["title"].as_str().unwrap_or("Neues Dokument").chars().take(180).collect();
                host.revision = request["revision"].as_u64().unwrap_or(0);
                host.busy = request["busy"].as_bool().unwrap_or(false);
                host.opened = request["opened"].as_bool().unwrap_or(false);
                host.message = request["message"].as_str().unwrap_or("").chars().take(250).collect();
            }
            Ok(json!({"ok":true}))
        }
        "new" | "close" => {
            app.session = wordcraft_engine::Session::new(wordcraft_doc::Document::new());
            app.canvas = Default::default();
            app.session.view.proofing = false;
            app.ui.backstage = false;
            app.canvas.want_focus = true;
            snapshot(app)
        }
        "open" => {
            let doc = if let Some(document) = request.get("document") {
                serde_json::from_value(document.clone()).map_err(|e| e.to_string())?
            } else {
                let data = request["data"].as_str().ok_or("Datei fehlt")?;
                let bytes = STANDARD.decode(data).map_err(|e| e.to_string())?;
                let name = request["fileName"].as_str().unwrap_or("document.docx");
                wordcraft_engine::io::open_bytes(name, &bytes)?
            };
            app.session = wordcraft_engine::Session::new(doc);
            app.canvas = Default::default();
            app.session.view.proofing = false;
            app.ui.backstage = false;
            app.session.path = Some(format!("{}.docx", request["title"].as_str().unwrap_or("Dokument")).into());
            app.canvas.want_focus = true;
            snapshot(app)
        }
        "convert" => {
            let name = request["fileName"].as_str().ok_or("Dateiname fehlt")?;
            let bytes = STANDARD.decode(request["data"].as_str().ok_or("Datei fehlt")?).map_err(|e| e.to_string())?;
            if bytes.len() > MAX_MESSAGE / 2 {
                return Err("Datei zu groß".into());
            }
            let doc = wordcraft_engine::io::open_bytes(name, &bytes)?;
            let docx = wordcraft_engine::io::save_bytes("document.docx", &doc)?;
            if docx.len() > MAX_MESSAGE / 2 {
                return Err("Dokument zu groß".into());
            }
            Ok(json!({"docx":STANDARD.encode(docx)}))
        }
        "append" => {
            let text = request["text"].as_str().ok_or("Text fehlt")?;
            if text.len() > 16_384 {
                return Err("Text zu lang".into());
            }
            app.run("caret.docEnd", json!({}))?;
            app.run("text.insert", json!({"text":text,"raw":true}))?;
            snapshot(app)
        }
        "snapshot" => snapshot(app),
        "ack" => {
            let current = serde_json::to_value(&app.session.doc).map_err(|e| e.to_string())?;
            // JSON through JavaScript normalizes 1.0 to 1. Restore typed numbers
            // before equality so an unchanged document is acknowledged correctly.
            let saved: wordcraft_doc::Document = serde_json::from_value(request["document"].clone()).map_err(|e| e.to_string())?;
            if serde_json::to_value(saved).map_err(|e| e.to_string())? == current {
                app.session.dirty = false;
            }
            Ok(json!({"dirty":app.session.dirty}))
        }
        "pdf" => {
            let bytes = wordcraft_engine::io::save_bytes("document.pdf", &app.session.doc)?;
            if bytes.len() > MAX_MESSAGE / 2 {
                return Err("PDF zu groß".into());
            }
            Ok(json!({"data":STANDARD.encode(bytes),"pages":app.session.export_layout().pages.len(),"document":app.session.doc}))
        }
        _ => Err("Unbekannte Anfrage".into()),
    }
}

fn snapshot(app: &mut WordApp) -> Result<Value, String> {
    let docx = wordcraft_engine::io::save_bytes("document.docx", &app.session.doc)?;
    if docx.len() > MAX_MESSAGE / 2 {
        return Err("Dokument zu groß".into());
    }
    Ok(json!({"document":app.session.doc,"docx":STANDARD.encode(docx),
        "text":app.session.doc.plain_text(wordcraft_doc::StoryRef::Body),
        "pages":app.session.export_layout().pages.len(),"dirty":app.session.dirty}))
}
