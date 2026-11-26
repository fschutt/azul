//! What the phone and AzReview say to each other: one enum of frames, the same in both directions (each side
//! refuses the frames that only the other may send). The encoding is `wire.rs`; the transport is `link.rs`.
//!
//! The numbers on the page are FRACTIONS OF THE PAGE (0 = the left / top edge, 1 = the right / bottom edge), so
//! the phone needs to know nothing of the Mac's window, its zoom model or its pixels, and AzReview needs to know
//! nothing of the phone's screen.

use crate::access::ScreenId;
use crate::limits::{clean_text, Capped, BUTTON_COUNT, MAX_BUTTON_TEXT_CHARS, MAX_LABEL_CHARS, MAX_PATH_BYTES};
use crate::link::Delivery;

/// The newest-wins track of the pen state.
pub const TRACK_PEN: u8 = 1;
/// The newest-wins track of the viewport gesture updates.
pub const TRACK_VIEWPORT: u8 = 2;

/// What a peer is for: the role of the screen or the device that says hello. The numbers are the wire's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// AzReview as a node that takes commands (the Mac the phone is paired with).
    Host,
    /// The companion on the phone: a remote control.
    Phone,
    /// The code view of AzReview.
    Review,
    /// A second code view of the same session (more code space on a big display).
    ReviewMirror,
    /// The architecture whiteboard of AzReview, popped out.
    Whiteboard,
}

impl Role {
    /// Whether this is the phone's companion (the others are windows or nodes of AzReview).
    #[must_use]
    pub fn is_companion(self) -> bool {
        self == Role::Phone
    }

    /// The role's number on the wire.
    #[must_use]
    pub fn tag(self) -> u8 {
        match self {
            Role::Host => 0,
            Role::Phone => 1,
            Role::Review => 2,
            Role::ReviewMirror => 3,
            Role::Whiteboard => 4,
        }
    }

    /// The role of a wire number (`None`: no such role).
    #[must_use]
    pub fn from_tag(tag: u8) -> Option<Role> {
        Some(match tag {
            0 => Role::Host,
            1 => Role::Phone,
            2 => Role::Review,
            3 => Role::ReviewMirror,
            4 => Role::Whiteboard,
            _ => return None,
        })
    }
}

/// The first frame each side sends.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hello {
    pub role: Role,
    /// The newest protocol the sender speaks.
    pub protocol: u16,
    /// The oldest protocol the sender still understands.
    pub min_protocol: u16,
    pub app_version: String,
    /// The Mac's or the phone's name, for the other side's screen.
    pub name: String,
    /// The host's current dialing string (its addresses change; the phone keeps the newest). Empty from a phone.
    pub ticket: String,
}

/// Why the Mac did not pair a phone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairReject {
    /// The proof of the one-time secret was wrong.
    WrongSecret,
    /// The offer is spent or its window is over.
    Expired,
    /// No pairing is open on the Mac.
    NoPairingOpen,
    /// Another phone is in the middle of pairing.
    Busy,
    /// Four phones are paired already.
    TooManyPhones,
    /// The user on the Mac said no.
    Declined,
}

/// Why the Mac closed a connection at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// The Mac does not know this phone (never paired, or forgotten).
    NotPaired,
    /// The phone speaks a protocol older than the Mac understands: update the companion from AzReview.
    UpdateCompanion,
    /// The Mac speaks a protocol older than the phone understands: update AzReview.
    UpdateAzReview,
    /// The peer says it is something this side does not take commands from (a role it does not serve).
    UnsupportedRole,
}

/// A rectangle in fractions of the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Where the pen (or the mouse cursor) is on the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PenMark {
    pub x: f32,
    pub y: f32,
    /// The tip is on the surface (drawing), not hovering.
    pub down: bool,
}

/// What a quick button would act on right now (AzReview's rule: the marked region, else the line under the pen,
/// else the page).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Context {
    Page,
    Line,
    Region,
}

/// The pages of the code review, as the Mac lays them out: a column of sheets with a break between two.
///
/// THE UNITS of every surface are widths of the surface: `x` runs 0 to 1 across a sheet's width, `y` runs down from the top
/// of the first sheet in the same unit, so a square on the page is a square in the numbers. A sheet is `1 / aspect` high,
/// sheet `i` starts at `i * (1 / aspect + gap)` and the break after it is `gap` high.
#[derive(Debug, Clone, PartialEq)]
pub struct PageSurface {
    /// The file under review, relative to the workspace (empty: none open).
    pub file: String,
    /// The page in view, from 0 (the one the middle of the view is on).
    pub page_index: u32,
    pub page_count: u32,
    /// The line under the pen, from 1 (0: none).
    pub line: u32,
    pub line_count: u32,
    /// A sheet's width over its height.
    pub aspect: f32,
    /// The break between two sheets, in sheet widths.
    pub gap: f32,
}

