//! A facts row in a card keeps its words: AzCtl's Local environment page, a ~1000 px window
//! with its sidebar, under flora.
//!
//! The lead's screenshot of 2026-10-10: every "facts" row on the page (the state card's
//! "Nodes" / "State folder" / "Since", the apps card's six rows) showed its label and its value
//! squeezed to a letter or two per line. azul-apps d658e9a worked around it with plain lines;
//! this test lays the page out as AzCtl built it before that commit (`view.rs`: `local_page`,
//! `local_state`, `local_apps`; `ui.rs`: `card`, `facts`, `vcol`, `hrow`, `para`,
//! `form_row`; `look.rs`: every CSS string below, copied verbatim with its `themed!` macro), in
//! the same shell (`lib.rs::layout`: `ShellThemeScope::body()` around a flex column around
//! `MediaShell::create(sidebar, page, status).with_sidebar_ratio(0.2).office_shell()` with a
//! title row), in the app theme flora that AzCtl pins.
//!
//! What Chrome does with that CSS, box by box (window 1000 x 720):
//!
//! - the body (`margin: 0; height: 100%; display: flex; flex-direction: column`), the scope
//!   root and the shell root (`display: flex; flex-direction: column; flex-grow: 1;
//!   width: 100%; height: 100%; min-width: 0; overflow: hidden`): 1000 px wide.
//! - the shell's body row and its splits box (`display: flex; flex-grow: 1; min-width: 0`):
//!   1000 px. The split pane's container (`display: flex; flex-direction: row; width: 100%`)
//!   holds the sidebar pane (`flex-grow: 0.2; flex-basis: 0; min-width: 0`), the 6 px divider
//!   (`flex-grow: 0; flex-shrink: 0`) and the content pane (`flex-grow: 0.8; flex-basis: 0;
//!   min-width: 0`). Both panes have a zero base size and `min-width: 0`, so they share the
//!   994 px the divider leaves by their grow factors, whatever their content's min-content
//!   width: the sidebar 198.8 px, the content 795.2 px.
//! - the main pane in the content pane (`display: flex; flex-direction: column; width: 100%`):
//!   795.2 px. The page (`look::PAGE`: `width: 100%; box-sizing: border-box; padding: 12px
//!   16px; overflow-y: auto`): 795.2 px, its content box 763.2 px (less a classic scrollbar
//!   if the page shows one: at most 16 px).
//! - the page is a flex column with the default `align-items: stretch`, and so is every box
//!   from there down to a facts row: the page's `vcol("min-width: 0px;")`, the card
//!   (`look::CARD`: `display: flex; flex-direction: column; flex-shrink: 0; min-width: 0;
//!   border: 1px`), the card's body (`look::CARD_BODY`: `padding: 10px 12px`), the body's
//!   `vcol`, `facts`' `vcol`. A stretched item with `width: auto` takes its container's inner
//!   width (CSS Flexbox 9.4 step 11; its min-content width does not come into it), so the row
//!   (`look::FACT_ROW`: `display: flex; flex-direction: row; align-items: baseline`) is
//!   763.2 - 2 (the card's border) - 24 (the body's padding) = 737.2 px wide (721.2 with a
//!   classic scrollbar).
//! - in the row, the label (`look::FACT_LABEL`: `width: 160px; flex-shrink: 0;
//!   padding-right: 8px`, a `<p>` with `margin: 0`) has a definite flex basis of 160 px and
//!   never shrinks: its border box is 168 px. The value (`look::FACT_VALUE`: `flex-grow: 1;
//!   min-width: 0; overflow-wrap: anywhere`) starts at its max-content width and grows into
//!   the rest: 737.2 - 168 = 569.2 px.
//! - a label (at most "They use this environment", ~141 px at 12px) fits its 160 px content
//!   box on one line; a value (at most 49 characters at 13px, under ~350 px) fits its 569 px
//!   on one line. Were a box narrower, Chrome would wrap its text between words (a label has
//!   the default `overflow-wrap: normal`, so a word is never cut); only a value may break
//!   inside a word (`overflow-wrap: anywhere`), and only when that word is wider than its box.
//!
//! So the test checks, for every facts row, with the rects in every message (and the whole
//! chain printed first): the row is as wide as its card's body (at least 700 px), the label's
//! box is 168 px wide and the value takes the rest of the row, and the painted text of each
//! label and value starts inside its box on no more lines than it has words - one line here.
//! It checks the window's first frame, a rebuild of the same page (AzCtl rebuilds its DOM when
//! a poll brings news; the layout caches carry over) and a resize to 1200 px and back.
//! A second test lays the same two cards out in a plain 763.2 px column, without the shell, to
//! tell a shell bug from a card bug.
//!
//! The values are made-up neutral paths, not anything from a real machine.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    app_theme::ThemeScope,
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::DisplayListItem,
    widgets::{
        button::{Button, ButtonType},
        segmented::Segmented,
        shells::{MediaShell, ShellThemeAccent, ShellThemeScope},
        text_input::TextInput,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The window (a ~1000 px window, as on the lead's screenshot).
const WINDOW_W: f32 = 1000.0;
const WINDOW_H: f32 = 720.0;
/// `lib.rs::SIDEBAR_RATIO`.
const SIDEBAR_RATIO: f32 = 0.2;
/// The split pane's divider (`split_pane::DIVIDER_THICKNESS`).
const DIVIDER: f32 = 6.0;
/// The page's content box in Chrome: the content pane, less the page's 16 px side paddings.
const PAGE_CONTENT_W: f32 = (WINDOW_W - DIVIDER) * (1.0 - SIDEBAR_RATIO) - 32.0;
/// A facts label's border box: `width: 160px` and `padding-right: 8px`.
const LABEL_W: f32 = 168.0;
/// The card's border (1 px a side) and its body's padding (12 px a side).
const CARD_INSET: f32 = 2.0 + 24.0;

/// The state card's facts (`view.rs::local_state`).
const STATE_FACTS: [(&str, &str); 3] = [
    ("Nodes", "3 as processes (block local, 2+1)"),
    ("State folder", "/home/dev/.local/share/azlin/local"),
    ("Since", "2026-10-10 05:06:40 UTC (3 min ago)"),
];

/// The state card's facts a poll later: AzCtl's `when()` words the time relative to now, so a
/// poll that brings news rewrites the "Since" value (and the page head's "Fetched" words).
const STATE_FACTS_LATER: [(&str, &str); 3] = [
    ("Nodes", "3 as processes (block local, 2+1)"),
    ("State folder", "/home/dev/.local/share/azlin/local"),
    ("Since", "2026-10-10 05:06:40 UTC (4 min ago)"),
];

std::thread_local! {
    /// Which poll the page shows: 0 the first, 1 the next (its times moved on).
    static POLL: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The state card's facts at the current poll.
fn state_facts() -> &'static [(&'static str, &'static str)] {
    if POLL.with(std::cell::Cell::get) == 0 {
        &STATE_FACTS
    } else {
        &STATE_FACTS_LATER
    }
}

/// The apps card's facts (`view.rs::local_apps`).
const APPS_FACTS: [(&str, &str); 6] = [
    ("Shared Azlin config", "/home/dev/.config/azlin/config.toml"),
    ("Its profile", "local"),
    ("The apps' token server", "http://127.0.0.1:8787"),
    ("They use this environment", "yes"),
    ("Copy of the file before", "/home/dev/.config/azlin/config.toml.before-local"),
    ("AzCtl works on", "dev"),
];

// ==== AzCtl's look (`apps/azctl/src/look.rs` before d658e9a, verbatim) ====

/// One part's look: `base` (every theme), then `flat` by day, `flat_dark`, `flora` by day and
/// `flora_dark`; state blocks (`:hover { .. }`) may nest in each.
macro_rules! themed {
    ($base:literal, $flat:literal, $flat_dark:literal, $flora:literal, $flora_dark:literal $(,)?) => {
        concat!(
            $base,
            " ",
            $flat,
            " @media (prefers-color-scheme: dark) { ",
            $flat_dark,
            " } @theme(flora) { ",
            $flora,
            " @media (prefers-color-scheme: dark) { ",
            $flora_dark,
            " } }"
        )
    };
}

/// The sidebar's column.
const SIDEBAR: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; width: 100%; height: 100%; \
     min-width: 0px; min-height: 0px; overflow: hidden; font-size: 13px;",
    "background: linear-gradient(to bottom, #EAEFF5, #D6DEE8); color: #1E1E1E;",
    "background: linear-gradient(to bottom, #303338, #25272B); color: #F2F2F2;",
    "background: linear-gradient(to bottom, #EEECE7, #DCD9D2); color: #262521;",
    "background: linear-gradient(to bottom, #282828, #1B1B1B); color: #E7E7E7;",
);

/// The sidebar's head: the environment and the azctl it drives.
const SIDEBAR_HEAD: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0; padding: 12px 12px 8px 12px; \
     font-size: 11px;",
    "border-bottom: 1px solid #C2C9D2; color: #4C607A;",
    "border-bottom: 1px solid #575757; color: #B0B8C4;",
    "border-bottom: 1px solid #C6C3BB; color: #66645C;",
    "border-bottom: 1px solid #3F3F3F; color: #A8A8A8;",
);

