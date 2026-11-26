//! The debug server's op that fills the bottom bar with mock conversations: for the screenshots of a bar with none, one
//! and many chips, and for the scripts that drive a real window. It exists only where the debug server does.

use azul::{error::CustomE2eOpResult, json::Json, prelude::*, str::String as AzString, time::SystemTimeDiff};

use crate::{chips, AppState};

/// The op's name.
pub const OP_NAME: &str = "azreview_mock_chips";

/// Adds the debug server's op to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let def = format!(
        "{{\"name\":\"{OP_NAME}\",\"summary\":\"fill the bottom bar with mock conversations\",\
         \"description\":\"count: how many conversations (0 to {}), spread over the folder's files, each going \
         through the states working, suggested, done.\",\"args\":[{{\"name\":\"count\",\"type\":\"number\"}}],\
         \"examples\":[]}}",
        chips::MAX_CHIPS
    );
    let json = Json::parse(AzString::from(def.as_str()))
        .into_result()
        .unwrap_or_else(|_| Json::null());
    config.add_e2e_command(AzString::from(OP_NAME), json, on_op);
}

/// How many conversations the op's arguments ask for (none: 0), held to the bar's cap.
fn asked(args: &str) -> usize {
    serde_json::from_str::<serde_json::Value>(args)
        .ok()
        .and_then(|v| v.get("count").and_then(serde_json::Value::as_u64))
        .map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX))
        .min(chips::MAX_CHIPS)
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    if name.as_str() != OP_NAME {
        return CustomE2eOpResult { handled: false, json: AzString::from("") };
    }
    let count = asked(args.as_str());
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        let files: Vec<String> = s.tree.file_paths(400);
        s.chips = chips::mock(count, &files);
    }
    // The window is built again, now that it has the conversations.
    let timer = Timer::create(data.clone(), on_rebuild, info.get_system_time_fn())
        .with_delay(Duration::System(SystemTimeDiff::from_millis(1)));
    info.add_timer(TimerId::unique(), timer);
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(format!("{{\"success\":true,\"chips\":{count}}}").as_str()),
    }
}

extern "C" fn on_rebuild(_data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::terminate_and_refresh_dom()
}

#[cfg(test)]
#[path = "chips_op_test.rs"]
mod chips_op_test;
