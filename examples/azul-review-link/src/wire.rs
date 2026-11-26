//! The bytes of a frame. Hand-written, compact, little endian:
//! `[MAGIC][wire version][tag][payload]`, strings as `u16 length + UTF-8`, optional values as a `0 / 1` byte
//! first, floats as `f32` (a NaN or an infinity is refused), lists as `u16 count + items`.
//!
//! DECODING IS TOTAL: whatever the bytes are, [`decode`] returns a frame or a [`DecodeError`] and never panics,
//! never allocates more than the caps allow (the length of a list or a string is checked against its cap BEFORE
//! anything is reserved for it), and refuses what follows a complete frame. [`encode`] refuses to write what
//! [`decode`] would refuse.

use crate::access::ScreenId;
use crate::frame::{
    AgentFinished, ButtonSet, Context, Entry, EntryKind, FileState, FinishKind, Focus, Frame, Hello, HostSettings, Notice, NoticeLevel,
    PageSurface, PairReject, PenMark, PenState, Phase, QuickButton, Rect, Refusal, Reply, ReplyStatus, ReturnPoint, Role, Rows, Surface,
    ThumbFormat, Thumbnail, ViewportCmd, ViewportOp, WhiteboardSurface,
};
use crate::limits::{
    BUTTON_COUNT, MAGIC, MAX_ASPECT, MAX_BOARD_ITEMS, MAX_BUTTON_TEXT_CHARS, MAX_ENTRY_NAME_BYTES, MAX_FRAME_BYTES, MAX_LABEL_CHARS,
    MAX_NAME_BYTES, MAX_NOTICE_BYTES, MAX_PAGE_COUNT, MAX_PAGE_GAP, MAX_PATH_BYTES, MAX_ROWS, MAX_THUMBNAIL_BYTES,
    MAX_THUMBNAIL_SIDE, MAX_TICKET_BYTES, MAX_VERSION_BYTES, MIN_ASPECT,
};

/// The wire format's version, the second byte of every frame.
pub const WIRE_VERSION: u8 = 1;

/// The tag of each frame.
pub mod tag {
    pub const HELLO: u8 = 1;
    pub const PING: u8 = 2;
    pub const PONG: u8 = 3;
    pub const BYE: u8 = 4;
    pub const PAIR_REQUEST: u8 = 5;
    pub const VIEWPORT: u8 = 6;
    pub const LIST_DIR: u8 = 7;
    pub const OPEN_FILE: u8 = 8;
    pub const BUTTON: u8 = 9;
    pub const SET_BUTTONS: u8 = 10;
    pub const NEED_THUMBNAIL: u8 = 11;
    pub const CONTINUE_REVIEWING: u8 = 12;
    pub const PAIR_ACCEPTED: u8 = 16;
    pub const PAIR_REJECTED: u8 = 17;
    pub const REFUSED: u8 = 18;
    pub const PEN_STATE: u8 = 19;
    pub const THUMBNAIL: u8 = 20;
    pub const ROWS: u8 = 21;
    pub const BUTTONS: u8 = 22;
    pub const REPLY: u8 = 23;
    pub const NOTICE: u8 = 24;
    pub const SETTINGS: u8 = 25;
    pub const AGENT_FINISHED: u8 = 26;
}

/// Why bytes are not a frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// More bytes than a frame may have (checked before anything is read).
    TooLarge { len: usize, max: usize },
    /// The first byte is not the magic: not this protocol.
    BadMagic,
    /// A wire format this build does not speak.
    UnsupportedVersion { got: u8, ours: u8 },
    /// A tag no frame has.
    UnknownTag(u8),
    /// The bytes end inside `field`.
    Truncated { field: &'static str },
    /// `field` claims more than its cap.
    TooLong { field: &'static str, len: usize, max: usize },
    /// `field` is not UTF-8.
    BadUtf8 { field: &'static str },
    /// `field` holds a value it may not (a number that is no number, a kind that is none).
    BadValue { field: &'static str },
    /// A whole frame was read and bytes are left.
    Trailing { extra: usize },
}

impl core::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            DecodeError::TooLarge { len, max } => write!(f, "a frame of {len} bytes is over the limit of {max}"),
            DecodeError::BadMagic => write!(f, "these bytes are not an AzReview companion frame"),
            DecodeError::UnsupportedVersion { got, ours } => {
                write!(f, "a frame of wire version {got}; this build speaks {ours}: update the companion or AzReview")
            }
            DecodeError::UnknownTag(tag) => write!(f, "a frame of kind {tag}, which this build does not know"),
            DecodeError::Truncated { field } => write!(f, "the frame ends inside {field}"),
            DecodeError::TooLong { field, len, max } => write!(f, "{field} is {len}, over the limit of {max}"),
            DecodeError::BadUtf8 { field } => write!(f, "{field} is not text"),
            DecodeError::BadValue { field } => write!(f, "{field} holds a value it may not"),
            DecodeError::Trailing { extra } => write!(f, "{extra} bytes follow the end of the frame"),
        }
    }
}

