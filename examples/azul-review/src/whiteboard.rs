//! The whiteboard: the architecture of a whole workspace as cutouts of code and arrows between
//! them. A [`Board`] is the model of one workspace's board - pure data and the steps that change
//! it, with no window, no azul and no disk: the lasso that makes a cutout is `whiteboard_lasso`,
//! the picture of it `whiteboard_cutout` and the render glue, the file it is kept in
//! `whiteboard_store`, the gestures on it `whiteboard_input`.
//!
//! - **Items.** A [`Cutout`] is a picture of some lines of a file (premultiplied RGBA at the
//!   scale of the screen it was drawn for, and its PNG) with the place it came from ([`Source`]:
//!   file, lines, the lasso's shape, hashes of what was there). An [`Arrow`] runs between two
//!   cutouts, or from one to a free point; it hangs on the cutouts and follows them
//!   ([`Board::place_arrow`]). Items are drawn in the order they are kept in: the last is on top.
//! - **Steps.** Every change a person makes is a step Mod+Z takes back ([`Board::undo`]) and
//!   brings again; a drag is one step however many moves it has ([`Board::move_live`],
//!   [`Board::commit_live`]). A cutout taken off takes its arrows with it, in the same step.
//! - **Caps.** [`Limits`]: how many items, how big a picture, how much picture in all, how long a
//!   label. What would pass one is refused with a [`Notice`] (never a panic, never a silent cut);
//!   a label is cut and told.
//! - **Hearing.** [`BoardObserver`] hears every change as a [`BoardEvent`] (a recorder plugs in
//!   there; the default hears nothing).

use std::{
    collections::{BTreeSet, HashMap},
    sync::Arc,
};

use crate::whiteboard_geom::{ray_exit, side_mid, Curve, Pt, Rect, Side, Size, View, MAX_BEND};

/// What names an item of the board; never given twice on one board.
pub type ItemId = u64;

/// How much a board takes. Every input has its realistic least and most (a cutout of a screenful,
/// not of a library), and the board keeps to them whatever it is given.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    /// Cutouts and arrows together.
    pub max_items: usize,
    /// The pixels of one picture (a page at 2x is 2.9 million: the capture is made at a smaller
    /// scale to fit).
    pub max_image_px: u64,
    /// The pixels of every picture on the board (4 bytes each: 160 MB at the cap).
    pub max_total_px: u64,
    /// The characters of a label.
    pub max_label_chars: usize,
    /// The points of the lasso's outline that are kept.
    pub max_shape_points: usize,
    /// The lines of text kept with a cutout (what it shows, for finding it again).
    pub max_source_lines: usize,
    /// The characters kept of one of those lines.
    pub max_source_line_chars: usize,
    /// How far from the origin an item may lie.
    pub max_coord: f32,
    /// The least side of a cutout, board px.
    pub min_cutout: f32,
    /// The greatest side of a cutout, board px.
    pub max_cutout: f32,
    /// How far the view zooms out and in.
    pub zoom_min: f32,
    pub zoom_max: f32,
    /// How many steps Mod+Z can take back.
    pub history: usize,
}

/// The limits a board has unless it is given others.
pub static LIMITS: Limits = Limits {
    max_items: 500,
    max_image_px: 1_500_000,
    max_total_px: 40_000_000,
    max_label_chars: 120,
    max_shape_points: 128,
    max_source_lines: 200,
    max_source_line_chars: 300,
    max_coord: 100_000.0,
    min_cutout: 24.0,
    max_cutout: 4_000.0,
    zoom_min: 0.2,
    zoom_max: 4.0,
    history: 200,
};

impl Default for Limits {
    fn default() -> Self {
        LIMITS
    }
}

/// What the board has to say about something it refused or changed: the person is told, in their
/// language (`key` and `args` name the sentence in the resources; `english` is the same sentence for
/// a log).
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    TooManyItems { limit: usize },
    PicturesTooBig { limit_px: u64 },
    PictureTooBig { px: u64, limit_px: u64 },
    PictureInvalid,
    /// The picture of a large cutout is drawn at a lower scale than the screen's to fit in memory.
    ScaleLowered { wanted_pct: u32, used_pct: u32 },
    LabelShortened { limit: usize },
    NoSuchItem { id: ItemId },
    ArrowToItself,
    ArrowNeedsACutout,
    ArrowRepeated,
    /// A board read from a file had items that could not be used (a dangling arrow, an id twice,
    /// more than the cap): they were left out.
    BoardRepaired { dropped: usize },
    /// The workspace has as many boards as it may (the whiteboards of a folder, not the items of one).
    TooManyBoards { limit: usize },
    /// A name of a board holds `limit` characters at most: the rest was cut off.
    BoardNameCut { limit: usize },
    BoardNameEmpty,
    NoSuchBoard,
    /// The board that is open is not kept yet, or a board is being read: the whiteboard is not left until it is.
    BoardBusy,
    /// A snapshot cannot be added: the board keeps `snapshots` of them, or `items` items, or `megabytes` MB of text in all of them,
    /// at most.
    HistoryFull { snapshots: usize, items: usize, megabytes: usize },
    NoSuchSnapshot,
    /// A snapshot waits until the pictures that are being drawn are.
    PicturesPending,
}

