//! The graphics-check dialog (`SysDialogType::GpuCheck`).
//!
//! For apps that NEED working video acceleration. It is a thin, honest face
//! on machinery that already exists: the shell's GL probe (`query_gpu_info`,
//! published through [`crate::appenv::gpu_status`]) says what the RENDERER
//! got, and the dll's provisioning check
//! (`VideoStartupCheck::run`/`remediate`, reached through
//! [`crate::appenv::gpu_provision_hooks`]) says whether hardware decode is
//! ready, whether the machine is SAFE TO REBOOT, and what a one-click repair
//! would run.
//!
//! Consent is the whole design: `run()` is inspection only and happens on a
//! worker thread when the dialog opens; `remediate()` — which elevates via
//! pkexec and can install drivers or repair an unbootable kernel — runs ONLY
//! after the user has read the exact command list and pressed the button.
//! When the remediation needs a reboot, or when the CURRENT boot path is
//! already unsafe, the dialog says so before anything is applied.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType, Update};
use azul_core::dom::Dom;
use azul_core::refany::RefAny;
use azul_core::task::ThreadId;
use azul_css::AzString;

use super::{cpu_dialog_window, style};
use crate::appenv::{GpuProvisionOutcome, GpuProvisionReport, GpuStatus};
use crate::callbacks::CallbackInfo;
use crate::thread::{
    Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg,
    WriteBackCallbackType,
};
use crate::widgets::button::{Button, ButtonOnClickCallbackType};
use crate::widgets::progressbar::ProgressBar;
use azul_core::task::ThreadReceiver;

/// Where the dialog is in the inspect → consent → apply story.
#[derive(Debug, Clone)]
pub enum GpuPhase {
    /// The readiness check is running on a worker thread.
    Checking,
    /// The check came back; waiting for the user.
    Report(GpuProvisionReport),
    /// No shell published provisioning hooks (a build without the video
    /// stack, or a headless run) — the GL section is still shown.
    Unavailable(String),
    /// The user consented; the remediation is running (pkexec prompt).
    /// Carries REAL progress: commands finished / total, and the command
    /// currently running.
    Applying {
        /// Commands finished so far.
        done: usize,
        /// Commands the plan set out to run.
        total: usize,
        /// The command running right now.
        step: String,
    },
    /// The remediation finished.
    Applied(GpuProvisionOutcome),
}

/// Shared dialog state (window ctx + worker writebacks).
#[derive(Debug, Clone)]
pub struct GpuDialogState {
    /// Current phase.
    pub phase: GpuPhase,
}

/// Opens the dialog and starts the (inspection-only) readiness check.
pub fn open(info: &mut CallbackInfo) {
    let state = RefAny::new(GpuDialogState {
        phase: GpuPhase::Checking,
    });
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(CheckTask),
            state.clone(),
            check_worker as ThreadCallbackType,
        ),
    );
    info.create_window(cpu_dialog_window(
        "Graphics Check",
        (560.0, 560.0),
        dialog_layout as LayoutCallbackType,
        state,
    ));
}

struct CheckTask;
struct NewPhase(GpuPhase);

/// Background: the provisioning readiness check. INSPECTION ONLY — it
/// dlopen's codec libraries and reads kernel/driver state, changes nothing.
extern "C" fn check_worker(mut _init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let phase = match crate::appenv::gpu_provision_hooks() {
        Some(hooks) => GpuPhase::Report((hooks.check)()),
        None => GpuPhase::Unavailable(
            "This build has no driver-provisioning support, so only the \
             renderer's own report is available."
                .to_owned(),
        ),
    };
    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        apply_phase as WriteBackCallbackType,
        RefAny::new(NewPhase(phase)),
    )));
}

