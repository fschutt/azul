//! A pane of a fixed size keeps its width while the window resizes.
//!
//! Explorer's navigation pane stays 228 px and its preview pane 300 px however
//! wide the window gets; the content takes the change. A split pane shared
//! its container by a RATIO only, so an app that wanted that had to turn the
//! widths into a share of the current window and rebuild its whole window on
//! every width step - AzDrive did (`pane_shares`, `on_resized` -> RefreshDom),
//! 100-180 ms per step - and between the engine's relayout (the old share:
//! the sidebar scaled with the window) and the app's rebuild (the new share:
//! back to 228 px) the sidebar jittered on every step. A fixed-size pane is
//! laid out at its width by the engine, through every relayout.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    widgets::{
        shells::{browser_shell, BrowserShell},
        split_pane::{SplitDirection, SplitPane},
    },
    window::LayoutWindow,
};
use rust_fontconfig::FcFontCache;

fn lay_out(lw: &mut LayoutWindow, styled: StyledDom, width: f32) {
    lw.current_window_state.size.dimensions = LogicalSize::new(width, 600.0);
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
}

/// The width of the first node carrying id or class `name`.
fn width_of(lw: &LayoutWindow, name: &str) -> f32 {
    let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let node = (0..styled.node_data.as_ref().len())
        .map(NodeId::new)
        .find(|n| {
            styled.node_data.as_container()[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_id() == Some(name) || c.as_class() == Some(name))
        })
        .unwrap_or_else(|| panic!("harness: a node named {name}"));
    lw.get_node_size(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    })
    .expect("laid out")
    .width
}

/// The retained DOM laid out again in a window `width` px wide: a resize step.
fn resize(lw: &mut LayoutWindow, width: f32) {
    let styled = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out").styled_dom;
    lay_out(lw, styled, width);
}

fn page(dom: Dom) -> StyledDom {
    StyledDom::create_from_dom(Dom::create_body().with_css("margin: 0; height: 100%;").with_child(dom))
}

fn split(fixed_first: bool) -> Dom {
    let first = Dom::create_div().with_css("height: 100px;");
    let second = Dom::create_div().with_css("height: 100px;");
    let sp = SplitPane::create(SplitDirection::Horizontal, first, second);
    if fixed_first {
        sp.with_fixed_first(228.0).dom()
    } else {
        sp.with_fixed_second(300.0).dom()
    }
}

#[test]
fn a_split_panes_fixed_first_pane_keeps_its_width() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(split(true)), 1000.0);
    for width in [1000.0, 940.0, 1200.0, 760.0, 1001.0] {
        resize(&mut lw, width);
        let first = width_of(&lw, "__azul-native-split-pane-first");
        assert!(
            (first - 228.0).abs() < 0.5,
            "window {width} px: the fixed first pane is {first} px, not its 228 px"
        );
    }
}

#[test]
fn a_split_panes_fixed_second_pane_keeps_its_width() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(split(false)), 1000.0);
    for width in [1000.0, 860.0, 1300.0, 700.0] {
        resize(&mut lw, width);
        let second = width_of(&lw, "__azul-native-split-pane-second");
        assert!(
            (second - 300.0).abs() < 0.5,
            "window {width} px: the fixed second pane is {second} px, not its 300 px"
        );
    }
}

#[test]
fn a_browser_shells_tree_and_preview_keep_their_widths_and_the_content_takes_the_change() {
    let shell = || {
        BrowserShell::create(
            Dom::create_div().with_css("height: 24px;"),
            Dom::create_div().with_css("height: 100px;"),
            Dom::create_div().with_css("height: 100px;"),
        )
        .with_preview(Dom::create_div().with_css("height: 100px;"))
        .with_tree_width(228.0)
        .with_preview_width(300.0)
        .office_shell()
        .dom()
    };
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(shell()), 1280.0);
    let mut content_before = width_of(&lw, browser_shell::CONTENT_ID);
    let mut width_before = 1280.0;
    for width in [1240.0, 1180.0, 1300.0, 900.0] {
        resize(&mut lw, width);
        let tree = width_of(&lw, browser_shell::TREE_ID);
        let preview = width_of(&lw, browser_shell::PREVIEW_ID);
        let content = width_of(&lw, browser_shell::CONTENT_ID);
        assert!((tree - 228.0).abs() < 0.5, "window {width}: the tree is {tree} px, not 228");
        assert!((preview - 300.0).abs() < 0.5, "window {width}: the preview is {preview} px, not 300");
        assert!(
            ((content - content_before) - (width - width_before)).abs() < 0.5,
            "window {width_before} -> {width}: the content changed by {} px, the window by {}",
            content - content_before,
            width - width_before
        );
        content_before = content;
        width_before = width;
    }
}
