//! The right side of the File tab (a backstage) in the Outlook 2010 look (feature `azul`): the
//! pieces an Azlin app's File pages are made of, one copy for every app. The `Backstage`
//! widget draws the navigation column; these draw what is right of it.
//!
//! ```text
//! Account Information                                [page: the big title]
//! (@) ada@example.org                                [card: an account, a printer]
//!     IMAP mail.example.org:993
//! [+ Add Account]
//! -------------------------------------------------
//! [Account Settings]  Account Settings               [command: a button, a heading and
//!                     Modify the settings of ...       what the button does]
//! -------------------------------------------------
//! ```
//!
//! Help and Print are two [`columns`]: the commands on the left, facts or a preview on the
//! right, each under [`section`] headings. The colours are the system's (`system:text`,
//! `system:secondary-text`, `system:separator`), so the pages read in the light and the dark
//! mode alike.

use azul::{callbacks::ButtonOnClickCallbackType, prelude::*, str::String as AzString};

use crate::{
    l10n::label,
    pieces::{column, text},
};

/// The page: its padding, and the text colour of the mode.
const PAGE: &str = "padding: 20px 32px; color: system:text; min-width: 0px;";
/// The page's title ("Account Information").
const TITLE: &str = "font-size: 26px; margin-bottom: 14px;";
/// A section's heading ("Support", "Printer", "About AzMail") over its rule; under flora
/// flora's label (`.fl-label`): capitals in the label ink.
const SECTION: &str = "font-size: 15px; font-weight: bold; margin-top: 18px; padding-bottom: \
                       4px; border-bottom: 1px solid system:separator; @theme(flora) { \
                       font-size: 11px; text-transform: uppercase; letter-spacing: 0.12em; \
                       color: system:secondary-text; }";
/// A command's heading.
const HEADING: &str = "font-size: 15px; font-weight: bold;";
/// Secondary lines: what a command does, a card's details, a fact's label.
const SECONDARY: &str = "font-size: 13px; color: system:secondary-text; margin-top: 3px;";
/// The column a command's button stands in (Outlook's square tiles line up the same way).
const BUTTON_COLUMN: &str = "display: flex; flex-direction: column; align-items: flex-start; \
                             width: 170px; flex-shrink: 0;";

/// A File page: the big title over `children`, one under the other.
#[must_use]
pub fn page(title: &str, children: Vec<Dom>) -> Dom {
    let mut all = Vec::with_capacity(children.len() + 1);
    all.push(text(label(title)).with_css(TITLE));
    all.extend(children);
    column(PAGE, all)
}

/// A File page of two columns (Help, Print): `left` (the commands) and `right` (facts, a
/// preview), a rule between them.
#[must_use]
pub fn columns(title: &str, left: Vec<Dom>, right: Vec<Dom>) -> Dom {
    let row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: flex-start;")
        .with_child(column(
            "flex-grow: 1; flex-basis: 0px; min-width: 0px; padding-right: 24px;",
            left,
        ))
        .with_child(column(
            "flex-grow: 1; flex-basis: 0px; min-width: 0px; padding-left: 24px; border-left: \
             1px solid system:separator;",
            right,
        ));
    page(title, vec![row])
}

/// A section's heading over a rule ("Support", "Printer", "About AzMail").
#[must_use]
pub fn section(title: &str) -> Dom {
    text(label(title)).with_css(SECTION)
}

/// A line of secondary text.
#[must_use]
pub fn note(content: &str) -> Dom {
    text(label(content)).with_css(SECONDARY)
}

/// A card: an icon beside a bold title and its detail lines (an account under Info, the
/// printer under Print). `selected` draws it in the accent's frame (the account shown).
#[must_use]
pub fn card(icon: &str, title: &str, details: &[&str], selected: bool) -> Dom {
    let frame = if selected {
        "border: 1px solid system:accent;"
    } else {
        "border: 1px solid system:separator;"
    };
    let mut lines = vec![text(label(title)).with_css("font-size: 14px; font-weight: bold;")];
    for detail in details.iter().filter(|d| !d.is_empty()) {
        lines.push(note(detail));
    }
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
             margin-bottom: 8px; border-radius: 3px; max-width: 560px; {frame}"
        ))
        .with_child(
            Dom::create_icon(icon)
                // The card's glyph: the accent under flat, flora's brass (its large glyphs'
                // ink) under flora.
                .with_css(
                    "font-size: 32px; margin-right: 12px; color: system:accent; @theme(flora) { \
                     color: system:link; }",
                ),
        )
        .with_child(column("flex-grow: 1; min-width: 0px;", lines))
}

/// A command of a File page: `button` in the left column, `heading` and what the command does
/// beside it, a rule under the row (Outlook's Info rows: Account Settings, Automatic Replies,
/// Mailbox Cleanup).
#[must_use]
pub fn command(button: Dom, heading: &str, description: &str) -> Dom {
    let mut words = vec![text(label(heading)).with_css(HEADING)];
    if !description.is_empty() {
        words.push(note(description));
    }
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: flex-start; padding: 14px 0px; \
             border-bottom: 1px solid system:separator; max-width: 720px;",
        )
        .with_child(Dom::create_div().with_css(BUTTON_COLUMN).with_child(button))
        .with_child(column("flex-grow: 1; min-width: 0px;", words))
}

/// A command's button: `label` after `icon`, `id` for scripts, `cb` called with `data`.
#[must_use]
pub fn command_button(
    label_text: &str,
    icon: &str,
    id: AzString,
    data: RefAny,
    cb: ButtonOnClickCallbackType,
) -> Dom {
    Button::create(label(label_text))
        .with_icon(icon)
        .with_on_click(data, cb)
        .dom()
        .with_id(id)
}

/// Facts as label / value rows (Help's "About": the version, the licence, the data folder).
#[must_use]
pub fn facts(rows: &[(String, String)]) -> Dom {
    let lines = rows
        .iter()
        .map(|(name, value)| {
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; padding: 3px 0px;")
                .with_child(
                    text(label(name.as_str())).with_css(
                        "width: 130px; flex-shrink: 0; font-size: 13px; color: \
                         system:secondary-text;",
                    ),
                )
                .with_child(text(value.as_str()).with_css(
                    "flex-grow: 1; min-width: 0px; font-size: 13px; overflow-wrap: anywhere;",
                ))
        })
        .collect();
    column("margin-top: 6px;", lines)
}