/// Background: the CONSENTED remediation. Side-effecting — driver install
/// and/or kernel repair through pkexec.
extern "C" fn remediate_worker(mut _init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let phase = match crate::appenv::gpu_provision_hooks() {
        Some(hooks) => {
            // One writeback per command, so the bar moves for real. (The
            // main-thread drain reads the queue until it is EMPTY — see the
            // `every_writeback_a_worker_sends_reaches_the_main_thread` law;
            // before that fix everything after the first step was dropped.)
            let mut on_step = |done: usize, total: usize, step: &str| {
                let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                    apply_phase as WriteBackCallbackType,
                    RefAny::new(NewPhase(GpuPhase::Applying {
                        done,
                        total,
                        step: step.to_owned(),
                    })),
                )));
            };
            GpuPhase::Applied((hooks.remediate)(&mut on_step))
        }
        None => GpuPhase::Applied(GpuProvisionOutcome {
            ok: false,
            reboot_required: false,
            message: "provisioning hooks disappeared between check and apply".to_owned(),
        }),
    };
    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        apply_phase as WriteBackCallbackType,
        RefAny::new(NewPhase(phase)),
    )));
}

/// Main thread: move the shared state to the worker's phase.
extern "C" fn apply_phase(mut state: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(new_phase) = msg.downcast_ref::<NewPhase>().map(|p| p.0.clone()) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = state.downcast_mut::<GpuDialogState>() {
        s.phase = new_phase;
    }
    Update::RefreshDomAllWindows
}

// --- buttons ---------------------------------------------------------------

/// The consent button. Everything before this point was inspection.
extern "C" fn on_repair_now(mut state: RefAny, mut info: CallbackInfo) -> Update {
    {
        let Some(mut s) = state.downcast_mut::<GpuDialogState>() else {
            return Update::DoNothing;
        };
        let GpuPhase::Report(report) = &s.phase else {
            return Update::DoNothing;
        };
        if !report.can_remediate {
            return Update::DoNothing;
        }
        s.phase = GpuPhase::Applying {
            done: 0,
            total: 0,
            step: String::new(),
        };
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(CheckTask),
            state.clone(),
            remediate_worker as ThreadCallbackType,
        ),
    );
    Update::RefreshDomAllWindows
}

extern "C" fn on_close(mut _state: RefAny, mut info: CallbackInfo) -> Update {
    info.close_window();
    Update::DoNothing
}

// --- layout ----------------------------------------------------------------