impl PageSurface {
    /// Nothing open: one A4-shaped sheet.
    #[must_use]
    pub fn empty() -> Self {
        PageSurface { file: String::new(), page_index: 0, page_count: 0, line: 0, line_count: 0, aspect: 794.0 / 1123.0, gap: 0.04 }
    }

    /// A sheet's height (a shape that is no shape is an A4 sheet's).
    #[must_use]
    pub fn page_height(&self) -> f32 {
        if self.aspect.is_finite() && self.aspect > 0.0 {
            1.0 / self.aspect
        } else {
            1123.0 / 794.0
        }
    }

    /// The break between two sheets (never negative).
    #[must_use]
    pub fn break_height(&self) -> f32 {
        if self.gap.is_finite() {
            self.gap.max(0.0)
        } else {
            0.0
        }
    }

    /// The height of the whole column (a column of no pages is one sheet).
    #[must_use]
    pub fn column_height(&self) -> f32 {
        let count = self.page_count.max(1) as f32;
        count * self.page_height() + (count - 1.0) * self.break_height()
    }

    /// Where sheet `index` starts down the column.
    #[must_use]
    pub fn page_top(&self, index: u32) -> f32 {
        index as f32 * (self.page_height() + self.break_height())
    }
}

/// The architecture whiteboard, as the pen sees it. Its units are the same as the pages': the box round the items is `x` 0
/// to 1 and `y` 0 to `1 / aspect` (the Mac turns its own coordinates into these).
#[derive(Debug, Clone, PartialEq)]
pub struct WhiteboardSurface {
    pub board_id: u32,
    /// The board's name, for the header.
    pub name: String,
    /// The box round everything on the board: its width over its height (1.0 for an empty board).
    pub aspect: f32,
    pub item_count: u32,
}

/// What the pen (or the cursor) is over: the pages of the code review, or the whiteboard.
#[derive(Debug, Clone, PartialEq)]
pub enum Surface {
    Page(PageSurface),
    Whiteboard(WhiteboardSurface),
}

/// Where the user was reviewing before jumping to an agent's conversation: the first jump saves it, "Continue reviewing" goes
/// back to it. AzReview keeps it in the session; the phone only shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnPoint {
    pub file: String,
    pub page_index: u32,
    pub line: u32,
    /// A few words to show ("fc.rs p3 l1337"); the phone makes its own when this is empty.
    pub label: String,
}

/// What the pen is looking at: sent at most 30 times a second, the newest replaces one that has not left.
#[derive(Debug, Clone, PartialEq)]
pub struct PenState {
    /// Counts up; a phone ignores a state older than the newest it has.
    pub seq: u32,
    /// The screen of AzReview this is the state of.
    pub screen: ScreenId,
    /// The workspace folder's name.
    pub folder: String,
    pub surface: Surface,
    /// The part of the surface the Mac shows now (in the surface's own units).
    pub view: Rect,
    /// 1.0 is the surface fitted to the width of the Mac's window (a sheet's width; the whole board).
    pub zoom: f32,
    /// Where the pen is on the surface (`None`: it is away from it).
    pub pen: Option<PenMark>,
    /// The thumbnail that shows this surface (0: none); it arrives as a `Thumbnail` frame. A page's picture for a
    /// page, the overview of its items for a board.
    pub thumb_id: u32,
    pub context: Context,
    /// Where "Continue reviewing" goes back to (`None`: nowhere).
    pub return_point: Option<ReturnPoint>,
}

impl PenState {
    /// A state with nothing open.
    #[must_use]
    pub fn empty() -> Self {
        PenState {
            seq: 0,
            screen: ScreenId::MAIN,
            folder: String::new(),
            surface: Surface::Page(PageSurface::empty()),
            view: Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
            zoom: 1.0,
            pen: None,
            thumb_id: 0,
            context: Context::Page,
            return_point: None,
        }
    }

