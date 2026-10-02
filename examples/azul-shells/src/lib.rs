//! AzShells: the eleven app shells on the public azul API, with placeholder
//! content, in one window.
//!
//! A picker (S1..S11, Settings) over the chosen shell. Every slot is a
//! placeholder block named after the slot (`ShellEmptyState`), the chrome
//! is the real widgets (a `Ribbon` with a FILE button, a `StatusBar`, an
//! `AddressBar`, a `TreeView`, the `ShellNavigationPane` with its groups
//! and module switcher), the window's title row is azul's `Titlebar`
//! (`WindowDecorations::NoTitle`), and the whole thing sits in a
//! `ShellThemeScope`. Flat / Flora and Light / Dark buttons switch the app
//! theme and the mode; F6 cycles the panes (the shell's own hook);
//! Ctrl/Cmd+K opens the `ShellCommandPalette`.
//!
//! On stdout, for scripts (`scripts/shells_e2e.py`): `AZSHELLS_SHELL <n>`
//! when shell n (1..12) is shown, then `AZSHELLS_SLOTS <n> <id,id,...>`
//! (the DOM ids of its slots) and `AZSHELLS_PANES <n> <id,...>` (the F6
//! cycle); `AZSHELLS_PANE <i>` when F6 moved the focus to pane i;
//! `AZSHELLS_RUN <i>` when the palette ran command i.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, MobileShellOnTabCallbackType,
        SegmentedOnChangeCallbackType, ShellCommandPaletteOnQueryCallbackType,
        ShellCommandPaletteOnRunCallbackType, ShellNavigationPaneOnEventCallbackType,
        ShellOnPaneFocusCallbackType,
    },
    css::DarkLightMode,
    dom::VirtualKeyCode,
    option::OptionDarkLightMode,
    prelude::*,
    shells::{
        BrowserShell, CallShell, CanvasShell, DeveloperShell, DocumentShell, MediaShell,
        MobileShell, PimShell, RecordsShell, ShellBottomTab, ShellCommandPalette,
        ShellEmptyState, ShellNavigationGroup, ShellNavigationModule, ShellNavigationPane,
        ShellNavigationPaneEvent, ShellNavigationPaneEventKind, ShellPaletteCommand,
        ShellSettingsLayout, ShellSettingsSection, ShellThemeAccent, ShellThemeScope,
        TimelineShell, UtilityShell,
    },
    str::String as AzString,
    vec::{DomVec, StringVec},
    window::WindowDecorations,
    widgets::{
        AddressBar, Button, DetailsPane, Ribbon, RibbonAppButton, RibbonButton, RibbonGroup,
        RibbonItem, RibbonTab, Segmented, SegmentedState, StatusBar, StatusBarSegment, Titlebar,
        TreeView, TreeViewNode,
    },
};

// ==== The shell table ====

/// One entry of the picker: the shell, the DOM ids of the slots the demo
/// fills, and the F6 cycle (what the e2e checks against the window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellInfo {
    /// The picker label ("S4").
    pub pick: &'static str,
    /// The shell's name ("Three-pane PIM").
    pub name: &'static str,
    /// The DOM ids of every slot the demo hands in, in tree order.
    pub slots: &'static [&'static str],
    /// The DOM ids F6 cycles through, in order.
    pub panes: &'static [&'static str],
}

