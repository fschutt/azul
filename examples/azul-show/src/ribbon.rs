//! The ribbon: FILE (the backstage), HOME, INSERT, DESIGN, TRANSITIONS,
//! ANIMATIONS, SLIDE SHOW, VIEW, and the contextual FORMAT tab while
//! something is selected. Every button runs a [`Command`].

use azul::{
    callbacks::{ButtonOnClickCallbackType, CallbackInfo, RefAny, RibbonGalleryOnSelectCallbackType, RibbonOnTabClickCallbackType, Update},
    dom::Dom,
    str::String as AzString,
    widgets::{
        Ribbon, RibbonAppButton, RibbonButton, RibbonColumn, RibbonGallery, RibbonGalleryCell, RibbonGroup,
        RibbonItem, RibbonRow, RibbonTab,
    },
};

use crate::{
    app::{command, AppState, BackstagePage, Command, View},
    commands::on_command,
    editor::Editor,
    model::{
        Align, AnimationEffect, Background, ChartKind, Color, ElementKind, FontScheme, ImageFit, LayoutKind,
        ShapeKind, SlideSize, TransitionKind, ZOrder,
    },
    render::{css_color, css_font_family},
    text, themes,
};

/// The tabs, in order (the contextual FORMAT tab comes last when shown).
pub const TABS: [&str; 7] = ["HOME", "INSERT", "DESIGN", "TRANSITIONS", "ANIMATIONS", "SLIDE SHOW", "VIEW"];

fn s(text: &str) -> AzString {
    AzString::from(text)
}

fn button(app: &RefAny, icon: &str, label: &str, cmd: Command) -> RibbonButton {
    RibbonButton::create(s(icon), s(label)).with_on_click(command(app, cmd), on_command as ButtonOnClickCallbackType)
}

fn large(app: &RefAny, icon: &str, label: &str, cmd: Command) -> RibbonItem {
    RibbonItem::LargeButton(button(app, icon, label, cmd))
}

fn small(app: &RefAny, icon: &str, label: &str, cmd: Command) -> RibbonItem {
    RibbonItem::SmallButton(button(app, icon, label, cmd))
}

fn toggle(app: &RefAny, icon: &str, label: &str, cmd: Command, on: bool) -> RibbonItem {
    RibbonItem::SmallButton(button(app, icon, label, cmd).with_toggled(on))
}

/// An icon-only small button (PowerPoint's Font / Paragraph rows), named
/// `name` for assistive technology (it was announced as "button").
fn icon_button(app: &RefAny, icon: &str, name: &str, cmd: Command, on: bool) -> RibbonItem {
    RibbonItem::SmallButton(button(app, icon, "", cmd).with_toggled(on).with_alt(s(name)))
}

fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(RibbonColumn::create().with_items(items))
}

fn row(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Row(RibbonRow::create().with_items(items))
}

fn group(label: &str, items: Vec<RibbonItem>) -> RibbonGroup {
    RibbonGroup::create(s(label)).with_items(items)
}

/// What a gallery's pick means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryKind {
    NewSlide,
    Layout,
    Theme,
    Variant,
    Fonts,
    Transition,
    Animation,
}

struct GalleryData {
    app: RefAny,
    kind: GalleryKind,
}

/// The animation gallery's cells: none, then every effect.
fn animation_of(index: usize) -> Option<AnimationEffect> {
    index.checked_sub(1).and_then(|i| AnimationEffect::ALL.get(i).copied())
}

/// The command a gallery pick runs.
#[must_use]
pub fn gallery_command(kind: GalleryKind, index: usize) -> Option<Command> {
    Some(match kind {
        GalleryKind::NewSlide => Command::NewSlide(*LayoutKind::ALL.get(index)?),
        GalleryKind::Layout => Command::Layout(*LayoutKind::ALL.get(index)?),
        GalleryKind::Theme => Command::Theme(index),
        GalleryKind::Variant => Command::Variant(index),
        GalleryKind::Fonts => Command::FontScheme(index),
        GalleryKind::Transition => Command::Transition(*TransitionKind::ALL.get(index)?),
        GalleryKind::Animation => Command::Animation(animation_of(index)),
    })
}

