//! What the phone knows of the Mac, as plain data: the session (paired? searching? lost?), the pen state, the picture
//! of the page, the four buttons and what became of the last press of each, the folder being listed. The phone's
//! screens draw from a [`PhoneView`] and nothing else; the client (`phone.rs`) keeps it up to date from the frames
//! that arrive ([`PhoneView::apply_frame`]), a simulator or a test feeds it frames directly.

use crate::access::ScreenId;
use crate::frame::{AgentFinished, ButtonSet, Entry, Frame, HostSettings, Notice, PenState, ReplyStatus, Thumbnail};
use crate::limits::BUTTON_COUNT;

/// Why a connection or a pairing did not work, in the terms the phone explains to the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureKind {
    /// The system did not let the app look at the local network (the iOS "Local Network" switch is off).
    NoPermission,
    /// Nothing answered at the Mac's address: AzReview is not running, the Mac is asleep, or the phone is on
    /// another network.
    NotFound,
    /// The companion is older than AzReview understands: update it from AzReview.
    UpdateCompanion,
    /// AzReview is older than the companion understands: update AzReview.
    UpdateAzReview,
    /// The person at the Mac declined.
    Declined,
    /// The pairing code (QR) was wrong, damaged or used already.
    WrongCode,
    /// The pairing code is older than five minutes.
    Expired,
    /// The Mac does not know this phone (it was forgotten there): pair again.
    Forgotten,
    /// Another phone is pairing, or four are paired already.
    Busy,
    /// The Mac stopped answering.
    Timeout,
    /// The phone was paired and the Mac does not answer at the address the phone saved: AzReview is closed, or the Mac's
    /// address on the Wi-Fi changed. The phone keeps trying slowly; the way out is to pair again (the code or the cable).
    AddressChanged,
}

/// Where the phone stands with the Mac.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Session {
    /// No Mac known: the pairing screen.
    Unpaired,
    /// Looking for the Mac (a saved address or a scanned code); `attempt` counts the dials.
    Searching { attempt: u32 },
    /// The Mac is found; both screens show `code`, the person at the Mac confirms it.
    Pairing { mac_name: String, code: u32 },
    Connected { mac_name: String },
    /// The link was there and is gone; the phone keeps trying.
    Lost { mac_name: String, attempt: u32 },
    /// It did not work, and why.
    Failed { kind: FailureKind },
}

/// The folder the Files screen shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FilesView {
    /// The folder, relative to the workspace (empty: the workspace itself).
    pub dir: String,
    pub entries: Vec<Entry>,
    /// There are rows after these (`Rows::more`).
    pub more: bool,
    /// The folder asked for and not answered yet (`None`: nothing is awaited).
    pub requested: Option<String>,
    /// What went wrong with the last request, in words.
    pub error: Option<String>,
}

impl FilesView {
    /// Whether an answer is awaited.
    #[must_use]
    pub fn loading(&self) -> bool {
        self.requested.is_some()
    }
}

/// Everything the screens draw from.
#[derive(Debug, Clone, PartialEq)]
pub struct PhoneView {
    pub session: Session,
    /// The Mac's name (from its hello; kept while the link is lost).
    pub mac_name: String,
    /// The round trip of the last ping, in milliseconds.
    pub latency_ms: Option<u32>,
    pub pen: Option<PenState>,
    /// The newest thumbnail that arrived (it shows `pen.thumb_id` when the ids match).
    pub thumb: Option<Thumbnail>,
    pub buttons: ButtonSet,
    /// The request each button was last pressed with.
    pub button_request: [u32; BUTTON_COUNT],
    /// How far the conversation of each button's last press has got (`None`: not pressed, or forgotten).
    pub button_status: [Option<ReplyStatus>; BUTTON_COUNT],
    pub files: FilesView,
    /// The newest line AzReview sent for the screen.
    pub notice: Option<Notice>,
    /// What the user settled on the Mac that the phone obeys (the defaults until the Mac says).
    pub settings: HostSettings,
    /// How many agents have finished since the phone connected (a screen that sees it grow knows there is news).
    pub finished_count: u32,
    /// The newest agent that finished.
    pub finished: Option<AgentFinished>,
}

impl Default for PhoneView {
    fn default() -> Self {
        Self::new()
    }
}

impl PhoneView {
    /// A phone that knows nothing: unpaired, the default buttons, no pen.
    #[must_use]
    pub fn new() -> Self {
        PhoneView {
            session: Session::Unpaired,
            mac_name: String::new(),
            latency_ms: None,
            pen: None,
            thumb: None,
            buttons: ButtonSet::defaults(),
            button_request: [0; BUTTON_COUNT],
            button_status: [None; BUTTON_COUNT],
            files: FilesView::default(),
            notice: None,
            settings: HostSettings::default(),
            finished_count: 0,
            finished: None,
        }
    }

