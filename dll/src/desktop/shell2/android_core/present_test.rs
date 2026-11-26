//! Presenting by rows against a model of the surface `ANativeWindow_lock` talks to. No device, no NDK.
//!
//! The model keeps what the NDK header documents: a pool of buffers handed out in turn, each holding an
//! OLD frame; a lock with a dirty region carries everything outside that region over from the buffer
//! posted last when the sizes match, and answers with the region to redraw (which it may enlarge); a
//! lock without one answers with the whole buffer and carries nothing over. Every test ends in the
//! one thing a user sees: the posted buffer equals the frame the engine rendered, row by row.

use super::*;

/// A frame is one tag per row: what was drawn there, and in which render.
type Frame = Vec<u32>;

/// The surface behind `ANativeWindow`: `pool` buffers handed out in turn.
struct FakeSurface {
    pool_len: usize,
    buffers: Vec<Frame>,
    size: (u32, u32),
    next: usize,
    front: Option<usize>,
    locked: Option<usize>,
    /// The system returns more than the region it was asked for ("this region is often extended").
    extend: bool,
}

impl FakeSurface {
    fn new(pool_len: usize, size: (u32, u32), extend: bool) -> Self {
        Self {
            pool_len,
            buffers: vec![vec![0; size.1 as usize]; pool_len],
            size,
            next: 0,
            front: None,
            locked: None,
            extend,
        }
    }

    /// `ANativeWindow_setBuffersGeometry`: another size means other buffers, and no front.
    fn set_geometry(&mut self, size: (u32, u32)) {
        if self.size != size {
            self.size = size;
            self.buffers = vec![vec![0; size.1 as usize]; self.pool_len];
            self.front = None;
        }
    }

    /// A new surface of the same window: the pool is new and holds nothing of ours.
    fn replace(&mut self) {
        self.buffers = vec![vec![0xDEAD; self.size.1 as usize]; self.pool_len];
        self.front = None;
        self.next = 0;
    }

    /// `ANativeWindow_lock`: the rows the caller has to redraw.
    fn lock(&mut self, dirty: Option<Rows>) -> Option<Rows> {
        let height = self.size.1;
        let back = self.next;
        self.next = (self.next + 1) % self.pool_len;
        self.locked = Some(back);
        match (dirty, self.front) {
            (Some((top, bottom)), Some(front)) => {
                let (top, bottom) = if self.extend {
                    (top.saturating_sub(5), (bottom + 9).min(height))
                } else {
                    (top, bottom)
                };
                if front != back {
                    for y in 0..height {
                        if y < top || y >= bottom {
                            self.buffers[back][y as usize] = self.buffers[front][y as usize];
                        }
                    }
                }
                Some((top, bottom))
            }
            // No region asked for, or nothing to carry over: the whole buffer is to be redrawn.
            _ => None,
        }
    }

    fn write(&mut self, rows: Rows, frame: &Frame) {
        let back = self.locked.expect("a locked buffer");
        for y in rows.0..rows.1 {
            self.buffers[back][y as usize] = frame[y as usize];
        }
    }

    /// `ANativeWindow_unlockAndPost`.
    fn post(&mut self) {
        self.front = self.locked.take();
    }

    fn visible(&self) -> Option<&Frame> {
        self.front.map(|front| &self.buffers[front])
    }
}

/// The shell: an engine that renders frames, the state this module owns and the surface.
struct Shell {
    frame: Frame,
    size: (u32, u32),
    state: PresentState,
    surface: FakeSurface,
    tag: u32,
}

impl Shell {
    fn new(pool_len: usize, size: (u32, u32), extend: bool) -> Self {
        Self {
            frame: vec![0; size.1 as usize],
            size,
            state: PresentState::new(),
            surface: FakeSurface::new(pool_len, size, extend),
            tag: 0,
        }
    }

    /// One render of the engine: `rows` changed (`None`: all of it).
    fn render(&mut self, rows: Option<Rows>) {
        self.tag += 1;
        let (damage, range) = match rows {
            Some((top, bottom)) => (
                RowDamage::from_rects(&[(0, top, self.size.0, bottom - top)]),
                top..bottom,
            ),
            None => (RowDamage::Full, 0..self.size.1),
        };
        for y in range {
            self.frame[y as usize] = self.tag;
        }
        self.state.note_render(self.size, damage);
    }

    /// One render at another size: the frame is new.
    fn resize(&mut self, size: (u32, u32)) {
        self.size = size;
        self.tag += 1;
        self.frame = vec![self.tag; size.1 as usize];
        self.state.note_render(size, RowDamage::Full);
    }