/// Why a frame cannot be written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodeError {
    /// `field` is over its cap (the sender cuts it first: `ButtonSet::sanitized`, `cut_bytes`).
    TooLong { field: &'static str, len: usize, max: usize },
    /// `field` holds a value that cannot be written (a NaN, an infinity, a thumbnail that is not its size).
    BadValue { field: &'static str },
}

impl core::fmt::Display for EncodeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EncodeError::TooLong { field, len, max } => write!(f, "{field} is {len}, over the limit of {max}"),
            EncodeError::BadValue { field } => write!(f, "{field} holds a value that cannot be sent"),
        }
    }
}

/// The bytes of `frame`.
///
/// # Errors
/// A field over its cap or a value that cannot be written.
pub fn encode(frame: &Frame) -> Result<Vec<u8>, EncodeError> {
    let mut w = Writer::new(frame_tag(frame));
    match frame {
        Frame::Hello(h) => {
            w.u8(h.role.tag());
            w.u16(h.protocol);
            w.u16(h.min_protocol);
            w.string(&h.app_version, "app version", MAX_VERSION_BYTES)?;
            w.string(&h.name, "name", MAX_NAME_BYTES)?;
            w.string(&h.ticket, "ticket", MAX_TICKET_BYTES)?;
        }
        Frame::Ping { nonce } | Frame::Pong { nonce } => w.u32(*nonce),
        Frame::Bye => {}
        Frame::PairRequest { proof, phone_name } => {
            w.bytes(proof);
            w.string(phone_name, "phone name", MAX_NAME_BYTES)?;
        }
        Frame::Viewport(cmd) => {
            w.u32(cmd.screen.0);
            w.u32(cmd.seq);
            match cmd.op {
                ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus } => {
                    w.u8(0);
                    w.u32(id);
                    w.u8(match phase {
                        Phase::Begin => 0,
                        Phase::Update => 1,
                        Phase::End => 2,
                    });
                    w.finite(pan_x, "pan")?;
                    w.finite(pan_y, "pan")?;
                    w.finite(zoom, "zoom")?;
                    match focus {
                        Focus::Pen => w.u8(0),
                        Focus::Center => w.u8(1),
                        Focus::Point { x, y } => {
                            w.u8(2);
                            w.finite(x, "focus")?;
                            w.finite(y, "focus")?;
                        }
                    }
                }
                ViewportOp::Fit => w.u8(1),
                ViewportOp::PageStep(step) => {
                    w.u8(2);
                    w.u8(step.to_le_bytes()[0]);
                }
                ViewportOp::Reset => w.u8(3),
            }
        }
        Frame::ListDir { request_id, path, offset } => {
            w.u32(*request_id);
            w.string(path, "path", MAX_PATH_BYTES)?;
            w.u32(*offset);
        }
        Frame::OpenFile { screen, request_id, path, line } => {
            w.u32(screen.0);
            w.u32(*request_id);
            w.string(path, "path", MAX_PATH_BYTES)?;
            match line {
                Some(line) => {
                    w.u8(1);
                    w.u32(*line);
                }
                None => w.u8(0),
            }
        }
        Frame::Button { screen, request_id, index } => {
            if usize::from(*index) >= BUTTON_COUNT {
                return Err(EncodeError::BadValue { field: "button index" });
            }
            w.u32(screen.0);
            w.u32(*request_id);
            w.u8(*index);
        }
        Frame::SetButtons { request_id, buttons } => {
            w.u32(*request_id);
            w.buttons(buttons)?;
        }
        Frame::NeedThumbnail { id } => w.u32(*id),
        Frame::ContinueReviewing { screen } => w.u32(screen.0),
        Frame::PairAccepted { code } => {
            if *code > 999_999 {
                return Err(EncodeError::BadValue { field: "code" });
            }
            w.u32(*code);
        }
        Frame::PairRejected(reason) => w.u8(match reason {
            PairReject::WrongSecret => 0,
            PairReject::Expired => 1,
            PairReject::NoPairingOpen => 2,
            PairReject::Busy => 3,
            PairReject::TooManyPhones => 4,
            PairReject::Declined => 5,
        }),
        Frame::Refused(refusal) => w.u8(match refusal {
            Refusal::NotPaired => 0,
            Refusal::UpdateCompanion => 1,
            Refusal::UpdateAzReview => 2,
            Refusal::UnsupportedRole => 3,
        }),
        Frame::PenState(p) => {
            w.u32(p.seq);
            w.u32(p.screen.0);
            w.string(&p.folder, "folder", MAX_ENTRY_NAME_BYTES)?;
            w.rect(p.view, "view")?;
            w.finite(p.zoom, "zoom")?;
            match p.pen {
                Some(pen) => {
                    w.u8(1);
                    w.finite(pen.x, "pen")?;
                    w.finite(pen.y, "pen")?;
                    w.u8(u8::from(pen.down));
                }
                None => w.u8(0),
            }
            w.u32(p.thumb_id);
            w.u8(match p.context {
                Context::Page => 0,
                Context::Line => 1,
                Context::Region => 2,
            });
            match &p.return_point {
                Some(point) => {
                    w.u8(1);
                    w.string(&point.file, "return file", MAX_PATH_BYTES)?;
                    w.u32(point.page_index);
                    w.u32(point.line);
                    w.string(&point.label, "return label", MAX_NAME_BYTES)?;
                }
                None => w.u8(0),
            }
            match &p.surface {
                Surface::Page(page) => {
                    w.u8(0);
                    if page.page_count > MAX_PAGE_COUNT {
                        return Err(EncodeError::TooLong { field: "pages", len: page.page_count as usize, max: MAX_PAGE_COUNT as usize });
                    }
                    if !(MIN_ASPECT..=MAX_ASPECT).contains(&page.aspect) || !(0.0..=MAX_PAGE_GAP).contains(&page.gap) {
                        return Err(EncodeError::BadValue { field: "page shape" });
                    }
                    w.string(&page.file, "file", MAX_PATH_BYTES)?;
                    w.u32(page.page_index);
                    w.u32(page.page_count);
                    w.u32(page.line);
                    w.u32(page.line_count);
                    w.finite(page.aspect, "page shape")?;
                    w.finite(page.gap, "page shape")?;
                }
                Surface::Whiteboard(board) => {
                    w.u8(1);
                    if board.item_count > MAX_BOARD_ITEMS {
                        return Err(EncodeError::TooLong { field: "board items", len: board.item_count as usize, max: MAX_BOARD_ITEMS as usize });
                    }
                    if !(MIN_ASPECT..=MAX_ASPECT).contains(&board.aspect) {
                        return Err(EncodeError::BadValue { field: "board shape" });
                    }
                    w.u32(board.board_id);
                    w.string(&board.name, "board name", MAX_NAME_BYTES)?;
                    w.finite(board.aspect, "board shape")?;
                    w.u32(board.item_count);
                }
            }
        }
        Frame::Thumbnail(t) => {
            check_thumbnail(t.width, t.height, t.format, t.bytes.len()).map_err(|e| match e {
                DecodeError::TooLong { field, len, max } => EncodeError::TooLong { field, len, max },
                DecodeError::BadValue { field } => EncodeError::BadValue { field },
                _ => EncodeError::BadValue { field: "thumbnail" },
            })?;
            w.u32(t.id);
            w.u16(t.width);
            w.u16(t.height);
            w.u8(match t.format {
                ThumbFormat::Rgba8 => 0,
                ThumbFormat::Png => 1,
                ThumbFormat::Jpeg => 2,
            });
            w.u32(t.bytes.len() as u32);
            w.bytes(&t.bytes);
        }
        Frame::Rows(rows) => {
            if rows.entries.len() > MAX_ROWS {
                return Err(EncodeError::TooLong { field: "rows", len: rows.entries.len(), max: MAX_ROWS });
            }
            w.u32(rows.request_id);
            w.string(&rows.dir, "dir", MAX_PATH_BYTES)?;
            w.u32(rows.offset);
            w.u8(u8::from(rows.more));
            w.u16(rows.entries.len() as u16);
            for entry in &rows.entries {
                w.string(&entry.name, "entry name", MAX_ENTRY_NAME_BYTES)?;
                w.u8(match entry.kind {
                    EntryKind::Dir => 0,
                    EntryKind::File => 1,
                });
                w.u8(match entry.state {
                    FileState::None => 0,
                    FileState::Clean => 1,
                    FileState::Unresolved => 2,
                    FileState::Working => 3,
                });
            }
        }
        Frame::Buttons(buttons) => w.buttons(buttons)?,
        Frame::Reply(reply) => {
            w.u32(reply.request_id);
            w.u8(match reply.status {
                ReplyStatus::Received => 0,
                ReplyStatus::Working => 1,
                ReplyStatus::Suggested => 2,
                ReplyStatus::Done => 3,
                ReplyStatus::Failed => 4,
            });
        }
        Frame::Notice(notice) => {
            w.u8(match notice.level {
                NoticeLevel::Info => 0,
                NoticeLevel::Warning => 1,
            });
            w.string(&notice.text, "notice", MAX_NOTICE_BYTES)?;
        }
        Frame::Settings(settings) => w.u8(settings_bits(settings)),
        Frame::AgentFinished(done) => {
            w.u32(done.conversation_id);
            w.string(&done.file, "file", MAX_PATH_BYTES)?;
            w.u32(done.line);
            w.u8(match done.kind {
                FinishKind::Done => 0,
                FinishKind::Suggestion => 1,
                FinishKind::NoAction => 2,
                FinishKind::Failed => 3,
            });
        }
    }
    if w.buf.len() > MAX_FRAME_BYTES {
        return Err(EncodeError::TooLong { field: "frame", len: w.buf.len(), max: MAX_FRAME_BYTES });
    }
    Ok(w.buf)
}

