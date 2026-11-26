//! The phone as a whole: which screen shows, inside the safe area, with the line that is shown for a moment on top.

use azul::prelude::*;
use azul_review_link::view::Session;

use azul_review_link::frame::{AgentFinished, FinishKind};

use super::kit::{column, px, text, Cx};
use super::{editor, files, pair, remote, settings, Screen};
use crate::theme as th;

/// The id of the phone's block (the layout check starts from it).
pub const PHONE_ID: &str = "az-phone";

/// The root of the page: a body without margins round the phone, as the simulator's wrapper has it, so that the phone's own app and the
/// layout study lay out the same tree.
#[must_use]
pub fn body(phone: Dom) -> Dom {
    Dom::create_body().with_css(format!("margin: 0px; padding: 0px; background: {}; font-family: system:ui, sans-serif;", th::BG).as_str()).with_child(phone)
}

/// The phone's screens in a block of the phone's size (the system's bars are the safe area's margins).
#[must_use]
pub fn phone(cx: &Cx) -> Dom {
    let ui = &cx.root.ui;
    let view = cx.view();
    let pairing = ui.pair_again
        || match &view.session {
            Session::Connected { .. } => ui.pair_card,
            Session::Lost { .. } => false,
            _ => true,
        };
    let screen = if pairing {
        pair::dom(cx)
    } else {
        match ui.screen {
            Screen::Remote => remote::dom(cx),
            Screen::Files => files::dom(cx),
            Screen::Editor => editor::dom(cx),
            Screen::Settings => settings::dom(cx),
        }
    };
    let ins = cx.env.insets.held(cx.env.width, cx.env.height);
    let mut shell = column(&format!(
        "position: relative; width: {}; height: {}; padding: {} {} {} {}; background: {}; color: {}; overflow: hidden; font-family: system:ui, sans-serif;",
        px(cx.env.width),
        px(cx.env.height),
        px(ins.top),
        px(ins.right),
        px(ins.bottom),
        px(ins.left),
        th::BG,
        th::TEXT
    ))
    .with_id(PHONE_ID)
    // the first mount of the screens says the window is ready (the timer, the connection): the phones' shells never call the window's
    // creation callback, and every shell reports a mount
    .with_callback(EventFilter::Component(ComponentEventFilter::AfterMount), cx.data.clone(), super::glue::on_window_ready)
    .with_child(screen);
    if let Some((key, _)) = &ui.toast {
        shell.add_child(toast(cx, &cx.t(key)));
    } else if let Some((done, _)) = &ui.finished_note {
        shell.add_child(banner(cx, &finished_words(cx, done)));
    }
    shell
}

/// What an agent that finished says: how it ended and where (the file and the line).
fn finished_words(cx: &Cx, done: &AgentFinished) -> String {
    let file = done.file.rsplit('/').next().unwrap_or(&done.file);
    let place = match (file.is_empty(), done.line) {
        (true, _) => String::new(),
        (false, 0) => file.to_string(),
        (false, line) => format!("{file}:{line}"),
    };
    let key = match done.kind {
        FinishKind::Done => "finished-done",
        FinishKind::Suggestion => "finished-suggestion",
        FinishKind::NoAction => "finished-noaction",
        FinishKind::Failed => "finished-failed",
    };
    cx.tf(key, &[("where", &place)])
}

/// A line over the top of the pad for a moment (an agent finished: the pad is where the eyes are, the buttons stay readable). The card
/// is the node that lies over the screen, with nothing round it: there is no `pointer-events: none`, so a wrapper as wide as the screen would
/// swallow the taps meant for what is under it, beside the card as well.
fn banner(cx: &Cx, words: &str) -> Dom {
    text(
        words,
        &format!(
            "position: absolute; left: 24px; right: 24px; top: {}; padding: 10px 16px; border-radius: 18px; background: {}; color: {}; \
             font-size: 14px; text-align: center; border: 1px solid {};",
            px(cx.z.pad.y + 10.0),
            th::SURFACE_HI,
            th::TEXT,
            th::SEPARATOR
        ),
    )
}

/// A line over the bottom of the screen for a moment (a card with nothing round it, as the banner).
fn toast(cx: &Cx, words: &str) -> Dom {
    let lift = cx.env.insets.bottom + 78.0;
    text(
        words,
        &format!(
            "position: absolute; left: 24px; right: 24px; bottom: {}; padding: 10px 16px; border-radius: 18px; background: {}; color: {}; \
             font-size: 14px; text-align: center; border: 1px solid {};",
            px(lift),
            th::SURFACE_HI,
            th::TEXT,
            th::SEPARATOR
        ),
    )
}

#[cfg(test)]
#[path = "chrome_test.rs"]
mod chrome_test;
