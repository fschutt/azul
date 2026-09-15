use azul::{
    css::ColorU,
    dialog::{
        ColorPickResult, ColorPickerDialog, FileDialog, FileOpenResult, MsgBox, MsgBoxIcon, YesNo,
    },
    dom::{ComboBoxOnSelectCallback, RibbonGalleryOnSelectCallback},
    option::{OptionColorU, OptionFileTypeList, OptionString},
    prelude::*,
    widgets::*,
};

#[derive(Clone)]
struct DocState {
    active_tab: usize,
    bold: bool,
    italic: bool,
    underline: bool,
    align: usize,
    selected_style: usize,
    applied_template: String,
    font_color: ColorU,
}

impl Default for DocState {
    fn default() -> Self {
        Self {
            active_tab: 0,
            bold: false,
            italic: false,
            underline: false,
            align: 0,
            selected_style: 0,
            applied_template: "Normal".to_string(),
            font_color: ColorU {
                r: 192,
                g: 0,
                b: 0,
                a: 255,
            },
        }
    }
}

extern "C" fn on_tab_click(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
    let Some(mut state) = data.downcast_mut::<DocState>() else {
        return Update::DoNothing;
    };
    state.active_tab = index;
    Update::RefreshDom
}

extern "C" fn on_style_select(mut data: RefAny, _: CallbackInfo, index: usize) -> Update {
    let Some(mut state) = data.downcast_mut::<DocState>() else {
        return Update::DoNothing;
    };
    state.selected_style = index;
    let name = TEMPLATE_NAMES.get(index).copied().unwrap_or("Normal");
    println!("[app] apply template: {name}");
    state.applied_template = name.into();
    Update::RefreshDom
}

extern "C" fn on_toggle_bold(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<DocState>() else {
        return Update::DoNothing;
    };
    state.bold = !state.bold;
    Update::RefreshDom
}

extern "C" fn on_toggle_italic(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<DocState>() else {
        return Update::DoNothing;
    };
    state.italic = !state.italic;
    Update::RefreshDom
}

extern "C" fn on_toggle_underline(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(mut state) = data.downcast_mut::<DocState>() else {
        return Update::DoNothing;
    };
    state.underline = !state.underline;
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
    let Some(mut state) = app.downcast_mut::<DocState>() else {
        return Update::DoNothing;
    };
    state.align = align;
    Update::RefreshDom
}

struct LauncherPayload {
    app: RefAny,
    which: usize,
}

extern "C" fn on_launcher(mut data: RefAny, _: CallbackInfo) -> Update {
    let Some(payload) = data.downcast_ref::<LauncherPayload>() else {
        return Update::DoNothing;
    };
    let which = payload.which;
    let mut app = payload.app.clone();
    drop(payload);

    match which {
        0 => {
            let _request = ColorPickerDialog::open(
                "Font Colour",
                OptionColorU::Some(ColorU {
                    r: 192,
                    g: 0,
                    b: 0,
                    a: 255,
                }),
                app,
                on_font_colour_picked,
            );
            Update::DoNothing
        }
        1 => {
            MsgBox::info("Paragraph settings\n\n(Indents and Spacing / Line and Page Breaks)");
            Update::DoNothing
        }
        _ => {
            if MsgBox::yes_no(
                "Styles",
                "Load a style set from a file?",
                MsgBoxIcon::Question,
                YesNo::No,
            ) == YesNo::Yes
            {
                let _request = FileDialog::open_file(
                    "Choose a style set",
                    OptionString::None,
                    OptionFileTypeList::None,
                    app,
                    on_style_set_picked,
                );
            }
            Update::DoNothing
        }
    }
}

extern "C" fn on_font_colour_picked(mut app: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = ColorPickResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    if let OptionColorU::Some(c) = picked.color {
        if let Some(mut state) = app.downcast_mut::<DocState>() {
            state.font_color = c;
            return Update::RefreshDom;
        }
    }
    Update::DoNothing
}

extern "C" fn on_style_set_picked(_app: RefAny, _: CallbackInfo, result: RefAny) -> Update {
    let Some(picked) = FileOpenResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    if let Some(path) = picked.path.into_option() {
        println!("style set: {}", path.as_string());
    }
    Update::DoNothing
}

const TEMPLATE_NAMES: &[&str] = &[
    "Normal",
    "No Spacing",
    "Heading 1",
    "Heading 2",
    "Title",
    "Subtitle",
    "Subtle Emphasis",
    "Emphasis",
];