/// The sidebar's title over its rows.
const SIDEBAR_TITLE: &str = themed!(
    "font-size: 15px; font-weight: 700; padding-bottom: 4px;",
    "color: #1E3287;",
    "color: #DCE3F2;",
    "color: #262521; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     letter-spacing: 1px;",
    "color: #E7E7E7; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     letter-spacing: 1px;",
);

/// The rows' column.
const SIDEBAR_LIST: &str =
    "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
     padding: 6px 0px 10px 0px;";

/// A page's row.
const NAV_ROW: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; height: 28px; flex-shrink: 0; \
     margin: 0px 6px 1px 6px; padding: 0px 8px; border-radius: 5px; cursor: pointer;",
    ":hover { background: #F2F5F9; } :focus { box-shadow: inset 0px 0px 0px 2px #4286F4; }",
    ":hover { background: #3A3D43; } :focus { box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    ":hover { background: #F6F5F1; } :focus { box-shadow: inset 0px 0px 0px 2px #8AA0B0; }",
    ":hover { background: #323232; } :focus { box-shadow: inset 0px 0px 0px 2px #8AA0B0; }",
);

/// The current page's row (after [`NAV_ROW`]).
const NAV_ROW_CURRENT: &str = themed!(
    "font-weight: 600;",
    "color: #1E1E1E; background: linear-gradient(to bottom, #DDEBFD, #C1DCFC); \
     box-shadow: inset 0px 0px 0px 1px #84ACDD; :hover { background: #CFE3FC; }",
    "color: #FFFFFF; background: linear-gradient(to bottom, #37577F, #2E4A6E); \
     box-shadow: inset 0px 0px 0px 1px #4A78B0; :hover { background: #335277; }",
    "color: var(--az-on-accent, #F4F2EA); background: var(--az-accent, #4A5C6B); \
     box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.16), 0px 1px 2px rgba(48, 45, 38, 0.30); \
     :hover { background: var(--az-accent, #4A5C6B); }",
    "box-shadow: inset 0px 1px 0px rgba(255, 255, 255, 0.10), 0px 1px 3px rgba(0, 0, 0, 0.60);",
);

