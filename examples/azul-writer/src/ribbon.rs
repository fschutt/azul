//! The ribbon: FILE (the backstage), HOME (font, paragraph, styles,
//! editing), INSERT (table, rule, page break), VIEW (print / web layout,
//! zoom). Every button runs a [`Command`]; the format, list, alignment and
//! style buttons show the caret's state (the editor's
//! `is_current_format` / `is_current_kind`), not a flag of their own.

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, RibbonGalleryOnSelectCallbackType,
        RibbonOnTabClickCallbackType, Update,
    },
    dom::Dom,
    str::String as AzString,
    widgets::{
        RichAlign, RichBlockKind, RichCheck, RichFormat, RichTableSize, RichTextCommand,
        RichTextEditorState, Ribbon, RibbonAppButton, RibbonGallery, RibbonGalleryCell, RibbonItem,
        RibbonTab,
    },
};
use azul_appkit::ribbon::{button, column, group, large, row, small, toggle, RibbonCommand};

use crate::{
    app::{command, AppState, BackstagePage, Command, View, TABS},
    commands::on_command,
};

fn s(text: &str) -> AzString {
    AzString::from(text)
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

/// The paragraph styles of the gallery, in its order.
pub const STYLES: [(&str, &str); 6] = [
    ("Normal", "font-size: 13px;"),
    ("Heading 1", "font-size: 17px; font-weight: bold;"),
    ("Heading 2", "font-size: 15px; font-weight: bold;"),
    ("Heading 3", "font-size: 14px; font-weight: bold;"),
    ("Quote", "font-size: 13px; font-style: italic;"),
    ("Code", "font-size: 12px; font-family: monospace;"),
];

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

extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::commands::run(&mut data, Command::RibbonTab(index), &mut info)
}

fn home_tab(app: &RefAny, editor: Option<&RichTextEditorState>) -> RibbonTab {
    let format_on = |f: RichFormat| editor.is_some_and(|e| e.is_current_format(f));
    let kind_on = |k: RichBlockKind| editor.is_some_and(|e| e.is_current_kind(k));
    let quoted = editor.is_some_and(RichTextEditorState::is_current_quoted);
    let align = editor.map_or(RichAlign::Left, current_align);

    let font = group(
        "Font",
        vec![row(vec![
            toggle(app, "format_bold", "Bold", Command::format(RichFormat::Bold), format_on(RichFormat::Bold)),
            toggle(app, "format_italic", "Italic", Command::format(RichFormat::Italic), format_on(RichFormat::Italic)),
            toggle(
                app,
                "format_underlined",
                "Underline",
                Command::format(RichFormat::Underline),
                format_on(RichFormat::Underline),
            ),
            toggle(
                app,
                "strikethrough_s",
                "Strikethrough",
                Command::format(RichFormat::Strike),
                format_on(RichFormat::Strike),
            ),
            toggle(app, "code", "Code", Command::format(RichFormat::Code), format_on(RichFormat::Code)),
        ])],
    );

    let paragraph = group(
        "Paragraph",
        vec![column(vec![
            row(vec![
                toggle(
                    app,
                    "format_list_bulleted",
                    "Bullets",
                    Command::kind(RichBlockKind::Bullet(0)),
                    kind_on(RichBlockKind::Bullet(0)),
                ),
                toggle(
                    app,
                    "format_list_numbered",
                    "Numbering",
                    Command::kind(RichBlockKind::Numbered(0)),
                    kind_on(RichBlockKind::Numbered(0)),
                ),
                toggle(
                    app,
                    "checklist",
                    "Checklist",
                    Command::kind(RichBlockKind::Check(RichCheck {
                        indent: 0,
                        checked: false,
                    })),
                    kind_on(RichBlockKind::Check(RichCheck {
                        indent: 0,
                        checked: false,
                    })),
                ),
                small(app, "format_indent_decrease", "Outdent", Command::Rich(RichTextCommand::Outdent)),
                small(app, "format_indent_increase", "Indent", Command::Rich(RichTextCommand::Indent)),
            ]),
            row(vec![
                toggle(app, "format_align_left", "Left", Command::align(RichAlign::Left), align == RichAlign::Left),
                toggle(
                    app,
                    "format_align_center",
                    "Center",
                    Command::align(RichAlign::Center),
                    align == RichAlign::Center,
                ),
                toggle(app, "format_align_right", "Right", Command::align(RichAlign::Right), align == RichAlign::Right),
                toggle(
                    app,
                    "format_align_justify",
                    "Justify",
                    Command::align(RichAlign::Justify),
                    align == RichAlign::Justify,
                ),
                toggle(app, "format_quote", "Quote", Command::Rich(RichTextCommand::ToggleQuote), quoted),
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
    let styles = group("Styles", vec![RibbonItem::Gallery(gallery)]);

    let mut undo = button(app, "undo", "Undo", Command::Rich(RichTextCommand::Undo));
    if !editor.is_some_and(RichTextEditorState::can_undo) {
        undo = undo.with_disabled(s("Nothing to undo"));
    }
    let mut redo = button(app, "redo", "Redo", Command::Rich(RichTextCommand::Redo));
    if !editor.is_some_and(RichTextEditorState::can_redo) {
        redo = redo.with_disabled(s("Nothing to redo"));
    }
    let editing = group(
        "Editing",
        vec![column(vec![RibbonItem::SmallButton(undo), RibbonItem::SmallButton(redo)])],
    );

    RibbonTab::create(s(TABS[0]))
        .with_group(font)
        .with_group(paragraph)
        .with_group(styles)
        .with_group(editing)
}

fn insert_tab(app: &RefAny) -> RibbonTab {
    let tables = group(
        "Tables",
        vec![large(
            app,
            "table_chart",
            "Table",
            Command::Rich(RichTextCommand::InsertTable(RichTableSize { rows: 3, columns: 3 })),
        )],
    );
    let pages = group(
        "Pages",
        vec![
            large(app, "insert_page_break", "Page Break", Command::Rich(RichTextCommand::InsertPageBreak)),
            large(app, "horizontal_rule", "Line", Command::Rich(RichTextCommand::InsertRule)),
        ],
    );
    RibbonTab::create(s(TABS[1])).with_group(tables).with_group(pages)
}

fn view_tab(app: &RefAny, st: &AppState) -> RibbonTab {
    let views = group(
        "Views",
        vec![
            RibbonItem::LargeButton(
                button(app, View::Print.icon(), "Print Layout", Command::View(View::Print))
                    .with_toggled(st.view == View::Print),
            ),
            RibbonItem::LargeButton(
                button(app, View::Web.icon(), "Web Layout", Command::View(View::Web))
                    .with_toggled(st.view == View::Web),
            ),
        ],
    );
    let zoom = group(
        "Zoom",
        vec![column(vec![
            small(app, "zoom_in", "Zoom In", Command::Zoom(10)),
            small(app, "zoom_out", "Zoom Out", Command::Zoom(-10)),
            small(app, "fit_screen", "100%", Command::Zoom(0)),
        ])],
    );
    let help = group(
        "Window",
        vec![
            large(app, "settings", "Settings", Command::Settings),
            large(app, "info", "About", Command::About),
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
        .with_app_button(RibbonAppButton::create(s("FILE")).with_on_click(
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