/// The twelve picks: S1..S11 and the settings layout.
pub const SHELLS: [ShellInfo; 12] = [
    ShellInfo {
        pick: "S1",
        name: "Document + ribbon",
        slots: &["shell-title", "shell-ribbon", "shell-navigation", "shell-document", "shell-side", "shell-status"],
        panes: &["shell-navigation", "shell-document", "shell-side"],
    },
    ShellInfo {
        pick: "S2",
        name: "Canvas editor",
        slots: &[
            "shell-title", "shell-ribbon", "shell-toolbar", "shell-tools", "shell-canvas", "shell-tabs",
            "shell-panels", "shell-drawer", "shell-status",
        ],
        panes: &["shell-tools", "shell-canvas", "shell-panels", "shell-drawer"],
    },
    ShellInfo {
        pick: "S3",
        name: "Timeline editor",
        slots: &["shell-title", "shell-media", "shell-source", "shell-program", "shell-inspector", "shell-timeline"],
        panes: &["shell-media", "shell-source", "shell-program", "shell-inspector", "shell-timeline"],
    },
    ShellInfo {
        pick: "S4",
        name: "Three-pane PIM",
        slots: &["shell-title", "shell-ribbon", "shell-navigation", "shell-list", "shell-reading", "shell-right-bar", "shell-status"],
        panes: &["shell-navigation", "shell-list", "shell-reading", "shell-right-bar"],
    },
    ShellInfo {
        pick: "S5",
        name: "Browser / manager",
        slots: &[
            "shell-title", "shell-ribbon", "shell-address-bar", "shell-tree", "shell-content", "shell-preview",
            "shell-details", "shell-status",
        ],
        panes: &["shell-tree", "shell-content", "shell-preview", "shell-details"],
    },
    ShellInfo {
        pick: "S6",
        name: "Records & dashboards",
        slots: &["shell-title", "shell-ribbon", "shell-table", "shell-cards", "shell-form", "shell-status"],
        panes: &["shell-table", "shell-form"],
    },
    ShellInfo {
        pick: "S7",
        name: "Media player",
        slots: &["shell-title", "shell-sidebar", "shell-content", "shell-status"],
        panes: &["shell-sidebar", "shell-content"],
    },
    ShellInfo {
        pick: "S8",
        name: "Developer surface",
        slots: &["shell-title", "shell-activity-bar", "shell-side-bar", "shell-editor", "shell-panel", "shell-status"],
        panes: &["shell-activity-bar", "shell-side-bar", "shell-editor", "shell-panel"],
    },
    ShellInfo {
        pick: "S9",
        name: "Utility window",
        slots: &["shell-title", "shell-modes", "shell-content"],
        panes: &[],
    },
    ShellInfo {
        pick: "S10",
        name: "Call / capture",
        slots: &["shell-title", "shell-tiles", "shell-side-panel", "shell-devices", "shell-status", "shell-controls"],
        panes: &["shell-tiles", "shell-side-panel"],
    },
    ShellInfo {
        pick: "S11",
        name: "Mobile stack",
        slots: &["shell-app-bar", "shell-page", "shell-bottom-tabs"],
        panes: &[],
    },
    ShellInfo {
        pick: "Settings",
        name: "Settings layout",
        slots: &["shell-area"],
        panes: &[],
    },
];

/// The stdout lines for pick `index` (0-based), as the scripts read them.
#[must_use]
pub fn stdout_lines(index: usize) -> Vec<String> {
    let info = &SHELLS[index.min(SHELLS.len() - 1)];
    vec![
        format!("AZSHELLS_SHELL {}", index + 1),
        format!("AZSHELLS_SLOTS {} {}", index + 1, info.slots.join(",")),
        format!("AZSHELLS_PANES {} {}", index + 1, info.panes.join(",")),
    ]
}

fn announce(index: usize) {
    for line in stdout_lines(index) {
        println!("{line}");
    }
}

// ==== State ====

struct Shells {
    pick: usize,
    palette_open: bool,
    query: String,
    nav_collapsed: bool,
    module: usize,
    tab: usize,
    group_open: [bool; 2],
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
}

/// A slot's placeholder: a block named after the slot.
fn slot(name: &str, icon: &str) -> Dom {
    ShellEmptyState::create(AzString::from(name))
        .with_icon(AzString::from(icon))
        .with_detail(AzString::from("placeholder content"))
        .dom()
}

/// A row of labelled buttons (a menu bar, a toolbar, a controls bar).
fn bar(app: &RefAny, labels: &[&str]) -> Dom {
    let mut row = Dom::create_div().with_css("display: flex; flex-direction: row; align-items: center;");
    for label in labels {
        row.add_child(
            Button::create(AzString::from(*label))
                .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType)
                .dom()
                .with_css("margin-right: 4px;"),
        );
    }
    row
}

/// A column of icon buttons (a tool palette, an activity bar).
fn rail(app: &RefAny, icons: &[&str]) -> Dom {
    let mut column = Dom::create_div().with_css("display: flex; flex-direction: column; align-items: center;");
    for icon in icons {
        column.add_child(
            Button::create(AzString::from(""))
                .with_icon(AzString::from(*icon))
                .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType)
                .dom()
                .with_css("margin: 2px;"),
        );
    }
    column
}

/// The shell's own title row: its name, in the shell's `<header>` slot
/// (the window's title row with the traffic lights sits above the picker).
fn heading(name: &str) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 12px;")
        .with_child(Dom::create_p_with_text(AzString::from(name)).with_css("font-size: 13px; font-weight: 600;"))
}

