//! The look of the settings page (feature `look`): Outlook 2010's Options dialog in pieces -
//! the category list on the left, the header line, the sections (a band, the rows indented
//! under it), OK / Cancel under both. The kit's page (`ui::settings_page`) is built from them,
//! and so is the settings screen of an app that cannot take the kit's `azul` feature (AzMeet
//! links azul statically on android / ios; `look` names no link mode).
//!
//! ```text
//! +--------------+---------------------------------------------------+
//! | Devices      | (icon) General options for working with AzMeet.   |  header_line
//! |--------------|                                                   |
//! |##General#####| Appearance                                        |  section: band
//! | Shortcuts    |    Theme      [ Flat | Flora ]                    |           rows
//! |--------------|                                                   |
//! | About        |                                                   |
//! +--------------+---------------------------------------------------+
//!                                                      [ OK ] [ Cancel ]  dialog_buttons
//! ```
//!
//! The colours are the theme's (`system:` colours), named here so a palette - Office 2010's
//! silver and orange, or any other - can be swapped in without touching a page.

use azul::{
    callbacks::{ButtonOnClickCallbackType, CallbackType},
    dom::TabIndex,
    prelude::*,
    widgets::ButtonType,
};

use crate::pieces::{column, text};

/// The dialog's own surface: around the two panes and under OK / Cancel.
pub const DIALOG_BG: &str = "system:window-background";
/// The category list's and the options pane's surface (Outlook's white panes).
pub const PANE_BG: &str = "system:control-background";
/// The frame of the two panes, and the rules between the groups of categories.
pub const PANE_EDGE: &str = "system:separator";
/// The selected category (Outlook 2010: the orange bar).
pub const SELECTED_BG: &str = "system:selection-background";
/// The selected category's text.
pub const SELECTED_TEXT: &str = "system:selection-text";
/// A category under the pointer.
pub const HOVER_BG: &str = "system:selection-background-inactive";
/// A section's band (Outlook's grey "User Interface options" bar over the rows).
pub const BAND_BG: &str = "system:window-background";
/// The header line's text ("General options for working with AzNotes.") and a row's note.
pub const QUIET_TEXT: &str = "system:secondary-text";
/// The header line's icon.
pub const HEADER_ICON: &str = "system:accent";
/// The width of the category list (it gives way down to 100 px in a narrow window).
pub const CATEGORY_WIDTH: &str = "180px";
/// The width of a row's label column (it gives way down to 72 px).
pub const LABEL_WIDTH: &str = "160px";

/// A settings row: a label column and the control (Outlook's "Color scheme: [Silver]").
#[must_use]
pub fn row(label: &str, control: Dom) -> Dom {
    Dom::create_div()
        .with_class("appkit-row")
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 4px 0px; \
             min-width: 0px;",
        )
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "width: {LABEL_WIDTH}; flex-shrink: 1; min-width: 72px; padding-right: 8px; \
                     font-size: 13px;"
                ))
                .with_child(text(label)),
        )
        .with_child(control)
}

/// A line of secondary text under a section's rows.
#[must_use]
pub fn note(content: &str) -> Dom {
    Dom::create_div()
        .with_css(format!("padding: 4px 0px; font-size: 12px; color: {QUIET_TEXT};"))
        .with_child(text(content))
}

/// A section's band: its title in bold on Outlook's grey bar.
#[must_use]
pub fn band(title: &str) -> Dom {
    Dom::create_div()
        .with_class("appkit-band")
        .with_css(format!(
            "padding: 4px 8px; margin-top: 12px; background: {BAND_BG}; font-size: 13px; \
             font-weight: bold;"
        ))
        .with_child(text(title))
}

/// A section: its band over its rows, the rows indented under it.
#[must_use]
pub fn section(title: &str, content: Dom) -> Dom {
    column(
        "flex-shrink: 0; min-width: 0px;",
        vec![
            band(title),
            Dom::create_div()
                .with_css("padding: 6px 4px 4px 20px; min-width: 0px;")
                .with_child(content),
        ],
    )
}

/// The line over a category's sections: a big icon (a name of azul's icon set) and what the
/// category is for ("General options for working with AzNotes.").
#[must_use]
pub fn header_line(id: &str, icon: &str, line: &str) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; padding: 2px 0px 6px \
             0px; flex-shrink: 0;",
        )
        .with_child(Dom::create_icon(icon).with_css(format!(
            "font-size: 32px; color: {HEADER_ICON}; margin-right: 12px; flex-shrink: 0;"
        )))
        .with_child(
            Dom::create_div()
                .with_css(format!(
                    "flex-grow: 1; min-width: 0px; font-size: 15px; color: {QUIET_TEXT};"
                ))
                .with_child(text(line)),
        )
}

