//! The "Tab looks" section: one set of tabs in every look a tab bar has.
//!
//! The same three tabs are drawn as Auto (the theme's buttons floating on a
//! leather strip that closes on a golden line: the look of every theme on every
//! platform), as Rectangular (classic square tabs, for an app that names them)
//! and as Pills (what Auto is on a desktop), all on ONE selection: a click in
//! any of them selects that tab in all three. Under them one panel shows
//! the selected tab's text and slides in from the side of the switch
//! (`TabContent::with_switch`). Last, a small ribbon on the leather, its tabs
//! as buttons, like AzDrive's title bar.
//!
//! Then the tabs for a finger (`TabStyle::TouchPills`): twelve pills in a row that scrolls sideways
//! without a scrollbar, a panel under them that a vertical swipe flips to the next / previous tab
//! (`TabContent::with_flip_of`), and a phone to try it on: three buttons make the window behave as if
//! it sat on a Pixel 8 or an iPhone (`WindowFlags::display_preset`), hide the system bars and draw into
//! the cutout area, and the page's top becomes the tab strip IN the status bar's band, sliding around
//! the camera (`TabHeader::with_status_bar`) - drawn here as a black dot, since a desktop window has
//! none.

use azul::{
    callbacks::{RibbonOnTabClickCallbackType, TabOnClickCallbackType},
    prelude::*,
    widgets::*,
    window::{DisplayPreset, SystemBarMode},
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
    /// The selected tab of the touch strip and the one before it.
    touch_active: usize,
    touch_previous: usize,
    /// The selected tab of the desktop's row that is longer than its box.
    wide_active: usize,
    /// The phone the window pretends to sit on: 0 none, 1 Pixel 8, 2 iPhone with the Dynamic Island,
    /// 3 the phone it really runs on (its own cutout, nothing simulated).
    phone: usize,
    /// The strip in the band is a word processor's ribbon (its tabs, the groups of the active one below)
    /// and not a plain tab bar.
    ribbon_band: bool,
}

/// The tabs of the ribbon in the band: more than a phone has room for.
const RIBBON_TABS: [&str; 6] = ["Home", "Insert", "Draw", "Design", "Layout", "Review"];

/// The tabs of the touch strip: more than a phone has room for.
const TOUCH_TABS: [&str; 12] = [
    "Home",
    "Share",
    "View",
    "Insert",
    "Draw",
    "Design",
    "Layout",
    "References",
    "Mailings",
    "Review",
    "Help",
    "Account",
];

/// One header of the three tabs in `style`, on the shared selection.
fn header(data: &RefAny, demo: &TabLooksDemo, style: TabStyle, theme: UiTheme) -> Dom {
    TabHeader::create(strs(&TABS))
        .with_active_tab(demo.active)
        .with_style(style)
        .with_theme(theme)
        .with_on_click(data.clone(), on_look_tab as TabOnClickCallbackType)
        .dom()
}

/// The names of the twelve tabs of the row that is longer than its box: not the touch strip's, so that a
/// scenario that clicks a pill by its text finds the one it means.
const WIDE_TABS: [&str; 12] = [
    "Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot", "Golf", "Hotel", "India", "Juliet", "Kilo", "Lima",
];

