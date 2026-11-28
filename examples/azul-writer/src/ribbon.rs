//! The ribbon: FILE (the location of the document), HOME (clipboard, font, paragraph, styles,
//! editing), INSERT (table, line, page break), LAYOUT (indent, breaks), REVIEW (word count; what
//! AzWriter does not do yet says why) and VIEW (flow / page layout, zoom, settings). Every button
//! runs a [`Command`]; the format, list, alignment and style buttons show the caret's state (the
//! editor's `is_current_format` / `is_current_kind`), not a flag of their own.
//!
//! The tab row is the window's title bar (no title row): on a phone it is a strip of icon squares
//! in the status bar's band beside the camera cutout (`for_the_phone`, `status_band`), on a desktop
//! the tabs' text. A button shows EITHER its words OR its icon: words on a desktop, icons on a
//! phone (the words are its name and its tooltip there); the formatting toggles of Font and
//! Paragraph have no words, only icons, as in Word.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, DropDownOnChoiceChangeCallbackType, LayoutCallbackInfo, RefAny,
        RibbonGalleryOnSelectCallbackType, RibbonOnTabClickCallbackType, Update,
    },
    css::LogicalSize,
    dom::Dom,
    option::OptionString,
    str::String as AzString,
    widgets::{
        DropDown, RibbonButtonFace, RichAlign, RichBlockKind, RichCheck, RichFormat, RichTableSize,
        RichTextCommand, RichTextEditorState, Ribbon, RibbonGallery, RibbonGalleryCell, RibbonItem,
        RibbonTab, StatusBarBand, TabLabels, TabsInTitlebar,
    },
    window::{SystemBarMode, WindowCreateOptions},
};
use azul_appkit::ribbon::{button, column, group, icon_button, large, row, small, RibbonCommand};

use crate::{
    app::{command, AppState, Command, View, TABS, TAB_ICONS},
    commands::on_command,
    ids, ui_location,
};

/// A key of the resources said, or words as they are (a document's title).
fn s(text: &str) -> AzString {
    azul_appkit::l10n::label(text)
}

/// Every ribbon button runs a [`Command`] through [`on_command`] (azul-appkit's ribbon
/// builder).
impl RibbonCommand for Command {
    fn click_data(self, app: &RefAny) -> RefAny {
        command(app, self)
    }

    fn on_click() -> ButtonOnClickCallbackType {
        on_command
    }
}

/// The paragraph styles of the gallery and of the sidebar's Styles section (their names keys of the
/// resources, the CSS of their sample), in their order.
pub static STYLES: [(&str, &str); 6] = [
    ("azwriter-style-normal", "font-size: 13px;"),
    ("azwriter-style-heading-1", "font-size: 17px; font-weight: bold;"),
    ("azwriter-style-heading-2", "font-size: 15px; font-weight: bold;"),
    ("azwriter-style-heading-3", "font-size: 14px; font-weight: bold;"),
    ("azwriter-style-quote", "font-size: 13px; font-style: italic;"),
    ("azwriter-style-code", "font-size: 12px; font-family: monospace;"),
];

/// The Font group's families: a key of the resources and the CSS family it sets (empty: the
/// paper's own face).
pub static FONT_FAMILIES: [(&str, &str); 4] = [
    ("azwriter-font-default", ""),
    ("azwriter-font-serif", "serif"),
    ("azwriter-font-sans-serif", "sans-serif"),
    ("azwriter-font-monospace", "monospace"),
];

/// The Font group's sizes in points, Word's list.
pub static FONT_SIZES: [f32; 17] = [
    8.0, 9.0, 10.0, 10.5, 11.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 26.0, 28.0, 36.0, 48.0,
    72.0,
];

/// The paper's text size in points (the Normal style, the editors' body size), shown while the
/// caret's text has no size of its own.
pub const DEFAULT_FONT_SIZE: f32 = 11.0;

