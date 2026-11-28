//! What the platform holds that an app reads without asking anybody: the clipboard, the permission table, the
//! settings the system hands us (dark mode, text size, language, safe area).
//!
//! None of these rows shows a system dialog. The permission row is the proof of the rule "a state query never asks":
//! it reads every capability's state in one tick, and a harness that launched a fresh install checks that no system
//! sheet appeared. The detail of every row is `key=value` pairs, so that a harness which flipped a setting (dark
//! mode, text size, a revoked permission) can read the answer back.

use azul::{
    dom::ClipboardContent,
    misc::{Capability, PermissionState},
    option::{OptionPixelValue, OptionString, OptionU8Vec},
    str::String as AzString,
    vec::{StringVec, StyledTextRunVec},
    window::Platform,
};

use super::{plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// How long the system clipboard may take to show what was written (the write happens when the tick returns).
const CLIPBOARD_WAIT_MS: u64 = 2_000;

pub static PROBES: &[ProbeDef] = &[
    plain("input.clipboard", "input", "Clipboard round trip", clipboard),
    plain("system.permission_state", "system", "Permission state without prompting", permission_state),
    plain("system.settings", "system", "Platform settings", settings),
];

/// Write a text, read it back from the system clipboard on the next tick: the text comes back as it went in.
fn clipboard(ctx: &mut Ctx<'_>) -> Step {
    const TEXT: &str = "az-selftest-clipboard-1";
    if ctx.phase == 0 {
        ctx.info.set_clipboard_content(ClipboardContent {
            plain_text: AzString::from(TEXT),
            styled_runs: StyledTextRunVec::create(),
            html: OptionString::None,
            files: StringVec::create(),
            image: OptionU8Vec::None,
            link: OptionString::None,
            concealed: false,
        });
        ctx.phase = 1;
        return Step::Wait;
    }
    match ctx.info.get_system_clipboard().into_option() {
        Some(content) if content.plain_text.as_str() == TEXT => {
            Step::Done(Verdict::Pass(format!("wrote and read back {} bytes after {} ms", TEXT.len(), ctx.elapsed_ms)))
        }
        Some(content) => Step::Done(Verdict::Fail(format!(
            "the system clipboard holds {:?} after the write of {TEXT:?}",
            content.plain_text.as_str()
        ))),
        None if ctx.elapsed_ms > CLIPBOARD_WAIT_MS => Step::Done(Verdict::Unavailable(format!(
            "the system clipboard read nothing {CLIPBOARD_WAIT_MS} ms after the write (a platform without a clipboard, or one that only \
             lets the focused app read it: Android 10 and later)"
        ))),
        None => Step::Wait,
    }
}

/// Every capability's state, read once. A state query must never ask the user: an unattended run starts from a state
/// nothing in this app has requested yet, so a `Requested` state in it means something asked on its own.
fn permission_state(ctx: &mut Ctx<'_>) -> Step {
    let capabilities = [
        ("camera", Capability::Camera),
        ("microphone", Capability::Microphone),
        ("geolocation", Capability::Geolocation),
        ("notifications", Capability::Notifications),
        ("motion", Capability::Motion),
        ("photos", Capability::PhotoLibrary),
        ("biometric", Capability::Biometric),
        ("local-network", Capability::LocalNetwork),
        ("bluetooth", Capability::Bluetooth),
        ("contacts", Capability::Contacts),
        ("calendars", Capability::Calendars),
        ("reminders", Capability::Reminders),
        ("tracking", Capability::AppTrackingTransparency),
        ("screen-capture", Capability::ScreenCapture),
    ];
    let mut text = String::new();
    let mut in_flight = Vec::new();
    for (name, capability) in capabilities {
        let state = ctx.info.get_permission_status(capability);
        if matches!(state, PermissionState::Requested) {
            in_flight.push(name);
        }
        if !text.is_empty() {
            text.push_str("; ");
        }
        text.push_str(&format!("{name}={state:?}"));
    }
    if ctx.unattended && !in_flight.is_empty() {
        return Step::Done(Verdict::Fail(format!(
            "an unattended run never asks, and {} is Requested (a dialog is up or was asked for on its own): {text}",
            in_flight.join(", ")
        )));
    }
    Step::Done(Verdict::Pass(text))
}

/// The settings the system hands the app: the platform it names must be the one this binary runs on, the text scale
/// must be a positive number, the language must be named. The numbers are in the detail for a harness that flipped
/// dark mode or the text size.
fn settings(ctx: &mut Ctx<'_>) -> Step {
    let style = ctx.info.get_system_style();
    let safe = ctx.info.get_safe_area_insets();
    let px = |v: OptionPixelValue| v.into_option().map_or(0.0, |p| p.to_pixels(0.0, 16.0, 16.0));
    let access = style.accessibility;
    let expected = if cfg!(target_os = "ios") {
        Some(Platform::Ios)
    } else if cfg!(target_os = "android") {
        Some(Platform::Android)
    } else {
        None
    };
    let detail = format!(
        "platform={:?}; os={:?}; mode={:?}; language={}; text_scale={:.2}; bold={}; reduce_motion={}; safe_top={:.0}; \
         safe_right={:.0}; safe_bottom={:.0}; safe_left={:.0}; keyboard={:.0}",
        style.platform,
        style.os_version,
        style.mode,
        style.language.id.as_str(),
        access.text_scale_factor,
        access.prefers_bold_text,
        access.prefers_reduced_motion,
        px(safe.top),
        px(safe.right),
        px(safe.bottom),
        px(safe.left),
        px(safe.keyboard),
    );
    if let Some(expected) = expected {
        if style.platform != expected {
            return Step::Done(Verdict::Fail(format!("the style names another platform than this one ({expected:?}): {detail}")));
        }
    }
    if !(access.text_scale_factor > 0.0) {
        return Step::Done(Verdict::Fail(format!("the text scale is not a positive number: {detail}")));
    }
    if style.language.id.as_str().is_empty() {
        return Step::Done(Verdict::Fail(format!("the system language is not named: {detail}")));
    }
    Step::Done(Verdict::Pass(detail))
}
