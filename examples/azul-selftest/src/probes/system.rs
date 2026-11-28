//! What the app asks of the system: a haptic, a notification, audio out, the video codecs.
//!
//! A notification needs a permission on Android 13 and later: unattended runs never ask (they list what to
//! grant); a person who runs the row by hand answers the dialog, once.

use azul::{
    audio::{AudioConfig, AudioSink},
    gesture::{HapticPattern, HapticTarget},
    misc::{Capability, PermissionState},
    notification::Notification,
    window::PlatformCapability,
};

use super::{describe_capability, plain, Ctx, ProbeDef, Step};
use crate::report::Verdict;

/// The video codec row: an H.264 round trip with a moving box, the picture checked (its own file).
mod video_codec_probe;

/// How long the notification permission dialog may stay unanswered.
const DIALOG_WAIT_MS: u64 = 60_000;

pub static PROBES: &[ProbeDef] = &[
    ProbeDef {
        id: "system.haptics",
        group: "system",
        title: "Haptic (system actuator)",
        human: Some("feel for the 'success' pattern: did the phone buzz?"),
        note: "",
        widget: None,
        step: haptics,
    },
    plain("system.notifications", "system", "Local notification", notifications),
    plain("system.audio_out", "system", "Audio output (open a sink)", audio_out),
    plain(
        "system.video_codec",
        "system",
        "Video codecs (encode a moving box, decode it back, check the picture)",
        video_codec_probe::video_codec,
    ),
];

fn haptics(ctx: &mut Ctx<'_>) -> Step {
    ctx.info.play_haptic(HapticPattern::Success, HapticTarget::System);
    Step::Done(Verdict::NeedsHuman("a 'success' haptic was dispatched: did the phone buzz?".into()))
}

fn notifications(ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::notifications());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the notifications capability is not available: {backend}")));
    }
    match ctx.info.get_permission_status(Capability::Notifications) {
        PermissionState::Granted(_) | PermissionState::EphemeralGranted(_) => {}
        PermissionState::Denied | PermissionState::Restricted => {
            return Step::Done(Verdict::Denied("the notification permission is refused: allow it in the settings".into()));
        }
        _ if ctx.unattended => {
            return Step::Done(Verdict::NeedsHuman(
                "the notification permission was never granted: grant it (adb shell pm grant <package> android.permission.POST_NOTIFICATIONS) or run this row by hand"
                    .into(),
            ));
        }
        _ => {
            if ctx.phase == 0 {
                ctx.info.request_notification_permission();
                ctx.phase = 1;
            }
            return if ctx.elapsed_ms > DIALOG_WAIT_MS {
                Step::Done(Verdict::NeedsHuman("nobody answered the notification permission dialog".into()))
            } else {
                Step::Wait
            };
        }
    }
    ctx.info.post_notification(Notification::create("azselftest.probe", "AzSelfTest: the notification path works"));
    Step::Done(Verdict::Pass(format!(
        "posted azselftest.probe via {backend}; the system lists it (adb shell dumpsys notification)"
    )))
}

fn audio_out(_ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::audio_output());
    let sink = AudioSink::open(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
    });
    Step::Done(match (sink.is_open(), available) {
        (true, _) => Verdict::Pass(format!("a 48 kHz mono sink opened via {backend}")),
        (false, false) => Verdict::Unavailable(format!("no audio output: {backend}")),
        (false, true) => Verdict::Fail(format!("the audio capability says available via {backend} and the sink did not open")),
    })
}
