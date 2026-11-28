//! The "Sidebar" section: the app frame every heavy app shares, on one card.
//!
//! A frame in a box of its own: the full-height left sidebar (the icon rail it starts as, the
//! expanded sidebar, the 16 px handle), the location block over the file list (the pen turns the
//! name into a live field: the list follows what is typed), the Inspector with the preview of the
//! selected file (docked at the right on a desktop, a bottom pane with a grip on a phone, both
//! resized by dragging), the status pies above the toggle, and the phone's quick-action sheet. The
//! rows are a mail client's and a drive's at once: a flat section, a tree with disclosure
//! triangles, a count, a collapsible section, a document row with its own pie and collaboration
//! icon.
//!
//! Every switch above the frame is the app's state (`SidebarDemo`): the widgets own nothing, they
//! report into it and the page is built again. The line under the frame says what the last report
//! was, which is what the scenarios (`e2e/sidebar_*.json`) read.

use azul::{
    callbacks::{
        ActionSheetOnEventCallbackType, ButtonOnClickCallbackType, InspectorOnEventCallbackType,
        LocationOnEventCallbackType, RibbonOnTabClickCallbackType, SidebarOnEventCallbackType,
    },
    prelude::*,
    str::String as AzString,
    widgets::*,
};

use crate::{captioned, keep, section};

/// The files of the folder the demo shows: `(name, kind, size)`.
const FILES: [(&str, &str, &str); 7] = [
    ("Inter-Regular.ttf", "TrueType font", "310 KB"),
    ("Inter-Bold.ttf", "TrueType font", "318 KB"),
    ("Inter-Italic.ttf", "TrueType font", "325 KB"),
    ("LICENSE.txt", "Text", "4 KB"),
    ("specimen.png", "PNG image", "1.2 MB"),
    ("notes.md", "Markdown", "1 KB"),
    ("fonts.json", "JSON", "3 KB"),
];

/// The tags of the sidebar's rows (the app's own names for them).
const TAG_COMPUTER: usize = 1;
const TAG_CLOUD: usize = 2;
const TAG_PHOTOS: usize = 3;
const TAG_DOCUMENTS: usize = 4;
const TAG_ASSETS: usize = 5;
const TAG_FONTS: usize = 6;
const TAG_ICONS: usize = 7;
const TAG_BOOK1: usize = 11;
const TAG_BOOK2: usize = 12;

/// The section's state. Lives in `Showcase::sidebar`.
#[derive(Debug, Clone)]
pub(crate) struct SidebarDemo {
    /// 0 a desktop, 1 a phone.
    pub(crate) form: usize,
    /// The sidebar's state: 0 expanded, 1 the rail, 2 the 16 px handle.
    pub(crate) state: usize,
    /// The surface: 0 the theme's, 1 leather, 2 vellum, 3 blue jeans, 4 aluminium.
    pub(crate) surface: usize,
    /// Whether the Inspector shows, and how wide / tall it is (the app keeps what the drag reports).
    pub(crate) inspector_open: bool,
    pub(crate) inspector_size: f32,
    /// The chosen row of the sidebar (its tag).
    pub(crate) selected: usize,
    /// Whether the tree's folder "assets" shows its children.
    pub(crate) assets_open: bool,
    /// Whether the collapsible "Favourites" section shows its rows.
    pub(crate) favourites_open: bool,
    /// The index of the file the Inspector shows.
    pub(crate) file: usize,
    /// The location's name field: whether it is open and what it holds.
    pub(crate) editing: bool,
    pub(crate) text: String,
    /// The phone's quick-action sheet.
    pub(crate) sheet_open: bool,
    /// Whether the two open documents collaborate.
    pub(crate) collab: [bool; 2],
    /// The ribbon's selected tab: 0 File (the location), 1 Home, 2 Share.
    pub(crate) tab: usize,
    /// What the last report was (the line under the frame).
    pub(crate) last: String,
}

