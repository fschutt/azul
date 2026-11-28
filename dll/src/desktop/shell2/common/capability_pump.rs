//! Single-threaded capability pump (MWA-A1).
//!
//! Six managers (gamepad, sensors, geolocation, permission, biometric,
//! keyring) receive data from native backends through the app's channels
//! (`azul_layout::managers::app_services::AppServices`, held by every window
//! as `LayoutWindow::app_services`). Historically those channels were
//! drained only inside `regenerate_layout()`, which runs only when something
//! *else* already caused UI work — so an idle app (blocked in WaitMessage /
//! select / the NSApp run loop) never observed a pad press, a GPS fix, or a
//! biometric prompt result, and even a busy app saw them one event-pass late.
//!
//! The pump fixes both halves WITHOUT a thread (user constraint: single
//! threaded so the identical code path works on WASM):
//!
//! - [`pump`] runs at the **top of every `process_window_events` pass**, before event determination
//!   — drained state raises the managers' pending-event flags, and the very same pass turns them
//!   into `GamepadInput` / `SensorChanged` / `GeolocationFix` events. No +1-pass latency.
//! - While a capability source needs attention with no other input arriving (a gamepad listener
//!   exists, a `GeolocationProbe` is mounted, …), a recurring shell timer
//!   (`CAPABILITY_PUMP_TIMER_ID`) is kept armed by `PlatformWindow::sync_capability_pump_timer`.
//!   Its tick wakes the blocked platform loop through the ordinary timer machinery (timerfd /
//!   SetTimer / NSTimer / headless tick) and `invoke_expired_timers` then fires an event pass. No
//!   listeners → no timer → a fully idle app burns zero CPU.
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
/// How often the platforms' quiet read of the permissions in use is folded into the manager.
pub const PERMISSION_PROBE_INTERVAL_MS: u64 = 1000;
/// The same while a Subscribe waits for the user's gesture.
pub const PERMISSION_WAITING_PROBE_INTERVAL_MS: u64 = 250;
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
        crate::desktop::extra::gamepad::poll(&lw.app_services.gamepad);
    }
    for state in lw.app_services.gamepad.drain() {
        changed |= lw.gamepad_manager.set_state(state);
    }

    // Sensors: started by DEMAND (a `SensorChanged` node, or an app that reads a kind: `get_sensor_reading`), never at
    // launch, and stopped when nobody wants them any more (`sensors::sync`). The poll follows what runs.
    {
        let listeners = lw.sensor_manager.has_listeners();
        crate::desktop::extra::sensors::sync(&lw.app_services.sensors, listeners);
        if listeners || crate::desktop::extra::sensors::needs_pump(&lw.app_services.sensors) {
            crate::desktop::extra::sensors::poll(&lw.app_services.sensors);
        }
    }
    for reading in lw.app_services.sensors.drain_readings() {
        changed |= lw.sensor_manager.set_reading(reading);
    }
    if let Some(proximity) = lw.app_services.sensors.take_proximity() {
        changed |= lw.sensor_manager.set_proximity(proximity);
    }

    // Generic HID. Enumerated ONCE (hidraw offers no hotplug signal without
    // libudev, and re-scanning /dev every pass would stat dozens of nodes at
    // frame rate), then polled - and enumerated from the FIRST USE on, not at
    // the first window activation: opening every HID device of the machine is
    // 146 ms of this thread on a Mac (IOHIDDeviceOpen, measured by a stack
    // sample of the recorder's demo) for an app that uses none of them. The
    // first use is a node that listens for `HidReport`, a pad listened to
    // (its IMU and touch surface ride the raw HID stream, below) or a callback
    // that asks (`get_hid_devices()` / `get_hid_reports()`, poll-style
    // accessors an app can call without ever subscribing: they ask for the
    // enumeration, and the list is there from the next pass). Polling is
    // skipped when nothing is open, which is the cost on a machine with no
    // accessible hidraw node.
    {
        let listening = lw.hid_manager.has_listeners() || lw.gamepad_manager.has_listeners();
        if lw.app_services.hid.claim_enumeration_if_used(listening) {
            crate::desktop::extra::hid::enumerate(&lw.app_services.hid);
        }
        crate::desktop::extra::hid::poll(&lw.app_services.hid);
    }
    // `None` means UNCHANGED, not empty: treating it as empty would clear the
    // device list on every pass that did not re-enumerate.
    if let Some(devices) = lw.app_services.hid.take_devices() {
        lw.hid_manager.set_devices(devices);
        changed = true;
    }
    // PER-PASS: the manager holds THIS pass's reports for every reader
    // (`get_hid_reports` copies, the pad parser of 8f-i-a-i-b reads the same
    // slice). Nothing ever cleared it before, so `get_hid_reports` answered
    // the whole process history and the buffer grew without bound.
    drop(lw.hid_manager.take_reports());
    for report in lw.app_services.hid.drain_reports() {
        lw.hid_manager.push_report(report);
        changed = true;
    }
    // PlayStation pads' IMU and touch surface ride the raw HID stream
    // (8f-i-a-i-b): decoded here, after the fold, off the same slice the
    // app reads. Gated like the gilrs poll - motion nobody listens for is
    // work nobody asked for.
    #[cfg(not(any(target_os = "ios", target_os = "android")))]
    if lw.gamepad_manager.has_listeners() {
        changed |= crate::desktop::extra::gamepad::ingest_hid_reports(lw);
    }

    // Geolocation: fixes / errors are pushed by the native subscription's
    // OS-thread callbacks; folding raises the manager's pending flags
    // (GeolocationFix / GeolocationError provider events).
    for fix in lw.app_services.geolocation.drain_fixes() {
        changed |= lw.geolocation_manager.set_latest_fix(fix);
    }
    for error in lw.app_services.geolocation.drain_errors() {
        lw.geolocation_manager.set_last_error(error);
        changed = true;
    }

    // Permission: async prompt outcomes parked by OS completion handlers.
    for (capability, state) in lw.app_services.permission.drain() {
        changed |= lw.permission_manager.set_status(capability, state);
    }
    // The platforms' QUIET read of the capabilities in use - the ones an app asked the state of and
    // the ones with a probe node (it never prompts) - at most once a second, with each pass the loop
    // runs (it arms no timer of its own; the window coming back from the settings is a pass): the OS knows more than the
    // manager (a switch the user flipped in the settings, a camera a backend opened on its own)
    // and `get_permission_status` reads the manager. An app that asks nothing reads nothing.
    // A Subscribe that waits for the user's gesture is looked at again, sooner: the state the read
    // finds may admit it (granted already: nothing can be asked), and so does a gesture.
    let waiting = lw.permission_manager.has_held() || lw.geolocation_manager.has_held_subscribe();
    let every_ms = if waiting {
        PERMISSION_WAITING_PROBE_INTERVAL_MS
    } else {
        PERMISSION_PROBE_INTERVAL_MS
    };
    if lw.permission_manager.probe_due(every_ms) {
        for capability in lw.permission_manager.capabilities_in_use() {
            let probed = crate::desktop::extra::permission::probe_status(capability);
            changed |= lw.permission_manager.observe(capability, probed);
        }
    }
    if lw.permission_manager.has_held() {
        let events = lw.permission_manager.take_dispatchable();
        if !events.is_empty() {
            crate::desktop::extra::permission::apply_diff_events(
                &events,
                &lw.app_services.permission,
            );
            changed = true;
        }
    }
    if lw.geolocation_manager.has_held_subscribe() {
        let events = {
            let permissions = &mut lw.permission_manager;
            lw.geolocation_manager.take_admitted_events(|| {
                permissions.admit(azul_layout::managers::permission::Capability::Geolocation)
            })
        };
        if !events.is_empty() {
            crate::desktop::extra::geolocation::apply_diff_events(&events, &lw.app_services);
            changed = true;
        }
    }

    // Which of the run's services the scripted user stands in for: a scenario that drives the
    // keyring or the gate asks, so it never reaches the user's own keychain or sensor.
    lw.app_services.biometric.user.set_driven(
        crate::desktop::extra::keyring::is_stand_in_run(),
        azul_core::run_mode::test_hooks().no_biometric,
    );

    // Biometric: availability probe (OnceLock-cached native call), then
    // dispatch queued prompts (tracked in-flight so the timer stays armed
    // until the reply folds back), then fold parked outcomes. A completion
    // always counts as a change — even a repeat outcome answers a fresh
    // request and must dispatch a BiometricResult event.
    lw.biometric_manager.set_availability(crate::desktop::extra::biometric::availability_for(
        &lw.app_services.biometric.user,
    ));
    let bio_requests = lw.app_services.biometric.requests.drain();
    if !bio_requests.is_empty() {
        lw.biometric_manager
            .mark_requests_dispatched(bio_requests.len() as u32);
        changed = true;
    }
    for prompt in &bio_requests {
        crate::desktop::extra::biometric::request(
            prompt,
            &lw.app_services.biometric.results,
            &lw.app_services.biometric.user,
        );
    }
    for result in lw.app_services.biometric.results.drain() {
        lw.biometric_manager.set_last_result(result);
        changed = true;
    }

    // Keyring: same request → native backend → parked-result shape.
    let keyring_requests = lw.app_services.keyring.drain_requests_in();
    if !keyring_requests.is_empty() {
        lw.keyring_manager
            .mark_requests_dispatched(keyring_requests.len() as u32);
        changed = true;
    }
    for (namespace, req) in &keyring_requests {
        crate::desktop::extra::keyring::request(
            namespace,
            req,
            &lw.app_services.keyring.results,
            &lw.app_services.biometric.user,
        );
    }
    // Only the answers this window waits for: another window's pass leaves them for it.
    let awaited = usize::try_from(lw.keyring_manager.in_flight).unwrap_or(usize::MAX);
    for result in lw.app_services.keyring.drain_results_up_to(awaited) {
        lw.keyring_manager.set_last_result(result);
        changed = true;
    }

    // Native notifications: hand queued posts / withdrawals to the backend
    // (nothing is started while nothing is queued). Here as well as in the
    // run loops' notification pump because THIS runs on every target - on
    // mobile, where no run loop pumps notifications, it is what turns a post
    // into a logged `Failed` instead of silence. Events are NOT delivered
    // here: a delivery needs a window to invoke against, which the run loops
    // have and this pass does not.
    crate::desktop::notifications::dispatch_queued_requests(&lw.app_services);

    // Eyedropper: only the results addressed to THIS window's requests (the
    // loupe / system sampler answers by request id; another window's pick
    // stays in the channel for that window).
    let issued = lw.eyedropper_manager.issued().to_vec();
    if !issued.is_empty() {
        for result in lw.app_services.eyedropper.drain_results_for(&issued) {
            if lw.eyedropper_manager.fold_result(
                &lw.app_services.eyedropper,
                result.request_id,
                result.color,
            ) {
                changed = true;
            }
        }
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
    // A `SensorChanged` node, or an app that keeps reading a sensor (a read wants its kind for a lease) and the
    // backend groups that run until the pump has stopped them.
    if lw.sensor_manager.has_listeners() || crate::desktop::extra::sensors::needs_pump(&lw.app_services.sensors) {
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
        // MWA-C-biometric: requests QUEUED mid-pass (parked in the channel,
        // not yet dispatched → in_flight still 0) must arm the timer too,
        // or a prompt queued by a DoNothing callback would wait for an
        // unrelated event before being shown.
        || lw.app_services.biometric.has_queued_requests()
        || lw.app_services.keyring.has_queued_requests()
        // A notification posted by a DoNothing callback must not wait for
        // an unrelated event to reach the OS.
        || lw.app_services.notifications.has_queued_requests()
        || lw.eyedropper_manager.has_pending_async()
        // Resumable-API requests (file dialogs, device lists, ...) completed or still pending.
        || azul_layout::request::has_work()
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
    use azul_core::{
        refany::RefAny,
        task::{Duration as AzulDuration, SystemTimeDiff},
    };
    use azul_layout::{
        callbacks::ExternalSystemCallbacks,
        timer::{Timer, TimerCallbackType},
    };

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
///
/// The DELAY is what makes it fire after `delay_ms`: `Timer::invoke` admits
/// a timer that has never run at once unless it has a delay, and this one
/// used to carry the span as its interval only, so the first timer pass of
/// any kind ran it — on X11 and Wayland the pass on the loop turn right
/// after the press. The interval stays the same span so the platform timer
/// (`Timer::tick_millis`) wakes the loop once, at the deadline, instead of
/// every 10 ms until then.
#[must_use]
pub fn make_one_shot_pass_timer(delay_ms: u64) -> azul_layout::timer::Timer {
    use azul_core::{
        refany::RefAny,
        task::{Duration as AzulDuration, SystemTimeDiff},
    };
    use azul_layout::{
        callbacks::ExternalSystemCallbacks,
        timer::{Timer, TimerCallbackType},
    };

    let span = AzulDuration::System(SystemTimeDiff {
        secs: delay_ms / 1000,
        nanos: ((delay_ms % 1000) * 1_000_000) as u32,
    });
    let external = ExternalSystemCallbacks::rust_internal();
    Timer::create(
        RefAny::new(()),
        one_shot_pass_marker_callback as TimerCallbackType,
        external.get_system_time_fn,
    )
    .with_delay(span)
    .with_interval(span)
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

#[cfg(test)]
#[path = "capability_pump_test.rs"]
mod capability_pump_test;