/// The family combo's entry for the caret's `family` (the default one for an unknown family).
#[must_use]
pub fn font_family_index(family: &str) -> usize {
    let family = family.trim();
    FONT_FAMILIES
        .iter()
        .position(|(_, css)| css.eq_ignore_ascii_case(family))
        .unwrap_or(0)
}

/// The size combo's entry for the caret's size in `points` (0: the paper's): the nearest size
/// of the list, the smaller of two as near.
#[must_use]
pub fn font_size_index(points: f32) -> usize {
    let px = if points > 0.0 { points } else { DEFAULT_FONT_SIZE };
    let mut best = 0;
    for (i, size) in FONT_SIZES.iter().enumerate() {
        if (size - px).abs() < (FONT_SIZES[best] - px).abs() {
            best = i;
        }
    }
    best
}

/// The block kind of gallery style `index` (`None` past the end).
#[must_use]
pub fn style_kind(index: usize) -> Option<RichBlockKind> {
    Some(match index {
        0 | 4 => RichBlockKind::Paragraph,
        1 => RichBlockKind::Heading(1),
        2 => RichBlockKind::Heading(2),
        3 => RichBlockKind::Heading(3),
        5 => RichBlockKind::Code(AzString::from("")),
        _ => return None,
    })
}

/// The gallery style the caret's block shows.
#[must_use]
pub fn current_style(editor: &RichTextEditorState) -> usize {
    match editor.current_kind() {
        RichBlockKind::Heading(1) => 1,
        RichBlockKind::Heading(2) => 2,
        RichBlockKind::Heading(n) if n >= 3 => 3,
        RichBlockKind::Code(_) => 5,
        _ if editor.is_current_quoted() => 4,
        _ => 0,
    }
}

/// The commands a gallery pick runs: the block kind (unless the caret's
/// block already is one - a pick never toggles a style off), and the quote
/// on or off.
#[must_use]
pub fn style_commands(editor: &RichTextEditorState, index: usize) -> Vec<RichTextCommand> {
    let Some(kind) = style_kind(index) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let current = editor.current_kind();
    if current != kind && !(kind == RichBlockKind::Paragraph && !current.has_text()) {
        out.push(RichTextCommand::ToggleKind(kind));
    }
    if (index == 4) != editor.is_current_quoted() {
        out.push(RichTextCommand::ToggleQuote);
    }
    out
}

/// The alignment of the caret's block.
fn current_align(editor: &RichTextEditorState) -> RichAlign {
    editor
        .doc
        .blocks
        .as_slice()
        .get(editor.caret_block)
        .map_or(RichAlign::Left, |b| b.align)
}

struct GalleryRef {
    app: RefAny,
}

extern "C" fn on_style(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut app) = data.downcast_ref::<GalleryRef>().map(|g| g.app.clone()) else {
        return Update::DoNothing;
    };
    let commands = match app.downcast_ref::<AppState>() {
        Some(st) => match st.doc() {
            Some(doc) => style_commands(&doc.editor, index),
            None => return Update::DoNothing,
        },
        None => return Update::DoNothing,
    };
    let mut update = Update::DoNothing;
    for cmd in commands {
        let next = crate::commands::run(&mut app, Command::Rich(cmd), &mut info);
        update.max_self(next);
    }
    update
}

/// A family picked in the Font group's combo: the selection (or what is typed next) is set in
/// it.
extern "C" fn on_font_family(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut app) = data.downcast_ref::<GalleryRef>().map(|g| g.app.clone()) else {
        return Update::DoNothing;
    };
    let Some((_, css)) = FONT_FAMILIES.get(index) else {
        return Update::DoNothing;
    };
    let family = if css.is_empty() {
        OptionString::None
    } else {
        OptionString::Some(AzString::from(*css))
    };
    let cmd = RichTextCommand::SetFontFamily(family);
    crate::commands::run(&mut app, Command::Rich(cmd), &mut info)
}

