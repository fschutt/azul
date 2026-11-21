//! One node's background, border and shadows painted again INTO a display list that is kept.
//!
//! A `:hover { background }` on one label of a map changes what ONE paint call of the
//! build emits: the node's background-and-border run (`EmitPhase::BgBorder`). The list was
//! built again whole for it - 12,470 nodes walked, 17,495 items emitted - on every enter
//! and every leave. Here the run alone is painted, by the build's own function
//! (`DisplayListGenerator::paint_node_background_and_border_inner`), and put where the
//! build puts it:
//!
//! - in place of the node's run, when the list holds one (a leave: the hover colour goes;
//!   an enter over a label that has a background of its own);
//! - right before the first item of the node's CONTENT run, when the node painted no
//!   background or border before (the item a colour that was transparent needs: the
//!   insert). The build paints a box's background-and-border run, then - for that box -
//!   its scroll hit area, its clip and scroll frames, the stacking contexts below it, its
//!   selection and its placeholder prompt, then its content run. [`plan`] asks the build's
//!   own functions whether any of those emits something for the box; when none does, the
//!   two runs are neighbours and the place is known.
//!
//! Everything else is a reason to build the list again, which the caller does when
//! [`plan`] or [`locate`] answer `None`: a box painted in several pieces or by another box
//! (an inline box on a line, a table part, the canvas' root, a list marker, an SVG shape's
//! stroke), a box in a named flow, a box inside an opacity group (the group's bounds are
//! what it paints), a run that did not change (the decorations are painted somewhere this
//! patch does not look).
//!
//! What a node's background changes beyond its own run is the PROOF of the text below it
//! ([`DisplayList::uniform_text_bgs`]): a run that had one gets the proof a build gives it
//! now, as a copied run does (`try_copy_cached_run`); a run without one keeps none, which
//! every renderer paints correctly (the proof is only a faster path).

use super::*;

/// How many boxes below a restyled node are looked at for the text runs whose proven
/// background the node's own background can change. A node with more is built again
/// with its list.
const MAX_SUBTREE: usize = 256;

/// One node's background-and-border run as a build paints it now, and what [`locate`]
/// needs to put it into a list.
#[derive(Debug)]
pub(crate) struct DecorationPatch {
    /// The node's layout box.
    layout_index: usize,
    /// The run: its items and their side tables, a list of its own.
    run: DisplayList,
    /// Whether the build emits nothing between this run and the node's content run, so
    /// that a run the list does not hold yet goes right before the content run.
    content_follows: bool,
    /// The layout boxes of the node's subtree, the node's own with them, sorted.
    subtree: Vec<usize>,
    /// The background a build proves now for a text run of each box of `subtree`
    /// (`compute_uniform_text_bg`), in the order of `subtree`.
    proofs: Vec<(usize, Option<(ColorU, WindowLogicalRect)>)>,
}

