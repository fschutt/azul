//! A long status text leaves the zoom control in its bar.
//!
//! AzMail's E2E: after File > Print the status bar says "Printed to <the PDF's whole path>", and
//! the `+` of the zoom control - the bar's last part - was laid out past the window's right edge,
//! clipped away, so the click meant for it hit nothing. The bar fitted its window; its parts did
//! not fit the bar, because a text segment never shrank below its text. A status bar is Outlook's:
//! the texts give way (clipped at their segment's edge), the controls at the right end keep
//! their size and their place inside the bar. In both looks.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        statusbar::{StatusBar, StatusBarSegment, StatusBarSync, StatusBarSyncKind, StatusBarZoom},
        themes::UiTheme,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const PRINTED: &str = "Printed to /Users/someone/Library/Application Support/Azlin/mail/print/\
                       Re: the quarterly figures for the board meeting next Tuesday.pdf";
const UP_TO_DATE: &str = "All folders are up to date.   Connected to imap.example.com";

const BAR_WIDTH: f32 = 640.0;

/// AzMail's status bar in `theme`, its notice `notice`, in a container `BAR_WIDTH` wide.
fn page(theme: UiTheme, notice: &str) -> StyledDom {
    let bar = StatusBar::new(
        vec![
            StatusBarSegment::new("Items: 12".into()),
            StatusBarSegment::new(notice.into()),
        ]
        .into(),
    )
    .with_sync(StatusBarSync::create(UP_TO_DATE.into(), StatusBarSyncKind::Connected))
    .with_zoom(StatusBarZoom::create(100.0, 10.0, 500.0))
    .with_theme(theme)
    .dom();
    let mut dom = Dom::create_body()
        .with_css("display: flex; flex-direction: column; margin: 0px;")
        .with_child(
            Dom::create_div()
                .with_css(&format!("display: flex; flex-direction: column; width: {BAR_WIDTH}px;"))
                .with_child(bar),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// Lays `dom` out in `lw` at 1000 x 400.
fn lay_out(lw: &mut LayoutWindow, dom: StyledDom) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(1000.0, 400.0);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the bar lays out");
}

/// The laid-out border boxes of the nodes with the class `class`, in tree order.
fn rects(lw: &LayoutWindow, class: &str) -> Vec<LogicalRect> {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .enumerate()
        .filter(|(_, node)| node.has_class(class))
        .map(|(index, _)| {
            lw.get_node_layout_rect(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
            })
            .unwrap_or_else(|| panic!(".{class} #{index} has no layout rect"))
        })
        .collect()
}

fn one(lw: &LayoutWindow, class: &str) -> LogicalRect {
    let found = rects(lw, class);
    assert_eq!(found.len(), 1, "one .{class}: {found:?}");
    found[0]
}

fn right(r: &LogicalRect) -> f32 {
    r.origin.x + r.size.width
}

fn check(theme: UiTheme) {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");

    lay_out(&mut lw, page(theme, "Ready"));
    let zoom_short = one(&lw, "__azul-native-statusbar-zoom");

    lay_out(&mut lw, page(theme, PRINTED));
    let bar = one(&lw, "__azul-native-statusbar");
    let zoom = one(&lw, "__azul-native-statusbar-zoom");
    let sync = one(&lw, "__azul-native-statusbar-sync");
    let segments = rects(&lw, "__azul-native-statusbar-segment");

    assert!(
        (bar.size.width - BAR_WIDTH).abs() < 0.5,
        "{theme:?}: the bar fits its container: {bar:?}"
    );
    assert!(
        right(&zoom) <= right(&bar) + 0.5 && zoom.origin.x >= bar.origin.x,
        "{theme:?}: the zoom control is laid out inside its bar: {zoom:?} in {bar:?}"
    );
    assert!(
        (zoom.size.width - zoom_short.size.width).abs() < 0.5,
        "{theme:?}: the zoom control keeps its width beside a long text: {} wide, {} beside a \
         short one",
        zoom.size.width,
        zoom_short.size.width
    );
    for part in segments.iter().chain([&sync]) {
        assert!(
            right(part) <= zoom.origin.x + 0.5,
            "{theme:?}: a text part ends before the zoom control: {part:?}, the zoom at x {}",
            zoom.origin.x
        );
        assert!(
            part.size.height <= bar.size.height + 0.5,
            "{theme:?}: a text part stays one line in its bar: {part:?} in {bar:?}"
        );
    }
}

#[test]
fn a_long_status_text_leaves_the_zoom_in_its_flat_bar() {
    check(UiTheme::Flat);
}

#[test]
fn a_long_status_text_leaves_the_zoom_in_its_flora_bar() {
    check(UiTheme::Flora);
}
