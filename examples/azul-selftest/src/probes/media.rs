//! The camera and the microphone: the capture widgets, in the DOM only while the row runs.
//!
//! A row adds its widget when it starts (`ctx.want_widget`), so the system's permission dialog comes when a person runs the
//! row and never at launch; an unattended run does not run it at all unless the permission is granted already (the dialog
//! would need a person). The widget's frame callback hands every frame to the app's state (`SelfTest::note_media_frame`), the
//! row counts them: PASS after [`FRAMES_NEEDED`] frames of a non-zero size, DENIED when the permission is refused, FAIL when the
//! permission is granted and no frame comes (the capability said available).
//!
//! On the emulator the camera is the virtual scene (front and back) and the microphone is the host's (silence without one: the
//! row still passes, it proves frames arrive). On a phone, point the camera at something and speak.

use azul::{
    audio::{AudioConfig, AudioFrame},
    camera::CameraConfig,
    dom::Dom,
    misc::{Capability, PermissionState},
    prelude::{CallbackInfo, RefAny, Update},
    video::VideoFrame,
    widgets::{CameraWidget, MicrophoneWidget},
    window::PlatformCapability,
};

use super::{describe_capability, Ctx, ProbeDef, Step, SETTLE_MS};
use crate::{report::Verdict, SelfTest};

/// Frames a row needs before it passes.
const FRAMES_NEEDED: u64 = 5;
/// How long a granted capture gets to deliver them (the first frame of a camera takes a second or two to start).
const FRAME_WAIT_MS: u64 = 12_000;

pub static PROBES: &[ProbeDef] = &[
    ProbeDef {
        id: "media.camera",
        group: "media",
        title: "Camera frames",
        human: None,
        note: "",
        widget: Some(camera_widget),
        step: camera,
    },
    ProbeDef {
        id: "media.microphone",
        group: "media",
        title: "Microphone frames",
        human: None,
        note: "",
        widget: Some(microphone_widget),
        step: microphone,
    },
];

fn camera_widget(data: &RefAny, _elapsed_ms: u64) -> Dom {
    CameraWidget::create(CameraConfig {
        width: 320,
        height: 240,
        fps: 15,
        ..CameraConfig::default()
    })
    .with_on_frame(data.clone(), on_camera_frame)
    .dom()
    .with_css("width: 160px; height: 120px; margin: 6px 12px;")
}

fn microphone_widget(data: &RefAny, _elapsed_ms: u64) -> Dom {
    MicrophoneWidget::create(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
    })
    .with_on_frame(data.clone(), on_microphone_frame)
    .dom()
    .with_css("width: 1px; height: 1px; overflow: hidden;")
}

extern "C" fn on_camera_frame(mut data: RefAny, _info: CallbackInfo, frame: VideoFrame) -> Update {
    if let Some(mut app) = data.downcast_mut::<SelfTest>() {
        app.note_media_frame(frame.width, frame.height, 0);
    }
    Update::DoNothing
}

extern "C" fn on_microphone_frame(mut data: RefAny, _info: CallbackInfo, frame: AudioFrame) -> Update {
    let samples: &[f32] = frame.samples.as_ref();
    let loudest = samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
    let loudest_micro = (f64::from(loudest) * 1_000_000.0) as u64;
    if let Some(mut app) = data.downcast_mut::<SelfTest>() {
        app.note_media_frame(frame.sample_rate, u32::from(frame.channels), loudest_micro);
    }
    Update::DoNothing
}

/// What a capture row does first, from the permission state.
enum Gate {
    /// Mount the widget and count frames.
    Go,
    /// An unattended run waits for the engine to read the system's permission state (about once a second): nothing is mounted, so
    /// nothing can ask.
    Settling,
    /// The row ends without the widget.
    End(Verdict),
}

fn gate(ctx: &mut Ctx<'_>, capability: Capability, what: &str, backend: &str) -> Gate {
    match ctx.info.get_permission_status(capability) {
        PermissionState::Denied | PermissionState::Restricted => Gate::End(Verdict::Denied(format!(
            "the {what} permission is refused: switch it on in the settings of the app (azul-settings:app opens them)"
        ))),
        PermissionState::NotDetermined if ctx.unattended => {
            if ctx.elapsed_ms < SETTLE_MS {
                Gate::Settling
            } else {
                Gate::End(Verdict::NeedsHuman(format!(
                    "the {what} permission was never granted: grant it (adb shell pm grant <package> <the {what} permission>) or run this row by hand and answer the dialog ({backend})"
                )))
            }
        }
        _ => Gate::Go,
    }
}

fn camera(ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::camera());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the camera capability is not available: {backend}")));
    }
    capture(ctx, Capability::Camera, "camera", &backend)
}

fn microphone(ctx: &mut Ctx<'_>) -> Step {
    let (available, backend) = describe_capability(&PlatformCapability::microphone());
    if !available {
        return Step::Done(Verdict::Unavailable(format!("the microphone capability is not available: {backend}")));
    }
    capture(ctx, Capability::Microphone, "microphone", &backend)
}

fn capture(ctx: &mut Ctx<'_>, capability: Capability, what: &str, backend: &str) -> Step {
    match gate(ctx, capability, what, backend) {
        Gate::Settling => return Step::Wait,
        Gate::End(verdict) => return Step::Done(verdict),
        Gate::Go => {}
    }
    ctx.want_widget = true;
    let scratch = |at: usize| ctx.scratch.get(at).copied().unwrap_or(0);
    let (frames, first, second, loudest_micro) = (scratch(0), scratch(1), scratch(2), scratch(3));
    if frames >= FRAMES_NEEDED {
        let detail = if capability == Capability::Camera {
            format!("{frames} frames of {first}x{second} via {backend} after {} ms", ctx.elapsed_ms)
        } else {
            format!(
                "{frames} chunks of audio at {first} Hz x {second} channel(s), loudest sample {:.4} via {backend} after {} ms",
                loudest_micro as f64 / 1_000_000.0,
                ctx.elapsed_ms
            )
        };
        return Step::Done(Verdict::Pass(detail));
    }
    if ctx.elapsed_ms < FRAME_WAIT_MS {
        return Step::Wait;
    }
    Step::Done(match ctx.info.get_permission_status(capability) {
        PermissionState::Granted(_) | PermissionState::EphemeralGranted(_) => Verdict::Fail(format!(
            "the {what} capability says available via {backend}, the permission is granted and {frames} of {FRAMES_NEEDED} frames came in {FRAME_WAIT_MS} ms"
        )),
        _ => Verdict::NeedsHuman(format!("answer the {what} permission dialog, then run the row again")),
    })
}
