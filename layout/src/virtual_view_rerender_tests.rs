//! A `VirtualView` rendered again shows what its callback drew THIS time.
//!
//! The map editor builds its labels with `Dom::with_css`: the style of a node is a sheet
//! that lives in the DOM, selector-matched by the cascade, and no field of the node's own
//! data shows it. The retained layout tree finds its dirty nodes by the nodes' own
//! fingerprints, so a re-render whose only change was in such a sheet - a label turned
//! another colour, a box one pixel to the right - found every node clean and served the list
//! (and the geometry) it had. The root's pass is handed the diff of its sheets
//! (`begin_reconciliation`); a view's re-render is not, and now lays out cold when the styles
//! of the DOM it built moved.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    callbacks::{VirtualViewCallback, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    refany::RefAny,
    resources::RendererResources,
    styled_dom::StyledDom,
    FastBTreeSet,
};
use azul_css::props::basic::color::ColorU;
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayList, DisplayListItem},
    window::LayoutWindow,
    window_state::FullWindowState,
};

/// How often the view's callback ran.
struct Calls(Arc<AtomicUsize>);

/// What the view draws on its `call`-th render.
#[derive(Clone, Copy)]
enum Drawing {
    /// A box that is red the first time and green the second.
    Colour,
    /// A red box that is 10 px from the left the first time and 50 px the second.
    Place,
    /// The same box every time.
    Same,
}

struct Page(Drawing, Arc<AtomicUsize>);

extern "C" fn drawn(mut data: RefAny, _info: VirtualViewCallbackInfo) -> VirtualViewReturn {
    let (drawing, call) = data
        .downcast_ref::<Page>()
        .map(|page| (page.0, page.1.fetch_add(1, Ordering::SeqCst)))
        .expect("the view's data");
    let (colour, left) = match drawing {
        Drawing::Colour => (if call == 0 { "#ff0000" } else { "#00ff00" }, 10),
        Drawing::Place => ("#ff0000", if call == 0 { 10 } else { 50 }),
        Drawing::Same => ("#ff0000", 10),
    };
    // The box is a child of the view's root, as a map's labels are.
    let dom = Dom::create_div().with_child(Dom::create_div().with_css(&format!(
        "position: absolute; left: {left}px; top: 20px; width: 100px; height: 40px; \
         background: {colour};"
    )));
    let window = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(300.0, 300.0));
    VirtualViewReturn::with_dom(dom, window, window)
}

struct Rendered {
    lw: LayoutWindow,
    host: DomId,
    view: NodeId,
    child: DomId,
    calls: Arc<AtomicUsize>,
}

fn rendered(drawing: Drawing) -> Rendered {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut dom = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_virtual_view(
            RefAny::new(Page(drawing, Arc::clone(&calls))),
            VirtualViewCallback::create(drawn),
        )
        .with_css("position: absolute; left: 0px; top: 0px; width: 300px; height: 300px;"),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(300.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_new_generation(
        styled_dom,
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
    let child = lw
        .virtual_view_manager
        .get_nested_dom_id(host, view)
        .expect("the view's child DOM");
    Rendered {
        lw,
        host,
        view,
        child,
        calls,
    }
}

impl Rendered {
    /// The frame's drain of a queued re-render of the view.
    fn render_again(&mut self) {
        let mut updates = BTreeMap::new();
        let mut views = FastBTreeSet::new();
        views.insert(self.view);
        updates.insert(self.host, views);
        self.lw.queue_virtual_view_updates(updates);
        let window_state = self.lw.current_window_state.clone();
        let updated = self.lw.process_pending_virtual_view_updates(
            &window_state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
        );
        assert_eq!(updated.len(), 1, "the view was rendered again");
    }

    fn list(&self) -> &Arc<DisplayList> {
        &self.lw.layout_results[&self.child].display_list
    }

    /// The boxes of the view's list filled in `colour`, by their left edge.
    fn boxes(&self, colour: ColorU) -> Vec<f32> {
        self.list()
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::Rect { color, bounds, .. } if *color == colour => {
                    Some(bounds.0.origin.x)
                }
                _ => None,
            })
            .collect()
    }
}

const RED: ColorU = ColorU::new(255, 0, 0, 255);
const GREEN: ColorU = ColorU::new(0, 255, 0, 255);

#[test]
fn a_view_rendered_again_shows_the_colour_its_callback_drew_this_time() {
    let mut view = rendered(Drawing::Colour);
    assert_eq!(view.boxes(RED).len(), 1, "harness: red the first time");

    view.render_again();

    assert_eq!(view.calls.load(Ordering::SeqCst), 2, "the callback ran again");
    assert_eq!(view.boxes(GREEN).len(), 1, "the second call drew green: the list holds it");
    assert!(view.boxes(RED).is_empty(), "and no red");
}

#[test]
fn a_view_rendered_again_puts_its_box_where_its_callback_put_it_this_time() {
    let mut view = rendered(Drawing::Place);
    assert_eq!(view.boxes(RED), vec![10.0], "harness: 10 px from the left the first time");

    view.render_again();

    assert_eq!(view.boxes(RED), vec![50.0], "the second call put the box 50 px from the left");
}

#[test]
fn a_view_that_draws_the_same_again_keeps_the_list_it_has() {
    let mut view = rendered(Drawing::Same);
    let before = Arc::as_ptr(view.list());

    view.render_again();

    assert_eq!(view.calls.load(Ordering::SeqCst), 2, "the callback ran again");
    assert_eq!(
        Arc::as_ptr(view.list()),
        before,
        "nothing moved in what it drew: the view keeps its layout and its list (the renderer \
         keeps its copy)"
    );
}