/// Does the box of `dom_id` open an opacity group around what it paints (the condition of
/// `generate_for_stacking_context`)? Its `PushOpacity` carries the bounds of that paint.
fn opens_opacity_group(
    styled_dom: &StyledDom,
    gpu_value_cache: Option<&GpuValueCache>,
    dom_id: NodeId,
) -> bool {
    let node_data = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    let (Some(node_data), Some(styled)) = (node_data.get(dom_id), styled_nodes.get(dom_id)) else {
        return true;
    };
    let node_state = &styled.styled_node_state;
    let opacity = crate::solver3::getters::get_opacity(styled_dom, dom_id, node_state);
    let animated = gpu_value_cache.is_some_and(|cache| {
        cache.anim_opacity_keys.contains_key(&dom_id)
            && cache.anim_current_opacity_values.contains_key(&dom_id)
    });
    let mix_blend_mode = styled_dom
        .css_property_cache
        .ptr
        .get_mix_blend_mode(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default();
    opacity < 1.0 || animated || mix_blend_mode != StyleMixBlendMode::Normal
}

/// The patch of one node, `None` when its decorations cannot be painted alone.
fn plan_one<T: ParsedFontTrait + Sync + 'static>(
    generator: &mut DisplayListGenerator<'_, '_, T>,
    node: NodeId,
) -> Option<DecorationPatch> {
    let tree = generator.positioned_tree.tree;
    let styled_dom = generator.ctx.styled_dom;
    let gpu_value_cache = generator.gpu_value_cache;

    // One box, painted once: a node of several boxes (a list item and its marker) is not.
    let boxes = tree.dom_to_layout.get(&node)?;
    if boxes.len() != 1 {
        return None;
    }
    let index = boxes[0].index();
    let hot = tree.get(LayoutNodeId::new(index))?;
    if hot.dom_node_id != Some(node) || crate::solver3::fc::is_marker_box(tree, index) {
        return None;
    }
    // The canvas paints the root's background (and a body's it took over), a table paints
    // its parts' backgrounds in its own run.
    if generator.canvas_painted.contains(&Some(node))
        || matches!(
            hot.formatting_context,
            FormattingContext::Table
                | FormattingContext::TableRowGroup
                | FormattingContext::TableRow
                | FormattingContext::TableCell
                | FormattingContext::TableColumnGroup
                | FormattingContext::TableCaption
        )
    {
        return None;
    }
    // The box and every box around it: none in a named flow (a region paints its parts),
    // none an opacity group (its bounds are what it paints).
    let mut at = Some(index);
    let mut steps = 0_usize;
    while let Some(i) = at {
        steps += 1;
        if steps > tree.nodes.len() {
            return None;
        }
        let id = LayoutNodeId::new(i);
        if tree.warm(id).is_some_and(|w| w.flow.is_some()) {
            return None;
        }
        let around = tree.get(id)?;
        if let Some(dom_id) = around.dom_node_id {
            if opens_opacity_group(styled_dom, gpu_value_cache, dom_id) {
                return None;
            }
        }
        at = around.parent;
    }
    // An SVG shape takes its border as a stroke, painted outside this run.
    let state = generator.get_styled_node_state(node);
    let border_info = get_border_info(styled_dom, node, &state);
    if generator.svg_stroke_for(node, &border_info).is_some() {
        return None;
    }

    // The run, as the build paints it.
    let mut builder = DisplayListBuilder::with_debug(false);
    if let Some(cache) = gpu_value_cache {
        builder.bound_colors = cache.color_keys.iter().map(|(n, k)| (*n, *k)).collect();
    }
    builder.set_current_layout(Some((index, EmitPhase::BgBorder)));
    generator
        .paint_node_background_and_border_inner(&mut builder, index)
        .ok()?;
    builder.set_current_layout(None);
    if !builder.forced_page_breaks.is_empty() || !builder.open_opacity_groups.is_empty() {
        return None;
    }
    let run = builder.build();
    if !run.carets.is_empty() || !run.fixed_position_item_ranges.is_empty() {
        return None;
    }

    // What the build emits for this box between the run and its content run: its scroll
    // hit area, its clip and scroll frames, the stacking contexts below it, its selection,
    // its placeholder prompt.
    let scrolls = get_overflow_x(styled_dom, node, &state).is_scroll()
        || get_overflow_y(styled_dom, node, &state).is_scroll();
    let mut between = DisplayListBuilder::with_debug(false);
    let clips = generator.push_node_clips(&mut between, index, hot);
    generator.paint_selections(&mut between, index).ok()?;
    if let Some(rect) = generator.get_paint_rect(index) {
        generator.maybe_paint_placeholder_prompt(&mut between, index, &rect);
    }
    let contexts_below = generator.establishes_stacking_context(index)
        && generator
            .collect_stacking_contexts(index)
            .ok()?
            .child_contexts
            .iter()
            .any(|context| context.z_index < 0);
    let content_follows = !scrolls && !clips && between.items.is_empty() && !contexts_below;

    // The boxes whose text sits on this node's background.
    let mut subtree = vec![index];
    let mut next = 0_usize;
    while next < subtree.len() {
        if subtree.len() > MAX_SUBTREE {
            return None;
        }
        let children = tree.children(subtree[next]);
        subtree.extend_from_slice(children);
        next += 1;
    }
    subtree.sort_unstable();
    subtree.dedup();
    let proofs = subtree
        .iter()
        .map(|i| (*i, generator.compute_uniform_text_bg(*i)))
        .collect();

    Some(DecorationPatch {
        layout_index: index,
        run,
        content_follows,
        subtree,
        proofs,
    })
}

