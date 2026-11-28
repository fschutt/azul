//! The main window's frame: azul's `AppFrame`, the one every heavy app shares.
//!
//! ```text
//! desktop                                         phone (the rail; the drawer pushes it all aside)
//! +----+-----------------------------------+      +--------------------------+
//! | ●●●| File | Home | View                |      | [File][Home][View]       |
//! |    +----------------------+------------+      +-+------------------------+
//! |the | the view: the month, | Preview:   |      |r| the view (month, hours) |
//! |side| the hours of a day or| the event  |      |a|                        |
//! |bar | a week, the list     | itself     |      |i+------------------------+
//! |    |                      | (?) banner |      |l| Preview (bottom pane)  |
//! |    +----------------------+------------+      +-+------------------------+
//! |    | status bar: items, the range, links|      | status bar               |
//! +----+-----------------------------------+      +--------------------------+
//! ```
//!
//! - The LEFT sidebar (`ui_sidebar`) is the calendars as coloured check rows (and the date
//!   navigator in its header while it is expanded); collapsed it is the 18 px rail with Today and
//!   New Appointment (nearest the thumb) at the bottom and the status slot of the meeting links.
//! - The tab strip is the title bar: File is the first tab and shows the LOCATION (`ui_location`:
//!   the year, the month, the week or the day big and bold, the pen goes to a date, `...` opens
//!   File's old pages), then Home and View (`chrome::ribbon`).
//! - The body is the calendar's view (`views_ui::calendar_pane`); the PREVIEW (`ui_preview`) shows
//!   the selected event and nothing else (its attendees, notes, reminder and meeting link are
//!   behind (?) buttons, a link still waiting for the meeting server is the banner), docked at the
//!   right of the view, on a phone as a bottom pane.
//! - The footer is the status bar. A phone's quick-action sheet (`ui_sheet`) lies over it all.
//! - File's old pages (Info, Open & Export, Print, Calendars, Options, About) are dialogs
//!   (`ui_dialogs`: the engine's `Modal`, an OS window on a desktop and an inline sheet on a phone).

use azul::{
    prelude::*,
    widgets::{AppFrame, FormFactor, SidebarState, TabsInTitlebar},
};

use crate::{chrome, ids, ui_dialogs, ui_preview, ui_sheet, ui_sidebar, views_ui, CalState};

/// The environment variable that asks for the phone's layout (`phone`) or the desktop's
/// (`desktop`) whatever the window is: what the scenarios and the pictures of a desktop window use
/// for the phone preset.
pub(crate) const FORM_VAR: &str = "AZCAL_FORM";

/// The widest a window is that is a phone's, px: the line the widgets draw (`Auto`'s).
pub(crate) const PHONE_MAX_WIDTH: f32 = 600.0;

/// The size of a phone's window on a desktop (the `phone` form): Pixel 8's logical size.
pub(crate) const PHONE_WINDOW: (f32, f32) = (411.0, 760.0);

/// The form `AZCAL_FORM` asks for: `phone`, `desktop` (any case), else the window decides.
#[must_use]
pub(crate) fn form_of(value: Option<&str>) -> FormFactor {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("phone") => FormFactor::Phone,
        Some("desktop") => FormFactor::Desktop,
        _ => FormFactor::Auto,
    }
}

/// Whether this build runs on a touch platform.
#[must_use]
pub(crate) const fn touch_platform() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// Whether a window `width` wide is built as a phone's: `Phone` always, `Desktop` never, `Auto`
/// where the platform is a touch one and the window is narrower than [`PHONE_MAX_WIDTH`] (a
/// desktop window narrowed stays a desktop's, a tablet at 600 dp and more is a desktop's).
#[must_use]
pub(crate) fn is_phone(form: FormFactor, width: f32, touch: bool) -> bool {
    match form {
        FormFactor::Phone => true,
        FormFactor::Desktop => false,
        FormFactor::Auto => touch && width > 0.0 && width < PHONE_MAX_WIDTH,
    }
}

/// Whether the window is a phone's now (what an event handler asks: the width is the last
/// layout's).
#[must_use]
pub(crate) fn phone_now(s: &CalState) -> bool {
    is_phone(s.form, s.window_width, touch_platform())
}