impl Default for SidebarDemo {
    fn default() -> Self {
        Self {
            form: 0,
            state: 1,
            surface: 0,
            inspector_open: true,
            inspector_size: 0.0,
            selected: TAG_FONTS,
            assets_open: true,
            favourites_open: true,
            file: 0,
            editing: false,
            text: String::new(),
            sheet_open: false,
            collab: [false, true],
            tab: 0,
            last: String::from("nothing yet"),
        }
    }
}

/// The files the name field lets through: the ones whose names contain what is typed.
fn shown(demo: &SidebarDemo) -> Vec<usize> {
    let needle = demo.text.to_lowercase();
    FILES
        .iter()
        .enumerate()
        .filter(|(_, (name, _, _))| !demo.editing || needle.is_empty() || name.to_lowercase().contains(&needle))
        .map(|(i, _)| i)
        .collect()
}

fn surface_of(demo: &SidebarDemo) -> SidebarSurface {
    match demo.surface {
        1 => SidebarSurface::Leather,
        2 => SidebarSurface::Vellum,
        3 => SidebarSurface::BlueJeans,
        4 => SidebarSurface::Aluminium,
        _ => SidebarSurface::Theme,
    }
}

fn form_of(demo: &SidebarDemo) -> FormFactor {
    if demo.form == 1 {
        FormFactor::Phone
    } else {
        FormFactor::Desktop
    }
}

fn state_of(demo: &SidebarDemo) -> SidebarState {
    match demo.state {
        0 => SidebarState::Expanded,
        2 => SidebarState::Handle,
        _ => SidebarState::Rail,
    }
}

/// The sidebar: drives, favourites (a collapsible section), a tree, two open documents.
fn the_sidebar(data: &RefAny, demo: &SidebarDemo, theme: UiTheme) -> Sidebar {
    let row = |label: &str, icon: &str, tag: usize| {
        SidebarRow::create(label)
            .with_icon(icon)
            .with_tag(tag)
            .with_selected(demo.selected == tag)
    };
    let drives = SidebarSection::create("Drives")
        .with_row(row("This computer", "computer", TAG_COMPUTER))
        .with_row(row("Azul Drive", "cloud", TAG_CLOUD).with_badge("cloud"));
    let favourites = SidebarSection::create("Favourites")
        .with_collapsible(demo.favourites_open)
        .with_row(row("Photos", "image", TAG_PHOTOS).with_badge("2.1k"))
        .with_row(row("Documents", "description", TAG_DOCUMENTS));
    let mut tree = SidebarSection::create("azul").with_row(
        SidebarRow::create("assets")
            .with_icon("folder")
            .with_tag(TAG_ASSETS)
            .with_disclosure(demo.assets_open)
            .with_selected(demo.selected == TAG_ASSETS),
    );
    if demo.assets_open {
        tree = tree
            .with_row(SidebarRow::create("fonts").with_depth(1).with_tag(TAG_FONTS).with_selected(demo.selected == TAG_FONTS))
            .with_row(SidebarRow::create("icons").with_depth(1).with_tag(TAG_ICONS).with_selected(demo.selected == TAG_ICONS));
    }
    let collab = |on: bool| if on { SidebarCollab::On } else { SidebarCollab::Off };
    let documents = SidebarSection::create("Open documents")
        .with_row(
            SidebarRow::create("book1.xlsx")
                .with_icon("table_chart")
                .with_tag(TAG_BOOK1)
                .with_pie(0.7, PieTone::Busy)
                .with_collab(collab(demo.collab[0])),
        )
        .with_row(
            SidebarRow::create("book2.xlsx")
                .with_icon("table_chart")
                .with_tag(TAG_BOOK2)
                .with_pie(1.0, PieTone::Good)
                .with_collab(collab(demo.collab[1])),
        );
    Sidebar::create()
        .with_section(drives)
        .with_section(favourites)
        .with_section(tree)
        .with_section(documents)
        .with_status(
            SidebarStatus::create("Sync", 0.82, PieTone::Busy)
                .with_text("82 %")
                .with_detail("3 files waiting, the last sync was 2 minutes ago."),
        )
        .with_status(
            SidebarStatus::create("Drive", 0.64, PieTone::Accent)
                .with_text("64 % full")
                .with_detail("64 GB of 100 GB used, 36 GB free."),
        )
        .with_help("Help")
        .with_label("Places")
        .with_state(state_of(demo))
        .with_surface(surface_of(demo))
        .with_form(form_of(demo))
        .with_frame_width(if demo.form == 1 { 411.0 } else { 0.0 })
        .with_theme(theme)
        .with_on_event(data.clone(), on_sidebar as SidebarOnEventCallbackType)
}