/// The desktop's row of buttons with more tabs than its box has room for: it scrolls inside the box
/// (the leather, the golden line and the fades at its ends stay), without a scrollbar.
fn wide_pills(data: &RefAny, demo: &TabLooksDemo, theme: UiTheme) -> Dom {
    Dom::create_div().with_id("wide-pills").with_css("max-width: 360px; overflow: hidden;").with_child(
        TabHeader::create(strs(&WIDE_TABS))
            .with_active_tab(demo.wide_active)
            .with_style(TabStyle::Pills)
            .with_theme(theme)
            .with_on_click(data.clone(), on_wide_tab as TabOnClickCallbackType)
            .dom(),
    )
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
            captioned("Auto - the buttons on the leather", header(data, demo, TabStyle::Auto, theme)),
            captioned(
                "Rectangular - square tabs",
                header(data, demo, TabStyle::Rectangular, theme),
            ),
            captioned(
                "Pills - what Auto is on a desktop",
                header(data, demo, TabStyle::Pills, theme),
            ),
            captioned(
                "Pills - twelve tabs in a 360 px box: the row scrolls",
                wide_pills(data, demo, theme),
            ),
            captioned("The panel slides in from the side of the switch", panel),
            captioned("A ribbon on the leather", leather_ribbon(data, demo, theme)),
            captioned(
                "Touch pills - three tabs fit a phone",
                header(data, demo, TabStyle::TouchPills, theme),
            ),
            captioned(
                "Touch pills - 32 px in a 48 px row: drag the row, the selected pill comes into view",
                Dom::create_div()
                    .with_css("max-width: 360px; overflow: hidden;")
                    .with_child(touch_header(data, demo, theme, None)),
            ),
            captioned(
                "A vertical swipe on the panel flips to the next / previous tab",
                touch_panel(data, demo, theme),
            ),
            captioned("A phone to try it on", phone_row(data, demo, theme)),
        ],
    )
}

/// The touch strip of twelve tabs on its own selection; `band` puts it in the status bar's band.
fn touch_header(
    data: &RefAny,
    demo: &TabLooksDemo,
    theme: UiTheme,
    band: Option<StatusBarBand>,
) -> Dom {
    // The window's ONE chevron (folds the content of the tabs away): this strip draws it when it is the
    // page's strip - the band on a phone, or the section's own while the window sits on no phone (the
    // first strip of a build that asks has it, and the section is built before the band).
    let header = TabHeader::create(strs(&TOUCH_TABS))
        .with_active_tab(demo.touch_active)
        .with_style(TabStyle::TouchPills)
        .with_theme(theme)
        .with_content_toggle(band.is_some() || demo.phone == 0)
        .with_on_click(data.clone(), on_touch_tab as TabOnClickCallbackType);
    match band {
        Some(band) => header.with_status_bar(band).dom(),
        None => header.dom(),
    }
}

/// The panel under the touch strip: its text slides in, and a vertical swipe flips the tab.
fn touch_panel(data: &RefAny, demo: &TabLooksDemo, theme: UiTheme) -> Dom {
    let header = TabHeader::create(strs(&TOUCH_TABS))
        .with_active_tab(demo.touch_active)
        .with_style(TabStyle::TouchPills)
        .with_theme(theme)
        .with_on_click(data.clone(), on_touch_tab as TabOnClickCallbackType);
    let label = TOUCH_TABS[demo.touch_active.min(TOUCH_TABS.len() - 1)];
    TabContent::create(
        Dom::create_p_with_text(format!("{label} panel"))
            .with_id("touch-panel-text")
            .with_css("margin: 0px; color: system:text; min-height: 120px;"),
    )
    .with_content_toggle(true)
    .with_switch(demo.touch_previous, demo.touch_active)
    .with_theme(theme)
    .with_flip_of(&header)
    .dom()
}

/// Three buttons that put the window on a phone: none, a Pixel 8, an iPhone with the island.
fn phone_row(data: &RefAny, demo: &TabLooksDemo, theme: UiTheme) -> Dom {
    let button = |label: &str, on_click: extern "C" fn(RefAny, CallbackInfo) -> Update| {
        Button::with_type(
            label,
            if label_is_on(label, demo) {
                ButtonType::Primary
            } else {
                ButtonType::Default
            },
        )
        .with_on_click(data.clone(), on_click)
        .with_theme(theme)
        .dom()
    };
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; gap: 8px; flex-wrap: wrap;")
        .with_child(button("No phone", on_phone_off))
        .with_child(button("Pixel 8", on_phone_pixel))
        .with_child(button("iPhone 16 Pro", on_phone_island))
        .with_child(button("This phone", on_phone_real))
        .with_child(button("Ribbon in the band", on_ribbon_band))
}

