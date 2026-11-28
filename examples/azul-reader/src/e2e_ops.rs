//! AzReader's ops for the scenario files and the debug server: the sample book a phone cannot be
//! started with (it has no command line for `--sample`). The same scenario file runs on a
//! desktop and on a phone. They exist only where the debug server does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};

use crate::{
    app::AppState,
    commands::on_job_done,
    jobs::{self, Job},
};

/// The sample book is added to the library and opens.
pub const OP_OPEN_SAMPLE: &str = "azreader_open_sample";
/// The sample book is added to the library (the library screen stays).
pub const OP_IMPORT_SAMPLE: &str = "azreader_import_sample";

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
    ] {
        let json = Json::parse(AzString::from(op_def(name, summary).as_str()))
            .into_result()
            .unwrap_or_else(|_| Json::null());
        config.add_e2e_command(AzString::from(name), json, on_op);
    }
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, _args: AzString) -> CustomE2eOpResult {
    let open = match name.as_str() {
        OP_OPEN_SAMPLE => true,
        OP_IMPORT_SAMPLE => false,
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
