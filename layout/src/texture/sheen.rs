//! The cursor's sheen on the materials: a soft highlight that follows the
//! cursor over a material surface, easing in over 140 ms when the cursor
//! comes over it and out over 320 ms when it leaves.
//!
//! The window keeps one [`SheenSurface`] per node that draws a built-in
//! material: the area whose hover drives it (the node itself, or the largest
//! ancestor sharing its `--azul-sheen-area` name, so a thin trim inside a
//! larger area shines whenever the cursor is over the area), and the eased
//! amount. Steps run on the animation clock; they ask for frames only while
//! an amount is still moving or the cursor moves over a lit surface.

use alloc::collections::BTreeMap;

use azul_core::{
    dom::DomId,
    geom::{LogicalPosition, LogicalRect},
    id::NodeId,
};

/// How close an amount must come to its target to be settled.
const SETTLED: f32 = 0.003;
/// The easing's time constant toward 1 (the cursor came), seconds.
const EASE_IN_S: f32 = 0.14;
/// The easing's time constant toward 0 (the cursor left), seconds.
const EASE_OUT_S: f32 = 0.32;

/// One material surface's sheen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SheenSurface {
    /// The rect whose hover drives the sheen, window logical px.
    pub area: LogicalRect,
    /// The surface's own rect, window logical px: the cursor is handed to
    /// the material relative to its top-left.
    pub rect: LogicalRect,
    /// The eased amount, 0..1.
    pub amount: f32,
    /// The cursor the amount was last stepped with (window logical px), to
    /// tell a cursor that moved over a lit surface.
    pub stepped_at: Option<LogicalPosition>,
    /// The cursor's last place over the area: a fading sheen stays where the
    /// cursor left it.
    pub last_cursor: LogicalPosition,
}

impl SheenSurface {
    /// A surface at rest: no sheen.
    #[must_use]
    pub const fn new(area: LogicalRect, rect: LogicalRect) -> Self {
        Self {
            area,
            rect,
            amount: 0.0,
            stepped_at: None,
            last_cursor: LogicalPosition::new(rect.origin.x, rect.origin.y),
        }
    }

    /// Where the amount is going for a cursor at `cursor` (none = out of the
    /// window): 1 over the area, 0 elsewhere.
    #[must_use]
    pub fn target(&self, cursor: Option<LogicalPosition>) -> f32 {
        match cursor {
            Some(c) if self.area.contains(c) => 1.0,
            _ => 0.0,
        }
    }

    /// The cursor relative to the surface's top-left, logical px.
    #[must_use]
    pub fn cursor_in_surface(&self) -> LogicalPosition {
        LogicalPosition::new(
            self.last_cursor.x - self.rect.origin.x,
            self.last_cursor.y - self.rect.origin.y,
        )
    }
}

/// Every material surface's sheen in a window.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MaterialSheen {
    /// One per node drawing a built-in material.
    pub surfaces: BTreeMap<(DomId, NodeId), SheenSurface>,
    /// A step changed what a frame must show since the last
    /// [`Self::take_stepped`].
    pub stepped: bool,
}

impl MaterialSheen {
    /// Whether a frame is owed: an amount is still moving toward its
    /// target, or the cursor moved over a surface that shines.
    #[must_use]
    pub fn needs_frame(&self, cursor: Option<LogicalPosition>) -> bool {
        self.surfaces.values().any(|s| {
            (s.amount - s.target(cursor)).abs() > SETTLED
                || (s.amount > 0.0 && cursor.is_some() && s.stepped_at != cursor)
        })
    }

    /// Advance every amount by `dt_s` seconds toward its target for a cursor
    /// at `cursor`. Returns whether anything a frame shows changed.
    pub fn step(&mut self, dt_s: f32, cursor: Option<LogicalPosition>) -> bool {
        let dt = dt_s.max(0.0);
        let mut changed = false;
        for s in self.surfaces.values_mut() {
            let target = s.target(cursor);
            if let Some(c) = cursor {
                if s.area.contains(c) {
                    if s.amount > 0.0 && s.last_cursor != c {
                        changed = true;
                    }
                    s.last_cursor = c;
                }
            }
            if (s.amount - target).abs() > SETTLED {
                let tau = if target > s.amount { EASE_IN_S } else { EASE_OUT_S };
                s.amount += (target - s.amount) * (1.0 - (-dt / tau).exp());
                if (s.amount - target).abs() <= SETTLED {
                    s.amount = target;
                }
                changed = true;
            } else if s.amount != target {
                s.amount = target;
                changed = true;
            }
            s.stepped_at = cursor;
        }
        self.stepped |= changed;
        changed
    }

    /// Whether a step changed a frame since the last ask; clears it.
    pub fn take_stepped(&mut self) -> bool {
        core::mem::take(&mut self.stepped)
    }

    /// Keep the surfaces of `seen` (with their current areas), add new ones
    /// at rest, drop the rest.
    pub fn sync(&mut self, seen: BTreeMap<(DomId, NodeId), (LogicalRect, LogicalRect)>) {
        let mut next = BTreeMap::new();
        for (key, (area, rect)) in seen {
            let surface = match self.surfaces.get(&key) {
                Some(old) => SheenSurface { area, rect, ..*old },
                None => SheenSurface::new(area, rect),
            };
            next.insert(key, surface);
        }
        self.surfaces = next;
    }
}

#[cfg(test)]
mod tests {
    use azul_core::geom::LogicalSize;

    use super::*;

    fn one() -> MaterialSheen {
        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(100.0, 40.0));
        let mut s = MaterialSheen::default();
        s.surfaces.insert((DomId::ROOT_ID, NodeId::new(1)), SheenSurface::new(rect, rect));
        s
    }

    #[test]
    fn the_sheen_eases_in_faster_than_it_fades() {
        let mut s = one();
        let over = Some(LogicalPosition::new(50.0, 20.0));
        assert!(s.needs_frame(over));
        assert!(s.step(0.14, over));
        let lit = s.surfaces.values().next().map(|x| x.amount).unwrap_or(0.0);
        assert!((lit - (1.0 - (-1.0_f32).exp())).abs() < 1e-3, "{lit}");
        assert!(s.step(0.32, None));
        let fading = s.surfaces.values().next().map(|x| x.amount).unwrap_or(0.0);
        assert!((fading - lit * (-1.0_f32).exp()).abs() < 1e-3, "{fading}");
        for _ in 0..400 {
            s.step(0.016, None);
        }
        assert!(!s.needs_frame(None), "settled");
    }
}