/// Whether the button named `label` is the phone that is on.
fn label_is_on(label: &str, demo: &TabLooksDemo) -> bool {
    match label {
        "No phone" => demo.phone == 0,
        "Pixel 8" => demo.phone == 1,
        "iPhone 16 Pro" => demo.phone == 2,
        "This phone" => demo.phone == 3,
        _ => demo.ribbon_band,
    }
}

/// The status bar's band for the phone the window pretends to sit on, `None` when it sits on none.
pub(crate) fn phone_band(info: &LayoutCallbackInfo, demo: &TabLooksDemo) -> Option<StatusBarBand> {
    if demo.phone == 0 {
        return None;
    }
    Some(StatusBarBand::from_display(
        &info.get_display_features(),
        &info.get_safe_area_insets(),
        LogicalSize {
            width: info.get_window_width(),
            height: info.get_window_height(),
        },
    ))
}

/// The top of the window when it sits on a phone: the touch strip IN the status bar's band, around the
/// camera.
pub(crate) fn phone_strip(
    data: &RefAny,
    demo: &TabLooksDemo,
    theme: UiTheme,
    band: StatusBarBand,
) -> Dom {
    if demo.ribbon_band {
        // The ribbon over a document, as a word processor shows it.
        Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-shrink: 0;")
            .with_child(ribbon_strip(data, demo, theme, band))
            .with_child(document_sheet())
    } else {
        touch_header(data, demo, theme, Some(band))
    }
}

/// The lines of the sheet under the ribbon.
const SHEET_LINES: [&str; 5] = [
    "The tab row above takes the place of the clock and the battery and slides around the camera.",
    "Drag it sideways to reach a tab that is out of view; the selected tab comes into view by itself.",
    "A vertical swipe on the groups, or on the page under them, flips to the next or the previous tab.",
    "The sheet is what a word processor shows under its ribbon: a white page on the desk.",
    "Turn the phone and the tab row finds its lanes around the camera again.",
];

/// A document under the ribbon, the way AzWriter shows one: a white sheet on the desk with a title and a few
/// lines (the sheet is paper, white in every mode).
fn document_sheet() -> Dom {
    let mut sheet = Dom::create_div()
        .with_css(
            "box-sizing: border-box; width: 100%; max-width: 560px; min-height: 220px; padding: 20px 22px; \
             background: #ffffff; border-radius: 2px; box-shadow: 0px 1px 4px rgba(0, 0, 0, 0.3);",
        )
        .with_child(
            Dom::create_p_with_text("Notes on the tab row")
                .with_css("margin: 0px 0px 12px 0px; font-size: 22px; font-weight: bold; color: #202124;"),
        );
    for line in SHEET_LINES {
        sheet = sheet.with_child(
            Dom::create_p_with_text(line)
                .with_css("margin: 0px 0px 8px 0px; font-size: 14px; line-height: 20px; color: #202124;"),
        );
    }
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; align-items: center; box-sizing: border-box; \
             padding: 14px 12px; background-color: system:under-page-background;",
        )
        .with_child(sheet)
}

/// A word processor's ribbon in the band: its tabs are the strip (the touch pills), the groups of
/// the active tab sit below it, and a vertical swipe over them flips to the next / previous tab.
fn ribbon_strip(data: &RefAny, demo: &TabLooksDemo, theme: UiTheme, band: StatusBarBand) -> Dom {
    let tabs: Vec<RibbonTab> = RIBBON_TABS
        .iter()
        .map(|name| {
            RibbonTab::create(*name)
                .with_group(
                    RibbonGroup::create("Clipboard")
                        .with_item(command("content_paste", "Paste"))
                        .with_item(command("content_copy", "Copy")),
                )
                .with_group(
                    RibbonGroup::create("Page")
                        .with_item(command("margin", "Margins"))
                        .with_item(command("crop_portrait", "Orientation")),
                )
        })
        .collect();
    Ribbon::create(tabs)
        .with_active_tab(demo.ribbon_tab)
        .with_tab_style(TabStyle::TouchPills)
        .with_theme(theme)
        .with_status_bar(band)
        .with_on_tab_click(data.clone(), on_look_ribbon_tab as RibbonOnTabClickCallbackType)
        .dom_desktop()
}

