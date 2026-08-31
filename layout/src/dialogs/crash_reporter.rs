//! The crash-reporter dialog — what the user sees when a crashed process
//! respawned itself with `AZ_CRASH_DUMP=<dump.json>`: every crash-tier panic
//! without an OTLP endpoint (see `telemetry::install_panic_hook`), and a
//! dump still queued from a previous launch. A crash contact is only needed
//! for Send; without one the dialog points at the dump on disk.
//!
//! Shows the dump (message, location, scope, path-stripped backtrace), asks
//! for an optional description, and mails dump + message to the configured
//! contact — or, without a mail transport, points the user at the dump file
//! on disk. MANUAL by design: the send happens when the user presses the
//! button, never before.

// fn-item -> fn-pointer casts: required for the Into<Callback> generics;
// the annotated-temporary alternative buries the callback wiring.
#![allow(trivial_casts)]
use azul_core::{
    callbacks::Update,
    refany::RefAny,
    task::{ThreadId, ThreadReceiver},
};
use azul_css::AzString;

use super::{cpu_dialog_window, style};
use crate::callbacks::CallbackInfo;
use crate::telemetry::CrashDump;
use crate::thread::{
    Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg,
    WriteBackCallbackType,
};
use crate::widgets::button::{Button, ButtonOnClickCallbackType};
use crate::widgets::text_area::{TextArea, TextAreaOnTextInputCallbackType, TextAreaState};
use crate::widgets::text_input::{OnTextInputReturn, TextInputValid};
use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType};
use azul_core::dom::Dom;

/// Where the submission currently is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrashReportStatus {
    Editing,
    Sending,
    Done(String),
    Failed(String),
}

/// Shared dialog state.
#[derive(Debug, Clone)]
pub struct CrashReporterState {
    pub dump: CrashDump,
    pub message: String,
    pub status: CrashReportStatus,
    /// Whether a mail transport + contact exist (decides Send vs disk-hint).
    pub can_mail: bool,
}

/// The `WindowCreateOptions` for a loaded dump — `App::run` opens this
/// INSTEAD of the app when `AZ_CRASH_DUMP` is set (always CPU-rendered; the
/// crash being reported may well be a GPU crash).
#[must_use]
pub fn window(dump: CrashDump) -> crate::window_state::WindowCreateOptions {
    let can_mail = mail_available();
    let state = RefAny::new(CrashReporterState {
        dump,
        message: String::new(),
        status: CrashReportStatus::Editing,
        can_mail,
    });
    cpu_dialog_window(
        "Crash Report",
        (600.0, 560.0),
        dialog_layout as LayoutCallbackType,
        state,
    )
}

#[cfg(feature = "crash-mail")]
fn mail_available() -> bool {
    crate::telemetry::crash_mail::crash_contact().is_some()
}

#[cfg(not(feature = "crash-mail"))]
const fn mail_available() -> bool {
    false
}

// --- callbacks ------------------------------------------------------------

extern "C" fn on_message_input(
    mut state: RefAny,
    _info: CallbackInfo,
    text_state: TextAreaState,
) -> OnTextInputReturn {
    if let Some(mut s) = state.downcast_mut::<CrashReporterState>() {
        s.message = text_state.get_text();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_close(mut _state: RefAny, mut info: CallbackInfo) -> Update {
    info.close_window();
    Update::DoNothing
}

struct SendTask {
    dump_path: std::path::PathBuf,
    message: String,
}

struct NewStatus(CrashReportStatus);

extern "C" fn on_send(mut state: RefAny, mut info: CallbackInfo) -> Update {
    let task = {
        let Some(mut s) = state.downcast_mut::<CrashReporterState>() else {
            return Update::DoNothing;
        };
        if s.status == CrashReportStatus::Sending {
            return Update::DoNothing;
        }
        s.status = CrashReportStatus::Sending;
        SendTask {
            dump_path: s.dump.path.clone(),
            message: s.message.clone(),
        }
    };
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(task),
            state.clone(),
            send_worker as ThreadCallbackType,
        ),
    );
    Update::RefreshDomAllWindows
}

