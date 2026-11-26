//! Presenting a CPU frame to an `ANativeWindow`: which rows changed since the last frame the surface
//! was given, what to ask of `ANativeWindow_lock`, and which rows to write after it answers.
//!
//! # What the system promises
//!
//! `ANativeWindow_lock` hands out the next buffer of the surface's pool (two or three, rotating). That
//! buffer holds an OLD frame. The NDK header (`android/native_window.h`) documents the one mechanism
//! that makes a partial write sound:
//!
//! > Lock the window's next drawing surface for writing. inOutDirtyBounds is used as an in/out
//! > parameter, upon entering the function, it contains the dirty region, that is, the region the
//! > caller intends to redraw. When the function returns, inOutDirtyBounds is updated with the actual
//! > area the caller needs to redraw -- this region is often extended by ANativeWindow_lock.
//!
//! The surface keeps what it posted last and, when the new buffer has the same size and format, copies
//! everything OUTSIDE the dirty region from it into the buffer it hands out. So the shell only has to
//! (1) say which rows it will redraw, and (2) write exactly the rows that come back. Called without a
//! rectangle the whole buffer is "dirty" and nothing is carried over.
//!
//! The shell used to keep its own per-buffer history instead, keyed by the address the buffer was
//! mapped at (an address is no identity: gralloc may map a buffer afresh and reuse addresses). Its
//! bookkeeping never included the rows of the frame being presented, recorded a full copy as "no
//! damage" for the other buffers, kept only the LAST render's damage of the several one regeneration
//! can make, and was never reset when the surface was replaced. The overlay scrollbar of a
//! page that fades out and the grey strip of a relayout stayed on screen (the 2026-11-26 emulator
//! run of AzWidgets). All of that is gone: the system does the per-buffer work, this module only
//! accumulates what changed between two presents.
//!
//! The blit is row based (a partial row saves no `memcpy`), so damage is kept as ONE band of rows,
//! the bounding rows of every rect of every render since the last present.

/// Rows `top..bottom` of a frame, device pixels, `top < bottom`.
pub type Rows = (u32, u32);

/// What changed in the frame, in rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RowDamage {
    /// Nothing: the previous frame is still right.
    #[default]
    Nothing,
    /// These rows, and no others.
    Rows {
        /// First changed row.
        top: u32,
        /// One past the last changed row.
        bottom: u32,
    },
    /// Everything, or not known: the first frame, a new size, a new surface.
    Full,
}

impl RowDamage {
    /// The rows of device pixel rects `(x, y, width, height)` (what
    /// `FrameDamage::to_rects_physical` returns): the band from the top of the topmost to the
    /// bottom of the lowest. No rect is nothing; a rect with no area counts as nothing.
    #[must_use]
    pub fn from_rects(rects: &[(u32, u32, u32, u32)]) -> Self {
        let mut damage = Self::Nothing;
        for &(_, y, width, height) in rects {
            if width == 0 || height == 0 {
                continue;
            }
            damage.merge(Self::Rows {
                top: y,
                bottom: y.saturating_add(height),
            });
        }
        damage
    }

    /// Add `other`: the union, as one band. `Full` absorbs everything.
    pub fn merge(&mut self, other: Self) {
        *self = match (*self, other) {
            (Self::Full, _) | (_, Self::Full) => Self::Full,
            (Self::Nothing, rows) | (rows, Self::Nothing) => rows,
            (
                Self::Rows { top, bottom },
                Self::Rows {
                    top: other_top,
                    bottom: other_bottom,
                },
            ) => Self::Rows {
                top: top.min(other_top),
                bottom: bottom.max(other_bottom),
            },
        };
    }
}

/// What to do for one present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentPlan {
    /// The size of the frame in device pixels.
    pub frame_px: (u32, u32),
    /// The surface's buffers must be told this size (`ANativeWindow_setBuffersGeometry`) before the
    /// lock: it is not the one it holds.
    pub set_geometry: bool,
    /// The rows to hand to `ANativeWindow_lock` as the dirty region, or `None`: lock without one and
    /// write every row.
    pub dirty: Option<Rows>,
}

