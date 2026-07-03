//! Single-threaded capability pump (MWA-A1).
//!
//! Six managers (gamepad, sensors, geolocation, permission, biometric,
//! keyring) receive data from native backends through process-global
//! channels in `azul_layout::managers::*`. Historically those channels were
//! drained only inside `regenerate_layout()`, which runs only when something
//! *else* already caused UI work — so an idle app (blocked in WaitMessage /
//! select / the NSApp run loop) never observed a pad press, a GPS fix, or a
//! biometric prompt result, and even a busy app saw them one event-pass late.
//!
//! The pump fixes both halves WITHOUT a thread (user constraint: single
//! threaded so the identical code path works on WASM):
//!
//! - [`pump`] runs at the **top of every `process_window_events` pass**,
//!   before event determination — drained state raises the managers'
//!   pending-event flags, and the very same pass turns them into
//!   `GamepadInput` / `SensorChanged` / `GeolocationFix` events. No +1-pass
//!   latency.
//! - While a capability source needs attention with no other input arriving
//!   (a gamepad listener exists, a `GeolocationProbe` is mounted, …), a
//!   recurring shell timer (`CAPABILITY_PUMP_TIMER_ID`) is kept armed by
//!   `PlatformWindow::sync_capability_pump_timer`. Its tick wakes the
//!   blocked platform loop through the ordinary timer machinery (timerfd /
//!   SetTimer / NSTimer / headless tick) and `invoke_expired_timers` then
//!   fires an event pass. No listeners → no timer → a fully idle app burns
//!   zero CPU.
//!
//! `regenerate_layout` keeps only the genuinely layout-coupled halves: the
//! DOM walks that diff probe nodes / listener registrations into the
//! managers (subscriptions are a function of the DOM; drains are not).

use azul_layout::window::LayoutWindow;

/// Poll cadence while a `GamepadInput` listener exists (~60Hz — pad input is
/// latency-sensitive).
pub const GAMEPAD_INTERVAL_MS: u64 = 16;
/// Poll cadence while a `SensorChanged` listener exists (20Hz is plenty for
/// UI-driving motion sensors).
pub const SENSOR_INTERVAL_MS: u64 = 50;
/// Drain cadence while a geolocation subscription is active. Fixes are
/// *pushed* into the channel by native callbacks on OS threads; this only
/// bounds how long a parked fix waits for an otherwise-idle loop.
pub const GEOLOCATION_INTERVAL_MS: u64 = 200;
/// Drain cadence while an async prompt (biometric / keyring / permission)
/// is in flight and its result may be parked by an OS completion handler.
pub const ASYNC_RESULT_INTERVAL_MS: u64 = 100;

/// Drain every capability channel into the managers and dispatch queued
/// outbound requests to the native backends. Returns `true` when any manager
/// state advanced (the caller's event pass will then see pending-event flags
/// from the providers).
///
/// Ordering note: outbound request dispatch (biometric / keyring) runs
/// before the matching result drain so a synchronously-resolving stub's
/// result is folded in the same call.
pub fn pump(lw: &mut LayoutWindow) -> bool {
    let mut changed = false;

    // Gamepad: gilrs enumeration is lazy and the poll is a queue pump — only
    // touch it while some node actually listens for GamepadInput.
    if lw.gamepad_manager.has_listeners() {
        crate::desktop::extra::gamepad::ensure_started();
        crate::desktop::extra::gamepad::poll();
    }
    for state in azul_layout::managers::gamepad::drain_gamepad_states() {
        changed |= lw.gamepad_manager.set_state(state);
    }

    // Sensors: CoreMotion / WinRT registration is listener-gated the same way.
    if lw.sensor_manager.has_listeners() {
        crate::desktop::extra::sensors::ensure_started();
        crate::desktop::extra::sensors::poll();
    }
    for reading in azul_layout::managers::sensors::drain_sensor_readings() {
        changed |= lw.sensor_manager.set_reading(reading);
    }

    // Geolocation: fixes / errors are pushed by the native subscription's
    // OS-thread callbacks; folding raises the manager's pending flags
    // (GeolocationFix / GeolocationError provider events).
    for fix in azul_layout::managers::geolocation::drain_location_fixes() {
        changed |= lw.geolocation_manager.set_latest_fix(fix);
    }
    for error in azul_layout::managers::geolocation::drain_location_errors() {
        lw.geolocation_manager.set_last_error(error);
        changed = true;
    }

    // Permission: async prompt outcomes parked by OS completion handlers.
    for (capability, state) in azul_layout::managers::permission::drain_async_results() {
        changed |= lw.permission_manager.set_status(capability, state);
    }

    // Biometric: availability probe (OnceLock-cached native call), then
    // dispatch queued prompts (tracked in-flight so the timer stays armed
    // until the reply folds back), then fold parked outcomes. A completion
    // always counts as a change — even a repeat outcome answers a fresh
    // request and must dispatch a BiometricResult event.
    lw.biometric_manager
        .set_availability(crate::desktop::extra::biometric::availability_cached());
    let bio_requests = azul_layout::managers::biometric::drain_biometric_requests();
    if !bio_requests.is_empty() {
        lw.biometric_manager
            .mark_requests_dispatched(bio_requests.len() as u32);
        changed = true;
    }
    for prompt in &bio_requests {
        crate::desktop::extra::biometric::request(prompt);
    }
    for result in azul_layout::managers::biometric::drain_biometric_results() {
        lw.biometric_manager.set_last_result(result);
        changed = true;
    }

    // Keyring: same request → native backend → parked-result shape.
    let keyring_requests = azul_layout::managers::keyring::drain_keyring_requests();
    if !keyring_requests.is_empty() {
        lw.keyring_manager
            .mark_requests_dispatched(keyring_requests.len() as u32);
        changed = true;
    }
    for req in &keyring_requests {
        crate::desktop::extra::keyring::request(req);
    }
    for result in azul_layout::managers::keyring::drain_keyring_results() {
        lw.keyring_manager.set_last_result(result);
        changed = true;
    }

    changed
}