/// The location block: the parents as one line, the folder's name big and bold with a pen.
fn the_location(data: &RefAny, demo: &SidebarDemo, theme: UiTheme) -> Dom {
    LocationBar::create("Fonts")
        .with_parent(LocationCrumb::create("azul", 1))
        .with_parent(LocationCrumb::create("assets", 2))
        .with_editing(demo.editing, demo.text.as_str())
        .with_placeholder("Type a name to filter the folder")
        .with_form(form_of(demo))
        .with_theme(theme)
        .with_on_event(data.clone(), on_location as LocationOnEventCallbackType)
        .dom()
}

/// The list of files under the location: the rows the name field lets through; a click picks the file
/// the Inspector shows.
fn the_list(data: &RefAny, demo: &SidebarDemo) -> Dom {
    let mut list = Dom::create_div().with_css(
        "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; overflow-y: auto; \
         border-top: 1px solid system:separator;",
    );
    for i in shown(demo) {
        let (name, _, size) = FILES[i];
        let selected = i == demo.file;
        list = list.with_child(
            Dom::create_div()
                .with_css(format!(
                    "display: flex; flex-direction: row; gap: 8px; align-items: center; height: 32px; \
                     padding: 0px 14px; flex-shrink: 0; cursor: pointer; font-size: 13px; \
                     border-bottom: 1px solid system:separator; {} :hover {{ background: \
                     rgba(127, 127, 127, 0.12); }}",
                    if selected { "background: system:selection-background; color: system:selection-text;" } else { "" }
                ))
                .with_class("sidebar-demo-file")
                .with_child(Dom::create_icon("insert_drive_file").with_css("font-size: 16px;"))
                .with_child(Dom::create_p_with_text(name).with_css("margin: 0px; flex-grow: 1;"))
                .with_child(Dom::create_p_with_text(size).with_css("margin: 0px; font-size: 11px; opacity: 0.7;"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(FileRef { data: data.clone(), file: i }),
                    on_file,
                ),
        );
    }
    list
}

/// What a file row carries: the page's data and its place.
struct FileRef {
    data: RefAny,
    file: usize,
}

/// The preview the Inspector shows: the file at the top of the list, its kind and size.
fn the_preview(demo: &SidebarDemo) -> Dom {
    let (name, kind, size) = FILES[demo.file.min(FILES.len() - 1)];
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; gap: 6px; padding: 12px 14px;")
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; align-items: center; justify-content: center; height: 72px; \
                     border-radius: 6px; background: rgba(127, 127, 127, 0.16); font-size: 12px;",
                )
                .with_child(Dom::create_icon("insert_drive_file").with_css("font-size: 32px;")),
        )
        .with_child(Dom::create_p_with_text(name).with_css("margin: 0px; font-weight: 700;").with_class("sidebar-demo-preview-name"))
        .with_child(Dom::create_p_with_text(format!("{kind} - {size}")).with_css("margin: 0px; font-size: 12px; opacity: 0.75;"))
}

/// The footer: the one-line command bar a drive shows at the foot of its window.
fn the_footer(demo: &SidebarDemo) -> Dom {
    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; gap: 12px; height: 28px; \
             padding: 0px 10px; font-size: 12px; border-top: 1px solid system:separator; \
             color: system:secondary-text;",
        )
        .with_child(Dom::create_p_with_text("> search, a path or a command").with_css("margin: 0px; flex-grow: 1;"))
        .with_child(Dom::create_p_with_text(format!("{} items", shown(demo).len())).with_css("margin: 0px;"))
}

/// A ribbon command that does nothing but look like one.
fn command(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::LargeButton(RibbonButton::create(icon, label))
}

