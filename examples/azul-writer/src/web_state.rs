//! JSON reflection of the root state, required by the web backend.
//!
//! The server renders the first frame natively and ships the app state inside
//! the HTML; the lifted wasm client rebuilds it and takes over from there.
//! Without a registered pair the backend refuses to start (there would be no
//! way to hand the client its state), so this module is what makes AzWriter
//! deployable as a web app at all.
//!
//! `AppState` cannot be derived directly: it holds session and derived values
//! that are meaningless on the other side of the wire — `undo_stack` /
//! `redo_stack` hold engine `DocumentOperation`s, `content` is a `Dom` cache,
//! and `pagination_thread` / `pages_vv_node` are live handles. So the wire
//! form carries the UI state plus the document AS MARKDOWN — the document's
//! own serialization format, which `DocumentModel::reparse` turns back into
//! the IR and content DOM. serde_json does the encoding; nothing here parses
//! or prints JSON by hand.

use azul::{error::ResultRefAnyString, json::Json, prelude::*};
use serde::{Deserialize, Serialize};

use crate::{document::DocumentModel, AppState, Screen};

#[derive(Serialize, Deserialize)]
struct Wire {
    screen: u8,
    backstage_pane: usize,
    ribbon_tab: usize,
    bold: bool,
    italic: bool,
    underline: bool,
    align: usize,
    selected_style: usize,
    view_mode: usize,
    zoom_percent: f32,
    editing_page: usize,
    /// `None` for the unsaved "Document1".
    path: Option<String>,
    /// The document itself. Markdown is `DocumentModel`'s round-trip format,
    /// so this is lossless for everything the IR can express.
    markdown: String,
    dirty: bool,
}

impl Wire {
    fn from_state(s: &AppState) -> Self {
        Self {
            screen: match s.screen {
                Screen::Editor => 0,
                Screen::Backstage => 1,
            },
            backstage_pane: s.backstage_pane,
            ribbon_tab: s.ribbon_tab,
            bold: s.bold,
            italic: s.italic,
            underline: s.underline,
            align: s.align,
            selected_style: s.selected_style,
            view_mode: s.view_mode,
            zoom_percent: s.zoom_percent,
            editing_page: s.editing_page,
            path: s
                .document
                .path
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            markdown: s.document.markdown.clone(),
            dirty: s.document.dirty,
        }
    }

    fn into_state(self) -> AppState {
        let mut s = AppState::default();
        s.screen = if self.screen == 1 {
            Screen::Backstage
        } else {
            Screen::Editor
        };
        s.backstage_pane = self.backstage_pane;
        s.ribbon_tab = self.ribbon_tab;
        s.bold = self.bold;
        s.italic = self.italic;
        s.underline = self.underline;
        s.align = self.align;
        s.selected_style = self.selected_style;
        s.view_mode = self.view_mode;
        s.zoom_percent = self.zoom_percent;
        s.editing_page = self.editing_page;

        let mut doc = DocumentModel::untitled();
        doc.path = self.path.map(std::path::PathBuf::from);
        doc.markdown = self.markdown;
        // Rebuilds ir + content from the markdown and bumps the generation, so
        // the derived caches the layout callback reads are consistent again.
        doc.reparse();
        doc.dirty = self.dirty;
        s.document = doc;
        s
    }
}

/// `RefAny -> Json`. Returns `Json::null()` rather than panicking: a failure
/// here must surface as "no state to hydrate", not as a crash inside the
/// server's render path.
extern "C" fn app_state_to_json(mut refany: RefAny) -> Json {
    let text = match refany.downcast_ref::<AppState>() {
        Some(s) => match serde_json::to_string(&Wire::from_state(&s)) {
            Ok(t) => t,
            Err(_) => return Json::null(),
        },
        None => return Json::null(),
    };
    // The Az result/option enums implement Drop, so clone out of a
    // match-by-reference instead of moving the parsed payload.
    match &Json::parse(text.as_str()) {
        azul::error::ResultJsonJsonParseError::Ok(j) => j.clone(),
        azul::error::ResultJsonJsonParseError::Err(_) => Json::null(),
    }
}

/// `Json -> RefAny`, re-registering the pair so round-trips keep working on
/// the client (state sync and undo snapshots serialize again over there).
extern "C" fn app_state_from_json(json: Json) -> ResultRefAnyString {
    let wire: Wire = match serde_json::from_str(json.to_string().as_str()) {
        Ok(w) => w,
        Err(e) => return ResultRefAnyString::err(format!("AzWriter AppState: {e}").as_str()),
    };
    let mut r = RefAny::new(wire.into_state());
    register(&mut r);
    ResultRefAnyString::ok(r)
}

/// Register the JSON reflection pair on the root `RefAny`. Call this before
/// `App::create`, on desktop too — it costs two stores and is what lets the
/// same binary be served by the web backend.
pub fn register(data: &mut RefAny) {
    data.set_serialize_fn(app_state_to_json as usize);
    data.set_deserialize_fn(app_state_from_json as usize);
}