extern "C" fn send_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let Some(task) = init.downcast_ref::<SendTask>() else {
        return;
    };
    let dump_path = task.dump_path.clone();
    let message = task.message.clone();
    drop(task);

    let status = send_dump(&dump_path, &message);
    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        apply_status as WriteBackCallbackType,
        RefAny::new(NewStatus(status)),
    )));
}

#[cfg(feature = "crash-mail")]
fn send_dump(path: &std::path::Path, message: &str) -> CrashReportStatus {
    let Some(config) = crate::telemetry::crash_mail::crash_contact() else {
        return CrashReportStatus::Failed("no crash contact configured".to_owned());
    };
    match crate::telemetry::crash_mail::send_dump_file(config, path, message) {
        Ok(()) => CrashReportStatus::Done(format!("Crash report sent to {}.", config.to)),
        Err(e) => CrashReportStatus::Failed(e),
    }
}

#[cfg(not(feature = "crash-mail"))]
fn send_dump(path: &std::path::Path, _message: &str) -> CrashReportStatus {
    CrashReportStatus::Done(format!(
        "No mail transport in this build — the dump stays at {}.",
        path.display()
    ))
}

extern "C" fn apply_status(mut state: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(new_status) = msg.downcast_ref::<NewStatus>().map(|s| s.0.clone()) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = state.downcast_mut::<CrashReporterState>() {
        s.status = new_status;
    }
    Update::RefreshDomAllWindows
}

// --- the dialog DOM -------------------------------------------------------

extern "C" fn dialog_layout(_data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let Some(mut ctx) = info.get_ctx().into_option() else {
        return Dom::create_body();
    };
    let snapshot = match ctx.downcast_ref::<CrashReporterState>() {
        Some(s) => s.clone(),
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

    let mut children: Vec<Dom> = vec![
        Dom::create_h2_with_text("The application crashed"),
        Dom::create_p_with_text(format!("Error: {}", snapshot.dump.message)),
        Dom::create_p_with_text(format!(
            "Where: {}  (scope: {})",
            snapshot.dump.location,
            if snapshot.dump.scope.is_empty() {
                "-"
            } else {
                &snapshot.dump.scope
            }
        )),
    ];
    if !snapshot.dump.backtrace.is_empty() {
        children.push(Dom::create_pre_with_text(snapshot.dump.backtrace.as_str()));
    }
    // The action journal captured in-process at panic time — the same
    // "recent actions" the problem-report dialog offers, here already part
    // of the dump. Handler names and node ids only, never typed text.
    if !snapshot.dump.recent_actions.is_empty() && snapshot.dump.recent_actions != "[]" {
        children.push(Dom::create_p_with_text("Recent actions before the crash:"));
        children.push(Dom::create_pre_with_text(
            snapshot.dump.recent_actions.as_str(),
        ));
    }

    match &snapshot.status {
        CrashReportStatus::Editing | CrashReportStatus::Failed(_) => {
            if let CrashReportStatus::Failed(e) = &snapshot.status {
                children.push(Dom::create_p_with_text(format!("Sending failed: {e}")));
            }
            if snapshot.can_mail {
                children.push(Dom::create_p_with_text(
                    "You can send this report to the developers. Nothing is sent until you press Send.",
                ));
                children.push(
                    TextArea::create()
                        .with_text(AzString::from(snapshot.message.as_str()))
                        .with_placeholder(AzString::from("What were you doing? (optional)"))
                        .with_on_text_input(
                            state.clone(),
                            on_message_input as TextAreaOnTextInputCallbackType,
                        )
                        .dom(),
                );
                children.push(button_row(vec![
                    ("Send report", on_send, state.clone()),
                    ("Don't send", on_close, state),
                ]));
            } else {
                children.push(Dom::create_p_with_text(format!(
                    "The crash dump was saved to {} — please attach it to a bug report.",
                    snapshot.dump.path.display()
                )));
                children.push(button_row(vec![("Close", on_close, state)]));
            }
        }
        CrashReportStatus::Sending => {
            children.push(Dom::create_p_with_text("Sending the crash report…"));
        }
        CrashReportStatus::Done(msg) => {
            children.push(Dom::create_p_with_text(msg.as_str()));
            children.push(button_row(vec![("Close", on_close, state)]));
        }
    }

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css_props(pad)
            .with_children(children.into()),
    )
}

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