extern "C" fn on_font_select(_: RefAny, _: CallbackInfo, state: ComboBoxState) -> Update {
    println!(
        "font changed: {} (index {})",
        state.text.as_str(),
        state.selected
    );
    Update::DoNothing
}

fn small(icon: &str, label: &str) -> RibbonButton {
    RibbonButton::create(icon, label)
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

fn cell(preview_css: &str, sample: &str, name: &str) -> RibbonGalleryCell {
    RibbonGalleryCell::create(
        Dom::create_div_with_text(sample).with_css(preview_css),
        name,
    )
}

fn home_tab(state: &DocState, data: &RefAny) -> RibbonTab {
    let ribbon_style = RibbonStyle::office_2013();

    let clipboard = RibbonGroup::create("Clipboard")
        .with_item(RibbonItem::LargeButton(
            RibbonButton::create("content_paste", "Paste").with_arrow(RibbonArrow::Split),
        ))
        .with_item(column(vec![
            item("content_cut", "Cut"),
            item("content_copy", "Copy"),
            item("format_paint", "Format Painter"),
        ]))
        .with_launcher(
            RefAny::new(LauncherPayload {
                app: data.clone(),
                which: 1,
            }),
            on_launcher,
        );

    let font_names: Vec<azul::str::String> = [
        "Calibri (Body)",
        "Calibri Light",
        "Cambria",
        "Arial",
        "Courier New",
        "Times New Roman",
    ]
    .iter()
    .map(|s| (*s).into())
    .collect();
    let font_sizes: Vec<azul::str::String> = ["8", "9", "10", "11", "12", "14", "18", "24", "36"]
        .iter()
        .map(|s| (*s).into())
        .collect();

    let mut name_combo = ribbon_style.styled_combo_box(font_names, "Calibri (Body)", 133);
    name_combo.set_on_select(
        data.clone(),
        ComboBoxOnSelectCallback {
            cb: on_font_select,
            callable: OptionRefAny::None,
        },
    );
    let size_combo = ribbon_style.styled_combo_box(font_sizes, "11", 45);

    let mut bold = small("format_bold", "").with_toggled(state.bold);
    bold.set_on_click(data.clone(), on_toggle_bold);
    let mut italic = small("format_italic", "").with_toggled(state.italic);
    italic.set_on_click(data.clone(), on_toggle_italic);
    let mut underline = small("format_underlined", "")
        .with_toggled(state.underline)
        .with_arrow(RibbonArrow::Menu);
    underline.set_on_click(data.clone(), on_toggle_underline);

    let font = RibbonGroup::create("Font")
        .with_item(column(vec![
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
        ]))
        .with_launcher(
            RefAny::new(LauncherPayload {
                app: data.clone(),
                which: 0,
            }),
            on_launcher,
        );

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
            on_align,
        );
        align_items.push(RibbonItem::SmallButton(b));
    }
    let mut para_row2 = align_items;
    para_row2.push(RibbonItem::Separator);
    para_row2.push(item_menu("format_line_spacing", ""));
    para_row2.push(RibbonItem::Separator);
    para_row2.push(item_menu("format_color_fill", ""));
    para_row2.push(item_menu("border_all", ""));

    let paragraph = RibbonGroup::create("Paragraph")
        .with_item(column(vec![
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
        ]))
        .with_launcher(
            RefAny::new(LauncherPayload {
                app: data.clone(),
                which: 1,
            }),
            on_launcher,
        );

    let cells = vec![
        cell(
            "font-size: 14px; color: #444444;",
            "AaBbCcDc",
            "\u{00b6} Normal",
        ),
        cell(
            "font-size: 14px; color: #444444;",
            "AaBbCcDc",
            "\u{00b6} No Spac...",
        ),
        cell("font-size: 15px; color: #2e74b5;", "AaBbCc", "Heading 1"),
        cell("font-size: 14px; color: #2e74b5;", "AaBbCcD", "Heading 2"),
        cell("font-size: 19px; color: #262626;", "AaB", "Title"),
        cell("font-size: 13px; color: #5a5a5a;", "AaBbCcD", "Subtitle"),
        cell(
            "font-size: 13px; color: #808080;",
            "AaBbCcDi",
            "Subtle Em...",
        ),
        cell("font-size: 13px; color: #4472c4;", "AaBbCcDi", "Emphasis"),
    ];
    let mut gallery = RibbonGallery::create(cells).with_selected(state.selected_style);
    gallery.set_on_select(
        data.clone(),
        RibbonGalleryOnSelectCallback {
            cb: on_style_select,
            callable: OptionRefAny::None,
        },
    );

    let styles = RibbonGroup::create("Styles")
        .with_item(RibbonItem::Gallery(gallery))
        .with_launcher(
            RefAny::new(LauncherPayload {
                app: data.clone(),
                which: 2,
            }),
            on_launcher,
        )
        .with_fills_space(true);

    let editing = RibbonGroup::create("Editing").with_item(column(vec![
        item_menu("search", "Find"),
        item("find_replace", "Replace"),
        item_menu("highlight_alt", "Select"),
    ]));

    RibbonTab::create("HOME")
        .with_group(clipboard)
        .with_group(font)
        .with_group(paragraph)
        .with_group(styles)
        .with_group(editing)
}

