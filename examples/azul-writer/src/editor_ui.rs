use azul::{
    callbacks::{
        ButtonOnClickCallbackType, RefAny, SliderOnValueChangeCallbackType,
        StatusBarOnViewSelectCallbackType, VirtualViewCallbackInfo, VirtualViewReturn,
    },
    component::ComponentEventFilter,
    css::{EventFilter, FocusEventFilter, LayoutSize, LogicalPosition, LogicalSize, SystemStyle},
    dom::{Dom, IdOrClass, SliderOnValueChangeCallback, StatusBarOnViewSelectCallback},
    option::{OptionDom, OptionRefAny},
    str::String as AzString,
    svg::LogicalRect,
    widgets::{
        ButtonOnClick, QuickAccessAction, QuickAccessBar, QuickAccessStyle, QuickAccessTheme,
        SliderOnValueChange, StatusBar, StatusBarSegment, StatusBarViewSwitcher, StatusBarZoom,
    },
};

use crate::{
    document::{self, FontCacheSnapshot},
    palette::{self, Palette},
    AppState,
};

pub fn page_sheet_w() -> f32 {
    document::A4_PAGE_W
}
pub fn page_sheet_h() -> f32 {
    document::A4_PAGE_H
}
fn page_pad() -> f32 {
    document::A4_MARGIN
}
pub fn page_stride(zoom: f32) -> f32 {
    (page_sheet_h() * zoom).round() + 16.0
}

fn s(v: &str) -> AzString {
    AzString::from(v)
}

fn word_logo(pal: &Palette) -> Dom {
    Dom::create_div()
        .with_css(
            format!(
                "display: flex; align-items: center; justify-content: center; width: 24px; \
                 height: 24px; background: {}; flex-grow: 0;",
                Palette::hex(pal.brand)
            )
            .as_str(),
        )
        .with_child(crate::fonts::text("W", 13, pal.on_brand))
}

pub fn title_band(
    state: &AppState,
    data: &RefAny,
    pal: &Palette,
    sys: &SystemStyle,
    compact: bool,
) -> Dom {
    let title = format!("{} - AzWriter", state.document.display_name());
    let mut band = QuickAccessBar::office_2013(AzString::from(title)).with_leading(word_logo(pal));
    let mut band_theme = QuickAccessTheme::from_system(SystemStyle::clone(sys));
    band_theme.bg = Palette::TRANSPARENT;
    band.style = QuickAccessStyle::from_theme(band_theme);
    band.actions = vec![
        QuickAccessAction::create(s("save")).with_on_click(
            data.clone(),
            crate::on_save_clicked as ButtonOnClickCallbackType,
        ),
        QuickAccessAction::create(s("undo"))
            .with_on_click(data.clone(), crate::on_undo as ButtonOnClickCallbackType),
        QuickAccessAction::create(s("redo"))
            .with_on_click(data.clone(), crate::on_redo as ButtonOnClickCallbackType),
    ]
    .into();
    let band_bar = band.style.resolved_bar_style();
    crate::fonts::push_ui_font(&mut band.style.bar_style, band_bar);
    if compact {
        band.show_minimize = false;
        band.show_maximize = false;
        band.show_close = false;
    }
    band.dom().with_css("-azul-app-region: drag;")
}

fn canvas(
    state: &AppState,
    data: &RefAny,
    fonts: Option<FontCacheSnapshot>,
    total_pages: usize,
    pal: &Palette,
) -> Dom {
    let _ = state;
    let mut area = Dom::create_div().with_css(
        format!(
            "flex-grow: 1; min-height: 0px; background: {canvas}; display: flex; flex-direction: \
             column; align-items: center; padding-top: 18px; border-top: 1px solid {edge}; \
             overflow: hidden;",
            canvas = Palette::hex(pal.canvas),
            edge = Palette::hex(pal.chrome_edge)
        ),
    );
    let mount_fonts = fonts.clone();
    let vv_payload = RefAny::new(PagesVv {
        app: data.clone(),
        fonts,
        total: total_pages,
        pal: *pal,
    });
    let mount_ctx = RefAny::new(PagesMountCtx {
        app: data.clone(),
        fonts: mount_fonts,
    });
    area.add_child(
        Dom::create_virtual_view(vv_payload, pages_virtual_view)
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                mount_ctx.clone(),
                crate::on_pages_mounted,
            )
            .with_callback(
                EventFilter::Component(ComponentEventFilter::BeforeUnmount),
                mount_ctx,
                crate::on_pages_unmounted,
            )
            .with_ids_and_classes(azul::vec::IdOrClassVec::from(vec![IdOrClass::Class(
                "mw-pages".into(),
            )]))
            .with_css("flex-grow: 1; min-height: 0px; width: 100%;"),
    );
    area
}