    /// The pages, when the pen is over them.
    #[must_use]
    pub fn page(&self) -> Option<&PageSurface> {
        match &self.surface {
            Surface::Page(page) => Some(page),
            Surface::Whiteboard(_) => None,
        }
    }

    /// The whiteboard, when the pen is over it.
    #[must_use]
    pub fn board(&self) -> Option<&WhiteboardSurface> {
        match &self.surface {
            Surface::Whiteboard(board) => Some(board),
            Surface::Page(_) => None,
        }
    }

    /// The file under review (empty on a whiteboard, and when none is open).
    #[must_use]
    pub fn file(&self) -> &str {
        self.page().map_or("", |p| p.file.as_str())
    }
}

/// How the bytes of a thumbnail are to be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbFormat {
    /// `width * height * 4` bytes, straight alpha.
    Rgba8,
    Png,
    Jpeg,
}

/// A picture of the page, sent when the page changes (not with every pen state).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumbnail {
    pub id: u32,
    pub width: u16,
    pub height: u16,
    pub format: ThumbFormat,
    pub bytes: Vec<u8>,
}

/// A row of a listing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
}

/// A file's state as AzReview colours it: green nothing unresolved, yellow an unresolved suggestion, blue an agent
/// at work. A folder shows the state of what is in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileState {
    /// Not touched: no colour.
    None,
    Clean,
    Unresolved,
    Working,
}

/// One file or folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: EntryKind,
    pub state: FileState,
}

/// The answer to a `ListDir`: a page of a folder's rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rows {
    /// The request this answers.
    pub request_id: u32,
    /// The folder, relative to the workspace (empty: the workspace itself).
    pub dir: String,
    /// The first row's place in the folder (a folder longer than a page comes in pages).
    pub offset: u32,
    pub entries: Vec<Entry>,
    /// There are rows after these.
    pub more: bool,
}

/// One quick button: a short label on the key and the instruction it stands for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickButton {
    pub label: String,
    pub text: String,
}

/// The four quick buttons.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ButtonSet {
    pub buttons: [QuickButton; BUTTON_COUNT],
}

impl ButtonSet {
    /// The placeholders AzReview starts with and "Reset" on the phone goes back to (the user's own words replace them).
    #[must_use]
    pub fn defaults() -> Self {
        let b = |label: &str, text: &str| QuickButton { label: label.to_string(), text: text.to_string() };
        ButtonSet {
            buttons: [
                b("Tone", "Slop wording: please rewrite this in a normal, plain tone."),
                b("Explain", "Explain what this does, briefly."),
                b("Shorter", "Say this in fewer words."),
                b("Doc", "Add a short doc comment here."),
            ],
        }
    }

    /// The set with every label and text cleaned and cut to its cap, and what was cut.
    #[must_use]
    pub fn sanitized(&self) -> (ButtonSet, Vec<Capped>) {
        let mut caps = Vec::new();
        let mut out = self.clone();
        for button in &mut out.buttons {
            let (label, label_cap) = clean_text(button.label.trim(), MAX_LABEL_CHARS, false, "button label");
            let (text, text_cap) = clean_text(button.text.trim(), MAX_BUTTON_TEXT_CHARS, true, "button text");
            button.label = label.trim().to_string();
            button.text = text.trim().to_string();
            caps.extend(label_cap);
            caps.extend(text_cap);
        }
        (out, caps)
    }
}

/// How far a conversation a button started has got (AzReview's bottom bar: blue, yellow, green).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyStatus {
    /// AzReview has the instruction.
    Received,
    /// An agent is working on it (blue).
    Working,
    /// It suggested something that waits for the user (yellow).
    Suggested,
    /// Finished (green).
    Done,
    /// It could not be done.
    Failed,
}

/// A status for a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reply {
    pub request_id: u32,
    pub status: ReplyStatus,
}

/// How serious a notice is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeLevel {
    Info,
    Warning,
}

/// A line AzReview wants the phone to show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub level: NoticeLevel,
    pub text: String,
}

/// What the user settled on the Mac that the phone obeys. The phone keeps none of it: it arrives with every connection and
/// whenever it changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostSettings {
    /// A drag moves the Mac's viewport WITH the finger instead of the content (see `viewport::PanDirection`).
    pub reverse_pan: bool,
    /// A drag over the pages that ends near a break puts the top of a page at the top of the view.
    pub snap_to_page_top: bool,
    /// The phone buzzes when an agent finished with a result.
    pub haptic_done: bool,
    /// The phone buzzes when an agent finished with a suggestion that waits for the user.
    pub haptic_suggestion: bool,
    /// The phone buzzes when an agent could not answer (it is missing, its process failed, it timed out).
    pub haptic_failed: bool,
}

