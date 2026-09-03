//! The the Office-2013-era look ribbon composition — HOME tab cloned control-by-control.
//!
//! Adapted from the verified `azul/examples/rust/src/ribbon.rs` (pixel-close
//! against the real the Office-2013-era look HOME tab), rebased onto the
//! public `azul::widgets` ribbon API. The FILE app button opens the
//! backstage (`crate::on_file_button`).

use azul::callbacks::{
    ButtonOnClickCallbackType, CallbackInfo, ComboBoxOnSelectCallbackType,
    RibbonGalleryOnSelectCallbackType, RibbonOnTabClickCallbackType, RefAny, Update,
};
use azul::dom::{ComboBoxOnSelectCallback, Dom, RibbonGalleryOnSelectCallback};
use azul::option::OptionRefAny;
use azul::str::String as AzString;
use azul::widgets::{
    ComboBoxState, Ribbon, RibbonAppButton, RibbonArrow, RibbonButton, RibbonColumn,
    RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonRow, RibbonTab,
};

use azul::css::SystemStyle;

use crate::palette::Palette;
use crate::AppState;

// ---------------------------------------------------------------------------
// Callbacks
// ---------------------------------------------------------------------------

extern "C" fn on_tab_click(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.ribbon_tab = index;
    Update::RefreshDom
}

extern "C" fn on_style_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    // Gallery cells: 0 Normal, 1 No Spacing, 2 Heading 1, 3 Heading 2,
    // 4 Title, 5 Subtitle, 6 Subtle Emphasis, 7 Emphasis. Paragraph styles
    // apply to the block(s) the selection/caret sits in; the two character
    // styles toggle italic over the selection.
    use crate::ir::{FormatAxis, IrParaStyle};
    let para_style = match index {
        0 | 1 => Some(IrParaStyle::Body),
        2 => Some(IrParaStyle::Heading(1)),
        3 => Some(IrParaStyle::Heading(2)),
        4 => Some(IrParaStyle::Heading(1)), // Title renders as the top heading
        5 => Some(IrParaStyle::Heading(3)), // Subtitle as the third level
        _ => None,
    };
    let update = match para_style {
        Some(style) => {
            let Some(mut state) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            let mut changed = crate::sync_ir_text_from_engine(&mut state, &mut info);
            // Target blocks: every block the selection touches, else the
            // caret's block.
            let mut blocks: Vec<usize> = Vec::new();
            let spans = info.get_document_selection();
            for span in spans.as_ref() {
                if let Some((b, _)) = crate::map_node_to_block(&state, &mut info, span.node) {
                    if !blocks.contains(&b) {
                        blocks.push(b);
                    }
                }
            }
            if blocks.is_empty() {
                if let Some(caret) = info.get_document_caret().into_option() {
                    if let Some((b, _)) = crate::map_node_to_block(&state, &mut info, caret.node) {
                        blocks.push(b);
                    }
                }
            }
            for b in blocks {
                changed |= crate::ir::set_block_style(&mut state.document.ir, b, style.clone());
            }
            if changed {
                state.document.refresh_derived();
                state.document.dirty = true;
            }
            Update::RefreshDom
        }
        None => crate::apply_format_axis(&mut data, &mut info, FormatAxis::Italic),
    };
    if let Some(mut state) = data.downcast_mut::<AppState>() {
        state.selected_style = index;
    }
    let _ = update;
    Update::RefreshDom
}

extern "C" fn on_toggle_bold(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = crate::apply_format_axis(&mut data, &mut info, crate::ir::FormatAxis::Bold);
    if let Some(mut state) = data.downcast_mut::<AppState>() {
        state.bold = !state.bold;
    }
    let _ = update;
    Update::RefreshDom
}

extern "C" fn on_toggle_italic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = crate::apply_format_axis(&mut data, &mut info, crate::ir::FormatAxis::Italic);
    if let Some(mut state) = data.downcast_mut::<AppState>() {
        state.italic = !state.italic;
    }
    let _ = update;
    Update::RefreshDom
}

extern "C" fn on_toggle_underline(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let update = crate::apply_format_axis(&mut data, &mut info, crate::ir::FormatAxis::Underline);
    if let Some(mut state) = data.downcast_mut::<AppState>() {
        state.underline = !state.underline;
    }
    let _ = update;
    Update::RefreshDom
}

/// Payload for the four alignment buttons: shared app state + this button's
/// alignment index.
struct AlignPayload {
    app: RefAny,
    align: usize,
}

extern "C" fn on_align(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(payload) = data.downcast_ref::<AlignPayload>() else {
        return Update::DoNothing;
    };
    let align = payload.align;
    let mut app = payload.app.clone();
    drop(payload);
    let Some(mut state) = app.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.align = align;
    Update::RefreshDom
}

extern "C" fn on_font_select(_: RefAny, _: CallbackInfo, state: ComboBoxState) -> Update {
    println!(
        "[azwriter] font changed: {} (index {})",
        state.text.as_str(),
        state.selected
    );
    Update::DoNothing
}

// ---------------------------------------------------------------------------
// Small builder helpers
// ---------------------------------------------------------------------------

