//! diff-first: a display list at rest holds no translated item; only the frames of a layout
//! tween translate, and the tween's last frame paints what they translated
//! (`audit/diff-first.md`, diff-first-15).

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::props::{layout::LayoutHeight, property::CssProperty};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    overlay::ContentChange,
    solver3::display_list::{translate_item, DisplayList, EmitPhase, MovedRunPolicy},
    window::LayoutWindow,
    window_state::FullWindowState,
};

/// The window the pages are laid out in first, and the one they are resized to. The boxes
/// that move come from coordinates in the thousands to coordinates in the hundreds: the
/// move is then a float on a coarser grid (2^-12 px) than the place a box arrives at
/// (2^-14 px and finer), so `old + (new - old)` cannot be `new` unless `new` happens to
/// lie on the coarse grid - one position in four at best, and no page of forty rows at
/// forty places has them all there.
const BEFORE: (f32, f32) = (4313.0, 3207.0);
const AFTER: (f32, f32) = (413.0, 607.0);

/// Forty rows per column that moves.
const MOVING: usize = 40;
/// Eight rows that stay.
const STILL: usize = 8;

/// A row: one box of a fixed size that paints one rect, its background, at its position.
/// Each row of a column stands at another x (a margin of `i` x 1.7 px).
fn row(i: usize) -> Dom {
    let tenths = i * 17;
    Dom::create_div().with_css(&format!(
        "width: 90.3px; height: 13.3px; margin-left: {}.{}px; background: #336699;",
        tenths / 10,
        tenths % 10
    ))
}

/// A column of `rows` rows, placed by `css`. Its width is fixed and its height is its
/// rows': a resize of the window changes neither.
fn column(css: &str, rows: usize) -> Dom {
    let mut column = Dom::create_div().with_css(css);
    for i in 0..rows {
        column = column.with_child(row(i));
    }
    column
}

/// A page of three columns of rows. `still` rows stand at the window's top left and stay
/// there whatever the window's size. `moving` rows hang at the window's right and bottom
/// edge, and `moving` more stand in the flow behind a margin of 61.7 % of the window's
/// width: a resize moves both columns and every row in them, and changes the size of none
/// of these boxes (the body's alone). No column that would be empty is built.
fn page(still: usize, moving: usize) -> Dom {
    let mut body = Dom::create_body().with_css("margin: 0px;");
    if moving > 0 {
        body = body.with_child(column("margin-left: 61.7%; width: 220px;", moving));
    }
    if still > 0 {
        body = body.with_child(column(
            "position: absolute; left: 3.7px; top: 5.1px; width: 220px;",
            still,
        ));
    }
    if moving > 0 {
        body = body.with_child(column(
            "position: absolute; right: 10.3px; bottom: 7.9px; width: 220px;",
            moving,
        ));
    }
    body
}

/// `dom` laid out in a window of `size`, its display list built.
fn laid_out(dom: Dom, size: (f32, f32), policy: MovedRunPolicy) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    // What this window's patched builds do with a moved box's run, whatever the
    // environment says (`AZ_DL_TRANSLATE_MOVED_RUNS`); no other window is told.
    lw.display_list_options.moved_runs = policy;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(size.0, size.1);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    lw
}