impl Notice {
    /// The sentence in English.
    #[must_use]
    pub fn english(&self) -> String {
        match self {
            Self::TooManyItems { limit } => {
                format!("The whiteboard holds {limit} items, as many as it has room for: remove one first.")
            }
            Self::PicturesTooBig { limit_px } => format!(
                "The pictures on the whiteboard take as much memory as they may ({limit_px} pixels): remove a cutout first."
            ),
            Self::PictureTooBig { px, limit_px } => format!(
                "This cutout's picture has {px} pixels, more than the {limit_px} a cutout may have: circle less code."
            ),
            Self::PictureInvalid => {
                "The cutout's picture could not be used: its bytes do not match its size.".to_string()
            }
            Self::ScaleLowered { wanted_pct, used_pct } => format!(
                "This cutout is large: its picture is drawn at {used_pct}% scale instead of {wanted_pct}% so that it fits in memory, so it is a little less sharp."
            ),
            Self::LabelShortened { limit } => {
                format!("A label holds {limit} characters at most: the rest was cut off.")
            }
            Self::NoSuchItem { id } => format!("There is no item {id} on the whiteboard."),
            Self::ArrowToItself => {
                "An arrow needs two different cutouts: it cannot start and end on the same one.".to_string()
            }
            Self::ArrowNeedsACutout => "An arrow has to start or end on a cutout.".to_string(),
            Self::ArrowRepeated => "There is an arrow like this one already.".to_string(),
            Self::BoardRepaired { dropped } => format!(
                "{dropped} items of the saved whiteboard could not be used and were left out."
            ),
            Self::TooManyBoards { limit } => {
                format!("This folder has {limit} whiteboards, as many as it has room for: delete one first.")
            }
            Self::BoardNameCut { limit } => {
                format!("The name of a whiteboard holds {limit} characters at most: the rest was cut off.")
            }
            Self::BoardNameEmpty => "A whiteboard needs a name.".to_string(),
            Self::NoSuchBoard => "There is no such whiteboard in this folder.".to_string(),
            Self::BoardBusy => "The whiteboard is being kept or opened: one moment.".to_string(),
            Self::HistoryFull { snapshots, items, megabytes } => format!(
                "The history of this whiteboard is full ({snapshots} snapshots, or {items} items, or {megabytes} MB of text in all of them, at most): it keeps what it has."
            ),
            Self::NoSuchSnapshot => "There is no such snapshot on this whiteboard.".to_string(),
            Self::PicturesPending => "A picture is still being drawn: one moment, then take the snapshot.".to_string(),
        }
    }

    /// The key of the sentence in `resources/*.ftl`.
    #[must_use]
    pub const fn key(&self) -> &'static str {
        match self {
            Self::TooManyItems { .. } => "azreview-whiteboard-notice-too-many-items",
            Self::PicturesTooBig { .. } => "azreview-whiteboard-notice-pictures-too-big",
            Self::PictureTooBig { .. } => "azreview-whiteboard-notice-picture-too-big",
            Self::PictureInvalid => "azreview-whiteboard-notice-picture-invalid",
            Self::ScaleLowered { .. } => "azreview-whiteboard-notice-scale-lowered",
            Self::LabelShortened { .. } => "azreview-whiteboard-notice-label-shortened",
            Self::NoSuchItem { .. } => "azreview-whiteboard-notice-no-such-item",
            Self::ArrowToItself => "azreview-whiteboard-notice-arrow-to-itself",
            Self::ArrowNeedsACutout => "azreview-whiteboard-notice-arrow-needs-a-cutout",
            Self::ArrowRepeated => "azreview-whiteboard-notice-arrow-repeated",
            Self::BoardRepaired { .. } => "azreview-whiteboard-notice-board-repaired",
            Self::TooManyBoards { .. } => "azreview-whiteboard-notice-too-many-boards",
            Self::BoardNameCut { .. } => "azreview-whiteboard-notice-board-name-cut",
            Self::BoardNameEmpty => "azreview-whiteboard-notice-board-name-empty",
            Self::NoSuchBoard => "azreview-whiteboard-notice-no-such-board",
            Self::BoardBusy => "azreview-whiteboard-notice-board-busy",
            Self::HistoryFull { .. } => "azreview-whiteboard-notice-history-full",
            Self::NoSuchSnapshot => "azreview-whiteboard-notice-no-such-snapshot",
            Self::PicturesPending => "azreview-whiteboard-notice-pictures-pending",
        }
    }

    /// The numbers the sentence names (`{ $limit }`), by the name the resources give them.
    #[must_use]
    pub fn args(&self) -> Vec<(&'static str, u64)> {
        match self {
            Self::TooManyItems { limit }
            | Self::LabelShortened { limit }
            | Self::TooManyBoards { limit }
            | Self::BoardNameCut { limit } => vec![("limit", *limit as u64)],
            Self::PicturesTooBig { limit_px } => vec![("limit", *limit_px)],
            Self::PictureTooBig { px, limit_px } => vec![("px", *px), ("limit", *limit_px)],
            Self::ScaleLowered { wanted_pct, used_pct } => {
                vec![("wanted", u64::from(*wanted_pct)), ("used", u64::from(*used_pct))]
            }
            Self::NoSuchItem { id } => vec![("id", *id)],
            Self::BoardRepaired { dropped } => vec![("dropped", *dropped as u64)],
            Self::HistoryFull { snapshots, items, megabytes } => vec![("snapshots", *snapshots as u64), ("items", *items as u64), ("megabytes", *megabytes as u64)],
            Self::PictureInvalid
            | Self::ArrowToItself
            | Self::ArrowNeedsACutout
            | Self::ArrowRepeated
            | Self::BoardNameEmpty
            | Self::NoSuchBoard
            | Self::BoardBusy
            | Self::NoSuchSnapshot
            | Self::PicturesPending => Vec::new(),
        }
    }
}

/// What a cutout shows, drawn on the CPU at the scale of the screen it was made for.
#[derive(Debug, Clone, PartialEq)]
pub struct CutoutImage {
    pub width: u32,
    pub height: u32,
    /// Device pixels per logical px when it was drawn: the picture is shown `width / scale` wide.
    pub scale: f32,
    /// `width x height` pixels, four bytes each, colours multiplied by alpha, rows top to bottom.
    pub rgba: Vec<u8>,
    /// The same picture as a PNG (made once, when the cutout was captured: a save writes it as it is).
    pub png: Vec<u8>,
}

