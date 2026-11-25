//! The owner of a workspace's whiteboard: what the app holds and does with it. The page's pointer
//! reaches it through [`Whiteboard::arm_stroke`] (a pen lifted: a loop arms its region) and
//! `page_*` (the press, the hold, the motion and the release of the drag to the bar); the bar's own
//! pointer goes to [`BoardInput`] (`self.input`); a picture drawn on a Thread comes back through
//! [`Whiteboard::image_ready`]; the file is kept and read through [`Whiteboard::save_job`] /
//! [`Whiteboard::save_done`] and [`Whiteboard::loaded`]. The clock, the files and the page's lines are
//! plain values the caller passes in. No window, no azul: the app's UI (`whiteboard_ui`) is a thin skin on this.

use std::sync::Arc;

use crate::{
    whiteboard::{kept_lines, lines_hash, Board, CutoutDraft, CutoutImage, ImageSlot, ItemId, Limits, Notice, Source},
    whiteboard_cutout::{plan, CapturePlan},
    whiteboard_drag::{Armed, ArmedRegion, ArmedRegions, Bar, Drag, Motion, Pointer, Press, RegionId, Release, Tick},
    whiteboard_geom::{Pt, Rect, Size},
    whiteboard_input::BoardInput,
    whiteboard_lasso::{covered_lines, recognize, PageLines},
    whiteboard_store::{self as store, Entry, Loaded, SaveGate, StoreError},
};

/// The narrowest and the widest the bar opens (a share of the window), and the width it starts with, px.
pub const BAR_MIN_PX: f32 = 220.0;
pub const BAR_MAX_SHARE: f32 = 0.6;
pub const BAR_DEFAULT_PX: f32 = 340.0;
/// The width of the thin bar a closed whiteboard is, px.
pub const EDGE_PX: f32 = 12.0;

/// Whether the right bar is open and how wide: kept in the settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarState {
    pub open: bool,
    pub width: f32,
}

/// What the app knows of the file a cutout comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFacts {
    /// Relative to the workspace, with `/`.
    pub file: String,
    /// [`lines_hash`] of the whole file now.
    pub file_hash: u64,
    /// The lines the loop covers, as they are.
    pub covered: Vec<String>,
}

/// A cutout made, whose picture is to be drawn: the Thread's job (`whiteboard_render::RenderRequest` is
/// this plus the page's lines, its strokes and the window's fonts).
#[derive(Debug, Clone, PartialEq)]
pub struct CutoutJob {
    pub id: ItemId,
    pub plan: CapturePlan,
    pub page: usize,
}

/// How the board's file is.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// No file yet.
    New,
    Loaded { items: usize, saved_ms: u64 },
    /// Read, with something left out.
    Repaired { left_out: usize, pictures: usize },
    /// The file could not be read; `kept_as` is the name it was kept under.
    Unreadable { why: String, kept_as: Option<String> },
    /// A newer version wrote it: it is left as it is.
    TooNew { found: u32 },
    SaveFailed(String),
    Saved(u64),
}

/// A write the app does on the file thread: the name (among the app's files) and the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Effect {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A save built: the entries of the file, what revision of the board they are of.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveJob {
    pub name: String,
    pub entries: Vec<Entry>,
    pub revision: u64,
    pub view_revision: u64,
}

/// The whiteboard of one workspace and everything that goes with it.
pub struct Whiteboard {
    pub board: Board,
    /// The gestures on the bar.
    pub input: BoardInput,
    /// The loops waiting to be lifted, and the lift.
    pub armed: ArmedRegions,
    pub drag: Drag,
    pub bar: BarState,
    pub status: Status,
    workspace: String,
    gate: SaveGate,
    notices: Vec<Notice>,
    /// A file written by a newer version: never replaced.
    protect_file: bool,
    saving: Option<(u64, u64, u64)>,
}

impl Whiteboard {
    /// The whiteboard of the workspace at `workspace` (the folder's path), empty.
    #[must_use]
    pub fn new(workspace: &str) -> Self {
        Self::with_limits(workspace, crate::whiteboard::LIMITS)
    }

    #[must_use]
    pub fn with_limits(workspace: &str, limits: Limits) -> Self {
        Self {
            board: Board::with_limits(limits),
            input: BoardInput::default(),
            armed: ArmedRegions::default(),
            drag: Drag::default(),
            bar: BarState { open: false, width: BAR_DEFAULT_PX },
            status: Status::New,
            workspace: workspace.to_string(),
            gate: SaveGate::default(),
            notices: Vec::new(),
            protect_file: false,
            saving: None,
        }
    }

