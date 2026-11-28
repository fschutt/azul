//! The "Display settings" section: what the user's own settings (the system's text size, bold text,
//! increased contrast, reduced motion, reduced transparency, differentiate without colour) do to a
//! page, and nothing else.
//!
//! No line of this file reads a setting. The text grows because the engine multiplies every font
//! size by the user's scale; the borders and the text are read at higher contrast because the
//! painters ask the contrast setting; the frosted panel is opaque because the transparency effects
//! are off. The only code that answers a setting is a media query: the lines of the "What this
//! window reads" list are hidden until the setting that names them is on, and the glyphs of the
//! status chips appear with `differentiate-without-color`.
//!
//! The same file runs on a desktop, an emulator and a Simulator (the system's settings change under
//! it; `set_system_accessibility` of the E2E ops says them without touching the system).

use azul::{prelude::*, widgets::*};

use crate::{captioned, section};

/// A line that is there only while `query` (the inside of `@media (..)`) holds.
fn read_by(query: &str, words: &str) -> Dom {
    Dom::create_span_with_text(words).with_css(format!(
        "display: none; font-size: 13px; color: system:text; margin-bottom: 2px; \
         @media ({query}) {{ display: block; }}"
    ))
}

/// The settings this window reads, one line each while it is on.
fn what_this_window_reads() -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; min-height: 18px;")
        .with_child(
            Dom::create_span_with_text("Nothing is switched on: the page is as it was written.").with_css(
                "font-size: 13px; color: system:secondary-text; margin-bottom: 2px; \
                 @media (min-text-scale: 1.01) { display: none; } \
                 @media (prefers-bold-text: bold) { display: none; } \
                 @media (prefers-contrast: more) { display: none; } \
                 @media (prefers-reduced-motion) { display: none; } \
                 @media (prefers-reduced-transparency: reduce) { display: none; } \
                 @media (differentiate-without-color: active) { display: none; }",
            ),
        )
        .with_child(read_by("min-text-scale: 1.01", "Text size: larger than the default"))
        .with_child(read_by("min-text-scale: 1.3", "Text size: 1.3 or more"))
        .with_child(read_by("min-text-scale: 2", "Text size: 2.0 or more"))
        .with_child(read_by("prefers-bold-text: bold", "Bold text"))
        .with_child(read_by("prefers-contrast: more", "Increased contrast"))
        .with_child(read_by("forced-colors: active", "A contrast theme (forced colours)"))
        .with_child(read_by("prefers-reduced-motion", "Reduced motion"))
        .with_child(read_by("prefers-reduced-transparency: reduce", "Reduced transparency"))
        .with_child(read_by("differentiate-without-color: active", "Differentiate without colour"))
        .with_child(read_by("screen-reader: active", "A screen reader is running"))
}

/// Text at four sizes, a box that is sized in px and one that is sized by its text.
fn text_samples() -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 6px;")
        .with_child(
            Dom::create_span_with_text("Heading, 24 px")
                .with_css("font-size: 24px; font-weight: bold; color: system:text;"),
        )
        .with_child(Dom::create_span_with_text("Body text, 16 px").with_css("font-size: 16px; color: system:text;"))
        .with_child(
            Dom::create_span_with_text("Secondary text, 13 px")
                .with_css("font-size: 13px; color: system:secondary-text;"),
        )
        .with_child(
            Dom::create_span_with_text("Small print, 11 px")
                .with_css("font-size: 11px; color: system:tertiary-text;"),
        )
        // sized in px: it does not grow, and the text inside is clipped by it only if it outgrows it
        .with_child(
            Dom::create_div()
                .with_css(
                    "width: 160px; height: 28px; border: 1px solid system:separator; \
                     border-radius: 4px; padding: 0 8px; display: flex; align-items: center;",
                )
                .with_child(
                    Dom::create_span_with_text("A box of 160 x 28 px")
                        .with_css("font-size: 12px; color: system:text;"),
                ),
        )
        // sized by its text: it grows with it
        .with_child(
            Dom::create_div()
                .with_css(
                    "min-height: 28px; height: auto; width: 160px; border: 1px solid system:separator; \
                     border-radius: 4px; padding: 4px 8px; display: flex; align-items: center;",
                )
                .with_child(
                    Dom::create_span_with_text("A box that follows its text")
                        .with_css("font-size: 12px; color: system:text;"),
                ),
        )
}

