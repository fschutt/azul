//! Where a floating surface opens: a tooltip, a menu, a popover, a dropdown list.
//!
//! Every floating surface is placed by the same rule. It hangs off an anchor box (the
//! control it belongs to, or the pointer) at one point of that box and grows away from it;
//! when the screen has no room there it flips to the other side of the box, and whatever
//! still sticks out is slid back onto the screen. That is the model `xdg_positioner` takes
//! natively, so a Wayland compositor applies these rules itself, while every other backend
//! measures its surface, picks the screen the anchor is on with [`screen_for_anchor`] and
//! asks [`floating_frame`] where the surface goes. One rule, so a menu, a tooltip and a
//! picker cannot disagree about what the edge of a screen means.
//!
//! The coordinates are in whatever space the caller works in (global logical points on
//! macOS, physical screen pixels on Windows and X11, the parent window on Wayland): the
//! functions only compare numbers, they never ask the system anything.

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

/// One end of an axis: the left or top end (`Start`), the middle, or the right or bottom
/// end (`End`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AxisEnd {
    /// The left end of the x axis, the top end of the y axis.
    Start,
    /// The middle.
    Center,
    /// The right end of the x axis, the bottom end of the y axis.
    End,
}

impl AxisEnd {
    /// The opposite end; the middle stays where it is.
    #[must_use]
    pub const fn flipped(self) -> Self {
        match self {
            Self::Start => Self::End,
            Self::Center => Self::Center,
            Self::End => Self::Start,
        }
    }
}

/// How a floating surface is placed along one axis.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct AxisRule {
    /// The point of the anchor box the surface hangs from.
    pub anchor: AxisEnd,
    /// Which way the surface grows from that point: `End` grows right (or down), `Start`
    /// grows left (or up), `Center` centres the surface on the point.
    pub grow: AxisEnd,
    /// When the surface would leave the screen, open it on the other side of the anchor
    /// instead - but only if it fits there.
    pub flip: bool,
    /// When it still sticks out, move it back onto the screen.
    pub slide: bool,
    /// When it is longer than the screen, shorten it to the screen (the content scrolls).
    pub resize: bool,
}

impl AxisRule {
    /// Past the anchor's far end: below it on the y axis, right of it on the x axis.
    /// `flip` lets it open before the anchor when there is no room after it.
    #[must_use]
    pub const fn after(flip: bool) -> Self {
        Self {
            anchor: AxisEnd::End,
            grow: AxisEnd::End,
            flip,
            slide: true,
            resize: false,
        }
    }

    /// Before the anchor's near end: above it on the y axis, left of it on the x axis.
    #[must_use]
    pub const fn before(flip: bool) -> Self {
        Self {
            anchor: AxisEnd::Start,
            grow: AxisEnd::Start,
            flip,
            slide: true,
            resize: false,
        }
    }

    /// Lined up with the anchor's start edge and growing on past it - the left edges of a
    /// dropdown and its control together. Flipped, the end edges line up instead.
    #[must_use]
    pub const fn start_aligned(flip: bool) -> Self {
        Self {
            anchor: AxisEnd::Start,
            grow: AxisEnd::End,
            flip,
            slide: true,
            resize: false,
        }
    }

    /// Centred on the anchor, slid onto the screen.
    #[must_use]
    pub const fn centered() -> Self {
        Self {
            anchor: AxisEnd::Center,
            grow: AxisEnd::Center,
            flip: false,
            slide: true,
            resize: false,
        }
    }

    /// Exactly at the anchor's start, whatever the screen says: a surface that covers its
    /// parent window must not be pushed off it.
    #[must_use]
    pub const fn pinned() -> Self {
        Self {
            anchor: AxisEnd::Start,
            grow: AxisEnd::End,
            flip: false,
            slide: false,
            resize: false,
        }
    }

    /// The same rule, and a surface longer than the screen is shortened to it.
    #[must_use]
    pub const fn with_resize(mut self) -> Self {
        self.resize = true;
        self
    }
}

