//! What the phone and AzReview say to each other: one enum of frames, the same in both directions (each side
//! refuses the frames that only the other may send). The encoding is `wire.rs`; the transport is `link.rs`.
//!
//! The numbers on the page are FRACTIONS OF THE PAGE (0 = the left / top edge, 1 = the right / bottom edge), so
//! the phone needs to know nothing of the Mac's window, its zoom model or its pixels, and AzReview needs to know
//! nothing of the phone's screen.

use crate::limits::{clean_text, Capped, BUTTON_COUNT, MAX_BUTTON_TEXT_CHARS, MAX_LABEL_CHARS, MAX_PATH_BYTES};
use crate::link::Delivery;

/// The newest-wins track of the pen state.
pub const TRACK_PEN: u8 = 1;
/// The newest-wins track of the viewport gesture updates.
pub const TRACK_VIEWPORT: u8 = 2;

/// Who sent a hello.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// AzReview on the Mac.
    Host,
    /// The companion on the phone.
    Phone,
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

/// What the pen is looking at: sent at most 30 times a second, the newest replaces one that has not left.
#[derive(Debug, Clone, PartialEq)]
pub struct PenState {
    /// Counts up; a phone ignores a state older than the newest it has.
    pub seq: u32,
    /// The workspace folder's name.
    pub folder: String,
    /// The file under review, relative to the workspace (empty: none open).
    pub file: String,
    /// The page in view, from 0.
    pub page_index: u32,
    pub page_count: u32,
    /// The line under the pen, from 1 (0: none).
    pub line: u32,
    pub line_count: u32,
    /// The page's size in the Mac's pixels (the phone only needs its shape; 0: unknown).
    pub page_w: f32,
    pub page_h: f32,
    /// The part of the page the Mac shows now.
    pub view: Rect,
    /// 1.0 is the page fitted to the width of the Mac's window.
    pub zoom: f32,
    /// Where the pen is (`None`: it is away from the page).
    pub pen: Option<PenMark>,
    /// The thumbnail that shows this page (0: none); it arrives as a `Thumbnail` frame.
    pub thumb_id: u32,
    pub context: Context,
}

impl PenState {
    /// A state with nothing open.
    #[must_use]
    pub fn empty() -> Self {
        PenState {
            seq: 0,
            folder: String::new(),
            file: String::new(),
            page_index: 0,
            page_count: 0,
            line: 0,
            line_count: 0,
            page_w: 0.0,
            page_h: 0.0,
            view: Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 },
            zoom: 1.0,
            pen: None,
            thumb_id: 0,
            context: Context::Page,
        }
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

/// A viewport command with its place in the phone's order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportCmd {
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
    OpenFile { request_id: u32, path: String, line: Option<u32> },
    Button { request_id: u32, index: u8 },
    SetButtons { request_id: u32, buttons: ButtonSet },
    NeedThumbnail { id: u32 },
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
            Frame::PairAccepted { .. } => "pair accepted",
            Frame::PairRejected(_) => "pair rejected",
            Frame::Refused(_) => "refused",
            Frame::PenState(_) => "pen state",
            Frame::Thumbnail(_) => "thumbnail",
            Frame::Rows(_) => "rows",
            Frame::Buttons(_) => "buttons",
            Frame::Reply(_) => "reply",
            Frame::Notice(_) => "notice",
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
    let (their_update, our_update) = match our_role {
        Role::Host => (Refusal::UpdateCompanion, Refusal::UpdateAzReview),
        Role::Phone => (Refusal::UpdateAzReview, Refusal::UpdateCompanion),
    };
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