extern "C" fn on_gallery(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let (mut app, kind) = match data.downcast_ref::<GalleryData>() {
        Some(d) => (d.app.clone(), d.kind),
        None => return Update::DoNothing,
    };
    match gallery_command(kind, index) {
        Some(cmd) => crate::commands::run(&mut app, cmd, &mut info),
        None => Update::DoNothing,
    }
}

extern "C" fn on_tab_click(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    crate::commands::run(&mut data, Command::RibbonTab(index), &mut info)
}

/// How many cells each gallery shows in the ribbon (the row holding the
/// selected one; "More" opens all): every tab fits a 1280 px window
/// (HOME's seven layouts inline were 863 px and pushed Font, Paragraph and
/// Editing off it).
fn visible_cells(kind: GalleryKind) -> usize {
    match kind {
        GalleryKind::Layout => 3,
        GalleryKind::NewSlide | GalleryKind::Theme => 4,
        GalleryKind::Variant | GalleryKind::Fonts => 3,
        GalleryKind::Transition => 4,
        GalleryKind::Animation => 6,
    }
}

fn gallery(app: &RefAny, kind: GalleryKind, cells: Vec<RibbonGalleryCell>, selected: usize) -> RibbonItem {
    RibbonItem::Gallery(
        RibbonGallery::create(cells)
            .with_selected(selected)
            .with_visible(visible_cells(kind))
            .with_on_select(
                RefAny::new(GalleryData {
                    app: app.clone(),
                    kind,
                }),
                on_gallery as RibbonGalleryOnSelectCallbackType,
            ),
    )
}

/// A layout's little picture: the title bar and the bodies as grey blocks.
fn layout_cell(layout: LayoutKind) -> RibbonGalleryCell {
    let scale = 0.03;
    let mut preview = Dom::create_div().with_css(
        "position: relative; width: 58px; height: 33px; background: #ffffff; border: 1px solid #b8b8b8;",
    );
    for spec in layout.placeholders(SlideSize::Wide) {
        let f = spec.frame;
        preview.add_child(Dom::create_div().with_css(format!(
            "position: absolute; left: {:.1}px; top: {:.1}px; width: {:.1}px; height: {:.1}px; background: {};",
            f.x * scale,
            f.y * scale,
            f.w * scale,
            f.h * scale,
            if spec.role.is_heading() { "#8a8a8a" } else { "#d0d0d0" },
        )));
    }
    RibbonGalleryCell::create(preview, s(layout.label()))
}

/// A theme's little picture: its ground, "Aa" in its heading font and ink,
/// and its accent bar.
fn theme_cell(index: usize, variant: usize, fonts: usize, label: &str) -> RibbonGalleryCell {
    let t = themes::theme(index, variant, fonts);
    let preview = Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: column; justify-content: space-between; width: 58px; \
             height: 33px; background: {}; border: 1px solid #b8b8b8; padding: 2px; box-sizing: border-box;",
            css_color(t.colors.background)
        ))
        .with_child(Dom::create_p_with_text("Aa").with_css(format!(
            "margin: 0px; font-size: 13px; color: {}; font-family: {};",
            css_color(t.colors.title),
            css_font_family(&t.fonts.heading)
        )))
        .with_child(Dom::create_div().with_css(format!(
            "height: 4px; background: {};",
            css_color(t.colors.accent)
        )));
    RibbonGalleryCell::create(preview, s(label))
}

fn swatch_cell(color: &str, label: &str) -> RibbonGalleryCell {
    RibbonGalleryCell::create(
        Dom::create_div().with_css(format!(
            "width: 40px; height: 24px; background: {color}; border: 1px solid #b8b8b8;"
        )),
        s(label),
    )
}

