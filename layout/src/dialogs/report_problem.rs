//! The `SysDialogType::ReportProblem` dialog: user message + optional
//! screenshot of the CURRENT window + optional system information, delivered
//! to the app's support mailbox (`AppConfig.report_problem`) — or saved to
//! disk when no mailbox / no mail transport is available. Nothing leaves the
//! machine before the user presses **Send**.
//!
//! The screenshot is captured IN-PROCESS at invoke time
//! (`CallbackInfo::take_screenshot` re-renders the current display list on
//! the CPU), so it shows the user's real situation — no state
//! serialization round-trip needed for the common case.

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
use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType};
use crate::thread::{
    Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg,
    WriteBackCallbackType,
};
use crate::widgets::button::{Button, ButtonOnClickCallbackType};
use crate::widgets::check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState};
use crate::widgets::text_area::{TextArea, TextAreaOnTextInputCallbackType, TextAreaState};
use crate::widgets::text_input::{OnTextInputReturn, TextInputValid};
use azul_core::dom::Dom;

/// Where the report currently is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportStatus {
    /// The user is typing.
    Editing,
    /// The send worker is running.
    Sending,
    /// Delivered (or saved) — the message names where it went.
    Done(String),
    /// Transport failed; the report stays editable for a retry.
    Failed(String),
}

/// Shared dialog state.
#[derive(Debug, Clone)]
pub struct ReportProblemState {
    /// Destination mailbox (from `AppConfig.report_problem`); None = save to
    /// disk instead of mailing.
    pub email: Option<String>,
    /// The user's description.
    pub message: String,
    /// Attach the best-effort hardware/OS block.
    pub include_sysinfo: bool,
    /// Attach the screenshot below.
    pub attach_screenshot: bool,
    /// PNG of the window the dialog was invoked from, captured at invoke
    /// time (None when the capture failed).
    pub screenshot_png: Option<Vec<u8>>,
    pub status: ReportStatus,
}

/// Opens the dialog. `screenshot_png` is the already-captured window
/// screenshot (the capture happens in `invoke_system_dialog`, BEFORE this
/// window exists, so the dialog itself is never in the picture).
pub fn open(info: &mut CallbackInfo, screenshot_png: Option<Vec<u8>>) {
    let env = crate::appenv::app_env();
    let state = RefAny::new(ReportProblemState {
        email: env.report_problem,
        message: String::new(),
        include_sysinfo: true,
        attach_screenshot: screenshot_png.is_some(),
        screenshot_png,
        status: ReportStatus::Editing,
    });
    info.create_window(cpu_dialog_window(
        "Report a Problem",
        (520.0, 480.0),
        dialog_layout as LayoutCallbackType,
        state,
    ));
}

// --- widget callbacks -----------------------------------------------------

extern "C" fn on_message_input(
    mut state: RefAny,
    _info: CallbackInfo,
    text_state: TextAreaState,
) -> OnTextInputReturn {
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.message = text_state.get_text();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_toggle_sysinfo(
    mut state: RefAny,
    _info: CallbackInfo,
    cb: CheckBoxState,
) -> Update {
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.include_sysinfo = cb.checked;
    }
    Update::DoNothing
}

extern "C" fn on_toggle_screenshot(
    mut state: RefAny,
    _info: CallbackInfo,
    cb: CheckBoxState,
) -> Update {
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.attach_screenshot = cb.checked;
    }
    Update::DoNothing
}

extern "C" fn on_cancel(mut _state: RefAny, mut info: CallbackInfo) -> Update {
    info.close_window();
    Update::DoNothing
}