/// A size picked in the Font group's combo.
extern "C" fn on_font_size(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut app) = data.downcast_ref::<GalleryRef>().map(|g| g.app.clone()) else {
        return Update::DoNothing;
    };
    let Some(points) = FONT_SIZES.get(index).copied() else {
        return Update::DoNothing;
    };
    // The editor keeps sizes in half points (22 is 11 pt), as word processors do.
    let half_points = (points * 2.0).round() as u16;
    crate::commands::run(&mut app, Command::Rich(RichTextCommand::SetFontSize(half_points)), &mut info)
}

extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::commands::run(&mut data, Command::RibbonTab(index), &mut info)
}

fn home_tab(app: &RefAny, editor: Option<&RichTextEditorState>) -> RibbonTab {
    let format_on = |f: RichFormat| editor.is_some_and(|e| e.is_current_format(f));
    let kind_on = |k: RichBlockKind| editor.is_some_and(|e| e.is_current_kind(k));
    let quoted = editor.is_some_and(RichTextEditorState::is_current_quoted);
    let align = editor.map_or(RichAlign::Left, current_align);

    // Word 2010's first group: Paste, and Cut and Copy beside it.
    let clipboard = group(
        "azwriter-group-clipboard",
        vec![
            large(app, "content_paste", "azwriter-paste", Command::Rich(RichTextCommand::Paste)),
            column(vec![
                small(app, "content_cut", "azwriter-cut", Command::Rich(RichTextCommand::Cut)),
                small(app, "content_copy", "azwriter-copy", Command::Rich(RichTextCommand::Copy)),
            ]),
        ],
    );

    // The family and size combos show the caret's; a pick sets the selection's.
    let family_labels: Vec<&str> = FONT_FAMILIES.iter().map(|(name, _)| *name).collect();
    let families = DropDown::create(azul_appkit::l10n::labels(&family_labels))
        .with_selected(editor.map_or(0, |e| match e.current_font().family {
            OptionString::Some(family) => font_family_index(family.as_str()),
            OptionString::None => 0,
        }))
        .with_accessibility_name(s("azwriter-font-family"))
        .with_on_choice_change(
            RefAny::new(GalleryRef { app: app.clone() }),
            on_font_family as DropDownOnChoiceChangeCallbackType,
        );
    let size_labels: Vec<String> = FONT_SIZES.iter().map(|px| format!("{px}")).collect();
    let size_refs: Vec<&str> = size_labels.iter().map(String::as_str).collect();
    let sizes = DropDown::create(azul_appkit::l10n::labels(&size_refs))
        .with_selected(font_size_index(
            editor.map_or(0.0, |e| f32::from(e.current_font().half_points) / 2.0),
        ))
        .with_accessibility_name(s("azwriter-font-size"))
        .with_on_choice_change(
            RefAny::new(GalleryRef { app: app.clone() }),
            on_font_size as DropDownOnChoiceChangeCallbackType,
        );

    let font = group(
        "azwriter-group-font",
        vec![column(vec![
            row(vec![RibbonItem::Drop(families), RibbonItem::Drop(sizes)]),
            row(vec![
                icon_button(app, "format_bold", "azwriter-bold", Command::format(RichFormat::Bold), format_on(RichFormat::Bold)),
                icon_button(app, "format_italic", "azwriter-italic", Command::format(RichFormat::Italic), format_on(RichFormat::Italic)),
                icon_button(
                    app,
                    "format_underlined",
                    "azwriter-underline",
                    Command::format(RichFormat::Underline),
                    format_on(RichFormat::Underline),
                ),
                icon_button(
                    app,
                    "strikethrough_s",
                    "azwriter-strikethrough",
                    Command::format(RichFormat::Strike),
                    format_on(RichFormat::Strike),
                ),
                icon_button(app, "code", "azwriter-style-code", Command::format(RichFormat::Code), format_on(RichFormat::Code)),
            ]),
        ])],
    );

    let check = RichBlockKind::Check(RichCheck {
        indent: 0,
        checked: false,
    });
    let paragraph = group(
        "azwriter-group-paragraph",
        vec![column(vec![
            row(vec![
                icon_button(
                    app,
                    "format_list_bulleted",
                    "azwriter-bullets",
                    Command::kind(RichBlockKind::Bullet(0)),
                    kind_on(RichBlockKind::Bullet(0)),
                ),
                icon_button(
                    app,
                    "format_list_numbered",
                    "azwriter-numbering",
                    Command::kind(RichBlockKind::Numbered(0)),
                    kind_on(RichBlockKind::Numbered(0)),
                ),
                icon_button(app, "checklist", "azwriter-checklist", Command::kind(check.clone()), kind_on(check)),
                icon_button(app, "format_indent_decrease", "azwriter-outdent", Command::Rich(RichTextCommand::Outdent), false),
                icon_button(app, "format_indent_increase", "azwriter-indent", Command::Rich(RichTextCommand::Indent), false),
            ]),
            row(vec![
                icon_button(app, "format_align_left", "azwriter-left", Command::align(RichAlign::Left), align == RichAlign::Left),
                icon_button(
                    app,
                    "format_align_center",
                    "azwriter-center",
                    Command::align(RichAlign::Center),
                    align == RichAlign::Center,
                ),
                icon_button(app, "format_align_right", "azwriter-right", Command::align(RichAlign::Right), align == RichAlign::Right),
                icon_button(
                    app,
                    "format_align_justify",
                    "azwriter-justify",
                    Command::align(RichAlign::Justify),
                    align == RichAlign::Justify,
                ),
                icon_button(app, "format_quote", "azwriter-style-quote", Command::Rich(RichTextCommand::ToggleQuote), quoted),
            ]),
        ])],
    );

    // A style's sample is set in the paper's face (the ribbon's own face is
    // the chrome's); the Code style names its own family after it.
    let cells: Vec<RibbonGalleryCell> = STYLES
        .iter()
        .map(|(name, css)| {
            let css = format!("{} {css}", crate::paginate::PAPER_TEXT_CSS);
            RibbonGalleryCell::create(Dom::create_span_with_text(s("AaBbCc")).with_css(s(&css)), s(name))
        })
        .collect();
    // A row of three styles (the row with the caret's) and More, as Word's
    // gallery: all six inline pushed the Editing group off a 1280 px window.
    let gallery = RibbonGallery::create(cells)
        .with_visible(3)
        .with_selected(editor.map_or(0, current_style))
        .with_on_select(
            RefAny::new(GalleryRef { app: app.clone() }),
            on_style as RibbonGalleryOnSelectCallbackType,
        );
    let styles = group("azwriter-group-styles", vec![RibbonItem::Gallery(gallery)]);

    let mut undo = button(app, "undo", "azwriter-undo", Command::Rich(RichTextCommand::Undo));
    if !editor.is_some_and(RichTextEditorState::can_undo) {
        undo = undo.with_disabled(s("azwriter-nothing-to-undo"));
    }
    let mut redo = button(app, "redo", "azwriter-redo", Command::Rich(RichTextCommand::Redo));
    if !editor.is_some_and(RichTextEditorState::can_redo) {
        redo = redo.with_disabled(s("azwriter-nothing-to-redo"));
    }
    let editing = group(
        "azwriter-group-editing",
        vec![column(vec![RibbonItem::SmallButton(undo), RibbonItem::SmallButton(redo)])],
    );

    RibbonTab::create(s(TABS[1]))
        .with_icon(TAB_ICONS[1])
        .with_group(clipboard)
        .with_group(font)
        .with_group(paragraph)
        .with_group(styles)
        .with_group(editing)
}