fn home_tab(app: &RefAny, ed: Option<&Editor>) -> RibbonTab {
    let body = ed.and_then(|e| {
        e.selection
            .keys
            .as_ref()
            .first()
            .and_then(|id| e.slide().element(*id))
            .and_then(|el| el.body())
    });
    let has = |axis| body.is_some_and(|b| text::all_have(b, axis));
    use azwriter::ir::FormatAxis as F;
    let align = body
        .and_then(|b| b.paragraphs.first())
        .map_or(Align::Left, |p| p.align);
    let layout_index = ed
        .map(|e| LayoutKind::ALL.iter().position(|l| *l == e.slide().layout).unwrap_or(0))
        .unwrap_or(0);
    let accent = ed.map_or(Color::rgb(0x2b, 0x57, 0x9a), |e| e.deck.theme.colors.accent);
    let accent2 = ed.map_or(Color::rgb(0xc5, 0x5a, 0x11), |e| e.deck.theme.colors.accent2);
    let ink = ed.map_or(Color::rgb(0x26, 0x26, 0x26), |e| e.deck.theme.colors.text);

    RibbonTab::create(s("HOME"))
        .with_group(group(
            "Clipboard",
            vec![
                large(app, "content_paste", "Paste", Command::Paste),
                column(vec![
                    small(app, "content_cut", "Cut", Command::Cut),
                    small(app, "content_copy", "Copy", Command::Copy),
                    small(app, "control_point_duplicate", "Duplicate", Command::Duplicate),
                ]),
            ],
        ))
        .with_group(group(
            "Slides",
            vec![
                large(app, "add_box", "New Slide", Command::NewSlide(LayoutKind::TitleAndContent)),
                column(vec![
                    small(app, "restart_alt", "Reset", Command::ResetSlide),
                    small(app, "segment", "Section", Command::AddSection),
                    small(app, "visibility_off", "Hide Slide", Command::ToggleHidden),
                ]),
            ],
        ))
        .with_group(
            group(
                "Layout",
                vec![gallery(
                    app,
                    GalleryKind::Layout,
                    LayoutKind::ALL.iter().map(|l| layout_cell(*l)).collect(),
                    layout_index,
                )],
            )
            .with_fills_space(false),
        )
        .with_group(group(
            "Font",
            vec![column(vec![
                row(vec![
                    icon_button(app, "format_bold", "Bold", Command::Bold, has(F::Bold)),
                    icon_button(app, "format_italic", "Italic", Command::Italic, has(F::Italic)),
                    icon_button(app, "format_underlined", "Underline", Command::Underline, has(F::Underline)),
                    icon_button(app, "strikethrough_s", "Strikethrough", Command::Strike, has(F::Strike)),
                ]),
                row(vec![
                    icon_button(app, "text_increase", "Increase font size", Command::Grow(1), false),
                    icon_button(app, "text_decrease", "Decrease font size", Command::Grow(-1), false),
                    icon_button(app, "format_color_text", "Font color: accent", Command::TextColor(Some(accent)), false),
                    icon_button(app, "format_color_reset", "Font color: automatic", Command::TextColor(None), false),
                ]),
            ])],
        ))
        .with_group(group(
            "Paragraph",
            vec![column(vec![
                row(vec![
                    icon_button(
                        app,
                        "format_list_bulleted",
                        "Bullets",
                        Command::Bullets,
                        body.is_some_and(|b| !b.paragraphs.is_empty() && b.paragraphs.iter().all(|p| p.bullet)),
                    ),
                    icon_button(app, "format_indent_decrease", "Decrease list level", Command::Indent(-1), false),
                    icon_button(app, "format_indent_increase", "Increase list level", Command::Indent(1), false),
                ]),
                row(vec![
                    icon_button(app, "format_align_left", "Align left", Command::Align(Align::Left), align == Align::Left),
                    icon_button(app, "format_align_center", "Center", Command::Align(Align::Center), align == Align::Center),
                    icon_button(app, "format_align_right", "Align right", Command::Align(Align::Right), align == Align::Right),
                    icon_button(app, "format_align_justify", "Justify", Command::Align(Align::Justify), align == Align::Justify),
                ]),
            ])],
        ))
        .with_group(group(
            "Drawing",
            vec![
                column(vec![
                    row(ShapeKind::ALL[..3].iter().map(|k| icon_button(app, k.icon(), k.label(), Command::Shape(*k), false)).collect()),
                    row(ShapeKind::ALL[3..].iter().map(|k| icon_button(app, k.icon(), k.label(), Command::Shape(*k), false)).collect()),
                ]),
                column(vec![
                    small(app, "flip_to_front", "Bring to Front", Command::Arrange(ZOrder::BringToFront)),
                    small(app, "flip_to_back", "Send to Back", Command::Arrange(ZOrder::SendToBack)),
                    small(app, "workspaces", "Group", Command::Group),
                ]),
                column(vec![
                    small(app, "format_color_fill", "Fill", Command::Fill(Some(accent))),
                    small(app, "palette", "Fill 2", Command::Fill(Some(accent2))),
                    small(app, "border_color", "Outline", Command::Outline(Some(ink))),
                ]),
            ],
        ))
        .with_group(group(
            "Editing",
            vec![
                column(vec![
                    small(app, "search", "Find", Command::Find(false)),
                    small(app, "find_replace", "Replace", Command::Find(true)),
                    small(app, "select_all", "Select All", Command::SelectAll),
                ]),
                column(vec![
                    small(app, "undo", "Undo", Command::Undo),
                    small(app, "redo", "Redo", Command::Redo),
                ]),
            ],
        ))
}