    /// The screen of AzReview the phone is looking at (the one the newest pen state is of; the main screen before one came).
    #[must_use]
    pub fn screen(&self) -> ScreenId {
        self.pen.as_ref().map_or(ScreenId::MAIN, |p| p.screen)
    }

    /// Whether the thumbnail is the picture of the page the pen state names.
    #[must_use]
    pub fn thumb_is_current(&self) -> bool {
        match (&self.pen, &self.thumb) {
            (Some(pen), Some(thumb)) => pen.thumb_id != 0 && pen.thumb_id == thumb.id,
            _ => false,
        }
    }

    /// Whether the phone has a Mac to talk to now.
    #[must_use]
    pub fn is_connected(&self) -> bool {
        matches!(self.session, Session::Connected { .. })
    }

    /// Takes what a frame from the Mac says. Whether anything the screens draw changed.
    pub fn apply_frame(&mut self, frame: &Frame) -> bool {
        match frame {
            Frame::PenState(new) => {
                if let Some(old) = &self.pen {
                    // newer in the order of the counter, which may have wrapped round
                    if (new.seq.wrapping_sub(old.seq) as i32) <= 0 {
                        return false;
                    }
                }
                let changed = self.pen.as_ref().is_none_or(|old| {
                    let mut same_seq = old.clone();
                    same_seq.seq = new.seq;
                    same_seq != *new
                });
                self.pen = Some(new.clone());
                changed
            }
            Frame::Thumbnail(thumb) => {
                if thumb.id == 0 || self.thumb.as_ref() == Some(thumb) {
                    return false;
                }
                self.thumb = Some(thumb.clone());
                true
            }
            Frame::Rows(rows) => self.apply_rows(rows),
            Frame::Buttons(set) => {
                if self.buttons == *set {
                    return false;
                }
                self.buttons = set.clone();
                true
            }
            Frame::Reply(reply) => {
                if reply.request_id == 0 {
                    return false;
                }
                for index in 0..BUTTON_COUNT {
                    if self.button_request[index] == reply.request_id {
                        let changed = self.button_status[index] != Some(reply.status);
                        self.button_status[index] = Some(reply.status);
                        return changed;
                    }
                }
                false
            }
            Frame::Notice(notice) => {
                if self.notice.as_ref() == Some(notice) {
                    return false;
                }
                self.notice = Some(notice.clone());
                true
            }
            Frame::Settings(settings) => {
                if self.settings == *settings {
                    return false;
                }
                self.settings = *settings;
                true
            }
            Frame::AgentFinished(done) => {
                self.finished_count = self.finished_count.wrapping_add(1);
                self.finished = Some(done.clone());
                true
            }
            _ => false,
        }
    }

    /// A page of a folder: the one asked for, or (when nothing is awaited) a push for the folder on screen.
    fn apply_rows(&mut self, rows: &crate::frame::Rows) -> bool {
        match &self.files.requested {
            Some(asked) if *asked != rows.dir => return false,
            None if rows.dir != self.files.dir => return false,
            _ => {}
        }
        let before = self.files.clone();
        if rows.offset == 0 {
            self.files.entries = rows.entries.clone();
        } else if rows.offset as usize == self.files.entries.len() && rows.dir == self.files.dir {
            self.files.entries.extend(rows.entries.iter().cloned());
        } else {
            return false;
        }
        self.files.dir = rows.dir.clone();
        self.files.more = rows.more;
        self.files.requested = None;
        self.files.error = None;
        self.files != before
    }

    /// The Files screen asks for `dir` (the answer is awaited until `Rows` for it arrive or [`Self::files_failed`]);
    /// the rows on screen stay until then.
    pub fn begin_listing(&mut self, dir: &str) {
        self.files.requested = Some(dir.to_string());
        self.files.error = None;
    }

    /// The request for the folder failed (a timeout, a refusal): the old rows stay and the screen says why.
    pub fn files_failed(&mut self, why: &str) {
        self.files.requested = None;
        self.files.error = Some(why.to_string());
    }

    /// A press of button `index` went out as `request_id`; its status stays empty until the Mac answers.
    pub fn button_pressed(&mut self, index: usize, request_id: u32) {
        if index < BUTTON_COUNT {
            self.button_request[index] = request_id;
            self.button_status[index] = None;
        }
    }
}

#[cfg(test)]
#[path = "view_test.rs"]
mod view_test;