/// The camera the simulated phone has, drawn as a black dot (a desktop window has none; on a phone
/// the hardware is there). The last child of the page, over everything.
pub(crate) fn camera_dots(info: &LayoutCallbackInfo, demo: &TabLooksDemo) -> Dom {
    let mut layer = Dom::create_div().with_css("position: absolute; left: 0px; top: 0px;");
    // A phone that is really there has its own camera.
    if demo.phone == 0 || demo.phone == 3 {
        return layer;
    }
    let features = info.get_display_features();
    for cutout in features.cutouts.as_slice() {
        let r = &cutout.rect;
        layer = layer.with_child(Dom::create_div().with_css(
            format!(
                "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; \
                 background: #000000; border-radius: 50%;",
                r.origin.x, r.origin.y, r.size.width, r.size.height
            )
            .as_str(),
        ));
    }
    layer
}

/// Put the window on `preset`'s phone (or take it off): the insets, the cutouts and the bars come from
/// the preset, the status bar is hidden and the window draws under it and into the cutout area, so
/// the top of the page is the strip.
fn put_on_phone(info: &mut CallbackInfo, preset: DisplayPreset, on: bool) {
    let mut window = info.get_current_window_state();
    window.flags.display_preset = preset;
    window.flags.extend_into_safe_area = on;
    window.flags.system_bars.status = if on {
        SystemBarMode::HiddenTransient
    } else {
        SystemBarMode::Visible
    };
    window.flags.system_bars.draw_into_cutout = on;
    info.modify_window_state(window);
}

/// The strip in the band is a ribbon, or a plain tab bar again.
extern "C" fn on_ribbon_band(mut data: RefAny, _info: CallbackInfo) -> Update {
    keep(&mut data, |s| s.tab_looks.ribbon_band = !s.tab_looks.ribbon_band)
}

extern "C" fn on_phone_off(mut data: RefAny, mut info: CallbackInfo) -> Update {
    put_on_phone(&mut info, DisplayPreset::Off, false);
    keep(&mut data, |s| s.tab_looks.phone = 0)
}

extern "C" fn on_phone_pixel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    put_on_phone(&mut info, DisplayPreset::Pixel8, true);
    keep(&mut data, |s| s.tab_looks.phone = 1)
}

extern "C" fn on_phone_island(mut data: RefAny, mut info: CallbackInfo) -> Update {
    put_on_phone(&mut info, DisplayPreset::IPhoneDynamicIsland, true);
    keep(&mut data, |s| s.tab_looks.phone = 2)
}

/// The phone the app really runs on: the bars are hidden and the window draws under them and into the
/// cutout area, with whatever cutout the phone has (nothing is simulated).
extern "C" fn on_phone_real(mut data: RefAny, mut info: CallbackInfo) -> Update {
    put_on_phone(&mut info, DisplayPreset::Off, true);
    keep(&mut data, |s| s.tab_looks.phone = 3)
}

/// A tab of the touch strip, or a flip of its panel: the selection moves, and the panel remembers
/// where it came from.
extern "C" fn on_touch_tab(mut data: RefAny, _info: CallbackInfo, state: TabHeaderState) -> Update {
    keep(&mut data, |s| {
        let demo = &mut s.tab_looks;
        if demo.touch_active != state.active_tab {
            demo.touch_previous = demo.touch_active;
            demo.touch_active = state.active_tab;
        }
    })
}

/// A tab of the row that is longer than its box.
extern "C" fn on_wide_tab(mut data: RefAny, _info: CallbackInfo, state: TabHeaderState) -> Update {
    keep(&mut data, |s| s.tab_looks.wide_active = state.active_tab)
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
