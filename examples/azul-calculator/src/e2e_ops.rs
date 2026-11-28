//! AzCalculator's ops for the scenario files and the debug server: a calculation typed as the
//! keyboard types it, and the sample history. A phone has no command line for `--sample` and no
//! keyboard focus a scenario can rely on; the same file runs on a desktop and on a phone. They
//! exist only where the debug server does.

use azul::{
    error::CustomE2eOpResult,
    json::Json,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};

use crate::ui::{after_calc, now_secs, sample_history, with_app};

/// Types `text` as the keyboard types it (digits, operators, names like `sqrt`, `=`).
pub const OP_TYPE: &str = "azcalculator_type";
/// Fills the history with the sample calculations.
pub const OP_LOAD_SAMPLE: &str = "azcalculator_load_sample";

/// The description `list_app_ops` shows for an op.
#[must_use]
pub fn op_def(name: &str, summary: &str, args: serde_json::Value) -> String {
    serde_json::json!({
        "name": name,
        "summary": summary,
        "description": summary,
        "args": args,
        "examples": [],
    })
    .to_string()
}

/// Adds the ops to `config`.
pub fn register_ops(config: &mut AppConfig) {
    let ops = [
        (
            OP_TYPE,
            "type text into the calculator as the keyboard does (12*7=)",
            serde_json::json!([{ "name": "text", "type": "string" }]),
        ),
        (
            OP_LOAD_SAMPLE,
            "fill the history with the sample calculations",
            serde_json::json!([]),
        ),
    ];
    for (name, summary, args) in ops {
        let json = Json::parse(AzString::from(op_def(name, summary, args).as_str()))
            .into_result()
            .unwrap_or_else(|_| Json::null());
        config.add_e2e_command(AzString::from(name), json, on_op);
    }
}

/// The text an `azcalculator_type` step carries (none: empty).
#[must_use]
pub fn typed_text(args: &str) -> String {
    serde_json::from_str::<serde_json::Value>(args)
        .ok()
        .and_then(|v| v.get("text").and_then(serde_json::Value::as_str).map(str::to_string))
        .unwrap_or_default()
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, args: AzString) -> CustomE2eOpResult {
    let answer = |json: serde_json::Value| CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string().as_str()),
    };
    let result = match name.as_str() {
        OP_TYPE => {
            let text = typed_text(args.as_str());
            let mut meant = 0;
            let _ = with_app(&mut data, &mut info, |s, info, handle| {
                meant = s.calc.type_text(&text, now_secs());
                after_calc(s, info, handle);
            });
            answer(serde_json::json!({ "success": true, "meant": meant }))
        }
        OP_LOAD_SAMPLE => {
            let _ = with_app(&mut data, &mut info, |s, info, handle| {
                s.calc.history = sample_history();
                s.calc.history_dirty = true;
                after_calc(s, info, handle);
            });
            answer(serde_json::json!({ "success": true }))
        }
        _ => {
            return CustomE2eOpResult {
                handled: false,
                json: AzString::from(""),
            }
        }
    };
    // The window is built again, now that the state has changed.
    let timer = Timer::create(data.clone(), on_rebuild, info.get_system_time_fn())
        .with_delay(Duration::System(SystemTimeDiff::from_millis(1)));
    info.add_timer(TimerId::unique(), timer);
    result
}

extern "C" fn on_rebuild(_data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::terminate_and_refresh_dom()
}

#[cfg(test)]
#[path = "e2e_ops_test.rs"]
mod e2e_ops_test;