/// One category of the list.
pub struct CategoryItem<'a> {
    /// Its name in the list.
    pub name: &'a str,
    /// Its DOM id, for scripts.
    pub id: String,
    /// A rule above it: it starts a group (Outlook's "Customize Ribbon", "Add-Ins").
    pub rule_before: bool,
}

/// The category list: `items` top to bottom, the `chosen` one on the selection's colour. A
/// click on an item - or Enter / Space on it focused - calls `on_pick` with `data(index)`.
#[must_use]
pub fn category_list(
    id: &str,
    items: &[CategoryItem<'_>],
    chosen: usize,
    data: &dyn Fn(usize) -> RefAny,
    on_pick: CallbackType,
) -> Dom {
    let item_css = "padding: 6px 12px; margin: 1px 4px; font-size: 13px; border-radius: 2px; \
                    cursor: pointer; flex-shrink: 0;";
    let mut children = Vec::with_capacity(items.len() + 2);
    for (index, item) in items.iter().enumerate() {
        if item.rule_before {
            children.push(Dom::create_div().with_css(format!(
                "height: 1px; margin: 5px 8px; background: {PANE_EDGE}; flex-shrink: 0;"
            )));
        }
        let css = if index == chosen {
            format!("{item_css} background: {SELECTED_BG}; color: {SELECTED_TEXT};")
        } else {
            format!("{item_css} color: system:text; :hover {{ background: {HOVER_BG}; }}")
        };
        let pick = data(index);
        children.push(
            Dom::create_div()
                .with_id(item.id.as_str())
                .with_css(css)
                .with_tab_index(TabIndex::Auto)
                .with_child(text(item.name))
                .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), pick.clone(), on_pick)
                // Enter / Space on the focused category (the engine's keyboard activation).
                .with_callback(EventFilter::Hover(HoverEventFilter::Click), pick, on_pick),
        );
    }
    column(
        &format!(
            "width: {CATEGORY_WIDTH}; flex-shrink: 1; min-width: 100px; min-height: 0px; \
             padding: 4px 0px; background: {PANE_BG}; border: 1px solid {PANE_EDGE}; \
             overflow-y: auto; overflow-x: hidden;"
        ),
        children,
    )
    .with_id(id)
}

/// The options pane right of the list: it scrolls, and holds `children` in one column that
/// does not shrink - a long category scrolls instead of squeezing its rows.
#[must_use]
pub fn options_pane(id: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; flex-shrink: 1; min-width: \
             0px; min-height: 0px; margin-left: 8px; background: {PANE_BG}; border: 1px solid \
             {PANE_EDGE}; overflow-y: auto; overflow-x: hidden;"
        ))
        .with_child(column(
            "flex-shrink: 0; min-width: 0px; padding: 12px 18px 18px 18px;",
            children,
        ))
}

/// A dialog button: `label`, `id` for scripts, `on_click` called with `data`; OK is the
/// primary (default) one.
#[must_use]
pub fn dialog_button(
    label: &str,
    primary: bool,
    id: &str,
    data: &RefAny,
    on_click: ButtonOnClickCallbackType,
) -> Dom {
    let button = Button::create(label);
    let button = if primary {
        button.with_button_type(ButtonType::Primary)
    } else {
        button
    };
    // The width on a wrapper: a widget's own root keeps the properties it sets.
    Dom::create_div()
        .with_css("min-width: 88px; margin-left: 8px;")
        .with_child(button.with_on_click(data.clone(), on_click).dom().with_id(id))
}

/// The row under the panes: `notice` on the left (a problem to tell, `None` = none), then
/// the buttons (OK, Cancel) on the right.
#[must_use]
pub fn dialog_buttons(id: &str, notice: Option<Dom>, buttons: Vec<Dom>) -> Dom {
    let mut row = Dom::create_div().with_id(id).with_css(
        "display: flex; flex-direction: row; align-items: center; padding: 10px; flex-shrink: 0;",
    );
    row.add_child(match notice {
        Some(notice) => Dom::create_div()
            .with_css(format!(
                "flex-grow: 1; min-width: 0px; padding-right: 12px; font-size: 12px; color: \
                 {QUIET_TEXT};"
            ))
            .with_child(notice),
        None => Dom::create_div().with_css("flex-grow: 1;"),
    });
    for button in buttons {
        row.add_child(button);
    }
    row
}

/// The dialog: the category list beside the options pane, over the buttons, on the dialog's
/// surface; `id` on its root.
#[must_use]
pub fn dialog(id: &str, categories: Dom, pane: Dom, buttons: Dom) -> Dom {
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: \
             0px; background: {DIALOG_BG}; color: system:text;"
        ))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; \
                     min-width: 0px; padding: 10px 10px 0px 10px;",
                )
                .with_child(categories)
                .with_child(pane),
        )
        .with_child(buttons)
}
