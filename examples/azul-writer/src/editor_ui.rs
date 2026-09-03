//! The main editing screen: title band (QAT), ribbon, page canvas and
//! status bar — the Office-2013-era look print-layout look.

use azul::callbacks::{
    ButtonOnClickCallbackType, RefAny, SliderOnValueChangeCallbackType,
    StatusBarOnViewSelectCallbackType, VirtualViewCallbackInfo, VirtualViewReturn,
};
use azul::component::ComponentEventFilter;
use azul::css::{EventFilter, FocusEventFilter, LayoutSize, LogicalPosition, LogicalSize};
use azul::dom::{Dom, IdOrClass, SliderOnValueChangeCallback, StatusBarOnViewSelectCallback};
use azul::option::{OptionDom, OptionRefAny};
use azul::str::String as AzString;
use azul::svg::LogicalRect;
use azul::css::SystemStyle;
use azul::widgets::{
    ButtonOnClick, QuickAccessAction, QuickAccessBar, QuickAccessStyle, QuickAccessTheme,
    SliderOnValueChange,
    StatusBar, StatusBarSegment, StatusBarViewSwitcher, StatusBarZoom,
};

use crate::document::{self, FontCacheSnapshot};
use crate::palette::{self, Palette};
use crate::AppState;

/// #28 (b): sheet geometry derives from the ONE `document::A4_*` source —
/// the same constants the pagination content box is computed from. (The
/// sheet frame is the FULL page; the pagination content box subtracts the
/// margins in `document::page_content_size`.)
pub fn page_sheet_w() -> f32 {
    document::A4_PAGE_W
}
pub fn page_sheet_h() -> f32 {
    document::A4_PAGE_H
}
fn page_pad() -> f32 {
    document::A4_MARGIN
}
/// #28(c): one page plus its bottom margin at `zoom` — the sheet-stack row
/// stride shared by the VirtualView window math and the background
/// writeback's scrollbar correction.
pub fn page_stride(zoom: f32) -> f32 {
    (page_sheet_h() * zoom).round() + 16.0
}

fn s(v: &str) -> AzString {
    AzString::from(v)
}

/// The accent "W" application logo square in the title band.
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

/// Title band: Word logo, quick-access save/undo/redo, customize chevron,
/// centered "<name> - AzWriter", help + window buttons.
pub fn title_band(state: &AppState, data: &RefAny, pal: &Palette, sys: &SystemStyle) -> Dom {
    let title = format!("{} - AzWriter", state.document.display_name());
    let mut band =
        QuickAccessBar::office_2013(AzString::from(title)).with_leading(word_logo(pal));
    // The band IS this window's titlebar, so it takes the DESKTOP's titlebar
    // colours (header fill, its own close-button hover) rather than the
    // The band draws NO fill of its own: `editor_screen` runs one gradient
    // behind it and the ribbon, so the window reads as a single surface from
    // the title bar down instead of two bands butted together. Everything else
    // about the band still comes from the desktop.
    let mut band_theme = QuickAccessTheme::from_system(SystemStyle::clone(sys));
    band_theme.bg = Palette::TRANSPARENT;
    band.style = QuickAccessStyle::from_theme(band_theme);
    band.actions = vec![
        QuickAccessAction::new(s("save")).with_on_click(
            data.clone(),
            crate::on_save_clicked as ButtonOnClickCallbackType,
        ),
        QuickAccessAction::new(s("undo"))
            .with_on_click(data.clone(), crate::on_undo as ButtonOnClickCallbackType),
        QuickAccessAction::new(s("redo"))
            .with_on_click(data.clone(), crate::on_redo as ButtonOnClickCallbackType),
    ]
    .into();
    crate::fonts::push_ui_font(&mut band.style.bar_style);
    // The title band IS the window's drag handle — `-azul-app-region: drag`,
    // the same rule Electron spells `-webkit-app-region`. Dragging it hands the
    // gesture to the window manager, and double-clicking it toggles
    // maximize/restore, exactly as a native title bar does.
    //
    // The property does not cascade, so the quick-access BUTTONS inside the bar
    // keep working: the framework walks up from whatever the press landed on
    // and only treats it as a drag once it reaches a node that declares `drag`.
    // A control that must never drag can stop that walk with `no-drag`.
    band.dom().with_css("-azul-app-region: drag;")
}

