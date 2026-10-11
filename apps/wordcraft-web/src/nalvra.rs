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
        "open" => {
            let doc = if let Some(document) = request.get("document") {
                serde_json::from_value(document.clone()).map_err(|e| e.to_string())?
            } else {
                let data = request["data"].as_str().ok_or("Datei fehlt")?;
                let bytes = STANDARD.decode(data).map_err(|e| e.to_string())?;
                wordcraft_engine::io::open_bytes("document.docx", &bytes)?
            };
            app.session = wordcraft_engine::Session::new(doc);
            app.session.view.proofing = false;
            app.ui.backstage = false;
            app.session.path = Some("NALVRA Dokument.docx".into());
            snapshot(app)
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
