//! The "Tab looks" section: one set of tabs in every look a tab bar has.
//!
//! The same three tabs are drawn as Aurora (flora's curved tabs), as
//! Rectangular (classic square tabs) and as Pills (the theme's buttons on a
//! leather strip that closes on a golden line), all on ONE selection: a click
//! in any of them selects that tab in all three. Under them one panel shows
//! the selected tab's text and slides in from the side of the switch
//! (`TabContent::with_switch`). Last, a small ribbon on the leather, its tabs
//! as buttons, like AzDrive's title bar.

use azul::{
    callbacks::{RibbonOnTabClickCallbackType, TabOnClickCallbackType},
    prelude::*,
    widgets::*,
};

use crate::{captioned, keep, section, strs};

/// The tabs every header shows.
const TABS: [&str; 3] = ["Home", "Share", "View"];

/// What the panel says for each tab.
const PANELS: [&str; 3] = [
    "Home: the commands you use most.",
    "Share: send a link, invite people.",
    "View: panes, layout, sorting.",
];

/// The section's state. Lives in `Showcase::tab_looks`.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TabLooksDemo {
    /// The selected tab of the three headers.
    active: usize,
    /// The tab selected before the last switch: the panel slides in from
    /// its side.
    previous: usize,
    /// The ribbon's selected tab.
    ribbon_tab: usize,
}

/// One header of the three tabs in `style`, on the shared selection.
fn header(data: &RefAny, demo: &TabLooksDemo, style: TabStyle, theme: UiTheme) -> Dom {
    TabHeader::create(strs(&TABS))
        .with_active_tab(demo.active)
        .with_style(style)
        .with_theme(theme)
        .with_on_click(data.clone(), on_look_tab as TabOnClickCallbackType)
        .dom()
}

/// A ribbon button that does nothing but look like one.
fn command(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::LargeButton(RibbonButton::create(icon, label))
}

/// A small ribbon on the leather: its tabs are buttons (`TabStyle::Pills`).
fn leather_ribbon(data: &RefAny, demo: &TabLooksDemo, theme: UiTheme) -> Dom {
    let start = RibbonTab::create("Start")
        .with_group(
            RibbonGroup::create("Clipboard")
                .with_item(command("content_paste", "Paste"))
                .with_item(command("content_copy", "Copy")),
        )
        .with_group(
            RibbonGroup::create("Organize")
                .with_item(command("drive_file_move", "Move to"))
                .with_item(command("delete", "Delete")),
        );
    let layout = RibbonTab::create("Layout").with_group(
        RibbonGroup::create("Page")
            .with_item(command("margin", "Margins"))
            .with_item(command("crop_portrait", "Orientation")),
    );
    Ribbon::create(vec![start, layout])
        .with_active_tab(demo.ribbon_tab)
        .with_tab_style(TabStyle::Pills)
        .with_theme(theme)
        .with_on_tab_click(data.clone(), on_look_ribbon_tab as RibbonOnTabClickCallbackType)
        .dom_desktop()
}

/// The section's card.
pub(crate) fn tab_looks_section(data: &RefAny, demo: &TabLooksDemo, theme: UiTheme) -> Dom {
    let panel = TabContent::create(
        Dom::create_p_with_text(PANELS[demo.active.min(PANELS.len() - 1)])
            .with_css("margin: 0px; color: system:text;"),
    )
    .with_switch(demo.previous, demo.active)
    .with_theme(theme)
    .dom();
    section(
        "Tab looks",
        vec![
            captioned("Aurora - flora's curved tabs", header(data, demo, TabStyle::Aurora, theme)),
            captioned(
                "Rectangular - square tabs",
                header(data, demo, TabStyle::Rectangular, theme),
            ),
            captioned(
                "Pills - buttons on the leather",
                header(data, demo, TabStyle::Pills, theme),
            ),
            captioned("The panel slides in from the side of the switch", panel),
            captioned("A ribbon on the leather", leather_ribbon(data, demo, theme)),
        ],
    )
}

/// A tab of any of the three headers: the selection moves, and the panel
/// remembers where it came from.
extern "C" fn on_look_tab(mut data: RefAny, _info: CallbackInfo, state: TabHeaderState) -> Update {
    keep(&mut data, |s| {
        let demo = &mut s.tab_looks;
        if demo.active != state.active_tab {
            demo.previous = demo.active;
            demo.active = state.active_tab;
        }
    })
}

/// A tab of the ribbon.
extern "C" fn on_look_ribbon_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    keep(&mut data, |s| s.tab_looks.ribbon_tab = index)
}