impl CutoutImage {
    #[must_use]
    pub fn pixels(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    /// Whether the bytes are the picture the size says and the scale means something.
    #[must_use]
    pub fn is_consistent(&self) -> bool {
        self.width > 0
            && self.height > 0
            && self.scale.is_finite()
            && self.scale > 0.0
            && self.rgba.len() as u64 == self.pixels() * 4
    }

    /// How large the picture is in board px.
    #[must_use]
    pub fn logical_size(&self) -> Size {
        Size::new(self.width as f32 / self.scale, self.height as f32 / self.scale)
    }
}

/// Where a cutout's picture is.
#[derive(Debug, Clone, PartialEq)]
pub enum ImageSlot {
    /// Being drawn (on a Thread): the cutout is there already, the picture comes.
    Pending,
    Ready(Arc<CutoutImage>),
    /// Could not be drawn or read back; the cutout shows the lines it was made from instead.
    Failed(String),
}

/// Where a cutout came from: what the jump goes back to.
#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// The file, relative to the workspace, with `/`.
    pub file: String,
    /// The page of the file the lasso was drawn on.
    pub page: u32,
    /// The lines the lasso covers, counted from 1, both included.
    pub first_line: u32,
    pub last_line: u32,
    /// The lasso's outline in the page's own px.
    pub shape: Vec<Pt>,
    /// [`lines_hash`] of the whole file, and of the covered lines, as they were.
    pub file_hash: u64,
    pub range_hash: u64,
    /// The covered lines as they were, cut to the limits (what finds them again).
    pub lines: Vec<String>,
    /// The full hash of the commit the workspace was at when the lines were circled: what a link to them names, so that
    /// it keeps showing these lines. Empty when it was not known (no repository, or a board of a version that did not
    /// record it).
    pub rev: String,
    /// `sha256:` and the hash of the covered lines as they were, all of them, not only the ones kept
    /// ([`crate::whiteboard_hash::lines_tagged`]). Empty when it was not known.
    pub sha256: String,
}

impl Source {
    /// This source cut to what a board keeps: the file with `/`, the lines the right way round, the
    /// outline without points that are no numbers and at most `max_shape_points` of them, the lines of
    /// text few and short.
    #[must_use]
    pub fn sanitized(&self, limits: &Limits) -> Self {
        let (first, last) = (self.first_line.min(self.last_line), self.first_line.max(self.last_line));
        let finite: Vec<Pt> = self.shape.iter().copied().filter(|p| p.is_finite()).collect();
        let shape = if finite.len() > limits.max_shape_points && limits.max_shape_points > 0 {
            let step = finite.len() as f32 / limits.max_shape_points as f32;
            (0..limits.max_shape_points).map(|i| finite[((i as f32 * step) as usize).min(finite.len() - 1)]).collect()
        } else {
            finite
        };
        Self {
            file: self.file.replace('\\', "/"),
            page: self.page,
            first_line: first,
            last_line: last,
            shape,
            file_hash: self.file_hash,
            range_hash: self.range_hash,
            lines: kept_lines(&self.lines, limits),
            rev: if crate::whiteboard_git::is_hash(&self.rev) { self.rev.clone() } else { String::new() },
            sha256: if crate::whiteboard_hash::is_tagged(&self.sha256) { self.sha256.clone() } else { String::new() },
        }
    }
}

/// `lines` as a board keeps them with a cutout: at most `max_source_lines` of them, each cut to
/// `max_source_line_chars` characters. What the hash of a cutout's lines ([`Source::range_hash`]) is of,
/// at the capture and at the jump alike.
#[must_use]
pub fn kept_lines(lines: &[String], limits: &Limits) -> Vec<String> {
    lines
        .iter()
        .take(limits.max_source_lines)
        .map(|l| l.chars().take(limits.max_source_line_chars).collect())
        .collect()
}

/// FNV-1a over the lines, each followed by a newline: a number that tells whether lines are what they
/// were. (Not a security hash; a board file's hashes only say "this changed".)
#[must_use]
pub fn lines_hash(lines: &[String]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for line in lines {
        for b in line.bytes().chain(std::iter::once(b'\n')) {
            h ^= u64::from(b);
            h = h.wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// A picture of some lines of code on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct Cutout {
    pub id: ItemId,
    /// Where it lies on the board and how large it is shown (board px).
    pub rect: Rect,
    pub image: ImageSlot,
    pub label: String,
    pub source: Source,
}

/// Where on a cutout an arrow is fastened.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// Where the line to the other end meets the cutout's edge.
    Auto,
    /// The middle of a side.
    Side(Side),
    /// A place on the cutout, as the fractions of its width and height.
    Point { u: f32, v: f32 },
}

/// One end of an arrow.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum End {
    /// Fastened to a cutout: it follows it.
    Item { id: ItemId, anchor: Anchor },
    /// Loose, at a place of the board.
    Free(Pt),
}

impl End {
    /// Fastened to cutout `id` where the line to the other end meets it.
    #[must_use]
    pub const fn item(id: ItemId) -> Self {
        Self::Item { id, anchor: Anchor::Auto }
    }

    /// The cutout this end hangs on.
    #[must_use]
    pub const fn item_id(&self) -> Option<ItemId> {
        match self {
            Self::Item { id, .. } => Some(*id),
            Self::Free(_) => None,
        }
    }
}

/// Which end of an arrow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Which {
    From,
    To,
}

/// An arrow from one end to another, with the head at `to`.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrow {
    pub id: ItemId,
    pub from: End,
    pub to: End,
    /// How far the arrow bends ([`Curve::bent`]); 0 is straight.
    pub bend: f32,
    pub label: String,
}

/// One thing on the board.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Cutout(Cutout),
    Arrow(Arrow),
}

impl Item {
    #[must_use]
    pub const fn id(&self) -> ItemId {
        match self {
            Self::Cutout(c) => c.id,
            Self::Arrow(a) => a.id,
        }
    }

    const fn label_mut(&mut self) -> &mut String {
        match self {
            Self::Cutout(c) => &mut c.label,
            Self::Arrow(a) => &mut a.label,
        }
    }
}

/// What a board is, as it is kept in a file: the items in the order they are drawn, the view, the id
/// the next item takes.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardData {
    pub items: Vec<Item>,
    pub view: View,
    pub next_id: ItemId,
}

impl Default for BoardData {
    fn default() -> Self {
        Self { items: Vec::new(), view: View::default(), next_id: 1 }
    }
}

/// An arrow where it is drawn now: from the edge of one end to the edge of the other.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub from: Pt,
    pub to: Pt,
    pub curve: Curve,
}