/// The frame in `bytes`. Total: no input panics.
///
/// # Errors
/// Why the bytes are not a frame.
pub fn decode(bytes: &[u8]) -> Result<Frame, DecodeError> {
    if bytes.len() > MAX_FRAME_BYTES {
        return Err(DecodeError::TooLarge { len: bytes.len(), max: MAX_FRAME_BYTES });
    }
    let mut r = Reader { bytes, at: 0 };
    let magic = r.u8("header")?;
    let version = r.u8("header")?;
    let kind = r.u8("header")?;
    if magic != MAGIC {
        return Err(DecodeError::BadMagic);
    }
    if version != WIRE_VERSION {
        return Err(DecodeError::UnsupportedVersion { got: version, ours: WIRE_VERSION });
    }
    let frame = match kind {
        tag::HELLO => {
            let role = Role::from_tag(r.u8("role")?).ok_or(DecodeError::BadValue { field: "role" })?;
            let protocol = r.u16("protocol")?;
            let min_protocol = r.u16("protocol")?;
            Frame::Hello(Hello {
                role,
                protocol,
                min_protocol,
                app_version: r.string("app version", MAX_VERSION_BYTES)?,
                name: r.string("name", MAX_NAME_BYTES)?,
                ticket: r.string("ticket", MAX_TICKET_BYTES)?,
            })
        }
        tag::PING => Frame::Ping { nonce: r.u32("nonce")? },
        tag::PONG => Frame::Pong { nonce: r.u32("nonce")? },
        tag::BYE => Frame::Bye,
        tag::PAIR_REQUEST => {
            let mut proof = [0u8; 32];
            proof.copy_from_slice(r.take(32, "proof")?);
            Frame::PairRequest { proof, phone_name: r.string("phone name", MAX_NAME_BYTES)? }
        }
        tag::VIEWPORT => {
            let screen = ScreenId(r.u32("screen")?);
            let seq = r.u32("seq")?;
            let op = match r.u8("viewport op")? {
                0 => {
                    let id = r.u32("gesture")?;
                    let phase = match r.u8("phase")? {
                        0 => Phase::Begin,
                        1 => Phase::Update,
                        2 => Phase::End,
                        _ => return Err(DecodeError::BadValue { field: "phase" }),
                    };
                    let pan_x = r.finite("pan")?;
                    let pan_y = r.finite("pan")?;
                    let zoom = r.finite("zoom")?;
                    let focus = match r.u8("focus")? {
                        0 => Focus::Pen,
                        1 => Focus::Center,
                        2 => Focus::Point { x: r.finite("focus")?, y: r.finite("focus")? },
                        _ => return Err(DecodeError::BadValue { field: "focus" }),
                    };
                    ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus }
                }
                1 => ViewportOp::Fit,
                2 => ViewportOp::PageStep(i8::from_le_bytes([r.u8("page step")?])),
                3 => ViewportOp::Reset,
                _ => return Err(DecodeError::BadValue { field: "viewport op" }),
            };
            Frame::Viewport(ViewportCmd { screen, seq, op })
        }
        tag::LIST_DIR => Frame::ListDir {
            request_id: r.u32("request")?,
            path: r.string("path", MAX_PATH_BYTES)?,
            offset: r.u32("offset")?,
        },
        tag::OPEN_FILE => {
            let screen = ScreenId(r.u32("screen")?);
            let request_id = r.u32("request")?;
            let path = r.string("path", MAX_PATH_BYTES)?;
            let line = match r.u8("line")? {
                0 => None,
                1 => Some(r.u32("line")?),
                _ => return Err(DecodeError::BadValue { field: "line" }),
            };
            Frame::OpenFile { screen, request_id, path, line }
        }
        tag::BUTTON => {
            let screen = ScreenId(r.u32("screen")?);
            let request_id = r.u32("request")?;
            let index = r.u8("button index")?;
            if usize::from(index) >= BUTTON_COUNT {
                return Err(DecodeError::BadValue { field: "button index" });
            }
            Frame::Button { screen, request_id, index }
        }
        tag::SET_BUTTONS => Frame::SetButtons { request_id: r.u32("request")?, buttons: r.buttons()? },
        tag::NEED_THUMBNAIL => Frame::NeedThumbnail { id: r.u32("thumbnail")? },
        tag::CONTINUE_REVIEWING => Frame::ContinueReviewing { screen: ScreenId(r.u32("screen")?) },
        tag::PAIR_ACCEPTED => {
            let code = r.u32("code")?;
            if code > 999_999 {
                return Err(DecodeError::BadValue { field: "code" });
            }
            Frame::PairAccepted { code }
        }
        tag::PAIR_REJECTED => Frame::PairRejected(match r.u8("reject reason")? {
            0 => PairReject::WrongSecret,
            1 => PairReject::Expired,
            2 => PairReject::NoPairingOpen,
            3 => PairReject::Busy,
            4 => PairReject::TooManyPhones,
            5 => PairReject::Declined,
            _ => return Err(DecodeError::BadValue { field: "reject reason" }),
        }),
        tag::REFUSED => Frame::Refused(match r.u8("refusal")? {
            0 => Refusal::NotPaired,
            1 => Refusal::UpdateCompanion,
            2 => Refusal::UpdateAzReview,
            3 => Refusal::UnsupportedRole,
            _ => return Err(DecodeError::BadValue { field: "refusal" }),
        }),
        tag::PEN_STATE => {
            let seq = r.u32("seq")?;
            let screen = ScreenId(r.u32("screen")?);
            let folder = r.string("folder", MAX_ENTRY_NAME_BYTES)?;
            let view = r.rect("view")?;
            let zoom = r.finite("zoom")?;
            let pen = match r.u8("pen")? {
                0 => None,
                1 => Some(PenMark { x: r.finite("pen")?, y: r.finite("pen")?, down: r.flag("pen")? }),
                _ => return Err(DecodeError::BadValue { field: "pen" }),
            };
            let thumb_id = r.u32("thumbnail")?;
            let context = match r.u8("context")? {
                0 => Context::Page,
                1 => Context::Line,
                2 => Context::Region,
                _ => return Err(DecodeError::BadValue { field: "context" }),
            };
            let return_point = match r.u8("return point")? {
                0 => None,
                1 => Some(ReturnPoint {
                    file: r.string("return file", MAX_PATH_BYTES)?,
                    page_index: r.u32("return page")?,
                    line: r.u32("return line")?,
                    label: r.string("return label", MAX_NAME_BYTES)?,
                }),
                _ => return Err(DecodeError::BadValue { field: "return point" }),
            };
            let surface = match r.u8("surface")? {
                0 => {
                    let file = r.string("file", MAX_PATH_BYTES)?;
                    let page_index = r.u32("page")?;
                    let page_count = r.u32("page")?;
                    if page_count > MAX_PAGE_COUNT {
                        return Err(DecodeError::TooLong { field: "pages", len: page_count as usize, max: MAX_PAGE_COUNT as usize });
                    }
                    let line = r.u32("line")?;
                    let line_count = r.u32("line")?;
                    let aspect = r.finite("page shape")?;
                    let gap = r.finite("page shape")?;
                    if !(MIN_ASPECT..=MAX_ASPECT).contains(&aspect) || !(0.0..=MAX_PAGE_GAP).contains(&gap) {
                        return Err(DecodeError::BadValue { field: "page shape" });
                    }
                    Surface::Page(PageSurface { file, page_index, page_count, line, line_count, aspect, gap })
                }
                1 => {
                    let board_id = r.u32("board")?;
                    let name = r.string("board name", MAX_NAME_BYTES)?;
                    let aspect = r.finite("board shape")?;
                    if !(MIN_ASPECT..=MAX_ASPECT).contains(&aspect) {
                        return Err(DecodeError::BadValue { field: "board shape" });
                    }
                    let item_count = r.u32("board items")?;
                    if item_count > MAX_BOARD_ITEMS {
                        return Err(DecodeError::TooLong { field: "board items", len: item_count as usize, max: MAX_BOARD_ITEMS as usize });
                    }
                    Surface::Whiteboard(WhiteboardSurface { board_id, name, aspect, item_count })
                }
                _ => return Err(DecodeError::BadValue { field: "surface" }),
            };
            Frame::PenState(PenState { seq, screen, folder, surface, view, zoom, pen, thumb_id, context, return_point })
        }
        tag::THUMBNAIL => {
            let id = r.u32("thumbnail")?;
            let width = r.u16("thumbnail size")?;
            let height = r.u16("thumbnail size")?;
            let format = match r.u8("thumbnail format")? {
                0 => ThumbFormat::Rgba8,
                1 => ThumbFormat::Png,
                2 => ThumbFormat::Jpeg,
                _ => return Err(DecodeError::BadValue { field: "thumbnail format" }),
            };
            let len = r.u32("thumbnail bytes")? as usize;
            check_thumbnail(width, height, format, len)?;
            let bytes = r.take(len, "thumbnail bytes")?.to_vec();
            Frame::Thumbnail(Thumbnail { id, width, height, format, bytes })
        }
        tag::ROWS => {
            let request_id = r.u32("request")?;
            let dir = r.string("dir", MAX_PATH_BYTES)?;
            let offset = r.u32("offset")?;
            let more = r.flag("more")?;
            let count = r.count("rows", MAX_ROWS)?;
            let mut entries = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                let name = r.string("entry name", MAX_ENTRY_NAME_BYTES)?;
                let kind = match r.u8("entry kind")? {
                    0 => EntryKind::Dir,
                    1 => EntryKind::File,
                    _ => return Err(DecodeError::BadValue { field: "entry kind" }),
                };
                let state = match r.u8("file state")? {
                    0 => FileState::None,
                    1 => FileState::Clean,
                    2 => FileState::Unresolved,
                    3 => FileState::Working,
                    _ => return Err(DecodeError::BadValue { field: "file state" }),
                };
                entries.push(Entry { name, kind, state });
            }
            Frame::Rows(Rows { request_id, dir, offset, entries, more })
        }
        tag::BUTTONS => Frame::Buttons(r.buttons()?),
        tag::REPLY => {
            let request_id = r.u32("request")?;
            let status = match r.u8("reply status")? {
                0 => ReplyStatus::Received,
                1 => ReplyStatus::Working,
                2 => ReplyStatus::Suggested,
                3 => ReplyStatus::Done,
                4 => ReplyStatus::Failed,
                _ => return Err(DecodeError::BadValue { field: "reply status" }),
            };
            Frame::Reply(Reply { request_id, status })
        }
        tag::NOTICE => {
            let level = match r.u8("notice level")? {
                0 => NoticeLevel::Info,
                1 => NoticeLevel::Warning,
                _ => return Err(DecodeError::BadValue { field: "notice level" }),
            };
            Frame::Notice(Notice { level, text: r.string("notice", MAX_NOTICE_BYTES)? })
        }
        tag::SETTINGS => Frame::Settings(settings_of_bits(r.u8("settings")?)),
        tag::AGENT_FINISHED => {
            let conversation_id = r.u32("conversation")?;
            let file = r.string("file", MAX_PATH_BYTES)?;
            let line = r.u32("line")?;
            // a way to end that this build does not know is read as "nothing to do": the phone shows the line and does not buzz
            let kind = match r.u8("finish kind")? {
                0 => FinishKind::Done,
                1 => FinishKind::Suggestion,
                3 => FinishKind::Failed,
                _ => FinishKind::NoAction,
            };
            Frame::AgentFinished(AgentFinished { conversation_id, file, line, kind })
        }
        other => return Err(DecodeError::UnknownTag(other)),
    };
    r.end()?;
    Ok(frame)
}

