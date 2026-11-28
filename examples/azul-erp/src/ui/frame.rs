//! The window's frame: azul's `AppFrame`, the one every heavy app shares.
//!
//! ```text
//! desktop                                         phone (the rail; the drawer pushes it all aside)
//! +----+-----------------------------------+      +--------------------------+
//! | ●●●| File | Home | Reports | View       |      | [File][Home][Rep][View]  |
//! |    +----------------------+------------+      +-+------------------------+
//! |the | the table of the     | Preview:   |      |r| the table              |
//! |side| page (azul's         | the record |      |a|                        |
//! |bar | DataTable, virtual-  | (?) banner |      |i+------------------------+
//! |    | ised)                |            |      |l| Preview (bottom pane)  |
//! |    +----------------------+------------+      +-+------------------------+
//! |    | status line: records, book value  |      | status line              |
//! +----+-----------------------------------+      +--------------------------+
//! ```
//!
//! - The LEFT sidebar (`ui/sidebar.rs`) lists the ERP's modules (a section each) and the pages of
//!   a module (its rows, with the count of records they list); collapsed it is the 18 px rail:
//!   navigation icons at the top, the quick access (CSV import, CSV export, New - New last,
//!   nearest the thumb) at the bottom, the status slot (what is saved, what is overdue) and the
//!   toggle.
//! - The tab strip is the title bar (there is no title row): File is the first tab and shows the
//!   LOCATION (`ui/location.rs`: the module and the page, the record's name big under it, `...` for
//!   File's old items: Options and About), then Home, Reports and View (`ui/ribbon.rs`).
//! - The body is the page the views route to: a record table (azul's DataTable) or a record's
//!   page, a form, the reports, the depreciation run, the CSV import. The Preview (`ui/preview.rs`)
//!   shows the selected record of a table and nothing else (its schedule, log, properties are
//!   behind (?) buttons); on a phone it is the bottom pane, and the selection and the Preview
//!   follow the row at the TOP EDGE of the table while it scrolls (`follow.rs`).
//! - The footer is the status line. A phone's quick-action sheet (`ui/sheet.rs`) lies over it all.
//! - The Options and the About are the kit's settings page under a title row: `...` of the
//!   location opens it, as the Help row of the sidebar opens it at the shortcuts.
//!
//! The window is made for the right hand and AzERP decides no side of its own: the sidebar's edge
//! and the Preview's dock are the widget family's, read from one `Handedness` there.

use azul::{
    callbacks::{CloseGuardDirtyCheckCallbackType, CloseGuardOnEventCallbackType},
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope},
    widgets::{AppFrame, CloseGuard, FormFactor, SidebarState, TabsInTitlebar},
};
use azul_appkit::{insets::Insets, l10n::t, ui as kit};

use super::{
    delete_question, dirty_check, form, metrics_room, on_guard, on_key, page, preview, ribbon,
    sheet, sidebar, status_bar, Erp,
};
use crate::ids;

/// The environment variable that asks for the phone's layout (`phone`) or the desktop's
/// (`desktop`) whatever the window is: what the scenarios and the pictures of a desktop window
/// use for the phone preset.
pub(crate) const FORM_VAR: &str = "AZERP_FORM";

/// The widest a window is that is a phone's, px: the line the widgets draw (`Auto`'s).
pub(crate) const PHONE_MAX_WIDTH: f32 = 600.0;

/// The form `AZERP_FORM` asks for: `phone`, `desktop` (any case), else the window decides.
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
pub(crate) fn phone_now(s: &Erp) -> bool {
    is_phone(s.form, s.window.0, touch_platform())
}

/// The sidebar's state at the start: a phone starts with the drawer shut (the rail); a desktop
/// with the modules showing; a touch platform whose width is not known yet starts as a phone does.
#[must_use]
pub(crate) fn initial_sidebar(form: FormFactor, touch: bool) -> SidebarState {
    match form {
        FormFactor::Phone => SidebarState::Rail,
        FormFactor::Desktop => SidebarState::Expanded,
        FormFactor::Auto => {
            if touch {
                SidebarState::Rail
            } else {
                SidebarState::Expanded
            }
        }
    }
}

