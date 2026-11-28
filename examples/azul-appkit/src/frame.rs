//! The frame the heavy Azlin apps share (feature `azul`).
//!
//! AzMail, AzChat, AzCalendar, AzContacts, AzNotes, AzTasks, AzWriter, AzSheets, AzShow, AzWorkspace,
//! AzERP and AzDrive stand on azul's sidebar family - [`AppFrame`] with its [`Sidebar`], its
//! optional [`Preview`], its [`LocationBar`] in the first tab and its [`ActionSheet`] - and this
//! module is the one place that wires the family to THIS window, so that no app does it again:
//!
//! - [`sidebar_in_window`] hands a sidebar the width of the window (the phone's drawer slides
//!   between its states) and the room the OS's window controls take at the left (macOS's traffic
//!   lights, which overhang the 18 px rail);
//! - [`tab_row_left_pad`] is how far the tab row keeps from the window's left edge to clear them;
//! - [`example`] is the whole frame of a small mail client, the example the apps copy from: a
//!   sidebar (navigation rows, quick-access buttons, a status slot), the ribbon with the location in
//!   its first tab, the list as the body, an opt-in Preview with its (?) buttons, the command bar.
//!
//! The widgets own no state and run nothing: an app keeps its model in its own `RefAny`, builds the
//! frame in its layout callback from that model, hears every action through the ONE callback each
//! widget has (`SidebarOnEvent`, `PreviewOnEvent`, `LocationOnEvent`, `ActionSheetOnEvent`) and
//! lets the page be built again. Every app starts in Flora (`AppConfig`'s theme), with the sidebar
//! as the 18 px rail: that is the widget's own default and needs no line here.
//!
//! The rules the wireframe fixes and the widgets already follow, so an app does not repeat them:
//! the collapsed rail is 18 px on a desktop and on a phone, its navigation icons at the top and
//! the app's quick-access buttons at the bottom (the most used LAST, nearest to the thumb), then
//! the status slot and the toggle; on a phone the rail starts below the tab row and the open
//! drawer pushes the whole view aside; the Preview shows the item itself and nothing else
//! (properties, history and comments are behind (?) buttons, a warning in a banner); the first tab
//! (File) shows the location and every tab's band is as tall as the others'.

use azul::{
    callbacks::{
        LayoutCallbackInfo, LocationOnEventCallbackType, PreviewOnEventCallbackType, RefAny,
        SidebarOnEventCallbackType,
    },
    prelude::*,
    widgets::*,
};

use crate::ui::title_insets;

/// A sidebar in this window: it knows the window's width (the phone's drawer is a share of it and
/// slides between its states) and the room the OS's window controls take at the left. A phone
/// has no window controls: the sidebar ignores them there, so an app passes what the system says.
#[must_use]
pub fn sidebar_in_window(sidebar: Sidebar, info: &LayoutCallbackInfo) -> Sidebar {
    let insets = title_insets(info);
    let sidebar = sidebar.with_frame_width(info.get_window_width());
    if insets.left > 0.0 {
        sidebar.with_window_controls(insets.left, insets.height)
    } else {
        sidebar
    }
}

/// How far the tab row keeps from the window's left edge so that its first tab clears the OS's
/// window controls: what `sidebar` (as [`sidebar_in_window`] made it) does not cover of them. An
/// app without a sidebar uses [`crate::ui::tabs_in_titlebar`] instead.
#[must_use]
pub fn tab_row_left_pad(sidebar: &Sidebar) -> f32 {
    sidebar.header_left_pad()
}

/// The model of [`example`]: what a small mail client keeps.
#[derive(Debug, Clone)]
pub struct ExampleModel {
    /// The state of the sidebar: the rail, expanded, or the 16 px handle.
    pub sidebar: SidebarState,
    /// The row of the sidebar the list shows (the app's own tag for it).
    pub folder: usize,
    /// Whether the Preview shows, and the size the drag left it at (0: the default).
    pub preview_open: bool,
    pub preview_size: f32,
    /// Whether the message in the Preview has something to warn about.
    pub warning: bool,
    /// Whether the "Bold" toggle of the quick access is on.
    pub bold: bool,
}

