//! The rows of a scroll box that are in view (feature `azul`): what an app's own list - not a
//! `SummaryList`, which reports its first row itself - tells [`crate::follow::ListScroll`] when
//! the reader's scroll settles.
//!
//! The app puts the rows (and the headers between them) as the DIRECT children of the box that
//! scrolls, in the order of its model's rows, and hangs a `ScrollEnd` callback on the box:
//!
//! ```ignore
//! list.with_callback(EventFilter::Hover(HoverEventFilter::ScrollEnd), app.clone(), on_list_scrolled)
//! // ... in on_list_scrolled:
//! let Some((first, end)) = azul_appkit::list::rows_in_view(&info) else { return Update::DoNothing };
//! ```
//!
//! The rows keep their own height (a row grows with the user's text size: nothing here assumes a
//! height), so the answer is MEASURED: the layout position of every child against the box's own
//! window, `[box top + scroll offset, + the box's height]`.

use azul::prelude::*;

/// How much of a row must lie under the top edge to count as the one at it: a row cut by less is
/// the one above's last pixels.
const EDGE_SLACK: f32 = 0.5;

/// The children of the hit node (a scroll box) that show in it now: the index of the first and one
/// past the last; `None` where the box, its children or their layout are not known (a box that
/// has not been laid out yet, a test harness).
#[must_use]
pub fn rows_in_view(info: &CallbackInfo) -> Option<(usize, usize)> {
    let container = info.get_hit_node();
    let rect = info.get_node_rect(container).into_option()?;
    let offset = info
        .get_scroll_offset()
        .into_option()
        .map_or(0.0, |p| if p.y.is_finite() && p.y > 0.0 { p.y } else { 0.0 });
    let top = rect.origin.y + offset;
    let bottom = top + rect.size.height;
    let mut first = None;
    let mut end = 0usize;
    let mut index = 0usize;
    let mut next = info.get_first_child(container).into_option();
    while let Some(node) = next {
        if let Some(r) = info.get_node_rect(node).into_option() {
            let (y0, y1) = (r.origin.y, r.origin.y + r.size.height);
            if y1 > top + EDGE_SLACK && y0 < bottom {
                first.get_or_insert(index);
                end = index + 1;
            }
        }
        index += 1;
        next = info.get_next_sibling(node).into_option();
    }
    first.map(|f| (f, end))
}
