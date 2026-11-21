//! diff-first: the display list of a patched pass holds no translated item
//! (`audit/diff-first.md`, "The copied runs of a patched pass").

use std::collections::BTreeMap;

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{
        cached_runs_copied_on_this_thread, moved_runs_painted_anew_on_this_thread,
        set_dl_translate_moved_runs_on_this_thread, translate_item, DisplayList, EmitPhase,
    },
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

/// The behaviour of the patched builds of this thread for the life of the value: the
/// translation of before the audit (`true`) or the default (`false`), whatever the
/// environment says; the environment's again when it goes, also by a failed assertion.
struct MovedRuns;

impl MovedRuns {
    fn translated(on: bool) -> Self {
        set_dl_translate_moved_runs_on_this_thread(Some(on));
        Self
    }
}

impl Drop for MovedRuns {
    fn drop(&mut self) {
        set_dl_translate_moved_runs_on_this_thread(None);
    }
}

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
fn laid_out(dom: Dom, size: (f32, f32)) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
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
    let _default = MovedRuns::translated(false);
    let mut lw = laid_out(page(STILL, MOVING), BEFORE);
    let (anew, copied) = (
        moved_runs_painted_anew_on_this_thread(),
        cached_runs_copied_on_this_thread(),
    );
    resize(&mut lw, AFTER);
    let (anew, copied) = (
        moved_runs_painted_anew_on_this_thread() - anew,
        cached_runs_copied_on_this_thread() - copied,
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

/// The switch `AZ_DL_TRANSLATE_MOVED_RUNS=1` (here: its per-thread twin) brings the
/// translation back: on the page and the resize of the test above no run is painted
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
    let _translated = MovedRuns::translated(true);
    let mut lw = laid_out(page(STILL, MOVING), BEFORE);
    let before = pass_of(&lw);
    let (anew, copied) = (
        moved_runs_painted_anew_on_this_thread(),
        cached_runs_copied_on_this_thread(),
    );
    resize(&mut lw, AFTER);
    let (anew, copied) = (
        moved_runs_painted_anew_on_this_thread() - anew,
        cached_runs_copied_on_this_thread() - copied,
    );
    let after = pass_of(&lw);
    assert!(
        lw.layout_cache.last_build_was_patched,
        "premise: a resize of a kept tree patches the list"
    );
    assert_eq!(anew, 0, "with the switch no moved run is painted anew");
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
    let _default = MovedRuns::translated(false);
    let mut lw = laid_out(page(MOVING, 0), BEFORE);
    let (anew, copied) = (
        moved_runs_painted_anew_on_this_thread(),
        cached_runs_copied_on_this_thread(),
    );
    resize(&mut lw, AFTER);
    let (anew, copied) = (
        moved_runs_painted_anew_on_this_thread() - anew,
        cached_runs_copied_on_this_thread() - copied,
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