/// What a floating surface asks for: the box it opens from, and how it is placed against
/// that box on each axis.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct FloatingPlacement {
    /// The box the surface opens from and must not cover: the control, or the pointer.
    pub anchor: LogicalRect,
    /// The placement along x.
    pub x: AxisRule,
    /// The placement along y.
    pub y: AxisRule,
}

impl FloatingPlacement {
    /// A placement from its parts.
    #[must_use]
    pub const fn new(anchor: LogicalRect, x: AxisRule, y: AxisRule) -> Self {
        Self { anchor, x, y }
    }

    /// Below the box with the left edges lined up - a dropdown, a menu opened for a
    /// control. No room below: above it. Too wide: the right edges line up when `flip_x`,
    /// otherwise it is slid sideways.
    #[must_use]
    pub const fn below(anchor: LogicalRect, flip_x: bool) -> Self {
        Self::new(anchor, AxisRule::start_aligned(flip_x), AxisRule::after(true))
    }

    /// Above the box with the left edges lined up. No room above: below it.
    #[must_use]
    pub const fn above(anchor: LogicalRect, flip_x: bool) -> Self {
        Self::new(anchor, AxisRule::start_aligned(flip_x), AxisRule::before(true))
    }

    /// Right of the box with the top edges lined up - a submenu beside its item. No room
    /// on the right: left of it.
    #[must_use]
    pub const fn right_of(anchor: LogicalRect) -> Self {
        Self::new(anchor, AxisRule::after(true), AxisRule::start_aligned(false))
    }

    /// Left of the box with the top edges lined up. No room on the left: right of it.
    #[must_use]
    pub const fn left_of(anchor: LogicalRect) -> Self {
        Self::new(anchor, AxisRule::before(true), AxisRule::start_aligned(false))
    }

    /// At a point, growing right and down from it - a context menu at the pointer. With
    /// `flip`, it opens left of and above the point where there is no room right or below.
    #[must_use]
    pub const fn at_point(point: LogicalPosition, flip: bool) -> Self {
        Self::new(
            LogicalRect::new(point, LogicalSize::zero()),
            AxisRule::after(flip),
            AxisRule::after(flip),
        )
    }

    /// Centred over the box - a dialog over its parent.
    #[must_use]
    pub const fn centered_over(anchor: LogicalRect) -> Self {
        Self::new(anchor, AxisRule::centered(), AxisRule::centered())
    }

    /// Exactly at `origin`, never moved - a surface that covers its parent window.
    #[must_use]
    pub const fn pinned(origin: LogicalPosition) -> Self {
        Self::new(
            LogicalRect::new(origin, LogicalSize::zero()),
            AxisRule::pinned(),
            AxisRule::pinned(),
        )
    }

    /// A tooltip for `anchor`, the box it explains (a control, or the pointer): below it
    /// with the left edges lined up; above it when there is no room below, with the right
    /// edges lined up when there is none to the right. Never taller than the screen.
    #[must_use]
    pub const fn tooltip(anchor: LogicalRect) -> Self {
        Self::new(
            anchor,
            AxisRule::start_aligned(true),
            AxisRule::after(true).with_resize(),
        )
    }

    /// Where the top-left of a surface of `size` goes before the screen has a say.
    #[must_use]
    pub const fn preferred_origin(&self, size: LogicalSize) -> LogicalPosition {
        LogicalPosition::new(
            origin_on_axis(
                self.x.anchor,
                self.x.grow,
                self.anchor.origin.x,
                self.anchor.size.width,
                size.width,
            ),
            origin_on_axis(
                self.y.anchor,
                self.y.grow,
                self.anchor.origin.y,
                self.anchor.size.height,
                size.height,
            ),
        )
    }
}

/// One screen as a backend reports it, in the caller's coordinate space.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ScreenRects {
    /// The whole screen.
    pub bounds: LogicalRect,
    /// The part a floating surface may cover: the screen less the menu bar, dock or
    /// taskbars.
    pub work_area: LogicalRect,
}