fn insert_tab(app: &RefAny) -> RibbonTab {
    RibbonTab::create(s("INSERT"))
        .with_group(group(
            "Slides",
            vec![gallery(
                app,
                GalleryKind::NewSlide,
                LayoutKind::ALL.iter().map(|l| layout_cell(*l)).collect(),
                usize::MAX,
            )],
        ))
        .with_group(group("Tables", vec![large(app, "table_chart", "Table", Command::Table)]))
        .with_group(group("Images", vec![large(app, "image", "Pictures", Command::Picture)]))
        .with_group(group(
            "Illustrations",
            vec![column(vec![
                row(ShapeKind::ALL[..3].iter().map(|k| small(app, k.icon(), k.label(), Command::Shape(*k))).collect()),
                row(ShapeKind::ALL[3..].iter().map(|k| small(app, k.icon(), k.label(), Command::Shape(*k))).collect()),
            ])],
        ))
        .with_group(group(
            "Charts",
            vec![column(vec![
                small(app, "bar_chart", "Bar", Command::Chart(ChartKind::Bar)),
                small(app, "show_chart", "Line", Command::Chart(ChartKind::Line)),
                small(app, "pie_chart", "Pie", Command::Chart(ChartKind::Pie)),
            ])],
        ))
        .with_group(group("Text", vec![large(app, "text_fields", "Text Box", Command::TextBox)]))
        .with_group(group("Media", vec![large(app, "movie", "Video", Command::Video)]))
}

