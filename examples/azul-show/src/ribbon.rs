//! The ribbon: FILE (the backstage), HOME, INSERT, DESIGN, TRANSITIONS,
//! ANIMATIONS, SLIDE SHOW, VIEW, and the contextual FORMAT tab while
//! something is selected. Every button runs a [`Command`].

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, CallbackInfo, DropDownOnChoiceChangeCallbackType, RefAny,
        RibbonGalleryOnSelectCallbackType, RibbonOnTabClickCallbackType, Update,
    },
    dom::Dom,
    str::String as AzString,
    widgets::{
        DropDown, Ribbon, RibbonAppButton, RibbonGallery, RibbonGalleryCell, RibbonGroup, RibbonItem,
        RibbonTab,
    },
};
use azul_appkit::ribbon::{
    button, column, group, icon_button, large, row, small, toggle, RibbonCommand,
};

use azul_appkit::l10n::{t_args, Arg};

use crate::{
    app::{command, AppState, BackstagePage, Command, PresenterMonitor, View},
    l10n::word,
    s,
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
        GalleryKind::Transition => 5,
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
    RibbonGalleryCell::create(preview, s(&word("layout", layout.label())))
}

/// A theme's little picture: its ground, "Aa" in its heading font and ink,
/// and its accent bar.
fn theme_cell(index: usize, variant: usize, fonts: usize, label: &str) -> RibbonGalleryCell {
    let t = themes::theme(index, variant, fonts);
    // The theme's own colours, not the chrome's: a spin leaves them.
    let preview = crate::render::content(Dom::create_div())
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
        // A colour for the deck: the document's, which a spin leaves.
        crate::render::content(Dom::create_div()).with_css(format!(
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
    use azul::widgets::RichFormat as F;
    // In the text being edited: the format at the caret or over the
    // selection (the shared editor's); else the whole selected text's.
    let edited = ed.and_then(|e| e.editing.and(e.text.as_ref()));
    let has = |format: F| match edited {
        Some(state) => state.is_current_format(format),
        None => body.is_some_and(|b| text::all_have(b, format)),
    };
    let align = body
        .and_then(|b| b.paragraphs.first())
        .map_or(Align::Left, |p| p.align);
    let layout_index = ed
        .map(|e| LayoutKind::ALL.iter().position(|l| *l == e.slide().layout).unwrap_or(0))
        .unwrap_or(0);
    let accent = ed.map_or(Color::rgb(0x2b, 0x57, 0x9a), |e| e.deck.theme.colors.accent);
    let accent2 = ed.map_or(Color::rgb(0xc5, 0x5a, 0x11), |e| e.deck.theme.colors.accent2);
    let ink = ed.map_or(Color::rgb(0x26, 0x26, 0x26), |e| e.deck.theme.colors.text);

    RibbonTab::create(s("azshow-tab-home"))
        .with_group(group(
            "azshow-clipboard",
            vec![
                large(app, "content_paste", "azshow-paste", Command::Paste),
                column(vec![
                    small(app, "content_cut", "azshow-cut", Command::Cut),
                    small(app, "content_copy", "azshow-copy", Command::Copy),
                    small(app, "control_point_duplicate", "azshow-duplicate", Command::Duplicate),
                ]),
            ],
        ))
        .with_group(group(
            "azshow-slides",
            vec![
                large(app, "add_box", "azshow-new-slide", Command::NewSlide(LayoutKind::TitleAndContent)),
                column(vec![
                    small(app, "restart_alt", "azshow-reset", Command::ResetSlide),
                    small(app, "segment", "azshow-section", Command::AddSection),
                    small(app, "visibility_off", "azshow-hide-slide", Command::ToggleHidden),
                ]),
            ],
        ))
        .with_group(
            group(
                "azshow-layout",
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
            "azshow-font",
            vec![column(vec![
                row(vec![
                    icon_button(app, "format_bold", "azshow-bold", Command::Bold, has(F::Bold)),
                    icon_button(app, "format_italic", "azshow-italic", Command::Italic, has(F::Italic)),
                    icon_button(app, "format_underlined", "azshow-underline", Command::Underline, has(F::Underline)),
                    icon_button(app, "strikethrough_s", "azshow-strikethrough", Command::Strike, has(F::Strike)),
                ]),
                row(vec![
                    icon_button(app, "text_increase", "azshow-increase-font-size", Command::Grow(1), false),
                    icon_button(app, "text_decrease", "azshow-decrease-font-size", Command::Grow(-1), false),
                    icon_button(app, "format_color_text", "azshow-font-color-accent", Command::TextColor(Some(accent)), false),
                    icon_button(app, "format_color_reset", "azshow-font-color-automatic", Command::TextColor(None), false),
                ]),
            ])],
        ))
        .with_group(group(
            "azshow-paragraph",
            vec![column(vec![
                row(vec![
                    icon_button(
                        app,
                        "format_list_bulleted",
                        "azshow-bullets",
                        Command::Bullets,
                        body.is_some_and(|b| !b.paragraphs.is_empty() && b.paragraphs.iter().all(|p| p.bullet)),
                    ),
                    icon_button(app, "format_indent_decrease", "azshow-decrease-list-level", Command::Indent(-1), false),
                    icon_button(app, "format_indent_increase", "azshow-increase-list-level", Command::Indent(1), false),
                ]),
                row(vec![
                    icon_button(app, "format_align_left", "azshow-align-left", Command::Align(Align::Left), align == Align::Left),
                    icon_button(app, "format_align_center", "azshow-center", Command::Align(Align::Center), align == Align::Center),
                    icon_button(app, "format_align_right", "azshow-align-right", Command::Align(Align::Right), align == Align::Right),
                    icon_button(app, "format_align_justify", "azshow-justify", Command::Align(Align::Justify), align == Align::Justify),
                ]),
            ])],
        ))
        .with_group(group(
            "azshow-drawing",
            vec![
                column(
                    ShapeKind::ALL
                        .chunks(3)
                        .map(|three| {
                            row(three
                                .iter()
                                .map(|k| icon_button(app, k.icon(), &word("shape", k.label()), Command::Shape(*k), false))
                                .collect())
                        })
                        .collect(),
                ),
                column(vec![
                    small(app, "flip_to_front", "azshow-bring-front", Command::Arrange(ZOrder::BringToFront)),
                    small(app, "flip_to_back", "azshow-send-back", Command::Arrange(ZOrder::SendToBack)),
                    small(app, "workspaces", "azshow-group", Command::Group),
                ]),
                column(vec![
                    small(app, "format_color_fill", "azshow-fill", Command::Fill(Some(accent))),
                    small(app, "palette", "azshow-fill-2", Command::Fill(Some(accent2))),
                    small(app, "border_color", "azshow-outline", Command::Outline(Some(ink))),
                ]),
            ],
        ))
        .with_group(group(
            "azshow-editing",
            vec![
                column(vec![
                    small(app, "search", "azshow-find", Command::Find(false)),
                    small(app, "find_replace", "azshow-replace", Command::Find(true)),
                    small(app, "select_all", "azshow-select-all", Command::SelectAll),
                ]),
                column(vec![
                    small(app, "undo", "azshow-undo", Command::Undo),
                    small(app, "redo", "azshow-redo", Command::Redo),
                ]),
            ],
        ))
}

fn insert_tab(app: &RefAny) -> RibbonTab {
    RibbonTab::create(s("azshow-tab-insert"))
        .with_group(group(
            "azshow-slides",
            vec![gallery(
                app,
                GalleryKind::NewSlide,
                LayoutKind::ALL.iter().map(|l| layout_cell(*l)).collect(),
                usize::MAX,
            )],
        ))
        .with_group(group("azshow-tables", vec![large(app, "table_chart", "azshow-table", Command::Table)]))
        .with_group(group("azshow-images", vec![large(app, "image", "azshow-pictures", Command::Picture)]))
        .with_group(group(
            "azshow-illustrations",
            // Three rows of three: the boxes, the line and the arrows, the
            // diamond, the chevron and the line arrow (drawn as SVG).
            vec![column(
                ShapeKind::ALL
                    .chunks(3)
                    .map(|three| {
                        row(three.iter().map(|k| small(app, k.icon(), &word("shape", k.label()), Command::Shape(*k))).collect())
                    })
                    .collect(),
            )],
        ))
        .with_group(group(
            "azshow-charts",
            vec![column(vec![
                small(app, "bar_chart", "azshow-bar", Command::Chart(ChartKind::Bar)),
                small(app, "show_chart", "azshow-line", Command::Chart(ChartKind::Line)),
                small(app, "pie_chart", "azshow-pie", Command::Chart(ChartKind::Pie)),
            ])],
        ))
        .with_group(group("azshow-text", vec![large(app, "text_fields", "azshow-text-box", Command::TextBox)]))
        .with_group(group("azshow-media", vec![large(app, "movie", "azshow-video", Command::Video)]))
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
    RibbonTab::create(s("azshow-tab-design"))
        .with_group(
            group(
                "azshow-themes",
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
            "azshow-variants",
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
            "azshow-fonts",
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
            "azshow-customize",
            vec![
                column(vec![
                    toggle(app, "crop_16_9", "azshow-size-widescreen-16-9", Command::SlideSize(SlideSize::Wide), size == SlideSize::Wide),
                    toggle(app, "crop_landscape", "azshow-size-standard-4-3", Command::SlideSize(SlideSize::Standard), size == SlideSize::Standard),
                ]),
                column(vec![
                    toggle(app, "format_paint", "azshow-theme-ground", Command::Background(None, false), current_bg.is_none()),
                    small(app, "texture", "azshow-soft-ground", Command::Background(Some(Background::Solid { color: soft }), false)),
                    small(
                        app,
                        "gradient",
                        "azshow-gradient",
                        Command::Background(Some(Background::Gradient { from: accent, to: deep }), false),
                    ),
                ]),
                large(app, "select_all", "azshow-apply-all", Command::Background(current_bg, true)),
            ],
        ))
}

fn transitions_tab(app: &RefAny, ed: Option<&Editor>) -> RibbonTab {
    let t = ed.map(|e| e.slide().transition).unwrap_or_default();
    let selected = TransitionKind::ALL.iter().position(|k| *k == t.kind).unwrap_or(0);
    let colors = ["#9e9e9e", "#6d8cc0", "#7fa98c", "#b3837a", "#9c7fb8"];
    RibbonTab::create(s("azshow-tab-transitions"))
        .with_group(group(
            "azshow-preview",
            vec![large(app, "play_circle", "azshow-preview", Command::PreviewTransition)],
        ))
        .with_group(
            group(
                "azshow-transition-slide",
                vec![gallery(
                    app,
                    GalleryKind::Transition,
                    TransitionKind::ALL
                        .iter()
                        .zip(colors)
                        .map(|(k, c)| swatch_cell(c, &word("transition", k.label())))
                        .collect(),
                    selected,
                )],
            )
            .with_fills_space(true),
        )
        .with_group(group(
            "azshow-timing",
            vec![
                RibbonItem::Custom(
                    Dom::create_p_with_text(t_args(
                        "azshow-duration",
                        &[("seconds", Arg::from(format!("{:.2}", t.duration_ms as f32 / 1000.0)))],
                    ))
                        .with_css("margin: 4px 8px; font-size: 12px;"),
                ),
                column(vec![
                    small(app, "add", "azshow-longer", Command::TransitionDuration(100)),
                    small(app, "remove", "azshow-shorter", Command::TransitionDuration(-100)),
                    small(app, "select_all", "azshow-apply-to-all", Command::TransitionToAll),
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
    let mut cells = vec![swatch_cell("#e0e0e0", "azshow-transition-none")];
    for effect in AnimationEffect::ALL {
        let color = match effect.class() {
            crate::model::AnimationClass::Entrance => "#7fa98c",
            crate::model::AnimationClass::Emphasis => "#d9b24c",
            crate::model::AnimationClass::Exit => "#c46a5a",
        };
        cells.push(swatch_cell(color, &word("effect", effect.label())));
    }
    let order = ed
        .and_then(|e| e.selection.keys.as_ref().first().and_then(|id| e.slide().build_step_of(*id)))
        .map_or_else(|| String::from("-"), |n| (n + 1).to_string());
    let order = t_args("azshow-order", &[("n", Arg::from(order))]);
    RibbonTab::create(s("azshow-tab-animations"))
        .with_group(group(
            "azshow-preview",
            vec![large(app, "play_circle", "azshow-preview", Command::PreviewBuild)],
        ))
        .with_group(
            group("azshow-animation", vec![gallery(app, GalleryKind::Animation, cells, selected)]).with_fills_space(true),
        )
        .with_group(group(
            "azshow-timing",
            vec![
                RibbonItem::Custom(Dom::create_p_with_text(order).with_css("margin: 4px 8px; font-size: 12px;")),
                column(vec![
                    small(app, "arrow_upward", "azshow-move-earlier", Command::MoveAnimation(-1)),
                    small(app, "arrow_downward", "azshow-move-later", Command::MoveAnimation(1)),
                ]),
            ],
        ))
}

/// Slide Show > Monitors: a pick of where the presenter view opens.
extern "C" fn on_monitor_pick(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let choice = match index {
        0 => PresenterMonitor::Automatic,
        n => match PresenterMonitor::choices(&info.get_monitors()).get(n - 1) {
            Some((monitor, _)) => PresenterMonitor::Monitor(*monitor),
            None => return Update::DoNothing,
        },
    };
    crate::commands::run(&mut data, Command::PresenterMonitor(choice), &mut info)
}

/// The Monitors group: "Automatic", then every connected screen.
fn monitors_group(app: &RefAny, chosen: PresenterMonitor, monitors: &[(u32, String)]) -> RibbonGroup {
    let mut labels = vec![String::from("azshow-monitor-automatic")];
    labels.extend(monitors.iter().map(|(_, label)| label.clone()));
    let selected = match chosen {
        PresenterMonitor::Automatic => 0,
        PresenterMonitor::Monitor(index) => monitors
            .iter()
            .position(|(m, _)| *m == index)
            .map_or(0, |at| at + 1),
    };
    let names: Vec<AzString> = labels.iter().map(|l| s(l)).collect();
    group(
        "azshow-monitors",
        vec![RibbonItem::Drop(
            DropDown::create(names)
                .with_selected(selected)
                .with_accessibility_name(s("azshow-presenter-view"))
                .with_on_choice_change(app.clone(), on_monitor_pick as DropDownOnChoiceChangeCallbackType),
        )],
    )
}

fn show_tab(app: &RefAny, ed: Option<&Editor>, chosen: PresenterMonitor, monitors: &[(u32, String)]) -> RibbonTab {
    let hidden = ed.is_some_and(|e| e.slide().hidden);
    RibbonTab::create(s("azshow-tab-slide-show"))
        .with_group(group(
            "azshow-start-slide-show",
            vec![
                large(app, "slideshow", "azshow-from-beginning", Command::StartShow { from_current: false }),
                large(app, "play_arrow", "azshow-from-current-slide", Command::StartShow { from_current: true }),
            ],
        ))
        .with_group(group(
            "azshow-set-up",
            vec![toggle(app, "visibility_off", "azshow-hide-slide", Command::ToggleHidden, hidden)],
        ))
        .with_group(monitors_group(app, chosen, monitors))
}

fn view_tab(app: &RefAny, st: &AppState) -> RibbonTab {
    let views = View::ALL
        .iter()
        .map(|v| {
            RibbonItem::LargeButton(button(app, v.icon(), &word("view", v.label()), Command::View(*v)).with_toggled(st.view == *v))
        })
        .collect();
    RibbonTab::create(s("azshow-tab-view"))
        .with_group(group("azshow-presentation-views", views))
        .with_group(group(
            "azshow-show",
            vec![toggle(app, "notes", "azshow-notes", Command::ToggleNotes, st.show_notes)],
        ))
        .with_group(group(
            "azshow-zoom",
            vec![column(vec![
                small(app, "zoom_in", "azshow-zoom-in", Command::Zoom(10)),
                small(app, "zoom_out", "azshow-zoom-out", Command::Zoom(-10)),
                toggle(app, "fit_screen", "azshow-fit-window", Command::ZoomFit, st.zoom.is_none()),
            ])],
        ))
        // The app theme and the mode are File > Options (appkit's settings
        // page: one switch, remembered across restarts).
        .with_group(group(
            "azshow-window",
            vec![small(app, "settings", "azshow-options", Command::OpenBackstage(BackstagePage::Options))],
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
    let mut tab = RibbonTab::create(s("azshow-tab-format")).with_group(group(
        "azshow-arrange",
        vec![
            column(vec![
                small(app, "flip_to_front", "azshow-bring-front", Command::Arrange(ZOrder::BringToFront)),
                small(app, "arrow_upward", "azshow-bring-forward", Command::Arrange(ZOrder::BringForward)),
                small(app, "arrow_downward", "azshow-send-backward", Command::Arrange(ZOrder::SendBackward)),
                small(app, "flip_to_back", "azshow-send-back", Command::Arrange(ZOrder::SendToBack)),
            ]),
            column(vec![
                small(app, "workspaces", "azshow-group", Command::Group),
                small(app, "workspaces_outline", "azshow-ungroup", Command::Ungroup),
                small(app, "delete", "azshow-delete", Command::Delete),
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
            "azshow-picture",
            vec![column(vec![
                toggle(app, "fit_screen", "azshow-fit-contain", Command::ImageFit(ImageFit::Contain), fit == ImageFit::Contain),
                toggle(app, "crop", "azshow-fit-cover", Command::ImageFit(ImageFit::Cover), fit == ImageFit::Cover),
                toggle(app, "aspect_ratio", "azshow-fit-stretch", Command::ImageFit(ImageFit::Stretch), fit == ImageFit::Stretch),
            ])],
        ));
    }
    if shapes {
        tab = tab.with_group(group(
            "azshow-shape-styles",
            vec![
                row(fills
                    .iter()
                    .map(|col| icon_button(app, "square", "azshow-fill-theme-colour", Command::Fill(Some(*col)), false))
                    .collect()),
                column(vec![
                    small(app, "format_color_reset", "azshow-no-fill", Command::Fill(None)),
                    small(app, "border_color", "azshow-outline", Command::Outline(Some(c.text))),
                    small(app, "border_clear", "azshow-no-outline", Command::Outline(None)),
                ]),
            ],
        ));
    }
    tab
}

/// The ribbon for the app's state.
#[must_use]
pub fn ribbon(app: &RefAny, st: &AppState, monitors: &[(u32, String)]) -> Dom {
    let ed = st.editor.as_ref();
    let mut tabs = vec![
        home_tab(app, ed),
        insert_tab(app),
        design_tab(app, ed),
        transitions_tab(app, ed),
        animations_tab(app, ed),
        show_tab(app, ed, st.presenter_monitor, monitors),
        view_tab(app, st),
    ];
    if let Some(e) = ed.filter(|e| !e.selection.is_empty()) {
        tabs.push(format_tab(app, e));
    }
    let active = st.ribbon_tab.min(tabs.len() - 1);
    Ribbon::create(tabs)
        .with_app_button(
            RibbonAppButton::create(s("azshow-tab-file")).with_on_click(
                command(app, Command::OpenBackstage(BackstagePage::Info)),
                on_command as ButtonOnClickCallbackType,
            ),
        )
        .with_active_tab(active)
        .with_on_tab_click(app.clone(), on_tab_click as RibbonOnTabClickCallbackType)
        // No title row over the ribbon: its tabs are the title bar.
        .with_tabs_in_titlebar(azul_appkit::ui::tabs_in_titlebar())
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
        assert_eq!(
            gallery_command(GalleryKind::Transition, 4),
            Some(Command::Transition(TransitionKind::Morph))
        );
        assert_eq!(gallery_command(GalleryKind::Layout, 99), None);
    }
}