fn s(v: &str) -> AzString {
    AzString::from(v)
}

fn small(icon: &str, label: &str) -> RibbonButton {
    RibbonButton::new(s(icon), s(label))
}

fn item(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::SmallButton(small(icon, label))
}

fn item_menu(icon: &str, label: &str) -> RibbonItem {
    RibbonItem::SmallButton(small(icon, label).with_arrow(RibbonArrow::Menu))
}

fn row(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Row(
        items
            .into_iter()
            .fold(RibbonRow::new(), |r, it| r.with_item(it)),
    )
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(
        items
            .into_iter()
            .fold(RibbonColumn::new(), |c, it| c.with_item(it)),
    )
}

fn cell(preview_css: String, sample: &str, name: &str) -> RibbonGalleryCell {
    // The preview sits next to the cell's <p> label, so it needs a box of its
    // own — a DIV, which (unlike <p>) adds no UA margins to the sample.
    RibbonGalleryCell::new(
        Dom::create_div_with_text(sample).with_css(preview_css),
        s(name),
    )
}

// ---------------------------------------------------------------------------
// The HOME tab (the the Office-2013-era look default tab, cloned control by control)
// ---------------------------------------------------------------------------

fn home_tab(state: &AppState, data: &RefAny, pal: &Palette, sys: &SystemStyle) -> RibbonTab {
    // The ribbon chrome is the app's biggest painted surface: the desktop
    // supplies its neutrals, so the tab strip and every control in it match
    // the session instead of being a white rectangle pasted into a dark one -
    // while the FILE button and the active-tab accent stay AzWriter's brand.
    let ribbon_style = crate::palette::widgets::ribbon(pal, sys);

    // -- Clipboard ---------------------------------------------------------
    let clipboard = RibbonGroup::new(s("Clipboard"))
        .with_item(RibbonItem::LargeButton(
            RibbonButton::new(s("content_paste"), s("Paste")).with_arrow(RibbonArrow::Split),
        ))
        .with_item(column(vec![
            item("content_cut", "Cut"),
            item("content_copy", "Copy"),
            item("format_paint", "Format Painter"),
        ]));

    // -- Font ----------------------------------------------------------------
    let font_names: Vec<AzString> = [
        "Calibri (Body)",
        "Calibri Light",
        "Cambria",
        "Arial",
        "Courier New",
        "Times New Roman",
    ]
    .iter()
    .map(|f| s(f))
    .collect();
    let font_sizes: Vec<AzString> = ["8", "9", "10", "11", "12", "14", "18", "24", "36"]
        .iter()
        .map(|f| s(f))
        .collect();

    let mut name_combo = ribbon_style.styled_combo_box(font_names, s("Calibri (Body)"), 133);
    name_combo.set_on_select(
        data.clone(),
        ComboBoxOnSelectCallback {
            cb: on_font_select as ComboBoxOnSelectCallbackType,
            callable: OptionRefAny::None,
        },
    );
    let mut size_combo = ribbon_style.styled_combo_box(font_sizes, s("11"), 45);
    // WORKAROUND(engine): pin the static UI font (see crate::fonts).
    crate::fonts::push_ui_font(&mut name_combo.text_style);
    crate::fonts::push_ui_font(&mut size_combo.text_style);

    let mut bold = small("format_bold", "").with_toggled(state.bold);
    bold.set_on_click(data.clone(), on_toggle_bold as ButtonOnClickCallbackType);
    let mut italic = small("format_italic", "").with_toggled(state.italic);
    italic.set_on_click(data.clone(), on_toggle_italic as ButtonOnClickCallbackType);
    let mut underline = small("format_underlined", "")
        .with_toggled(state.underline)
        .with_arrow(RibbonArrow::Menu);
    underline.set_on_click(
        data.clone(),
        on_toggle_underline as ButtonOnClickCallbackType,
    );

    let font = RibbonGroup::new(s("Font")).with_item(column(vec![
        row(vec![
            RibbonItem::Combo(name_combo),
            RibbonItem::Combo(size_combo),
            item("text_increase", ""),
            item("text_decrease", ""),
            item_menu("text_fields", ""),
            item("format_clear", ""),
        ]),
        row(vec![
            RibbonItem::SmallButton(bold),
            RibbonItem::SmallButton(italic),
            RibbonItem::SmallButton(underline),
            item("strikethrough_s", ""),
            item("subscript", ""),
            item("superscript", ""),
            RibbonItem::Separator,
            item_menu("format_shapes", ""),
            item_menu("border_color", ""),
            item_menu("format_color_text", ""),
        ]),
    ]));

    // -- Paragraph -----------------------------------------------------------
    let align_icons = [
        "format_align_left",
        "format_align_center",
        "format_align_right",
        "format_align_justify",
    ];
    let mut align_items: Vec<RibbonItem> = Vec::new();
    for (i, icon) in align_icons.iter().enumerate() {
        let mut b = small(icon, "").with_toggled(state.align == i);
        b.set_on_click(
            RefAny::new(AlignPayload {
                app: data.clone(),
                align: i,
            }),
            on_align as ButtonOnClickCallbackType,
        );
        align_items.push(RibbonItem::SmallButton(b));
    }
    let mut para_row2 = align_items;
    para_row2.push(RibbonItem::Separator);
    para_row2.push(item_menu("format_line_spacing", ""));
    para_row2.push(RibbonItem::Separator);
    para_row2.push(item_menu("format_color_fill", ""));
    para_row2.push(item_menu("border_all", ""));

    let paragraph = RibbonGroup::new(s("Paragraph")).with_item(column(vec![
        row(vec![
            item_menu("format_list_bulleted", ""),
            item_menu("format_list_numbered", ""),
            item_menu("format_list_numbered_rtl", ""),
            RibbonItem::Separator,
            item("format_indent_decrease", ""),
            item("format_indent_increase", ""),
            RibbonItem::Separator,
            item("sort_by_alpha", ""),
            item("", "\u{00b6}"),
        ]),
        row(para_row2),
    ]));

    // -- Styles (in-ribbon gallery) -------------------------------------------
    // The gallery previews are SAMPLES OF THE DOCUMENT STYLES, and a document
    // style is a fixed thing: "Heading 1 is 2E74B5" does not become something
    // else because the desktop theme did. They are the one part of this app's
    // chrome that is deliberately NOT themed (user ruling) - the preview has
    // to show what the style will actually be.
    let cells = vec![
        cell(
            "font-size: 14px; color: #444444;".to_string(),
            "AaBbCcDc",
            "\u{00b6} Normal",
        ),
        cell(
            "font-size: 14px; color: #444444;".to_string(),
            "AaBbCcDc",
            "\u{00b6} No Spac...",
        ),
        cell(
            "font-size: 15px; color: #2e74b5;".to_string(),
            "AaBbCc",
            "Heading 1",
        ),
        cell(
            "font-size: 14px; color: #2e74b5;".to_string(),
            "AaBbCcD",
            "Heading 2",
        ),
        cell("font-size: 19px; color: #262626;".to_string(), "AaB", "Title"),
        cell(
            "font-size: 13px; color: #5a5a5a;".to_string(),
            "AaBbCcD",
            "Subtitle",
        ),
        cell(
            "font-size: 13px; color: #808080;".to_string(),
            "AaBbCcDi",
            "Subtle Em...",
        ),
        cell(
            "font-size: 13px; color: #4472c4;".to_string(),
            "AaBbCcDi",
            "Emphasis",
        ),
    ];
    let mut gallery = RibbonGallery::new(cells).with_selected(state.selected_style);
    gallery.set_on_select(
        data.clone(),
        RibbonGalleryOnSelectCallback {
            cb: on_style_select as RibbonGalleryOnSelectCallbackType,
            callable: OptionRefAny::None,
        },
    );

    let styles = RibbonGroup::new(s("Styles"))
        .with_item(RibbonItem::Gallery(gallery))
        .with_fills_space(true);

    // -- Editing ---------------------------------------------------------------
    let editing = RibbonGroup::new(s("Editing")).with_item(column(vec![
        item_menu("search", "Find"),
        item("find_replace", "Replace"),
        item_menu("highlight_alt", "Select"),
    ]));

    RibbonTab::new(s("HOME"))
        .with_group(clipboard)
        .with_group(font)
        .with_group(paragraph)
        .with_group(styles)
        .with_group(editing)
}

