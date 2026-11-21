//! A6-13: the scrollbars the registration raises after a layout pass stand beside the layout
//! tree (`managers::scroll_state::RaisedBars`), not in it.
//!
//! The page of every test: `body(0) > view(1)`, the view a `VirtualView` 200 x 100 with
//! `overflow-y: auto` whose callback publishes a document as tall as its data says. A layout
//! pass cannot see that extent (the view holds no flow content), so the pass leaves the view's
//! box without a bar and `register_scroll_nodes` raises it.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::TransformChannel,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    solver3::{
        cache::{TREE_COPIES, TREE_WRITE_COPIES},
        layout_tree::{LayoutNodeId, LayoutTree},
        scrollbar::ScrollbarRequirements,
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};

const ROOT: DomId = DomId::ROOT_ID;

/// The data of the view: how tall the document it publishes is, in px.
struct Extent(Arc<AtomicUsize>);

extern "C" fn page_of_given_height(
    mut data: RefAny,
    _info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let height = data
        .downcast_ref::<Extent>()
        .map(|extent| extent.0.load(Ordering::SeqCst))
        .expect("the view's data");
    let dom = Dom::create_div().with_css("width: 100px; height: 40px;");
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(200.0, 100.0));
    let document = LogicalRect::new(
        LogicalPosition::zero(),
        LogicalSize::new(200.0, height as f32),
    );
    VirtualViewReturn::with_dom(dom, window, document)
}

fn page(extent: &Arc<AtomicUsize>) -> StyledDom {
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(Extent(Arc::clone(extent))),
            VirtualViewCallback::create(page_of_given_height),
        )
        .with_css("overflow-x: hidden; overflow-y: auto; width: 200px; height: 100px;"),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// A window showing [`page`], laid out once (a new generation), with the view's node.
struct ViewWindow {
    lw: LayoutWindow,
    ws: FullWindowState,
    extent: Arc<AtomicUsize>,
    view: NodeId,
}

