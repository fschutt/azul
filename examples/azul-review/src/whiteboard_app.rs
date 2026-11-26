//! The owner of a workspace's whiteboard: what the app holds and does with it. The page's pointer
//! reaches it through [`Whiteboard::arm_stroke`] (a pen lifted: a loop arms its region) and
//! `page_*` (the press, the hold, the motion and the release of the drag to the bar); the bar's own
//! pointer goes to [`BoardInput`] (`self.input`); a picture drawn on a Thread comes back through
//! [`Whiteboard::image_ready`]; the file is kept and read through [`Whiteboard::save_job`] /
//! [`Whiteboard::save_done`] and [`Whiteboard::loaded`]. The clock, the files and the page's lines are
//! plain values the caller passes in. No window, no azul: the app's UI (`whiteboard_ui`) is a thin skin on this.

use std::sync::Arc;

use crate::{
    whiteboard::{kept_lines, lines_hash, Board, BoardData, CutoutDraft, CutoutImage, ImageSlot, Item, ItemId, Limits, Notice, Source},
    whiteboard_azboard::{self as azboard, Meta},
    whiteboard_boards::{self as boards, BoardIndex, BoardsError},
    whiteboard_cutout::{plan, CapturePlan},
    whiteboard_drag::{Armed, ArmedRegion, ArmedRegions, Bar, Drag, Motion, Pointer, Press, RegionId, Release, Tick},
    whiteboard_export::{self as export, ExportPictures, ExportRequest, ExportScope},
    whiteboard_geom::{Pt, Rect, Size},
    whiteboard_git::RepoFacts,
    whiteboard_hash,
    whiteboard_history::{self as history, Code, History, RefChange, Snapshot, Summary},
    whiteboard_input::{BoardInput, Mods, Out},
    whiteboard_jump::FileView,
    whiteboard_relocate as relocate,
    whiteboard_lasso::{covered_lines, recognize, PageLines},
    whiteboard_settings::Settings,
    whiteboard_store::{self as store, Entry, Loaded, SaveGate, StoreError},
};

/// The narrowest and the widest the bar opens (a share of the window), and the width it starts with, px.
pub const BAR_MIN_PX: f32 = 220.0;
pub const BAR_MAX_SHARE: f32 = 0.6;
pub const BAR_DEFAULT_PX: f32 = 340.0;
/// The width of the thin bar a closed whiteboard is, px.
pub const EDGE_PX: f32 = 12.0;
/// The height of the header of the bar, px: the board's name and the list of boards. The board lies under it, so a
/// point of the bar is a point of the board that much higher.
pub const HEADER_PX: f32 = 30.0;

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

/// A file to write that is not the open board's own save: the copy a duplicate made.
#[derive(Debug, Clone, PartialEq)]
pub struct Write {
    /// Among the app's files.
    pub name: String,
    pub entries: Vec<Entry>,
}

/// What the host does after a change of the list of boards.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    /// Write these, each atomically (the list of boards: a few hundred bytes).
    pub effects: Vec<Effect>,
    /// The file of a copy to make: its zip is made of the entries and written like a save, but it is not the open
    /// board's save (`save_done` is not told of it).
    pub copy: Option<Write>,
    /// Read this board's file (among the app's files) and hand what it holds to [`Whiteboard::loaded`] (or
    /// [`Whiteboard::loaded_none`] when there is none).
    pub load: Option<String>,
    /// Ask where to write the board as a JSON file to share, and write it there on a Thread ([`export::export`]).
    pub export: Option<ExportRequest>,
    /// Ask for a file of a board to open, read it on a Thread ([`export::import`]) and hand it to
    /// `Panel::import_loaded`.
    pub import: bool,
}

