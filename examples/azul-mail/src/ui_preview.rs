//! The message in azul's `Preview`: the mail ITSELF - who it is from and when, then its text or
//! its sanitized HTML on its paper - and nothing else. What the reading pane used to pile around
//! it is behind small (?) buttons in the Preview's header: the message's details (Sent, To, Cc),
//! its attachments and its people. What needs saying at once is the banner at the very top: the
//! pictures AzMail held back, with its own (?) holding the explanation and "Download pictures".
//!
//! On a desktop the Preview is docked at the right of the list; on a phone it is the bottom pane
//! (four detents, a tap on its grip toggles the peek and the half) and it follows the message at
//! the TOP EDGE of the list while the list scrolls (`ui_main::on_list_event`): a message is read
//! by a tap, never by scrolling past it, and the selection never follows the scroll.

use azul::{
    callbacks::{ButtonOnClickCallbackType, PreviewOnEventCallbackType},
    prelude::*,
    shells::ShellEmptyState,
    str::String as AzString,
    widgets::{
        Button, ButtonType, Preview, PreviewBanner, PreviewBannerTone, PreviewDetail, PreviewEvent,
        PreviewEventKind,
    },
};
use azul_appkit::l10n::{label, t, t_args, Arg};

use crate::{
    message::{self, MessageView},
    ui_main::{self, action_ref, on_action, Action},
    with_app, MailApp, OpenMessage,
};

/// The widest or tallest a Preview is taken to be from what a drag reported, px.
pub(crate) const PREVIEW_SIZE_MAX: f32 = 4_000.0;

/// A pane size a drag reported, held to what the app keeps: a number that is no size is 0 (the
/// widget's default), a size beyond [`PREVIEW_SIZE_MAX`] is the limit.
#[must_use]
pub(crate) fn clamped_preview_size(size: f32) -> f32 {
    if size.is_finite() && size > 0.0 {
        size.min(PREVIEW_SIZE_MAX)
    } else {
        0.0
    }
}

/// Whether the window has a Preview: the reading pane is on (View > Reading Pane), the mail
/// module is shown, and there is a mailbox to read from.
#[must_use]
pub(crate) const fn shows(reading_pane: bool, module: usize, has_mailbox: bool) -> bool {
    reading_pane && module == 0 && has_mailbox
}

/// The Preview of the window now; `None` where it has none (the body is the whole window).
pub(crate) fn preview(s: &MailApp, app: &RefAny) -> Option<Preview> {
    if !shows(s.show_reading, s.module, !s.accounts.is_empty() || s.shows_local()) {
        return None;
    }
    let mut preview = Preview::create(Dom::create_div(), pane(s))
        .with_title(title(s).as_str())
        .with_size(clamped_preview_size(s.preview_size))
        .with_form(s.form)
        .with_on_event(app.clone(), on_preview as PreviewOnEventCallbackType);
    if let Some(open) = &s.open {
        let view = open.view.clone().unwrap_or_default();
        preview = preview.with_detail(PreviewDetail::create(
            label("azmail-preview-details"),
            details_dom(open, &view),
        ));
        if !view.attachments.is_empty() {
            preview = preview.with_detail(
                PreviewDetail::create(label("azmail-preview-attachments"), attachments_dom(&view))
                    .with_icon("attach_file"),
            );
        }
        preview = preview.with_detail(
            PreviewDetail::create(label("azmail-preview-people"), people_dom(open, &view)).with_icon("person"),
        );
        if let Some(held) = pictures_held(s, open) {
            preview = preview.with_banner(
                PreviewBanner::create(
                    t_args("azmail-preview-pictures-held", &[("held", Arg::from(held.clone()))]).as_str(),
                    PreviewBannerTone::Info,
                )
                .with_icon("image")
                .with_details(held_dom(&held, app)),
            );
        }
    }
    Some(preview)
}

/// The Preview's title: the open message's subject, else "Message".
fn title(s: &MailApp) -> String {
    match &s.open {
        Some(open) => {
            let subject = open.view.as_ref().map_or("", |v| v.subject.as_str());
            if subject.is_empty() {
                t("azmail-no-subject")
            } else {
                subject.to_string()
            }
        }
        None => t("azmail-preview-title"),
    }
}

