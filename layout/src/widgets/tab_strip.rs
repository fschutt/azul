//! The geometry of a tab strip that scrolls sideways without a scrollbar and, in the status bar's
//! band, goes around the cutouts of the display.
//!
//! A strip is one row of pills longer than the room it has. It scrolls: a finger drags it with
//! momentum, a wheel or a trackpad moves it, the selected pill comes into view by itself. In
//! the status bar's band ([`StatusBarBand`]) the strip takes the place of the clock and the
//! battery icons: the room it has is not one stretch but LANES, the band minus every cutout (and
//! a margin around each). The row slides under the camera like a strip of paper behind a hole:
//! what lies under a cutout is covered, never drawn, and a pill half in one lane is the hint
//! that there is more on the other side. The selected pill is never left half under the camera:
//! [`scroll_for_pill`] picks a scroll offset at which it stands whole inside one lane.
//!
//! Everything here is plain numbers in the strip's own coordinates (x from its left edge, in
//! logical px), so the rules are tested without a window. The DOM of the strip and its
//! callbacks live in `tabs.rs`.

use alloc::vec::Vec;

use azul_core::{
    display_features::DisplayFeatures,
    geom::{LogicalPosition, LogicalRect, LogicalRectVec, LogicalSize},
};
use azul_css::system::SafeAreaInsets;

/// The height of a row of touch pills, in logical px: the pill (32) and the room above and below it
/// that makes its target (8 each): at least 44 pt / 48 dp. A strip in the status bar's band is
/// never shorter.
pub const TOUCH_ROW: f32 = 48.0;

/// The room kept clear on each side of a cutout, in logical px: the camera's own glass is wider
/// than the hole, and a thumb that misses a pill by a hair should not land on the camera.
pub const CUTOUT_MARGIN: f32 = 8.0;

/// How far from the edge of a lane the selected pill stands when the strip scrolled it into
/// place, in logical px (less where the lane has no more room).
pub const LANE_INSET: f32 = 12.0;

/// The status bar's band and the lanes a strip in it can use (window coordinates, logical px).
///
/// Made once per layout pass from what the shell reports ([`Self::from_display`]) and handed to
/// the tab bar (`TabHeader::with_status_bar`): a rotation or a resize runs `layout()` again,
/// and the strip is built again around the cutouts of the new window.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct StatusBarBand {
    /// The free stretches of the band, left to right, each as tall as the band. One lane the
    /// width of the window less its side insets where no cutout is in the band; two or more
    /// where cutouts are.
    pub lanes: LogicalRectVec,
    /// The strip's height, from the top edge of the window: the status bar's, and never less than
    /// [`TOUCH_ROW`]. `0` where the window has no status bar and no inset at its top (a desktop
    /// window, an iPhone on its side): the strip is an ordinary one then.
    pub height: f32,
    /// The window's width the lanes were made for.
    pub width: f32,
}