fn design_tab(app: &RefAny, ed: Option<&Editor>) -> RibbonTab {
    let theme_index = ed.map_or(0, |e| themes::index_of(&e.deck.theme));
    let fonts_index = ed.map_or(0, |e| {
        FontScheme::all()
            .iter()
            .position(|f| *f == e.deck.theme.fonts)
            .unwrap_or(0)
    });
    let variant_index = ed.map_or(0, |e| {
        themes::VARIANTS
            .iter()
            .position(|v| e.deck.theme.colors.name.ends_with(v))
            .unwrap_or(0)
    });
    let size = ed.map_or(SlideSize::Wide, |e| e.deck.size);
    let (accent, soft, deep) = ed.map_or(
        (Color::rgb(0x2b, 0x57, 0x9a), Color::rgb(0xe0, 0xe4, 0xee), Color::rgb(0x1e, 0x32, 0x60)),
        |e| (e.deck.theme.colors.accent, e.deck.theme.colors.accent3, e.deck.theme.colors.title),
    );
    let current_bg = ed.and_then(|e| e.slide().background);
    RibbonTab::create(s("DESIGN"))
        .with_group(
            group(
                "Themes",
                vec![gallery(
                    app,
                    GalleryKind::Theme,
                    themes::names()
                        .iter()
                        .enumerate()
                        .map(|(i, name)| theme_cell(i, 0, fonts_index, name))
                        .collect(),
                    theme_index,
                )],
            )
            .with_fills_space(true),
        )
        .with_group(group(
            "Variants",
            vec![gallery(
                app,
                GalleryKind::Variant,
                themes::VARIANTS
                    .iter()
                    .enumerate()
                    .map(|(v, name)| theme_cell(theme_index.max(1), v, fonts_index, name))
                    .collect(),
                variant_index,
            )],
        ))
        .with_group(group(
            "Fonts",
            vec![gallery(
                app,
                GalleryKind::Fonts,
                FontScheme::all()
                    .iter()
                    .enumerate()
                    .map(|(i, f)| theme_cell(theme_index, 0, i, &f.name))
                    .collect(),
                fonts_index,
            )],
        ))
        .with_group(group(
            "Customize",
            vec![
                column(vec![
                    toggle(app, "crop_16_9", "Widescreen (16:9)", Command::SlideSize(SlideSize::Wide), size == SlideSize::Wide),
                    toggle(app, "crop_landscape", "Standard (4:3)", Command::SlideSize(SlideSize::Standard), size == SlideSize::Standard),
                ]),
                column(vec![
                    toggle(app, "format_paint", "Theme Ground", Command::Background(None, false), current_bg.is_none()),
                    small(app, "texture", "Soft Ground", Command::Background(Some(Background::Solid { color: soft }), false)),
                    small(
                        app,
                        "gradient",
                        "Gradient",
                        Command::Background(Some(Background::Gradient { from: accent, to: deep }), false),
                    ),
                ]),
                large(app, "select_all", "Apply to All", Command::Background(current_bg, true)),
            ],
        ))
}

fn transitions_tab(app: &RefAny, ed: Option<&Editor>) -> RibbonTab {
    let t = ed.map(|e| e.slide().transition).unwrap_or_default();
    let selected = TransitionKind::ALL.iter().position(|k| *k == t.kind).unwrap_or(0);
    let colors = ["#9e9e9e", "#6d8cc0", "#7fa98c", "#b3837a"];
    RibbonTab::create(s("TRANSITIONS"))
        .with_group(
            group(
                "Transition to This Slide",
                vec![gallery(
                    app,
                    GalleryKind::Transition,
                    TransitionKind::ALL
                        .iter()
                        .zip(colors)
                        .map(|(k, c)| swatch_cell(c, k.label()))
                        .collect(),
                    selected,
                )],
            )
            .with_fills_space(true),
        )
        .with_group(group(
            "Timing",
            vec![
                RibbonItem::Custom(
                    Dom::create_p_with_text(format!("Duration: {:.2} s", t.duration_ms as f32 / 1000.0))
                        .with_css("margin: 4px 8px; font-size: 12px;"),
                ),
                column(vec![
                    small(app, "add", "Longer", Command::TransitionDuration(100)),
                    small(app, "remove", "Shorter", Command::TransitionDuration(-100)),
                    small(app, "select_all", "Apply To All", Command::TransitionToAll),
                ]),
            ],
        ))
}