/// A row's icon.
const NAV_ICON: &str = "font-size: 17px; width: 20px; flex-shrink: 0; margin-right: 8px;";

/// A row's label.
const NAV_LABEL: &str =
    "flex-grow: 1; min-width: 0px; overflow: hidden; white-space: nowrap; text-overflow: ellipsis;";

/// The local environment's status light in the sidebar's head.
const LIGHT: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0; margin-top: 8px; padding: 6px 8px; \
     border-radius: 5px; font-size: 12px;",
    "background: #F2F5F9; border: 1px solid #C2C9D2;",
    "background: #2B2E33; border: 1px solid #4A4D52;",
    "background: #F6F5F1; border: 1px solid #C6C3BB;",
    "background: #222222; border: 1px solid #3F3F3F;",
);

/// The light's row: the dot and its words (a click opens the Local environment page).
const LIGHT_ROW: &str = themed!(
    "gap: 6px; cursor: pointer; border-radius: 4px;",
    ":focus { box-shadow: inset 0px 0px 0px 2px #4286F4; }",
    ":focus { box-shadow: inset 0px 0px 0px 2px #4683D6; }",
    ":focus { box-shadow: inset 0px 0px 0px 2px #8AA0B0; }",
    ":focus { box-shadow: inset 0px 0px 0px 2px #8AA0B0; }",
);

/// The light's dot (after a tone).
const LED: &str =
    "width: 10px; height: 10px; border-radius: 5px; flex-shrink: 0; padding: 0px;";

/// The light's words.
const LIGHT_TEXT: &str =
    "flex-grow: 1; min-width: 0px; font-weight: 600; white-space: nowrap; overflow: hidden; \
     text-overflow: ellipsis;";

/// The page the cards lie on: Office's silver, flora's linen; it scrolls.
const PAGE: &str = themed!(
    "display: flex; flex-direction: column; flex-grow: 1; width: 100%; height: 100%; \
     min-width: 0px; min-height: 0px; box-sizing: border-box; padding: 12px 16px; \
     overflow-y: auto; font-size: 13px;",
    "background: #DCE2E9; color: #1E1E1E;",
    "background: #191919; color: #F2F2F2;",
    "background: #E6E4DF; background: #E6E4DF, \
     repeating-linear-gradient(0deg, rgba(90, 86, 74, 0.031) 0px, rgba(90, 86, 74, 0.031) 1px, \
     rgba(90, 86, 74, 0) 1px, rgba(90, 86, 74, 0) 3px), \
     repeating-linear-gradient(90deg, rgba(90, 86, 74, 0.022) 0px, rgba(90, 86, 74, 0.022) 1px, \
     rgba(90, 86, 74, 0) 1px, rgba(90, 86, 74, 0) 3px); color: #262521;",
    "background: #151515; color: #E7E7E7;",
);

/// The page's head: its title, when the data came, its buttons.
const PAGE_HEAD: &str =
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
     padding: 0px 0px 10px 0px; flex-wrap: wrap;";