/// A cutout about to be put on the board.
#[derive(Debug, Clone, PartialEq)]
pub struct CutoutDraft {
    /// Where its top-left corner lies.
    pub at: Pt,
    /// How large it is shown, when the picture does not say (it is still being drawn).
    pub size: Option<Size>,
    pub image: ImageSlot,
    pub label: String,
    pub source: Source,
}

impl CutoutDraft {
    /// A cutout whose picture is drawn: shown as large as the picture is.
    #[must_use]
    pub fn ready(at: Pt, image: Arc<CutoutImage>, source: Source) -> Self {
        Self { at, size: None, image: ImageSlot::Ready(image), label: String::new(), source }
    }

    /// A cutout whose picture is still being drawn, `size` large.
    #[must_use]
    pub fn pending(at: Pt, size: Size, source: Source) -> Self {
        Self { at, size: Some(size), image: ImageSlot::Pending, label: String::new(), source }
    }
}

/// What happened on the board.
#[derive(Debug, Clone, PartialEq)]
pub enum BoardEvent {
    CutoutCreated { id: ItemId, file: String, first_line: u32, last_line: u32 },
    ArrowDrawn { id: ItemId, from: Option<ItemId>, to: Option<ItemId> },
    ArrowChanged { id: ItemId },
    CutoutMoved { id: ItemId },
    CutoutResized { id: ItemId },
    Labelled { id: ItemId },
    /// Items taken off (a cutout and the arrows that hung on it).
    Removed { ids: Vec<ItemId> },
    Undone,
    Redone,
    /// A cutout was double-clicked and the app went to its lines.
    Jumped { id: ItemId, file: String, first_line: u32, last_line: u32 },
}

/// Hears what happens on a board. A recorder of the review implements this; the default does nothing.
pub trait BoardObserver {
    fn on_event(&mut self, _event: &BoardEvent) {}
}

/// The observer that hears nothing.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoObserver;

impl BoardObserver for NoObserver {}

/// One change to the items, and (applied) the change that puts them back.
#[derive(Debug, Clone, PartialEq)]
enum Edit {
    /// Put these items at these places of the list (ascending).
    Insert(Vec<(usize, Item)>),
    /// Take these items off.
    Delete(Vec<ItemId>),
    SetRect { id: ItemId, rect: Rect },
    SetLabel { id: ItemId, label: String },
    SetArrow(Arrow),
}

/// A drag in progress: what the cutout was before it began.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Live {
    id: ItemId,
    from: Rect,
    resizing: bool,
}

/// The board of one workspace.
pub struct Board {
    data: BoardData,
    limits: Limits,
    /// What puts things back, the newest last.
    history: Vec<Edit>,
    /// What undoes the undoing.
    future: Vec<Edit>,
    live: Option<Live>,
    /// Counts every change to the items (and to a picture): a view redraws when it moved.
    revision: u64,
    saved_revision: u64,
    view_revision: u64,
    saved_view_revision: u64,
    notices: Vec<Notice>,
    observer: Box<dyn BoardObserver>,
}

impl Default for Board {
    fn default() -> Self {
        Self::new()
    }
}

impl Board {
    /// An empty board with the usual limits.
    #[must_use]
    pub fn new() -> Self {
        Self::with_limits(LIMITS)
    }

