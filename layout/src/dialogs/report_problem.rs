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
use crate::thread::{
    Thread, ThreadCallbackType, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg,
    WriteBackCallbackType,
};
use crate::widgets::button::{Button, ButtonOnClickCallbackType};
use crate::widgets::check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState};
use crate::widgets::text_area::{TextArea, TextAreaOnTextInputCallbackType, TextAreaState};
use crate::widgets::text_input::{OnTextInputReturn, TextInputValid};
use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType};
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
// Each bool is one independent opt-in checkbox in the dialog.
#[allow(clippy::struct_excessive_bools)]
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
    /// Attach the ACTION JOURNAL (handler names + nodes, no user content).
    pub include_actions: bool,
    /// Attach the app's serialized state. DEFAULT OFF: this is the one
    /// section that can carry the user's actual document.
    pub include_app_data: bool,
    /// Blackout rectangles the user drew on the preview, in PREVIEW
    /// coordinates. Applied to the PNG that is actually sent.
    pub redactions: Vec<crate::dialogs::report::RedactRect>,
    /// First corner of the blackout currently being dragged.
    pub drag_start: Option<(f32, f32)>,
    /// `image_px / preview_px`, so a rectangle drawn on the preview lands on
    /// the right pixels of the full-size capture.
    pub preview_scale: f32,
    /// Preview size in logical pixels (width, height).
    pub preview_size: (f32, f32),
    pub status: ReportStatus,
}

/// Opens the dialog. `screenshot_png` is the already-captured window
/// screenshot (the capture happens in `invoke_system_dialog`, BEFORE this
/// window exists, so the dialog itself is never in the picture).
pub fn open(info: &mut CallbackInfo, screenshot_png: Option<Vec<u8>>) {
    let env = crate::appenv::app_env();
    // Fit the capture into the dialog: the preview is a scaled copy, and the
    // scale is REMEMBERED so a blackout drawn here maps to the real pixels.
    const PREVIEW_MAX_W: f32 = 460.0;
    let (preview_size, preview_scale) = screenshot_png
        .as_deref()
        .and_then(|png| crate::cpurender::AzulPixmap::decode_png(png).ok())
        .map_or(((0.0, 0.0), 1.0), |p| {
            #[allow(clippy::cast_precision_loss)]
            let (w, h) = (p.width() as f32, p.height() as f32);
            if w <= 0.0 || h <= 0.0 {
                return ((0.0, 0.0), 1.0);
            }
            let scale = (w / PREVIEW_MAX_W).max(1.0);
            ((w / scale, h / scale), scale)
        });
    let state = RefAny::new(ReportProblemState {
        email: env.report_problem,
        message: String::new(),
        include_sysinfo: true,
        attach_screenshot: screenshot_png.is_some(),
        screenshot_png,
        // The journal carries handler names and node ids, never user
        // content, so it defaults ON; app data is the user's document and
        // defaults OFF.
        include_actions: crate::journal::is_enabled(),
        include_app_data: false,
        redactions: Vec::new(),
        drag_start: None,
        preview_scale,
        preview_size,
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

extern "C" fn on_toggle_actions(
    mut state: RefAny,
    _info: CallbackInfo,
    cb: CheckBoxState,
) -> Update {
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.include_actions = cb.checked;
    }
    Update::DoNothing
}

extern "C" fn on_toggle_app_data(
    mut state: RefAny,
    _info: CallbackInfo,
    cb: CheckBoxState,
) -> Update {
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.include_app_data = cb.checked;
    }
    Update::DoNothing
}

/// First corner of a blackout drag.
extern "C" fn on_preview_mouse_down(mut state: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.drag_start = Some((pos.x, pos.y));
    }
    Update::DoNothing
}

/// Second corner: the rectangle is added and drawn over the preview. It is
/// applied to the PNG at SEND time (see `on_send`) — the overlay here is a
/// preview of a redaction that really happens, not the redaction itself.
extern "C" fn on_preview_mouse_up(mut state: RefAny, mut info: CallbackInfo) -> Update {
    let Some(pos) = info.get_cursor_relative_to_node().into_option() else {
        return Update::DoNothing;
    };
    let Some(mut s) = state.downcast_mut::<ReportProblemState>() else {
        return Update::DoNothing;
    };
    let Some((sx, sy)) = s.drag_start.take() else {
        return Update::DoNothing;
    };
    let rect = crate::dialogs::report::RedactRect::from_corners(sx, sy, pos.x, pos.y);
    if rect.is_empty() {
        return Update::DoNothing;
    }
    s.redactions.push(rect);
    Update::RefreshDomAllWindows
}

