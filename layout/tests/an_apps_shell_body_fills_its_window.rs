//! An app's shell body fills its window, and an install wizard keeps its buttons in it.
//!
//! The wave-6 LOOK at AzSetup, AzShells and AzCalculator (headless screenshots): every app that
//! returned `Dom::create_body().with_css("display: flex; flex-direction: column;")` around its
//! `ShellThemeScope` showed the shell in a band at the top of the window - the UA's 8px body
//! margin around it, its panes collapsed to their content (S5's tree / content / preview had no
//! height at all), the rest of the window the canvas' white, in dark mode too - and AzSetup's
//! wizard lost its button row under the window's bottom edge on the components page. The body is
//! an HTML body: a UA margin and an automatic height. `ShellThemeScope::body()` is the ONE root
//! an app returns: the body without the margin, the window's full height, the scope growing in it.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_css::AzString;
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        shells::{ShellThemeScope, UtilityShell},
        themes::UiTheme,
        wizard_layout::{WizardLayout, WizardLayoutSize, WizardLayoutStyle},
        wizard_pages::{WizardComponent, WizardComponentVec, WizardComponentsPage},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 640.0;
const H: f32 = 514.0;

/// Lays `dom` out in `lw` at `W` x `H`.
fn lay_out(lw: &mut LayoutWindow, mut dom: Dom) {
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the window lays out");
}

/// The laid-out border box of the first node that `matches`.
fn rect_where(
    lw: &LayoutWindow,
    what: &str,
    matches: impl Fn(&azul_core::dom::NodeData) -> bool,
) -> LogicalRect {
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the root DOM is laid out");
    let index = result
        .styled_dom
        .node_data
        .as_container()
        .internal
        .iter()
        .position(matches)
        .unwrap_or_else(|| panic!("no node {what}"));
    lw.get_node_layout_rect(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(index))),
    })
    .unwrap_or_else(|| panic!("{what} has no layout rect"))
}

fn rect_of_id(lw: &LayoutWindow, id: &str) -> LogicalRect {
    rect_where(lw, &format!("#{id}"), |n| n.has_id(id))
}

fn rect_of_class(lw: &LayoutWindow, class: &str) -> LogicalRect {
    rect_where(lw, &format!(".{class}"), |n| n.has_class(class))
}

#[test]
fn the_scope_body_puts_the_app_edge_to_edge_in_its_window() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let content = Dom::create_div()
        .with_id("content".into())
        .with_css("flex-grow: 1;");
    lay_out(
        &mut lw,
        ShellThemeScope::create(content)
            .with_theme(UiTheme::Flat)
            .body(),
    );
    let scope = rect_of_class(&lw, "__azul-native-theme-scope");
    let content = rect_of_id(&lw, "content");
    for (what, r) in [("the scope", scope), ("the content", content)] {
        assert!(
            r.origin.x.abs() < 0.5 && r.origin.y.abs() < 0.5,
            "{what} starts at the window's corner, not inside a UA margin: {r:?}"
        );
        assert!(
            (r.size.width - W).abs() < 0.5 && (r.size.height - H).abs() < 0.5,
            "{what} takes the whole {W} x {H} window: {r:?}"
        );
    }
}

/// AzSetup's components page: more rows than the classic wizard's page has room for.
fn components_page() -> Dom {
    let mut rows = Vec::new();
    for i in 0..14u32 {
        rows.push(
            WizardComponent::create(AzString::from(format!("Component {i}")), 10 * 1024 * 1024)
                .with_depth(i % 2)
                .with_checked(true),
        );
    }
    WizardComponentsPage::create(WizardComponentVec::from_vec(rows))
        .with_theme(UiTheme::Flat)
        .dom()
}

#[test]
fn an_install_wizard_keeps_its_buttons_in_its_window_and_scrolls_its_list() {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let wizard = WizardLayout::create(
        AzString::from("AzOffice Setup"),
        vec![
            AzString::from("Welcome"),
            AzString::from("Components"),
            AzString::from("Done"),
        ]
        .into(),
    )
    .with_current_step(1)
    .with_style(WizardLayoutStyle::Banner)
    .with_size(WizardLayoutSize::Classic)
    .with_page(components_page())
    .with_theme(UiTheme::Flat)
    .dom();
    let title = Dom::create_div()
        .with_id("title".into())
        .with_css("height: 34px; flex-shrink: 0;");
    let shell = UtilityShell::create(wizard)
        .with_title_row(title)
        .with_theme(UiTheme::Flat)
        .dom();
    lay_out(&mut lw, ShellThemeScope::create(shell).with_theme(UiTheme::Flat).body());

    let buttons = rect_of_class(&lw, "__azul-native-wizard-layout-buttons");
    assert!(
        buttons.origin.y + buttons.size.height <= H + 0.5,
        "the button row ends inside the {H}px window: {buttons:?}"
    );
    assert!(buttons.size.height > 10.0, "the button row has its height: {buttons:?}");
    let page = rect_of_class(&lw, "__azul-native-wizard-components");
    assert!(
        page.origin.y + page.size.height <= buttons.origin.y + 0.5,
        "the page ends above the buttons: page {page:?}, buttons {buttons:?}"
    );
    let total = rect_of_class(&lw, "__azul-native-wizard-components-total");
    assert!(
        total.origin.y + total.size.height <= buttons.origin.y + 0.5,
        "the total stays above the buttons - the list scrolls instead: {total:?}"
    );
}
