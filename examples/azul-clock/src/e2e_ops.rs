//! AzClock's ops for the scenario files and the debug server: the data a phone cannot be started
//! with (it has no command line for `--sample`), and a ring to look at. The same scenario file
//! runs on a desktop and on a phone. They exist only where the debug server does.

use azul::{
    error::CustomE2eOpResult,
    json::Json,
    prelude::*,
    str::String as AzString,
    task::{Timer, TimerId},
    time::{Duration, SystemTimeDiff},
};
use chrono::{Local, Utc};

use crate::{
    alarm::{instant, Ring, RingKind},
    store,
    ui::{now_ms, start_ringing, with_app, Ringing},
};

/// Fills the app with the sample (three alarms, two timers, four cities, the stopwatch's laps).
pub const OP_LOAD_SAMPLE: &str = "azclock_load_sample";
/// The first alarm rings, as if it were due now.
pub const OP_RING: &str = "azclock_ring";

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
        (OP_LOAD_SAMPLE, "fill the clock with the sample alarms, timers, cities and laps"),
        (OP_RING, "the first alarm rings now (a tone, the overlay)"),
    ] {
        let json = Json::parse(AzString::from(op_def(name, summary).as_str()))
            .into_result()
            .unwrap_or_else(|_| Json::null());
        config.add_e2e_command(AzString::from(name), json, on_op);
    }
}

fn answer(json: &serde_json::Value) -> CustomE2eOpResult {
    CustomE2eOpResult {
        handled: true,
        json: AzString::from(json.to_string().as_str()),
    }
}

extern "C" fn on_op(mut data: RefAny, mut info: CallbackInfo, name: AzString, _args: AzString) -> CustomE2eOpResult {
    let answered = match name.as_str() {
        OP_LOAD_SAMPLE => {
            let loaded = store::sample(Utc::now(), &Local);
            let counts = (
                loaded.alarms.len(),
                loaded.timers.len(),
                loaded.world.as_ref().map_or(0, |w| w.cities.len()),
            );
            let _ = with_app(&mut data, &mut info, |s, _info, _| {
                s.adopt(loaded, true);
                s.loaded = true;
            });
            answer(&serde_json::json!({
                "success": true,
                "alarms": counts.0,
                "timers": counts.1,
                "cities": counts.2,
            }))
        }
        OP_RING => {
            let mut rang = false;
            let _ = with_app(&mut data, &mut info, |s, info, _| {
                if let Some(id) = s.alarms.first().map(|a| a.id.clone()) {
                    let ring = Ring {
                        at: instant(now_ms()),
                        kind: RingKind::Occurrence,
                    };
                    start_ringing(s, info, Ringing::Alarm { id, ring });
                    rang = true;
                }
            });
            if rang {
                answer(&serde_json::json!({ "success": true }))
            } else {
                answer(&serde_json::json!({ "success": false, "error": "there is no alarm to ring" }))
            }
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
    answered
}

extern "C" fn on_rebuild(_data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    TimerCallbackReturn::terminate_and_refresh_dom()
}

#[cfg(test)]
#[path = "e2e_ops_test.rs"]
mod e2e_ops_test;