    /// One present, as `render_frame` does it. Returns whether anything was posted.
    fn present(&mut self) -> bool {
        let Some(plan) = self.state.plan() else {
            return false;
        };
        if plan.set_geometry {
            self.surface.set_geometry(plan.frame_px);
        }
        let returned = self.surface.lock(plan.dirty);
        let rows = rows_to_write(&plan, returned, plan.frame_px.1);
        self.surface.write(rows, &self.frame);
        self.surface.post();
        self.state.posted(&plan);
        true
    }

    fn assert_on_screen(&self, what: &str) {
        assert_eq!(
            self.surface.visible(),
            Some(&self.frame),
            "{what}: what the user sees is not the frame the engine rendered"
        );
    }
}

const SIZE: (u32, u32) = (1080, 2400);

/// The pools a device may have, with and without a system that enlarges the region.
fn every_surface() -> Vec<(usize, bool)> {
    let mut all = Vec::new();
    for pool in 1..=4 {
        for extend in [false, true] {
            all.push((pool, extend));
        }
    }
    all
}

/// The newest frame's own rows are on screen. The shell wrote the rows changed in the frames BETWEEN
/// a buffer's last use and this one, never the rows of the frame being presented: with one buffer
/// nothing was copied at all, with a pool the picture lagged by the newest frame. The overlay
/// scrollbar that fades out in the last frame of a burst stayed on screen.
///
/// Mutation: a plan that leaves the pending damage out of the dirty region; a dirty region that
/// is the damage of the frame before.
#[test]
fn the_rows_of_the_newest_frame_are_on_screen() {
    for (pool, extend) in every_surface() {
        let mut shell = Shell::new(pool, SIZE, extend);
        shell.render(None);
        shell.present();
        shell.assert_on_screen("the first frame");
        for step in 0..10u32 {
            let top = 100 + step * 97;
            shell.render(Some((top, top + 40)));
            shell.present();
            shell.assert_on_screen(&format!("pool {pool}, extend {extend}, frame {step}"));
        }
    }
}

/// A whole frame in the middle is a change for the buffers it was not written into: the frame
/// after it on another buffer must not take the buffer for current.
///
/// Mutation: a whole frame recorded as no damage (the shell recorded `Vec::new()` for it).
#[test]
fn a_whole_frame_in_the_middle_does_not_hide_the_change_from_the_other_buffers() {
    for (pool, extend) in every_surface() {
        let mut shell = Shell::new(pool, SIZE, extend);
        shell.render(None);
        shell.present();
        shell.render(Some((10, 50)));
        shell.present();
        shell.render(None);
        shell.present();
        shell.assert_on_screen("the whole frame");
        for step in 0..6u32 {
            shell.render(Some((300 + step * 10, 320 + step * 10)));
            shell.present();
            shell.assert_on_screen(&format!("pool {pool}, extend {extend}, step {step}"));
        }
    }
}

/// `regenerate_layout` renders several times (its bounded lifecycle loop) before the shell presents:
/// every render's rows are in the present, not only the last one's.
///
/// Mutation: a state that keeps the damage of the last render only.
#[test]
fn every_render_between_two_presents_reaches_the_screen() {
    for (pool, extend) in every_surface() {
        let mut shell = Shell::new(pool, SIZE, extend);
        shell.render(None);
        shell.present();
        for step in 0..5u32 {
            shell.render(Some((50 + step * 300, 80 + step * 300)));
            shell.render(Some((1900, 1950)));
            shell.render(Some((700 - step * 40, 720 - step * 40)));
            shell.present();
            shell.assert_on_screen(&format!("pool {pool}, extend {extend}, step {step}"));
        }
    }
}

/// A new surface (home and back, a rotation, split screen) has buffers that hold nothing of ours,
/// and may map them where the old ones were: the first frame on it is whole.
///
/// Mutation: a state that is not reset when the surface is replaced (the shell's was not).
#[test]
fn a_new_surface_is_written_in_full() {
    for (pool, extend) in every_surface() {
        let mut shell = Shell::new(pool, SIZE, extend);
        shell.render(None);
        shell.present();
        for step in 0..4u32 {
            shell.render(Some((200 * step, 200 * step + 50)));
            shell.present();
        }
        shell.surface.replace();
        shell.state.surface_changed();
        shell.render(Some((900, 940)));
        shell.present();
        shell.assert_on_screen(&format!("pool {pool}, extend {extend}, the new surface"));
        shell.render(Some((1000, 1040)));
        shell.present();
        shell.assert_on_screen(&format!("pool {pool}, extend {extend}, after it"));
    }
}