fn placeholder_tab(label: &str) -> RibbonTab {
    RibbonTab::create(label).with_group(
        RibbonGroup::create("Preview")
            .with_item(RibbonItem::LargeButton(RibbonButton::create("layers", label))),
    )
}

fn qat_icon(name: &str) -> Dom {
    Dom::create_icon(name).with_css("font-size: 16px; color: #6a6a6a; margin-right: 10px;")
}

fn title_bar() -> Dom {
    let word_logo = Dom::create_div()
        .with_css(
            "display: flex; align-items: center; justify-content: center; width: 22px; height: \
             22px; background: #2b579a; margin-right: 10px;",
        )
        .with_child(Dom::create_div_with_text("W").with_css("font-size: 13px; color: white;"));

    let left = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(word_logo)
        .with_child(qat_icon("save"))
        .with_child(qat_icon("undo"))
        .with_child(qat_icon("redo"))
        .with_child(
            Dom::create_icon("arrow_drop_down").with_css("font-size: 14px; color: #6a6a6a;"),
        );

    let title = Dom::create_div_with_text("Document1 - AzWriter")
        .with_css("flex-grow: 1; text-align: center; font-size: 12px; color: #444444;");

    let right = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center;")
        .with_child(
            Dom::create_icon("help_outline")
                .with_css("font-size: 15px; color: #6a6a6a; margin-right: 12px;"),
        )
        .with_child(
            Dom::create_icon("minimize")
                .with_css("font-size: 15px; color: #6a6a6a; margin-right: 12px;"),
        )
        .with_child(
            Dom::create_icon("crop_square")
                .with_css("font-size: 15px; color: #6a6a6a; margin-right: 12px;"),
        )
        .with_child(Dom::create_icon("close").with_css("font-size: 15px; color: #6a6a6a;"));

    Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; align-items: center; height: 30px; background: \
             white; padding-left: 8px; padding-right: 8px; flex-grow: 0;",
        )
        .with_child(left)
        .with_child(title)
        .with_child(right)
}

extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let state = match data.downcast_ref::<DocState>() {
        Some(s) => (*s).clone(),
        None => return Dom::create_body(),
    };

    let tabs: Vec<RibbonTab> = vec![
        home_tab(&state, &data),
        placeholder_tab("INSERT"),
        placeholder_tab("DESIGN"),
        placeholder_tab("PAGE LAYOUT"),
        placeholder_tab("REFERENCES"),
        placeholder_tab("MAILINGS"),
        placeholder_tab("REVIEW"),
        placeholder_tab("VIEW"),
        placeholder_tab("ADD-INS"),
    ];

    let mut ribbon = Ribbon::create(tabs)
        .with_app_button(RibbonAppButton::create("FILE"))
        .with_active_tab(state.active_tab);
    ribbon.set_on_tab_click(data.clone(), on_tab_click);

    let document_area = Dom::create_div().with_css("flex-grow: 1; background: #e6e6e6;");

    Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; background: white; margin: 0; padding: 0; \
             font-family: system:ui; font-size: 12px; color: #444444;",
        )
        .with_child(title_bar())
        .with_child(if info.viewport_bigger_than(720.0) {
            ribbon.dom_desktop()
        } else {
            ribbon.dom_mobile()
        })
        .with_child(document_area)
}

fn main() {
    let data = RefAny::new(DocState::default());
    let app = App::create(data, AppConfig::create());
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = "Document1 - AzWriter".into();
    window.window_state.size.dimensions.width = 1388.0;
    window.window_state.size.dimensions.height = 260.0;
    app.run(window);
}
