//! The debug server's op that adds a note to the open file, as a dictation would, without the microphone: for the
//! scripts that drive the conversations (the chips in the margin, the agent's answers) and for the screenshots. It exists
//! only where the debug server does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};

use crate::{
    agent_window,
    model::{Note, NoteState},
    ui, AppState,
};

/// The op's name.
pub const OP_NAME: &str = "azreview_add_note";
/// The most bytes of words a note takes.
pub const MAX_TEXT_BYTES: usize = 4_096;

/// What the op is asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Add {
    pub text: String,
    pub page: usize,
    /// Where the anchor is on the page, px.
    pub y: f32,
    /// The agent is asked about it at once.
    pub ask: bool,
    /// Its conversation is open in the margin.
    pub open: bool,
    /// A word about what was cut.
    pub cut: Option<String>,
}

/// The op's arguments, with the page and the height held to the file's `pages`.
///
/// # Errors
/// A sentence for the script: no words, or arguments that are no JSON.
pub fn parse(args: &str, pages: usize) -> Result<Add, String> {
    let json: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("the arguments are no JSON: {e}"))?;
    let text = json.get("text").and_then(serde_json::Value::as_str).unwrap_or("").trim().to_string();
    if text.is_empty() {
        return Err("a note needs words: \"text\"".to_string());
    }
    let (kept, cut) = if text.len() > MAX_TEXT_BYTES {
        let mut end = MAX_TEXT_BYTES;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        (text[..end].to_string(), Some(format!("the words were {} bytes: cut to {MAX_TEXT_BYTES} bytes", text.len())))
    } else {
        (text, None)
    };
    let page = json
        .get("page")
        .and_then(serde_json::Value::as_u64)
        .map_or(0, |p| usize::try_from(p).unwrap_or(usize::MAX))
        .min(pages.saturating_sub(1));
    let y = json.get("y").and_then(serde_json::Value::as_f64).map_or(ui::PAGE_PAD, |y| y as f32);
    let y = y.clamp(ui::PAGE_PAD, ui::page_h() - ui::PAGE_PAD);
    Ok(Add {
        text: kept,
        page,
        y,
        ask: json.get("ask").and_then(serde_json::Value::as_bool).unwrap_or(false),
        open: json.get("open").and_then(serde_json::Value::as_bool).unwrap_or(false),
        cut,
    })
}

/// The scripted dictation's op: a dictation that says what it is given, so that every step of one runs where nobody may
/// open a microphone.
pub const OP_DICTATE: &str = "azreview_dictate";
/// How long a scripted utterance takes when the op says nothing, ms.
pub const DEFAULT_GAP_MS: u64 = 600;
/// The longest a scripted utterance takes, ms.
pub const MAX_GAP_MS: u64 = 5_000;

/// What the dictation op is asked.
#[derive(Debug, Clone, PartialEq)]
pub struct Dictate {
    pub words: Vec<String>,
    pub page: usize,
    pub y: f32,
    pub gap_ms: u64,
}

