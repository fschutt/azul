//! The device itself: which capabilities the engine says it has here, the battery, the network.
//! None of them prompts; all of them run on every platform (a desktop answers honestly).

use azul::{
    sensor::{BatteryState, NetworkKind, NetworkState},
    window::PlatformCapability as Cap,
};

use super::{describe_capability, plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

pub static PROBES: &[ProbeDef] = &[
    plain("device.capabilities", "device", "Capability table", capabilities),
    plain("device.battery", "device", "Battery state", battery),
    plain("device.network", "device", "Network state", network),
];

/// The engine's own answer to 'what is usable here', for every capability with a probe. Always a PASS: the
/// rows below it hold each capability to what it says here.
fn capabilities(_ctx: &mut Ctx<'_>) -> Step {
    let rows = [
        ("camera", Cap::camera()),
        ("microphone", Cap::microphone()),
        ("audio-out", Cap::audio_output()),
        ("sensors", Cap::sensors()),
        ("gamepad", Cap::gamepad()),
        ("geolocation", Cap::geolocation()),
        ("keyring", Cap::keyring()),
        ("biometric", Cap::biometric()),
        ("video-codec", Cap::video_codec()),
        ("notifications", Cap::notifications()),
        ("scheduled-notifications", Cap::scheduled_notifications()),
        ("iroh", Cap::iroh()),
    ];
    let mut available = 0;
    let mut text = String::new();
    for (name, cap) in &rows {
        let (yes, backend) = describe_capability(cap);
        if yes {
            available += 1;
        }
        if !text.is_empty() {
            text.push_str("; ");
        }
        text.push_str(&format!("{name}={} via {backend}", if yes { "yes" } else { "no" }));
    }
    Step::Done(Verdict::Pass(format!("{available} of {} available: {text}", rows.len())))
}

fn battery(ctx: &mut Ctx<'_>) -> Step {
    let state = BatteryState::query();
    if !state.present {
        // The first query of a process answers before the monitor thread's first reading (a few milliseconds later): the emulator of
        // the first Android run, which has a battery, read "a desktop on mains power". A device is without one only after it had time.
        if ctx.elapsed_ms < super::SETTLE_MS {
            return Step::Wait;
        }
        return Step::Done(Verdict::Unavailable("no battery (a desktop on mains power)".into()));
    }
    Step::Done(Verdict::Pass(format!(
        "{}% charging={} low_power_mode={}",
        state.level_percent, state.charging, state.low_power_mode
    )))
}

fn network(_ctx: &mut Ctx<'_>) -> Step {
    let state = NetworkState::query();
    let kind = match state.kind {
        NetworkKind::Wired => "wired",
        NetworkKind::WiFi => "wifi",
        NetworkKind::Cellular => "cellular",
        NetworkKind::Other => "other",
        NetworkKind::Unknown => "unknown",
    };
    Step::Done(Verdict::Pass(format!(
        "connected={} kind={kind} metered={} constrained={}",
        state.connected, state.metered, state.constrained
    )))
}
