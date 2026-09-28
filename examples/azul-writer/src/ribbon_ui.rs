use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, RefAny, RibbonOnTabClickCallbackType, Update,
    },
    css::{ColorU, SystemStyle},
    dom::Dom,
    str::String as AzString,
    widgets::{
        ComboBoxState, Ribbon, RibbonAppButton, RibbonArrow, RibbonButton, RibbonColumn,
        RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem, RibbonRow, RibbonTab,
    },
};

use crate::{palette::Palette, AppState};

extern "C" fn on_tab_click(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
    let Some(mut state) = data.downcast_mut::<AppState>() else {
        return Update::DoNothing;
    };
    state.ribbon_tab = index;
    Update::RefreshDom
}

extern "C" fn on_style_select(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    use crate::ir::{FormatAxis, IrParaStyle};
    let para_style = match index {
        0 | 1 => Some(IrParaStyle::Body),
        2 => Some(IrParaStyle::Heading(1)),
        3 => Some(IrParaStyle::Heading(2)),
        4 => Some(IrParaStyle::Heading(1)),
        5 => Some(IrParaStyle::Heading(3)),
        _ => None,
    };
    let update = match para_style {
        Some(style) => {
            let Some(mut state) = data.downcast_mut::<AppState>() else {
                return Update::DoNothing;
            };
            let mut changed = crate::sync_ir_text_from_engine(&mut state, &mut info);
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

fn s(v: &str) -> AzString {
    AzString::from(v)
}

fn small(icon: &str, label: &str) -> RibbonButton {
    RibbonButton::create(s(icon), s(label))
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
            .fold(RibbonRow::create(), |r, it| r.with_item(it)),
    )
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(
        items
            .into_iter()
            .fold(RibbonColumn::create(), |c, it| c.with_item(it)),
    )
}

fn cell(preview_css: String, sample: &str, name: &str) -> RibbonGalleryCell {
    RibbonGalleryCell::create(
        Dom::create_div_with_text(sample).with_css(preview_css),
        s(name),
    )
}

fn home_tab(state: &AppState, data: &RefAny, pal: &Palette, sys: &SystemStyle) -> RibbonTab {
    let ribbon_style = crate::palette::widgets::ribbon(pal, sys);

    let clipboard = RibbonGroup::create(s("Clipboard"))
        .with_item(RibbonItem::LargeButton(
            RibbonButton::create(s("content_paste"), s("Paste")).with_arrow(RibbonArrow::Split),
        ))
        .with_item(column(vec![
            item("content_cut", "Cut"),
            item("content_copy", "Copy"),
            item("format_paint", "Format Painter"),
        ]));

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
        on_font_select,
    );
    let mut size_combo = ribbon_style.styled_combo_box(font_sizes, s("11"), 45);
    let name_text = name_combo.resolved_text_style();
    crate::fonts::push_ui_font(&mut name_combo.text_style, name_text);
    let size_text = size_combo.resolved_text_style();
    crate::fonts::push_ui_font(&mut size_combo.text_style, size_text);

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

    let font = RibbonGroup::create(s("Font")).with_item(column(vec![
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

    let paragraph = RibbonGroup::create(s("Paragraph")).with_item(column(vec![
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

    let ink = |r: u8, g: u8, b: u8| {
        Palette::hex(crate::palette::sample_ink(ColorU { r, g, b, a: 255 }, pal))
    };
    let cells = vec![
        cell(
            format!("font-size: 14px; color: {};", ink(68, 68, 68)),
            "AaBbCcDc",
            "\u{00b6} Normal",
        ),
        cell(
            format!("font-size: 14px; color: {};", ink(68, 68, 68)),
            "AaBbCcDc",
            "\u{00b6} No Spac...",
        ),
        cell(
            format!("font-size: 15px; color: {};", ink(46, 116, 181)),
            "AaBbCc",
            "Heading 1",
        ),
        cell(
            format!("font-size: 14px; color: {};", ink(46, 116, 181)),
            "AaBbCcD",
            "Heading 2",
        ),
        cell(
            format!("font-size: 19px; color: {};", ink(38, 38, 38)),
            "AaB",
            "Title",
        ),
        cell(
            format!("font-size: 13px; color: {};", ink(90, 90, 90)),
            "AaBbCcD",
            "Subtitle",
        ),
        cell(
            format!("font-size: 13px; color: {};", ink(128, 128, 128)),
            "AaBbCcDi",
            "Subtle Em...",
        ),
        cell(
            format!("font-size: 13px; color: {};", ink(68, 114, 196)),
            "AaBbCcDi",
            "Emphasis",
        ),
    ];
    let mut gallery = RibbonGallery::create(cells).with_selected(state.selected_style);
    gallery.set_on_select(
        data.clone(),
        on_style_select,
    );

    let styles = RibbonGroup::create(s("Styles"))
        .with_item(RibbonItem::Gallery(gallery))
        .with_fills_space(true);

    let editing = RibbonGroup::create(s("Editing")).with_item(column(vec![
        item_menu("search", "Find"),
        item("find_replace", "Replace"),
        item_menu("highlight_alt", "Select"),
    ]));

    RibbonTab::create(s("HOME"))
        .with_group(clipboard)
        .with_group(font)
        .with_group(paragraph)
        .with_group(styles)
        .with_group(editing)
}

fn placeholder_tab(label: &str) -> RibbonTab {
    RibbonTab::create(s(label)).with_group(RibbonGroup::create(s("Preview")).with_item(
        RibbonItem::LargeButton(RibbonButton::create(s("layers"), s(label))),
    ))
}

pub fn build(
    state: &AppState,
    data: &RefAny,
    pal: &Palette,
    sys: &SystemStyle,
    compact: bool,
) -> Dom {
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

    let mut ribbon = Ribbon::create(tabs)
        .with_app_button(RibbonAppButton::create(s("FILE")).with_on_click(
            data.clone(),
            crate::on_file_button as ButtonOnClickCallbackType,
        ))
        .with_active_tab(state.ribbon_tab);
    ribbon.style = crate::palette::widgets::ribbon(pal, sys);
    ribbon.set_on_tab_click(data.clone(), on_tab_click as RibbonOnTabClickCallbackType);
    let container = ribbon.style.resolved_container_style();
    crate::fonts::push_ui_font(&mut ribbon.style.container_style, container);
    if compact {
        ribbon.dom_mobile()
    } else {
        ribbon.dom_desktop()
    }
}