/// The dark-gray print-layout canvas with the paginated white sheets.
/// EVERY page comes out of `document::paginate()` — the engine cuts the
/// cached content DOM at its structural break paths.
///
/// Sizing is pure flexbox (ENGINE-ISSUE 3 resolution): the app root is a
/// definite-height column (`body { display:flex; height:100% }`), so
/// `flex-grow: 1; min-height: 0` gives the canvas exactly the leftover
/// height and `overflow: hidden` crops the sheets like classic office suites do.
/// `align-items: center` centers the sheet stack in-flow.
fn canvas(
    state: &AppState,
    data: &RefAny,
    fonts: Option<FontCacheSnapshot>,
    total_pages: usize,
    pal: &Palette,
) -> Dom {
    let _ = state;
    let mut area = Dom::create_div().with_css(
        // `overflow: hidden`, NOT `auto`: the scroll container is the
        // VirtualView below, not this box. `compute_scroll_ids` hands a scroll
        // id — and with it the wheel target — to every node whose overflow is
        // hidden/scroll/auto, so an `auto` canvas swallowed the wheel that the
        // VirtualView needs to page, and (with padding-top: 18px inflating the
        // Taffy content size) grew a phantom scrollbar with a full-track thumb.
        // `hidden` still crops the sheet stack the way classic office suites do
        // and stays out of the wheel-target list.
        format!(
            // The top border is the CHROME EDGE: the boundary between the
            // app's controls and the document. Without it a dark ribbon and a
            // dark canvas read as one surface and the ribbon appears to float
            // in nothing.
            "flex-grow: 1; min-height: 0px; background: {canvas}; display: flex; \
             flex-direction: column; align-items: center; padding-top: 18px; \
             border-top: 1px solid {edge}; overflow: hidden;",
            canvas = Palette::hex(pal.canvas),
            edge = Palette::hex(pal.chrome_edge)
        ),
    );
    // #28: pages materialize lazily — the callback renders a ~3-page window
    // around the scroll position; everything else exists only as scrollbar
    // geometry (`virtual_scroll_size`).
    let mount_fonts = fonts.clone();
    let vv_payload = RefAny::new(PagesVv {
        app: data.clone(),
        fonts,
        total: total_pages,
        pal: *pal,
    });
    // #28(c): mount/unmount drive the STREAMING background pagination — the
    // mount callback spawns the chunk-loop thread (needs the font cache, so
    // it gets its own ctx payload; the event CallbackInfo has no fonts
    // accessor), the unmount callback CANCELS it (doc/app closed before the
    // load finished — USER design).
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
            // The VirtualView node IS the scroll container (same as
            // examples/rust/src/infinity.rs). No overflow CSS needed: the UA
            // stylesheet defaults a VirtualView to `overflow: auto`, and the
            // virtual-size-aware necessity rule paints/updates the bar exactly
            // when the published `virtual_scroll_size` overflows the viewport.
            // (A VV that must not wheel-scroll opts out with `overflow: hidden`,
            // as the map and video widgets do.)
            .with_ids_and_classes(azul::vec::IdOrClassVec::from(vec![IdOrClass::Class(
                "mw-pages".into(),
            )]))
            .with_css("flex-grow: 1; min-height: 0px; width: 100%;"),
    );
    area
}

/// #28(c): payload of the pages VirtualView's mount/unmount callbacks — the
/// app handle plus the font cache the background pagination must use (SAME
/// fonts as the main thread, or the thread's break metrics would diverge).
pub struct PagesMountCtx {
    pub app: RefAny,
    pub fonts: Option<FontCacheSnapshot>,
}