/// The page's title.
const PAGE_TITLE: &str = themed!(
    "font-size: 20px; font-weight: 600; margin: 0px 12px 0px 0px;",
    "color: #1E3287;",
    "color: #DCE3F2;",
    "font-family: EB Garamond, Georgia, Times New Roman, serif; font-weight: 700; \
     color: #262521;",
    "font-family: EB Garamond, Georgia, Times New Roman, serif; font-weight: 700; \
     color: #E7E7E7;",
);

/// Secondary text: when the data came, a caption.
const QUIET: &str = themed!(
    "font-size: 12px;",
    "color: #5A6878;",
    "color: #9AA6B5;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// A card: a leaf of paper in a thin rule with a soft cast.
const CARD: &str = themed!(
    "display: flex; flex-direction: column; flex-shrink: 0; min-width: 0px; \
     margin: 0px 0px 12px 0px; border-radius: 3px; overflow: hidden;",
    "background: #FFFFFF; border: 1px solid #B8C2CE; \
     box-shadow: 0px 1px 3px rgba(40, 60, 90, 0.14);",
    "background: #262626; border: 1px solid #4A4A4A; box-shadow: 0px 1px 3px rgba(0, 0, 0, 0.50);",
    "background: #FBFAF6; border: 1px solid #C6C3BB; border-radius: 5px; \
     box-shadow: 0px 1px 2px rgba(48, 45, 38, 0.14), 0px 4px 12px rgba(48, 45, 38, 0.12);",
    "background: #1D1D1D; border: 1px solid #3F3F3F; \
     box-shadow: 0px 1px 2px rgba(0, 0, 0, 0.55), 0px 4px 12px rgba(0, 0, 0, 0.45);",
);

/// A card's title band.
const CARD_HEAD: &str = themed!(
    "display: flex; flex-direction: row; align-items: center; flex-shrink: 0; \
     padding: 6px 12px; font-size: 12px; font-weight: 700;",
    "background: linear-gradient(to bottom, #FFFFFF, #EDF1F5); border-bottom: 1px solid #C2C9D2; \
     color: #4C607A;",
    "background: linear-gradient(to bottom, #474747, #3A3A3A); border-bottom: 1px solid #1E1E1E; \
     color: #D0D0D0;",
    "background: linear-gradient(to bottom, #FAF9F5, #ECEAE4); border-bottom: 1px solid #C6C3BB; \
     color: #4E4C45; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     text-transform: uppercase; letter-spacing: 1px;",
    "background: linear-gradient(to bottom, #333333, #292929); border-bottom: 1px solid #101010; \
     color: #BCBCBC; font-family: EB Garamond, Georgia, Times New Roman, serif; \
     text-transform: uppercase; letter-spacing: 1px;",
);

/// A card's body.
const CARD_BODY: &str =
    "display: flex; flex-direction: column; padding: 10px 12px; min-width: 0px;";

/// A label and its value.
const FACT_ROW: &str =
    "display: flex; flex-direction: row; align-items: baseline; padding: 2px 0px; \
     min-width: 0px;";

/// A fact's label.
const FACT_LABEL: &str = themed!(
    "width: 160px; flex-shrink: 0; padding-right: 8px; font-size: 12px;",
    "color: #5A6878;",
    "color: #9AA6B5;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

/// A fact's value.
const FACT_VALUE: &str =
    "flex-grow: 1; min-width: 0px; font-size: 13px; overflow-wrap: anywhere;";

/// A badge's base: a small rounded label.
const BADGE: &str =
    "flex-shrink: 0; padding: 1px 7px; border-radius: 9px; font-size: 11px; font-weight: 700; \
     line-height: 16px; white-space: nowrap;";

/// Good news.
const TONE_OK: &str = themed!(
    "",
    "color: #FFFFFF; background: #3A7D44;",
    "color: #FFFFFF; background: #3E8B4A;",
    "color: #FBFAF6; background: #44684F;",
    "color: #FBFAF6; background: #4E7A5B;",
);

/// A form's row: its fields side by side.
const FORM_ROW: &str =
    "display: flex; flex-direction: row; flex-wrap: wrap; align-items: flex-end; gap: 8px; \
     flex-shrink: 0; padding: 4px 0px;";

/// A field: its label over its input.
const FIELD: &str = "display: flex; flex-direction: column; min-width: 0px;";

/// A field's label.
const FIELD_LABEL: &str = themed!(
    "font-size: 11px; padding-bottom: 2px;",
    "color: #5A6878;",
    "color: #9AA6B5;",
    "color: #66645C;",
    "color: #A8A8A8;",
);

// ==== AzCtl's pieces (`apps/azctl/src/ui.rs` before d658e9a) ====

fn id(s: &str) -> AzString {
    AzString::from(s)
}

/// A run of text.
fn text(s: &str) -> Dom {
    Dom::create_span_with_text(s)
}

/// A block of text with `css` (a `<p>` without the UA margin).
fn para(s: &str, css: &str) -> Dom {
    Dom::create_p_with_text(s).with_css(&format!("margin: 0px; {css}"))
}

/// A `<div>` with `css`.
fn div(css: &str) -> Dom {
    Dom::create_div().with_css(css)
}

/// The children side by side, centred on the cross axis, plus `css`.
fn hrow(css: &str, children: Vec<Dom>) -> Dom {
    let mut row = div(&format!(
        "display: flex; flex-direction: row; align-items: center; {css}"
    ));
    for c in children {
        row.add_child(c);
    }
    row
}

/// The children one under the other, plus `css`.
fn vcol(css: &str, children: Vec<Dom>) -> Dom {
    let mut col = div(&format!("display: flex; flex-direction: column; {css}"));
    for c in children {
        col.add_child(c);
    }
    col
}

/// The room between a row's left and right parts.
fn spacer() -> Dom {
    div("flex-grow: 1;")
}

/// A small rounded label in the good tone.
fn badge(label: &str) -> Dom {
    text(label).with_css(&format!("{BADGE} {TONE_OK}"))
}

/// A line of quiet text.
fn quiet(s: &str) -> Dom {
    para(s, QUIET)
}

/// A card: a title band over its body (`ui::card`; the ids are the test's).
fn card(title: &str, stem: &str, body: Dom) -> Dom {
    div(CARD)
        .with_id(id(&format!("{stem}-card")))
        .with_child(div(CARD_HEAD).with_child(text(title)))
        .with_child(
            div(CARD_BODY)
                .with_id(id(&format!("{stem}-card-body")))
                .with_child(body),
        )
}

/// Labels and their values (`ui::facts`; the ids are the test's).
fn facts(stem: &str, rows: &[(&str, &str)]) -> Dom {
    let mut col = vcol("min-width: 0px;", Vec::new()).with_id(id(&format!("{stem}-facts")));
    for (i, (label, value)) in rows.iter().enumerate() {
        col.add_child(
            div(FACT_ROW)
                .with_id(id(&format!("{stem}-row-{i}")))
                .with_child(para(label, FACT_LABEL).with_id(id(&format!("{stem}-label-{i}"))))
                .with_child(para(value, FACT_VALUE).with_id(id(&format!("{stem}-value-{i}")))),
        );
    }
    col
}

fn button(label: &str) -> Dom {
    Button::create(AzString::from(label)).dom()
}

fn primary(label: &str) -> Dom {
    Button::create(AzString::from(label))
        .with_button_type(ButtonType::Primary)
        .dom()
}

/// A button that cannot act now, and why.
fn unable(label: &str, why: &str) -> Dom {
    Button::create(AzString::from(label))
        .with_disabled(AzString::from(why))
        .dom()
}

/// An on / off button.
fn toggle(label: &str, on: bool) -> Dom {
    Button::create(AzString::from(label)).with_toggled(on).dom()
}

/// A field: its label over its text input, `width` px wide.
fn field(label: &str, placeholder: &str, width: f32) -> Dom {
    div(FIELD).with_child(para(label, FIELD_LABEL)).with_child(
        TextInput::create()
            .with_text(AzString::from(""))
            .with_placeholder(AzString::from(placeholder))
            .dom()
            .with_css(&format!("width: {width}px;")),
    )
}

/// A row of fields and buttons.
fn form_row(children: Vec<Dom>) -> Dom {
    let mut row = div(FORM_ROW);
    for c in children {
        row.add_child(c);
    }
    row
}

/// A segmented control over `labels`.
fn segmented(labels: &[&str], selected: usize) -> Dom {
    Segmented::create(StringVec::from_vec(
        labels.iter().map(|l| AzString::from(*l)).collect(),
    ))
    .with_selected_index(selected)
    .dom()
}

// ==== AzCtl's Local environment page (`apps/azctl/src/view.rs` before d658e9a) ====

/// The state, Create and Teardown (`view.rs::local_state`, the environment running).
fn local_state() -> Dom {
    let mut body = vcol("min-width: 0px;", Vec::new());
    body.add_child(hrow(
        "gap: 8px; padding-bottom: 6px;",
        vec![badge("running"), text("Running, 3 of 3 nodes")],
    ));
    body.add_child(facts("state", state_facts()));
    body.add_child(form_row(vec![
        div(FIELD)
            .with_child(para("Nodes run as", FIELD_LABEL))
            .with_child(segmented(&["Processes", "VMs"], 0)),
        field("Nodes", "3", 60.0),
        field("Layout (k+m)", "2+1", 80.0),
        unable("Create", "the local environment runs or starts already"),
    ]));
    body.add_child(quiet(
        "Processes start in seconds; VMs boot the node image in QEMU and take minutes. What it \
         does shows above while it runs.",
    ));
    body.add_child(form_row(vec![toggle("Wipe data", false), button("Teardown")]));
    body.add_child(quiet(
        "Teardown stops every process; the data stays until the next start. With \"Wipe data\" \
         the database, the nodes' data and the test drives go too, after you confirm.",
    ));
    card("Local environment", "state", body)
}

/// What the apps on this computer use (`view.rs::local_apps`, the environment running).
fn local_apps() -> Dom {
    let mut body = vcol("min-width: 0px;", Vec::new()).with_id(id("local-apps"));
    body.add_child(facts("apps", &APPS_FACTS));
    body.add_child(form_row(vec![
        primary("Use this environment in my apps"),
        button("Open AzDrive"),
    ]));
    body.add_child(quiet(
        "\"Use this environment in my apps\" points AzCtl and every Azlin app on this computer \
         (AzDrive, AzMail, ...) at the local environment: the shared Azlin config's endpoints, after \
         a plan you confirm, with a copy of the file kept; the apps take it at their next start. \
         \"Back to default\" puts the copy back. \"Open AzDrive\" starts the azul checkout's \
         AzDrive on the local environment with a data folder of its own.",
    ));
    card("Use this environment in my apps", "apps", body)
}

/// The Local environment page's column (`view.rs::local_page`; the processes and keys cards
/// between the two are left out).
fn local_page() -> Dom {
    vcol("min-width: 0px;", vec![local_state(), local_apps()]).with_id(id("local"))
}

/// The page pane: its head over the Local environment page (`view.rs::page`).
fn page() -> Dom {
    let head = hrow(
        PAGE_HEAD,
        vec![
            para("Local environment", PAGE_TITLE),
            quiet(&format!("Fetched {} s ago", 2 + 5 * POLL.with(std::cell::Cell::get))),
            spacer(),
            button("Refresh"),
        ],
    );
    div(PAGE)
        .with_id(id("page"))
        .with_child(head)
        .with_child(local_page())
}

/// The sidebar (`ui::sidebar`): AzCtl, the environment, the light, the pages.
fn sidebar() -> Dom {
    let light = vcol(
        LIGHT,
        vec![
            hrow(
                LIGHT_ROW,
                vec![
                    div(&format!("{LED} {TONE_OK}")),
                    div(LIGHT_TEXT).with_child(text("Running, 3 of 3 nodes")),
                ],
            ),
            hrow(
                "gap: 4px; flex-wrap: wrap; padding-top: 4px;",
                vec![button("Stop"), button("Use local everywhere")],
            ),
        ],
    );
    let head = vcol(
        SIDEBAR_HEAD,
        vec![
            para("AzCtl", SIDEBAR_TITLE),
            hrow("", vec![text("Environment dev")]),
            quiet("drives azctl"),
            light,
        ],
    );
    let mut list = div(SIDEBAR_LIST);
    for title in [
        "Overview",
        "Connections",
        "Capacity & orders",
        "Logs",
        "Disks",
        "Network",
        "Self-test",
        "Keys",
        "Customers",
        "Releases",
        "Local environment",
        "Settings",
    ] {
        let css = if title == "Local environment" {
            format!("{NAV_ROW} {NAV_ROW_CURRENT}")
        } else {
            NAV_ROW.to_string()
        };
        list.add_child(
            div(&css)
                .with_child(div(NAV_ICON).with_child(text("o")))
                .with_child(div(NAV_LABEL).with_child(text(title))),
        );
    }
    div(SIDEBAR).with_child(head).with_child(list)
}

/// The window (`lib.rs::layout`): the theme scope as the body, around a column around the
/// shell (sidebar | page, a title row, the status bar).
fn window() -> Dom {
    let title_row = div("height: 30px; flex-shrink: 0;")
        .with_child(text("AzCtl - Local environment (dev)"));
    let status = div("height: 22px; flex-shrink: 0; font-size: 12px;")
        .with_child(text("dev - drives azctl - idle"));
    let shell = MediaShell::create(sidebar(), page(), status).with_sidebar_ratio(SIDEBAR_RATIO);
    let column = div("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell.office_shell().with_title_row(title_row).dom());
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Slate)
        .body()
}

// ==== Laying out and reading back ====

/// A window in the app theme flora, as AzCtl pins it.
fn new_window() -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    lw.app_theme = AzString::from("flora");
    lw
}

/// One frame of `lw`: `build`'s DOM (built under the app theme flora) laid out at `width` x
/// `WINDOW_H` - the first frame, a rebuild of the same DOM (AzCtl's poll), or a resize.
fn frame(lw: &mut LayoutWindow, build: impl FnOnce() -> Dom, width: f32) {
    let dom = {
        let _scope = ThemeScope::enter(AzString::from("flora"));
        build()
    };
    let styled = StyledDom::create_from_dom(dom);
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, WINDOW_H);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .expect("the window lays out");
}