fn insert_tab(app: &RefAny) -> RibbonTab {
    let tables = group(
        "azwriter-group-tables",
        vec![large(
            app,
            "table_chart",
            "azwriter-table",
            Command::Rich(RichTextCommand::InsertTable(RichTableSize { rows: 3, columns: 3 })),
        )],
    );
    let pages = group(
        "azwriter-group-pages",
        vec![
            large(app, "insert_page_break", "azwriter-page-break", Command::Rich(RichTextCommand::InsertPageBreak)),
            large(app, "horizontal_rule", "azwriter-line", Command::Rich(RichTextCommand::InsertRule)),
        ],
    );
    RibbonTab::create(s(TABS[2])).with_icon(TAB_ICONS[2]).with_group(tables).with_group(pages)
}

/// LAYOUT: the indent of a list item and the breaks. (The paper is A4 and its margins are an inch:
/// nothing to set yet, so nothing is offered.)
fn layout_tab(app: &RefAny) -> RibbonTab {
    let indent = group(
        "azwriter-group-indent",
        vec![column(vec![
            small(app, "format_indent_decrease", "azwriter-outdent", Command::Rich(RichTextCommand::Outdent)),
            small(app, "format_indent_increase", "azwriter-indent", Command::Rich(RichTextCommand::Indent)),
        ])],
    );
    let breaks = group(
        "azwriter-group-breaks",
        vec![
            large(app, "insert_page_break", "azwriter-page-break", Command::Rich(RichTextCommand::InsertPageBreak)),
            large(app, "horizontal_rule", "azwriter-line", Command::Rich(RichTextCommand::InsertRule)),
        ],
    );
    RibbonTab::create(s(TABS[3])).with_icon(TAB_ICONS[3]).with_group(indent).with_group(breaks)
}

