//! The debug server's op that says what AzReview's state is (the pen and the nib in hand, the panels, the number of
//! conversations): for the scripts that drive a window and cannot read an icon. It exists only where the debug server
//! does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString};

use crate::AppState;

/// The op's name.
pub const OP_NAME: &str = "azreview_state";

/// The state as JSON: `pen` (the place of the pen in hand), `meaning`, `nib`, `tree_open`, `tree_w`, `right_open`,
/// `right_w`, `chips` (the open file's conversations), `mic` (the microphone's state), `dictating` (a dictation runs),
/// `ready` (its microphone takes words), `partial` (the words so far), `slashes` and `deleted_words` (what quick slashes took), `standby` (the policy: `mode`, `enabled`, `armed`, `arms`,
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

/// Adds the debug server's op to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"what AzReview's state is\",\
         \"description\":\"pen, meaning, nib, tree_open, tree_w, right_open, right_w, chips, mic, dictating, ready, partial, slashes, deleted_words, standby.\",\"args\":[],\"examples\":[]}}"
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
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