/// The DOM node with the id `name`, if any.
fn find_id(lw: &LayoutWindow, name: &str) -> Option<NodeId> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(|n| n.has_id(name))
        .map(NodeId::new)
}

fn node_of_id(lw: &LayoutWindow, name: &str) -> NodeId {
    find_id(lw, name).unwrap_or_else(|| panic!("no node #{name}"))
}

fn rect_of_node(lw: &LayoutWindow, node: NodeId) -> Option<LogicalRect> {
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
}

fn rect_of_id(lw: &LayoutWindow, name: &str) -> LogicalRect {
    rect_of_node(lw, node_of_id(lw, name)).unwrap_or_else(|| panic!("#{name} has no layout rect"))
}

/// The pen positions (x, baseline y) of every glyph painted for the text inside the DOM node
/// `p`: the text runs whose source layout node is `p` or a node inside it.
fn glyphs_of(lw: &LayoutWindow, p: NodeId) -> Vec<(f32, f32)> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let hierarchy = result.styled_dom.node_hierarchy.as_container();
    let mut out = Vec::new();
    for item in result.display_list.items.iter() {
        if let DisplayListItem::Text {
            glyphs,
            source_node_index: Some(src),
            ..
        } = item
        {
            let Some(mut node) = result.layout_tree.nodes.get(*src).and_then(|n| n.dom_node_id)
            else {
                continue;
            };
            let mut inside = node == p;
            while !inside {
                match hierarchy[node].parent_id() {
                    Some(up) => {
                        node = up;
                        inside = node == p;
                    }
                    None => break,
                }
            }
            if inside {
                out.extend(glyphs.iter().map(|g| (g.point.x, g.point.y)));
            }
        }
    }
    out
}