extern "C" fn dialog_layout(_data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let Some(mut ctx) = info.get_ctx().into_option() else {
        return Dom::create_body();
    };
    let phase = match ctx.downcast_ref::<GpuDialogState>() {
        Some(s) => s.phase.clone(),
        None => return Dom::create_body(),
    };
    drop(ctx);
    let state = info
        .get_ctx()
        .into_option()
        .unwrap_or_else(|| RefAny::new(()));

    use azul_css::props::{
        basic::pixel::PixelValue,
        layout::{LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop},
    };
    let pad = style(vec![
        azul_css::props::property::CssProperty::padding_left(LayoutPaddingLeft {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_right(LayoutPaddingRight {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_top(LayoutPaddingTop {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_bottom(LayoutPaddingBottom {
            inner: PixelValue::px(16.0),
        }),
    ]);

    let mut children: Vec<Dom> = vec![Dom::create_h2_with_text("Graphics check")];
    let mut buttons: Vec<(&str, ButtonOnClickCallbackType, RefAny)> = Vec::new();

    match &phase {
        GpuPhase::Checking => {
            children.push(Dom::create_p_with_text(
                "Checking the graphics drivers and the boot path...",
            ));
        }
        GpuPhase::Report(report) => {
            children.push(Dom::create_p_with_text(report.summary.as_str()));
            children.push(Dom::create_p_with_text(alloc::format!(
                "Hardware video decode: {}",
                yes_no(report.hw_decode_ready)
            )));
            children.push(Dom::create_p_with_text(alloc::format!(
                "Safe to reboot: {}",
                yes_no(report.boot_safe)
            )));
            if !report.boot_safe {
                children.push(Dom::create_p_with_text(
                    "WARNING: as things stand, the next reboot may not reach a \
                     usable desktop. Apply the repair below BEFORE rebooting.",
                ));
            }
            children.extend(detail_lines(&report.detail));
            if report.can_remediate {
                children.push(Dom::create_p_with_text(
                    "The repair runs the commands listed above and will ask for \
                     your password.",
                ));
                if report.needs_reboot {
                    children.push(Dom::create_p_with_text(
                        "It takes effect after a restart - nothing reboots on its own.",
                    ));
                }
                buttons.push((
                    "Repair now",
                    on_repair_now as ButtonOnClickCallbackType,
                    state.clone(),
                ));
            }
        }
        GpuPhase::Unavailable(reason) => {
            children.push(Dom::create_p_with_text(reason.as_str()));
        }
        GpuPhase::Applying { done, total, step } => {
            children.push(Dom::create_p_with_text(
                "Applying... you may be asked for your password. Do not close \
                 this window.",
            ));
            // A real fraction or nothing: an indeterminate bar drawn as if it
            // measured something is worse than no bar. `total == 0` is the
            // window between consent and the first command report.
            if *total > 0 {
                #[allow(clippy::cast_precision_loss)]
                let percent = (*done as f32 / *total as f32) * 100.0;
                children.push(ProgressBar::create(percent).dom());
                children.push(Dom::create_p_with_text(alloc::format!(
                    "Step {} of {}: {}",
                    done + 1,
                    total,
                    step
                )));
            } else {
                children.push(Dom::create_p_with_text("Starting..."));
            }
        }
        GpuPhase::Applied(outcome) => {
            children.push(Dom::create_p_with_text(if outcome.ok {
                "The repair finished."
            } else {
                "The repair did not finish."
            }));
            children.push(Dom::create_p_with_text(outcome.message.as_str()));
            if outcome.reboot_required {
                children.push(Dom::create_p_with_text(
                    "Restart the machine to finish - the change is staged, not live.",
                ));
            }
        }
    }

    children.extend(gl_section(crate::appenv::gpu_status().as_ref()));
    buttons.push(("Close", on_close as ButtonOnClickCallbackType, state));
    children.push(button_row(buttons));

    Dom::create_body()
        .with_css_props(pad)
        .with_children(children.into())
}

/// What the RENDERER actually got, as opposed to what the machine could do.
/// Always shown: it is the answer to "why is this app slow/soft-rendered".
fn gl_section(status: Option<&GpuStatus>) -> Vec<Dom> {
    let mut out = vec![Dom::create_h2_with_text("This window's renderer")];
    match status {
        Some(s) if s.ok => {
            out.push(Dom::create_p_with_text(alloc::format!(
                "GPU rendering, on {} ({})",
                s.renderer,
                s.vendor
            )));
            out.push(Dom::create_p_with_text(alloc::format!(
                "OpenGL {} - GLSL {}",
                s.version,
                s.glsl_version
            )));
        }
        Some(s) => {
            out.push(Dom::create_p_with_text(
                "CPU rendering - the GPU path was rejected.",
            ));
            if !s.renderer.is_empty() {
                out.push(Dom::create_p_with_text(alloc::format!(
                    "Detected: {} ({}) - OpenGL {}",
                    s.renderer,
                    s.vendor,
                    s.version
                )));
            }
            out.push(Dom::create_p_with_text(alloc::format!(
                "Why: {}", s.verdict
            )));
        }
        None => out.push(Dom::create_p_with_text(
            "CPU rendering - no GPU probe ran in this session.",
        )),
    }
    out
}

/// The provisioning report's multi-line detail, one paragraph per line, so
/// the exact command list the user is consenting to stays readable.
fn detail_lines(detail: &str) -> Vec<Dom> {
    detail
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(Dom::create_p_with_text)
        .collect()
}

const fn yes_no(v: bool) -> &'static str {
    if v {
        "yes"
    } else {
        "no"
    }
}

/// Buttons on one line (same shape as the other dialogs' rows).
fn button_row(buttons: Vec<(&str, ButtonOnClickCallbackType, RefAny)>) -> Dom {
    let children: Vec<Dom> = buttons
        .into_iter()
        .map(|(label, cb, state)| {
            Button::create(AzString::from(label))
                .with_on_click(state, cb)
                .dom()
        })
        .collect();
    Dom::create_div().with_children(children.into())
}