/// The tag of a frame.
fn frame_tag(frame: &Frame) -> u8 {
    match frame {
        Frame::Hello(_) => tag::HELLO,
        Frame::Ping { .. } => tag::PING,
        Frame::Pong { .. } => tag::PONG,
        Frame::Bye => tag::BYE,
        Frame::PairRequest { .. } => tag::PAIR_REQUEST,
        Frame::Viewport(_) => tag::VIEWPORT,
        Frame::ListDir { .. } => tag::LIST_DIR,
        Frame::OpenFile { .. } => tag::OPEN_FILE,
        Frame::Button { .. } => tag::BUTTON,
        Frame::SetButtons { .. } => tag::SET_BUTTONS,
        Frame::NeedThumbnail { .. } => tag::NEED_THUMBNAIL,
        Frame::ContinueReviewing { .. } => tag::CONTINUE_REVIEWING,
        Frame::PairAccepted { .. } => tag::PAIR_ACCEPTED,
        Frame::PairRejected(_) => tag::PAIR_REJECTED,
        Frame::Refused(_) => tag::REFUSED,
        Frame::PenState(_) => tag::PEN_STATE,
        Frame::Thumbnail(_) => tag::THUMBNAIL,
        Frame::Rows(_) => tag::ROWS,
        Frame::Buttons(_) => tag::BUTTONS,
        Frame::Reply(_) => tag::REPLY,
        Frame::Notice(_) => tag::NOTICE,
        Frame::Settings(_) => tag::SETTINGS,
        Frame::AgentFinished(_) => tag::AGENT_FINISHED,
    }
}