impl Default for ExampleModel {
    fn default() -> Self {
        Self {
            sidebar: SidebarState::Rail,
            folder: 1,
            preview_open: true,
            preview_size: 0.0,
            warning: false,
            bold: false,
        }
    }
}

/// The whole frame of a small mail client: the example the apps copy from. `app` is the app's own
/// `RefAny` (its model); the three callbacks are the app's: they change the model and ask for a new
/// page, nothing else.
#[must_use]
pub fn example(
    app: &RefAny,
    model: &ExampleModel,
    info: &LayoutCallbackInfo,
    on_sidebar: SidebarOnEventCallbackType,
    on_preview: PreviewOnEventCallbackType,
    on_location: LocationOnEventCallbackType,
) -> Dom {
    let row = |label: &str, icon: &str, tag: usize| {
        SidebarRow::create(label)
            .with_icon(icon)
            .with_tag(tag)
            .with_selected(model.folder == tag)
    };
    // navigation at the top of the rail, quick access at the bottom, the status slot above the toggle
    let sidebar = Sidebar::create()
        .with_section(
            SidebarSection::create("Favourites")
                .with_row(row("Inbox", "inbox", 1).with_badge("329"))
                .with_row(row("Flagged", "flag", 2)),
        )
        .with_section(
            SidebarSection::create("Mailboxes")
                .with_row(row("Sent", "send", 3))
                .with_row(row("Archive", "archive", 4)),
        )
        .with_quick_action(
            SidebarQuickAction::create("Bold", "format_bold")
                .with_tag(1)
                .with_state(if model.bold { SidebarQuickState::On } else { SidebarQuickState::Off }),
        )
        .with_quick_action(SidebarQuickAction::create("Compose", "edit").with_tag(2))
        .with_status(SidebarStatus::create("Sync", 0.82, PieTone::Busy).with_text("82 %"))
        .with_status(SidebarStatus::badge("Unread", "3", PieTone::Danger))
        .with_help("Help")
        .with_label("Mailboxes")
        .with_state(model.sidebar)
        .with_surface(SidebarSurface::Theme)
        .with_on_event(app.clone(), on_sidebar);
    let sidebar = sidebar_in_window(sidebar, info);

    // the first tab shows the location, every tab names its icon (a phone shows the icons)
    let file = RibbonTab::create("File").with_icon("folder").with_location(
        LocationBar::create("Inbox")
            .with_parent(LocationCrumb::create("Outlook", 1))
            .with_more(true)
            .with_on_event(app.clone(), on_location)
            .dom(),
    );
    let home = RibbonTab::create("Home").with_icon("home").with_group(
        RibbonGroup::create("New").with_item(RibbonItem::LargeButton(RibbonButton::create("edit", "Compose"))),
    );
    let ribbon = Ribbon::create(vec![file, home]).dom_desktop();

    // the Preview is opt-in and shows the message itself; its details are behind (?) buttons
    let mut preview = Preview::create(Dom::create_div(), Dom::create_p_with_text("The message."))
        .with_title("Message")
        .with_detail(PreviewDetail::create("Headers", Dom::create_p_with_text("From, To, Date ...")))
        .with_open(model.preview_open)
        .with_size(model.preview_size)
        .with_on_event(app.clone(), on_preview);
    if model.warning {
        preview = preview.with_banner(
            PreviewBanner::create("Unverified signer", PreviewBannerTone::Warning)
                .with_details(Dom::create_p_with_text("The signer is not in your contacts.")),
        );
    }

    AppFrame::create(ribbon, Dom::create_div())
        .with_sidebar(sidebar)
        .with_preview(preview)
        .with_footer(Dom::create_p_with_text("> search, a folder or a command"))
        .dom()
}
