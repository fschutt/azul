//! Where the conversations of a page stand in its margin.
//!
//! A page has a margin next to its code (`ui::MARGIN_GUTTER_W` wide): each conversation of the page stands in it as a
//! small chip at the height of the line it was anchored to. Two anchors on nearby lines would put two chips on each
//! other, so a chip is pushed down below the one before it, and a stack that would run off the foot of the page is pushed
//! up together; a margin that holds fewer chips than there are conversations leaves out the least important ones (a
//! conversation at work or waiting outranks a finished one; of two alike the later goes) and names them, so the window
//! can say "and 4 more" and the bottom bar still reaches them.
//!
//! Pure numbers: no window, no DOM.

/// One conversation that wants a place in the margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slot {
    pub id: u64,
    /// The height of its anchor on the page, from the top of the margin, px.
    pub want_y: f32,
    /// How much it matters to stay in sight (a greater number stays): 2 working, 1 waiting, 0 anything else.
    pub keep: u8,
}

/// Where the chips stand.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Placement {
    /// `(id, top)` of the chips that stand in the margin, from the top down.
    pub tops: Vec<(u64, f32)>,
    /// The conversations that do not fit, in the order of the page.
    pub hidden: Vec<u64>,
}

/// How many chips of `chip_h` with `gap` between them stand in a margin `height` tall.
#[must_use]
pub fn capacity(height: f32, chip_h: f32, gap: f32) -> usize {
    if !(chip_h > 0.0) || !(height >= chip_h) {
        return 0;
    }
    ((height + gap) / (chip_h + gap)).floor().max(0.0) as usize
}

/// The anchor as a number the layout can use (a NaN is the top).
fn wanted(slot: &Slot) -> f32 {
    if slot.want_y.is_nan() {
        0.0
    } else {
        slot.want_y
    }
}

/// Places `slots` in a margin `height` tall for chips of `chip_h` with `gap` between them.
#[must_use]
pub fn place(slots: &[Slot], height: f32, chip_h: f32, gap: f32) -> Placement {
    // the order of the page: the height of the anchor, then the age (the lower id)
    let mut order: Vec<Slot> = slots.to_vec();
    order.sort_by(|a, b| wanted(a).total_cmp(&wanted(b)).then(a.id.cmp(&b.id)));

    // who stays: the most important first, the earlier of two alike
    let room = capacity(height, chip_h, gap);
    let mut hidden: Vec<u64> = Vec::new();
    if order.len() > room {
        let mut ranked: Vec<(usize, Slot)> = order.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.keep.cmp(&a.1.keep).then(a.0.cmp(&b.0)));
        let mut leaves: Vec<usize> = ranked.iter().skip(room).map(|(i, _)| *i).collect();
        leaves.sort_unstable();
        hidden = leaves.iter().map(|i| order[*i].id).collect();
        let left: std::collections::BTreeSet<usize> = leaves.into_iter().collect();
        order = order.into_iter().enumerate().filter(|(i, _)| !left.contains(i)).map(|(_, s)| s).collect();
    }
    if order.is_empty() {
        return Placement { tops: Vec::new(), hidden };
    }

    // down: each at its anchor, below the one before
    let step = chip_h + gap;
    let last_top = height - chip_h;
    let mut tops: Vec<f32> = Vec::with_capacity(order.len());
    for slot in &order {
        let mut top = wanted(slot).clamp(0.0, last_top.max(0.0));
        if let Some(previous) = tops.last() {
            top = top.max(previous + step);
        }
        tops.push(top);
    }
    // up: what ran off the foot is pushed back, the stack together
    let mut limit = last_top;
    for top in tops.iter_mut().rev() {
        if *top > limit {
            *top = limit;
        }
        limit = *top - step;
    }
    Placement { tops: order.iter().map(|s| s.id).zip(tops).collect(), hidden }
}

/// Where a popover `popover_h` tall that opens at the chip standing at `chip_top` stands on a page `page_h` tall: at the
/// chip, pushed up so that its foot is on the page, never above the top.
#[must_use]
pub fn popover_top(chip_top: f32, popover_h: f32, page_h: f32) -> f32 {
    chip_top.min(page_h - popover_h).max(0.0)
}

/// Where the open conversation, `popover_w` wide, stands across a page `page_w` wide of which `visible_right` px (from the
/// page's left edge) are in view: at the page's right edge where the whole page is, and further left where the window shows
/// less, so that it is always all in view; never past the page's left edge.
#[must_use]
pub fn popover_left(visible_right: f32, popover_w: f32, page_w: f32) -> f32 {
    (page_w - popover_w - 6.0).min(visible_right - popover_w - 6.0).max(8.0)
}

#[cfg(test)]
#[path = "margin_test.rs"]
mod margin_test;