pub struct PagesMountCtx {
    pub app: RefAny,
    pub fonts: Option<FontCacheSnapshot>,
}

struct PagesVv {
    total: usize,
    app: RefAny,
    fonts: Option<FontCacheSnapshot>,
    pal: Palette,
}

extern "C" fn pages_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let (mut app, fonts, payload_total, pal) = {
        let Some(vv) = data.downcast_ref::<PagesVv>() else {
            return VirtualViewReturn::default();
        };
        (vv.app.clone(), vv.fonts.clone(), vv.total, vv.pal)
    };
    let (zoom, total, first, count, pages) = {
        let Some(state) = app.downcast_ref::<AppState>() else {
            return VirtualViewReturn::default();
        };

        let zoom = state.zoom_percent / 100.0;
        let stride = (page_sheet_h() * zoom).round() + 16.0;

        let total = payload_total;

        let viewport_h = info.bounds.get_logical_size().height;
        let first_visible = (info.scroll_offset.y.max(0.0) / stride) as usize;
        let first = first_visible.saturating_sub(1);
        let visible = (viewport_h / stride).ceil() as usize + 2;
        let count = visible.max(3).min(total.saturating_sub(first));

        let pages = document::paginate_range_cached(
            &state.document.content,
            state.document.generation,
            fonts.clone(),
            first,
            count,
        );
        (zoom, total, first, count, pages)
    };

    let page_w = (page_sheet_w() * zoom).round();
    let page_h = (page_sheet_h() * zoom).round();
    let pad = (page_pad() * zoom).round() as isize;
    let stride = page_h + 16.0;

    let page_css = format!(
        "width: {}px; height: {}px; background: {sheet}; flex-grow: 0; flex-shrink: 0; border: \
         1px solid {border}; box-shadow: 0px 1px 4px {shadow}; margin-bottom: 16px; box-sizing: \
         border-box; padding: {pad}px; overflow: hidden;",
        page_w as isize,
        page_h as isize,
        sheet = Palette::hex(pal.sheet),
        border = Palette::hex(pal.sheet_border),
        shadow = if pal.dark { "#00000000" } else { "#00000059" },
    );
    let mut col =
        Dom::create_div().with_css("display: flex; flex-direction: column; align-items: center;");
    for page in pages.into_iter().map(|p| p.dom) {
        let page = page
            .with_callback(
                EventFilter::Focus(FocusEventFilter::DocumentEdit),
                app.clone(),
                crate::on_document_edit,
            )
            .with_callback(
                EventFilter::Focus(FocusEventFilter::TextChanged),
                app.clone(),
                crate::on_text_changed,
            )
            .with_ids_and_classes(azul::vec::IdOrClassVec::from(vec![IdOrClass::Class(
                "mw-doc".into(),
            )]))
            .with_css("min-height: 100%;");
        col.add_child(
            Dom::create_div()
                .with_css(page_css.as_str())
                .with_child(page),
        );
    }

    VirtualViewReturn {
        dom: OptionDom::Some(col),
        materialized: LogicalRect {
            origin: LogicalPosition {
                x: 0.0,
                y: first as f32 * stride,
            },
            size: LogicalSize {
                width: page_w + 2.0,
                height: count as f32 * stride,
            },
        },
        virtual_rect: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize {
                width: page_w + 2.0,
                height: total as f32 * stride,
            },
        },
    }
}

