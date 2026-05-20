//! AzulDoc — P5 goal app (SUPER_PLAN_2 §4 P5.4).
//!
//! A simple document view with an "Export to PDF" button. The button's
//! callback calls `CallbackInfo::export_to_pdf(path)`, which queues the
//! export; the dll's `printpdf` engine then walks the window's display list
//! → PDF. Built on the public `azul::` surface only.
//!
//! v1 exports the document's layout (solid fills / section backgrounds) —
//! the on-screen view shows full text via the renderer; text-in-PDF (walk
//! `TextLayout` → printpdf text Ops) and a markdown editor / live preview
//! are follow-ups (research/06 §2.1, §5.4).

use azul::prelude::*;

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

    let mut page = Dom::create_div()
        .with_css(PAGE)
        .with_child(Dom::create_text("Project Brief").with_css(H1))
        .with_child(section("Overview", "AzulDoc renders a styled document and exports it to PDF via the public API — the printpdf engine walks the window's display list."))
        .with_child(section("Status", "P5: PDF export wired end-to-end. Solid fills export today; text and inline images follow."))
        .with_child(section("Next", "Markdown editing, live preview, and a reference-PDF diff round out the AzulDoc demo."));

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

/// Export the current document to a PDF. Queues the export (runs on the next
/// layout pass, where printpdf walks the display list).
extern "C" fn on_export(mut data: RefAny, mut info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<DocState>() {
        info.export_to_pdf(s.export_path.clone());
        s.exported = true;
    }
    Update::RefreshDom
}

fn main() {
    let data = RefAny::new(DocState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