/// The window of `lw` gets another size and its DOM is laid out again, as the shells do
/// it for a resize (`incremental_relayout_for_resize`): the DOM that is laid out, taken
/// out of its result and handed in again, with the solver told that only the viewport
/// moved. The pass keeps the layout tree and patches the display list.
fn resize(lw: &mut LayoutWindow, size: (f32, f32)) {
    let mut ws = lw.current_window_state.clone();
    ws.size.dimensions = LogicalSize::new(size.0, size.1);
    lw.layout_cache.resize_only_hint = true;
    let kept = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("the page is laid out");
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        kept.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

/// What a pass left of the window's DOM: its list, and per layout box its position and size.
struct Pass {
    list: DisplayList,
    positions: Vec<LogicalPosition>,
    sizes: Vec<Option<LogicalSize>>,
}

fn pass_of(lw: &LayoutWindow) -> Pass {
    let result = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the page is laid out");
    Pass {
        list: (*result.display_list).clone(),
        positions: result.calculated_positions.clone(),
        sizes: result
            .layout_tree
            .nodes
            .iter()
            .map(|node| node.used_size)
            .collect(),
    }
}

/// Per layout box and paint call (its background, its content): the items of `list` that
/// call emitted, each as its debug text (every float in full: two floats an ulp apart
/// are two texts). `shifted` moves the items of a box first.
fn runs_of(
    list: &DisplayList,
    shifted: impl Fn(usize) -> LogicalPosition,
) -> BTreeMap<(usize, u8), Vec<String>> {
    let mut runs: BTreeMap<(usize, u8), Vec<String>> = BTreeMap::new();
    assert_eq!(
        list.layout_node_mapping.len(),
        list.items.len(),
        "harness: every item names the paint call it came from"
    );
    for (item, tag) in list.items.iter().zip(list.layout_node_mapping.iter()) {
        let Some((node, phase)) = *tag else {
            continue;
        };
        if !matches!(phase, EmitPhase::BgBorder | EmitPhase::Content) {
            continue;
        }
        let item = translate_item(item.clone(), shifted(node));
        runs.entry((node, phase as u8))
            .or_default()
            .push(format!("{item:?}"));
    }
    runs
}

/// Diff-first, the copied runs: the list of a patched pass is the list a build makes, bit
/// for bit, also for the boxes the pass moved. A resize moves two columns of forty rows
/// (by about -3900 / -2600 px and by about -2406 px) and leaves eight rows where they
/// were; no box of them changes size, so none is re-emitted for its size. The pass used
/// to copy the moved rows' rects with `x + (new - old)`, which is not `new` in the last
/// bits (the module's constants say why this page cannot hide it); the moved rows are
/// painted anew now, and only the rows that stayed are copied.
/// Mutation: the delta test removed from `try_copy_cached_run` (every run copied and
/// shifted): `verify_patched_display_list` names the first moved row, and no run is
/// counted as painted anew.
#[test]
fn a_box_a_patched_pass_moved_is_painted_as_a_build_paints_it() {
    let mut lw = laid_out(page(STILL, MOVING), BEFORE, MovedRunPolicy::TweenFramesTranslate);
    let before = lw.display_list_patch_counters();
    resize(&mut lw, AFTER);
    let counted = lw.display_list_patch_counters();
    let (anew, copied) = (
        counted.moved_runs_painted_anew - before.moved_runs_painted_anew,
        counted.cached_runs_copied - before.cached_runs_copied,
    );
    assert!(
        lw.layout_cache.last_build_was_patched,
        "premise: a resize of a kept tree patches the list"
    );
    assert!(
        anew >= MOVING,
        "premise: the resize moved a column of {MOVING} rows, each painted anew ({anew})"
    );
    assert!(
        copied >= STILL,
        "the {STILL} rows that stayed are copied ({copied})"
    );
    let differences = lw.verify_patched_display_list(DomId::ROOT_ID);
    assert!(
        differences.is_empty(),
        "the patched list is not the list a build makes of the same layout:\n{}",
        differences.join("\n")
    );
}

/// The switch `AZ_DL_TRANSLATE_MOVED_RUNS=1` (here: its per-thread twin,
/// `MovedRunPolicy::AlwaysTranslate`) brings the translation of every pass back: on the page
/// and the resize of the test above (a resize is no tween frame) no run is painted
/// anew, and every run of a box that kept its size is the run of the list before with
/// each item shifted by the box's move (`translate_item`), which for a box that stayed
/// is the run as it was. That list is NOT the list a build makes: the check of the test
/// above finds differences here, which is what says that the page tells the two
/// behaviours apart.
/// Mutation: the switch not asked in `try_copy_cached_run` (moved boxes painted anew
/// whatever it says): runs are counted as painted anew, and a moved row's rect is not
/// the shifted one.
#[test]
fn the_switch_translates_a_moved_run_as_before() {
    let mut lw = laid_out(page(STILL, MOVING), BEFORE, MovedRunPolicy::AlwaysTranslate);
    let before = pass_of(&lw);
    let counted_before = lw.display_list_patch_counters();
    resize(&mut lw, AFTER);
    let counted = lw.display_list_patch_counters();
    let (anew, copied, translated) = (
        counted.moved_runs_painted_anew - counted_before.moved_runs_painted_anew,
        counted.cached_runs_copied - counted_before.cached_runs_copied,
        counted.moved_runs_translated - counted_before.moved_runs_translated,
    );
    let after = pass_of(&lw);
    assert!(
        lw.layout_cache.last_build_was_patched,
        "premise: a resize of a kept tree patches the list"
    );
    assert_eq!(anew, 0, "with the switch no moved run is painted anew");
    assert!(
        translated >= MOVING,
        "the moved rows' runs are counted as translated ({translated})"
    );
    assert!(
        after.list.translated_since_painted.is_empty(),
        "the old behaviour records nothing: no pass paints for it later"
    );
    assert!(
        copied >= STILL + MOVING,
        "the rows that stayed and the rows that moved are copied ({copied})"
    );

    // The move of each box, as the patch computes it.
    let moves: Vec<LogicalPosition> = after
        .positions
        .iter()
        .zip(before.positions.iter())
        .map(|(to, from)| LogicalPosition {
            x: to.x - from.x,
            y: to.y - from.y,
        })
        .collect();
    assert_eq!(
        before.positions.len(),
        after.positions.len(),
        "harness: the resize kept the layout tree"
    );
    let shifted = runs_of(&before.list, |node| moves[node]);
    let now = runs_of(&after.list, |_| LogicalPosition::zero());
    let mut moved_runs = 0;
    for ((node, phase), items) in &now {
        if before.sizes.get(*node) != after.sizes.get(*node) {
            // Another size: re-emitted, as it always was.
            continue;
        }
        assert_eq!(
            Some(items),
            shifted.get(&(*node, *phase)),
            "box {node}: its run is the run before, shifted by its move {:?}",
            moves[*node]
        );
        if moves[*node].x != 0.0 || moves[*node].y != 0.0 {
            moved_runs += 1;
        }
    }
    assert!(
        moved_runs >= MOVING,
        "premise: the resize moved a column of {MOVING} rows ({moved_runs} moved runs)"
    );

    let differences = lw.verify_patched_display_list(DomId::ROOT_ID);
    assert!(
        !differences.is_empty(),
        "a translated run is the run a build makes on this page: the page does not tell the \
         two behaviours apart, and the test of the default proves nothing"
    );
}

/// A box that did not move is copied as before: a resize of a page whose rows all stay
/// where they are paints none of them anew and copies each.
/// Mutation: `try_copy_cached_run` answering `false` for every box, moved or not (the
/// patch painting everything): no run is counted as copied.
#[test]
fn a_box_that_stayed_is_still_copied() {
    let mut lw = laid_out(page(MOVING, 0), BEFORE, MovedRunPolicy::TweenFramesTranslate);
    let before = lw.display_list_patch_counters();
    resize(&mut lw, AFTER);
    let counted = lw.display_list_patch_counters();
    let (anew, copied) = (
        counted.moved_runs_painted_anew - before.moved_runs_painted_anew,
        counted.cached_runs_copied - before.cached_runs_copied,
    );
    assert!(
        lw.layout_cache.last_build_was_patched,
        "premise: a resize of a kept tree patches the list"
    );
    assert_eq!(anew, 0, "no box moved: none is painted anew for a move");
    assert!(
        copied >= MOVING,
        "each of the {MOVING} rows is copied ({copied})"
    );
    let differences = lw.verify_patched_display_list(DomId::ROOT_ID);
    assert!(
        differences.is_empty(),
        "the patched list is not the list a build makes of the same layout:\n{}",
        differences.join("\n")
    );
}

// ---- the frames of a layout tween translate, its last frame paints -------------------------

/// The rows a tween pushes.
const ROWS: usize = 40;
/// The band over them: its height before the tween and the height written (the tween's end).
const BAND_BEFORE: isize = 3000;
const BAND_AFTER: isize = 40;
/// The node of the band: body 0, the band 1, the rows 2 ..= 41.
const BAND: usize = 1;
/// The step of an `f32` just above 4096: an upper bound for the step of every coordinate of
/// [`tween_page`] (its boxes lie below 3000 + 40 x 17.9 = 3716), twice the step at its
/// largest ones. One translation rounds twice (the move, then the sum), each by at most half
/// a step of its largest operand: a frame of the tween cannot take an item further from where
/// a paint puts it than this.
const STEP: f32 = 1.0 / 2048.0;

/// A band whose height tweens (`animation: height`), over forty rows in the flow. A row
/// paints one rect, its background in its CONTENT box: at `position + padding`, a sum the
/// painter makes, so the rect a paint emits is `fl(new + 2.3)` and a translated copy holds
/// `fl(fl(old + 2.3) + move)`. The band shrinks from 3000 px to 40 px, so the rows climb
/// from coordinates in the thousands through 2048, 1024, 512 and 256: below each of these a
/// float has twice as many steps, a translated rect lies on the coarser steps it came from,
/// and a painted one does not have to.
fn tween_page() -> Dom {
    let mut body = Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div().with_css(&format!(
            "height: {BAND_BEFORE}px; animation: height 200ms linear;"
        )),
    );
    for i in 0..ROWS {
        let tenths = i * 17;
        body = body.with_child(Dom::create_div().with_css(&format!(
            "width: 90.3px; height: 13.3px; padding: 2.3px; margin-left: {}.{}px; \
             background: #336699; background-clip: content-box;",
            tenths / 10,
            tenths % 10
        )));
    }
    body
}