/// The distinct baselines of `glyphs` (half a pixel apart at least), top to bottom.
fn baselines(glyphs: &[(f32, f32)]) -> Vec<f32> {
    let mut ys: Vec<f32> = Vec::new();
    for (_, y) in glyphs {
        if !ys.iter().any(|b| (b - y).abs() < 0.5) {
            ys.push(*y);
        }
    }
    ys.sort_by(f32::total_cmp);
    ys
}

/// Prints the border box of every node in `names` that is there, so a failing run shows the
/// whole chain from the window down to the rows.
fn print_chain(case: &str, lw: &LayoutWindow, names: &[String]) {
    for name in names {
        match find_id(lw, name).and_then(|n| rect_of_node(lw, n)) {
            Some(r) => println!("{case}: #{name} {r:?}"),
            None => println!("{case}: #{name} not laid out"),
        }
    }
}

/// The ids of a card's chain: the card, its body, its facts column, every row, label, value.
fn card_chain(stem: &str, rows: usize) -> Vec<String> {
    let mut names = vec![
        format!("{stem}-card"),
        format!("{stem}-card-body"),
        format!("{stem}-facts"),
    ];
    for i in 0..rows {
        names.push(format!("{stem}-row-{i}"));
        names.push(format!("{stem}-label-{i}"));
        names.push(format!("{stem}-value-{i}"));
    }
    names
}

