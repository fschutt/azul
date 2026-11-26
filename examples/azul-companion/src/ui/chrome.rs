//! The phone as a whole: which screen shows, inside the safe area, with the line that is shown for a moment on top.

use azul::prelude::*;
use azul_review_link::view::Session;

use azul_review_link::frame::{AgentFinished, FinishKind};

use super::kit::{column, div, px, text, Cx};
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

/// A line over the top of the pad for a moment (an agent finished: the pad is where the eyes are, the buttons stay readable).
fn banner(cx: &Cx, words: &str) -> Dom {
    div(&format!(
        "position: absolute; left: 0px; right: 0px; top: {}; display: flex; flex-direction: row; justify-content: center;",
        px(cx.z.pad.y + 10.0)
    ))
    .with_child(text(
        words,
        &format!(
            "max-width: {}; padding: 10px 16px; border-radius: 18px; background: {}; color: {}; font-size: 14px; \
             border: 1px solid {};",
            px(cx.env.width - 48.0),
            th::SURFACE_HI,
            th::TEXT,
            th::SEPARATOR
        ),
    ))
}

/// A line over the bottom of the screen for a moment.
fn toast(cx: &Cx, words: &str) -> Dom {
    let lift = cx.env.insets.bottom + 78.0;
    div(&format!(
        "position: absolute; left: 0px; right: 0px; bottom: {}; display: flex; flex-direction: row; justify-content: center;",
        px(lift)
    ))
    .with_child(text(
        words,
        &format!(
            "max-width: {}; padding: 10px 16px; border-radius: 18px; background: {}; color: {}; font-size: 14px; \
             border: 1px solid {};",
            px(cx.env.width - 48.0),
            th::SURFACE_HI,
            th::TEXT,
            th::SEPARATOR
        ),
    ))
}