/// `RedrawNeeded` asks for the content again: it is posted whole, from the frame the shell holds, with
/// nothing rendered in between.
///
/// Mutation: a redraw that posts nothing because no row changed.
#[test]
fn a_redraw_the_system_asks_for_posts_the_whole_frame() {
    let mut shell = Shell::new(2, SIZE, false);
    shell.render(None);
    shell.present();
    shell.render(Some((10, 20)));
    shell.present();
    // the buffers lost their content behind the shell's back
    shell.surface.buffers = vec![vec![0xBEEF; SIZE.1 as usize]; 2];
    shell.surface.front = None;
    shell.state.redraw_needed();
    assert!(shell.present());
    shell.assert_on_screen("the redraw");
}

/// A frame of another size has nothing in common with the one before, and the buffers are told the
/// new size once.
///
/// Mutation: a resize treated as a change of the rows that moved.
#[test]
fn a_frame_of_another_size_is_written_in_full() {
    for (pool, extend) in every_surface() {
        let mut shell = Shell::new(pool, SIZE, extend);
        shell.render(None);
        shell.present();
        shell.render(Some((100, 200)));
        shell.present();
        shell.resize((2400, 1080));
        let plan = shell.state.plan().expect("a resized frame is posted");
        assert!(plan.set_geometry, "the buffers are told the new size");
        assert_eq!(plan.dirty, None, "and it is written whole");
        shell.present();
        shell.assert_on_screen("the rotated frame");
        shell.render(Some((500, 540)));
        let plan = shell.state.plan().expect("the next frame");
        assert!(!plan.set_geometry, "the size is told once, not every frame");
        shell.present();
        shell.assert_on_screen("the frame after the rotation");
    }
}

/// Nothing changed since the frame on the surface: nothing is posted (a post makes SurfaceFlinger
/// compose the window again), and nothing is locked with an empty region.
///
/// Mutation: a frame with no damage that is presented anyway.
#[test]
fn a_frame_that_changed_nothing_posts_nothing() {
    let mut shell = Shell::new(3, SIZE, false);
    assert!(shell.state.plan().is_none(), "no frame yet");
    shell.render(None);
    assert!(shell.present());
    shell.state.note_render(SIZE, RowDamage::Nothing);
    assert!(shell.state.plan().is_none());
    assert!(!shell.present());
    shell.assert_on_screen("the frame that stayed");
}

/// The first frame, and a frame with a damage the shell could not name, are locked without a region.
#[test]
fn the_first_frame_and_a_whole_frame_are_locked_without_a_region() {
    let mut state = PresentState::new();
    state.note_render(SIZE, RowDamage::Full);
    let plan = state.plan().expect("the first frame");
    assert_eq!(plan.dirty, None);
    assert!(plan.set_geometry);
    state.posted(&plan);
    state.note_render(SIZE, RowDamage::Rows { top: 5, bottom: 9 });
    assert_eq!(state.plan().expect("a partial frame").dirty, Some((5, 9)));
    state.note_render(SIZE, RowDamage::Full);
    assert_eq!(state.plan().expect("a whole frame").dirty, None);
}

/// A lock that failed leaves the damage where it was: the next present still owes it.
#[test]
fn a_present_that_did_not_happen_keeps_its_damage() {
    let mut state = PresentState::new();
    state.note_render(SIZE, RowDamage::Full);
    let plan = state.plan().unwrap();
    state.posted(&plan);
    state.note_render(SIZE, RowDamage::Rows { top: 100, bottom: 120 });
    // the lock failed: `posted` is not called
    assert!(state.plan().is_some());
    state.note_render(SIZE, RowDamage::Rows { top: 400, bottom: 420 });
    assert_eq!(
        state.plan().unwrap().dirty,
        Some((100, 420)),
        "the rows of both renders are owed"
    );
}

/// A geometry the surface refused is asked for again.
#[test]
fn a_geometry_the_surface_refused_is_asked_for_again() {
    let mut state = PresentState::new();
    state.note_render(SIZE, RowDamage::Full);
    let plan = state.plan().unwrap();
    assert!(plan.set_geometry);
    state.geometry_failed();
    assert!(state.plan().unwrap().set_geometry);
}