fn animations_tab(app: &RefAny, ed: Option<&Editor>) -> RibbonTab {
    let current = ed
        .and_then(|e| e.selection.keys.as_ref().first().and_then(|id| e.slide().element(*id)))
        .and_then(|el| el.animation);
    let selected = current
        .and_then(|a| AnimationEffect::ALL.iter().position(|x| *x == a.effect))
        .map_or(0, |i| i + 1);
    let mut cells = vec![swatch_cell("#e0e0e0", "None")];
    for effect in AnimationEffect::ALL {
        let color = match effect.class() {
            crate::model::AnimationClass::Entrance => "#7fa98c",
            crate::model::AnimationClass::Emphasis => "#d9b24c",
            crate::model::AnimationClass::Exit => "#c46a5a",
        };
        cells.push(swatch_cell(color, effect.label()));
    }
    let order = ed
        .and_then(|e| e.selection.keys.as_ref().first().and_then(|id| e.slide().build_step_of(*id)))
        .map_or_else(|| String::from("Order: -"), |n| format!("Order: {}", n + 1));
    RibbonTab::create(s("ANIMATIONS"))
        .with_group(
            group("Animation", vec![gallery(app, GalleryKind::Animation, cells, selected)]).with_fills_space(true),
        )
        .with_group(group(
            "Timing",
            vec![
                RibbonItem::Custom(Dom::create_p_with_text(order).with_css("margin: 4px 8px; font-size: 12px;")),
                column(vec![
                    small(app, "arrow_upward", "Move Earlier", Command::MoveAnimation(-1)),
                    small(app, "arrow_downward", "Move Later", Command::MoveAnimation(1)),
                ]),
            ],
        ))
}

fn show_tab(app: &RefAny, ed: Option<&Editor>) -> RibbonTab {
    let hidden = ed.is_some_and(|e| e.slide().hidden);
    RibbonTab::create(s("SLIDE SHOW"))
        .with_group(group(
            "Start Slide Show",
            vec![
                large(app, "slideshow", "From Beginning", Command::StartShow { from_current: false }),
                large(app, "play_arrow", "From Current Slide", Command::StartShow { from_current: true }),
            ],
        ))
        .with_group(group(
            "Set Up",
            vec![toggle(app, "visibility_off", "Hide Slide", Command::ToggleHidden, hidden)],
        ))
}

fn view_tab(app: &RefAny, st: &AppState) -> RibbonTab {
    let views = View::ALL
        .iter()
        .map(|v| RibbonItem::LargeButton(button(app, v.icon(), v.label(), Command::View(*v)).with_toggled(st.view == *v)))
        .collect();
    RibbonTab::create(s("VIEW"))
        .with_group(group("Presentation Views", views))
        .with_group(group(
            "Show",
            vec![toggle(app, "notes", "Notes", Command::ToggleNotes, st.show_notes)],
        ))
        .with_group(group(
            "Zoom",
            vec![column(vec![
                small(app, "zoom_in", "Zoom In", Command::Zoom(10)),
                small(app, "zoom_out", "Zoom Out", Command::Zoom(-10)),
                toggle(app, "fit_screen", "Fit to Window", Command::ZoomFit, st.zoom.is_none()),
            ])],
        ))
        // The app theme and the mode are File > Options (appkit's settings
        // page: one switch, remembered across restarts).
        .with_group(group(
            "Window",
            vec![small(app, "settings", "Options", Command::OpenBackstage(BackstagePage::Options))],
        ))
}