/// The sidebar's state after a choice on a phone: choosing a page closes the drawer.
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
    /// How far the ribbon's content band keeps from the window's left edge: the phone's rail
    /// stands beside it, below the tab row (`Sidebar::phone_content_inset`); 0 elsewhere.
    pub content_inset: f32,
}

/// The tab strip's offsets: the platform's row of tab buttons that IS the title bar, with the room
/// before the first tab the sidebar leaves of the window's controls.
#[must_use]
pub(crate) fn strip_offsets(strip: &Strip) -> TabsInTitlebar {
    let platform = kit::tabs_in_titlebar();
    TabsInTitlebar::create(
        platform.top,
        strip.left.max(0.0),
        strip.right.max(platform.right),
    )
}

/// The window's DOM below the body element: the frame of the page, or the settings page under its
/// title row; the delete question and a modal form over either; the close guard of an open form.
pub(crate) fn window(s: &mut Erp, app: &RefAny, info: &LayoutCallbackInfo) -> Dom {
    let inner = if kit::settings_open(&s.kit) {
        settings_window(s)
    } else {
        frame(s, app, info)
    };
    let mut column = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0px;",
        )
        .with_child(inner);
    if let Some(modal) = form::modal(s, app) {
        column.add_child(modal);
    }
    if let Some(question) = delete_question(s, app) {
        column.add_child(question);
    }
    // A form with edits is not lost to the close button: the guard asks.
    let content = if s.state.form.is_some() {
        CloseGuard::create(column, "")
            .with_question(t("azerp-close-question"), t("azerp-close-text"))
            .with_labels(
                t("azerp-save"),
                t("azerp-close-discard"),
                t("kit-button-cancel"),
            )
            .with_dirty_check(app.clone(), dirty_check as CloseGuardDirtyCheckCallbackType)
            .with_asking(s.asking)
            .with_on_event(app.clone(), on_guard as CloseGuardOnEventCallbackType)
            .dom()
    } else {
        column
    };
    ShellThemeScope::create(content)
        .with_accent(ShellThemeAccent::Slate)
        .body()
        .with_callback(
            EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            app.clone(),
            on_key,
        )
}

/// The Options and the About: the kit's settings page under a title row (the window is moved by
/// the row; there is no ribbon to move it by).
fn settings_window(s: &Erp) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; width: 100%; height: 100%; min-height: 0px;")
        .with_child(kit::title_row(crate::SPEC.name))
        .with_child(
            Dom::create_div()
                .with_id(ids::SETTINGS)
                .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;")
                .with_child(kit::settings_page(&s.kit, Vec::new())),
        )
}

/// The frame of the page.
fn frame(s: &mut Erp, app: &RefAny, info: &LayoutCallbackInfo) -> Dom {
    let width = info.get_window_width();
    let height = info.get_window_height();
    let title = kit::title_insets(info);
    let insets = Insets::of(info);
    let root = insets.root_size((width, height));
    let phone = is_phone(s.form, width, touch_platform());

    // The sidebar first: the tab strip starts where it leaves off and clears what it leaves of the
    // window's controls.
    let sidebar = azul_appkit::frame::sidebar_in_window(sidebar::sidebar(s, app), info);
    let column_px = sidebar.column().in_frame(width);
    let strip = Strip {
        left: azul_appkit::frame::tab_row_left_pad(&sidebar),
        right: title.right,
        width: (width - column_px).max(1.0),
        phone,
        content_inset: sidebar.phone_content_inset(),
    };
    let header = ribbon::ribbon(s, app, &strip);

    // what the table has room for: the root less the sidebar, the Preview, the ribbon and the status line
    let preview_shown = preview::shown(s);
    let room = metrics_room(
        s,
        root,
        phone,
        if phone { 0.0 } else { column_px },
        preview_shown,
    );
    let body = page(s, app, &room, phone, insets.inside_root().keyboard);

    let mut frame = AppFrame::create(header, body)
        .with_sidebar(sidebar)
        .with_footer(status_bar(s))
        .with_dom_id(ids::FRAME)
        .with_form(s.form);
    if preview_shown {
        frame = frame.with_preview(preview::preview(s, app));
    }
    if let Some(sheet) = sheet::sheet(s, app, phone) {
        frame = frame.with_sheet(sheet);
    }
    frame.dom()
}

#[cfg(test)]
#[path = "frame_test.rs"]
mod frame_test;