#[test]
fn damage_of_rects_is_the_band_from_the_top_one_to_the_lowest() {
    assert_eq!(RowDamage::from_rects(&[]), RowDamage::Nothing);
    assert_eq!(
        RowDamage::from_rects(&[(0, 100, 50, 10), (0, 30, 5, 5), (900, 400, 1, 1)]),
        RowDamage::Rows { top: 30, bottom: 401 }
    );
    // a rect with no area is no change
    assert_eq!(RowDamage::from_rects(&[(0, 100, 0, 10)]), RowDamage::Nothing);
    assert_eq!(RowDamage::from_rects(&[(0, 100, 10, 0)]), RowDamage::Nothing);
    // a rect that reaches past the u32 range does not wrap
    assert_eq!(
        RowDamage::from_rects(&[(0, u32::MAX - 2, 1, 10)]),
        RowDamage::Rows { top: u32::MAX - 2, bottom: u32::MAX }
    );
}

#[test]
fn merging_is_a_union_and_whole_absorbs_everything() {
    let mut d = RowDamage::Nothing;
    d.merge(RowDamage::Rows { top: 10, bottom: 20 });
    d.merge(RowDamage::Nothing);
    assert_eq!(d, RowDamage::Rows { top: 10, bottom: 20 });
    d.merge(RowDamage::Rows { top: 5, bottom: 12 });
    assert_eq!(d, RowDamage::Rows { top: 5, bottom: 20 });
    d.merge(RowDamage::Full);
    assert_eq!(d, RowDamage::Full);
    d.merge(RowDamage::Rows { top: 1, bottom: 2 });
    assert_eq!(d, RowDamage::Full);
}

/// The rows after the lock: the region it answered with, clamped to the buffer; every row when no region
/// was asked for or the system answered with none.
#[test]
fn the_rows_written_are_the_ones_the_system_asked_for() {
    let with_region = PresentPlan { frame_px: SIZE, set_geometry: false, dirty: Some((100, 200)) };
    assert_eq!(rows_to_write(&with_region, Some((90, 230)), 2400), (90, 230));
    assert_eq!(rows_to_write(&with_region, Some((0, 2400)), 2400), (0, 2400));
    // a system that answers past the buffer is clamped, not trusted
    assert_eq!(rows_to_write(&with_region, Some((2390, 9999)), 2400), (2390, 2400));
    // a system that carried nothing over answers with no region: everything
    assert_eq!(rows_to_write(&with_region, None, 2400), (0, 2400));
    let whole = PresentPlan { frame_px: SIZE, set_geometry: false, dirty: None };
    assert_eq!(rows_to_write(&whole, None, 2400), (0, 2400));
    // a buffer shorter than the frame
    assert_eq!(rows_to_write(&whole, None, 2000), (0, 2000));
}

/// A damage that lies below the frame is no damage to carry: the plan does not ask for an empty region.
#[test]
fn a_damage_below_the_frame_is_not_a_region() {
    let mut state = PresentState::new();
    state.note_render(SIZE, RowDamage::Full);
    let plan = state.plan().unwrap();
    state.posted(&plan);
    state.note_render(SIZE, RowDamage::Rows { top: 2500, bottom: 2600 });
    let plan = state.plan().expect("something is pending");
    assert_eq!(plan.dirty, None);
}

/// Long random runs: renders of every kind, presents, surfaces that are replaced, resizes; after every
/// present what is on screen is the frame the engine rendered. A fixed generator: a failure replays.
#[test]
fn a_long_random_run_never_leaves_a_stale_row_on_screen() {
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    for (pool, extend) in every_surface() {
        let mut shell = Shell::new(pool, (1080, 2400), extend);
        shell.render(None);
        shell.present();
        for step in 0..2000 {
            match next() % 20 {
                // most renders are partial, some whole, some change nothing
                0..=11 => {
                    let top = (next() % u64::from(shell.size.1 - 100)) as u32;
                    let height = 1 + (next() % 100) as u32;
                    shell.render(Some((top, (top + height).min(shell.size.1))));
                }
                12 => shell.render(None),
                13 => shell.state.note_render(shell.size, RowDamage::Nothing),
                14 => {
                    shell.surface.replace();
                    shell.state.surface_changed();
                    shell.render(None);
                }
                15 => {
                    let size = if shell.size.1 == 2400 { (1080, 2200) } else { (1080, 2400) };
                    shell.resize(size);
                }
                16 => shell.state.redraw_needed(),
                _ => {}
            }
            // one to three presents in a row; only the first can have something to post
            for _ in 0..1 + next() % 3 {
                shell.present();
            }
            if shell.surface.visible().is_some() {
                shell.assert_on_screen(&format!("pool {pool}, extend {extend}, step {step}"));
            }
        }
    }
}
