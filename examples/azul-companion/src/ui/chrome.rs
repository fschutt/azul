//! The phone as a whole: which screen shows, inside the safe area, with the line that is shown for a moment on top.

use azul::prelude::*;
use azul_review_link::view::Session;

use super::kit::{column, div, px, text, Cx};
use super::{editor, files, pair, remote, settings, Screen};
use crate::theme as th;

/// The id of the phone's block (the layout check starts from it).
pub const PHONE_ID: &str = "az-phone";

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
    }
    shell
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
