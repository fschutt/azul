//! The probes: one row per capability.
//!
//! A probe is a function the runner calls once per tick (100 ms) until it answers [`Step::Done`]. It first
//! asks whether the capability is available (`PlatformCapability`, the permission state: neither prompts),
//! then does a REAL operation and reports what came back. The rule: a device that is unavailable is NOT a
//! failure (`Verdict::Unavailable`), a capability that says available and then fails IS (`Verdict::Fail`).
//!
//! Adding a probe: write the function in the group's file, add its [`ProbeDef`] to that file's `PROBES`
//! (ids are `group.name` and stay stable: a harness asserts on them). A row that needs a person says exactly
//! what to do in `human`; unattended runs do not run it, they list the instruction. A probe that needs a
//! widget in the DOM while it runs (the geolocation probe node, a camera preview) names it in `widget`: the
//! layout adds it only while that row runs, so no system dialog appears before the user asks.

use azul::prelude::*;

use crate::report::{RowResult, Verdict};

pub mod device;
pub mod media;
pub mod pending;
pub mod platform;
pub mod render;
pub mod security;
pub mod sensors;
pub mod system;

/// Rows a harness can play the person for: it feeds the platform (a finger or face that matches, a location) while the
/// row waits. They run in an unattended run only when the harness says so with `AZ_SELFTEST_FED=1` (it promises to
/// answer every `AZ_SELFTEST_AWAIT` line); without it they are listed with their instruction like every other row that
/// needs a person.
pub const FEEDABLE: &[&str] = &["security.biometric_prompt"];

/// The harness's turn: prints the ONE line `AZ_SELFTEST_AWAIT <key>` the harness waits for before it feeds the
/// platform (key `biometric.match`: Android `adb emu finger touch 1`, iOS Simulator
/// `notifyutil -p com.apple.BiometricKit_Sim.pearl.match`). Call it once, right after the probe started what waits.
pub fn await_feed(key: &str) {
    use std::io::Write as _;
    println!("AZ_SELFTEST_AWAIT {key}");
    let _ = std::io::stdout().flush();
}

/// What a probe sees on each tick.
pub struct Ctx<'a> {
    /// The window's callback info: the capability readings (`get_sensor_reading`, `get_location_fix`, ...).
    pub info: &'a mut CallbackInfo,
    /// Milliseconds since the probe started.
    pub elapsed_ms: u64,
    /// A counter the probe owns (which step of its sequence it is at); kept between ticks.
    pub phase: u32,
    /// No person is part of this run: never start anything that shows a system dialog.
    pub unattended: bool,
    /// Set to true to have the row's `widget` in the DOM from the next layout on.
    pub want_widget: bool,
    /// Set to true to have the DOM refreshed on EVERY tick (an animation: frames come one after another).
    pub want_refresh: bool,
    /// Samples the probe keeps between ticks; empty when the row starts.
    pub scratch: &'a mut Vec<u64>,
    /// The rows that already have an answer (an aggregate probe reads them).
    pub done: &'a [RowResult],
    /// The row's own note (what a `Pending` row still has to prove).
    pub note: &'static str,
}

/// What a probe answers on a tick.
pub enum Step {
    /// Not yet: call me again at the next tick.
    Wait,
    /// The verdict.
    Done(Verdict),
}

/// One step of a probe.
pub type StepFn = fn(&mut Ctx<'_>) -> Step;

/// A widget a probe needs in the DOM while it runs; it gets the milliseconds since the probe started (an animation).
pub type WidgetFn = fn(&RefAny, u64) -> Dom;

/// One row.
pub struct ProbeDef {
    /// `group.name`, stable.
    pub id: &'static str,
    /// The group of the list.
    pub group: &'static str,
    /// What the list shows.
    pub title: &'static str,
    /// What a person has to do for the row, or `None` when it runs on its own.
    pub human: Option<&'static str>,
    /// For a `Pending` row: the API entry and what a probe has to prove.
    pub note: &'static str,
    /// The widget the probe needs while it runs.
    pub widget: Option<WidgetFn>,
    /// The probe.
    pub step: StepFn,
}

/// Every probe, in the order of the list.
#[must_use]
pub fn all() -> Vec<&'static ProbeDef> {
    [
        device::PROBES,
        sensors::PROBES,
        media::PROBES,
        security::PROBES,
        system::PROBES,
        platform::PROBES,
        render::PROBES,
        pending::PROBES,
    ]
    .iter()
    .flat_map(|group| group.iter())
    .collect()
}

/// A probe with no widget and no person: the usual row.
#[must_use]
pub const fn plain(id: &'static str, group: &'static str, title: &'static str, step: StepFn) -> ProbeDef {
    ProbeDef {
        id,
        group,
        title,
        human: None,
        note: "",
        widget: None,
        step,
    }
}

/// `PlatformCapability` as `(available, "backend (reason)")`.
#[must_use]
pub fn describe_capability(cap: &azul::window::PlatformCapability) -> (bool, String) {
    let text = if cap.reason.as_str().is_empty() {
        cap.backend.as_str().to_string()
    } else {
        format!("{} ({})", cap.backend.as_str(), cap.reason.as_str())
    };
    (cap.available, text)
}