    #[must_use]
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// What the whiteboard has to tell the person since it was last asked.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        let mut told = std::mem::take(&mut self.notices);
        told.extend(self.board.take_notices());
        told
    }

    // ---- from the page

    /// A pen lifted on `page`: when its stroke is a loop round some lines, its region is armed (its id);
    /// anything else, or a loop over no line, arms nothing.
    pub fn arm_stroke(
        &mut self,
        page: usize,
        points: &[Pt],
        lines: &PageLines<'_>,
        page_size: Size,
        stroke_id: u64,
        now_ms: u64,
    ) -> Option<RegionId> {
        let lasso = recognize(points).ok()?;
        let range = covered_lines(&lasso.polygon, lines)?;
        Some(self.armed.arm(
            Armed { page, polygon: lasso.polygon, first_line: range.first, last_line: range.last, stroke_id },
            page_size,
            now_ms,
        ))
    }

    /// The armed region at the point `p` of `page` (what a pen resting there is about).
    #[must_use]
    pub fn region_at(&self, page: usize, p: Pt, now_ms: u64) -> Option<&ArmedRegion> {
        self.armed.armed_region_at(page, p, now_ms)
    }

    /// Lets go of the regions whose time is up; whether any went.
    pub fn expire(&mut self, now_ms: u64) -> bool {
        self.armed.expire(now_ms)
    }

    /// The pointer pressed on a page: what it is over.
    pub fn page_press(&mut self, at: Pointer, now_ms: u64) -> Press {
        self.drag.press(&self.armed, at, now_ms)
    }

    pub fn page_tick(&mut self, now_ms: u64) -> Option<Tick> {
        self.drag.tick(now_ms)
    }

    pub fn page_motion(&mut self, at_window: Pt, now_ms: u64, bar: &Bar) -> Motion {
        self.drag.motion(at_window, now_ms, bar)
    }

    pub fn page_release(&mut self, at_window: Pt, bar: &Bar) -> Release {
        self.drag.release(at_window, bar)
    }

    /// Escape: a drag in progress is dropped, else a gesture on the bar, else the armed regions go;
    /// whether there was anything to let go of.
    pub fn escape(&mut self) -> bool {
        if self.drag.cancel() {
            return true;
        }
        if self.input.is_busy() {
            self.input.cancel(&mut self.board);
            return true;
        }
        let had = !self.armed.is_empty();
        self.armed.clear();
        had
    }

    /// Region `region` is dropped on the bar at `at_bar` (from the bar's top-left corner): its cutout is
    /// put on the board, centred there, and waits for its picture ([`Self::image_ready`]). The region is
    /// used up; when the cutout is refused (the board is full) it stays armed.
    ///
    /// # Errors
    /// There is no such region, no picture can be made of it, or the board has no room.
    pub fn drop_region(
        &mut self,
        region: RegionId,
        at_bar: Pt,
        facts: SourceFacts,
        dpr: f32,
        page: Size,
    ) -> Result<CutoutJob, Notice> {
        let Some(r) = self.armed.get(region).cloned() else {
            return Err(Notice::NoSuchItem { id: region });
        };
        let limits = *self.board.limits();
        let made = plan(&r.polygon, dpr, page, &limits)?;
        let lines = kept_lines(&facts.covered, &limits);
        let source = Source {
            file: facts.file,
            page: u32::try_from(r.page).unwrap_or(0),
            first_line: r.first_line,
            last_line: r.last_line,
            shape: r.polygon.clone(),
            file_hash: facts.file_hash,
            range_hash: lines_hash(&lines),
            lines,
        };
        let centre = self.board.view().to_board(at_bar);
        let at = Pt::new(centre.x - made.logical.w * 0.5, centre.y - made.logical.h * 0.5);
        let id = self.board.add_cutout(CutoutDraft::pending(at, made.logical, source))?;
        self.armed.disarm(region);
        if let Some(n) = made.notice.clone() {
            self.notices.push(n);
        }
        self.input.select(Some(id));
        Ok(CutoutJob { id, plan: made, page: r.page })
    }

    /// The picture of cutout `id` was drawn (or could not be): it is put in. Whether the cutout is still there.
    pub fn image_ready(&mut self, id: ItemId, drawn: Result<CutoutImage, String>) -> bool {
        let slot = match drawn {
            Ok(img) if !img.is_consistent() => ImageSlot::Failed("the picture does not fit its size".to_string()),
            Ok(img) if self.board.picture_pixels() + img.pixels() > self.board.limits().max_total_px => {
                ImageSlot::Failed("there is no room for it among the pictures of the board".to_string())
            }
            Ok(img) => ImageSlot::Ready(Arc::new(img)),
            Err(why) => ImageSlot::Failed(why),
        };
        self.board.set_image(id, slot)
    }

    // ---- the bar

    /// Opens or closes the bar and sets its width (kept inside what a bar may be in a window `window_w` wide).
    pub fn set_bar(&mut self, open: bool, width: f32, window_w: f32) {
        let widest = (window_w * BAR_MAX_SHARE).max(BAR_MIN_PX);
        let wanted = if width.is_finite() { width } else { BAR_DEFAULT_PX };
        self.bar = BarState { open, width: wanted.clamp(BAR_MIN_PX, widest) };
    }

    /// The bar as the window has it: the thin edge while it is closed, its width when it is open, at the
    /// right of `viewport` (the window's area under its title row).
    #[must_use]
    pub fn bar_geometry(&self, viewport: Rect) -> Bar {
        let w = if self.bar.open { self.bar.width } else { EDGE_PX };
        Bar {
            rect: Rect::new(viewport.right() - w, viewport.y, w, viewport.h),
            open: self.bar.open,
            window_w: viewport.right(),
        }
    }

    /// The width as the settings keep it.
    #[must_use]
    pub fn width_setting(width: f32) -> String {
        format!("{}", width.round() as i64)
    }

    /// The width a setting holds (`None` for what is no width).
    #[must_use]
    pub fn parse_width(text: &str) -> Option<f32> {
        text.trim().parse::<f32>().ok().filter(|w| w.is_finite() && *w > 0.0)
    }

    // ---- the file

    /// The save to write, when the board has changed since it was kept and none is being written (a save
    /// wanted while one runs is asked again when it is done: [`Self::save_done`]).
    pub fn save_job(&mut self, now_ms: u64) -> Option<SaveJob> {
        if self.protect_file || !self.board.needs_save() || !self.gate.request() {
            return None;
        }
        let revision = self.board.content_revision();
        let view_revision = self.board.view_revision();
        self.saving = Some((revision, view_revision, now_ms));
        Some(SaveJob {
            name: store::board_name(&self.workspace),
            entries: store::write(&self.board.snapshot(), &self.workspace, now_ms),
            revision,
            view_revision,
        })
    }

    /// The write of the save is over. `true` when another was wanted meanwhile and is worth building
    /// ([`Self::save_job`]).
    pub fn save_done(&mut self, result: &Result<(), String>) -> bool {
        let again = self.gate.finished();
        match (result, self.saving.take()) {
            (Ok(()), Some((revision, view, at))) => {
                self.board.mark_saved_at(revision, view);
                self.status = Status::Saved(at);
            }
            (Err(why), _) => self.status = Status::SaveFailed(why.clone()),
            (Ok(()), None) => {}
        }
        again
    }

    /// There was no file for this workspace.
    pub fn loaded_none(&mut self) -> Vec<Effect> {
        self.status = Status::New;
        Vec::new()
    }

    /// The file was read (`raw`: its bytes, for keeping a copy of one that cannot be used). What it holds
    /// is the board now; a file that is no board of this version is kept aside (the effects to write)
    /// before anything replaces it, and a file of a newer version is never replaced.
    pub fn loaded(&mut self, result: Result<Loaded, StoreError>, raw: Option<Vec<u8>>, now_ms: u64) -> Vec<Effect> {
        let aside = |raw: Option<Vec<u8>>| -> (Option<String>, Vec<Effect>) {
            let name = store::quarantine_name(&store::board_name(&self.workspace), now_ms);
            match raw {
                Some(bytes) => (Some(name.clone()), vec![Effect { name, bytes }]),
                None => (None, Vec::new()),
            }
        };
        match result {
            Ok(l) => {
                let (items_left_out, pictures) = (l.left_out, l.unreadable_pictures);
                let notices = self.board.replace_with(l.data);
                self.status = if items_left_out > 0 || pictures > 0 || !notices.is_empty() {
                    Status::Repaired { left_out: items_left_out, pictures }
                } else {
                    Status::Loaded { items: self.board.len(), saved_ms: l.saved_ms }
                };
                self.notices.extend(notices);
                if items_left_out > 0 {
                    // What this version could not use is not thrown away by the next save without a copy.
                    return aside(raw).1;
                }
                Vec::new()
            }
            Err(StoreError::TooNew { found }) => {
                self.status = Status::TooNew { found };
                self.protect_file = true;
                Vec::new()
            }
            Err(e) => {
                let (kept_as, effects) = aside(raw);
                self.status = Status::Unreadable { why: e.why().map_or_else(|| e.english(), str::to_string), kept_as };
                effects
            }
        }
    }
}

#[cfg(test)]
#[path = "whiteboard_app_test.rs"]
mod whiteboard_app_test;