fn ribbon(app: &RefAny) -> Dom {
    let home = RibbonTab::create(AzString::from("HOME"))
        .with_group(
            RibbonGroup::create(AzString::from("New"))
                .with_item(RibbonItem::LargeButton(
                    RibbonButton::create(AzString::from("mail"), AzString::from("New item"))
                        .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType),
                ))
                .with_item(RibbonItem::LargeButton(
                    RibbonButton::create(AzString::from("delete"), AzString::from("Delete"))
                        .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType),
                )),
        )
        .with_group(RibbonGroup::create(AzString::from("Respond")).with_item(RibbonItem::LargeButton(
            RibbonButton::create(AzString::from("reply"), AzString::from("Reply"))
                .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType),
        )));
    let view = RibbonTab::create(AzString::from("VIEW")).with_group(
        RibbonGroup::create(AzString::from("Layout")).with_item(RibbonItem::LargeButton(
            RibbonButton::create(AzString::from("view_sidebar"), AzString::from("Navigation pane"))
                .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType),
        )),
    );
    Ribbon::create(vec![home, view])
        .with_app_button(RibbonAppButton::create(AzString::from("FILE")))
        .dom_desktop()
}

fn status_bar(text: &str) -> Dom {
    StatusBar::create(vec![StatusBarSegment::create(AzString::from(text))]).dom()
}

fn tree(root: &str, children: &[&str]) -> TreeViewNode {
    let mut node = TreeViewNode::create(AzString::from(root)).with_expanded(true);
    for child in children {
        node = node.with_child(TreeViewNode::create(AzString::from(*child)));
    }
    node
}

fn tree_view(root: &str, children: &[&str]) -> Dom {
    TreeView::create(tree(root, children)).dom()
}

fn navigation_pane(s: &Shells, app: &RefAny) -> Dom {
    ShellNavigationPane::create()
        .with_header(
            Button::create(AzString::from("New message"))
                .with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType)
                .dom(),
        )
        .with_group(
            ShellNavigationGroup::create(AzString::from("Favorites"), tree("Favorites", &["Inbox", "Sent"]))
                .with_open(s.group_open[0]),
        )
        .with_group(
            ShellNavigationGroup::create(
                AzString::from("me@example.org"),
                tree("me@example.org", &["Inbox", "Drafts", "Sent", "Archive"]),
            )
            .with_count(4)
            .with_open(s.group_open[1]),
        )
        .with_module(ShellNavigationModule::create(AzString::from("Mail"), AzString::from("mail")).with_badge(AzString::from("12")))
        .with_module(ShellNavigationModule::create(AzString::from("Calendar"), AzString::from("event")))
        .with_module(ShellNavigationModule::create(AzString::from("Contacts"), AzString::from("person")))
        .with_module(ShellNavigationModule::create(AzString::from("Tasks"), AzString::from("check_circle")))
        .with_active_module(s.module)
        .with_collapsed(s.nav_collapsed)
        .with_on_event(app.clone(), on_nav_event as ShellNavigationPaneOnEventCallbackType)
        .dom()
}

fn palette(s: &Shells, app: &RefAny) -> Dom {
    ShellCommandPalette::create()
        .with_command(ShellPaletteCommand::create(AzString::from("New message")).with_shortcut(AzString::from("Ctrl+N")).with_icon(AzString::from("mail")))
        .with_command(ShellPaletteCommand::create(AzString::from("Archive")).with_icon(AzString::from("archive")))
        .with_command(ShellPaletteCommand::create(AzString::from("Reply")).with_shortcut(AzString::from("Ctrl+R")))
        .with_command(ShellPaletteCommand::create(AzString::from("Settings")).with_category(AzString::from("App")))
        .with_query(AzString::from(s.query.as_str()))
        .with_open(s.palette_open)
        .with_on_query(app.clone(), on_palette_query as ShellCommandPaletteOnQueryCallbackType)
        .with_on_run(app.clone(), on_palette_run as ShellCommandPaletteOnRunCallbackType)
        .with_on_close(app.clone(), on_palette_close as ButtonOnClickCallbackType)
        .dom()
}