/// The window of the tween: taller than the page at its tallest (3716 px), so that no
/// scrollbar comes or goes while the band shrinks.
const TWEEN_WINDOW: (f32, f32) = (600.0, 4000.0);

/// [`tween_page`] laid out, the write of the band's new height made: its tween is seeded.
fn tweening(policy: MovedRunPolicy) -> LayoutWindow {
    let mut lw = laid_out(tween_page(), TWEEN_WINDOW, policy);
    let _ = lw.apply_content_change(ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(BAND),
        props: vec![CssProperty::const_height(LayoutHeight::const_px(BAND_AFTER))],
        override_only: false,
    });
    assert!(
        lw.css_transitions.iter().any(|t| t.node == NodeId::new(BAND)),
        "harness: the write of the band's height seeds its tween"
    );
    lw
}

/// The window's DOM laid out again as it is (what a shell does for the relayout a tick owes).
fn relayout(lw: &mut LayoutWindow) {
    let kept = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        kept.styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out again");
}

/// One frame of the tween: the tick of 16 ms and the relayout it owes. Answers whether the
/// tween goes on after it (`false`: this was its last frame; the tick drops a transition in
/// the tick that ends it).
fn tween_frame(lw: &mut LayoutWindow) -> bool {
    let _ = lw.tick_animations(0.016);
    assert!(
        lw.take_transition_relayout(),
        "harness: a step of the band's height owes a relayout"
    );
    let goes_on = !lw.css_transitions.is_empty();
    relayout(lw);
    goes_on
}