impl Default for HostSettings {
    fn default() -> Self {
        HostSettings { reverse_pan: false, snap_to_page_top: false, haptic_done: true, haptic_suggestion: true, haptic_failed: true }
    }
}

/// How an agent's conversation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishKind {
    /// It is done (green).
    Done,
    /// It suggested something that waits for the user (yellow).
    Suggestion,
    /// It found nothing to do.
    NoAction,
    /// It could not answer: the agent is missing, its process failed, it timed out.
    Failed,
}

/// The buzz the phone gives (the engine's haptic patterns of the same names).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Haptic {
    /// The agent is done.
    Success,
    /// The agent suggested something that waits for the user.
    Warning,
    /// The agent could not answer.
    Error,
}

impl HostSettings {
    /// The buzz for an agent that ended as `kind`, or none: a result buzzes as a success, a suggestion as a warning, an agent
    /// that could not answer as an error, an agent that found nothing to do never buzzes; each buzz can be switched off in the
    /// Mac's settings.
    #[must_use]
    pub fn haptic_for(&self, kind: FinishKind) -> Option<Haptic> {
        match kind {
            FinishKind::Done if self.haptic_done => Some(Haptic::Success),
            FinishKind::Suggestion if self.haptic_suggestion => Some(Haptic::Warning),
            FinishKind::Failed if self.haptic_failed => Some(Haptic::Error),
            _ => None,
        }
    }
}

/// An agent finished answering: the phone may buzz (success for `Done`, warning for `Suggestion`, nothing for `NoAction`)
/// and says where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFinished {
    pub conversation_id: u32,
    pub file: String,
    pub line: u32,
    pub kind: FinishKind,
}

/// How far a gesture has got.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Begin,
    Update,
    End,
}

/// Where a zoom is centred.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Focus {
    /// Around the pen (or the cursor) on the Mac: what the user asked for.
    Pen,
    /// Around the middle of what the Mac shows.
    Center,
    /// Around a point of the page (fractions).
    Point { x: f32, y: f32 },
}

/// What the phone asks the Mac's viewport to do.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportOp {
    /// A drag and / or a pinch. The numbers are CUMULATIVE since the gesture began (so a dropped update loses
    /// nothing: the newest says it all): `pan_*` is how far the viewport has moved over the page in page
    /// fractions (positive: toward the right / bottom), `zoom` the factor so far (above 1 zooms in).
    Gesture { id: u32, phase: Phase, pan_x: f32, pan_y: f32, zoom: f32, focus: Focus },
    /// Fit the page to the width of the window (double tap).
    Fit,
    /// The next (+1) or previous (-1) page.
    PageStep(i8),
    /// The default zoom at the top of the page.
    Reset,
}

/// A viewport command with the screen it is for and its place in the phone's order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCmd {
    pub screen: ScreenId,
    pub seq: u32,
    pub op: ViewportOp,
}

/// One message.
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    // ---- both ways
    Hello(Hello),
    Ping { nonce: u32 },
    Pong { nonce: u32 },
    Bye,
    // ---- phone to Mac
    /// The proof (HMAC-SHA256 of the offer's secret over both keys) and the phone's name.
    PairRequest { proof: [u8; 32], phone_name: String },
    Viewport(ViewportCmd),
    ListDir { request_id: u32, path: String, offset: u32 },
    OpenFile { screen: ScreenId, request_id: u32, path: String, line: Option<u32> },
    Button { screen: ScreenId, request_id: u32, index: u8 },
    SetButtons { request_id: u32, buttons: ButtonSet },
    NeedThumbnail { id: u32 },
    /// Go back to the return point of the screen (and clear it).
    ContinueReviewing { screen: ScreenId },
    // ---- Mac to phone
    /// The pairing went through; `code` is the six-digit number both screens show.
    PairAccepted { code: u32 },
    PairRejected(PairReject),
    Refused(Refusal),
    PenState(PenState),
    Thumbnail(Thumbnail),
    Rows(Rows),
    Buttons(ButtonSet),
    Reply(Reply),
    Notice(Notice),
    Settings(HostSettings),
    AgentFinished(AgentFinished),
}

