//! The ribbon: FILE (the backstage), HOME (font, paragraph, styles,
//! editing), INSERT (table, rule, page break), VIEW (print / web layout,
//! zoom). Every button runs a [`Command`]; the format, list, alignment and
//! style buttons show the caret's state (the editor's
//! `is_current_format` / `is_current_kind`), not a flag of their own.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, DropDownOnChoiceChangeCallbackType, RefAny,
        RibbonGalleryOnSelectCallbackType, RibbonOnTabClickCallbackType, Update,
    },
    dom::Dom,
    str::String as AzString,
    widgets::{
        DropDown, RichAlign, RichBlockKind, RichCheck, RichFormat, RichTableSize, RichTextCommand,
        RichTextEditorState, Ribbon, RibbonAppButton, RibbonGallery, RibbonGalleryCell, RibbonItem,
        RibbonTab,
    },
};
use azul_appkit::ribbon::{button, column, group, large, row, small, toggle, RibbonCommand};

use crate::{
    app::{command, AppState, BackstagePage, Command, View, TABS},
    commands::on_command,
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

/// The paragraph styles of the gallery (their names keys of the resources), in its order.
pub const STYLES: [(&str, &str); 6] = [
    ("azwriter-style-normal", "font-size: 13px;"),
    ("azwriter-style-heading-1", "font-size: 17px; font-weight: bold;"),
    ("azwriter-style-heading-2", "font-size: 15px; font-weight: bold;"),
    ("azwriter-style-heading-3", "font-size: 14px; font-weight: bold;"),
    ("azwriter-style-quote", "font-size: 13px; font-style: italic;"),
    ("azwriter-style-code", "font-size: 12px; font-family: monospace;"),
];

/// The Font group's families: a key of the resources and the CSS family it sets (empty: the
/// paper's own face).
pub const FONT_FAMILIES: [(&str, &str); 4] = [
    ("azwriter-font-default", ""),
    ("azwriter-font-serif", "serif"),
    ("azwriter-font-sans-serif", "sans-serif"),
    ("azwriter-font-monospace", "monospace"),
];

/// The Font group's sizes in px, Word's list around the paper's 13 px.
pub const FONT_SIZES: [f32; 15] =
    [8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 16.0, 18.0, 20.0, 24.0, 28.0, 36.0, 48.0, 72.0];

/// The paper's text size (the Normal style), shown while the caret's text has no size of its
/// own.
pub const DEFAULT_FONT_SIZE: f32 = 13.0;

/// The family combo's entry for the caret's `family` (the default one for an unknown family).
#[must_use]
pub fn font_family_index(family: &str) -> usize {
    let family = family.trim();
    FONT_FAMILIES
        .iter()
        .position(|(_, css)| css.eq_ignore_ascii_case(family))
        .unwrap_or(0)
}

/// The size combo's entry for the caret's size `px` (0: the paper's): the nearest size of the
/// list, the smaller of two as near.
#[must_use]
pub fn font_size_index(px: f32) -> usize {
    let px = if px > 0.0 { px } else { DEFAULT_FONT_SIZE };
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
        Some(st) => match st.doc.as_ref() {
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
    let cmd = RichTextCommand::SetFontFamily(AzString::from(*css));
    crate::commands::run(&mut app, Command::Rich(cmd), &mut info)
}

/// A size picked in the Font group's combo.
extern "C" fn on_font_size(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut app) = data.downcast_ref::<GalleryRef>().map(|g| g.app.clone()) else {
        return Update::DoNothing;
    };
    let Some(px) = FONT_SIZES.get(index).copied() else {
        return Update::DoNothing;
    };
    crate::commands::run(&mut app, Command::Rich(RichTextCommand::SetFontSize(px)), &mut info)
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
        .with_selected(editor.map_or(0, |e| font_family_index(e.current_font_family().as_str())))
        .with_accessibility_name(s("azwriter-font-family"))
        .with_on_choice_change(
            RefAny::new(GalleryRef { app: app.clone() }),
            on_font_family as DropDownOnChoiceChangeCallbackType,
        );
    let size_labels: Vec<String> = FONT_SIZES.iter().map(|px| format!("{px}")).collect();
    let size_refs: Vec<&str> = size_labels.iter().map(String::as_str).collect();
    let sizes = DropDown::create(azul_appkit::l10n::labels(&size_refs))
        .with_selected(font_size_index(editor.map_or(0.0, |e| e.current_font_size())))
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
            toggle(app, "format_bold", "azwriter-bold", Command::format(RichFormat::Bold), format_on(RichFormat::Bold)),
            toggle(app, "format_italic", "azwriter-italic", Command::format(RichFormat::Italic), format_on(RichFormat::Italic)),
            toggle(
                app,
                "format_underlined",
                "azwriter-underline",
                Command::format(RichFormat::Underline),
                format_on(RichFormat::Underline),
            ),
            toggle(
                app,
                "strikethrough_s",
                "azwriter-strikethrough",
                Command::format(RichFormat::Strike),
                format_on(RichFormat::Strike),
            ),
            toggle(app, "code", "azwriter-style-code", Command::format(RichFormat::Code), format_on(RichFormat::Code)),
            ]),
        ])],
    );

    let paragraph = group(
        "azwriter-group-paragraph",
        vec![column(vec![
            row(vec![
                toggle(
                    app,
                    "format_list_bulleted",
                    "azwriter-bullets",
                    Command::kind(RichBlockKind::Bullet(0)),
                    kind_on(RichBlockKind::Bullet(0)),
                ),
                toggle(
                    app,
                    "format_list_numbered",
                    "azwriter-numbering",
                    Command::kind(RichBlockKind::Numbered(0)),
                    kind_on(RichBlockKind::Numbered(0)),
                ),
                toggle(
                    app,
                    "checklist",
                    "azwriter-checklist",
                    Command::kind(RichBlockKind::Check(RichCheck {
                        indent: 0,
                        checked: false,
                    })),
                    kind_on(RichBlockKind::Check(RichCheck {
                        indent: 0,
                        checked: false,
                    })),
                ),
                small(app, "format_indent_decrease", "azwriter-outdent", Command::Rich(RichTextCommand::Outdent)),
                small(app, "format_indent_increase", "azwriter-indent", Command::Rich(RichTextCommand::Indent)),
            ]),
            row(vec![
                toggle(app, "format_align_left", "azwriter-left", Command::align(RichAlign::Left), align == RichAlign::Left),
                toggle(
                    app,
                    "format_align_center",
                    "azwriter-center",
                    Command::align(RichAlign::Center),
                    align == RichAlign::Center,
                ),
                toggle(app, "format_align_right", "azwriter-right", Command::align(RichAlign::Right), align == RichAlign::Right),
                toggle(
                    app,
                    "format_align_justify",
                    "azwriter-justify",
                    Command::align(RichAlign::Justify),
                    align == RichAlign::Justify,
                ),
                toggle(app, "format_quote", "azwriter-style-quote", Command::Rich(RichTextCommand::ToggleQuote), quoted),
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

    RibbonTab::create(s(TABS[0]))
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
    RibbonTab::create(s(TABS[1])).with_group(tables).with_group(pages)
}

fn view_tab(app: &RefAny, st: &AppState) -> RibbonTab {
    let views = group(
        "azwriter-group-views",
        vec![
            RibbonItem::LargeButton(
                button(app, View::Print.icon(), "azwriter-print-layout", Command::View(View::Print))
                    .with_toggled(st.view == View::Print),
            ),
            RibbonItem::LargeButton(
                button(app, View::Web.icon(), "azwriter-web-layout", Command::View(View::Web))
                    .with_toggled(st.view == View::Web),
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
    RibbonTab::create(s(TABS[2])).with_group(views).with_group(zoom).with_group(help)
}

/// The ribbon for the app's state.
#[must_use]
pub fn ribbon(app: &RefAny, st: &AppState) -> Dom {
    let editor = st.doc.as_ref().map(|d| &d.editor);
    let tabs = vec![home_tab(app, editor), insert_tab(app), view_tab(app, st)];
    let active = st.ribbon_tab.min(tabs.len() - 1);
    Ribbon::create(tabs)
        .with_app_button(RibbonAppButton::create(s("azwriter-tab-file")).with_on_click(
            command(app, Command::OpenBackstage(BackstagePage::Info)),
            on_command as ButtonOnClickCallbackType,
        ))
        .with_active_tab(active)
        .with_on_tab_click(app.clone(), on_tab_click as RibbonOnTabClickCallbackType)
        // No title row over the ribbon: its tabs are the title bar.
        .with_tabs_in_titlebar(azul_appkit::ui::tabs_in_titlebar())
        .dom_desktop()
}

#[cfg(test)]
mod tests {
    use super::*;
    use azul::widgets::RichTextDoc;

    fn editor_on(markdown: &str) -> RichTextEditorState {
        RichTextEditorState::create(RichTextDoc::create_from_markdown(markdown))
    }

    /// The words inside `dom`.
    fn texts(dom: &Dom) -> Vec<String> {
        let mut out: Vec<String> = dom
            .root
            .node_type
            .get_text()
            .into_option()
            .map(|t| t.as_str().to_string())
            .into_iter()
            .collect();
        for child in dom.children.as_slice() {
            out.extend(texts(child));
        }
        out
    }

    /// The File tab is said in the window's language, as the tabs beside it are: DATEI, not
    /// FILE, in a German window.
    #[test]
    fn the_file_tab_is_said_in_the_windows_language() {
        crate::l10n::in_english();
        let st = AppState::new(RefAny::new(()), std::path::PathBuf::from("data"), false);
        azul_appkit::l10n::set_locale("de-DE");
        let shown = texts(&ribbon(&RefAny::new(()), &st));
        azul_appkit::l10n::set_locale("en-US");
        assert!(shown.iter().any(|t| t == "DATEI"), "{shown:?}");
        assert!(shown.iter().any(|t| t == "START"), "the tab beside it: {shown:?}");
        assert!(!shown.iter().any(|t| t == "FILE"), "{shown:?}");
    }

    /// Word 2010's HOME tab starts with the Clipboard group: Paste, Cut and Copy.
    #[test]
    fn the_home_tab_starts_with_the_clipboard_group() {
        crate::l10n::in_english();
        let st = AppState::new(RefAny::new(()), std::path::PathBuf::from("data"), false);
        let shown = texts(&ribbon(&RefAny::new(()), &st));
        for word in ["Clipboard", "Paste", "Cut", "Copy"] {
            assert!(shown.iter().any(|t| t == word), "{word} is missing: {shown:?}");
        }
    }

    #[test]
    fn the_font_combos_show_the_carets_family_and_the_nearest_size() {
        assert_eq!(font_family_index(""), 0, "the paper's face");
        assert_eq!(FONT_FAMILIES[font_family_index("monospace")].1, "monospace");
        assert_eq!(font_family_index("Comic Sans"), 0, "an unknown family shows the default");
        assert_eq!(FONT_SIZES[font_size_index(0.0)], DEFAULT_FONT_SIZE, "no size of its own");
        assert_eq!(FONT_SIZES[font_size_index(13.0)], 13.0);
        assert_eq!(FONT_SIZES[font_size_index(15.0)], 14.0, "between 14 and 16: the smaller");
        assert_eq!(FONT_SIZES[font_size_index(17.5)], 18.0);
        assert_eq!(FONT_SIZES[font_size_index(500.0)], 72.0);
    }

    #[test]
    fn every_style_of_the_gallery_is_its_own_kind() {
        // The old gallery mapped its 3rd and 5th cells both to heading 1
        // (DEDUP_EDITORS A3.5).
        let kinds: Vec<Option<RichBlockKind>> = (0..STYLES.len()).map(style_kind).collect();
        assert_eq!(kinds[1], Some(RichBlockKind::Heading(1)));
        assert_eq!(kinds[2], Some(RichBlockKind::Heading(2)));
        assert_eq!(kinds[3], Some(RichBlockKind::Heading(3)));
        assert_eq!(style_kind(STYLES.len()), None);
    }

    #[test]
    fn the_gallery_shows_the_carets_style_and_a_pick_never_toggles_it_off() {
        let heading = editor_on("# Title\n");
        assert_eq!(current_style(&heading), 1);
        assert!(style_commands(&heading, 1).is_empty(), "heading 1 again changes nothing");
        assert_eq!(
            style_commands(&heading, 2),
            vec![RichTextCommand::ToggleKind(RichBlockKind::Heading(2))]
        );
        let quote = editor_on("> words\n");
        assert_eq!(current_style(&quote), 4);
        assert_eq!(style_commands(&quote, 0), vec![RichTextCommand::ToggleQuote]);
        let plain = editor_on("words\n");
        assert_eq!(style_commands(&plain, 4), vec![RichTextCommand::ToggleQuote]);
    }
}