/// What the host does when the list of boards has been read at the start.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexStart {
    /// Write these (the list started anew or repaired, and a copy of a list that could not be used).
    pub effects: Vec<Effect>,
    /// The file of the board that is open: read it.
    pub load: String,
    /// What was wrong with the list, to tell the person.
    pub told: Option<BoardsError>,
    /// The name a list that could not be used was kept under.
    pub kept_as: Option<String>,
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
    /// The list of boards was written by a newer version: it is never replaced, so no board is kept.
    index_protected: bool,
    saving: Option<(u64, u64, u64, u64)>,
    /// The boards of the workspace, and which is open.
    boards: BoardIndex,
    /// The board that is open is being read: what shows is empty until [`Self::loaded`].
    loading: bool,
    /// The list of boards changed (a board was kept) and was not given to the host yet ([`Self::take_index_effect`]).
    index_dirty: bool,
    /// What the repository of the workspace says (read from the files of `.git` by the host): the revision a new cutout
    /// is circled at, and where a document says its code lies.
    repo: Option<RepoFacts>,
    /// The settings of the whiteboard: the author a document names, the template of a link to code.
    settings: Settings,
    /// The workspace's folder on disk, when `workspace` (what names the files of its boards) is not itself the folder's path.
    folder: Option<std::path::PathBuf>,
    /// The snapshots of the open board, and whether they are all kept ([`Self::needs_save`]).
    history: History,
    history_revision: u64,
    saved_history_revision: u64,
    /// A snapshot being looked at: read-only, in place of the board.
    viewing: Option<Viewing>,
    /// What looking for the code again found since the last snapshot: the references that moved, changed or vanished, by
    /// cutout, with where they were first (the board flags them; the next snapshot records them).
    relocated: std::collections::BTreeMap<ItemId, RefChange>,
}

/// A snapshot that is shown in place of the board: its own board, which nothing the person does changes the history by.
pub struct Viewing {
    pub id: String,
    /// The snapshot's state as a board (its view can be panned and zoomed; nothing else of it changes).
    pub board: Board,
    /// What the snapshot changed since its parent, shown on the board while this is `Some`.
    pub compare: Option<Compare>,
}

/// A snapshot set against its parent: what [`Summary`] says, and what is needed to draw it - the items that were taken off
/// (they are not in the snapshot, so the parent's are kept to be drawn as ghosts).
#[derive(Debug, Clone, PartialEq)]
pub struct Compare {
    pub summary: Summary,
    pub removed: Vec<Item>,
    /// The parent's id, when there is one.
    pub against: Option<String>,
}