impl Frame {
    /// What the frame is called in a log.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Frame::Hello(_) => "hello",
            Frame::Ping { .. } => "ping",
            Frame::Pong { .. } => "pong",
            Frame::Bye => "bye",
            Frame::PairRequest { .. } => "pair request",
            Frame::Viewport(_) => "viewport",
            Frame::ListDir { .. } => "list dir",
            Frame::OpenFile { .. } => "open file",
            Frame::Button { .. } => "button",
            Frame::SetButtons { .. } => "set buttons",
            Frame::NeedThumbnail { .. } => "need thumbnail",
            Frame::ContinueReviewing { .. } => "continue reviewing",
            Frame::PairAccepted { .. } => "pair accepted",
            Frame::PairRejected(_) => "pair rejected",
            Frame::Refused(_) => "refused",
            Frame::PenState(_) => "pen state",
            Frame::Thumbnail(_) => "thumbnail",
            Frame::Rows(_) => "rows",
            Frame::Buttons(_) => "buttons",
            Frame::Reply(_) => "reply",
            Frame::Notice(_) => "notice",
            Frame::Settings(_) => "settings",
            Frame::AgentFinished(_) => "agent finished",
        }
    }

    /// How the frame travels: the pen state and the gesture updates are newest-wins on their own tracks (a slow
    /// link drops the ones in between), everything else is reliable and in order.
    #[must_use]
    pub fn delivery(&self) -> Delivery {
        match self {
            Frame::PenState(_) => Delivery::Latest(TRACK_PEN),
            Frame::Viewport(ViewportCmd { op: ViewportOp::Gesture { phase: Phase::Update, .. }, .. }) => {
                Delivery::Latest(TRACK_VIEWPORT)
            }
            _ => Delivery::Reliable,
        }
    }

    /// Whether only the Mac may send this frame.
    #[must_use]
    pub fn is_host_to_phone(&self) -> bool {
        matches!(
            self,
            Frame::PairAccepted { .. }
                | Frame::PairRejected(_)
                | Frame::Refused(_)
                | Frame::PenState(_)
                | Frame::Thumbnail(_)
                | Frame::Rows(_)
                | Frame::Buttons(_)
                | Frame::Reply(_)
                | Frame::Notice(_)
                | Frame::Settings(_)
                | Frame::AgentFinished(_)
        )
    }

    /// Whether only a phone may send this frame.
    #[must_use]
    pub fn is_phone_to_host(&self) -> bool {
        matches!(
            self,
            Frame::PairRequest { .. }
                | Frame::Viewport(_)
                | Frame::ListDir { .. }
                | Frame::OpenFile { .. }
                | Frame::Button { .. }
                | Frame::SetButtons { .. }
                | Frame::NeedThumbnail { .. }
                | Frame::ContinueReviewing { .. }
        )
    }
}

/// Whether a path from a peer is one the Mac may look up: relative, no `..`, no NUL, no backslash tricks, not
/// longer than the cap.
#[must_use]
pub fn is_safe_relative_path(path: &str) -> bool {
    if path.len() > MAX_PATH_BYTES || path.chars().any(|c| c.is_control() || c == '\\') {
        return false;
    }
    if path.is_empty() {
        return true;
    }
    if path.starts_with('/') {
        return false;
    }
    let mut chars = path.chars();
    if let (Some(first), Some(':')) = (chars.next(), chars.next()) {
        if first.is_ascii_alphabetic() {
            return false;
        }
    }
    path.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != "..")
}

/// What two protocols make of each other: the version both speak, or who must update. `ours` is this side's
/// (newest, oldest), `our_role` which side that is.
///
/// # Errors
/// The [`Refusal`] to tell the user (and the peer): the companion or AzReview is too old.
pub fn negotiate(our_role: Role, ours: (u16, u16), theirs: &Hello) -> Result<u16, Refusal> {
    // whoever is too old must update its own app: the companion is updated from AzReview, AzReview from its own release
    let update_of = |role: Role| if role.is_companion() { Refusal::UpdateCompanion } else { Refusal::UpdateAzReview };
    let (their_update, our_update) = (update_of(theirs.role), update_of(our_role));
    if theirs.protocol < ours.1 {
        return Err(their_update);
    }
    if ours.0 < theirs.min_protocol {
        return Err(our_update);
    }
    Ok(ours.0.min(theirs.protocol))
}

#[cfg(test)]
#[path = "frame_test.rs"]
mod frame_test;