/// The pump-timer cadence the current subscription set wants, or `None` when
/// nothing needs unsolicited wake-ups (timer gets disarmed — idle apps stay
/// at zero CPU). The effective cadence is the fastest requirement.
#[must_use]
pub fn desired_interval_ms(lw: &LayoutWindow) -> Option<u64> {
    let mut interval: Option<u64> = None;
    let mut want = |ms: u64| {
        interval = Some(interval.map_or(ms, |cur| cur.min(ms)));
    };

    if lw.gamepad_manager.has_listeners() {
        want(GAMEPAD_INTERVAL_MS);
    }
    if lw.sensor_manager.has_listeners() {
        want(SENSOR_INTERVAL_MS);
    }
    if lw.geolocation_manager.has_active_subscription() {
        want(GEOLOCATION_INTERVAL_MS);
    }
    // Async prompt outcomes (MWA-A1b): while a biometric / keyring op is in
    // flight or an OS permission prompt is up, keep draining so the reply
    // reaches callbacks even if the user never touches the window again.
    if lw.permission_manager.has_pending_async()
        || lw.biometric_manager.has_pending_async()
        || lw.keyring_manager.has_pending_async()
    {
        want(ASYNC_RESULT_INTERVAL_MS);
    }

    interval
}

/// Build the recurring pump timer. Its callback is a no-op marker — the real
/// work happens in `process_window_events` (top-of-pass [`pump`]), which
/// `invoke_expired_timers` triggers whenever this timer expires.
#[must_use]
pub fn make_pump_timer(interval_ms: u64) -> azul_layout::timer::Timer {
    use azul_core::refany::RefAny;
    use azul_core::task::{Duration as AzulDuration, SystemTimeDiff};
    use azul_layout::callbacks::ExternalSystemCallbacks;
    use azul_layout::timer::{Timer, TimerCallbackType};

    let external = ExternalSystemCallbacks::rust_internal();
    Timer::create(
        RefAny::new(()),
        pump_timer_marker_callback as TimerCallbackType,
        external.get_system_time_fn,
    )
    .with_interval(AzulDuration::System(SystemTimeDiff {
        secs: interval_ms / 1000,
        nanos: ((interval_ms % 1000) * 1_000_000) as u32,
    }))
}

/// Read the armed interval back out of a timer (for cadence-change detection
/// in `sync_capability_pump_timer`).
#[must_use]
pub fn timer_interval_ms(timer: &azul_layout::timer::Timer) -> Option<u64> {
    use azul_core::task::{Duration as AzulDuration, OptionDuration};
    match &timer.interval {
        OptionDuration::Some(AzulDuration::System(d)) => {
            Some(d.secs * 1000 + u64::from(d.nanos / 1_000_000))
        }
        _ => None,
    }
}

/// MWA-B12: one-shot wake-up timer — fires once after `delay_ms`, causing
/// `invoke_expired_timers` to run an event pass (see LONG_PRESS_TIMER_ID),
/// then self-terminates via its callback.
#[must_use]
pub fn make_one_shot_pass_timer(delay_ms: u64) -> azul_layout::timer::Timer {
    use azul_core::refany::RefAny;
    use azul_core::task::{Duration as AzulDuration, SystemTimeDiff};
    use azul_layout::callbacks::ExternalSystemCallbacks;
    use azul_layout::timer::{Timer, TimerCallbackType};

    let external = ExternalSystemCallbacks::rust_internal();
    Timer::create(
        RefAny::new(()),
        one_shot_pass_marker_callback as TimerCallbackType,
        external.get_system_time_fn,
    )
    .with_interval(AzulDuration::System(SystemTimeDiff {
        secs: delay_ms / 1000,
        nanos: ((delay_ms % 1000) * 1_000_000) as u32,
    }))
}

extern "C" fn one_shot_pass_marker_callback(
    _data: azul_core::refany::RefAny,
    _info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    // One firing is the whole job (invoke_expired_timers runs the pass).
    azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
}

extern "C" fn pump_timer_marker_callback(
    _data: azul_core::refany::RefAny,
    _info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    // Intentionally empty: the timer exists to wake the blocked platform
    // loop; invoke_expired_timers notices CAPABILITY_PUMP_TIMER_ID expired
    // and runs a process_window_events pass, whose top-of-pass pump() does
    // the actual draining. Keeping the callback inert means user-visible
    // timer semantics (run_count, tick bookkeeping) stay ordinary.
    azul_core::callbacks::TimerCallbackReturn::continue_unchanged()
}