/// The dictation op's arguments, with the page and the height held to the file's `pages`.
///
/// # Errors
/// A sentence for the script: no words, or arguments that are no JSON.
pub fn parse_dictate(args: &str, pages: usize) -> Result<Dictate, String> {
    let json: serde_json::Value = serde_json::from_str(args).map_err(|e| format!("the arguments are no JSON: {e}"))?;
    let words: Vec<String> = json
        .get("words")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(serde_json::Value::as_str)
                .map(|w| w.trim().to_string())
                .filter(|w| !w.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if words.is_empty() {
        return Err("a dictation needs words: \"words\", a list of utterances".to_string());
    }
    let page = json
        .get("page")
        .and_then(serde_json::Value::as_u64)
        .map_or(0, |p| usize::try_from(p).unwrap_or(usize::MAX))
        .min(pages.saturating_sub(1));
    let y = json.get("y").and_then(serde_json::Value::as_f64).map_or(ui::PAGE_PAD, |y| y as f32);
    let y = y.clamp(ui::PAGE_PAD, ui::page_h() - ui::PAGE_PAD);
    let gap_ms = json
        .get("gap_ms")
        .and_then(serde_json::Value::as_u64)
        .map_or(DEFAULT_GAP_MS, |g| g.min(MAX_GAP_MS));
    Ok(Dictate { words, page, y, gap_ms })
}

/// Adds the debug server's ops to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_DICTATE}\",\"summary\":\"a dictation that says what it is given (no microphone)\",\
         \"description\":\"words: the utterances, one a call; page: 0-based; y: px on the page; gap_ms: how long each \
         takes. The note, its conversation and the agent go the way a dictation's do. Answers the note's id.\",\
         \"args\":[{{\"name\":\"words\",\"type\":\"array\"}},{{\"name\":\"page\",\"type\":\"number\"}},\
         {{\"name\":\"y\",\"type\":\"number\"}},{{\"name\":\"gap_ms\",\"type\":\"number\"}}],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str())).into_result().unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_DICTATE), json, on_dictate);
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"add a note (a conversation) to the open file\",\
         \"description\":\"text: the words; page: 0-based; y: px on the page; ask: ask the agent at once; open: open it in \
         the margin. Answers the note's id.\",\"args\":[{{\"name\":\"text\",\"type\":\"string\"}},\
         {{\"name\":\"page\",\"type\":\"number\"}},{{\"name\":\"y\",\"type\":\"number\"}},\
         {{\"name\":\"ask\",\"type\":\"boolean\"}},{{\"name\":\"open\",\"type\":\"boolean\"}}],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str())).into_result().unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

fn answer(json: &str) -> CustomE2eOpResult {
    CustomE2eOpResult { handled: true, json: AzString::from(json) }
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let (add, id) = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return answer("{\"success\":false,\"error\":\"no state\"}");
        };
        let Some(pages) = s.file().map(crate::code::SourceFile::page_count) else {
            return answer("{\"success\":false,\"error\":\"no file is open\"}");
        };
        let add = match parse(args.as_str(), pages) {
            Ok(add) => add,
            Err(why) => return answer(&serde_json::json!({"success": false, "error": why}).to_string()),
        };
        let id = s.next_note_id;
        s.next_note_id += 1;
        let language = crate::voice::language_of(&s.kit);
        let mut note = Note::listening(id, add.page, 120.0, add.y, &language);
        note.text = add.text.clone();
        note.state = NoteState::Done;
        s.notes.push(note);
        s.sync_chip(id);
        if add.open {
            s.open_conv = Some(id);
        }
        (add, id)
    };
    if add.ask {
        let _ = agent_window::ask(&mut data, &mut info, id);
    }
    info.trigger_all_virtual_view_rerender();
    answer(&serde_json::json!({"success": true, "id": id, "cut": add.cut}).to_string())
}

extern "C" fn on_dictate(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_DICTATE {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let (dictate, id) = {
        let Some(s) = data.downcast_ref::<AppState>() else {
            return answer("{\"success\":false,\"error\":\"no state\"}");
        };
        let Some(pages) = s.file().map(crate::code::SourceFile::page_count) else {
            return answer("{\"success\":false,\"error\":\"no file is open\"}");
        };
        if s.voice.running.is_some() {
            return answer("{\"success\":false,\"error\":\"a dictation runs already\"}");
        }
        match parse_dictate(args.as_str(), pages) {
            Ok(dictate) => (dictate, s.next_note_id),
            Err(why) => return answer(&serde_json::json!({"success": false, "error": why}).to_string()),
        }
    };
    let anchor = crate::dwell::Anchor { page: dictate.page, x: 120.0, y: dictate.y, contact: false };
    let _ = crate::voice::start_scripted(&mut data, &mut info, anchor, dictate.words, dictate.gap_ms);
    answer(&serde_json::json!({"success": true, "id": id}).to_string())
}

#[cfg(test)]
#[path = "notes_op_test.rs"]
mod notes_op_test;