/// The tab ribbon over the frame's full width: File (tab 0) shows the location in a band exactly as
/// tall as Home's, every tab names its icon (a phone shows the icons as squares), a vertical swipe on
/// the band flips between the tabs.
fn the_header(data: &RefAny, demo: &SidebarDemo, theme: UiTheme) -> Dom {
    let file = RibbonTab::create("File")
        .with_icon("folder")
        .with_location(the_location(data, demo, theme));
    let home = RibbonTab::create("Home")
        .with_icon("home")
        .with_group(
            RibbonGroup::create("Navigate")
                .with_item(command("arrow_back", "Back"))
                .with_item(command("arrow_forward", "Forward"))
                .with_item(command("arrow_upward", "Up")),
        )
        .with_group(
            RibbonGroup::create("Organize")
                .with_item(command("create_new_folder", "New folder"))
                .with_item(command("drive_file_rename_outline", "Rename"))
                .with_item(command("delete", "Delete")),
        )
        .with_group(
            RibbonGroup::create("Select")
                .with_item(command("select_all", "All"))
                .with_item(command("deselect", "None")),
        );
    let share = RibbonTab::create("Share").with_icon("share").with_group(
        RibbonGroup::create("Send")
            .with_item(command("link", "Copy link"))
            .with_item(command("mail", "Mail")),
    );
    Ribbon::create(vec![file, home, share])
        .with_active_tab(demo.tab)
        .with_theme(theme)
        .with_tab_style(if demo.form == 1 { TabStyle::TouchPills } else { TabStyle::Auto })
        .with_available_width(if demo.form == 1 { 411.0 } else { 800.0 })
        .with_on_tab_click(data.clone(), on_ribbon_tab as RibbonOnTabClickCallbackType)
        .dom_desktop()
}

/// The frame itself, in a box of the size of a desktop window or of a phone.
fn the_frame(data: &RefAny, demo: &SidebarDemo, theme: UiTheme) -> Dom {
    let body = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; min-width: 0px;")
        .with_child(the_list(data, demo));
    let inspector = Inspector::create(Dom::create_div(), the_preview(demo))
        .with_title("Preview")
        .with_open(demo.inspector_open)
        .with_size(demo.inspector_size)
        .with_surface(surface_of(demo))
        .with_form(form_of(demo))
        .with_theme(theme)
        .with_on_event(data.clone(), on_inspector as InspectorOnEventCallbackType);
    let (name, _, size) = FILES[demo.file.min(FILES.len() - 1)];
    let sheet = ActionSheet::create()
        .with_title(name, size)
        .with_item(ActionSheetItem::create("Preview", "visibility", 1))
        .with_item(ActionSheetItem::create("Download", "download", 2))
        .with_item(ActionSheetItem::create("Rename", "edit", 3))
        .with_item(ActionSheetItem::create("Move to trash", "delete", 4).with_danger(true))
        .with_open(demo.sheet_open)
        .with_form(form_of(demo))
        .with_surface(surface_of(demo))
        .with_theme(theme)
        .with_on_event(data.clone(), on_sheet as ActionSheetOnEventCallbackType);
    let frame = AppFrame::create(the_header(data, demo, theme), body)
        .with_sidebar(the_sidebar(data, demo, theme))
        .with_inspector(inspector)
        .with_footer(the_footer(demo))
        .with_sheet(sheet.dom())
        .with_form(form_of(demo));
    let (w, h) = if demo.form == 1 { (411, 760) } else { (880, 520) };
    Dom::create_div()
        .with_id("sidebar-demo-frame")
        .with_css(format!(
            "position: relative; width: {w}px; max-width: 100%; height: {h}px; overflow: hidden; \
             border: 1px solid system:separator; border-radius: 8px; background: system:background;"
        ))
        .with_child(frame.dom())
}

