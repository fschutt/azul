//! A view that scrolls a long document keeps the rows it rendered when its
//! box only gets shorter or taller along the axis it scrolls: the window of
//! rows it materialized still covers the box, so its callback does not run
//! again, and its child DOM is laid out once more for the new box.
//!
//! The case is a folder list under a ribbon whose content opens: every frame
//! of the opening moves the list's box by a few pixels. Rendering the rows
//! again for each of those frames ran the app's row builder, the cascade of
//! every row and a cold layout of them, about two thirds of each frame. A box
//! that changes along an axis the view lays out to its box (here the width:
//! the rows are as wide as the box) still renders again, as before.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// Height of one row of the list.
const ROW: f32 = 20.0;
/// Rows the whole document has.
const ROWS: usize = 500;
/// Rows the callback materializes from the top: far more than any box here
/// shows, as a list renders a screen either side of the one in view.
const WINDOW_ROWS: usize = 60;

struct ListData {
    calls: Arc<AtomicUsize>,
    /// What the rows' root declares besides its width.
    root_css: &'static str,
}

/// The first `WINDOW_ROWS` rows of a `ROWS`-row document, as wide as the box.
extern "C" fn render_list(mut data: RefAny, info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let root_css = match data.downcast_ref::<ListData>() {
        Some(list) => {
            list.calls.fetch_add(1, Ordering::SeqCst);
            list.root_css
        }
        None => "",
    };
    let width = info.bounds.logical_size.width;
    let mut rows = Dom::create_div().with_css(&format!("width: {width}px; {root_css}"));
    for i in 0..WINDOW_ROWS {
        rows.add_child(
            Dom::create_div_with_text(format!("row {i}")).with_css(&format!("height: {ROW}px;")),
        );
    }
    #[allow(clippy::cast_precision_loss)] // small row counts
    VirtualViewReturn::with_dom(
        rows,
        LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width, WINDOW_ROWS as f32 * ROW),
        ),
        LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width, ROWS as f32 * ROW),
        ),
    )
}

/// A page whose list box is `list_css`, under a bar of `bar_height` px (the
/// ribbon's content, opening).
fn page(calls: &Arc<AtomicUsize>, bar_height: f32, list_css: &str) -> StyledDom {
    page_with_rows(calls, bar_height, list_css, "")
}

/// [`page`] whose rows' root also declares `root_css`.
fn page_with_rows(
    calls: &Arc<AtomicUsize>,
    bar_height: f32,
    list_css: &str,
    root_css: &'static str,
) -> StyledDom {
    let mut dom = Dom::create_body()
        .with_css("display: flex; flex-direction: column; height: 100vh;")
        .with_child(Dom::create_div().with_css(&format!("height: {bar_height}px; flex-shrink: 0;")))
        .with_child(
            Dom::create_virtual_view(
                RefAny::new(ListData {
                    calls: Arc::clone(calls),
                    root_css,
                }),
                VirtualViewCallback::create(render_list),
            )
            .with_css(list_css),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

fn window(width: f32, height: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = ws;
    lw
}

fn lay_out(lw: &mut LayoutWindow, styled_dom: StyledDom) {
    let window_state = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// Lay the page out again from its retained DOM in a window of `height` px,
/// as the frames of a resize or of a height animation do.
fn relayout_at(lw: &mut LayoutWindow, width: f32, height: f32) {
    lw.current_window_state.size.dimensions = LogicalSize::new(width, height);
    let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
    lay_out(lw, result.styled_dom);
}

/// The only child DOM: the list's rows.
fn rows_dom(lw: &LayoutWindow) -> DomId {
    let children: Vec<DomId> = lw
        .layout_results
        .keys()
        .copied()
        .filter(|d| *d != DomId::ROOT_ID)
        .collect();
    assert_eq!(children.len(), 1, "harness: the list's rows are laid out");
    children[0]
}

fn rows_viewport(lw: &LayoutWindow) -> LogicalSize {
    lw.layout_results[&rows_dom(lw)].viewport.size
}

const LIST: &str = "width: 300px; flex-grow: 1; overflow-y: auto;";

#[test]
fn a_shorter_box_keeps_the_rows_and_lays_them_out_for_it() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0, 400.0);
    lay_out(&mut lw, page(&calls, 0.0, LIST));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: one render");
    let rows = rows_dom(&lw);
    assert!(
        (rows_viewport(&lw).height - 400.0).abs() < 0.5,
        "harness: the list fills the window, got {:?}",
        rows_viewport(&lw)
    );

    // The window gets shorter in steps, as a bar above the list grows.
    for (step, height) in [380.0_f32, 350.0, 310.0, 300.0].into_iter().enumerate() {
        relayout_at(&mut lw, 800.0, height);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "step {step}: the list's box got shorter ({height} px) inside the {} px of rows it \
             rendered - its callback must not run again",
            WINDOW_ROWS as f32 * ROW
        );
        assert_eq!(rows_dom(&lw), rows, "step {step}: the rows keep their DOM");
        assert!(
            (rows_viewport(&lw).height - height).abs() < 0.5,
            "step {step}: the kept rows must be laid out for the new box, {height} px high, \
             not the old one: got {:?}",
            rows_viewport(&lw)
        );
    }
}