/// The list the window holds for its DOM.
fn held_list(lw: &LayoutWindow) -> Arc<DisplayList> {
    lw.layout_results
        .get(&DomId::ROOT_ID)
        .expect("the page is laid out")
        .display_list
        .clone()
}

/// The list a build PAINTS of the layout the window holds, every item from its box's
/// position, with the window left as it was: the list it holds, the solver's kept list and
/// the mark of the last build are put back, so the next frame patches the list this frame
/// left (a check that swapped the painted list in would end the drift it measures).
fn painted_list(lw: &mut LayoutWindow) -> Arc<DisplayList> {
    let held = held_list(lw);
    let kept = lw.layout_cache.cached_display_list.clone();
    let patched = lw.layout_cache.last_build_was_patched;
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    let painted = held_list(lw);
    lw.layout_results
        .get_mut(&DomId::ROOT_ID)
        .expect("the page is laid out")
        .display_list = held;
    lw.layout_cache.cached_display_list = kept;
    lw.layout_cache.last_build_was_patched = patched;
    painted
}

/// How far the items of `list` lie from the items of `painted`: the largest distance of a
/// coordinate or a size in px, and how many items are not the painted item bit for bit
/// (their debug texts differ). The two lists hold the same items in the same order.
fn distance(list: &DisplayList, painted: &DisplayList) -> (f32, usize) {
    assert_eq!(
        list.items.len(),
        painted.items.len(),
        "the list holds the items a build paints"
    );
    let (mut furthest, mut different) = (0.0_f32, 0);
    for (item, fresh) in list.items.iter().zip(painted.items.iter()) {
        if format!("{item:?}") != format!("{fresh:?}") {
            different += 1;
        }
        match (item.visual_bounds(), fresh.visual_bounds()) {
            (Some(a), Some(b)) => {
                for d in [
                    a.origin.x - b.origin.x,
                    a.origin.y - b.origin.y,
                    a.size.width - b.size.width,
                    a.size.height - b.size.height,
                ] {
                    furthest = furthest.max(d.abs());
                }
            }
            (None, None) => {}
            _ => panic!("an item with bounds stands where the painted list has one without"),
        }
    }
    (furthest, different)
}