/// REVIEW: the word count, and what AzWriter cannot do yet, greyed with its reason.
fn review_tab(app: &RefAny) -> RibbonTab {
    let proofing = group(
        "azwriter-group-proofing",
        vec![
            large(app, "pin", "azwriter-word-count-button", Command::WordCount),
            RibbonItem::LargeButton(
                button(app, "spellcheck", "azwriter-spelling", Command::WordCount).with_disabled(s("azwriter-no-spelling")),
            ),
        ],
    );
    let comments = group(
        "azwriter-group-comments",
        vec![RibbonItem::LargeButton(
            button(app, "add_comment", "azwriter-new-comment", Command::WordCount).with_disabled(s("azwriter-no-comments")),
        )],
    );
    RibbonTab::create(s(TABS[4])).with_icon(TAB_ICONS[4]).with_group(proofing).with_group(comments)
}

fn view_tab(app: &RefAny, st: &AppState) -> RibbonTab {
    let views = group(
        "azwriter-group-views",
        vec![
            RibbonItem::LargeButton(
                button(app, View::Flow.icon(), "azwriter-flow-layout", Command::View(View::Flow))
                    .with_toggled(st.view == View::Flow),
            ),
            RibbonItem::LargeButton(
                button(app, View::Pages.icon(), "azwriter-page-layout", Command::View(View::Pages))
                    .with_toggled(st.view == View::Pages),
            ),
        ],
    );
    let zoom = group(
        "azwriter-group-zoom",
        vec![column(vec![
            small(app, "zoom_in", "azwriter-zoom-in", Command::Zoom(10)),
            small(app, "zoom_out", "azwriter-zoom-out", Command::Zoom(-10)),
            small(app, "fit_screen", "100%", Command::Zoom(0)),
        ])],
    );
    let help = group(
        "azwriter-group-window",
        vec![
            large(app, "settings", "azwriter-settings", Command::Settings),
            large(app, "info", "azwriter-page-about", Command::About),
        ],
    );
    RibbonTab::create(s(TABS[5])).with_icon(TAB_ICONS[5]).with_group(views).with_group(zoom).with_group(help)
}

/// The room the tab strip keeps at the window's edges for the OS's window controls the sidebar
/// does not cover, the width it is laid out in, and where it stands.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    /// Room before the first tab (macOS's traffic lights, as far as the sidebar leaves them).
    pub left: f32,
    /// Room after the last tab (Linux's and Windows's controls over the strip).
    pub right: f32,
    /// The width the ribbon is laid out in, px.
    pub width: f32,
    /// Whether the window is a phone's: the tabs are icon squares, the buttons icons.
    pub phone: bool,
    /// How far the ribbon's content band keeps from the window's left edge: the phone's rail
    /// stands beside it, below the tab row (`Sidebar::phone_content_inset`); 0 elsewhere.
    pub content_inset: f32,
    /// The status bar's band the tabs stand in, beside the camera, on a phone that draws under its
    /// bars; none elsewhere (the tab row is an ordinary row then).
    pub band: Option<StatusBarBand>,
}