/// Payload of the page-canvas `VirtualView` (#28).
struct PagesVv {
    /// #28(c): the page count the canvas was built with (exact or the
    /// monitor-bounded estimate) — the VirtualView's virtual size derives
    /// from THIS, not from a fresh pagination: the payload refreshes on
    /// every RefreshDom (including the exact-count writeback's), and in
    /// between, `update_virtual_view` has already corrected the scroll
    /// bounds directly.
    total: usize,
    /// The app data — for reading `AppState` at materialization time and for
    /// wiring the `DocumentEdit` callback onto materialized pages.
    app: RefAny,
    fonts: Option<FontCacheSnapshot>,
    /// The sheet colours. Carried in the payload because the VirtualView
    /// callback runs OUTSIDE `layout()` (the manager re-invokes it on scroll)
    /// and has no `LayoutCallbackInfo` to re-derive them from; the payload is
    /// rebuilt on every DOM regeneration, so a theme switch refreshes it.
    pal: Palette,
}

/// #28: materialize pages `first−1 ..= last_visible+1` around the scroll
/// position. The engine styles/lays out/renders ONLY these; the rest of the
/// document exists as `virtual_scroll_size` (scrollbar geometry). The
/// manager re-invokes on `EdgeScrolled` (200px threshold), bounds expansion
/// and DOM regeneration.
extern "C" fn pages_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    // RefAny downcasts take `&mut self` (borrow bookkeeping), so clone the
    // handles out of the payload borrow first (refcount bumps only).
    let (mut app, fonts, payload_total, pal) = {
        let Some(vv) = data.downcast_ref::<PagesVv>() else {
            return VirtualViewReturn::default();
        };
        (vv.app.clone(), vv.fonts.clone(), vv.total, vv.pal)
    };
    // Everything that needs `state` happens inside this scope — the borrow
    // must end before `app` is cloned into page callbacks below (E0502).
    let (zoom, total, first, count, pages) = {
        let Some(state) = app.downcast_ref::<AppState>() else {
            return VirtualViewReturn::default();
        };

        let zoom = state.zoom_percent / 100.0;
        // One page plus its bottom margin — the row stride of the sheet
        // stack.
        let stride = (page_sheet_h() * zoom).round() + 16.0;

        // #28(c): the payload's count (exact or monitor-bounded estimate) —
        // NOT a fresh pagination: on a huge document that call would compute
        // full break paths and defeat the bounded first paint.
        let total = payload_total;

        let viewport_h = info.bounds.get_logical_size().height;
        // Where the user is looking. This used to read `virtual_scroll_offset`,
        // which the engine hardcoded to zero — so `first` was always 0 and the
        // same first pages were materialized no matter how far you scrolled.
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
        "width: {}px; height: {}px; background: {sheet}; flex-grow: 0; \
         flex-shrink: 0; border: 1px solid {border}; box-shadow: 0px 1px 4px {shadow}; \
         margin-bottom: 16px; box-sizing: border-box; padding: {pad}px; \
         overflow: hidden;",
        page_w as isize,
        page_h as isize,
        sheet = Palette::hex(pal.sheet),
        border = Palette::hex(pal.sheet_border),
        // A drop shadow reads as depth only against a lighter surround. On a
        // dark canvas the same black smudge just muddies the sheet edge, so
        // the border carries the separation there and the shadow stands down.
        shadow = if pal.dark { "#00000000" } else { "#00000059" },
    );
    let mut col =
        Dom::create_div().with_css("display: flex; flex-direction: column; align-items: center;");
    for page in pages.into_iter().map(|p| p.dom) {
        // C11: the engine records structural edits on this editable subtree
        // and fires DocumentEdit; the app applies them to its model.
        let page = page
            .with_callback(
                EventFilter::Focus(FocusEventFilter::DocumentEdit),
                app.clone(),
                crate::on_document_edit,
            )
            // Focus anchor: startup focus targets this class so the caret
            // exists the moment the window opens (classic office-suite behavior). The window
            // includes page 0 at startup by construction.
            .with_ids_and_classes(azul::vec::IdOrClassVec::from(vec![IdOrClass::Class(
                "mw-doc".into(),
            )]))
            // The WHOLE writable area of the sheet is the editing host, the way
            // a word processor's page is: `with_css` appends, so this only adds
            // a floor to the content root's height.
            //
            // Without it the content root is exactly as tall as its blocks — on
            // a new document, one empty line — so a click anywhere else on the
            // white page landed on the sheet `<div>`, which is not editable and
            // not focusable. Nothing took the click, no caret appeared, and the
            // document could not be typed into. Filling the sheet's content box
            // (`height: 1123px` minus the 96px margins, via `box-sizing:
            // border-box`) makes every click inside the margins reach the
            // editable host, which then seeds the caret at the end of the text.
            .with_css("min-height: 100%;");
        col.add_child(Dom::create_div().with_css(page_css.as_str()).with_child(page));
    }

    VirtualViewReturn {
        dom: OptionDom::Some(col),
        // What we just materialized, and WHERE it sits in the document: pages
        // `first..first+count`, so the window starts at `first * stride`. The
        // engine places it at `window_origin - scroll_offset`.
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
        // The whole document — scrollbar geometry only. A background exact
        // pagination pass may refine `total` later; that re-scales the bar
        // without moving the page under the cursor.
        virtual_rect: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize {
                width: page_w + 2.0,
                height: total as f32 * stride,
            },
        },
    }
}