extern "C" fn on_clear_redactions(mut state: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = state.downcast_mut::<ReportProblemState>() {
        s.redactions.clear();
        s.drag_start = None;
    }
    Update::RefreshDomAllWindows
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
        // The redactions are applied HERE, to the bytes that will be
        // attached. If the blackout cannot be applied, the screenshot is
        // DROPPED rather than sent unredacted.
        let screenshot = if s.attach_screenshot {
            match (&s.screenshot_png, s.redactions.is_empty()) {
                (Some(png), true) => Some(png.clone()),
                (Some(png), false) => {
                    crate::dialogs::report::redact_png(png, &s.redactions, s.preview_scale).ok()
                }
                (None, _) => None,
            }
        } else {
            None
        };
        SendTask {
            email: s.email.clone(),
            report: build_report_text(&s.message, s.include_sysinfo, s.include_actions),
            screenshot,
            recent_actions: if s.include_actions {
                Some(crate::journal::recent_json(
                    crate::journal::DEFAULT_CAPACITY,
                ))
            } else {
                None
            },
            app_data: if s.include_app_data {
                app_data_json(&info)
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
fn build_report_text(message: &str, include_sysinfo: bool, include_actions: bool) -> String {
    use std::fmt::Write as _;
    let env = crate::appenv::app_env();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Problem report — {} {}",
        env.app_name, env.current_version
    );
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
    if include_actions {
        let _ = writeln!(out, "\nRecent actions are attached as recent-actions.json.");
    }
    out
}

/// The app's serialized state, when the app registered a JSON serializer
/// (`RefAny::set_serialize_fn`) — the honest reading of "include app data".
/// Without a serializer there is nothing to include and the report says so
/// rather than attaching an empty file.
#[cfg(feature = "json")]
fn app_data_json(info: &CallbackInfo) -> Option<String> {
    let data = info.get_ctx().into_option()?;
    let json = crate::json::serialize_refany_to_json(&data)?;
    Some(json.to_json_string().as_str().to_owned())
}

/// Without the `json` feature there is no serializer to ask.
#[cfg(not(feature = "json"))]
const fn app_data_json(_info: &CallbackInfo) -> Option<String> {
    None
}

struct SendTask {
    email: Option<String>,
    report: String,
    screenshot: Option<Vec<u8>>,
    recent_actions: Option<String>,
    app_data: Option<String>,
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
    let recent_actions = task.recent_actions.clone();
    let app_data = task.app_data.clone();
    drop(task);

    let mut attachments: Vec<(String, Vec<u8>)> =
        vec![("report.txt".to_owned(), report.clone().into_bytes())];
    if let Some(png) = screenshot {
        attachments.push(("screenshot.png".to_owned(), png));
    }
    if let Some(actions) = recent_actions {
        attachments.push(("recent-actions.json".to_owned(), actions.into_bytes()));
    }
    if let Some(data) = app_data {
        attachments.push(("app-data.json".to_owned(), data.into_bytes()));
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
                children.push(Dom::create_p_with_text(format!(
                    "Previous attempt failed: {e}"
                )));
            }
            children.push(
                TextArea::create()
                    .with_text(AzString::from(snapshot.message.as_str()))
                    .with_placeholder(AzString::from(
                        "What were you doing when the problem happened?",
                    ))
                    .with_on_text_input(
                        state.clone(),
                        on_message_input as TextAreaOnTextInputCallbackType,
                    )
                    .dom(),
            );
            children.push(check_row(
                snapshot.include_sysinfo,
                "Include system information (CPU, GPU, OS, RAM)",
                on_toggle_sysinfo,
                state.clone(),
            ));
            children.push(check_row(
                snapshot.include_actions,
                "Include recent actions (which handlers ran - no typed text)",
                on_toggle_actions,
                state.clone(),
            ));
            children.push(check_row(
                snapshot.include_app_data,
                "Include application data (your document - off by default)",
                on_toggle_app_data,
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
            if snapshot.attach_screenshot {
                children.extend(preview_section(&snapshot, &state));
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

/// The screenshot preview plus its blackout overlay.
///
/// Drag on the image to cover anything private; the rectangles are drawn
/// here and applied to the attached PNG at send time. Nothing about the
/// preview is decorative: what you black out is what leaves the machine.
fn preview_section(snapshot: &ReportProblemState, state: &RefAny) -> Vec<Dom> {
    use azul_core::{
        callbacks::{CoreCallback, CoreCallbackType},
        dom::{EventFilter, HoverEventFilter},
    };
    use azul_css::{
        dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
        props::{
            basic::color::ColorU,
            layout::{LayoutHeight, LayoutLeft, LayoutPosition, LayoutTop, LayoutWidth},
            property::CssProperty,
            style::StyleBackgroundContent,
        },
    };

    let Some(png) = snapshot.screenshot_png.as_deref() else {
        return Vec::new();
    };
    let (pw, ph) = snapshot.preview_size;
    if pw <= 0.0 || ph <= 0.0 {
        return Vec::new();
    }
    let Ok(pixmap) = crate::cpurender::AzulPixmap::decode_png(png) else {
        return vec![Dom::create_p_with_text(
            "The screenshot could not be decoded for preview; it will not be attached.",
        )];
    };
    let raw = azul_core::resources::RawImage {
        pixels: azul_core::resources::RawImageData::U8(pixmap.data().to_vec().into()),
        width: pixmap.width() as usize,
        height: pixmap.height() as usize,
        premultiplied_alpha: false,
        data_format: azul_core::resources::RawImageFormat::RGBA8,
        tag: Vec::new().into(),
    };
    let Some(image_ref) = azul_core::resources::ImageRef::new_rawimage(raw) else {
        return vec![Dom::create_p_with_text(
            "The screenshot could not be prepared for preview.",
        )];
    };

    let sized = |w: f32, h: f32| {
        style(vec![
            CssProperty::width(LayoutWidth::px(w)),
            CssProperty::height(LayoutHeight::px(h)),
        ])
    };

    // The image, listening for the two drag corners.
    let image = Dom::create_image(image_ref)
        .with_css_props(sized(pw, ph))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseDown),
            state.clone(),
            CoreCallback {
                cb: on_preview_mouse_down as *const () as CoreCallbackType,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        )
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            state.clone(),
            CoreCallback {
                cb: on_preview_mouse_up as *const () as CoreCallbackType,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        );

    // Absolutely-positioned black rectangles over it.
    let mut stack: Vec<Dom> = vec![image];
    for rect in &snapshot.redactions {
        let r = rect.normalized();
        stack.push(Dom::create_div().with_css_props(style(vec![
                CssProperty::position(LayoutPosition::Absolute),
                CssProperty::left(LayoutLeft::px(r.x)),
                CssProperty::top(LayoutTop::px(r.y)),
                CssProperty::width(LayoutWidth::px(r.width)),
                CssProperty::height(LayoutHeight::px(r.height)),
                CssProperty::background_content(
                    vec![StyleBackgroundContent::Color(ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 255,
                    })]
                    .into(),
                ),
            ])));
    }

    vec![
        Dom::create_p_with_text(
            "Drag on the preview to black out anything private - the blackout is \
             applied to the image that is sent.",
        ),
        Dom::create_div()
            .with_css_props({
                let mut props = sized(pw, ph).as_ref().to_vec();
                props.push(CssPropertyWithConditions::simple(CssProperty::position(
                    LayoutPosition::Relative,
                )));
                CssPropertyWithConditionsVec::from_vec(props)
            })
            .with_children(stack.into()),
        button_row(vec![(
            "Clear blackouts",
            on_clear_redactions as ButtonOnClickCallbackType,
            state.clone(),
        )]),
    ]
}

fn check_row(checked: bool, label: &str, cb: CheckBoxOnToggleCallbackType, state: RefAny) -> Dom {
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
        let with = build_report_text("it broke while saving", true, false);
        assert!(with.contains("reporttest 9.9.9"), "{with}");
        assert!(with.contains("it broke while saving"), "{with}");
        assert!(with.contains("System information"), "{with}");

        // The toggle is a PRIVACY control: off must mean ABSENT, not empty.
        let without = build_report_text("msg", false, false);
        assert!(!without.contains("System information"), "{without}");
    }

    /// LAW: every opt-in section is ABSENT when declined — the report must
    /// never mention data the user chose not to send.
    #[test]
    fn the_recent_actions_section_follows_its_toggle() {
        let on = build_report_text("msg", false, true);
        assert!(on.contains("recent-actions.json"), "{on}");
        let off = build_report_text("msg", false, false);
        assert!(!off.contains("recent-actions"), "{off}");
    }

    #[test]
    fn empty_message_is_labeled_not_blank() {
        let text = build_report_text("   ", true, false);
        assert!(text.contains("(no user message)"), "{text}");
    }
}