/// Every facts row of the `stem` card is at least `min_row_w` wide (its card body's width),
/// its label's box is the CSS width (168 px) and its value takes the rest; the label's and the
/// value's text lie inside their boxes on no more lines than they have words - one line here.
fn assert_facts_keep_their_words(
    case: &str,
    lw: &LayoutWindow,
    stem: &str,
    rows: &[(&str, &str)],
    min_row_w: f32,
) {
    let card = rect_of_id(lw, &format!("{stem}-card"));
    let body = rect_of_id(lw, &format!("{stem}-card-body"));
    let column = rect_of_id(lw, &format!("{stem}-facts"));
    for (i, (label, value)) in rows.iter().enumerate() {
        let row = rect_of_id(lw, &format!("{stem}-row-{i}"));
        let label_node = node_of_id(lw, &format!("{stem}-label-{i}"));
        let value_node = node_of_id(lw, &format!("{stem}-value-{i}"));
        let l = rect_of_node(lw, label_node).expect("the label is laid out");
        let v = rect_of_node(lw, value_node).expect("the value is laid out");
        let what = format!(
            "{case}: the {stem} card's fact {i} ({label:?}: {value:?}) - card {card:?}, card \
             body {body:?}, facts column {column:?}, row {row:?}, label {l:?}, value {v:?}"
        );
        assert!(
            row.size.width >= min_row_w,
            "{what}: the row stretches to its card body's width (at least {min_row_w:.1} px), \
             it is {:.2} px",
            row.size.width
        );
        assert!(
            (l.size.width - LABEL_W).abs() < 0.5,
            "{what}: the label's box is its CSS width, 160 px and its 8 px padding \
             (flex-shrink: 0), it is {:.2} px",
            l.size.width
        );
        let label_right = l.origin.x + l.size.width;
        let row_right = row.origin.x + row.size.width;
        assert!(
            (v.origin.x - label_right).abs() < 0.5
                && (v.origin.x + v.size.width - row_right).abs() < 0.5,
            "{what}: the value (flex-grow: 1) takes the rest of the row, {label_right:.2} to \
             {row_right:.2}"
        );
        let parts = [("label", *label, l, label_node), ("value", *value, v, value_node)];
        for (name, words_of, rect, node) in parts {
            let glyphs = glyphs_of(lw, node);
            let words = words_of.split_whitespace().count();
            let lines = baselines(&glyphs);
            let xs: Vec<f32> = glyphs.iter().map(|g| g.0).collect();
            assert!(!glyphs.is_empty(), "{what}: the {name} paints its text");
            assert!(
                lines.len() <= words,
                "{what}: the {name} {words_of:?} ({words} words) breaks only between words, \
                 it has {} lines - baselines {lines:?}, glyph x {xs:?}",
                lines.len()
            );
            assert_eq!(
                lines.len(),
                1,
                "{what}: the {name} {words_of:?} fits its box on one line - baselines \
                 {lines:?}, glyph x {xs:?}"
            );
            let first = xs.iter().copied().fold(f32::INFINITY, f32::min);
            let last = xs.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            assert!(
                first >= rect.origin.x - 0.5 && last <= rect.origin.x + rect.size.width + 0.5,
                "{what}: the {name}'s glyphs start inside its box, x {first:.2} to {last:.2}"
            );
        }
    }
}