/// The contextual FORMAT tab of a selection.
fn format_tab(app: &RefAny, ed: &Editor) -> RibbonTab {
    let c = &ed.deck.theme.colors;
    let fills = [c.accent, c.accent2, c.accent3, c.title];
    let shapes = ed
        .selection
        .keys
        .as_ref()
        .iter()
        .filter_map(|id| ed.slide().element(*id))
        .any(|e| matches!(e.kind, ElementKind::Shape { .. }));
    let mut tab = RibbonTab::create(s("FORMAT")).with_group(group(
        "Arrange",
        vec![
            column(vec![
                small(app, "flip_to_front", "Bring to Front", Command::Arrange(ZOrder::BringToFront)),
                small(app, "arrow_upward", "Bring Forward", Command::Arrange(ZOrder::BringForward)),
                small(app, "arrow_downward", "Send Backward", Command::Arrange(ZOrder::SendBackward)),
                small(app, "flip_to_back", "Send to Back", Command::Arrange(ZOrder::SendToBack)),
            ]),
            column(vec![
                small(app, "workspaces", "Group", Command::Group),
                small(app, "workspaces_outline", "Ungroup", Command::Ungroup),
                small(app, "delete", "Delete", Command::Delete),
            ]),
        ],
    ));
    // A selected picture: how it fills its frame (PowerPoint's Crop > Fit /
    // Fill).
    let picture_fit = ed
        .selection
        .keys
        .as_ref()
        .iter()
        .filter_map(|id| ed.slide().element(*id))
        .find_map(|e| match e.kind {
            ElementKind::Image { fit, .. } => Some(fit),
            _ => None,
        });
    if let Some(fit) = picture_fit {
        tab = tab.with_group(group(
            "Picture",
            vec![column(vec![
                toggle(app, "fit_screen", "Fit", Command::ImageFit(ImageFit::Contain), fit == ImageFit::Contain),
                toggle(app, "crop", "Fill", Command::ImageFit(ImageFit::Cover), fit == ImageFit::Cover),
                toggle(app, "aspect_ratio", "Stretch", Command::ImageFit(ImageFit::Stretch), fit == ImageFit::Stretch),
            ])],
        ));
    }
    if shapes {
        tab = tab.with_group(group(
            "Shape Styles",
            vec![
                row(fills
                    .iter()
                    .map(|col| icon_button(app, "square", "Fill with a theme colour", Command::Fill(Some(*col)), false))
                    .collect()),
                column(vec![
                    small(app, "format_color_reset", "No Fill", Command::Fill(None)),
                    small(app, "border_color", "Outline", Command::Outline(Some(c.text))),
                    small(app, "border_clear", "No Outline", Command::Outline(None)),
                ]),
            ],
        ));
    }
    tab
}

/// The ribbon for the app's state.
#[must_use]
pub fn ribbon(app: &RefAny, st: &AppState) -> Dom {
    let ed = st.editor.as_ref();
    let mut tabs = vec![
        home_tab(app, ed),
        insert_tab(app),
        design_tab(app, ed),
        transitions_tab(app, ed),
        animations_tab(app, ed),
        show_tab(app, ed),
        view_tab(app, st),
    ];
    if let Some(e) = ed.filter(|e| !e.selection.is_empty()) {
        tabs.push(format_tab(app, e));
    }
    let active = st.ribbon_tab.min(tabs.len() - 1);
    Ribbon::create(tabs)
        .with_app_button(
            RibbonAppButton::create(s("FILE")).with_on_click(
                command(app, Command::OpenBackstage(BackstagePage::Info)),
                on_command as ButtonOnClickCallbackType,
            ),
        )
        .with_active_tab(active)
        .with_on_tab_click(app.clone(), on_tab_click as RibbonOnTabClickCallbackType)
        .dom_desktop()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_gallery_pick_runs_its_command() {
        assert_eq!(
            gallery_command(GalleryKind::NewSlide, 3),
            Some(Command::NewSlide(LayoutKind::TwoContent))
        );
        assert_eq!(gallery_command(GalleryKind::Animation, 0), Some(Command::Animation(None)));
        assert_eq!(
            gallery_command(GalleryKind::Animation, 2),
            Some(Command::Animation(Some(AnimationEffect::Fade)))
        );
        assert_eq!(
            gallery_command(GalleryKind::Transition, 2),
            Some(Command::Transition(TransitionKind::Push))
        );
        assert_eq!(gallery_command(GalleryKind::Layout, 99), None);
    }
}