fn shell_dom(s: &Shells, app: &RefAny) -> Dom {
    let info = &SHELLS[s.pick.min(SHELLS.len() - 1)];
    let title = heading(&format!("{} - {}", info.pick, info.name));
    match s.pick {
        0 => DocumentShell::create(slot("Document", "description"))
            .with_navigation(slot("Navigation", "list"))
            .with_side_pane(slot("Side pane", "comment"))
            .office_shell()
            .with_title_row(title)
            .with_ribbon(ribbon(app))
            .with_status_bar(status_bar("PAGE 1 OF 1"))
            .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
            .dom(),
        1 => CanvasShell::create(slot("Canvas", "image"))
            .with_menu_bar(title)
            .with_tool_options(bar(app, &["Size", "Hardness", "Opacity", "Mode"]))
            .with_tool_palette(rail(app, &["brush", "edit", "crop", "text_fields"]))
            .with_document_tabs(Segmented::create(strs(&["photo.graphite", "poster.graphite"])).dom())
            .with_panels(slot("Panels", "layers"))
            .with_drawer(slot("Node graph", "account_tree"))
            .office_shell()
            .with_status_bar(status_bar("100% | 1920x1080 px | sRGB"))
            .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
            .dom(),
        2 => TimelineShell::create(
            slot("Media", "perm_media"),
            slot("Source", "movie"),
            slot("Program", "movie"),
            slot("Inspector", "tune"),
            slot("Timeline", "view_timeline"),
        )
        .with_menu_bar(title)
        .with_meters(slot("Meters", "graphic_eq"))
        .office_shell()
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        3 => PimShell::create(
            navigation_pane(s, app),
            slot("Message list", "inbox"),
            slot("Reading pane", "mail"),
        )
        .with_todo_bar(slot("To-Do bar", "event"))
        .with_list_label(AzString::from("Message list"))
        .office_shell()
        .with_title_row(title)
        .with_ribbon(ribbon(app))
        .with_status_bar(status_bar("Synced 1 min ago"))
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        4 => BrowserShell::create(
            AddressBar::create(strs(&["This PC", "Home", "Documents"])).with_can_go(true, false, true).dom(),
            tree_view("This PC", &["Home", "Desktop", "Downloads"]),
            slot("Content", "folder"),
        )
        .with_ribbon(ribbon(app))
        .with_preview(slot("Preview", "image"))
        .with_details(
            DetailsPane::create(AzString::from("Documents"))
                .with_icon(AzString::from("folder"))
                .with_subtitle(AzString::from("File folder"))
                .with_property(AzString::from("Items"), AzString::from("6"))
                .dom(),
        )
        .office_shell()
        .with_title_row(title)
        .with_status_bar(status_bar("6 items"))
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        5 => RecordsShell::create(
            Segmented::create(strs(&["Processes", "Performance", "Services"])).dom(),
            slot("Table", "table_chart"),
        )
        .with_cards(bar(app, &["CPU 23%", "Memory 61%", "Disk 4 MB/s", "Net 1.2 Mb/s"]))
        .with_form(slot("Record", "edit_note"))
        .office_shell()
        .with_title_row(title)
        .with_status_bar(status_bar("214 processes"))
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        6 => MediaShell::create(
            slot("Library", "library_music"),
            slot("Albums", "album"),
            bar(app, &["Previous", "Play", "Next"]),
        )
        .office_shell()
        .with_title_row(title)
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        7 => DeveloperShell::create(
            rail(app, &["description", "search", "call_split", "play_arrow"]),
            tree_view("azul-apps", &["apps", "planning", "Cargo.toml"]),
            slot("Editor", "code"),
        )
        .with_panel(slot("Terminal", "terminal"))
        .office_shell()
        .with_title_row(title)
        .with_status_bar(status_bar("main | Ln 4, Col 25 | Rust"))
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        8 => UtilityShell::create(slot("Keypad", "calculate"))
            .with_title_row(title)
            .with_modes(Segmented::create(strs(&["World", "Alarm", "Timer", "Stopwatch"])).dom())
            .with_min_size(320.0, 480.0)
            .dom(),
        9 => CallShell::create(
            DomVec::from(vec![slot("You", "videocam"), slot("Anna", "person"), slot("Bob", "person")]),
            bar(app, &["Mute", "Stop video", "Share", "Leave"]),
        )
        .with_header(title)
        .with_side_panel(slot("Participants", "group"))
        .with_devices(slot("Devices", "settings_voice"))
        .office_shell()
        .with_on_pane_focus(app.clone(), on_pane as ShellOnPaneFocusCallbackType)
        .dom(),
        10 => MobileShell::create(AzString::from("Inbox"))
            .with_page(slot("Inbox", "inbox"))
            .with_page(slot("Message", "mail"))
            .with_fab(Button::create(AzString::from("+")).with_on_click(app.clone(), on_noop as ButtonOnClickCallbackType).dom())
            .with_tab(ShellBottomTab::create(AzString::from("Mail"), AzString::from("mail")).with_badge(AzString::from("12")))
            .with_tab(ShellBottomTab::create(AzString::from("Calendar"), AzString::from("event")))
            .with_tab(ShellBottomTab::create(AzString::from("Contacts"), AzString::from("person")))
            .with_tab(ShellBottomTab::create(AzString::from("Settings"), AzString::from("settings")))
            .with_active_tab(s.tab)
            .with_on_back(app.clone(), on_noop as ButtonOnClickCallbackType)
            .with_on_tab(app.clone(), on_tab as MobileShellOnTabCallbackType)
            .dom(),
        _ => ShellSettingsLayout::create(strs(&["General", "Accounts", "Appearance", "Advanced"]))
            .with_section(ShellSettingsSection::create(AzString::from("Startup"), slot("Startup options", "tune")))
            .with_section(ShellSettingsSection::create(AzString::from("Language"), slot("Language options", "language")))
            .with_active_category(1)
            .dom(),
    }
}