/// The ribbon for the app's state: File (the location) and the five bands, the active tab the
/// app's.
#[must_use]
pub fn ribbon(app: &RefAny, st: &AppState, strip: &Strip) -> Dom {
    let editor = st.doc().map(|d| &d.editor);
    let file = RibbonTab::create(s(TABS[0]))
        .with_icon(TAB_ICONS[0])
        .with_location(ui_location::location(app, st));
    let tabs = vec![
        file,
        home_tab(app, editor),
        insert_tab(app),
        layout_tab(app),
        review_tab(app),
        view_tab(app, st),
    ];
    let active = st.ribbon_tab.min(tabs.len() - 1);
    let platform = azul_appkit::ui::tabs_in_titlebar();
    let mut ribbon = Ribbon::create(tabs)
        .with_active_tab(active)
        .with_on_tab_click(app.clone(), on_tab_click as RibbonOnTabClickCallbackType)
        // No title row over the ribbon: its tabs are the title bar.
        .with_tabs_in_titlebar(TabsInTitlebar::create(platform.top, strip.left.max(0.0), strip.right.max(platform.right)))
        // a phone's tabs are icon squares where every tab names an icon, the desktop's are text
        .with_labels(TabLabels::Auto)
        // a button shows its words OR its icon, never both (the words are its name and its tooltip):
        // words on a desktop, icons on a phone
        .with_button_face(if strip.phone { RibbonButtonFace::Icon } else { RibbonButtonFace::Text })
        // the phone's 18 px rail stands beside the band, under the tab row: the band keeps clear
        .with_content_inset(strip.content_inset)
        // the window's ONE chevron at the left end of the tab row folds the band away
        .with_content_toggle(true)
        .with_available_width(strip.width);
    if let Some(band) = strip.band.clone() {
        ribbon = ribbon.with_status_bar(band);
    }
    ribbon.dom_desktop().with_id(ids::RIBBON)
}

/// Whether the window draws under the system bars: on a phone (an Android or iOS build). Elsewhere the engine keeps
/// the root clear of the insets itself, and the app must not do it twice.
#[must_use]
pub const fn draws_under_the_bars() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// The window of a phone draws under the system bars and into the cutout area, and hides the status bar (a swipe
/// from the top edge shows it for a moment): the tab strip takes the place of the clock and the battery
/// icons, a word processor needs neither while a person writes. Everything below the strip keeps clear of the insets
/// itself (`ui_frame`). Any other window is returned as it is.
#[must_use]
pub fn for_the_phone(mut window: WindowCreateOptions) -> WindowCreateOptions {
    if draws_under_the_bars() {
        let flags = &mut window.window_state.flags;
        flags.extend_into_safe_area = true;
        flags.system_bars.status = SystemBarMode::HiddenTransient;
        flags.system_bars.draw_into_cutout = true;
    }
    window
}

/// The status bar's band of the window now (its height and the lanes beside the camera), for a strip that takes the
/// band's place - or `None` while the system still shows its status bar: a shell that does not hide the bar (the
/// window asked for it in [`for_the_phone`]) must not have a strip drawn over the clock. `None` on a desktop too (a
/// window made narrow): the strip is an ordinary row at its top.
#[must_use]
pub fn status_band(info: &LayoutCallbackInfo) -> Option<StatusBarBand> {
    if !draws_under_the_bars() {
        return None;
    }
    let features = info.get_display_features();
    if features.bars.status.into_option().is_some_and(|bar| bar.visible) {
        return None;
    }
    Some(StatusBarBand::from_display(
        &features,
        &info.get_safe_area_insets(),
        LogicalSize { width: info.get_window_width(), height: info.get_window_height() },
    ))
}

#[cfg(test)]
#[path = "ribbon_test.rs"]
mod ribbon_test;