/// The settings as one byte: reverse pan, snap to the page top, buzz for a result, for a suggestion, for a failure.
fn settings_bits(settings: &HostSettings) -> u8 {
    u8::from(settings.reverse_pan)
        | (u8::from(settings.snap_to_page_top) << 1)
        | (u8::from(settings.haptic_done) << 2)
        | (u8::from(settings.haptic_suggestion) << 3)
        | (u8::from(settings.haptic_failed) << 4)
}

/// The settings of a byte. A bit this build does not know is a setting a newer AzReview has: it is left alone (a phone obeys what
/// it understands), not refused.
fn settings_of_bits(bits: u8) -> HostSettings {
    HostSettings {
        reverse_pan: bits & 1 != 0,
        snap_to_page_top: bits & 2 != 0,
        haptic_done: bits & 4 != 0,
        haptic_suggestion: bits & 8 != 0,
        haptic_failed: bits & 16 != 0,
    }
}

/// The rules of a thumbnail, the same going out and coming in: a side from 1 to the cap, no more bytes than the cap,
/// and for raw pixels exactly four bytes each.
fn check_thumbnail(width: u16, height: u16, format: ThumbFormat, len: usize) -> Result<(), DecodeError> {
    if len > MAX_THUMBNAIL_BYTES {
        return Err(DecodeError::TooLong { field: "thumbnail bytes", len, max: MAX_THUMBNAIL_BYTES });
    }
    if width == 0 || height == 0 || width > MAX_THUMBNAIL_SIDE || height > MAX_THUMBNAIL_SIDE {
        return Err(DecodeError::BadValue { field: "thumbnail size" });
    }
    if format == ThumbFormat::Rgba8 && len != usize::from(width) * usize::from(height) * 4 {
        return Err(DecodeError::BadValue { field: "thumbnail bytes" });
    }
    Ok(())
}

struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn new(kind: u8) -> Writer {
        Writer { buf: vec![MAGIC, WIRE_VERSION, kind] }
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn bytes(&mut self, v: &[u8]) {
        self.buf.extend_from_slice(v);
    }

    fn finite(&mut self, v: f32, field: &'static str) -> Result<(), EncodeError> {
        if !v.is_finite() {
            return Err(EncodeError::BadValue { field });
        }
        self.buf.extend_from_slice(&v.to_le_bytes());
        Ok(())
    }

    fn rect(&mut self, rect: Rect, field: &'static str) -> Result<(), EncodeError> {
        self.finite(rect.x, field)?;
        self.finite(rect.y, field)?;
        self.finite(rect.w, field)?;
        self.finite(rect.h, field)
    }

    fn string(&mut self, s: &str, field: &'static str, max: usize) -> Result<(), EncodeError> {
        if s.len() > max || s.len() > usize::from(u16::MAX) {
            return Err(EncodeError::TooLong { field, len: s.len(), max });
        }
        self.u16(s.len() as u16);
        self.bytes(s.as_bytes());
        Ok(())
    }

    fn buttons(&mut self, set: &ButtonSet) -> Result<(), EncodeError> {
        for button in &set.buttons {
            let labels = button.label.chars().count();
            if labels > MAX_LABEL_CHARS {
                return Err(EncodeError::TooLong { field: "button label", len: labels, max: MAX_LABEL_CHARS });
            }
            let texts = button.text.chars().count();
            if texts > MAX_BUTTON_TEXT_CHARS {
                return Err(EncodeError::TooLong { field: "button text", len: texts, max: MAX_BUTTON_TEXT_CHARS });
            }
            self.string(&button.label, "button label", MAX_LABEL_CHARS * 4)?;
            self.string(&button.text, "button text", MAX_BUTTON_TEXT_CHARS * 4)?;
        }
        Ok(())
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize, field: &'static str) -> Result<&'a [u8], DecodeError> {
        let end = self.at.checked_add(n).filter(|end| *end <= self.bytes.len()).ok_or(DecodeError::Truncated { field })?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self, field: &'static str) -> Result<u8, DecodeError> {
        Ok(self.take(1, field)?[0])
    }

    fn flag(&mut self, field: &'static str) -> Result<bool, DecodeError> {
        match self.u8(field)? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(DecodeError::BadValue { field }),
        }
    }

    fn u16(&mut self, field: &'static str) -> Result<u16, DecodeError> {
        let b = self.take(2, field)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self, field: &'static str) -> Result<u32, DecodeError> {
        let b = self.take(4, field)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn finite(&mut self, field: &'static str) -> Result<f32, DecodeError> {
        let v = f32::from_bits(self.u32(field)?);
        if v.is_finite() {
            Ok(v)
        } else {
            Err(DecodeError::BadValue { field })
        }
    }

    fn rect(&mut self, field: &'static str) -> Result<Rect, DecodeError> {
        Ok(Rect { x: self.finite(field)?, y: self.finite(field)?, w: self.finite(field)?, h: self.finite(field)? })
    }

    /// A list's length, against its cap, before anything is reserved for it.
    fn count(&mut self, field: &'static str, max: usize) -> Result<usize, DecodeError> {
        let len = usize::from(self.u16(field)?);
        if len > max {
            return Err(DecodeError::TooLong { field, len, max });
        }
        Ok(len)
    }

    fn string(&mut self, field: &'static str, max: usize) -> Result<String, DecodeError> {
        let len = self.count(field, max)?;
        let raw = self.take(len, field)?;
        core::str::from_utf8(raw).map(str::to_string).map_err(|_| DecodeError::BadUtf8 { field })
    }

    fn button(&mut self) -> Result<QuickButton, DecodeError> {
        let label = self.string("button label", MAX_LABEL_CHARS * 4)?;
        let chars = label.chars().count();
        if chars > MAX_LABEL_CHARS {
            return Err(DecodeError::TooLong { field: "button label", len: chars, max: MAX_LABEL_CHARS });
        }
        let text = self.string("button text", MAX_BUTTON_TEXT_CHARS * 4)?;
        let chars = text.chars().count();
        if chars > MAX_BUTTON_TEXT_CHARS {
            return Err(DecodeError::TooLong { field: "button text", len: chars, max: MAX_BUTTON_TEXT_CHARS });
        }
        Ok(QuickButton { label, text })
    }

    fn buttons(&mut self) -> Result<ButtonSet, DecodeError> {
        Ok(ButtonSet { buttons: [self.button()?, self.button()?, self.button()?, self.button()?] })
    }

    /// The frame must end here.
    fn end(&self) -> Result<(), DecodeError> {
        let extra = self.bytes.len() - self.at;
        if extra == 0 {
            Ok(())
        } else {
            Err(DecodeError::Trailing { extra })
        }
    }
}

#[cfg(test)]
#[path = "wire_test.rs"]
mod wire_test;