/// The pane: who the message is from and when, then the message. With none open, the empty state.
fn pane(s: &MailApp) -> Dom {
    let Some(open) = &s.open else {
        return ShellEmptyState::create(label("azmail-select-item"))
            .with_icon("mail")
            .with_detail(label("azmail-select-item-detail"))
            .dom();
    };
    let view = open.view.clone().unwrap_or_default();
    let sender = if view.from.is_empty() {
        open.entry.from.clone()
    } else {
        view.from.clone()
    };
    let date_source = if view.date.is_empty() {
        open.entry.date.as_str()
    } else {
        view.date.as_str()
    };
    let sent = message::short_date_in(date_source, &chrono::Local);
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-width: 0px;")
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: baseline; gap: 10px; padding: 8px 14px 4px 14px; \
                     font-size: 12px; color: system:secondary-text;",
                )
                .with_child(
                    Dom::create_span_with_text(ui_main::display_name(&sender))
                        .with_css("font-weight: 700; color: system:text; flex-shrink: 1; min-width: 0px;"),
                )
                .with_child(Dom::create_span_with_text(sent.as_str()).with_css("flex-shrink: 0;")),
        )
        .with_child(ui_main::message_body(s, open, &view))
}

/// A row of a popover: a name and what it says.
fn fact(name: AzString, value: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; gap: 8px; font-size: 12px;")
        .with_child(
            Dom::create_span_with_text(name.as_str())
                .with_css("flex-shrink: 0; width: 44px; color: system:secondary-text;"),
        )
        .with_child(Dom::create_span_with_text(value).with_css("flex-grow: 1; min-width: 0px; overflow-wrap: anywhere;"))
}

/// Behind the first (?): Sent, To, Cc.
fn details_dom(open: &OpenMessage, view: &MessageView) -> Dom {
    let date_source = if view.date.is_empty() {
        open.entry.date.as_str()
    } else {
        view.date.as_str()
    };
    let sent = message::short_date_in(date_source, &chrono::Local);
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 4px;")
        .with_child(fact(label("azmail-field-sent"), sent.as_str()));
    if !view.to.is_empty() {
        column.add_child(fact(label("azmail-field-to"), view.to.as_str()));
    }
    if !view.cc.is_empty() {
        column.add_child(fact(label("azmail-field-cc"), view.cc.as_str()));
    }
    column
}

/// Behind the attachments' (?): their names and sizes, one to a line.
fn attachments_dom(view: &MessageView) -> Dom {
    let mut column = Dom::create_div().with_css("display: flex; flex-direction: column; gap: 4px;");
    for attachment in &view.attachments {
        let size = azul_appkit::l10n::decimal(azul::file::DiskSpace::format_bytes(attachment.size as u64).as_str());
        column.add_child(
            Dom::create_span_with_text(format!("{} ({size})", attachment.name)).with_css("font-size: 12px;"),
        );
    }
    column
}

/// Behind the people's (?): the sender, with the initials the PIM apps show.
fn people_dom(open: &OpenMessage, view: &MessageView) -> Dom {
    let sender = if view.from.is_empty() {
        open.entry.from.clone()
    } else {
        view.from.clone()
    };
    let name = ui_main::display_name(&sender);
    let initials = azul_pim::initials::initials(&name);
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; gap: 10px;")
        .with_child(
            Dom::create_span_with_text(initials.as_str())
                .with_css("font-weight: 700; padding: 6px 8px; border-radius: 16px; background: rgba(127, 127, 127, 0.2);"),
        )
        .with_child(Dom::create_span_with_text(t_args("azmail-see-more-about", &[("name", Arg::from(name))])))
}

/// What the held-back pictures are called ("3 pictures and 1 font"), when the open message has
/// pictures held back and "Download pictures" was not clicked.
fn pictures_held(s: &MailApp, open: &OpenMessage) -> Option<String> {
    let sanitized = open.sanitized.as_ref().filter(|_| !s.plain_text)?;
    if sanitized.blocked_images == 0 || open.pictures {
        return None;
    }
    // what the pre-pass found on the web, else "some pictures" (pictures that are not on the web)
    Some(match sanitized.remote.summary() {
        summary if summary.is_empty() => t("azmail-some-pictures"),
        summary => summary,
    })
}

/// Behind the banner's (?): the explanation and the button that downloads them.
fn held_dom(held: &str, app: &RefAny) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 8px; max-width: 320px;")
        .with_child(
            Dom::create_span_with_text(t_args("azmail-pictures-held", &[("held", Arg::from(held))]))
                .with_css("font-size: 12px;"),
        )
        .with_child(
            Button::with_type(label("azmail-download-pictures"), ButtonType::Primary)
                .with_on_click(action_ref(app, Action::LoadPictures), on_action as ButtonOnClickCallbackType)
                .dom(),
        )
}

/// The pane's size as a drag reports it: kept for the next build.
extern "C" fn on_preview(mut data: RefAny, _info: CallbackInfo, event: PreviewEvent) -> Update {
    with_app(&mut data, |s, _| {
        s.preview_size = clamped_preview_size(event.size);
        // while the pane is dragged the widget moves it; a rebuild is for where it settled
        if event.kind == PreviewEventKind::Settled {
            Update::RefreshDom
        } else {
            Update::DoNothing
        }
    })
    .unwrap_or(Update::DoNothing)
}

#[cfg(test)]
#[path = "ui_preview_test.rs"]
mod ui_preview_test;