/// The background-and-border runs of `nodes` as a build of the list paints them now, from
/// the inputs `generate_display_list` takes. `None` when one of them cannot be painted
/// alone: the caller builds the list.
#[allow(clippy::too_many_arguments)] // the inputs of `generate_display_list`
pub(crate) fn plan<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    calculated_positions: &crate::solver3::PositionVec,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &HashMap<LayoutNodeId, u64>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
    nodes: &[NodeId],
) -> Option<Vec<DecorationPatch>> {
    let positioned_tree = PositionedTree {
        tree,
        calculated_positions,
    };
    let mut generator = DisplayListGenerator::new(
        ctx,
        scroll_offsets,
        &positioned_tree,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    );
    // The nodes whose background the canvas painted, as step 0 of the build finds them.
    let root_dom_id = tree
        .get(LayoutNodeId::new(tree.root))
        .and_then(|root| root.dom_node_id)?;
    let root_state = generator.get_styled_node_state(root_dom_id);
    let body = crate::solver3::getters::body_background_propagated_to(
        generator.ctx.styled_dom,
        root_dom_id,
        &root_state,
    );
    generator.canvas_painted = [Some(root_dom_id), body];

    let mut patches = Vec::with_capacity(nodes.len());
    for node in nodes {
        patches.push(plan_one(&mut generator, *node)?);
    }
    Some(patches)
}

/// Where the runs of `patches` go in `list`: `(index of the first item, items replaced)`
/// per patch. `None`, and nothing is to be changed, when one of them has no place this
/// patch can prove: the caller builds the list.
pub(crate) fn locate(list: &DisplayList, patches: &[DecorationPatch]) -> Option<Vec<(usize, usize)>> {
    let n = list.items.len();
    if list.layout_node_mapping.len() != n
        || list.node_mapping.len() != n
        || list.uniform_text_bgs.len() != n
        || list.text_selection_colors.len() != n
        || list.color_bindings.len() != n
    {
        return None;
    }
    // The runs of each patched box in the list: its background-and-border runs, and
    // where its content runs start.
    let mut background: Vec<Vec<(usize, usize)>> = vec![Vec::new(); patches.len()];
    let mut content: Vec<Vec<usize>> = vec![Vec::new(); patches.len()];
    // Whether the list holds a selection run of the box (`EmitPhase::Selection`, A3-core-17):
    // it lies between the box's background run and its content run.
    let mut selected: Vec<bool> = vec![false; patches.len()];
    let mut i = 0_usize;
    while i < n {
        let tag = list.layout_node_mapping[i];
        let start = i;
        while i < n && list.layout_node_mapping[i] == tag {
            i += 1;
        }
        let Some((index, phase)) = tag else {
            continue;
        };
        let Some(k) = patches.iter().position(|p| p.layout_index == index) else {
            continue;
        };
        match phase {
            EmitPhase::BgBorder => background[k].push((start, i)),
            EmitPhase::Content => content[k].push(start),
            EmitPhase::Selection => selected[k] = true,
            EmitPhase::ScWalk => {}
        }
    }

    let mut placements: Vec<(usize, usize)> = Vec::with_capacity(patches.len());
    for (k, patch) in patches.iter().enumerate() {
        let (at, old_len) = match background[k].as_slice() {
            [(start, end)] => (*start, *end - *start),
            [] => match content[k].as_slice() {
                // A run the list does not hold yet goes right before the content run only
                // when nothing lies between the two: not when the box's highlight does
                // (`plan_one` says so too, from the selection as it is now; this is the
                // list as it was built).
                [start] if patch.content_follows && !selected[k] => (*start, 0),
                _ => return None,
            },
            _ => return None,
        };
        let end = at + old_len;
        // A run that stays as it is: the node's decorations are painted somewhere this
        // patch does not look (or the restyle changed nothing that is painted).
        let old = &list.items[at..end];
        if old.len() == patch.run.items.len()
            && old
                .iter()
                .zip(&patch.run.items)
                .all(|(a, b)| a.is_visually_equal(b))
        {
            return None;
        }
        // A fixed-position range that begins or ends at an insert: on which side of the
        // new items it lies is not known. One that cuts through a run cannot be.
        let ranges = &list.fixed_position_item_ranges;
        if old_len == 0 && ranges.iter().any(|(s, e)| *s == at || *e == at) {
            return None;
        }
        if ranges
            .iter()
            .any(|(s, e)| (*s > at && *s < end) || (*e > at && *e < end))
        {
            return None;
        }
        if list.carets.iter().any(|(index, _)| *index >= at && *index < end) {
            return None;
        }
        if placements.iter().any(|(other, _)| *other == at) {
            return None;
        }
        placements.push((at, old_len));
    }
    Some(placements)
}