/// The non-HOME tabs only exist as switchable headers with placeholder
/// content — the HOME tab is the cloning target.
fn placeholder_tab(label: &str) -> RibbonTab {
    RibbonTab::new(s(label)).with_group(RibbonGroup::new(s("Preview")).with_item(
        RibbonItem::LargeButton(RibbonButton::new(s("layers"), s(label))),
    ))
}

/// Builds the full ribbon (tab strip + active tab content) for the editor
/// screen. The FILE button opens the backstage.
pub fn build(state: &AppState, data: &RefAny, pal: &Palette, sys: &SystemStyle) -> Dom {
    let tabs: Vec<RibbonTab> = vec![
        home_tab(state, data, pal, sys),
        placeholder_tab("INSERT"),
        placeholder_tab("DESIGN"),
        placeholder_tab("PAGE LAYOUT"),
        placeholder_tab("REFERENCES"),
        placeholder_tab("MAILINGS"),
        placeholder_tab("REVIEW"),
        placeholder_tab("VIEW"),
    ];

    let mut ribbon = Ribbon::new(tabs)
        .with_app_button(RibbonAppButton::new(s("FILE")).with_on_click(
            data.clone(),
            crate::on_file_button as ButtonOnClickCallbackType,
        ))
        .with_active_tab(state.ribbon_tab);
    ribbon.style = crate::palette::widgets::ribbon(pal, sys);
    ribbon.set_on_tab_click(data.clone(), on_tab_click as RibbonOnTabClickCallbackType);
    // WORKAROUND(engine): pin the static UI font on the ribbon container —
    // the font inherits into every tab / group / label (see crate::fonts).
    crate::fonts::push_ui_font(&mut ribbon.style.container_style);
    ribbon.dom_desktop()
}