/// the Office-2013-era look status bar: page / word count / language left, view switcher
/// and zoom cluster right.
pub fn status_bar(
    state: &AppState,
    data: &RefAny,
    page_count: usize,
    pal: &Palette,
    sys: &SystemStyle,
) -> Dom {
    let words = state.document.word_count();
    let segments = vec![
        StatusBarSegment::new(AzString::from(format!("PAGE 1 OF {page_count}"))),
        StatusBarSegment::new(AzString::from(format!("{words} WORDS"))),
        StatusBarSegment::new(s("")).with_icon(s("spellcheck")),
        StatusBarSegment::new(s("ENGLISH (UNITED STATES)")),
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

    let mut bar = StatusBar::new(segments).with_views(views).with_zoom(zoom);
    // A BRAND-filled strip over the desktop's neutrals - the office blue is
    // AzWriter's, not the session's accent.
    bar.style = crate::palette::widgets::status_bar(pal, sys);
    crate::fonts::push_ui_font(&mut bar.style.bar_style);
    bar.dom()
}

/// A `ButtonOnClick` wrapper for a bare callback fn (the field-assigned
/// zoom hooks take the wrapper struct, unlike the `with_on_click` setters).
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

/// The complete editor screen (vertical: title band / ribbon / canvas /
/// status bar).
pub fn editor_screen(
    state: &AppState,
    data: &RefAny,
    fonts: Option<FontCacheSnapshot>,
    max_monitor: Option<LayoutSize>,
    pal: &Palette,
    sys: &SystemStyle,
) -> Dom {
    // Dynamic pagination: the engine cuts the cached content DOM at its
    // structural break paths; pages build LAZILY inside the canvas
    // VirtualView (a 3-page-ish window around the scroll position).
    // #28(c): the COUNT is layered — the background thread's EXACT count
    // (generation-matched) wins; otherwise the monitor-bounded first count:
    // a huge file's open paginates only a monitor-sized prefix and ESTIMATES
    // the rest, and the writeback corrects status bar + scrollbar as the
    // exact result lands.
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
        title_band(state, data, pal, sys)
    };
    let ribbon = {
        let _p = crate::perf::Phase::start("ribbon");
        crate::ribbon_ui::build(state, data, pal, sys)
    };
    let canvas_dom = {
        let _p = crate::perf::Phase::start("canvas");
        canvas(state, data, fonts, page_count, pal)
    };
    let status = {
        let _p = crate::perf::Phase::start("status_bar");
        status_bar(state, data, page_count, pal, sys)
    };

    // ONE surface from the title bar to the bottom of the ribbon.
    //
    // The band and the ribbon paint no fill of their own (see `title_band` and
    // `palette::widgets::ribbon`); this gradient behind both is what they show,
    // running from the desktop's title-bar colour into the app's chrome so the
    // two flow into each other instead of meeting at a hard edge. On the
    // WRAPPER rather than on the body: the stops then span exactly the chrome,
    // where a percentage of the body would slide with the window's height.
    let chrome = Dom::create_div()
        .with_css(
            format!(
                "display: flex; flex-direction: column; flex-shrink: 0; \
                 background: linear-gradient(to bottom, {}, {});",
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