/// The counters of the patched builds of `lw`: moved runs painted anew, moved runs
/// translated, translated runs painted anew although their box stood.
fn counts(lw: &LayoutWindow) -> (usize, usize, usize) {
    let counted = lw.display_list_patch_counters();
    (
        counted.moved_runs_painted_anew,
        counted.moved_runs_translated,
        counted.translated_runs_painted_anew,
    )
}

/// The frames of a tween but the last translate what the tween pushes, and paint none of it.
///
/// Every frame of the band's tween moves the forty rows. Such a frame copies their rects
/// shifted by the move (no row is painted anew for having moved: the cost a layout tween
/// must not have, the guard of the ribbon in `layout/tests`), records the rows with the
/// list, and leaves each rect within one [`STEP`] per frame so far of where a paint puts
/// it: the drift is bounded by the tween. And the drift is real on this page, which is what
/// makes the test of the last frame below say something: by the middle of the tween some
/// rect is not the painted one.
/// Mutations: a tween frame painting the moved rows (`translate_moved` never set: the
/// window not saying that a frame follows): rows are counted as painted anew and none as
/// translated; the move added twice in `translate_item`: the distance is the move, not a
/// rounding.
#[test]
fn a_frame_of_a_tween_translates_what_the_tween_pushes_within_a_rounding_per_frame() {
    let mut lw = tweening(MovedRunPolicy::TweenFramesTranslate);
    let (mut frames, mut drifted_frames) = (0_usize, 0_usize);
    loop {
        let before = counts(&lw);
        if !tween_frame(&mut lw) {
            break;
        }
        frames += 1;
        let after = counts(&lw);
        assert!(
            lw.layout_cache.last_reconcile_was_skipped && lw.layout_cache.last_build_was_patched,
            "frame {frames}: harness: a tween frame keeps the layout tree and patches the list"
        );
        assert_eq!(
            (after.0 - before.0, after.2 - before.2),
            (0, 0),
            "frame {frames}: a tween frame paints no box anew for its move"
        );
        assert!(
            after.1 - before.1 >= ROWS,
            "frame {frames}: the {ROWS} rows the band pushes are translated ({})",
            after.1 - before.1
        );
        let list = held_list(&lw);
        assert!(
            list.translated_since_painted.len() >= ROWS,
            "frame {frames}: the translated rows are recorded with the list ({})",
            list.translated_since_painted.len()
        );
        let painted = painted_list(&mut lw);
        let (furthest, different) = distance(&list, &painted);
        assert!(
            furthest <= frames as f32 * STEP,
            "frame {frames}: an item lies {furthest} px from where a paint puts it: more than \
             a rounding per frame"
        );
        drifted_frames += usize::from(different > 0);
    }
    assert!(frames >= 6, "harness: the tween of 200 ms has frames between its ends ({frames})");
    assert!(
        drifted_frames >= 1,
        "premise: on this page a translated rect is not the painted one in some frame; with \
         none the test of the last frame proves nothing"
    );
}