/// A frosted panel over a strip of colour: a translucent fill and a blur behind it.
fn frosted_panel() -> Dom {
    Dom::create_div()
        .with_css(
            "position: relative; height: 112px; border-radius: 8px; overflow: hidden; \
             background: linear-gradient(90deg, #d4402f, #f2b134 50%, #2f6fd4);",
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "position: absolute; left: 24px; right: 24px; top: 20px; bottom: 20px; \
                     border-radius: 8px; padding: 10px 14px; \
                     background: rgba(255, 255, 255, 0.40); backdrop-filter: blur(8px);",
                )
                .with_child(
                    Dom::create_span_with_text(
                        "A frosted panel: translucent over a blur. With the transparency effects \
                         off it is an opaque fill and no blur.",
                    )
                    .with_css("font-size: 14px; color: #14161a;"),
                ),
        )
}

/// Chrome whose edges are light: a card, a ring, a rule, a button, a switch.
fn controls(theme: UiTheme) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 14px; align-items: center;")
        .with_child(
            Dom::create_div()
                .with_css(
                    "padding: 8px 12px; border-radius: 6px; background: #f1f2f4; \
                     border: 1px solid #d6d9de; color: #3d4350; font-size: 13px;",
                )
                .with_child(Dom::create_span_with_text("A card with a light edge")),
        )
        .with_child(
            Dom::create_div()
                .with_css(
                    "padding: 8px 12px; border-radius: 6px; background: transparent; \
                     box-shadow: 0 0 0 2px #c9d3e6; color: #3d4350; font-size: 13px;",
                )
                .with_child(Dom::create_span_with_text("A ring")),
        )
        .with_child(Button::create("A button").with_theme(theme).dom())
        .with_child(CheckBox::create(true).with_accessibility_name("A checked box").with_theme(theme).dom())
}

/// One status chip: a coloured fill, its word, and a glyph that comes with
/// `differentiate-without-color` (colour alone must not carry the meaning).
fn chip(word: &str, glyph: &str, fill: &str, ink: &str, edge_when_colourless: &str) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; padding: 3px 10px; \
             border-radius: 10px; background: {fill}; color: {ink}; font-size: 13px; \
             border: 1px solid {fill}; \
             @media (differentiate-without-color: active) {{ border: 1px {edge_when_colourless} {ink}; }}"
        ))
        .with_child(Dom::create_span_with_text(format!("{glyph} ")).with_css(
            "display: none; @media (differentiate-without-color: active) { display: inline; }",
        ))
        .with_child(Dom::create_span_with_text(word))
}

fn status_chips() -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-wrap: wrap; gap: 10px;")
        .with_child(chip("Saved", "\u{2713}", "#2e7d4f", "#ffffff", "solid"))
        .with_child(chip("Failed", "\u{2715}", "#b3261e", "#ffffff", "dashed"))
        .with_child(chip("Pending", "\u{2026}", "#f2b134", "#1a1a1a", "dotted"))
}

/// The section's card.
pub(crate) fn display_settings_section(theme: UiTheme) -> Dom {
    section(
        "Display settings",
        vec![
            captioned("What this window reads", what_this_window_reads()),
            captioned("Text: the system's text size", text_samples()),
            captioned("A frosted surface: the transparency effects", frosted_panel()),
            captioned("Edges, rings and rules: the contrast", controls(theme)),
            captioned("Status that does not rely on colour: differentiate without colour", status_chips()),
        ],
    )
}
