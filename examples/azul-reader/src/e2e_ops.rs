//! AzReader's ops for the scenario files and the debug server: the sample book a phone cannot be
//! started with (it has no command line for `--sample`). The same scenario file runs on a
//! desktop and on a phone. They exist only where the debug server does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};

use crate::{
    app::{AppState, Screen},
    commands::on_job_done,
    jobs::{self, Job},
};

/// The sample book is added to the library and opens.
pub const OP_OPEN_SAMPLE: &str = "azreader_open_sample";
/// The sample book is added to the library (the library screen stays).
pub const OP_IMPORT_SAMPLE: &str = "azreader_import_sample";
/// Waits (the step is run again after each turn of the event loop) until what it names has
/// happened: the library holds a book, a chapter is laid out, a notice is up. Files and chapters are made on
/// threads, so a scenario cannot count frames for them.
pub const OP_WAIT: &str = "azreader_wait";

/// Whether what a wait step names has happened: `None` for a name nobody knows.
#[must_use]
pub fn ready(what: &str, st: &AppState) -> Option<bool> {
    match what {
        // The library was read and holds a book.
        "library" => Some(st.listed && !st.entries.is_empty()),
        // A book is open and its chapter on screen is laid out for the window.
        "chapter" => Some(st.screen == Screen::Reader && st.chapter.is_some() && st.pending.is_none()),
        // A job is done and said so (a copy of the book saved: it is read on a thread).
        "notice" => Some(!st.notice.is_empty()),
        _ => None,
    }
}

/// What a wait step names (`{"for": "library"}`; none: empty).
#[must_use]
pub fn waited_for(args: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args)
        .ok()
        .and_then(|v| v.get("for").and_then(serde_json::Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

/// The description `list_app_ops` shows for an op.
#[must_use]
pub fn op_def(name: &str, summary: &str) -> String {
    serde_json::json!({
        "name": name,
        "summary": summary,
        "description": summary,
        "args": [],
        "examples": [],
    })
    .to_string()
}

/// Adds the ops to `config`.
pub fn register_ops(config: &mut AppConfig) {
    for (name, summary) in [
        (OP_OPEN_SAMPLE, "add the sample book (two chapters of Alice) to the library and open it"),
        (OP_IMPORT_SAMPLE, "add the sample book to the library"),
        (OP_WAIT, "wait until the library holds a book (for: library), a chapter is laid out (for: chapter) or a notice is up (for: notice)"),
    ] {
        let json = Json::parse(AzString::from(op_def(name, summary).as_str()))
            .into_result()
            .unwrap_or_else(|_| Json::null());
        config.add_e2e_command(AzString::from(name), json, on_op);
    }
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    let open = match name.as_str() {
        OP_OPEN_SAMPLE => true,
        OP_IMPORT_SAMPLE => false,
        OP_WAIT => {
            let what = waited_for(args.as_str());
            let answer = match data.downcast_ref::<AppState>().map(|st| ready(&what, &st)) {
                Some(Some(true)) => "{\"success\":true}".to_string(),
                Some(Some(false)) => "{\"success\":true,\"pending\":true}".to_string(),
                Some(None) => serde_json::json!({ "success": false, "error": format!("nothing to wait for named {what:?}") }).to_string(),
                None => "{\"success\":false,\"error\":\"no app state\"}".to_string(),
            };
            return CustomE2eOpResult {
                handled: true,
                json: AzString::from(answer.as_str()),
            };
        }
        _ => {
            return CustomE2eOpResult {
                handled: false,
                json: AzString::from(""),
            }
        }
    };
    let handle = data.clone();
    let Some(mut st) = data.downcast_mut::<AppState>() else {
        return CustomE2eOpResult {
            handled: true,
            json: AzString::from("{\"success\":false,\"error\":\"no app state\"}"),
        };
    };
    // The import's answer opens the book when `sample_done` says the sample asked for it.
    st.sample_done = open;
    let job = Job::ImportBytes {
        name: crate::sample::FILE_NAME.to_string(),
        bytes: crate::sample::sample_epub(),
    };
    jobs::spawn(&mut info, &handle, &st.data_root, job, on_job_done);
    CustomE2eOpResult {
        handled: true,
        json: AzString::from("{\"success\":true}"),
    }
}

#[cfg(test)]
#[path = "e2e_ops_test.rs"]
mod e2e_ops_test;
