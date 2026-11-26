//! The small pieces every screen is made of: a block with a style, a row, a column, a line of text, an icon, a tappable
//! key, a card. Every tappable thing is a `Button` node with a name for the screen reader, at least
//! [`MIN_TARGET`](crate::layout::MIN_TARGET) tall and wide, and marked with the class `az-target` so the layout check
//! can find it.

use azul::prelude::*;
use azul_review_link::view::PhoneView;

use super::glue::ActCtx;
use super::{Action, Root};
use crate::env::{Env, Lang};
use crate::layout::Zones;
use crate::theme as th;

/// What a screen is built from.
pub struct Cx<'a> {
    pub root: &'a Root,
    pub env: &'a Env,
    /// The app state as the callbacks get it.
    pub data: &'a RefAny,
    pub z: Zones,
}

impl<'a> Cx<'a> {
    /// The language of the screens.
    #[must_use]
    pub fn lang(&self) -> Lang {
        self.env.lang
    }

    /// The words of `key`.
    #[must_use]
    pub fn t(&self, key: &str) -> String {
        self.root.strings.t(self.env.lang, key)
    }

    /// The words of `key` with its names filled in.
    #[must_use]
    pub fn tf(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.root.strings.tf(self.env.lang, key, args)
    }

    /// What the phone knows of the Mac.
    #[must_use]
    pub fn view(&self) -> &PhoneView {
        self.root.view()
    }
}

/// A length in logical pixels as the CSS writes it.
#[must_use]
pub fn px(v: f32) -> String {
    format!("{:.1}px", if v.is_finite() { v } else { 0.0 })
}

/// Every block is measured by its border box.
const BASE: &str = "box-sizing: border-box; ";

/// A block.
#[must_use]
pub fn div(css: &str) -> Dom {
    Dom::create_div().with_css(format!("{BASE}{css}").as_str())
}

/// A block that lays its children out in a row, centred across.
#[must_use]
pub fn row(css: &str) -> Dom {
    div(&format!("display: flex; flex-direction: row; align-items: center; {css}"))
}

/// A block that lays its children out in a column.
#[must_use]
pub fn column(css: &str) -> Dom {
    div(&format!("display: flex; flex-direction: column; {css}"))
}

/// A line of text that must be seen whole (the layout check finds it when it is not).
#[must_use]
pub fn text(words: &str, css: &str) -> Dom {
    Dom::create_p_with_text(words).with_css(format!("{BASE}margin: 0px; {css}").as_str()).with_class("az-text")
}

/// A line of text that is cut to its line on purpose (a file name, a path).
#[must_use]
pub fn text_clip(words: &str, css: &str) -> Dom {
    text(words, &format!("overflow: hidden; white-space: nowrap; {css}")).with_class("az-clip")
}

/// A Material icon.
#[must_use]
pub fn icon(name: &str, size: f32, colour: &str) -> Dom {
    Dom::create_icon(name).with_css(format!("font-size: {size}px; color: {colour}; flex-shrink: 0;").as_str())
}

/// A tappable key: a `Button` node without the system's face, named for the screen reader, doing `action` when it is
/// tapped. `css` gives its size and look.
#[must_use]
pub fn tap(name: &str, css: &str, cx: &Cx, action: Action) -> Dom {
    Dom::create_node(NodeType::Button)
        .with_css(
            format!(
                "{BASE}display: flex; align-items: center; justify-content: center; margin: 0px; padding: 0px; \
                 border: 0px solid transparent; background: transparent; color: {}; font-size: 15px; cursor: pointer; \
                 flex-shrink: 0; {css}",
                th::TEXT
            )
            .as_str(),
        )
        .with_class("az-target")
        .with_accessibility_name(name)
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), RefAny::new(ActCtx { root: cx.data.clone(), action }), super::glue::on_act)
}

/// A card: a surface with rounded corners.
#[must_use]
pub fn card(css: &str) -> Dom {
    div(&format!("background: {}; border-radius: 14px; {css}", th::SURFACE))
}

/// A hairline across a row or a column.
#[must_use]
pub fn hairline(css: &str) -> Dom {
    div(&format!("background: {}; flex-shrink: 0; {css}", th::SEPARATOR))
}

/// `text` kept to `max` characters by dropping the middle (`src/…/long_name.rs`): a path that is too long for its line
/// keeps its start and its file name.
#[must_use]
pub fn shorten_middle(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max || max < 5 {
        return text.to_string();
    }
    let keep = max - 1;
    let tail = keep * 2 / 3;
    let head = keep - tail;
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

#[cfg(test)]
#[path = "kit_test.rs"]
mod kit_test;