/// The frame that ends a tween paints what its frames translated: after it the list is bit
/// for bit the list a build paints, and it records no box. So is the list the first pass
/// that is no tween frame leaves when the tween is still running: a resize of the window in
/// the middle of it paints the recorded rows anew although they stand where they stood.
/// Mutations: the last frame treated as every frame of the tween (the stamp's
/// `more_frames_follow` always set): the list after it records the forty rows and its rects
/// are not the painted ones; recorded boxes not painted by a pass that is no tween frame
/// (the `recorded` test removed): the list after the resize differs from a painted one.
#[test]
fn the_frame_that_ends_a_tween_and_a_pass_that_is_none_leave_a_painted_list() {
    // (1) The tween run to its end.
    let mut lw = tweening(MovedRunPolicy::TweenFramesTranslate);
    let mut frames = 0;
    while tween_frame(&mut lw) {
        frames += 1;
        assert!(frames < 64, "harness: the tween ends");
    }
    assert!(lw.css_transitions.is_empty(), "harness: the tween is over");
    let list = held_list(&lw);
    assert!(
        list.translated_since_painted.is_empty(),
        "the last frame painted what the tween translated: {} boxes are still recorded",
        list.translated_since_painted.len()
    );
    let painted = painted_list(&mut lw);
    let (_, different) = distance(&list, &painted);
    assert_eq!(different, 0, "after the tween the list is the list a build paints");

    // (2) A pass that is no tween frame, in the middle of the tween: a resize.
    let mut lw = tweening(MovedRunPolicy::TweenFramesTranslate);
    for frame in 0..5 {
        assert!(tween_frame(&mut lw), "harness: frame {frame} is not the tween's last");
    }
    assert!(
        held_list(&lw).translated_since_painted.len() >= ROWS,
        "harness: five frames in, the rows are recorded"
    );
    let before = counts(&lw);
    resize(&mut lw, (TWEEN_WINDOW.0 + 40.0, TWEEN_WINDOW.1));
    let after = counts(&lw);
    assert!(
        lw.layout_cache.last_build_was_patched,
        "premise: a resize of a kept tree patches the list"
    );
    assert!(
        after.2 - before.2 >= ROWS,
        "the resize painted the {ROWS} recorded rows anew, though none of them moved ({})",
        after.2 - before.2
    );
    let list = held_list(&lw);
    assert!(
        list.translated_since_painted.is_empty(),
        "a pass that is no tween frame records nothing"
    );
    let painted = painted_list(&mut lw);
    let (_, different) = distance(&list, &painted);
    assert_eq!(different, 0, "after the resize the list is the list a build paints");
}

/// A tween that is cut off leaves a painted list too: the app builds its DOM anew in the
/// middle of the tween (a new generation: the override the tween wrote is not the new
/// DOM's), and the pass of that rebuild is no tween frame.
/// Mutation: a rebuild's pass translating like a tween frame, or serving the kept list of
/// the frame before as it lies (the cache hit not asking `translated_since_painted`): the
/// list after the rebuild records rows, or differs from a painted one.
#[test]
fn a_tween_cut_off_by_a_rebuild_leaves_a_painted_list() {
    let mut lw = tweening(MovedRunPolicy::TweenFramesTranslate);
    for frame in 0..5 {
        assert!(tween_frame(&mut lw), "harness: frame {frame} is not the tween's last");
    }
    assert!(
        held_list(&lw).translated_since_painted.len() >= ROWS,
        "harness: five frames in, the rows are recorded"
    );
    let ws = lw.current_window_state.clone();
    lw.layout_new_generation(
        StyledDom::create_from_dom(tween_page()),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the rebuilt page lays out");
    let list = held_list(&lw);
    assert!(
        list.translated_since_painted.is_empty(),
        "the pass of a rebuild records nothing ({} boxes)",
        list.translated_since_painted.len()
    );
    let painted = painted_list(&mut lw);
    let (_, different) = distance(&list, &painted);
    assert_eq!(different, 0, "after the rebuild the list is the list a build paints");
}

/// The other extreme of the switch (`AZ_DL_TRANSLATE_MOVED_RUNS=0`,
/// `MovedRunPolicy::NeverTranslate`): a tween frame paints the rows it moves like any other
/// pass, translates and records nothing, and its list is the painted one.
/// Mutation: the policy not asked on a tween frame: rows are counted as translated.
#[test]
fn the_other_extreme_paints_what_a_tween_pushes_on_every_frame() {
    let mut lw = tweening(MovedRunPolicy::NeverTranslate);
    let before = counts(&lw);
    assert!(tween_frame(&mut lw), "harness: the first frame is not the tween's last");
    let after = counts(&lw);
    assert!(
        lw.layout_cache.last_build_was_patched,
        "harness: a tween frame patches the list"
    );
    assert_eq!(after.1 - before.1, 0, "no run is translated");
    assert!(
        after.0 - before.0 >= ROWS,
        "the {ROWS} rows the band pushes are painted anew ({})",
        after.0 - before.0
    );
    let list = held_list(&lw);
    assert!(list.translated_since_painted.is_empty(), "nothing is recorded");
    let painted = painted_list(&mut lw);
    let (_, different) = distance(&list, &painted);
    assert_eq!(different, 0, "the list is the list a build paints");
}