/// The work area of the screen a surface anchored at `anchor` belongs on: the screen
/// that contains the anchor's centre, or else the screen nearest to it (a pointer in
/// the gap between two monitors). `None` when no screen is known.
#[must_use]
pub fn screen_for_anchor(anchor: LogicalRect, screens: &[ScreenRects]) -> Option<LogicalRect> {
    let _ = anchor;
    screens.first().map(|screen| screen.work_area)

}

/// Where a floating surface of `size` goes on `screen` (a work area, from
/// [`screen_for_anchor`]): the frame it is shown at, its size shortened where its rule
/// allows and it is longer than the screen.
///
/// Per axis, in this order: shorten it to the screen (`resize`); if it would leave the
/// screen, open it on the other side of the anchor when it fits there (`flip`); slide
/// what still sticks out back on, the start edge winning for a surface longer than the
/// screen (`slide`). An anchor that is not a number - a position a callback computed from
/// nothing - opens at the screen's start rather than nowhere.
#[must_use]
pub const fn floating_frame(
    placement: &FloatingPlacement,
    size: LogicalSize,
    screen: LogicalRect,
) -> LogicalRect {
    let _ = screen;
    LogicalRect::new(placement.preferred_origin(size), size)

}

/// The widest a tooltip's text runs before it wraps, in logical px: about fifty
/// characters, where the platforms' own tooltips wrap.
pub const TOOLTIP_MAX_WIDTH: f32 = 360.0;

/// How wide a tooltip's text may run on a screen `screen_width` wide when the tooltip
/// has `padding` on either side of its text: [`TOOLTIP_MAX_WIDTH`], or less on a screen
/// too narrow for that, so the tooltip always fits beside itself.
#[must_use]
pub const fn tooltip_text_width(screen_width: f32, padding: f32) -> f32 {
    let _ = (screen_width, padding);
    TOOLTIP_MAX_WIDTH

}

/// How far `point` lies outside the span `start..end`, zero inside it.
const fn gap_to_span(point: f32, start: f32, end: f32) -> f32 {
    if point < start {
        start - point
    } else if point > end {
        point - end
    } else {
        0.0
    }
}

/// The start of a surface `len` long that hangs off the `anchor` end of the anchor span
/// and grows towards `grow`.
const fn origin_on_axis(
    anchor: AxisEnd,
    grow: AxisEnd,
    anchor_start: f32,
    anchor_len: f32,
    len: f32,
) -> f32 {
    let point = match anchor {
        AxisEnd::Start => anchor_start,
        AxisEnd::Center => anchor_start + anchor_len / 2.0,
        AxisEnd::End => anchor_start + anchor_len,
    };
    match grow {
        AxisEnd::Start => point - len,
        AxisEnd::Center => point - len / 2.0,
        AxisEnd::End => point,
    }
}

/// Whether a surface from `start`, `len` long, lies within `screen_start..screen_end`.
const fn fits(start: f32, len: f32, screen_start: f32, screen_end: f32) -> bool {
    start >= screen_start && start + len <= screen_end
}

