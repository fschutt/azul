//! Where everything sits, as numbers. The screens give each zone an explicit height and width taken from here, so the
//! pad (the one flexible zone) is known to the pixel when the DOM is built: its page can be fitted to it and the
//! viewport rectangle drawn on it without asking the layout engine. A test checks that the zones fill the safe area
//! exactly, never overlap, and that every touch target is big enough, for every size of the study.

use crate::env::Env;

/// The least a touch target may be, in logical pixels (iOS 44 pt, Android 48 dp: the larger, for both).
pub const MIN_TARGET: f32 = 44.0;

/// A rectangle in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Zone {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Zone {
    /// The right edge.
    #[must_use]
    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    /// The bottom edge.
    #[must_use]
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Whether two zones share any area (touching edges do not count).
    #[must_use]
    pub fn overlaps(&self, other: &Zone) -> bool {
        self.x < other.right() - 0.01 && other.x < self.right() - 0.01 && self.y < other.bottom() - 0.01 && other.y < self.bottom() - 0.01
    }
}

/// What the screens that share the tab bar (the remote and the files) are made of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zones {
    /// The window's safe area.
    pub safe: Zone,
    /// The side gutter inside the safe area.
    pub gutter: f32,
    /// The connection and file line(s) at the top of the pad's column.
    pub header: Zone,
    /// Portrait only: the file / page / line line under the header (landscape folds it into the header).
    pub filebar: Zone,
    /// The touch pad.
    pub pad: Zone,
    /// Fit, reset, previous and next page.
    pub controls: Zone,
    /// "The buttons act on ..."
    pub context: Zone,
    /// The four quick buttons.
    pub dock: Zone,
    /// The height of one quick button.
    pub button_h: f32,
    /// The gap between quick buttons.
    pub button_gap: f32,
    /// Portrait: the tab bar. Landscape: the side panel's switch (Buttons / Files, and the gear).
    pub tabs: Zone,
    /// Landscape only: the side panel (its switch, then its body); portrait: empty.
    pub side: Zone,
    /// The body of the side panel below its switch (landscape), or the body of the Files screen (portrait).
    pub side_body: Zone,
}

/// The zones of the remote and the files screens for `env`.
#[must_use]
pub fn zones(env: &Env) -> Zones {
    let ins = env.insets.held(env.width, env.height);
    let safe = Zone {
        x: ins.left,
        y: ins.top,
        w: (env.width - ins.left - ins.right).max(0.0),
        h: (env.height - ins.top - ins.bottom).max(0.0),
    };
    if env.landscape() {
        landscape(safe)
    } else {
        portrait(safe, env.short())
    }
}

/// The remote and the files stacked: header, file line, pad, controls, context, the four buttons, the tab bar.
fn portrait(safe: Zone, short: bool) -> Zones {
    let gutter = if short { 12.0 } else { 16.0 };
    let (header_h, filebar_h, controls_h, context_h, tabs_h) =
        if short { (48.0, 36.0, 48.0, 24.0, 52.0) } else { (56.0, 40.0, 52.0, 28.0, 56.0) };
    let (button_h, button_gap) = if short { (52.0, 8.0) } else { (60.0, 10.0) };
    let dock_h = 2.0 * button_h + button_gap + 8.0;
    let fixed = header_h + filebar_h + controls_h + context_h + dock_h + tabs_h;
    let pad_h = (safe.h - fixed).max(0.0);
    let band = |y: f32, h: f32| Zone { x: safe.x, y, w: safe.w, h };
    let header = band(safe.y, header_h);
    let filebar = band(header.bottom(), filebar_h);
    let pad = Zone { x: safe.x + gutter, y: filebar.bottom(), w: (safe.w - 2.0 * gutter).max(0.0), h: pad_h };
    let controls = band(pad.bottom(), controls_h);
    let context = band(controls.bottom(), context_h);
    let dock = band(context.bottom(), dock_h);
    let tabs = band(dock.bottom(), tabs_h);
    // the Files screen: everything between the header and the tab bar
    let side_body = Zone { x: safe.x, y: header.bottom(), w: safe.w, h: (safe.h - header_h - tabs_h).max(0.0) };
    Zones {
        safe,
        gutter,
        header,
        filebar,
        pad,
        controls,
        context,
        dock,
        button_h,
        button_gap,
        tabs,
        side: Zone::default(),
        side_body,
    }
}

