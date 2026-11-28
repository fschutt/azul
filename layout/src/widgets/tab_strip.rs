//! The geometry of a tab strip that scrolls sideways without a scrollbar and, in the status bar's
//! band, goes around the cutouts of the display.
//!
//! A strip is one row of pills longer than the room it has. It scrolls: a finger drags it with
//! momentum, a wheel or a trackpad moves it, the selected pill comes into view by itself. In
//! the status bar's band ([`StatusBarBand`]) the strip takes the place of the clock and the
//! battery icons: the room it has is not one stretch but LANES, the band minus every cutout (and
//! a margin around each). The row slides under the camera like a strip of paper behind a hole:
//! what lies under a cutout is covered, never drawn, and a pill half in one lane is the hint
//! that there is more on the other side. AT REST, though, no pill stands under the camera: a pill
//! that would is given the room to start the next lane ([`room_around_cutouts`]), so that a strip of
//! four tabs reads `FILE HOME (camera) INSERT VIEW`, not `FILE HOME (blank) VIEW`. The selected pill
//! is never left half under the camera either: [`scroll_for_pill`] picks a scroll offset at which it
//! stands whole inside one lane.
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
            lanes,
            height,
            width,
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
    scroll_for_pill_inset(pill, scroll, max_scroll, lanes, LANE_INSET)
}