/// The sidebar's state at the start: a phone starts with the drawer shut (the rail), a desktop as
/// it was left (`nav_folded`: the rail), the widget's default being the rail for an app that
/// remembers nothing. A touch platform whose width is not known yet starts as a phone does.
#[must_use]
pub(crate) fn initial_sidebar(form: FormFactor, touch: bool, nav_folded: bool) -> SidebarState {
    match form {
        FormFactor::Phone => SidebarState::Rail,
        FormFactor::Desktop => {
            if nav_folded {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
        FormFactor::Auto => {
            if touch || nav_folded {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
    }
}

/// The sidebar's state after a choice on a phone: choosing something closes the drawer.
#[must_use]
pub(crate) const fn after_choice(state: SidebarState, phone: bool) -> SidebarState {
    match (state, phone) {
        (SidebarState::Expanded, true) => SidebarState::Rail,
        (other, _) => other,
    }
}

/// The view a phone starts on when nothing was saved: one day, hours down the page (a week of
/// seven columns does not fit 360 dp); a desktop starts on the week.
#[must_use]
pub(crate) fn default_view_for(phone: bool) -> crate::views::ViewKind {
    if phone {
        crate::views::ViewKind::Day
    } else {
        crate::views::default_view()
    }
}

/// The room the tab strip keeps at the window's edges for the OS's window controls the sidebar
/// does not cover, and the width it is laid out in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Strip {
    /// Room before the first tab (macOS's traffic lights, as far as the sidebar leaves them).
    pub left: f32,
    /// Room after the last tab (Linux's and Windows's controls over the strip).
    pub right: f32,
    /// The width the ribbon is laid out in, px.
    pub width: f32,
    /// Whether the window is a phone's: the tabs are icon squares.
    pub phone: bool,
    /// How far the ribbon's content band keeps from the window's left edge: the phone's rail
    /// stands beside it, below the tab row (`Sidebar::phone_content_inset`); 0 elsewhere.
    pub content_inset: f32,
}

/// The tab strip's offsets: the platform's row of tab buttons that IS the title bar, with the room
/// before the first tab the sidebar leaves of the window's controls.
#[must_use]
pub(crate) fn strip_offsets(strip: &Strip) -> TabsInTitlebar {
    let platform = azul_appkit::ui::tabs_in_titlebar();
    TabsInTitlebar::create(platform.top, strip.left.max(0.0), strip.right.max(platform.right))
}

/// The main window's DOM below the theme scope: the frame of the calendar, and the dialog of File's
/// page that is open over it.
pub(crate) fn window(s: &CalState, app: &RefAny, info: &LayoutCallbackInfo) -> Dom {
    let width = info.get_window_width();
    let insets = azul_appkit::ui::title_insets(info);
    let phone = is_phone(s.form, width, touch_platform());
    // The sidebar first: the tab strip starts where it leaves off and clears what it leaves of
    // the window's controls.
    let dark = matches!(info.get_mode(), azul::css::DarkLightMode::Dark);
    let sidebar = azul_appkit::frame::sidebar_in_window(ui_sidebar::sidebar(s, app, dark), info);
    let strip = Strip {
        left: azul_appkit::frame::tab_row_left_pad(&sidebar),
        right: insets.right,
        width: (width - sidebar.column().in_frame(width)).max(1.0),
        phone,
        content_inset: sidebar.phone_content_inset(),
    };
    let header = chrome::ribbon(s, app, &strip);
    let body = views_ui::calendar_pane(s, app, phone, info.get_window_height());
    let mut frame = AppFrame::create(header, body)
        .with_sidebar(sidebar)
        .with_footer(chrome::status_bar(s, app))
        .with_dom_id(ids::FRAME)
        .with_form(s.form);
    if let Some(preview) = ui_preview::preview(s, app, phone) {
        frame = frame.with_preview(preview);
    }
    if let Some(sheet) = ui_sheet::sheet(s, app, phone) {
        frame = frame.with_sheet(sheet);
    }
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0px;")
        .with_child(frame.dom());
    if let Some(dialog) = ui_dialogs::file_dialog(s, app) {
        column.add_child(dialog);
    }
    column
}

#[cfg(test)]
#[path = "ui_frame_test.rs"]
mod ui_frame_test;