fn picker_row(s: &Shells, app: &RefAny) -> Dom {
    let picks: Vec<&str> = SHELLS.iter().map(|i| i.pick).collect();
    Dom::create_div()
        .with_id("picker")
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 8px; flex-shrink: 0;")
        .with_child(
            Segmented::create(strs(&picks))
                .with_selected_index(s.pick)
                .with_on_change(app.clone(), on_pick as SegmentedOnChangeCallbackType)
                .dom(),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(Button::create(AzString::from("Flat")).with_on_click(app.clone(), on_flat as ButtonOnClickCallbackType).dom())
        .with_child(Button::create(AzString::from("Flora")).with_on_click(app.clone(), on_flora as ButtonOnClickCallbackType).dom())
        .with_child(Button::create(AzString::from("Light")).with_on_click(app.clone(), on_light as ButtonOnClickCallbackType).dom())
        .with_child(Button::create(AzString::from("Dark")).with_on_click(app.clone(), on_dark as ButtonOnClickCallbackType).dom())
        .with_child(Button::create(AzString::from("Palette")).with_on_click(app.clone(), on_palette_toggle as ButtonOnClickCallbackType).dom())
}

/// The window's title row, drawn by azul (the window is `NoTitle`).
fn title_row() -> Dom {
    Titlebar::create(AzString::from("AzShells")).without_border_bottom().dom()
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<Shells>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let area = Dom::create_div()
        .with_id("shell-area")
        .with_css("position: relative; display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell_dom(s, &app))
        .with_child(palette(s, &app));
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(title_row())
        .with_child(picker_row(s, &app))
        .with_child(area);
    Dom::create_body()
        .with_css("display: flex; flex-direction: column;")
        .with_child(ShellThemeScope::create(column).with_accent(ShellThemeAccent::Blue).dom())
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app, on_key)
}

// ==== Callbacks ====

extern "C" fn on_noop(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

extern "C" fn on_pick(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    s.pick = state.selected_index.min(SHELLS.len() - 1);
    announce(s.pick);
    Update::RefreshDom
}

extern "C" fn on_pane(_data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    println!("AZSHELLS_PANE {index}");
    Update::DoNothing
}

extern "C" fn on_flat(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_theme(AzString::from("flat"));
    Update::DoNothing
}

extern "C" fn on_flora(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_theme(AzString::from("flora"));
    Update::DoNothing
}

extern "C" fn on_light(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Light));
    Update::DoNothing
}

extern "C" fn on_dark(_data: RefAny, mut info: CallbackInfo) -> Update {
    info.set_mode(OptionDarkLightMode::Some(DarkLightMode::Dark));
    Update::DoNothing
}

extern "C" fn on_palette_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    s.palette_open = !s.palette_open;
    Update::RefreshDom
}