    #[must_use]
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            data: BoardData::default(),
            limits,
            history: Vec::new(),
            future: Vec::new(),
            live: None,
            revision: 0,
            saved_revision: 0,
            view_revision: 0,
            saved_view_revision: 0,
            notices: Vec::new(),
            observer: Box::new(NoObserver),
        }
    }

    /// The board a file holds, made consistent: an item that cannot be used (an id twice, an arrow
    /// hanging on nothing or on itself or like another, items past the cap) is left out and counted.
    /// The board comes without history.
    #[must_use]
    pub fn from_data(data: BoardData, limits: Limits) -> (Self, Vec<Notice>) {
        let mut board = Self::with_limits(limits);
        let total = data.items.len();
        let mut ids = BTreeSet::new();
        let mut cutout_ids = BTreeSet::new();
        for item in &data.items {
            if let Item::Cutout(c) = item {
                if ids.insert(c.id) {
                    cutout_ids.insert(c.id);
                }
            }
        }
        let mut seen = BTreeSet::new();
        let mut kept: Vec<Item> = Vec::new();
        let mut max_id = data.next_id.saturating_sub(1);
        for item in data.items {
            max_id = max_id.max(item.id());
            if kept.len() >= limits.max_items || !seen.insert(item.id()) {
                continue;
            }
            match item {
                Item::Cutout(mut c) => {
                    c.rect = board.sane_rect(c.rect);
                    c.label = c.label.chars().take(limits.max_label_chars).collect();
                    c.source = c.source.sanitized(&limits);
                    kept.push(Item::Cutout(c));
                }
                Item::Arrow(mut a) => {
                    a.bend = sane_bend(a.bend);
                    a.label = a.label.chars().take(limits.max_label_chars).collect();
                    a.from = board.sane_end(a.from);
                    a.to = board.sane_end(a.to);
                    kept.push(Item::Arrow(a));
                }
            }
        }
        // An arrow keeps only what still hangs on cutouts that are on the board.
        let on_board: BTreeSet<ItemId> =
            kept.iter().filter_map(|i| matches!(i, Item::Cutout(_)).then(|| i.id())).collect();
        let mut arrows: Vec<(End, End)> = Vec::new();
        kept.retain(|item| {
            let Item::Arrow(a) = item else { return true };
            let hangs = |e: &End| e.item_id().is_none_or(|id| on_board.contains(&id));
            let ok = hangs(&a.from)
                && hangs(&a.to)
                && validate_ends(&a.from, &a.to).is_ok()
                && !arrows.contains(&(a.from, a.to));
            if ok {
                arrows.push((a.from, a.to));
            }
            ok
        });
        let dropped = total - kept.len();
        board.data = BoardData { items: kept, view: board.sane_view(data.view), next_id: max_id + 1 };
        let mut notices = Vec::new();
        if dropped > 0 {
            notices.push(Notice::BoardRepaired { dropped });
            // What was left out is not on disk any more once saved: the board asks for a save.
            board.revision = 1;
        }
        (board, notices)
    }

    /// Lets `observer` hear everything that happens from now on.
    pub fn set_observer(&mut self, observer: Box<dyn BoardObserver>) {
        self.observer = observer;
    }

    fn emit(&mut self, event: &BoardEvent) {
        self.observer.on_event(event);
    }

    #[must_use]
    pub const fn limits(&self) -> &Limits {
        &self.limits
    }

    /// What the board is now, to keep in a file (cheap: the pictures are shared, not copied).
    #[must_use]
    pub fn snapshot(&self) -> BoardData {
        self.data.clone()
    }

    #[must_use]
    pub const fn data(&self) -> &BoardData {
        &self.data
    }

    /// The items, the one on top last.
    #[must_use]
    pub fn items(&self) -> &[Item] {
        &self.data.items
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.data.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.items.is_empty()
    }

    #[must_use]
    pub fn get(&self, id: ItemId) -> Option<&Item> {
        self.data.items.iter().find(|i| i.id() == id)
    }

    #[must_use]
    pub fn cutout(&self, id: ItemId) -> Option<&Cutout> {
        match self.get(id) {
            Some(Item::Cutout(c)) => Some(c),
            _ => None,
        }
    }

    #[must_use]
    pub fn arrow(&self, id: ItemId) -> Option<&Arrow> {
        match self.get(id) {
            Some(Item::Arrow(a)) => Some(a),
            _ => None,
        }
    }

    pub fn cutouts(&self) -> impl Iterator<Item = &Cutout> {
        self.data.items.iter().filter_map(|i| match i {
            Item::Cutout(c) => Some(c),
            Item::Arrow(_) => None,
        })
    }

    pub fn arrows(&self) -> impl Iterator<Item = &Arrow> {
        self.data.items.iter().filter_map(|i| match i {
            Item::Arrow(a) => Some(a),
            Item::Cutout(_) => None,
        })
    }

    fn cutout_mut(&mut self, id: ItemId) -> Option<&mut Cutout> {
        self.data.items.iter_mut().find_map(|i| match i {
            Item::Cutout(c) if c.id == id => Some(c),
            _ => None,
        })
    }

    /// The pixels of every picture on the board.
    #[must_use]
    pub fn picture_pixels(&self) -> u64 {
        self.cutouts()
            .map(|c| match &c.image {
                ImageSlot::Ready(img) => img.pixels(),
                _ => 0,
            })
            .sum()
    }

    // ---- the view and what has to be saved

    #[must_use]
    pub const fn view(&self) -> View {
        self.data.view
    }

    /// Shows another part of the board. The view is kept with the board, but moving it is no content:
    /// it asks for no save of its own ([`Self::view_unsaved`]).
    pub fn set_view(&mut self, view: View) {
        let view = self.sane_view(view);
        if view != self.data.view {
            self.data.view = view;
            self.view_revision += 1;
        }
    }

    fn sane_view(&self, v: View) -> View {
        let zoom = if v.zoom.is_nan() { 1.0 } else { v.zoom.clamp(self.limits.zoom_min, self.limits.zoom_max) };
        View { pan: self.sane_pt(v.pan), zoom }
    }

    /// Counts the changes to the items and the pictures: what a view draws is new when it moved.
    #[must_use]
    pub const fn content_revision(&self) -> u64 {
        self.revision
    }

    /// Whether the items changed since the board was last kept.
    #[must_use]
    pub const fn needs_save(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// Whether only the view changed since: worth keeping at the next save, not worth a save.
    #[must_use]
    pub const fn view_unsaved(&self) -> bool {
        self.view_revision != self.saved_view_revision
    }

    /// The board was kept as it is now.
    pub const fn mark_saved(&mut self) {
        self.saved_revision = self.revision;
        self.saved_view_revision = self.view_revision;
    }

    /// The board as it was when its revisions were `content` and `view` was kept (a save built then
    /// and written since): what changed after is still to be kept.
    pub const fn mark_saved_at(&mut self, content: u64, view: u64) {
        self.saved_revision = if content < self.revision { content } else { self.revision };
        self.saved_view_revision = if view < self.view_revision { view } else { self.view_revision };
    }

    /// Counts the changes to the view.
    #[must_use]
    pub const fn view_revision(&self) -> u64 {
        self.view_revision
    }

    /// The board a file holds replaces this one (the history starts afresh, the observer and the limits
    /// stay); what was left out is the notices.
    pub fn replace_with(&mut self, data: BoardData) -> Vec<Notice> {
        let (loaded, notices) = Self::from_data(data, self.limits);
        self.data = loaded.data;
        self.history.clear();
        self.future.clear();
        self.live = None;
        self.revision += 1;
        // What was just read is what is on disk, unless it had to be repaired.
        self.saved_revision = if notices.is_empty() { self.revision } else { self.revision - 1 };
        self.view_revision += 1;
        self.saved_view_revision = self.view_revision;
        notices
    }

    /// Points cutout `id` at another place of the code (a reference that was found again, [`crate::whiteboard_relocate`]):
    /// not a step of the board and not undone by Mod+Z - where the code is now is not the person's doing - but the board is
    /// no longer what is on disk. Whether there is such a cutout.
    pub fn set_source(&mut self, id: ItemId, source: &Source) -> bool {
        let limits = self.limits;
        let Some(Item::Cutout(c)) = self.data.items.iter_mut().find(|i| i.id() == id) else {
            return false;
        };
        let source = source.sanitized(&limits);
        if c.source == source {
            return true;
        }
        c.source = source;
        self.revision += 1;
        true
    }

    /// The board as it is is not what is on disk (it came from somewhere else): the next save writes it.
    pub const fn mark_unsaved(&mut self) {
        self.saved_revision = self.revision.wrapping_sub(1);
    }

    /// What the board has told since it was last asked.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        std::mem::take(&mut self.notices)
    }

    // ---- keeping inputs inside what the board can hold

    fn sane(&self, v: f32) -> f32 {
        if v.is_nan() {
            0.0
        } else {
            v.clamp(-self.limits.max_coord, self.limits.max_coord)
        }
    }

    fn sane_pt(&self, p: Pt) -> Pt {
        Pt::new(self.sane(p.x), self.sane(p.y))
    }

    /// `r` with its place pulled in and its size between the least and the greatest side.
    fn sane_rect(&self, r: Rect) -> Rect {
        let side = |v: f32| if v.is_nan() { self.limits.min_cutout } else { v.clamp(self.limits.min_cutout, self.limits.max_cutout) };
        Rect::new(self.sane(r.x), self.sane(r.y), side(r.w), side(r.h))
    }

    fn sane_end(&self, e: End) -> End {
        match e {
            End::Free(p) => End::Free(self.sane_pt(p)),
            End::Item { id, anchor: Anchor::Point { u, v } } => End::Item {
                id,
                anchor: Anchor::Point {
                    u: if u.is_nan() { 0.5 } else { u.clamp(0.0, 1.0) },
                    v: if v.is_nan() { 0.5 } else { v.clamp(0.0, 1.0) },
                },
            },
            other => other,
        }
    }

    /// `text` cut to a label's length; the notice is kept when it was cut.
    fn label_of(&mut self, text: &str) -> String {
        let limit = self.limits.max_label_chars;
        if text.chars().count() > limit {
            self.notices.push(Notice::LabelShortened { limit });
            text.chars().take(limit).collect()
        } else {
            text.to_string()
        }
    }

    /// `size` fitted between the least and greatest side with its shape kept.
    fn fit_size(&self, size: Size) -> Size {
        let (min, max) = (self.limits.min_cutout, self.limits.max_cutout);
        let (w, h) = (if size.w.is_finite() { size.w.max(1.0) } else { min }, if size.h.is_finite() { size.h.max(1.0) } else { min });
        let mut k = 1.0_f32;
        if w.max(h) > max {
            k = max / w.max(h);
        }
        if w.min(h) * k < min {
            k = min / w.min(h);
        }
        Size::new((w * k).clamp(min, max), (h * k).clamp(min, max))
    }

    // ---- steps

    /// Does `edit` as a step a person can take back.
    fn step(&mut self, edit: Edit) {
        self.commit_live();
        let inverse = self.apply(edit);
        self.record(inverse);
        self.future.clear();
    }

    fn record(&mut self, inverse: Edit) {
        self.history.push(inverse);
        if self.history.len() > self.limits.history {
            self.history.remove(0);
        }
    }

    /// Applies `edit`; what it returns puts things back.
    fn apply(&mut self, edit: Edit) -> Edit {
        self.revision += 1;
        match edit {
            Edit::Insert(items) => {
                let ids = items.iter().map(|(_, item)| item.id()).collect();
                for (index, item) in items {
                    self.data.items.insert(index.min(self.data.items.len()), item);
                }
                Edit::Delete(ids)
            }
            Edit::Delete(ids) => {
                let mut removed = Vec::new();
                let mut kept = Vec::with_capacity(self.data.items.len());
                for (index, item) in std::mem::take(&mut self.data.items).into_iter().enumerate() {
                    if ids.contains(&item.id()) {
                        removed.push((index, item));
                    } else {
                        kept.push(item);
                    }
                }
                self.data.items = kept;
                Edit::Insert(removed)
            }
            Edit::SetRect { id, rect } => {
                let was = self.cutout_mut(id).map_or(rect, |c| std::mem::replace(&mut c.rect, rect));
                Edit::SetRect { id, rect: was }
            }
            Edit::SetLabel { id, label } => {
                let mut label = label;
                if let Some(item) = self.data.items.iter_mut().find(|i| i.id() == id) {
                    std::mem::swap(item.label_mut(), &mut label);
                }
                Edit::SetLabel { id, label }
            }
            Edit::SetArrow(arrow) => {
                let id = arrow.id;
                let mut arrow = arrow;
                for item in &mut self.data.items {
                    if let Item::Arrow(a) = item {
                        if a.id == id {
                            std::mem::swap(a, &mut arrow);
                            break;
                        }
                    }
                }
                Edit::SetArrow(arrow)
            }
        }
    }

    /// Takes the last step back; whether there was one.
    pub fn undo(&mut self) -> bool {
        self.commit_live();
        let Some(inverse) = self.history.pop() else {
            return false;
        };
        let redo = self.apply(inverse);
        self.future.push(redo);
        self.emit(&BoardEvent::Undone);
        true
    }

    /// Brings the step taken back again; whether there was one.
    pub fn redo(&mut self) -> bool {
        self.commit_live();
        let Some(redo) = self.future.pop() else {
            return false;
        };
        let inverse = self.apply(redo);
        self.history.push(inverse);
        self.emit(&BoardEvent::Redone);
        true
    }

    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }

    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }

    fn take_id(&mut self) -> ItemId {
        let id = self.data.next_id;
        self.data.next_id += 1;
        id
    }

    // ---- commands

    /// Puts a cutout on the board, on top.
    ///
    /// # Errors
    /// The board is full, the picture is too large or does not fit its size: nothing is added.
    pub fn add_cutout(&mut self, draft: CutoutDraft) -> Result<ItemId, Notice> {
        if self.len() >= self.limits.max_items {
            return Err(Notice::TooManyItems { limit: self.limits.max_items });
        }
        let natural = match &draft.image {
            ImageSlot::Ready(img) => {
                if !img.is_consistent() {
                    return Err(Notice::PictureInvalid);
                }
                if img.pixels() > self.limits.max_image_px {
                    return Err(Notice::PictureTooBig { px: img.pixels(), limit_px: self.limits.max_image_px });
                }
                if self.picture_pixels() + img.pixels() > self.limits.max_total_px {
                    return Err(Notice::PicturesTooBig { limit_px: self.limits.max_total_px });
                }
                img.logical_size()
            }
            _ => draft.size.unwrap_or(Size::new(200.0, 100.0)),
        };
        let size = self.fit_size(natural);
        let at = self.sane_pt(draft.at);
        let label = self.label_of(&draft.label);
        let source = draft.source.sanitized(&self.limits);
        let id = self.take_id();
        let event = BoardEvent::CutoutCreated {
            id,
            file: source.file.clone(),
            first_line: source.first_line,
            last_line: source.last_line,
        };
        let item = Item::Cutout(Cutout {
            id,
            rect: Rect::new(at.x, at.y, size.w, size.h),
            image: draft.image,
            label,
            source,
        });
        let index = self.data.items.len();
        self.step(Edit::Insert(vec![(index, item)]));
        self.emit(&event);
        Ok(id)
    }

    /// Puts an arrow on the board.
    ///
    /// # Errors
    /// An end hangs on a cutout that is not there, both ends are on the same cutout or both are
    /// loose, an arrow like it is there, or the board is full.
    pub fn add_arrow(&mut self, from: End, to: End, bend: f32, label: &str) -> Result<ItemId, Notice> {
        if self.len() >= self.limits.max_items {
            return Err(Notice::TooManyItems { limit: self.limits.max_items });
        }
        let (from, to) = (self.sane_end(from), self.sane_end(to));
        self.check_ends(&from, &to, None)?;
        let label = self.label_of(label);
        let id = self.take_id();
        let event = BoardEvent::ArrowDrawn { id, from: from.item_id(), to: to.item_id() };
        let arrow = Arrow { id, from, to, bend: sane_bend(bend), label };
        let index = self.data.items.len();
        self.step(Edit::Insert(vec![(index, Item::Arrow(arrow))]));
        self.emit(&event);
        Ok(id)
    }

    /// Whether an arrow from `from` to `to` may be on the board (`own`: the arrow being changed, which
    /// is not "another arrow like it").
    fn check_ends(&self, from: &End, to: &End, own: Option<ItemId>) -> Result<(), Notice> {
        for end in [from, to] {
            if let Some(id) = end.item_id() {
                if self.cutout(id).is_none() {
                    return Err(Notice::NoSuchItem { id });
                }
            }
        }
        validate_ends(from, to)?;
        if self.arrows().any(|a| Some(a.id) != own && a.from == *from && a.to == *to) {
            return Err(Notice::ArrowRepeated);
        }
        Ok(())
    }

    /// Gives cutout `id` its picture (a Thread drew it): not a step. It finds the cutout also while the
    /// cutout is taken back, so that it comes again with its picture; whether it was found.
    pub fn set_image(&mut self, id: ItemId, slot: ImageSlot) -> bool {
        if let Some(c) = self.cutout_mut(id) {
            c.image = slot;
            self.revision += 1;
            return true;
        }
        for edit in self.history.iter_mut().chain(self.future.iter_mut()) {
            if let Edit::Insert(items) = edit {
                for (_, item) in items {
                    if let Item::Cutout(c) = item {
                        if c.id == id {
                            c.image = slot;
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Takes item `id` off the board: a cutout with the arrows that hang on it, in one step. The ids
    /// that went, in the order of the board.
    ///
    /// # Errors
    /// There is no such item.
    pub fn remove(&mut self, id: ItemId) -> Result<Vec<ItemId>, Notice> {
        let Some(item) = self.get(id) else {
            return Err(Notice::NoSuchItem { id });
        };
        let is_cutout = matches!(item, Item::Cutout(_));
        let gone: Vec<ItemId> = self
            .data
            .items
            .iter()
            .filter(|i| {
                i.id() == id
                    || (is_cutout && matches!(i, Item::Arrow(a) if a.from.item_id() == Some(id) || a.to.item_id() == Some(id)))
            })
            .map(Item::id)
            .collect();
        self.step(Edit::Delete(gone.clone()));
        self.emit(&BoardEvent::Removed { ids: gone.clone() });
        Ok(gone)
    }

    /// Gives item `id` a label (cut to the length a label may have). The same label is no step.
    ///
    /// # Errors
    /// There is no such item.
    pub fn set_label(&mut self, id: ItemId, text: &str) -> Result<(), Notice> {
        let Some(current) = self.get(id).map(|i| match i {
            Item::Cutout(c) => c.label.clone(),
            Item::Arrow(a) => a.label.clone(),
        }) else {
            return Err(Notice::NoSuchItem { id });
        };
        let label = self.label_of(text);
        if label == current {
            return Ok(());
        }
        self.step(Edit::SetLabel { id, label });
        self.emit(&BoardEvent::Labelled { id });
        Ok(())
    }

    /// Fastens one end of arrow `arrow` somewhere else.
    ///
    /// # Errors
    /// There is no such arrow, the new end hangs on a cutout that is not there, or the arrow would be
    /// one of those an arrow cannot be ([`Self::add_arrow`]).
    pub fn rewire(&mut self, arrow: ItemId, which: Which, end: End) -> Result<(), Notice> {
        let Some(old) = self.arrow(arrow).cloned() else {
            return Err(Notice::NoSuchItem { id: arrow });
        };
        let end = self.sane_end(end);
        let mut new = old.clone();
        match which {
            Which::From => new.from = end,
            Which::To => new.to = end,
        }
        self.check_ends(&new.from, &new.to, Some(arrow))?;
        if new == old {
            return Ok(());
        }
        self.step(Edit::SetArrow(new));
        self.emit(&BoardEvent::ArrowChanged { id: arrow });
        Ok(())
    }

    /// Bends arrow `arrow` ([`Curve::bent`]).
    ///
    /// # Errors
    /// There is no such arrow.
    pub fn set_bend(&mut self, arrow: ItemId, bend: f32) -> Result<(), Notice> {
        let Some(old) = self.arrow(arrow).cloned() else {
            return Err(Notice::NoSuchItem { id: arrow });
        };
        let mut new = old.clone();
        new.bend = sane_bend(bend);
        if new == old {
            return Ok(());
        }
        self.step(Edit::SetArrow(new));
        self.emit(&BoardEvent::ArrowChanged { id: arrow });
        Ok(())
    }

    /// Puts cutout `id` on top of the others. No step: it is not a change to what the board shows.
    pub fn raise(&mut self, id: ItemId) {
        let Some(index) = self.data.items.iter().position(|i| i.id() == id && matches!(i, Item::Cutout(_))) else {
            return;
        };
        if index + 1 == self.data.items.len() {
            return;
        }
        let item = self.data.items.remove(index);
        self.data.items.push(item);
        self.revision += 1;
    }

    // ---- a drag

    fn begin_live(&mut self, id: ItemId, resizing: bool) -> Option<Rect> {
        let now = self.cutout(id)?.rect;
        match self.live {
            Some(l) if l.id == id && l.resizing == resizing => {}
            Some(_) => {
                self.commit_live();
                self.live = Some(Live { id, from: now, resizing });
            }
            None => self.live = Some(Live { id, from: now, resizing }),
        }
        Some(now)
    }

    /// Moves cutout `id` so its top-left corner is at `to`, as the pointer drags it: no step yet
    /// ([`Self::commit_live`]).
    pub fn move_live(&mut self, id: ItemId, to: Pt) {
        let to = self.sane_pt(to);
        let Some(now) = self.begin_live(id, false) else {
            return;
        };
        if let Some(c) = self.cutout_mut(id) {
            c.rect = Rect::new(to.x, to.y, now.w, now.h);
            self.revision += 1;
        }
    }

    /// Gives cutout `id` the place and size `to` (kept inside the size limits), as the pointer
    /// resizes it: no step yet.
    pub fn resize_live(&mut self, id: ItemId, to: Rect) {
        let rect = self.sane_rect(to);
        if self.begin_live(id, true).is_none() {
            return;
        }
        if let Some(c) = self.cutout_mut(id) {
            c.rect = rect;
            self.revision += 1;
        }
    }

    /// Ends the drag: when the cutout is not where it began, the drag is one step. Whether it was.
    pub fn commit_live(&mut self) -> bool {
        let Some(live) = self.live.take() else {
            return false;
        };
        let Some(now) = self.cutout(live.id).map(|c| c.rect) else {
            return false;
        };
        if now == live.from {
            return false;
        }
        self.record(Edit::SetRect { id: live.id, rect: live.from });
        self.future.clear();
        self.emit(&if live.resizing {
            BoardEvent::CutoutResized { id: live.id }
        } else {
            BoardEvent::CutoutMoved { id: live.id }
        });
        true
    }

    /// Ends the drag by putting the cutout back where it was.
    pub fn cancel_live(&mut self) {
        if let Some(live) = self.live.take() {
            if let Some(c) = self.cutout_mut(live.id) {
                c.rect = live.from;
                self.revision += 1;
            }
        }
    }

    /// The app went to the lines of cutout `id`: the observer hears it.
    pub fn note_jump(&mut self, id: ItemId, file: &str, first_line: u32, last_line: u32) {
        self.emit(&BoardEvent::Jumped { id, file: file.to_string(), first_line, last_line });
    }

    // ---- where an arrow is

    /// Where arrow `id` is drawn now: from the edge of its first end to the edge of its second. The
    /// ends that hang on cutouts follow them.
    #[must_use]
    pub fn place_arrow(&self, id: ItemId) -> Option<Placed> {
        let arrow = self.arrow(id)?;
        place(arrow, &|cutout| self.cutout(cutout).map(|c| c.rect))
    }

    /// Every arrow where it is drawn now, in the order of the board (an arrow whose ends are gone is left out): what
    /// [`Self::place_arrow`] says for each, with the cutouts looked up once for all of them - a frame of a board of
    /// hundreds asks for every arrow, and a search of the items for each end of each would be most of the frame.
    #[must_use]
    pub fn placed_arrows(&self) -> Vec<(ItemId, Placed)> {
        let rects: HashMap<ItemId, Rect> = self
            .data
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Cutout(c) => Some((c.id, c.rect)),
                Item::Arrow(_) => None,
            })
            .collect();
        let rect_of = |id: ItemId| rects.get(&id).copied();
        self.data
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Arrow(a) => place(a, &rect_of).map(|p| (a.id, p)),
                Item::Cutout(_) => None,
            })
            .collect()
    }
}

/// Where `arrow` is drawn, the cutouts' places given by `rect_of`.
fn place(arrow: &Arrow, rect_of: &dyn Fn(ItemId) -> Option<Rect>) -> Option<Placed> {
    let aim_from = aim(rect_of, &arrow.from)?;
    let aim_to = aim(rect_of, &arrow.to)?;
    let from = end_point(rect_of, &arrow.from, aim_to)?;
    let to = end_point(rect_of, &arrow.to, aim_from)?;
    Some(Placed { from, to, curve: Curve::bent(from, to, arrow.bend) })
}

/// What the other end of an arrow aims at: the loose point, the fastening, or the middle.
fn aim(rect_of: &dyn Fn(ItemId) -> Option<Rect>, end: &End) -> Option<Pt> {
    match end {
        End::Free(p) => Some(*p),
        End::Item { id, anchor } => {
            let r = rect_of(*id)?;
            Some(match anchor {
                Anchor::Auto => r.center(),
                Anchor::Side(s) => side_mid(&r, *s),
                Anchor::Point { u, v } => Pt::new(r.x + r.w * u, r.y + r.h * v),
            })
        }
    }
}

/// Where `end` is drawn when the other end is at `toward`.
fn end_point(rect_of: &dyn Fn(ItemId) -> Option<Rect>, end: &End, toward: Pt) -> Option<Pt> {
    match end {
        End::Free(p) => Some(*p),
        End::Item { id, anchor: Anchor::Auto } => Some(ray_exit(&rect_of(*id)?, toward)),
        End::Item { .. } => aim(rect_of, end),
    }
}

/// What an arrow's two ends may be, apart from whether the cutouts are there.
fn validate_ends(from: &End, to: &End) -> Result<(), Notice> {
    match (from.item_id(), to.item_id()) {
        (None, None) => Err(Notice::ArrowNeedsACutout),
        (Some(a), Some(b)) if a == b => Err(Notice::ArrowToItself),
        _ => Ok(()),
    }
}

/// A bend a curve can have: not a number is straight, the rest is kept inside `MAX_BEND`.
fn sane_bend(bend: f32) -> f32 {
    if bend.is_nan() {
        0.0
    } else {
        bend.clamp(-MAX_BEND, MAX_BEND)
    }
}

#[cfg(test)]
#[path = "whiteboard_test.rs"]
mod whiteboard_test;