pub fn status_bar(
    state: &AppState,
    data: &RefAny,
    page_count: usize,
    pal: &Palette,
    sys: &SystemStyle,
) -> Dom {
    let words = state.document.word_count();
    let segments = vec![
        StatusBarSegment::create(AzString::from(format!("PAGE 1 OF {page_count}"))),
        StatusBarSegment::create(AzString::from(format!("{words} WORDS")))
            .with_marker(state.word_count_marker.clone()),
        StatusBarSegment::create(s("")).with_icon(s("spellcheck")),
        StatusBarSegment::create(s("ENGLISH (UNITED STATES)")),
    ];

    let views = StatusBarViewSwitcher::office_2013()
        .with_active_view(state.view_mode)
        .with_on_select(
            data.clone(),
            StatusBarOnViewSelectCallback {
                cb: crate::on_view_select as StatusBarOnViewSelectCallbackType,
                callable: OptionRefAny::None,
            },
        );

    let mut zoom = StatusBarZoom::office_2013().with_percent(state.zoom_percent);
    zoom.on_zoom_out = Some(button_click(data, crate::on_zoom_out)).into();
    zoom.on_zoom_in = Some(button_click(data, crate::on_zoom_in)).into();
    zoom.on_slider_change = Some(SliderOnValueChange {
        data: data.clone(),
        callback: SliderOnValueChangeCallback {
            cb: crate::on_zoom_slider as SliderOnValueChangeCallbackType,
            callable: OptionRefAny::None,
        },
    })
    .into();

    let mut bar = StatusBar::create(segments).with_views(views).with_zoom(zoom);
    bar.style = crate::palette::widgets::status_bar(pal, sys);
    let status_bar = bar.style.resolved_bar_style();
    crate::fonts::push_ui_font(&mut bar.style.bar_style, status_bar);
    bar.dom()
}

fn button_click(data: &RefAny, cb: ButtonOnClickCallbackType) -> ButtonOnClick {
    use azul::dom::ButtonOnClickCallback;
    ButtonOnClick {
        data: data.clone(),
        callback: ButtonOnClickCallback {
            cb,
            callable: OptionRefAny::None,
        },
    }
}

pub fn editor_screen(
    state: &AppState,
    data: &RefAny,
    fonts: Option<FontCacheSnapshot>,
    max_monitor: Option<LayoutSize>,
    pal: &Palette,
    sys: &SystemStyle,
    compact: bool,
) -> Dom {
    let page_count = match state.exact_page_count {
        Some((generation, n)) if generation == state.document.generation => n,
        _ => {
            let _p = crate::perf::Phase::start("page_count_bounded");
            state
                .document
                .page_count_bounded(fonts.clone(), max_monitor)
                .0
        }
    };

    let title = {
        let _p = crate::perf::Phase::start("title_band");
        title_band(state, data, pal, sys, compact)
    };
    let ribbon = {
        let _p = crate::perf::Phase::start("ribbon");
        crate::ribbon_ui::build(state, data, pal, sys, compact)
    };
    let canvas_dom = {
        let _p = crate::perf::Phase::start("canvas");
        canvas(state, data, fonts, page_count, pal)
    };
    let status = {
        let _p = crate::perf::Phase::start("status_bar");
        status_bar(state, data, page_count, pal, sys)
    };

    let chrome = Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: column; flex-shrink: 0; background: \
                 linear-gradient(to bottom, {}, {});",
                Palette::rgba(palette::widgets::header_bg(sys)),
                Palette::hex(pal.chrome),
            )
            .as_str(),
        )
        .with_child(title)
        .with_child(ribbon);

    Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: column; flex-grow: 1; min-height: 0px; \
                 background: {}; {}",
                Palette::hex(pal.chrome),
                crate::fonts::UI_FONT_CSS
            )
            .as_str(),
        )
        .with_child(chrome)
        .with_child(canvas_dom)
        .with_child(status)
}