/// Ctrl/Cmd+K opens (or closes) the command palette; Escape closes it.
extern "C" fn on_key(mut data: RefAny, info: CallbackInfo) -> Update {
    let keyboard = info.get_current_keyboard_state();
    let key = keyboard.current_virtual_keycode.into_option();
    let modifiers = info.get_key_modifiers();
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    match key {
        Some(VirtualKeyCode::K) if modifiers.ctrl || modifiers.meta => {
            s.palette_open = !s.palette_open;
            Update::RefreshDom
        }
        Some(VirtualKeyCode::Escape) if s.palette_open => {
            s.palette_open = false;
            Update::RefreshDom
        }
        _ => Update::DoNothing,
    }
}

extern "C" fn on_palette_query(mut data: RefAny, _info: CallbackInfo, query: AzString) -> Update {
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    s.query = query.as_str().to_string();
    Update::RefreshDom
}

extern "C" fn on_palette_run(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    println!("AZSHELLS_RUN {index}");
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    s.palette_open = false;
    Update::RefreshDom
}

extern "C" fn on_palette_close(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    s.palette_open = false;
    Update::RefreshDom
}

extern "C" fn on_nav_event(mut data: RefAny, _info: CallbackInfo, event: ShellNavigationPaneEvent) -> Update {
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    match event.kind {
        ShellNavigationPaneEventKind::ModuleSelected => s.module = event.index,
        ShellNavigationPaneEventKind::CollapseToggled => s.nav_collapsed = !event.expand,
        ShellNavigationPaneEventKind::GroupToggled => {
            if event.group < s.group_open.len() {
                s.group_open[event.group] = event.expand;
            }
        }
        ShellNavigationPaneEventKind::NodeClicked
        | ShellNavigationPaneEventKind::NodeToggled
        | ShellNavigationPaneEventKind::NodeDropped => {}
    }
    Update::RefreshDom
}

extern "C" fn on_tab(mut data: RefAny, _info: CallbackInfo, index: usize) -> Update {
    let Some(mut s) = data.downcast_mut::<Shells>() else {
        return Update::DoNothing;
    };
    s.tab = index;
    Update::RefreshDom
}

// ==== Entry ====

pub fn start() {
    let state = Shells {
        pick: 0,
        palette_open: false,
        query: String::new(),
        nav_collapsed: false,
        module: 0,
        tab: 0,
        group_open: [true, true],
    };
    announce(0);
    let app = App::create(RefAny::new(state), AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.size.dimensions = LogicalSize::create(1100.0, 720.0);
    window.window_state.title = AzString::from("AzShells");
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    app.run(window);
}

#[cfg(test)]
mod shell_table_tests {
    //! No azul types here: the shell table, as the picker and the scripts
    //! read it, is tested without a window.

    use super::*;

    #[test]
    fn the_picker_names_the_eleven_shells_and_the_settings_layout() {
        let picks: Vec<&str> = SHELLS.iter().map(|i| i.pick).collect();
        assert_eq!(
            picks,
            vec!["S1", "S2", "S3", "S4", "S5", "S6", "S7", "S8", "S9", "S10", "S11", "Settings"]
        );
    }

    #[test]
    fn every_shell_lists_its_slots_once_and_every_pane_is_a_slot() {
        for info in &SHELLS {
            assert!(!info.slots.is_empty(), "{}: no slots", info.pick);
            let mut seen = Vec::new();
            for slot in info.slots {
                assert!(!seen.contains(slot), "{}: slot {slot} listed twice", info.pick);
                seen.push(*slot);
            }
            for pane in info.panes {
                assert!(info.slots.contains(pane), "{}: pane {pane} is not a slot", info.pick);
            }
        }
    }

    #[test]
    fn s4_cycles_navigation_list_reading_and_the_todo_bar() {
        let s4 = &SHELLS[3];
        assert_eq!(s4.pick, "S4");
        assert_eq!(
            s4.panes,
            &["shell-navigation", "shell-list", "shell-reading", "shell-right-bar"]
        );
    }

    #[test]
    fn the_stdout_lines_name_the_shell_its_slots_and_its_cycle() {
        let lines = stdout_lines(3);
        assert_eq!(lines[0], "AZSHELLS_SHELL 4");
        assert!(lines[1].starts_with("AZSHELLS_SLOTS 4 shell-title,shell-ribbon,"));
        assert_eq!(
            lines[2],
            "AZSHELLS_PANES 4 shell-navigation,shell-list,shell-reading,shell-right-bar"
        );
        assert_eq!(stdout_lines(8)[2], "AZSHELLS_PANES 9 ", "a utility has no cycle");
    }
}