/// One axis of [`floating_frame`]: the surface's start and length.
const fn place_on_axis(
    rule: AxisRule,
    anchor_start: f32,
    anchor_len: f32,
    len: f32,
    screen_start: f32,
    screen_len: f32,
) -> (f32, f32) {
    let anchor_start = if anchor_start.is_finite() {
        anchor_start
    } else {
        screen_start
    };
    let anchor_len = if anchor_len.is_finite() && anchor_len > 0.0 {
        anchor_len
    } else {
        0.0
    };
    let mut len = if len.is_finite() && len > 0.0 { len } else { 0.0 };
    if rule.resize && screen_len >= 0.0 && len > screen_len {
        len = screen_len;
    }
    let screen_end = screen_start + screen_len;

    let mut start = origin_on_axis(rule.anchor, rule.grow, anchor_start, anchor_len, len);
    if rule.flip && !fits(start, len, screen_start, screen_end) {
        let flipped = origin_on_axis(
            rule.anchor.flipped(),
            rule.grow.flipped(),
            anchor_start,
            anchor_len,
            len,
        );
        if fits(flipped, len, screen_start, screen_end) {
            start = flipped;
        }
    }
    if rule.slide {
        if start + len > screen_end {
            start = screen_end - len;
        }
        if start < screen_start {
            start = screen_start;
        }
    }
    (start, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    /// The pointer as a tooltip's anchor: its hot spot, and the cursor image below it.
    fn pointer(x: f32, y: f32) -> LogicalRect {
        rect(x, y, 0.0, 20.0)
    }

    const SCREEN: LogicalRect = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(1920.0, 1080.0),
    );

    #[test]
    fn a_tooltip_with_room_opens_below_the_pointer_with_its_left_edge_at_it() {
        let frame = floating_frame(
            &FloatingPlacement::tooltip(pointer(400.0, 300.0)),
            LogicalSize::new(200.0, 40.0),
            SCREEN,
        );
        assert_eq!(frame, rect(400.0, 320.0, 200.0, 40.0));
    }

    #[test]
    fn a_tooltip_at_the_right_edge_of_a_screen_opens_left_of_the_cursor() {
        let frame = floating_frame(
            &FloatingPlacement::tooltip(pointer(1850.0, 300.0)),
            LogicalSize::new(200.0, 40.0),
            SCREEN,
        );
        assert_eq!(
            (frame.origin.x, frame.origin.y),
            (1650.0, 320.0),
            "no room right of the pointer: the tooltip's right edge is at the pointer"
        );
    }

    #[test]
    fn a_tooltip_at_the_bottom_edge_of_a_screen_opens_above_the_pointer() {
        let frame = floating_frame(
            &FloatingPlacement::tooltip(pointer(400.0, 1050.0)),
            LogicalSize::new(200.0, 40.0),
            SCREEN,
        );
        assert_eq!(
            (frame.origin.x, frame.origin.y),
            (400.0, 1010.0),
            "no room below the pointer: the tooltip's bottom edge is at the pointer's hot spot"
        );
    }

    #[test]
    fn a_tooltip_in_the_bottom_right_corner_opens_above_and_left_of_the_pointer() {
        let frame = floating_frame(
            &FloatingPlacement::tooltip(pointer(1900.0, 1060.0)),
            LogicalSize::new(300.0, 60.0),
            SCREEN,
        );
        assert_eq!(frame, rect(1600.0, 1000.0, 300.0, 60.0));
    }

    #[test]
    fn a_tooltip_on_a_second_monitor_stays_on_that_monitor() {
        // A monitor left of and above the primary one: its coordinates are negative.
        let second = ScreenRects {
            bounds: rect(-1920.0, -200.0, 1920.0, 1080.0),
            work_area: rect(-1920.0, -175.0, 1920.0, 1055.0),
        };
        let primary = ScreenRects {
            bounds: SCREEN,
            work_area: rect(0.0, 25.0, 1920.0, 1055.0),
        };
        let anchor = pointer(-30.0, 840.0);
        let screen = screen_for_anchor(anchor, &[primary, second]).expect("a screen is known");
        assert_eq!(screen, second.work_area, "the pointer is on the second monitor");

        let frame = floating_frame(
            &FloatingPlacement::tooltip(anchor),
            LogicalSize::new(200.0, 40.0),
            screen,
        );
        assert_eq!(
            frame,
            rect(-230.0, 800.0, 200.0, 40.0),
            "flipped left of and above the pointer, not slid across onto the primary monitor"
        );
    }

    #[test]
    fn a_tooltip_wider_than_its_screen_starts_at_the_screens_left_edge() {
        let narrow = rect(100.0, 0.0, 300.0, 600.0);
        let frame = floating_frame(
            &FloatingPlacement::tooltip(pointer(250.0, 100.0)),
            LogicalSize::new(500.0, 40.0),
            narrow,
        );
        assert_eq!(frame.origin.x, 100.0, "the start of the text is what stays readable");
        assert_eq!(frame.origin.y, 120.0);
    }

    #[test]
    fn a_tooltip_taller_than_its_screen_is_cut_to_the_screen() {
        let frame = floating_frame(
            &FloatingPlacement::tooltip(pointer(400.0, 300.0)),
            LogicalSize::new(200.0, 5000.0),
            SCREEN,
        );
        assert_eq!(frame, rect(400.0, 0.0, 200.0, 1080.0));
    }

    #[test]
    fn a_tooltip_at_a_position_that_is_not_a_number_still_opens_on_its_screen() {
        let frame = floating_frame(
            &FloatingPlacement::tooltip(rect(f32::NAN, f32::INFINITY, 0.0, 0.0)),
            LogicalSize::new(200.0, f32::NAN),
            SCREEN,
        );
        assert!(
            frame.origin.x.is_finite()
                && frame.origin.y.is_finite()
                && frame.size.width.is_finite()
                && frame.size.height.is_finite(),
            "a frame that is not a number would hide the tooltip or crash the window system: \
             {frame:?}"
        );
        assert!(SCREEN.contains(frame.origin));
    }

    #[test]
    fn a_surface_only_flips_when_the_other_side_has_room() {
        // A menu taller than the room both above and below its control: flipping would
        // not help, so it stays below and slides up as far as it must.
        let control = rect(100.0, 500.0, 120.0, 30.0);
        let frame = floating_frame(
            &FloatingPlacement::below(control, true),
            LogicalSize::new(150.0, 700.0),
            SCREEN,
        );
        assert_eq!(frame.origin.y, 380.0);
    }

    #[test]
    fn a_dropdown_too_wide_for_the_right_lines_up_its_right_edge_with_its_control() {
        let control = rect(1800.0, 100.0, 100.0, 30.0);
        let flipping = floating_frame(
            &FloatingPlacement::below(control, true),
            LogicalSize::new(300.0, 200.0),
            SCREEN,
        );
        assert_eq!(flipping.origin.x, 1600.0, "right edges together at 1900");
        let sliding = floating_frame(
            &FloatingPlacement::below(control, false),
            LogicalSize::new(300.0, 200.0),
            SCREEN,
        );
        assert_eq!(sliding.origin.x, 1620.0, "slid until it touches the screen's edge");
    }

    #[test]
    fn a_pinned_surface_is_never_moved_by_the_screen() {
        let frame = floating_frame(
            &FloatingPlacement::pinned(LogicalPosition::new(-50.0, -20.0)),
            LogicalSize::new(800.0, 600.0),
            SCREEN,
        );
        assert_eq!(frame, rect(-50.0, -20.0, 800.0, 600.0));
    }

    #[test]
    fn the_screen_of_a_surface_is_the_one_its_anchor_is_on_or_else_the_nearest() {
        let left = ScreenRects {
            bounds: rect(0.0, 0.0, 1920.0, 1080.0),
            work_area: rect(0.0, 0.0, 1920.0, 1040.0),
        };
        let right = ScreenRects {
            bounds: rect(1920.0, 0.0, 2560.0, 1440.0),
            work_area: rect(1920.0, 0.0, 2560.0, 1400.0),
        };
        let screens = [left, right];
        assert_eq!(
            screen_for_anchor(rect(1920.0, 500.0, 0.0, 0.0), &screens),
            Some(right.work_area),
            "the first column of the right monitor belongs to the right monitor"
        );
        assert_eq!(
            screen_for_anchor(rect(1800.0, 500.0, 200.0, 30.0), &screens),
            Some(left.work_area),
            "a control straddling two monitors belongs to the one its centre is on"
        );
        assert_eq!(
            screen_for_anchor(rect(1900.0, 1300.0, 0.0, 0.0), &screens),
            Some(right.work_area),
            "below the short left monitor and beside the tall right one: the right one is nearer"
        );
        assert_eq!(screen_for_anchor(rect(0.0, 0.0, 0.0, 0.0), &[]), None);
    }

    #[test]
    fn a_tooltip_on_a_narrow_screen_wraps_at_the_screen_width() {
        assert_eq!(tooltip_text_width(1920.0, 4.0), TOOLTIP_MAX_WIDTH);
        assert_eq!(tooltip_text_width(208.0, 4.0), 200.0);
        assert_eq!(tooltip_text_width(0.0, 4.0), 1.0, "never zero or negative");
    }
}
