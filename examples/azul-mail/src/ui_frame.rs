//! The main window's frame: azul's `AppFrame`, the one every heavy app shares.
//!
//! ```text
//! desktop                                         phone (the rail; the drawer pushes it all aside)
//! +----+-----------------------------------+      +--------------------------+
//! | ●●●| File | Home | Send/Receive | ...   |      | [File][Home][S/R][...]   |
//! |    +----------------------+------------+      +-+------------------------+
//! |the | message list         | Preview:   |      |r| message list           |
//! |side| (search, arrange by, | the message|      |a|                        |
//! |bar | date groups)         | itself     |      |i+------------------------+
//! |    |                      | (?) banner |      |l| Preview (bottom pane)  |
//! |    +----------------------+------------+      +-+------------------------+
//! |    | status bar: items, unread, sync   |      | status bar               |
//! +----+-----------------------------------+      +--------------------------+
//! ```
//!
//! - The LEFT sidebar (`ui_sidebar`) is the accounts' folder trees, Local Folders and the module
//!   buttons; collapsed it is the 18 px rail with navigation icons at the top, the quick-access
//!   buttons (Compose last) at the bottom, the status slot (Send / Receive, unread) and the toggle.
//! - The tab strip is the title bar (there is no title row): File is the first tab and shows the
//!   LOCATION (`ui_location`: the account, the folder, `...` for File's old items), then Home,
//!   Send / Receive, Folder, View (`ui_main::ribbon`).
//! - The body is the message list; the PREVIEW (`ui_preview`) shows the message and nothing else
//!   (headers, attachments and the people are behind (?) buttons, held-back pictures are the
//!   banner), docked at the right of the list, on a phone as a bottom pane that follows the
//!   message at the TOP EDGE of the list while it scrolls.
//! - The footer is the status bar. A phone's quick-action sheet (`ui_sheet`) lies over it all.
//! - File's old pages (Info, Print, Help, Options, Exit) are the backstage: `...` of the location
//!   opens it under the tab strip ([`backstage_frame`]).

use azul::{
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope},
    widgets::{AppFrame, FormFactor, SidebarState, TabsInTitlebar},
};

use crate::{ids, ui_backstage, ui_main, ui_preview, ui_sheet, ui_sidebar, MailApp};

/// The environment variable that asks for the phone's layout (`phone`) or the desktop's
/// (`desktop`) whatever the window is: what the scenarios and the pictures of a desktop window
/// use for the phone preset.
pub(crate) const FORM_VAR: &str = "AZMAIL_FORM";

/// The widest a window is that is a phone's, px: the line the widgets draw (`Auto`'s).
pub(crate) const PHONE_MAX_WIDTH: f32 = 600.0;

/// The width of a To-Do bar beside the list on a desktop, px.
pub(crate) const TODO_WIDTH: f32 = 252.0;

/// The form `AZMAIL_FORM` asks for: `phone`, `desktop` (any case), else the window decides.
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
pub(crate) fn phone_now(s: &MailApp) -> bool {
    is_phone(s.form, s.window_width, touch_platform())
}

