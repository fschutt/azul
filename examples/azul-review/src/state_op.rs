//! The debug server's op that says what AzReview's state is (the pen and the nib in hand, the panels, the number of
//! conversations): for the scripts that drive a window and cannot read an icon. It exists only where the debug server
//! does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};

use crate::AppState;

/// The op's name.
pub const OP_NAME: &str = "azreview_state";

/// The state as JSON: `pen` (the place of the pen in hand), `meaning`, `nib`, `tree_open`, `tree_w`, `right_open`,
/// `right_w`, `chips` (the open file's conversations), `mic` (the microphone's state), `dictating` (a dictation runs),
/// `ready` (its microphone takes words), `partial` (the words so far), `tree` (the rows, the files and folders known so far, the file open), `slashes` and `deleted_words` (what quick slashes took), `standby` (the policy: `mode`, `enabled`, `armed`, `arms`,
/// `disarms`).
#[must_use]
pub fn state_json(s: &AppState) -> String {
    serde_json::json!({
        "pen": s.pens.selected(),
        "meaning": s.ink_meaning().label(),
        "nib": s.tool().label(),
        "tree_open": s.panels.tree_open,
        "tree_w": s.panels.tree_w,
        "right_open": s.panels.right_open,
        "right_w": s.panels.right_w,
        "chips": s.open_chips().len(),
        "mic": s.voice.mic.name(),
        "dictating": s.voice.running.is_some(),
        "ready": s.voice.is_ready(),
        "partial": s.voice.partial(),
        "tree": {
            "rows": s.rows.len(),
            "files": s.tree.file_count(),
            "folders": s.tree.dir_count(),
            "open": s.file().map(|f| f.display.clone()),
        },
        "slashes": s.slashes.len(),
        "deleted_words": s.slashes.iter().map(|d| d.words.len()).sum::<usize>(),
        "standby": {
            "mode": s.voice.arming.config().standby.name(),
            "enabled": s.voice.can_arm,
            "armed": s.voice.arming.is_armed(),
            "arms": s.voice.arms,
            "disarms": s.voice.disarms,
        },
    })
    .to_string()
}

/// The op that lists and opens every folder of the tree (for the scripts that measure a whole workspace).
pub const OP_EXPAND_ALL: &str = "azreview_expand_all";

/// Adds the debug server's ops to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_EXPAND_ALL}\",\"summary\":\"list and open every folder of the tree\",\
         \"description\":\"On the window's thread: how many folders were listed, how long it took (ms), and the tree's \
         files, folders and rows afterwards.\",\"args\":[],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_EXPAND_ALL), json, on_expand_all);
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"what AzReview's state is\",\
         \"description\":\"pen, meaning, nib, tree_open, tree_w, right_open, right_w, chips, mic, dictating, ready, partial, tree, slashes, deleted_words, standby.\",\"args\":[],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

extern "C" fn on_expand_all(mut data: RefAny, mut info: CallbackInfo, name: AzString, _args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_EXPAND_ALL {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let started = std::time::Instant::now();
    let (listed, files, folders, rows) = data.downcast_mut::<AppState>().map_or((0, 0, 0, 0), |mut s| {
        let listed = s.expand_all();
        (listed, s.tree.file_count(), s.tree.dir_count(), s.rows.len())
    });
    info.trigger_all_virtual_view_rerender();
    let ms = started.elapsed().as_millis();
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(
            format!("{{\"success\":true,\"listed\":{listed},\"ms\":{ms},\"files\":{files},\"folders\":{folders},\"rows\":{rows}}}")
                .as_str(),
        ),
    }
}

extern "C" fn on_op(mut data: RefAny, _info: CallbackInfo, name: AzString, _args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let state = data.downcast_ref::<AppState>().map_or_else(|| "{}".to_string(), |s| state_json(&s));
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(format!("{{\"success\":true,\"state\":{state}}}").as_str()),
    }
}

#[cfg(test)]
#[path = "state_op_test.rs"]
mod state_op_test;