/// A switch of the card: the buttons of one choice, the chosen one primary.
fn choice(
    data: &RefAny,
    theme: UiTheme,
    labels: &[&str],
    chosen: usize,
    ids: &[&str],
    on_click: extern "C" fn(RefAny, CallbackInfo) -> Update,
) -> Dom {
    let mut row = Dom::create_div().with_css("display: flex; flex-direction: row; gap: 6px; flex-wrap: wrap;");
    for (i, label) in labels.iter().enumerate() {
        row = row.with_child(
            Button::with_type(*label, if i == chosen { ButtonType::Primary } else { ButtonType::Default })
                .with_theme(theme)
                .with_on_click(RefAny::new(Pick { data: data.clone(), index: i }), on_click)
                .dom()
                .with_id(ids[i]),
        );
    }
    row
}

/// What a switch button carries: the page's data and which choice it is.
struct Pick {
    data: RefAny,
    index: usize,
}

/// The section's card.
pub(crate) fn sidebar_section(data: &RefAny, demo: &SidebarDemo, theme: UiTheme) -> Dom {
    let one_more = |on: bool| if on { "Close the Inspector" } else { "Open the Inspector" };
    section(
        "Sidebar",
        vec![
            captioned(
                "Window",
                choice(data, theme, &["Desktop", "Phone"], demo.form, &["sidebar-form-desktop", "sidebar-form-phone"], on_form),
            ),
            captioned(
                "Sidebar",
                choice(
                    data,
                    theme,
                    &["Expanded", "Icon rail", "16 px handle"],
                    demo.state,
                    &["sidebar-state-expanded", "sidebar-state-rail", "sidebar-state-handle"],
                    on_state,
                ),
            ),
            captioned(
                "Surface",
                choice(
                    data,
                    theme,
                    &["Theme", "Leather", "Vellum", "Blue jeans", "Aluminium"],
                    demo.surface,
                    &["sidebar-surface-theme", "sidebar-surface-leather", "sidebar-surface-vellum", "sidebar-surface-jeans", "sidebar-surface-aluminium"],
                    on_surface,
                ),
            ),
            captioned(
                "Inspector and sheet",
                Dom::create_div()
                    .with_css("display: flex; flex-direction: row; gap: 6px; flex-wrap: wrap;")
                    .with_child(
                        Button::with_type(one_more(demo.inspector_open), ButtonType::Default)
                            .with_theme(theme)
                            .with_on_click(data.clone(), on_inspector_open as ButtonOnClickCallbackType)
                            .dom()
                            .with_id("sidebar-inspector-toggle"),
                    )
                    .with_child(
                        Button::with_type("Open the quick actions", ButtonType::Default)
                            .with_theme(theme)
                            .with_on_click(data.clone(), on_sheet_open as ButtonOnClickCallbackType)
                            .dom()
                            .with_id("sidebar-sheet-open"),
                    ),
            ),
            captioned("The frame", the_frame(data, demo, theme)),
            Dom::create_p_with_text(format!("Last report: {}", demo.last))
                .with_id("sidebar-demo-last")
                .with_css("margin: 6px 0px 0px 0px; font-size: 12px; color: system:secondary-text;"),
        ],
    )
}

// ---- the reports ----

/// Which button of a switch was pressed.
fn picked(data: &mut RefAny) -> Option<(RefAny, usize)> {
    data.downcast_ref::<Pick>().map(|p| (p.data.clone(), p.index))
}

extern "C" fn on_ribbon_tab(mut data: RefAny, _: CallbackInfo, tab: usize) -> Update {
    keep(&mut data, |s| {
        s.sidebar.tab = tab;
        s.sidebar.last = format!("tab {tab}");
    })
}

extern "C" fn on_form(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some((mut page, i)) = picked(&mut data) else {
        return Update::DoNothing;
    };
    keep(&mut page, |s| {
        s.sidebar.form = i;
        s.sidebar.sheet_open = false;
        // a phone starts with the rail, as every app does
        s.sidebar.state = 1;
    })
}

extern "C" fn on_state(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some((mut page, i)) = picked(&mut data) else {
        return Update::DoNothing;
    };
    keep(&mut page, |s| s.sidebar.state = i)
}

extern "C" fn on_surface(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some((mut page, i)) = picked(&mut data) else {
        return Update::DoNothing;
    };
    keep(&mut page, |s| s.sidebar.surface = i)
}

extern "C" fn on_inspector_open(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |s| s.sidebar.inspector_open = !s.sidebar.inspector_open)
}