azul_css::impl_option!(
    StatusBarBand,
    OptionStatusBarBand,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl StatusBarBand {
    /// The band of a window of `window` size with `features` and `safe_area` as the shell reports
    /// them (`LayoutCallbackInfo::get_display_features` / `get_safe_area_insets`).
    ///
    /// The band is the status bar's height; a window with a cutout and no status bar rect takes
    /// the top inset; a window with neither has none (`height == 0`). Lanes are the band between
    /// the left and the right inset, less every cutout in it with [`CUTOUT_MARGIN`] on each side.
    #[must_use]
    pub fn from_display(
        features: &DisplayFeatures,
        safe_area: &SafeAreaInsets,
        window: LogicalSize,
    ) -> Self {
        let [inset_top, _, inset_left, inset_right] =
            crate::window::LayoutWindow::safe_area_px(safe_area);
        let width = if window.width.is_finite() && window.width > 0.0 {
            window.width
        } else {
            0.0
        };
        let status = features.bars.status.into_option().map_or(0.0, |bar| {
            bar.rect.origin.y.max(0.0) + bar.rect.size.height.max(0.0)
        });
        let band = if status.is_finite() && status > 0.0 {
            status
        } else {
            inset_top
        };
        // A strip in the band is a touch row at least: a landscape phone's 24 dp status bar is not
        // room for a pill. The cutouts that count are those in the strip's whole height.
        let height = if band > 0.0 { band.max(TOUCH_ROW) } else { 0.0 };
        let left = inset_left.min(width);
        let right = (width - inset_right).max(left);
        let lanes = features.free_lanes(0.0, height, left, right, CUTOUT_MARGIN);
        Self {
            height,
            width,
            lanes,
        }
    }

    /// The lanes as `(left, right)` pairs.
    #[must_use]
    pub fn lane_spans(&self) -> Vec<(f32, f32)> {
        self.lanes
            .as_ref()
            .iter()
            .map(|r| (r.origin.x, r.origin.x + r.size.width))
            .collect()
    }

    /// The stretches of the band no lane covers: the cutouts with their margins, the insets at the
    /// sides. What the strip hides its pills behind.
    #[must_use]
    pub fn gaps(&self) -> Vec<(f32, f32)> {
        gaps_between(&self.lane_spans(), self.width)
    }

    /// Whether the strip is in a band at all (the window has a status bar or a cutout at its top).
    #[must_use]
    pub fn is_in_band(&self) -> bool {
        self.height > 0.0
    }

    /// A band without any cutout in it: one lane, no gap but the insets.
    #[must_use]
    pub fn is_one_lane(&self) -> bool {
        self.lanes.as_ref().len() <= 1
    }
}

/// The stretches of `0..width` that `lanes` (sorted, disjoint) leave uncovered, widest-first
/// order not implied: left to right. No lane at all is one gap, the whole width.
#[must_use]
pub fn gaps_between(lanes: &[(f32, f32)], width: f32) -> Vec<(f32, f32)> {
    let mut gaps = Vec::new();
    let mut cursor = 0.0_f32;
    for &(left, right) in lanes {
        if left > cursor {
            gaps.push((cursor, left));
        }
        if right > cursor {
            cursor = right;
        }
    }
    if width > cursor {
        gaps.push((cursor, width));
    }
    gaps
}

/// A pill `(x, width)` on a strip scrolled by `scroll`, as the `(left, right)` it stands at.
#[must_use]
pub fn pill_span(pill: (f32, f32), scroll: f32) -> (f32, f32) {
    (pill.0 - scroll, pill.0 + pill.1 - scroll)
}

/// The pieces of `span` that stand in a lane: what the strip shows of a pill. A pill whole
/// in a lane has one piece, the pill; one that straddles a gap has two (or more); one that lies
/// under a cutout has none.
#[must_use]
pub fn visible_pieces(span: (f32, f32), lanes: &[(f32, f32)]) -> Vec<(f32, f32)> {
    lanes
        .iter()
        .filter_map(|&(left, right)| {
            let a = span.0.max(left);
            let b = span.1.min(right);
            (b > a).then_some((a, b))
        })
        .collect()
}

/// Whether `span` is whole inside one of the `lanes`.
#[must_use]
pub fn is_whole_in_a_lane(span: (f32, f32), lanes: &[(f32, f32)]) -> bool {
    lanes
        .iter()
        .any(|&(left, right)| span.0 >= left && span.1 <= right)
}

/// How far a strip of `content` width scrolls in a view of `viewport` width: never negative.
#[must_use]
pub fn max_scroll(content: f32, viewport: f32) -> f32 {
    let m = content - viewport;
    if m.is_finite() && m > 0.0 {
        m
    } else {
        0.0
    }
}

/// The scroll offset at which the selected `pill` (`(x, width)` on the strip) stands whole inside
/// a lane, given the strip is at `scroll` now, can scroll `0..=max_scroll`, and has `lanes`
/// (`(left, right)` pairs in the strip's coordinates; one lane the width of the view for a strip
/// that is not in the status bar's band).
///
/// The rules:
/// - a pill already whole in a lane, at least [`LANE_INSET`] (or what the lane has to spare) from
///   its edges, STAYS: the strip does not move under a finger that just tapped it;
/// - otherwise it is CENTRED in the lane that takes the shortest scroll, as far as the scroll
///   range lets it (the first pills cannot be pulled to the middle, the last ones neither);
/// - a lane narrower than the pill cannot take it; with no lane that can, the pill is brought to
///   the start of the widest lane and is clipped there, which is the least bad of the answers;
/// - never an offset outside `0..=max_scroll`, whatever the input (a number that is not one is
///   no scroll).
#[must_use]
pub fn scroll_for_pill(
    pill: (f32, f32),
    scroll: f32,
    max_scroll: f32,
    lanes: &[(f32, f32)],
) -> f32 {
    let max = if max_scroll.is_finite() && max_scroll > 0.0 {
        max_scroll
    } else {
        0.0
    };
    let now = if scroll.is_finite() {
        scroll.clamp(0.0, max)
    } else {
        0.0
    };
    let (x, width) = (pill.0, pill.1);
    if !x.is_finite() || !width.is_finite() || width <= 0.0 || lanes.is_empty() {
        return now;
    }
    // Already in place?
    let span = pill_span(pill, now);
    let stays = lanes.iter().any(|&(left, right)| {
        let spare = ((right - left) - width).max(0.0) / 2.0;
        let inset = LANE_INSET.min(spare);
        span.0 >= left + inset && span.1 <= right - inset
    });
    if stays {
        return now;
    }
    // The best scroll for each lane that can take the pill: the centring one, kept inside what
    // keeps the pill whole and inside the scroll range.
    let mut best: Option<(f32, f32)> = None;
    for &(left, right) in lanes {
        let room = right - left;
        if room < width {
            continue;
        }
        let inset = LANE_INSET.min((room - width) / 2.0);
        let lowest = (x + width - right + inset).max(0.0);
        let highest = (x - left - inset).min(max);
        if lowest > highest {
            continue;
        }
        let centred = x + width / 2.0 - f32::midpoint(left, right);
        let target = centred.clamp(lowest, highest);
        let cost = (target - now).abs();
        if best.is_none_or(|(_, c)| cost < c) {
            best = Some((target, cost));
        }
    }
    if let Some((target, _)) = best {
        return target;
    }
    // No lane can take it whole: the start of the widest lane.
    let widest = lanes
        .iter()
        .copied()
        .max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)));
    widest.map_or(now, |(left, _)| (x - left).clamp(0.0, max))
}

/// The rects of the covers a strip in the band lays over its gaps: one per gap, as tall as the band,
/// in the strip's coordinates. A cover is drawn in the colour of the band over the pills, so a
/// pill sliding into a gap is hidden by it, and a finger on it touches the cover, not a hidden
/// pill. The gaps at the very edges of the window (the insets) are covered as well.
#[must_use]
pub fn cover_rects(band: &StatusBarBand) -> Vec<LogicalRect> {
    band.gaps()
        .into_iter()
        .filter(|&(left, right)| right - left >= 0.5)
        .map(|(left, right)| {
            LogicalRect::new(
                LogicalPosition { x: left, y: 0.0 },
                LogicalSize {
                    width: right - left,
                    height: band.height,
                },
            )
        })
        .collect()
}

#[cfg(test)]
#[path = "tab_strip_test.rs"]
mod tab_strip_test;
