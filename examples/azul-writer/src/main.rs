//! AzulDoc — P5 goal app (SUPER_PLAN_2 §4 P5.4).
//!
//! A simple document view with an "Export to PDF" button. The button's
//! callback builds the document `Dom` and calls `Pdf::from_dom(dom, w, h)`
//! (headless dom -> PDF pages, no window, no file I/O), then writes the
//! returned bytes to a file itself. Built on the public `azul::` surface only.
//!
//! v1 exports the document's layout (solid fills / section backgrounds);
//! text-in-PDF (walk `TextLayout` -> printpdf text Ops) and a markdown
//! editor / live preview are follow-ups.

use azul::error::ResultRefAnyString;
use azul::json::{Json, JsonKeyValue};
use azul::option::{OptionBool, OptionJson, OptionString};
use azul::pdf::Pdf;
use azul::prelude::*;
use azul::vec::JsonKeyValueVec;

struct DocState {
    /// Where the PDF is written (temp dir). Shown after export.
    export_path: String,
    /// Set once an export has been triggered (drives the status line).
    exported: bool,
}

impl DocState {
    fn new() -> Self {
        let path = std::env::temp_dir().join("azul-doc-export.pdf");
        Self {
            export_path: path.to_string_lossy().into_owned(),
            exported: false,
        }
    }
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    font-family: sans-serif; background: #f4f4f7;";
const TOOLBAR: &str = "display: flex; flex-direction: row; align-items: center; \
    background: #2b2b3c; color: white; padding: 10px 16px;";
const TITLE: &str = "font-size: 18px; flex-grow: 1;";
const BTN: &str = "background: #4a90e2; color: white; padding: 8px 16px; \
    border-radius: 6px; font-size: 14px; cursor: pointer;";
const PAGE: &str = "flex-grow: 1; margin: 16px; padding: 24px; background: white; \
    border-radius: 8px;";
const H1: &str = "font-size: 24px; color: #1c1c28; margin-bottom: 6px;";
const SECTION: &str = "background: #eef1f8; border-radius: 6px; padding: 12px; \
    margin: 10px 0px;";
const SECTION_H: &str = "font-size: 16px; color: #2b2b3c; margin-bottom: 4px;";
const BODY: &str = "color: #4a4a5a; font-size: 14px;";
const STATUS: &str = "color: #2e7d32; font-size: 13px; margin-top: 10px;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (path, exported) = match data.downcast_ref::<DocState>() {
        Some(s) => (s.export_path.clone(), s.exported),
        None => (String::new(), false),
    };

    let mut page = doc_page();

    if exported {
        page = page.with_child(
            Dom::create_text(format!("Exported to {}", path).as_str()).with_css(STATUS),
        );
    }

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(
                Dom::create_div()
                    .with_css(TOOLBAR)
                    .with_child(Dom::create_text("AzulDoc").with_css(TITLE))
                    .with_child(
                        Dom::create_div()
                            .with_css(BTN)
                            .with_child(Dom::create_text("Export to PDF"))
                            .with_callback(
                                EventFilter::Hover(HoverEventFilter::MouseUp),
                                data.clone(),
                                on_export,
                            ),
                    ),
            )
            .with_child(page),
    )
}

fn section(heading: &str, body: &str) -> Dom {
    Dom::create_div()
        .with_css(SECTION)
        .with_child(Dom::create_text(heading).with_css(SECTION_H))
        .with_child(Dom::create_text(body).with_css(BODY))
}

/// The document content (shared by the on-screen view + the PDF export).
fn doc_page() -> Dom {
    Dom::create_div()
        .with_css(PAGE)
        .with_child(Dom::create_text("Project Brief").with_css(H1))
        .with_child(section(
            "Overview",
            "AzulDoc renders a styled document and exports it to PDF via the public Pdf::from_dom API (headless dom -> PDF pages, no window).",
        ))
        .with_child(section(
            "Status",
            "P5: PDF export wired end-to-end. Solid fills export today; text and inline images follow.",
        ))
        .with_child(section(
            "Next",
            "Markdown editing, live preview, and a reference-PDF diff round out the AzulDoc demo.",
        ))
}

/// Export the document to a PDF. Headless: build the document `Dom`, render it
/// to PDF bytes via `Pdf::from_dom` (no window, no file I/O in the API), and
/// write the bytes ourselves. A4 at 96 DPI = 794 x 1123 px.
extern "C" fn on_export(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<DocState>() {
        let doc = Dom::create_body().with_child(doc_page());
        let bytes = Pdf::new().from_dom(doc, 794.0, 1123.0);
        let _ = std::fs::write(&s.export_path, bytes.as_slice());
        s.exported = true;
    }
    Update::RefreshDom
}

/// JSON serializer for [`DocState`] — the Rust equivalent of the C examples'
/// `AZ_REFLECT_JSON` toJson half. The web backend requires this on the root
/// `RefAny`: the server serializes app state into the pre-rendered HTML and
/// the wasm client rebuilds it via [`doc_state_from_json`].
extern "C" fn doc_state_to_json(mut refany: RefAny) -> Json {
    match refany.downcast_ref::<DocState>() {
        Some(s) => {
            let entries = [
                JsonKeyValue::create("export_path", Json::string(s.export_path.as_str())),
                JsonKeyValue::create("exported", Json::bool(s.exported)),
            ];
            Json::object(JsonKeyValueVec::copy_from_array(&entries[0], entries.len()))
        }
        None => Json::null(),
    }
}

/// JSON deserializer for [`DocState`] — runs INSIDE the lifted wasm on the
/// web client to rebuild the app state the server serialized. Missing fields
/// fall back to defaults rather than erroring: the state is display-only
/// (`export_path` is a server-side path string; the status flag).
extern "C" fn doc_state_from_json(json: Json) -> ResultRefAnyString {
    // The Az option enums implement Drop, so payloads must be read by
    // reference rather than moved out of the match.
    let export_path = match &json.get_key("export_path") {
        OptionJson::Some(j) => match &j.as_string() {
            OptionString::Some(s) => s.as_str().to_string(),
            OptionString::None => String::new(),
        },
        OptionJson::None => String::new(),
    };
    let exported = match &json.get_key("exported") {
        OptionJson::Some(j) => matches!(j.as_bool(), OptionBool::Some(true)),
        OptionJson::None => false,
    };
    let mut r = RefAny::new(DocState { export_path, exported });
    // Re-register on the reconstructed RefAny so later serialize round-trips
    // (state sync, undo snapshots) keep working on the client too.
    r.set_serialize_fn(doc_state_to_json as usize);
    r.set_deserialize_fn(doc_state_from_json as usize);
    ResultRefAnyString::ok(r)
}

fn main() {
    let mut data = RefAny::new(DocState::new());
    // Required by the web backend (hydrate serializes the root state into the
    // pre-rendered HTML; the wasm client deserializes it back). No-op cost on
    // desktop. See dll/src/web/mod.rs's pre-render check.
    data.set_serialize_fn(doc_state_to_json as usize);
    data.set_deserialize_fn(doc_state_from_json as usize);
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