/// The pad on the left with its header and controls, the side panel (a switch, then the buttons or the files) on the right.
fn landscape(safe: Zone) -> Zones {
    let gutter = 12.0;
    let side_w = 300.0_f32.min(safe.w * 0.4);
    let col_w = (safe.w - side_w).max(0.0);
    let (header_h, controls_h, switch_h, context_h): (f32, f32, f32, f32) = (48.0, 44.0, 48.0, 28.0);
    let header = Zone { x: safe.x, y: safe.y, w: col_w, h: header_h };
    let pad = Zone {
        x: safe.x + gutter,
        y: header.bottom(),
        w: (col_w - gutter - 8.0).max(0.0),
        h: (safe.h - header_h - controls_h).max(0.0),
    };
    let controls = Zone { x: safe.x, y: pad.bottom(), w: col_w, h: controls_h };
    let side = Zone { x: safe.x + col_w, y: safe.y, w: side_w, h: safe.h };
    let tabs = Zone { x: side.x, y: side.y, w: side_w, h: switch_h };
    let side_body = Zone { x: side.x, y: tabs.bottom(), w: side_w, h: (safe.h - switch_h).max(0.0) };
    let context = Zone { x: side.x, y: side_body.y, w: side_w, h: context_h.min(side_body.h) };
    let dock = Zone { x: side.x, y: context.bottom(), w: side_w, h: (side_body.bottom() - context.bottom()).max(0.0) };
    let button_gap = 8.0;
    let button_h = ((dock.h - 8.0 - 3.0 * button_gap) / 4.0).clamp(48.0, 64.0);
    Zones {
        safe,
        gutter,
        header,
        filebar: Zone { x: safe.x, y: header.bottom(), w: col_w, h: 0.0 },
        pad,
        controls,
        context,
        dock,
        button_h,
        button_gap,
        tabs,
        side,
        side_body,
    }
}

/// How the page and the right-edge strip sit inside the pad.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadGeometry {
    /// The page's picture, inside the pad (the pad's own coordinates).
    pub page: Zone,
    /// The strip along the right edge that turns pages with a swipe.
    pub strip: Zone,
}

/// The width of the page-turning strip.
pub const STRIP_W: f32 = 44.0;
/// The margin between the pad's edge and the page.
pub const PAD_MARGIN: f32 = 12.0;

/// The page's size when the Mac has not said (an A4 sheet).
pub const DEFAULT_ASPECT: f32 = 794.0 / 1123.0;

/// Fits a page of `aspect` (width over height) inside a pad of `pad_w` by `pad_h`, left of the strip.
#[must_use]
pub fn pad_geometry(pad_w: f32, pad_h: f32, aspect: Option<f32>) -> PadGeometry {
    let size = |v: f32| if v.is_finite() && v > 0.0 { v } else { 0.0 };
    let (pad_w, pad_h) = (size(pad_w), size(pad_h));
    let aspect = aspect.filter(|a| a.is_finite() && *a > 0.0).unwrap_or(DEFAULT_ASPECT);
    let room_w = (pad_w - STRIP_W - 2.0 * PAD_MARGIN).max(0.0);
    let room_h = (pad_h - 2.0 * PAD_MARGIN).max(0.0);
    let (w, h) = if room_h > 0.0 && room_w / room_h > aspect { (room_h * aspect, room_h) } else { (room_w, room_w / aspect) };
    let page = Zone { x: PAD_MARGIN + (room_w - w) / 2.0, y: PAD_MARGIN + (room_h - h) / 2.0, w, h };
    let strip_w = STRIP_W.min(pad_w);
    PadGeometry { page, strip: Zone { x: pad_w - strip_w, y: 0.0, w: strip_w, h: pad_h } }
}

#[cfg(test)]
#[path = "layout_test.rs"]
mod layout_test;