extern "C" fn on_sheet_open(mut data: RefAny, _: CallbackInfo) -> Update {
    keep(&mut data, |s| s.sidebar.sheet_open = true)
}

extern "C" fn on_file(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some((mut page, file)) = data.downcast_ref::<FileRef>().map(|f| (f.data.clone(), f.file)) else {
        return Update::DoNothing;
    };
    keep(&mut page, |s| {
        s.sidebar.file = file;
        s.sidebar.last = format!("file {}", FILES[file].0);
    })
}

/// The sidebar's reports: a row chosen, a triangle, a section, the toggle, help, a pie, collaboration.
extern "C" fn on_sidebar(mut data: RefAny, _: CallbackInfo, event: SidebarEvent) -> Update {
    keep(&mut data, |s| {
        let d = &mut s.sidebar;
        match event.kind {
            SidebarEventKind::RowClicked => {
                d.selected = event.tag;
                d.last = format!("row {} (tag {})", event.index, event.tag);
                // on a phone the drawer closes once a place is chosen
                if d.form == 1 && d.state == 0 {
                    d.state = 1;
                }
            }
            SidebarEventKind::RowToggled => {
                if event.tag == TAG_ASSETS {
                    d.assets_open = event.expand;
                }
                d.last = format!("triangle of row {} opens: {}", event.index, event.expand);
            }
            SidebarEventKind::SectionToggled => {
                d.favourites_open = event.expand;
                d.last = format!("section {} opens: {}", event.index, event.expand);
            }
            SidebarEventKind::StateChanged => {
                d.state = match event.state {
                    SidebarState::Expanded => 0,
                    SidebarState::Rail => 1,
                    SidebarState::Handle => 2,
                };
                d.last = format!("the sidebar asks for state {}", d.state);
            }
            SidebarEventKind::HelpClicked => d.last = String::from("help"),
            SidebarEventKind::RowLongPressed => {
                d.sheet_open = true;
                d.last = format!("long press on row {}", event.index);
            }
            SidebarEventKind::RowStatusClicked => d.last = format!("pie of row {}", event.index),
            SidebarEventKind::RowCollabToggled => {
                let which = if event.tag == TAG_BOOK1 { 0 } else { 1 };
                d.collab[which] = event.expand;
                d.last = format!("collaboration of row {} is {}", event.index, event.expand);
            }
        }
    })
}

/// The location's reports: the pen, every key of the field (the list follows), Enter, Escape.
extern "C" fn on_location(mut data: RefAny, _: CallbackInfo, event: LocationEvent) -> Update {
    keep(&mut data, |s| {
        let d = &mut s.sidebar;
        match event.kind {
            LocationEventKind::EditStarted => {
                d.editing = true;
                d.text = String::new();
                d.last = String::from("the pen");
            }
            LocationEventKind::EditChanged => {
                d.text = event.text.as_str().to_string();
                d.last = format!("typed: {}", d.text);
            }
            LocationEventKind::EditCommitted => {
                d.editing = false;
                d.last = format!("committed: {}", event.text.as_str());
            }
            LocationEventKind::EditCancelled => {
                d.editing = false;
                d.last = String::from("edit given up");
            }
            LocationEventKind::CrumbClicked => d.last = format!("folder {}", event.tag),
        }
    })
}

/// The Inspector's size, as a drag reports it.
extern "C" fn on_inspector(mut data: RefAny, _: CallbackInfo, event: InspectorEvent) -> Update {
    keep(&mut data, |s| {
        s.sidebar.inspector_size = event.size;
        s.sidebar.last = format!("inspector {:?} {}", event.kind, event.size.round());
    })
}

/// The sheet's reports.
extern "C" fn on_sheet(mut data: RefAny, _: CallbackInfo, event: ActionSheetEvent) -> Update {
    keep(&mut data, |s| {
        s.sidebar.sheet_open = false;
        s.sidebar.last = match event.kind {
            ActionSheetEventKind::ItemChosen => format!("sheet row {}", event.tag),
            ActionSheetEventKind::Dismissed => String::from("sheet dismissed"),
        };
    })
}