extern "C" fn on_send(mut state: RefAny, mut info: CallbackInfo) -> Update {
    let task = {
        let Some(mut s) = state.downcast_mut::<ReportProblemState>() else {
            return Update::DoNothing;
        };
        if s.status == ReportStatus::Sending {
            return Update::DoNothing;
        }
        s.status = ReportStatus::Sending;
        SendTask {
            email: s.email.clone(),
            report: build_report_text(&s.message, s.include_sysinfo),
            screenshot: if s.attach_screenshot {
                s.screenshot_png.clone()
            } else {
                None
            },
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

/// The report body: user message + app identity + (optional) system block.
/// Plain text by design — it is read by a HUMAN at the support mailbox.
fn build_report_text(message: &str, include_sysinfo: bool) -> String {
    use std::fmt::Write as _;
    let env = crate::appenv::app_env();
    let mut out = String::new();
    let _ = writeln!(out, "Problem report — {} {}", env.app_name, env.current_version);
    let _ = writeln!(out, "----------------------------------------");
    if message.trim().is_empty() {
        let _ = writeln!(out, "(no user message)");
    } else {
        let _ = writeln!(out, "{}", message.trim());
    }
    if include_sysinfo {
        let _ = writeln!(out, "\nSystem information:");
        #[cfg(feature = "telemetry")]
        for (k, v) in crate::telemetry::sysinfo::get().as_attributes() {
            let _ = writeln!(out, "  {k} = {v}");
        }
        #[cfg(not(feature = "telemetry"))]
        {
            let _ = writeln!(out, "  os = {}", std::env::consts::OS);
            let _ = writeln!(out, "  arch = {}", std::env::consts::ARCH);
        }
    }
    out
}

struct SendTask {
    email: Option<String>,
    report: String,
    screenshot: Option<Vec<u8>>,
}

struct NewStatus(ReportStatus);

/// Background transport ladder: mail (crash-mail feature + mailbox set) →
/// save to disk. Either way the user gets told exactly where it went.
extern "C" fn send_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let Some(task) = init.downcast_ref::<SendTask>() else {
        return;
    };
    let email = task.email.clone();
    let report = task.report.clone();
    let screenshot = task.screenshot.clone();
    drop(task);

    let mut attachments: Vec<(String, Vec<u8>)> =
        vec![("report.txt".to_owned(), report.clone().into_bytes())];
    if let Some(png) = screenshot {
        attachments.push(("screenshot.png".to_owned(), png));
    }

    let status = deliver(email.as_deref(), &report, &attachments);

    let _ = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
        apply_status as WriteBackCallbackType,
        RefAny::new(NewStatus(status)),
    )));
}

#[cfg(feature = "crash-mail")]
fn deliver(email: Option<&str>, _report: &str, attachments: &[(String, Vec<u8>)]) -> ReportStatus {
    match email {
        Some(to) => {
            let domain = to.split('@').nth(1).unwrap_or("localhost").to_owned();
            let config = crate::telemetry::crash_mail::CrashMailConfig::new(
                to.to_owned(),
                format!("problem-reporter@{domain}"),
                domain,
            );
            match crate::telemetry::crash_mail::send_attachments(
                &config,
                "Problem report attached (report.txt).",
                attachments,
            ) {
                Ok(()) => ReportStatus::Done(format!("Report sent to {to}.")),
                Err(e) => ReportStatus::Failed(format!("Sending failed: {e}")),
            }
        }
        None => save_to_disk(attachments),
    }
}

#[cfg(not(feature = "crash-mail"))]
fn deliver(_email: Option<&str>, _report: &str, attachments: &[(String, Vec<u8>)]) -> ReportStatus {
    // Built without the mail transport: saving to disk is the honest path.
    save_to_disk(attachments)
}

/// Disk fallback: `{data_dir}/{app}/problem-reports/report-<unix>[.-]*`.
fn save_to_disk(attachments: &[(String, Vec<u8>)]) -> ReportStatus {
    let env = crate::appenv::app_env();
    let dir = report_dir(&env.app_name);
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return ReportStatus::Failed(format!("cannot create {}: {e}", dir.display()));
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    for (name, bytes) in attachments {
        let path = dir.join(format!("report-{stamp}-{name}"));
        if let Err(e) = std::fs::write(&path, bytes) {
            return ReportStatus::Failed(format!("cannot write {}: {e}", path.display()));
        }
    }
    ReportStatus::Done(format!("Report saved to {}.", dir.display()))
}

/// Where disk-fallback reports go (public so apps can point users at it).
#[must_use]
pub fn report_dir(app_name: &str) -> std::path::PathBuf {
    #[cfg(feature = "updater")]
    {
        crate::updater::default_state_dir(app_name).join("problem-reports")
    }
    #[cfg(not(feature = "updater"))]
    {
        std::env::temp_dir().join(app_name).join("problem-reports")
    }
}