impl ViewWindow {
    fn new(document_height: usize) -> Self {
        let extent = Arc::new(AtomicUsize::new(document_height));
        let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(400.0, 300.0);
        lw.current_window_state = ws.clone();
        lw.layout_new_generation(
            page(&extent),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let (host, view) = *lw
            .virtual_view_manager
            .all_view_keys()
            .first()
            .expect("the page holds a view");
        assert_eq!(host, ROOT, "harness: the view is a node of the window's DOM");
        Self {
            lw,
            ws,
            extent,
            view,
        }
    }

    /// The app built its DOM again: the view renders again and publishes its extent.
    fn rebuild(&mut self) {
        self.lw
            .layout_new_generation(
                page(&self.extent),
                &self.ws,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("the page lays out again");
    }

    /// The same DOM laid out again for a window `width` wide: what a shell's resize does.
    fn lay_out_again(&mut self, width: f32) {
        let previous = self
            .lw
            .layout_results
            .remove(&ROOT)
            .expect("the root's result");
        let styled = previous.styled_dom;
        drop(previous.layout_tree);
        drop(previous.display_list);
        self.ws.size.dimensions = LogicalSize::new(width, 300.0);
        self.lw.current_window_state = self.ws.clone();
        self.lw
            .layout_and_generate_display_list(
                styled,
                &self.ws,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("the page lays out again");
    }

    /// The layout index of the view's box.
    fn view_box(&self) -> usize {
        self.lw.layout_results[&ROOT]
            .layout_tree
            .dom_to_layout
            .get(&self.view)
            .and_then(|boxes| boxes.first())
            .expect("the view has a box")
            .index()
    }

    /// The tree of the root's result.
    fn tree(&self) -> &LayoutTree {
        &self.lw.layout_results[&ROOT].layout_tree
    }

    /// What every reader after the registration is told about the view's box.
    fn told(&self) -> ScrollbarRequirements {
        self.lw
            .scroll_manager
            .scrollbar_info(ROOT, self.view_box(), self.tree())
            .expect("the layout sized the view's box")
    }

    /// What the tree itself holds for the view's box.
    fn in_the_tree(&self) -> ScrollbarRequirements {
        self.tree()
            .warm(LayoutNodeId::new(self.view_box()))
            .and_then(|warm| warm.scrollbar_info)
            .expect("the layout sized the view's box")
    }

    fn register_again(&mut self) {
        let now = (ExternalSystemCallbacks::rust_internal()
            .get_system_time_fn
            .cb)();
        crate::managers::scroll_registration::register_scroll_nodes(&mut self.lw, &now);
    }
}

/// A view whose published document is taller than its box has a vertical bar, and everything
/// that runs after the registration finds it: the registration's own track, the thumb's
/// transform, the fade's key. The tree the solver's cache shares was not written.
///
/// Mutations: `update_scroll_driven_box` reading `warm.scrollbar_info` (the value before
/// A6-13) -> no thumb transform; `synchronize_scrollbar_opacity` reading it -> no fade key;
/// `raise_bars` dropping the entry -> `told()` has no bar.
#[test]
fn a_view_taller_than_its_box_has_a_bar_that_every_reader_finds() {
    let mut w = ViewWindow::new(1000);
    assert!(
        !w.in_the_tree().needs_vertical,
        "harness: the layout pass sees no extent of a view, the bar is the registration's"
    );
    let told = w.told();
    assert!(told.needs_vertical, "the registration raised the vertical bar: {told:?}");
    assert!(!told.needs_horizontal, "overflow-x: hidden gains no bar: {told:?}");
    assert_eq!(
        w.lw.scroll_manager.raised_bar_count(ROOT),
        1,
        "the bar stands beside the tree, for the view's box alone"
    );
    assert!(
        w.lw
            .scroll_manager
            .get_scrollbar_state(ROOT, w.view, ScrollbarOrientation::Vertical)
            .is_some(),
        "the registration measured the bar's track"
    );
    assert!(
        w.lw
            .gpu_state_manager
            .get_cache(ROOT)
            .and_then(|cache| cache.transform_value(TransformChannel::ScrollbarVertical, w.view))
            .is_some(),
        "the thumb has a transform (the update after the registration read the raised bar)"
    );
    let _ = w.lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    assert!(
        w.lw
            .gpu_state_manager
            .get_cache(ROOT)
            .and_then(|cache| {
                cache.scrollbar_opacity_key(ScrollbarOrientation::Vertical, ROOT, w.view)
            })
            .is_some(),
        "the bar has a fade key (a frame's fade pass read the raised bar)"
    );
}

/// The registration runs more than once on one layout result (the layout funnel, then the
/// shell). A bar raised by one run stands for the next, whatever the extent is by then: flags
/// are only ever raised, and the display list built in between paints the bar.
///
/// Mutation: the set cleared at the top of every registration (the first design of A6-13) ->
/// the second run starts from the tree's own value, finds 50 px in a 100 px box and tells no
/// bar.
#[test]
fn a_second_registration_of_the_same_result_keeps_the_bar_it_raised() {
    let mut w = ViewWindow::new(1000);
    assert!(w.told().needs_vertical, "harness: the first registration raised the bar");
    // The view's document shrinks to less than its box, without a layout pass.
    w.lw.scroll_manager.update_virtual_scroll_bounds(
        ROOT,
        w.view,
        LogicalSize::new(200.0, 50.0),
        None,
    );
    w.register_again();
    assert!(
        w.told().needs_vertical,
        "the bar raised for this result stands until a layout pass makes another result"
    );
    assert!(!w.in_the_tree().needs_vertical, "and the tree is as the pass left it");
}

/// A layout pass makes another result: the bar of the result before it is gone, and the
/// registration after the pass raises none for a document that fits.
#[test]
fn a_pass_for_a_document_that_fits_tells_no_bar() {
    let mut w = ViewWindow::new(1000);
    assert!(w.told().needs_vertical, "harness: the first registration raised the bar");
    w.extent.store(50, Ordering::SeqCst);
    w.rebuild();
    assert_eq!(
        w.lw
            .scroll_manager
            .get_scroll_state(ROOT, w.view)
            .and_then(|state| state.virtual_scroll_size),
        Some(LogicalSize::new(200.0, 50.0)),
        "harness: the view rendered again and published the short document"
    );
    let told = w.told();
    assert!(!told.needs_vertical, "a document that fits has no bar: {told:?}");
    assert_eq!(w.lw.scroll_manager.raised_bar_count(ROOT), 0);
}

/// The same, for a pass that hands the SAME tree on (the same DOM, the same window: nothing to
/// lay out): the set's own check by tree cannot see that the result is another one, the pass
/// says so (`ScrollManager::forget_raised_bars`).
///
/// Mutation: the `forget_raised_bars` call of `layout_dom_recursive_impl` removed -> the set
/// of the tree is kept, the registration starts from the raised flag and tells a bar for a
/// document of 50 px in a box of 100 px.
#[test]
fn a_pass_that_hands_the_same_tree_on_forgets_the_bars_raised_beside_it() {
    let mut w = ViewWindow::new(1000);
    assert!(w.told().needs_vertical, "harness: the first registration raised the bar");
    // The view's document shrinks to less than its box. The pass below keeps the view (it
    // renders nothing again), so the extent the registration after it reads is the one
    // written here; a pass that did render it again would publish the same.
    w.extent.store(50, Ordering::SeqCst);
    w.lw.scroll_manager.update_virtual_scroll_bounds(
        ROOT,
        w.view,
        LogicalSize::new(200.0, 50.0),
        None,
    );
    let tree_before: *const LayoutTree = w.tree();
    w.lay_out_again(400.0);
    let tree_after: *const LayoutTree = w.tree();
    assert_eq!(
        w.lw
            .scroll_manager
            .get_scroll_state(ROOT, w.view)
            .and_then(|state| state.virtual_scroll_size),
        Some(LogicalSize::new(200.0, 50.0)),
        "harness: the view's extent after the pass is the short one"
    );
    assert!(
        core::ptr::eq(tree_before, tree_after),
        "harness: a pass with nothing to lay out hands the tree on (the set of the tree pins \
         its allocation, so another tree cannot stand at its address)"
    );
    let told = w.told();
    assert!(!told.needs_vertical, "the pass forgot the bar of the result before: {told:?}");
}

/// THE COUNT: a layout pass of a DOM whose view raises a bar, and the registration after it,
/// copy no layout tree. The registration used to write the raised flag into the result's tree
/// (`Arc::make_mut`), which the solver's cache shares after a pass: every node of the DOM
/// copied, per pass.
///
/// Mutation: `ScrollManager::raise_bars` writing through `cache::own_mut` (or
/// `DomLayoutResult::layout_tree_mut`) while the cache shares the tree -> one write copy per
/// pass: the counter reads 3.
#[test]
fn a_layout_pass_of_a_view_that_raises_a_bar_copies_no_layout_tree() {
    let mut w = ViewWindow::new(1000);
    assert!(w.told().needs_vertical, "harness: the first registration raised the bar");
    TREE_COPIES.with(|copies| copies.set(0));
    TREE_WRITE_COPIES.with(|copies| copies.set(0));
    for width in [380.0_f32, 360.0, 420.0] {
        w.lay_out_again(width);
        assert!(
            w.told().needs_vertical,
            "harness: the registration after the pass for {width} px raised the bar again"
        );
        assert_eq!(w.lw.scroll_manager.raised_bar_count(ROOT), 1);
    }
    assert_eq!(
        TREE_WRITE_COPIES.with(std::cell::Cell::get),
        0,
        "the registration wrote into a tree the solver's cache shares"
    );
    assert_eq!(
        TREE_COPIES.with(std::cell::Cell::get),
        0,
        "a pass copied a layout tree something still held"
    );
}

/// A display list built from the result's tree outside a layout pass reads the scrollbars
/// from the tree (`paint_scrollbars`): the raised ones are folded into the result's own tree
/// first, once, and every reader goes on finding them.
///
/// Mutation: the `settle_raised_bars` call of `regenerate_display_list_for_dom` removed -> the
/// tree's own flag stays unraised and the build paints from it.
#[test]
fn a_display_list_built_from_the_result_finds_the_raised_bar_in_the_tree() {
    let mut w = ViewWindow::new(1000);
    assert!(!w.in_the_tree().needs_vertical, "harness: the bar stands beside the tree");
    TREE_WRITE_COPIES.with(|copies| copies.set(0));
    w.lw.regenerate_display_list_for_dom(ROOT);
    assert!(w.in_the_tree().needs_vertical, "the build found the bar in the tree");
    assert!(w.told().needs_vertical, "and so does every other reader");
    assert_eq!(w.lw.scroll_manager.raised_bar_count(ROOT), 0, "the tree holds it now");
    w.lw.regenerate_display_list_for_dom(ROOT);
    w.register_again();
    assert!(w.told().needs_vertical, "a later registration starts from the tree's flag");
    assert_eq!(
        TREE_WRITE_COPIES.with(std::cell::Cell::get),
        1,
        "the tree became the result's own once; later builds and registrations copy nothing"
    );
}

/// The set of one tree answers for that tree alone, by identity: a copy of the tree at
/// another address, or the tree of another DOM, gets its own value.
///
/// Mutation: `scrollbar_info` without the `ptr::eq` test -> the copy is told the raised bar.
#[test]
fn bars_raised_beside_a_tree_are_told_for_that_tree_alone() {
    let w = ViewWindow::new(1000);
    let view_box = w.view_box();
    let copy: LayoutTree = w.tree().clone();
    let of_copy = w
        .lw
        .scroll_manager
        .scrollbar_info(ROOT, view_box, &copy)
        .expect("the copy holds the layout's own answer");
    assert!(!of_copy.needs_vertical, "another tree is told its own value: {of_copy:?}");
    assert!(w.told().needs_vertical, "the tree the bar was raised for is told the bar");
}

/// `raise_bars` writes into a tree the caller holds alone (a hand-built result, a result a
/// text edit made its own) and beside one that is shared; `settle_raised_bars` then writes
/// the shared one's into the caller's own copy and forgets the set.
#[test]
fn a_bar_is_raised_in_a_tree_held_alone_and_beside_a_shared_one() {
    let w = ViewWindow::new(1000);
    let view_box = w.view_box();
    let raised = w.told();
    let mut manager = crate::managers::scroll_state::ScrollManager::new();

    // Held alone: in the tree, no set.
    let mut alone = Arc::new(w.tree().clone());
    manager.raise_bars(ROOT, &mut alone, view_box, raised);
    assert_eq!(manager.raised_bar_count(ROOT), 0);
    assert!(alone
        .warm(LayoutNodeId::new(view_box))
        .and_then(|warm| warm.scrollbar_info)
        .is_some_and(|reqs| reqs.needs_vertical));

    // Shared: beside the tree, the tree untouched.
    let mut shared = Arc::new(w.tree().clone());
    let other_holder = Arc::clone(&shared);
    manager.raise_bars(ROOT, &mut shared, view_box, raised);
    assert_eq!(manager.raised_bar_count(ROOT), 1);
    assert!(Arc::ptr_eq(&shared, &other_holder), "no copy was made to write to");
    assert!(!other_holder
        .warm(LayoutNodeId::new(view_box))
        .and_then(|warm| warm.scrollbar_info)
        .is_some_and(|reqs| reqs.needs_vertical));
    assert!(manager
        .scrollbar_info(ROOT, view_box, &shared)
        .is_some_and(|reqs| reqs.needs_vertical));

    // A registration of the same tree keeps the set, one of another tree drops it.
    manager.keep_raised_bars_of(ROOT, &shared);
    assert_eq!(manager.raised_bar_count(ROOT), 1);

    // Settled: into the caller's own copy, the other holder's tree as it was.
    manager.settle_raised_bars(ROOT, &mut shared);
    assert_eq!(manager.raised_bar_count(ROOT), 0);
    assert!(!Arc::ptr_eq(&shared, &other_holder), "the write went to a copy");
    assert!(shared
        .warm(LayoutNodeId::new(view_box))
        .and_then(|warm| warm.scrollbar_info)
        .is_some_and(|reqs| reqs.needs_vertical));
    assert!(!other_holder
        .warm(LayoutNodeId::new(view_box))
        .and_then(|warm| warm.scrollbar_info)
        .is_some_and(|reqs| reqs.needs_vertical));

    manager.raise_bars(ROOT, &mut shared, view_box, raised);
    let mut another = Arc::new(w.tree().clone());
    let _held = Arc::clone(&another);
    manager.raise_bars(ROOT, &mut another, view_box, raised);
    manager.keep_raised_bars_of(ROOT, &shared);
    assert_eq!(
        manager.raised_bar_count(ROOT),
        0,
        "the set of another tree goes at the top of a registration"
    );
}