/// The whole window's facts rows (`case` names the frame), the chain printed first.
fn check_window(case: &str, lw: &LayoutWindow) {
    let mut names: Vec<String> = ["shell-sidebar", "shell-content", "page", "local"]
        .iter()
        .map(|n| n.to_string())
        .collect();
    names.extend(card_chain("state", state_facts().len()));
    names.extend(card_chain("apps", APPS_FACTS.len()));
    print_chain(case, lw, &names);

    // a classic scrollbar on the page takes at most 16 px of the rows' width
    let min_row_w = PAGE_CONTENT_W - CARD_INSET - 16.0;
    assert_facts_keep_their_words(case, lw, "state", state_facts(), min_row_w);
    assert_facts_keep_their_words(case, lw, "apps", &APPS_FACTS, min_row_w);

    let sidebar = rect_of_id(lw, "shell-sidebar");
    let content = rect_of_id(lw, "shell-content");
    let share = WINDOW_W - DIVIDER;
    assert!(
        (sidebar.size.width - share * SIDEBAR_RATIO).abs() < 1.0
            && (content.size.width - share * (1.0 - SIDEBAR_RATIO)).abs() < 1.0,
        "{case}: the panes share the {share} px the divider leaves 0.2 : 0.8 - sidebar \
         {sidebar:?}, content {content:?}"
    );
}

/// Shows poll `poll` from the next frame on.
fn set_poll(poll: usize) {
    POLL.with(|p| p.set(poll));
}

/// The window's first frame, a rebuild of the same page, the next poll (AzCtl rebuilds its
/// DOM when a poll brings news: the "Since" value and the page head's "Fetched" words change,
/// the rest of the page stays as it was laid out) and a resize to 1200 px and back: every
/// frame keeps the words.
#[test]
fn a_facts_row_on_azctls_local_page_keeps_its_words() {
    let mut lw = new_window();
    set_poll(0);
    frame(&mut lw, window, WINDOW_W);
    check_window("in the window's first frame", &lw);
    frame(&mut lw, window, WINDOW_W);
    check_window("in the window rebuilt", &lw);
    set_poll(1);
    frame(&mut lw, window, WINDOW_W);
    check_window("in the window at the next poll", &lw);
    frame(&mut lw, window, 1200.0);
    frame(&mut lw, window, WINDOW_W);
    check_window("in the window resized to 1200 px and back", &lw);
}

/// The two cards in a plain column as wide as the page's content box.
fn plain_column() -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        div(&format!(
            "display: flex; flex-direction: column; width: {PAGE_CONTENT_W}px; font-size: 13px;"
        ))
        .with_child(local_page()),
    )
}

fn check_plain_column(case: &str, lw: &LayoutWindow) {
    let mut names: Vec<String> = vec!["local".to_string()];
    names.extend(card_chain("state", state_facts().len()));
    names.extend(card_chain("apps", APPS_FACTS.len()));
    print_chain(case, lw, &names);

    let min_row_w = PAGE_CONTENT_W - CARD_INSET - 0.5;
    assert_facts_keep_their_words(case, lw, "state", state_facts(), min_row_w);
    assert_facts_keep_their_words(case, lw, "apps", &APPS_FACTS, min_row_w);
}

#[test]
fn a_facts_row_in_a_card_in_a_plain_column_keeps_its_words() {
    let mut lw = new_window();
    set_poll(0);
    frame(&mut lw, plain_column, WINDOW_W);
    check_plain_column("in a plain column", &lw);
    frame(&mut lw, plain_column, WINDOW_W);
    check_plain_column("in a plain column rebuilt", &lw);
    set_poll(1);
    frame(&mut lw, plain_column, WINDOW_W);
    check_plain_column("in a plain column at the next poll", &lw);
}
