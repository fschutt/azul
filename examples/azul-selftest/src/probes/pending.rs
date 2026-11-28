//! Capabilities that have a row and no probe yet. Each note names the API entry and what the probe has to
//! prove, so that the row is a work item and not a hole: write the probe in the group's own file, move the
//! row there, and delete it from this list. Nothing in here is a pass.

use super::{Ctx, ProbeDef, Step};
use crate::report::Verdict;

pub static PROBES: &[ProbeDef] = &[
    pending(
        "media.camera",
        "media",
        "Camera frames",
        "CameraWidget (api.json widgets.CameraWidget, with_on_frame): frames arrive on the loop thread with a non-zero size; denied permission is DENIED; the widget is added only while the row runs so no dialog comes at launch; torch on a device that has one",
    ),
    pending(
        "media.microphone",
        "media",
        "Microphone frames",
        "MicrophoneWidget (with_on_frame): audio frames arrive with a non-zero RMS when the emulator feeds the host microphone, DENIED when the RECORD_AUDIO permission is refused",
    ),
    pending(
        "media.screen_capture",
        "media",
        "Screen capture",
        "ScreenCaptureWidget / ScreenRecorder: the MediaProjection consent dialog is NEEDS-HUMAN; frames after consent",
    ),
    pending(
        "input.gamepad",
        "input",
        "Gamepad",
        "CallbackInfo::get_primary_gamepad: NEEDS-HUMAN (press a button on a paired pad); UNAVAILABLE without one",
    ),
    pending(
        "input.soft_keyboard",
        "input",
        "Soft keyboard",
        "a tap on a TextInput raises the keyboard exactly once (the harness asks the window system), BACK hides it",
    ),
    pending(
        "system.file_picker",
        "system",
        "File picker",
        "FileDialog::open_file / save_file: NEEDS-HUMAN (choose a file); the result is readable (content:// URI)",
    ),
    pending(
        "system.auth_session",
        "system",
        "Web sign-in session",
        "AuthSession::start with a test redirect: NEEDS-HUMAN; the redirect arrives as AuthSessionResult",
    ),
    pending(
        "system.speech",
        "system",
        "Speech (platform recogniser)",
        "Speech::is_available, speak; the platform recogniser only (the built-in engine stays out); RECORD_AUDIO",
    ),
    pending(
        "system.webview",
        "system",
        "Web view",
        "a webview node loads a local page and reports its title (WebViewEvent)",
    ),
    pending(
        "network.iroh_lan",
        "network",
        "LAN discovery (iroh)",
        "IrohConfig::with_local_discovery is OFF by default; turned on it holds the multicast lock only while it is on; the Local Network dialog on iOS comes only at pairing",
    ),
    pending(
        "lifecycle.background",
        "lifecycle",
        "Pause, resume, low memory",
        "harness-driven: HOME key, then the launcher icon (same process), am send-trim-memory; the app's callbacks fire and the picture survives",
    ),
];

const fn pending(id: &'static str, group: &'static str, title: &'static str, note: &'static str) -> ProbeDef {
    ProbeDef {
        id,
        group,
        title,
        human: None,
        note,
        widget: None,
        step: pending_step,
    }
}

fn pending_step(ctx: &mut Ctx<'_>) -> Step {
    Step::Done(Verdict::Pending(ctx.note.to_string()))
}