extern "C" fn apply_status(mut state: RefAny, mut msg: RefAny, _info: CallbackInfo) -> Update {
    let Some(new_status) = msg.downcast_ref::<NewStatus>().map(|s| s.0.clone()) else {
        return Update::DoNothing;
    };
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.status = new_status;
    }
    Update::RefreshDomAllWindows
}

// --- the dialog DOM -------------------------------------------------------

extern "C" fn dialog_layout(_data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let Some(mut ctx) = info.get_ctx().into_option() else {
        return Dom::create_body();
    };
    let snapshot = match ctx.downcast_ref::<ReportProblemState>() {
        Some(s) => s.clone(),
        None => return Dom::create_body(),
    };
    drop(ctx);
    let state = info.get_ctx().into_option().unwrap_or_else(|| RefAny::new(()));

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

    let mut children: Vec<Dom> = vec![Dom::create_h2_with_text("Report a problem")];
    match &snapshot.email {
        Some(to) => children.push(Dom::create_p_with_text(format!(
            "Describe what went wrong. The report goes to {to} — nothing is sent until you press Send."
        ))),
        None => children.push(Dom::create_p_with_text(
            "Describe what went wrong. The report is saved to disk (no support address is configured).",
        )),
    }

    match &snapshot.status {
        ReportStatus::Editing | ReportStatus::Failed(_) => {
            if let ReportStatus::Failed(e) = &snapshot.status {
                children.push(Dom::create_p_with_text(format!("Previous attempt failed: {e}")));
            }
            children.push(
                TextArea::create()
                    .with_text(AzString::from(snapshot.message.as_str()))
                    .with_placeholder(AzString::from("What were you doing when the problem happened?"))
                    .with_on_text_input(state.clone(), on_message_input as TextAreaOnTextInputCallbackType)
                    .dom(),
            );
            children.push(check_row(
                snapshot.include_sysinfo,
                "Include system information (CPU, GPU, OS, RAM)",
                on_toggle_sysinfo,
                state.clone(),
            ));
            if snapshot.screenshot_png.is_some() {
                children.push(check_row(
                    snapshot.attach_screenshot,
                    "Attach a screenshot of the window",
                    on_toggle_screenshot,
                    state.clone(),
                ));
            }
            children.push(button_row(vec![
                ("Send", on_send, state.clone()),
                ("Cancel", on_cancel, state),
            ]));
        }
        ReportStatus::Sending => {
            children.push(Dom::create_p_with_text("Sending the report…"));
        }
        ReportStatus::Done(msg) => {
            children.push(Dom::create_p_with_text(msg.as_str()));
            children.push(button_row(vec![("Close", on_cancel, state)]));
        }
    }

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css_props(pad)
            .with_children(children.into()),
    )
}

fn check_row(
    checked: bool,
    label: &str,
    cb: CheckBoxOnToggleCallbackType,
    state: RefAny,
) -> Dom {
    Dom::create_div().with_children(
        vec![
            CheckBox::create(checked).with_on_toggle(state, cb).dom(),
            Dom::create_p_with_text(label),
        ]
        .into(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_text_carries_message_and_sysinfo_toggle_is_respected() {
        crate::appenv::set_app_env(crate::appenv::AppEnv {
            app_name: "reporttest".to_owned(),
            current_version: "9.9.9".to_owned(),
            ..Default::default()
        });
        let with = build_report_text("it broke while saving", true);
        assert!(with.contains("reporttest 9.9.9"), "{with}");
        assert!(with.contains("it broke while saving"), "{with}");
        assert!(with.contains("System information"), "{with}");

        // The toggle is a PRIVACY control: off must mean ABSENT, not empty.
        let without = build_report_text("msg", false);
        assert!(!without.contains("System information"), "{without}");
    }

    #[test]
    fn empty_message_is_labeled_not_blank() {
        let text = build_report_text("   ", true);
        assert!(text.contains("(no user message)"), "{text}");
    }
}