/// The sidebar's state at the start: a phone starts with the drawer shut (the rail), a desktop as
/// it was left (`nav_collapsed`: the rail), the widget's default being the rail for an app that
/// remembers nothing. A touch platform whose width is not known yet starts as a phone does.
#[must_use]
pub(crate) fn initial_sidebar(form: FormFactor, touch: bool, nav_collapsed: bool) -> SidebarState {
    match form {
        FormFactor::Phone => SidebarState::Rail,
        FormFactor::Desktop => {
            if nav_collapsed {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
        FormFactor::Auto => {
            if touch || nav_collapsed {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
    }
}

/// The sidebar's state after a choice on a phone: choosing a folder closes the drawer.
#[must_use]
pub(crate) const fn after_choice(state: SidebarState, phone: bool) -> SidebarState {
    match (state, phone) {
        (SidebarState::Expanded, true) => SidebarState::Rail,
        (other, _) => other,
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
}

/// The tab strip's offsets: the platform's row of tab buttons that IS the title bar, with the room
/// before the first tab the sidebar leaves of the window's controls.
#[must_use]
pub(crate) fn strip_offsets(strip: &Strip) -> TabsInTitlebar {
    let platform = azul_appkit::ui::tabs_in_titlebar();
    TabsInTitlebar::create(platform.top, strip.left.max(0.0), strip.right.max(platform.right))
}

/// The main window's DOM below the body element: the frame of the mail view, or File's backstage
/// under the tab strip; the About box over either.
pub(crate) fn window(s: &MailApp, app: &RefAny, info: &LayoutCallbackInfo) -> Dom {
    let frame = match s.backstage {
        Some(page) => backstage_frame(s, app, info, page),
        None => frame(s, app, info),
    };
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0px;")
        .with_child(frame);
    if s.about_open {
        column.add_child(ui_main::about_dialog(app));
    }
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Blue)
        .dom()
}

/// The frame of the mail view.
fn frame(s: &MailApp, app: &RefAny, info: &LayoutCallbackInfo) -> Dom {
    let width = info.get_window_width();
    let insets = azul_appkit::ui::title_insets(info);
    let phone = is_phone(s.form, width, touch_platform());
    // The sidebar first: the tab strip starts where it leaves off and clears what it leaves of the
    // window's controls.
    let sidebar = azul_appkit::frame::sidebar_in_window(ui_sidebar::sidebar(s, app), info);
    let strip = Strip {
        left: azul_appkit::frame::tab_row_left_pad(&sidebar),
        right: insets.right,
        width: (width - sidebar.column().in_frame(width)).max(1.0),
        phone,
    };
    let header = ui_main::ribbon(s, app, false, &strip);

    // the body: the message list (the ban banner over it), and on a desktop the To-Do bar beside it
    let mut list = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px; min-width: 0px;")
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;")
                .with_child(ui_main::message_list(s, app)),
        );
    if s.show_todo && !phone && s.module == 0 {
        list.add_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: column; flex-shrink: 0; width: {TODO_WIDTH}px; min-height: 0px; \
                     border-left: 1px solid system:separator;"
                ))
                .with_child(ui_main::todo_bar(s, app)),
        );
    }
    let mut body = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;");
    if let Some(bar) = ui_main::ban_bar(s) {
        body.add_child(bar);
    }
    body.add_child(list);

    let mut frame = AppFrame::create(header, body)
        .with_sidebar(sidebar)
        .with_footer(ui_main::status_bar(s, app))
        .with_form(s.form);
    if let Some(preview) = ui_preview::preview(s, app) {
        frame = frame.with_preview(preview);
    }
    if let Some(sheet) = ui_sheet::sheet(s, app, phone) {
        frame = frame.with_sheet(sheet);
    }
    frame.dom()
}

/// File's backstage: the tab strip stays on top (File lit, its band hidden), the page under it
/// fills the window. No sidebar: the strip is what the window is moved by.
fn backstage_frame(s: &MailApp, app: &RefAny, info: &LayoutCallbackInfo, page: usize) -> Dom {
    let width = info.get_window_width();
    let insets = azul_appkit::ui::title_insets(info);
    let strip = Strip {
        left: insets.left,
        right: insets.right,
        width: width.max(1.0),
        phone: is_phone(s.form, width, touch_platform()),
    };
    let header = ui_main::ribbon(s, app, true, &strip);
    // the container the office shell gave the backstage: the page fills what the strip leaves
    let body = Dom::create_div()
        .with_id(ids::BACKSTAGE)
        .with_class("__azul-native-office-shell-backstage")
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;")
        .with_child(ui_backstage::backstage(s, app, page));
    AppFrame::create(header, body).with_form(s.form).dom()
}

#[cfg(test)]
#[path = "ui_frame_test.rs"]
mod ui_frame_test;