/// Puts the runs of `patches` into `list` at the `placements` [`locate`] found in this
/// very list, every side table and index range kept aligned.
pub(crate) fn commit(list: &mut DisplayList, patches: &[DecorationPatch], placements: &[(usize, usize)]) {
    // The proofs of the text runs painted below the nodes, while the indices still are
    // the ones of the list as it was located in.
    for i in 0..list.items.len() {
        let Some(Some((index, EmitPhase::Content))) = list.layout_node_mapping.get(i).copied() else {
            continue;
        };
        if list.uniform_text_bgs.get(i).copied().flatten().is_none() {
            continue;
        }
        let Some(patch) = patches
            .iter()
            .find(|p| p.subtree.binary_search(&index).is_ok())
        else {
            continue;
        };
        let DisplayListItem::Text {
            source_node_index: Some(source),
            ..
        } = &list.items[i]
        else {
            continue;
        };
        if let Ok(found) = patch.proofs.binary_search_by_key(source, |(box_index, _)| *box_index) {
            list.uniform_text_bgs[i] = patch.proofs[found].1;
        }
    }

    // From the back of the list, so the places of the earlier runs stay valid.
    let mut order: Vec<usize> = (0..patches.len().min(placements.len())).collect();
    order.sort_by_key(|k| core::cmp::Reverse(placements[*k].0));
    for k in order {
        let (at, old_len) = placements[k];
        let run = &patches[k].run;
        let new_len = run.items.len();
        let end = at + old_len;
        if end > list.items.len() {
            continue;
        }
        list.items.splice(at..end, run.items.iter().cloned());
        list.node_mapping
            .splice(at..end, run.node_mapping.iter().copied());
        list.layout_node_mapping
            .splice(at..end, run.layout_node_mapping.iter().copied());
        list.uniform_text_bgs
            .splice(at..end, run.uniform_text_bgs.iter().copied());
        list.text_selection_colors
            .splice(at..end, run.text_selection_colors.iter().copied());
        list.color_bindings
            .splice(at..end, run.color_bindings.iter().copied());
        if new_len == old_len {
            continue;
        }
        // What lies behind the run moves by the difference; an index at or beyond `end`
        // is at least `old_len` beyond `at`, so the subtraction cannot wrap.
        let moved = |index: usize| index - old_len + new_len;
        for (index, _) in &mut list.carets {
            if *index >= end {
                *index = moved(*index);
            }
        }
        for (start, finish) in &mut list.fixed_position_item_ranges {
            // `locate` let no range begin or end inside the run, nor at an insert.
            if *start > at {
                *start = moved(*start);
            }
            if *finish > at {
                *finish = moved(*finish);
            }
        }
    }
    list.frame_index.clear();
}