#[test]
fn a_taller_box_inside_the_rendered_rows_keeps_them() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0, 300.0);
    lay_out(&mut lw, page(&calls, 0.0, LIST));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: one render");

    relayout_at(&mut lw, 800.0, 420.0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the box grew to 420 px, still inside the {} px of rows rendered - nothing to render",
        WINDOW_ROWS as f32 * ROW
    );
    assert!(
        (rows_viewport(&lw).height - 420.0).abs() < 0.5,
        "the kept rows are laid out for the taller box: got {:?}",
        rows_viewport(&lw)
    );
}

#[test]
fn a_box_taller_than_the_rendered_rows_renders_again() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0, 300.0);
    lay_out(&mut lw, page(&calls, 0.0, LIST));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: one render");

    relayout_at(&mut lw, 800.0, 1400.0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the box grew past the {} px of rows rendered - the list must render more",
        WINDOW_ROWS as f32 * ROW
    );
}

#[test]
fn a_box_of_another_width_renders_the_rows_again() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0, 400.0);
    lay_out(
        &mut lw,
        page(&calls, 0.0, "width: 50%; flex-grow: 1; overflow-y: auto;"),
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1, "harness: one render");

    // The rows are as wide as the box: the width is an axis the list lays
    // out to its box, not one it scrolls along.
    relayout_at(&mut lw, 600.0, 400.0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "the list's box changed width - its rows are as wide as the box, so it renders again"
    );
}

/// Where the rows' boxes are, in the child DOM's own space.
fn row_rects(lw: &LayoutWindow) -> Vec<(Option<LogicalPosition>, Option<LogicalSize>)> {
    let result = &lw.layout_results[&rows_dom(lw)];
    (0..result.layout_tree.nodes.len())
        .map(|i| {
            let size = result.layout_tree.nodes[i].used_size;
            let pos = result.calculated_positions.get(i).copied();
            (pos, size)
        })
        .collect()
}

/// Rows as tall as their content do not follow the height of the list's box:
/// a box that only gets shorter keeps their layout and their display list
/// (each frame of a ribbon opening over a folder list), and they stand
/// exactly where a list laid out at that height from scratch puts them.
#[test]
fn rows_as_tall_as_their_content_keep_their_layout_when_the_box_gets_shorter() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0, 400.0);
    lay_out(&mut lw, page(&calls, 0.0, LIST));
    let rows = rows_dom(&lw);
    let list = Arc::clone(&lw.layout_results[&rows].display_list);
    for (step, height) in [380.0_f32, 350.0, 310.0].into_iter().enumerate() {
        relayout_at(&mut lw, 800.0, height);
        assert!(
            Arc::ptr_eq(&list, &lw.layout_results[&rows].display_list),
            "step {step}: only the box's height moved ({height} px) - the rows keep their \
             display list, no pass of theirs ran"
        );
        assert!(
            (rows_viewport(&lw).height - height).abs() < 0.5,
            "step {step}: the result records the new box: got {:?}",
            rows_viewport(&lw)
        );
        let mut fresh = window(800.0, height);
        lay_out(&mut fresh, page(&Arc::new(AtomicUsize::new(0)), 0.0, LIST));
        assert_eq!(
            row_rects(&lw),
            row_rects(&fresh),
            "step {step}: the kept rows stand where a cold layout at {height} px puts them"
        );
    }
}

/// Rows whose root is as tall as the box DO follow its height: they are laid
/// out again, as a resize of their document - the retained tree at the new
/// size, no reconcile of every row.
#[test]
fn rows_as_tall_as_the_box_are_laid_out_again_without_a_reconcile() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut lw = window(800.0, 400.0);
    lay_out(&mut lw, page_with_rows(&calls, 0.0, LIST, "height: 100%;"));
    let rows = rows_dom(&lw);
    for (step, height) in [380.0_f32, 350.0, 310.0].into_iter().enumerate() {
        relayout_at(&mut lw, 800.0, height);
        let cache = &lw.child_layout_caches[&rows];
        assert!(
            cache.last_reconcile_was_skipped,
            "step {step}: only the rows' box moved ({height} px) - their pass takes the \
             retained tree (reused {}, fresh {})",
            cache.last_reconcile_reused,
            cache.last_reconcile_fresh
        );
        let root = lw.layout_results[&rows].layout_tree.nodes[0].used_size;
        assert!(
            root.is_some_and(|s| (s.height - height).abs() < 0.5),
            "step {step}: the root is laid out for the new box: {root:?}"
        );
    }
}