/// [`scroll_for_pill`] with the room a pill keeps from a lane's edge, `inset`, given: a row whose first
/// pill stands 10 px in (the desktop's) keeps 10, so that a first or last pill at its place at rest STAYS.
#[must_use]
pub fn scroll_for_pill_inset(
    pill: (f32, f32),
    scroll: f32,
    max_scroll: f32,
    lanes: &[(f32, f32)],
    inset: f32,
) -> f32 {
    let inset_wanted = if inset.is_finite() { inset.max(0.0) } else { LANE_INSET };
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
        let inset = inset_wanted.min(spare);
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
        let inset = inset_wanted.min((room - width) / 2.0);
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

/// The room each pill of a strip needs in front of it, so that at rest no pill stands under a camera.
///
/// It is on top of the `gap` between pills: at rest (the strip scrolled to its start) no pill stands
/// in a gap of the band, or under the fade at a lane's end.
///
/// `widths` are the pills' widths in order, `lead` is where the first one starts (the room before it,
/// the chevron's included), `lanes` the band's lanes in the strip's coordinates, `inset` the room a
/// pill keeps from the edge of a lane, which is the width of the fade at its end. The pills are laid
/// out one after the other as they flow; one that does not fit what is left of its lane - from where it
/// would start to the lane's edge less `inset` - starts the next lane, `inset` from its edge, and the
/// pills after it follow. The last lane takes what comes: a strip longer than the lanes scrolls, and
/// what scrolls under a camera is covered, as it always was. A strip of fewer than two lanes has no
/// camera in it: no room is added.
///
/// The returned vector has one entry per pill, none negative: the pill's own `margin-left`.
#[must_use]
pub fn room_around_cutouts(
    widths: &[f32],
    lead: f32,
    gap: f32,
    lanes: &[(f32, f32)],
    inset: f32,
) -> Vec<f32> {
    let mut room = vec![0.0_f32; widths.len()];
    if lanes.len() < 2 || !lead.is_finite() || !gap.is_finite() {
        return room;
    }
    // What of lane `k` a pill may stand on: the first lane from where the strip starts (it holds the
    // chevron, when there is one), the others from `inset` in; every lane to `inset` before its end.
    let usable = |k: usize| -> (f32, f32) {
        let (left, right) = lanes[k];
        (if k == 0 { lead } else { left + inset }, right - inset)
    };
    let mut x = lead;
    let mut lane = 0;
    for (index, &width) in widths.iter().enumerate() {
        if !width.is_finite() || width <= 0.0 {
            continue;
        }
        loop {
            let (from, to) = usable(lane);
            let start = x.max(from);
            if start + width <= to + 0.01 || lane + 1 >= lanes.len() {
                room[index] = start - x;
                x = start;
                break;
            }
            lane += 1;
        }
        x += width + gap;
    }
    room
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

// ---- Icon-only pills (a phone's strip whose pills are ONE ICON each) ----
//
// The user's rule (2026-11-28): on a phone a tab header is ONE ICON, every pill the same SQUARE, so
// that the strip splits evenly over the lanes around the camera and fits, where text pills of
// every length slide under the hole and scroll. The rules are plain numbers:
// [`icon_pill_layout`] stands the pills in the lanes, [`icon_pill_width_where`] sizes them in a
// view whose width is only asked about (a strip with no band).

/// The widest (and tallest: an icon pill is a square) an icon pill is, in logical px.
pub const ICON_PILL_MAX: f32 = 40.0;

/// The narrowest an icon pill is, in logical px: as tall as a text pill. Where the pills do not
/// fit at this size the strip scrolls as a strip of text pills does.
pub const ICON_PILL_MIN: f32 = 32.0;

/// The least wide the target of an icon pill is, in logical px (44 pt / 48 dp is the touch
/// target; the target is a whole row tall): the pill is padded to it, the picture is not.
pub const ICON_TARGET_MIN: f32 = 44.0;

/// The most pills a strip is laid out for: a strip of more tabs than this is not a strip any
/// finger can use, and the layout (positions, lane shares) is made for this many ([`IconPillLayout::capped`]).
pub const MAX_STRIP_PILLS: usize = 256;

/// How a strip of icon pills stands: the one width, the room between two pills of one lane, where
/// each pill's left edge is, and whether nothing needs to scroll.
#[derive(Debug, Clone, PartialEq)]
pub struct IconPillLayout {
    /// The width of EVERY pill, in logical px: between [`ICON_PILL_MIN`] and [`ICON_PILL_MAX`].
    /// An icon pill is a square: this is its height too.
    pub pill_width: f32,
    /// The room between two pills of one lane, in logical px.
    pub gap: f32,
    /// The left edge of each pill in the strip's coordinates, one per tab, left to right.
    pub positions: Vec<f32>,
    /// Whether every pill stands whole inside a lane, so that nothing needs to scroll. `false`:
    /// the pills are [`ICON_PILL_MIN`] wide in one row from the first lane's start, as a strip of
    /// text pills is laid, and the row scrolls.
    pub fits: bool,
    /// The tabs that were asked for were more than [`MAX_STRIP_PILLS`] and the layout was made for
    /// that many: the diagnostic of the cap.
    pub capped: bool,
}

/// How many pills of width `width` with `gap` between them stand in a span of `room` px.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a few pills, never negative
fn pills_in(room: f32, width: f32, gap: f32) -> usize {
    if !(room.is_finite() && room >= width && width > 0.0) {
        return 0;
    }
    // n pills need n * width + (n - 1) * gap <= room
    let n = ((room + gap) / (width + gap)).floor();
    if n.is_finite() && n >= 1.0 {
        n as usize
    } else {
        0
    }
}

/// The pills of each lane when `count` stand in lanes whose rooms take at most `caps` of them: the
/// even split (the first lanes get the odd one out), and what a lane cannot take goes to the lanes
/// that can, the nearest first. `None` when the lanes cannot take `count` together.
fn lane_shares(caps: &[usize], count: usize) -> Option<Vec<usize>> {
    if caps.is_empty() || caps.iter().sum::<usize>() < count {
        return None;
    }
    let lanes = caps.len();
    let mut shares: Vec<usize> = (0..lanes)
        .map(|l| count / lanes + usize::from(l < count % lanes))
        .collect();
    // What does not fit where it was dealt moves to the nearest lane with room.
    for l in 0..lanes {
        while shares[l] > caps[l] {
            let target = (1..lanes)
                .flat_map(|d| [l.checked_sub(d), l.checked_add(d).filter(|&r| r < lanes)])
                .flatten()
                .find(|&t| shares[t] < caps[t])?;
            shares[l] -= 1;
            shares[target] += 1;
        }
    }
    Some(shares)
}

/// Stand `count` icon pills in `lanes` (`(left, right)` pairs in the strip's coordinates, one lane
/// the width of the view for a strip that is not in a band; `gap` the room between two pills).
///
/// Every pill is the same width: the largest whole number of px in
/// [`ICON_PILL_MIN`]..=[`ICON_PILL_MAX`] at which every lane takes its share of the pills with
/// [`LANE_INSET`] to spare at both ends. The pills are shared out EVENLY over the lanes - the first
/// `ceil(count / lanes)` in the first lane, the rest in the next (a lane that cannot take its
/// share gives the rest to its nearest neighbour) - and each lane's pills stand in a group centred
/// in the lane, so two lanes either side of a camera hole mirror each other. Where they cannot
/// stand at [`ICON_PILL_MIN`] the layout says `fits: false` and is the plain row of minimum-width
/// pills that scrolls.
///
/// Counts above [`MAX_STRIP_PILLS`] are laid out as [`MAX_STRIP_PILLS`] (`capped`), and numbers
/// that are not numbers make a lane no lane: never a crash.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // at most MAX_STRIP_PILLS pills, a few px of width
pub fn icon_pill_layout(lanes: &[(f32, f32)], count: usize, gap: f32) -> IconPillLayout {
    let capped = count > MAX_STRIP_PILLS;
    let count = count.min(MAX_STRIP_PILLS);
    let gap = if gap.is_finite() && gap >= 0.0 { gap } else { 0.0 };
    let lanes: Vec<(f32, f32)> = lanes
        .iter()
        .copied()
        .filter(|&(left, right)| left.is_finite() && right.is_finite() && right > left)
        .collect();
    let inset = |left: f32, right: f32| -> f32 { LANE_INSET.min(((right - left) / 2.0).max(0.0)) };
    let row = |first: f32| -> IconPillLayout {
        IconPillLayout {
            pill_width: ICON_PILL_MIN,
            gap,
            positions: (0..count)
                .map(|i| first + i as f32 * (ICON_PILL_MIN + gap))
                .collect(),
            fits: false,
            capped,
        }
    };
    if count == 0 {
        return IconPillLayout {
            pill_width: ICON_PILL_MAX,
            gap,
            positions: Vec::new(),
            fits: true,
            capped,
        };
    }
    let Some(first) = lanes.first() else {
        return row(LANE_INSET);
    };
    // The room of each lane for pills: the lane less the inset at both ends.
    let rooms: Vec<f32> = lanes
        .iter()
        .map(|&(left, right)| (right - left) - 2.0 * inset(left, right))
        .collect();
    // The largest whole number of px first, down to the smallest.
    let steps = (ICON_PILL_MAX - ICON_PILL_MIN) as u32;
    for step in 0..=steps {
        let width = ICON_PILL_MAX - step as f32;
        let caps: Vec<usize> = rooms.iter().map(|&room| pills_in(room, width, gap)).collect();
        if let Some(shares) = lane_shares(&caps, count) {
            let mut positions = Vec::with_capacity(count);
            for (&(left, right), &share) in lanes.iter().zip(&shares) {
                if share == 0 {
                    continue;
                }
                let group = share as f32 * width + (share - 1) as f32 * gap;
                let start = left + ((right - left) - group) / 2.0;
                positions.extend((0..share).map(|i| start + i as f32 * (width + gap)));
            }
            return IconPillLayout {
                pill_width: width,
                gap,
                positions,
                fits: true,
                capped,
            };
        }
    }
    row(first.0 + inset(first.0, first.1))
}

/// The width of an icon pill in a view whose width is only ASKED about (a strip in no band: the
/// window's width is known to the build only as the answers it records): the largest of
/// [`ICON_PILL_MAX`], and each 4 px less down to [`ICON_PILL_MIN`], at which a row of `count`
/// pills with `lead` before the first and after the last and `gap` between them is told to fit by
/// `fits(total)`, and `true`; [`ICON_PILL_MIN`] and `false` where none does (the row scrolls). Only
/// the widths of the steps tried are asked, widest first.
#[must_use]
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)] // at most MAX_STRIP_PILLS pills, a few px of width
pub fn icon_pill_width_where(
    count: usize,
    lead: f32,
    gap: f32,
    mut fits: impl FnMut(f32) -> bool,
) -> (f32, bool) {
    let count = count.min(MAX_STRIP_PILLS);
    let lead = if lead.is_finite() && lead >= 0.0 { lead } else { 0.0 };
    let gap = if gap.is_finite() && gap >= 0.0 { gap } else { 0.0 };
    if count == 0 {
        return (ICON_PILL_MAX, true);
    }
    let steps = ((ICON_PILL_MAX - ICON_PILL_MIN) / 4.0) as u32;
    for step in 0..=steps {
        let width = ICON_PILL_MAX - 4.0 * step as f32;
        let total = 2.0 * lead + count as f32 * width + (count - 1) as f32 * gap;
        if fits(total) {
            return (width, true);
        }
    }
    (ICON_PILL_MIN, false)
}

#[cfg(test)]
#[path = "tab_strip_test.rs"]
mod tab_strip_test;