/// What the shell remembers between two presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentState {
    /// Everything that changed since the last frame posted to this surface: the damage of every render
    /// in between (one regeneration may render several times), not only the last one.
    pending: RowDamage,
    /// The size of the last frame rendered.
    frame_px: Option<(u32, u32)>,
    /// The size of the last frame posted to the CURRENT surface; `None` while nothing was.
    posted_px: Option<(u32, u32)>,
    /// What `ANativeWindow_setBuffersGeometry` was last told for the current surface.
    geometry_px: Option<(u32, u32)>,
}

impl Default for PresentState {
    fn default() -> Self {
        Self::new()
    }
}

impl PresentState {
    /// A shell that has rendered and posted nothing: the first frame is whole.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pending: RowDamage::Full,
            frame_px: None,
            posted_px: None,
            geometry_px: None,
        }
    }

    /// A frame of `frame_px` pixels was rendered, and `damage` of it changed since the render before.
    /// A frame of another size has no pixels in common with the one before: it is whole.
    pub fn note_render(&mut self, frame_px: (u32, u32), damage: RowDamage) {
        if self.frame_px != Some(frame_px) {
            self.pending = RowDamage::Full;
        } else {
            self.pending.merge(damage);
        }
        self.frame_px = Some(frame_px);
    }

    /// The surface was created, replaced or lost (`InitWindow`, `TerminateWindow`, a rotation that
    /// makes a new one): its buffers hold nothing of ours, and the geometry it was told is gone.
    pub fn surface_changed(&mut self) {
        self.pending = RowDamage::Full;
        self.posted_px = None;
        self.geometry_px = None;
    }

    /// The system wants the window's content again (`RedrawNeeded`: a resume, a compositor that lost
    /// its state), whatever changed.
    pub fn redraw_needed(&mut self) {
        self.pending = RowDamage::Full;
        self.posted_px = None;
    }

    /// Whether a frame has been rendered at all.
    #[must_use]
    pub const fn has_frame(&self) -> bool {
        self.frame_px.is_some()
    }

    /// What the next present has to do, or `None` when there is nothing to post: no frame yet, or no
    /// change since the one on the surface.
    #[must_use]
    pub fn plan(&self) -> Option<PresentPlan> {
        let frame_px = self.frame_px?;
        let on_surface = self.posted_px == Some(frame_px);
        if on_surface && self.pending == RowDamage::Nothing {
            return None;
        }
        let dirty = match self.pending {
            // The surface carries the rest of the last frame over only when it has one of this size.
            RowDamage::Rows { top, bottom } if on_surface => Some((top, bottom.min(frame_px.1))),
            _ => None,
        };
        // Rows below the frame are no rows: a damage that lies outside it is no damage to carry.
        let dirty = dirty.filter(|&(top, bottom)| top < bottom);
        Some(PresentPlan {
            frame_px,
            set_geometry: self.geometry_px != Some(frame_px),
            dirty,
        })
    }

    /// The frame of `plan` was written and posted (`unlockAndPost` returned).
    pub fn posted(&mut self, plan: &PresentPlan) {
        self.posted_px = Some(plan.frame_px);
        self.geometry_px = Some(plan.frame_px);
        self.pending = RowDamage::Nothing;
    }

    /// The geometry of `plan` could not be set: the surface has none of ours, and nothing was posted.
    pub fn geometry_failed(&mut self) {
        self.geometry_px = None;
    }
}

/// The rows to write after the lock answered: the region it returned (it may be larger than the one
/// asked for, and is the whole buffer when it could not carry the last frame over), or every row when
/// none was asked for. Clamped to the buffer's `buffer_height` rows.
#[must_use]
pub fn rows_to_write(plan: &PresentPlan, returned: Option<Rows>, buffer_height: u32) -> Rows {
    let (top, bottom) = match (plan.dirty, returned) {
        (Some(_), Some(rows)) => rows,
        _ => (0, buffer_height),
    };
    (top.min(buffer_height), bottom.min(buffer_height))
}

#[cfg(test)]
#[path = "present_test.rs"]
mod tests;