/// What the sheet that takes a snapshot shows before it is taken.
#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// The snapshot it will follow.
    pub parent: Option<String>,
    /// What changed since it.
    pub summary: Summary,
    /// The code as it is now.
    pub code: Code,
    /// Pictures still being drawn: a snapshot waits for them.
    pub pending: usize,
    /// The references that were looked for again and are not as they were.
    pub references: Vec<RefChange>,
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
            index_protected: false,
            saving: None,
            // Until the list has been read ([`Self::index_loaded`]) the workspace has the one board of the one-board
            // version, in its file.
            boards: BoardIndex::migrated(workspace, "Board 1", 0),
            loading: false,
            index_dirty: false,
            repo: None,
            settings: Settings::default(),
            folder: None,
            history: History::default(),
            history_revision: 0,
            saved_history_revision: 0,
            viewing: None,
            relocated: std::collections::BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    /// The workspace's folder on disk, for a host whose `workspace` is only a name (the repository is looked at there).
    pub fn set_folder(&mut self, folder: Option<std::path::PathBuf>) {
        self.folder = folder;
    }

    /// The workspace's folder: what [`Self::set_folder`] said, else the workspace itself (the folder's path).
    #[must_use]
    pub fn folder(&self) -> std::path::PathBuf {
        self.folder.clone().unwrap_or_else(|| std::path::PathBuf::from(&self.workspace))
    }

    /// What the repository says, as the host last read it (`None`: no repository, or not read yet).
    #[must_use]
    pub const fn repo(&self) -> Option<&RepoFacts> {
        self.repo.as_ref()
    }

    /// The host has read the repository's facts: a cutout made from now on names its revision.
    pub fn set_repo(&mut self, facts: Option<RepoFacts>) {
        self.repo = facts;
    }

    #[must_use]
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = settings;
    }

    /// The name a document is written with: the setting, else the name git is configured with; `None` for none.
    #[must_use]
    pub fn author_name(&self) -> Option<String> {
        self.settings.author_name(self.repo.as_ref().and_then(|r| r.author.as_deref()))
    }

    /// What a document of the open board says besides its items (`now_ms`: when it is kept, or exported).
    fn meta(&self, now_ms: u64) -> Meta {
        let b = self.boards.active();
        Meta {
            board_id: b.id.clone(),
            board_name: b.name.clone(),
            created_ms: b.created_ms,
            modified_ms: now_ms,
            workspace_name: store::folder_name(&self.folder().to_string_lossy()),
            repo: self.repo.clone(),
            author: self.author_name(),
            template: self.settings.permalink_template.clone(),
        }
    }

    /// The export of the open board as it is now (its unkept changes too): what the Thread that writes the file is
    /// given. `folder` is where the host's workspace is, `home` where the global git config is.
    #[must_use]
    pub fn export_request(&self, pictures: ExportPictures, scope: ExportScope, home: Option<&str>, now_ms: u64) -> ExportRequest {
        ExportRequest {
            data: self.board.snapshot(),
            history: self.history.clone(),
            scope,
            meta: self.meta(now_ms),
            pictures,
            folder: self.folder(),
            home: home.map(Into::into),
            remote: None,
        }
    }

    /// The name the open board's file is proposed under.
    #[must_use]
    pub fn export_file_name(&self) -> String {
        export::suggested_name(&self.boards.active().name)
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

    /// Region `region` is dropped on the bar at `at_bar` (from the bar's top-left corner, the header [`HEADER_PX`] included):
    /// its cutout is put on the board, centred there, and waits for its picture ([`Self::image_ready`]). The region is
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
        if self.loading {
            return Err(Notice::BoardBusy);
        }
        let Some(r) = self.armed.get(region).cloned() else {
            return Err(Notice::NoSuchItem { id: region });
        };
        let limits = *self.board.limits();
        let made = plan(&r.polygon, dpr, page, &limits)?;
        let lines = kept_lines(&facts.covered, &limits);
        let source = Source {
            rev: self.repo.as_ref().and_then(|r| r.rev.clone()).unwrap_or_default(),
            sha256: whiteboard_hash::lines_tagged(&facts.covered),
            file: facts.file,
            page: u32::try_from(r.page).unwrap_or(0),
            first_line: r.first_line,
            last_line: r.last_line,
            shape: r.polygon.clone(),
            file_hash: facts.file_hash,
            range_hash: lines_hash(&lines),
            lines,
        };
        // A drop on the header lands at the top of the board, the whole cutout in view.
        let view = self.board.view();
        let y = (at_bar.y - HEADER_PX).max(made.logical.h * 0.5 * view.zoom);
        let centre = view.to_board(Pt::new(at_bar.x, y));
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

    // ---- the boards

    /// The boards of the workspace.
    #[must_use]
    pub const fn boards(&self) -> &BoardIndex {
        &self.boards
    }

    /// The board that is open: its id and its name (what the app's companion shows).
    #[must_use]
    pub fn active_board(&self) -> (&str, &str) {
        let b = self.boards.active();
        (&b.id, &b.name)
    }

    /// The name of the open board's file among the app's files.
    #[must_use]
    pub fn board_file(&self) -> String {
        self.boards.active().file.clone()
    }

    /// The name of the list of boards among the app's files.
    #[must_use]
    pub fn index_name(&self) -> String {
        boards::index_name(&self.workspace)
    }

    /// Whether the board that is open is being read (the bar shows it empty and takes no pointer until it is in).
    #[must_use]
    pub const fn is_loading(&self) -> bool {
        self.loading
    }

    /// The list of boards was read at the start (`raw`: the bytes of its file, `None` when there is none;
    /// `legacy_file`: whether the one-board version's file is there). What it holds is the list now; a list that
    /// is none is kept aside and a new one started, one of a newer version is never replaced, one with entries that
    /// cannot be used is repaired with a copy of what it was. Then the board that is open is to be read.
    pub fn index_loaded(&mut self, raw: Option<Vec<u8>>, legacy_file: bool, first_name: &str, now_ms: u64) -> IndexStart {
        let started = |ws: &str| if legacy_file { BoardIndex::migrated(ws, first_name, now_ms) } else { BoardIndex::fresh(ws, first_name, now_ms) };
        let aside = |bytes: Vec<u8>, ws: &str| {
            let name = store::quarantine_name(&boards::index_name(ws), now_ms);
            (name.clone(), Effect { name, bytes })
        };
        let ws = self.workspace.clone();
        let (mut effects, mut told, mut kept_as) = (Vec::new(), None, None);
        match raw {
            None => {
                self.boards = started(&ws);
                effects.push(self.index_effect());
            }
            Some(bytes) => {
                let parsed = std::str::from_utf8(&bytes)
                    .map_err(|_| BoardsError::NotAnIndex("it is not text".to_string()))
                    .and_then(|text| BoardIndex::from_json(text, &ws));
                match parsed {
                    Ok((index, 0)) => self.boards = index,
                    Ok((index, _left_out)) => {
                        let (name, copy) = aside(bytes, &ws);
                        self.boards = index;
                        kept_as = Some(name);
                        effects.push(copy);
                        effects.push(self.index_effect());
                    }
                    Err(BoardsError::TooNew { found }) => {
                        self.boards = started(&ws);
                        self.index_protected = true;
                        self.protect_file = true;
                        told = Some(BoardsError::TooNew { found });
                    }
                    Err(e) => {
                        let (name, copy) = aside(bytes, &ws);
                        self.boards = started(&ws);
                        kept_as = Some(name);
                        told = Some(e);
                        effects.push(copy);
                        effects.push(self.index_effect());
                    }
                }
            }
        }
        self.index_dirty = false;
        IndexStart { effects, load: self.boards.active().file.clone(), told, kept_as }
    }

    fn index_effect(&self) -> Effect {
        Effect { name: boards::index_name(&self.workspace), bytes: self.boards.to_json().into_bytes() }
    }

    /// The list of boards, to be written, if it changed since the host last had it (a board was kept: its time is new).
    pub fn take_index_effect(&mut self) -> Option<Effect> {
        if !self.index_dirty || self.index_protected {
            return None;
        }
        self.index_dirty = false;
        Some(self.index_effect())
    }

    /// Whether the whiteboard may be left for another board now: the open board is kept, no write runs and no board is
    /// being read.
    ///
    /// # Errors
    /// [`Notice::BoardBusy`]: ask for a save, and again when it is done.
    pub fn can_leave(&self) -> Result<(), Notice> {
        self.leave()
    }

    /// What the whiteboard is left for another board only when nothing is lost: the open board is kept, no write
    /// runs and no board is being read.
    fn leave(&self) -> Result<(), Notice> {
        let unkept = self.needs_save() && !self.protect_file;
        if self.loading || self.gate.is_busy() || unkept {
            Err(Notice::BoardBusy)
        } else {
            Ok(())
        }
    }

    /// The open board is replaced by an empty one (then maybe read: `loading`).
    fn start_empty(&mut self, loading: bool) {
        let _repaired = self.board.replace_with(BoardData::default());
        self.history = History::default();
        self.history_revision = 0;
        self.saved_history_revision = 0;
        self.viewing = None;
        self.relocated.clear();
        self.input = BoardInput::default();
        self.status = Status::New;
        self.saving = None;
        self.protect_file = self.index_protected;
        self.loading = loading;
    }

    /// The plan of a change of the list alone: the list, to be written.
    fn list_plan(&mut self) -> Plan {
        self.index_dirty = false;
        Plan { effects: vec![self.index_effect()], ..Plan::default() }
    }

    fn name_cut(&mut self, cut: bool) {
        if cut {
            self.notices.push(Notice::BoardNameCut { limit: boards::MAX_NAME_CHARS });
        }
    }

    /// Opens board `id`: the board that is open must be kept first, then the other is read from its file.
    ///
    /// # Errors
    /// There is no such board, or the open one is not kept yet, or a board is being read ([`Notice::BoardBusy`]: ask
    /// for a save, and again when it is done).
    pub fn switch_board(&mut self, id: &str, _now_ms: u64) -> Result<Plan, Notice> {
        if !self.boards.get(id).is_some_and(boards::BoardMeta::is_shown) {
            return Err(Notice::NoSuchBoard);
        }
        if self.boards.active().id == id {
            return Ok(Plan::default());
        }
        self.leave()?;
        self.boards.set_active(id)?;
        self.start_empty(true);
        let mut plan = self.list_plan();
        plan.load = Some(self.boards.active().file.clone());
        Ok(plan)
    }

    /// A new, empty board, named `name`, at the end of the list, and open.
    ///
    /// # Errors
    /// The open board is not kept yet ([`Notice::BoardBusy`]), the workspace has as many boards as it may, the name is empty.
    pub fn new_board(&mut self, name: &str, now_ms: u64) -> Result<Plan, Notice> {
        self.leave()?;
        let (id, cut) = self.boards.create(name, now_ms)?;
        self.name_cut(cut);
        self.boards.set_active(&id)?;
        self.start_empty(false);
        Ok(self.list_plan())
    }

    /// Gives board `id` another name.
    ///
    /// # Errors
    /// There is no such board, or the name is empty.
    pub fn rename_board(&mut self, id: &str, name: &str, now_ms: u64) -> Result<Plan, Notice> {
        let cut = self.boards.rename(id, name, now_ms)?;
        self.name_cut(cut);
        Ok(self.list_plan())
    }

    /// A copy of the open board as it is now (its unkept changes too), named `name`, in a file of its own, at the
    /// end of the list. The open board stays open.
    ///
    /// # Errors
    /// The workspace has as many boards as it may, the name is empty.
    pub fn duplicate_board(&mut self, name: &str, now_ms: u64) -> Result<Plan, Notice> {
        let (id, cut) = self.boards.create(name, now_ms)?;
        self.name_cut(cut);
        let file = self.boards.get(&id).map(|b| b.file.clone()).unwrap_or_default();
        let entries = store::write_history(&self.board.snapshot(), &self.meta(now_ms), &self.history);
        let mut plan = self.list_plan();
        plan.copy = Some(Write { name: file, entries });
        Ok(plan)
    }

    /// Deletes board `id` (softly: [`Self::restore_board`] brings it back). When it was the open one the next board
    /// opens (read from its file), or, when it was the last, a new empty one named `fresh_name`.
    ///
    /// # Errors
    /// There is no such board, or the board is open and not kept yet ([`Notice::BoardBusy`]).
    pub fn delete_board(&mut self, id: &str, now_ms: u64, fresh_name: &str) -> Result<Plan, Notice> {
        let was_open = self.boards.active().id == id;
        if was_open && self.boards.get(id).is_some_and(boards::BoardMeta::is_shown) {
            self.leave()?;
        }
        let gone = self.boards.delete(id, now_ms, fresh_name)?;
        if !was_open {
            return Ok(self.list_plan());
        }
        self.start_empty(!gone.replaced_by_new);
        let mut plan = self.list_plan();
        if !gone.replaced_by_new {
            plan.load = Some(self.boards.active().file.clone());
        }
        Ok(plan)
    }

    /// Brings a deleted board back into the list (it is not opened).
    ///
    /// # Errors
    /// There is no such deleted board, or the workspace has as many boards as it may.
    pub fn restore_board(&mut self, id: &str, _now_ms: u64) -> Result<Plan, Notice> {
        self.boards.restore(id)?;
        Ok(self.list_plan())
    }

    /// Puts board `id` at place `to` in the list.
    ///
    /// # Errors
    /// There is no such board.
    pub fn move_board(&mut self, id: &str, to: usize, _now_ms: u64) -> Result<Plan, Notice> {
        self.boards.move_to(id, to)?;
        Ok(self.list_plan())
    }

    /// A board read from a file of another (or a copy of its own) becomes a new board of the workspace, named for what the
    /// file calls it (`fallback` when it calls it nothing), and open: the board that was open must be kept first. It is
    /// kept in a file of its own by the next save.
    ///
    /// # Errors
    /// The open board is not kept yet ([`Notice::BoardBusy`]), or the workspace has as many boards as it may.
    pub fn import_board(&mut self, loaded: Loaded, fallback: &str, now_ms: u64) -> Result<Plan, Notice> {
        self.leave()?;
        let named = loaded.meta.board_name.trim();
        let (id, cut) = self.boards.create(if named.is_empty() { fallback } else { named }, now_ms)?;
        self.name_cut(cut);
        self.boards.set_active(&id)?;
        self.start_empty(false);
        let (left_out, pictures, modified) = (loaded.left_out + loaded.snapshots_left_out, loaded.unreadable_pictures, loaded.meta.modified_ms);
        let notices = self.board.replace_with(loaded.data);
        self.history = loaded.history;
        // What was read is not what is in this board's own file (there is none): the next save writes it.
        self.board.mark_unsaved();
        self.history_revision = 1;
        self.saved_history_revision = 0;
        self.status = if left_out > 0 || pictures > 0 || !notices.is_empty() {
            Status::Repaired { left_out, pictures }
        } else {
            Status::Loaded { items: self.board.len(), saved_ms: modified }
        };
        if left_out > 0 {
            self.notices.push(Notice::BoardRepaired { dropped: left_out });
        }
        self.notices.extend(notices);
        Ok(self.list_plan())
    }

    // ---- the history

    /// The snapshots of the open board, oldest first.
    #[must_use]
    pub const fn history(&self) -> &History {
        &self.history
    }

    /// Whether the board, its view or its history has changes that are not kept.
    #[must_use]
    pub fn needs_save(&self) -> bool {
        self.board.needs_save() || self.history_revision != self.saved_history_revision
    }

    /// What a snapshot taken now would say: what it follows, what changed since, the code as it is.
    #[must_use]
    pub fn snapshot_draft(&self) -> Draft {
        let latest = self.history.latest();
        let data = self.board.snapshot();
        Draft {
            parent: latest.map(|s| s.id.clone()),
            summary: history::diff(latest.map(|s| &s.data), &data),
            code: self.repo.as_ref().map_or_else(Code::default, Code::of),
            pending: data.items.iter().filter(|i| matches!(i, Item::Cutout(c) if matches!(c.image, ImageSlot::Pending))).count(),
            references: self.relocated.values().cloned().collect(),
        }
    }

    /// Looks for the code every cutout points at again, in the files as they are now (`files`), at the revision the code is at
    /// ([`Self::set_repo`]): a reference that moved or changed follows the code, one that vanished is left as it was, and
    /// all that are not as they were are flagged until the next snapshot records them. Returns how many are flagged.
    pub fn relocate_all(&mut self, files: &dyn FileView) -> usize {
        let limits = *self.board.limits();
        let rev = self.repo.as_ref().and_then(|r| r.rev.clone());
        let sources: Vec<(ItemId, Source)> = self.board.cutouts().map(|c| (c.id, c.source.clone())).collect();
        for (id, source) in sources {
            let r = relocate::relocate(&source, files, rev.as_deref(), &limits);
            if r.status.is_flag() {
                // Where it first was since the last snapshot, and where it is now.
                let from = self.relocated.get(&id).map_or(r.from, |c| c.from);
                self.relocated.insert(id, RefChange { id, status: r.status, from, to: r.to });
            }
            self.board.set_source(id, &r.source);
        }
        self.relocated.len()
    }

    /// The references that are flagged (not as they were when the last snapshot recorded them), by cutout.
    #[must_use]
    pub fn flags(&self) -> &std::collections::BTreeMap<ItemId, RefChange> {
        &self.relocated
    }

    /// The revision the code is at now, and the one the newest snapshot was taken at, when they differ: the code moved on
    /// since the board was last kept as a snapshot. `None` for a board with no snapshot, or when either is not known.
    #[must_use]
    pub fn head_moved(&self) -> Option<(String, String)> {
        let was = self.history.latest()?.code.rev.clone()?;
        let now = self.repo.as_ref()?.rev.clone()?;
        (was != now).then_some((was, now))
    }

    /// A snapshot of `data` made final (not in the history yet): `before` is what it is set against for its summary.
    #[allow(clippy::too_many_arguments)]
    fn build_snapshot(
        &self,
        id: String,
        parent: Option<String>,
        data: BoardData,
        before: Option<&BoardData>,
        references: Vec<RefChange>,
        why: &str,
        tags: &str,
        now_ms: u64,
    ) -> Snapshot {
        let snapshot = Snapshot {
            id,
            parent,
            taken_ms: now_ms,
            author: self.author_name(),
            code: self.repo.as_ref().map_or_else(Code::default, Code::of),
            why: history::clean_why(why),
            tags: history::parse_tags(tags),
            summary: Summary { references, ..history::diff(before, &data) },
            evidence: None,
            data,
            value: crate::whiteboard_json::Json::Null,
            text: String::new(),
            hash: String::new(),
            verified: false,
            pictures: std::collections::BTreeMap::new(),
        };
        azboard::seal(snapshot, self.settings.permalink_template.as_deref())
    }

    fn history_full() -> Notice {
        Notice::HistoryFull { snapshots: history::MAX_SNAPSHOTS, items: history::MAX_HISTORY_ITEMS, megabytes: history::MAX_HISTORY_BYTES / (1024 * 1024) }
    }

    /// The board as it is now becomes a snapshot of the history: why it is as it is (`why`), the tags typed for it (`tags`,
    /// split at commas). Its text is written once and hashed; the board is kept again by the next save.
    ///
    /// # Errors
    /// A board is being read or a picture drawn ([`Notice::BoardBusy`], [`Notice::PicturesPending`]: again in a moment), or
    /// the history is full ([`Notice::HistoryFull`]: as many snapshots, items or text as a board keeps).
    pub fn take_snapshot(&mut self, why: &str, tags: &str, now_ms: u64) -> Result<String, Notice> {
        if self.loading {
            return Err(Notice::BoardBusy);
        }
        let draft = self.snapshot_draft();
        if draft.pending > 0 {
            return Err(Notice::PicturesPending);
        }
        let data = self.board.snapshot();
        if !self.history.has_room_for(data.items.len()) {
            return Err(Self::history_full());
        }
        let sealed = self.build_snapshot(self.history.next_id(), draft.parent, data, self.history.latest().map(|s| &s.data), draft.references, why, tags, now_ms);
        if !self.history.fits(1, sealed.data.items.len(), sealed.text.len()) {
            return Err(Self::history_full());
        }
        let id = sealed.id.clone();
        self.history.snapshots.push(sealed);
        self.history_revision += 1;
        // What was flagged is recorded now.
        self.relocated.clear();
        Ok(id)
    }

    /// Shows snapshot `id` in place of the board, read-only: the board is not touched.
    ///
    /// # Errors
    /// There is no such snapshot.
    pub fn view_snapshot(&mut self, id: &str) -> Result<(), Notice> {
        let snapshot = self.history.get(id).ok_or(Notice::NoSuchSnapshot)?;
        let (board, _notices) = Board::from_data(snapshot.data.clone(), *self.board.limits());
        self.viewing = Some(Viewing { id: id.to_string(), board, compare: None });
        Ok(())
    }

    /// The board is shown again (the snapshot that was looked at is let go of).
    pub fn back_to_live(&mut self) {
        self.viewing = None;
    }

    /// The snapshot that is looked at.
    #[must_use]
    pub const fn viewing(&self) -> Option<&Viewing> {
        self.viewing.as_ref()
    }

    /// The board that is shown: the snapshot's while one is looked at, else the open board.
    #[must_use]
    pub fn display(&self) -> &Board {
        self.viewing.as_ref().map_or(&self.board, |v| &v.board)
    }

    /// The board that is shown, for the view to be panned and zoomed: the snapshot's own board while one is looked at (its
    /// items are not changed by anything the person does), else the open board.
    pub fn display_mut(&mut self) -> &mut Board {
        match self.viewing.as_mut() {
            Some(v) => &mut v.board,
            None => &mut self.board,
        }
    }

    /// The wheel or a trackpad at `at`: the board that is shown is moved (or, with the command key, zoomed); a snapshot that is
    /// looked at has its own view and nothing else of it changes.
    pub fn wheel(&mut self, at: Pt, delta: Pt, mods: Mods) -> Out {
        match self.viewing.as_mut() {
            Some(v) => self.input.wheel(&mut v.board, at, delta, mods),
            None => self.input.wheel(&mut self.board, at, delta, mods),
        }
    }

    /// Sets the snapshot that is looked at against its parent (`on`), or takes the comparison off.
    pub fn set_compare(&mut self, on: bool) {
        let Some(id) = self.viewing.as_ref().map(|v| v.id.clone()) else {
            return;
        };
        let compare = on.then(|| {
            let snapshot = self.history.get(&id);
            let parent = self.history.parent_of(&id);
            let summary = history::diff(parent.map(|p| &p.data), snapshot.map_or(&BoardData::default(), |s| &s.data));
            let removed: Vec<Item> = parent
                .map(|p| p.data.items.iter().filter(|i| summary.removed.iter().any(|b| b.id == i.id())).cloned().collect())
                .unwrap_or_default();
            Compare { summary, removed, against: parent.map(|p| p.id.clone()) }
        });
        if let Some(v) = self.viewing.as_mut() {
            v.compare = compare;
        }
    }

    /// The board becomes what snapshot `id` was, and the new state is a snapshot of its own (`why`, `tags`): history is
    /// appended to and nothing in it is rewritten. A board that has changes no snapshot holds is made a snapshot first (said
    /// so in its reason), so that restoring loses nothing. The board's view and the ids it has given stay as they are; the
    /// items are the snapshot's.
    ///
    /// # Errors
    /// There is no such snapshot, a board is being read or a picture drawn, or the history has no room for the one or two
    /// snapshots ([`Notice::HistoryFull`]); nothing is changed then.
    pub fn restore_snapshot(&mut self, id: &str, why: &str, tags: &str, now_ms: u64) -> Result<String, Notice> {
        if self.loading {
            return Err(Notice::BoardBusy);
        }
        let target = self.history.get(id).ok_or(Notice::NoSuchSnapshot)?.data.clone();
        let draft = self.snapshot_draft();
        let live = self.board.snapshot();
        let latest = self.history.latest();
        let unsaved = history::diff(latest.map(|s| &s.data), &live);
        if !unsaved.is_empty() && draft.pending > 0 {
            return Err(Notice::PicturesPending);
        }
        // Both snapshots are made before anything is changed: if the history cannot hold them, nothing happened.
        let first = self.history.next_id();
        let number: u64 = first.strip_prefix('s').and_then(|n| n.parse().ok()).unwrap_or(0);
        let (mut parent, mut before) = (latest.map(|s| s.id.clone()), latest.map(|s| s.data.clone()));
        let mut made: Vec<Snapshot> = Vec::new();
        if !unsaved.is_empty() {
            let reason = format!("The state of the board before it was restored to snapshot {id} (taken by itself).");
            let s = self.build_snapshot(first, parent.clone(), live.clone(), before.as_ref(), draft.references.clone(), &reason, "", now_ms);
            parent = Some(s.id.clone());
            before = Some(s.data.clone());
            made.push(s);
        }
        let mut data = target;
        data.next_id = data.next_id.max(live.next_id);
        data.view = live.view;
        let restored_id = format!("s{}", number + u64::try_from(made.len()).unwrap_or(0));
        let restored = self.build_snapshot(restored_id.clone(), parent, data.clone(), before.as_ref(), Vec::new(), why, tags, now_ms);
        made.push(restored);
        let (items, bytes) = (made.iter().map(|s| s.data.items.len()).sum(), made.iter().map(|s| s.text.len()).sum());
        if !self.history.fits(made.len(), items, bytes) {
            return Err(Self::history_full());
        }
        self.history.snapshots.extend(made);
        self.history_revision += 1;
        let _notices = self.board.replace_with(data);
        self.board.mark_unsaved();
        self.viewing = None;
        self.relocated.clear();
        Ok(restored_id)
    }

    // ---- the file

    /// The save to write, when the board has changed since it was kept and none is being written (a save
    /// wanted while one runs is asked again when it is done: [`Self::save_done`]).
    pub fn save_job(&mut self, now_ms: u64) -> Option<SaveJob> {
        if self.protect_file || !self.needs_save() || !self.gate.request() {
            return None;
        }
        let revision = self.board.content_revision();
        let view_revision = self.board.view_revision();
        self.saving = Some((revision, view_revision, now_ms, self.history_revision));
        Some(SaveJob {
            name: self.boards.active().file.clone(),
            entries: store::write_history(&self.board.snapshot(), &self.meta(now_ms), &self.history),
            revision,
            view_revision,
        })
    }

    /// The write of the save is over. `true` when another was wanted meanwhile and is worth building
    /// ([`Self::save_job`]).
    pub fn save_done(&mut self, result: &Result<(), String>) -> bool {
        let again = self.gate.finished();
        match (result, self.saving.take()) {
            (Ok(()), Some((revision, view, at, history_revision))) => {
                self.board.mark_saved_at(revision, view);
                self.saved_history_revision = history_revision.min(self.history_revision);
                self.status = Status::Saved(at);
                let open = self.boards.active().id.clone();
                self.boards.touch(&open, at);
                self.index_dirty = true;
            }
            (Err(why), _) => self.status = Status::SaveFailed(why.clone()),
            (Ok(()), None) => {}
        }
        again
    }

    /// There was no file for the board that is open.
    pub fn loaded_none(&mut self) -> Vec<Effect> {
        self.status = Status::New;
        self.loading = false;
        Vec::new()
    }

    /// The file was read (`raw`: its bytes, for keeping a copy of one that cannot be used). What it holds
    /// is the board now; a file that is no board of this version is kept aside (the effects to write)
    /// before anything replaces it, and a file of a newer version is never replaced.
    pub fn loaded(&mut self, result: Result<Loaded, StoreError>, raw: Option<Vec<u8>>, now_ms: u64) -> Vec<Effect> {
        self.loading = false;
        let aside = |raw: Option<Vec<u8>>| -> (Option<String>, Vec<Effect>) {
            let name = store::quarantine_name(&self.boards.active().file, now_ms);
            match raw {
                Some(bytes) => (Some(name.clone()), vec![Effect { name, bytes }]),
                None => (None, Vec::new()),
            }
        };
        match result {
            Ok(l) => {
                let (items_left_out, pictures) = (l.left_out + l.snapshots_left_out, l.unreadable_pictures);
                let notices = self.board.replace_with(l.data);
                self.history = l.history;
                self.history_revision = 0;
                self.saved_history_revision = 0;
                self.viewing = None;
                self.relocated.clear();
                self.status = if items_left_out > 0 || pictures > 0 || !notices.is_empty() {
                    Status::Repaired { left_out: items_left_out, pictures }
                } else {
                    Status::Loaded { items: self.board.len(), saved_ms: l.meta.modified_ms }
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
