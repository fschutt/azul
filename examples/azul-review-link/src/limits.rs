//! Every cap of the link in one place: what a frame, a string or a list may hold, how often a kind of frame may
//! leave, how many phones may be paired, how long a request may wait. A value over a cap is cut or refused and
//! says so (a [`Capped`]); nothing here panics on what a peer sends.

/// The newest protocol this build speaks.
pub const PROTOCOL_VERSION: u16 = 1;
/// The oldest protocol this build still understands (a peer below it is told to update).
pub const MIN_PROTOCOL_VERSION: u16 = 1;
/// The oldest AzReview this companion works with (the demo pipeline writes it into `companion.json`; AzReview's own release
/// says the same the other way round: the oldest companion it serves).
pub const MIN_AZREVIEW_VERSION: &str = "0.1.0";
/// The application protocol name of the iroh endpoint: only peers that bind the same name can connect.
pub const ALPN: &str = "azreview/companion/1";

/// The first byte of every frame.
pub const MAGIC: u8 = 0xA7;

/// A whole frame, header included.
pub const MAX_FRAME_BYTES: usize = 256 * 1024;
/// Any string on the wire, the hard cap (the fields below are lower).
pub const MAX_STRING_BYTES: usize = 4096;
/// A device or Mac name.
pub const MAX_NAME_BYTES: usize = 96;
/// A workspace-relative path.
pub const MAX_PATH_BYTES: usize = 1024;
/// A file or folder name inside a listing.
pub const MAX_ENTRY_NAME_BYTES: usize = 255;
/// A version string.
pub const MAX_VERSION_BYTES: usize = 32;
/// A dialing string (the iroh endpoint ticket).
pub const MAX_TICKET_BYTES: usize = 2048;
/// A notice's text.
pub const MAX_NOTICE_BYTES: usize = 512;
/// Rows in one listing frame (a longer folder comes in pages: `Rows::more`).
pub const MAX_ROWS: usize = 500;
/// The encoded picture of a page.
pub const MAX_THUMBNAIL_BYTES: usize = 128 * 1024;
/// The side of a thumbnail in pixels.
pub const MAX_THUMBNAIL_SIDE: u16 = 1024;

/// The pages of a file under review (a column of a million sheets is more than any file).
pub const MAX_PAGE_COUNT: u32 = 1_000_000;
/// The items of a whiteboard.
pub const MAX_BOARD_ITEMS: u32 = 1_000_000;
/// The shape of a sheet, its width over its height: the least (a tall strip) and the most (a wide one).
pub const MIN_ASPECT: f32 = 0.05;
pub const MAX_ASPECT: f32 = 20.0;
/// The break between two sheets, in sheet widths: at most two.
pub const MAX_PAGE_GAP: f32 = 2.0;

/// The quick buttons of the phone.
pub const BUTTON_COUNT: usize = 4;
/// A quick button's label, in characters.
pub const MAX_LABEL_CHARS: usize = 12;
/// A quick button's instruction, in characters.
pub const MAX_BUTTON_TEXT_CHARS: usize = 280;

/// Phones that may be paired with one AzReview.
pub const MAX_PAIRED_PHONES: usize = 4;
/// Pen states leave at most this often (a newer one replaces one that has not left).
pub const PEN_STATE_MAX_HZ: u64 = 30;
/// Viewport updates leave at most this often.
pub const VIEWPORT_MAX_HZ: u64 = 60;
/// Quick-button presses a phone may make per second (more are dropped and said).
pub const BUTTONS_PER_SECOND: usize = 8;
/// Requests (list, open, button, set buttons) a phone may have unanswered at once.
pub const MAX_PENDING_REQUESTS: usize = 32;
/// How long a request may wait for its answer.
pub const REQUEST_TIMEOUT_MS: u64 = 10_000;
/// A ping goes out this often to a phone (and from a phone to the Mac).
pub const PING_INTERVAL_MS: u64 = 2_000;
/// A peer that said nothing for this long is gone.
pub const PEER_TIMEOUT_MS: u64 = 8_000;
/// A connection that has not said hello (or, unpaired, not asked to pair) within this long is closed.
pub const HELLO_TIMEOUT_MS: u64 = 5_000;
/// How long a phone may sit between its pairing request and the Mac user's answer.
pub const CONFIRM_TIMEOUT_MS: u64 = 120_000;

/// The one-time secret of a pairing offer.
pub const PAIRING_SECRET_BYTES: usize = 16;
/// How long a pairing offer is valid.
pub const PAIRING_WINDOW_MS: u64 = 5 * 60_000;
/// Wrong proofs the open pairing window takes before it closes.
pub const PAIRING_MAX_WRONG_PROOFS: u32 = 3;

/// A reconnect attempt waits this long after the first failure, doubling up to the maximum.
pub const RECONNECT_FIRST_MS: u64 = 500;
/// The longest wait between two reconnect attempts.
pub const RECONNECT_MAX_MS: u64 = 8_000;
/// How long a dial may take before the phone gives up on it and tries the next address.
pub const DIAL_TIMEOUT_MS: u64 = 10_000;
/// Failed dials of a paired Mac after which the phone says the Mac's address probably changed (it keeps trying, slowly).
pub const ADDRESS_CHANGED_AFTER: u32 = 6;
/// The wait between two dials once the address is thought to have changed.
pub const RECONNECT_SLOW_MS: u64 = 30_000;
/// A pen state names a picture that has not arrived: how long the phone waits before it asks for it.
pub const THUMB_WAIT_MS: u64 = 1_500;
/// How long before it asks for the same picture again.
pub const THUMB_RETRY_MS: u64 = 5_000;

/// The magnitude of a pan or a zoom factor a gesture frame may carry (fractions of a page, cumulative).
pub const MAX_GESTURE_PAN: f32 = 16.0;
/// The smallest and largest cumulative zoom factor of one gesture.
pub const MIN_GESTURE_ZOOM: f32 = 0.05;
/// See [`MIN_GESTURE_ZOOM`].
pub const MAX_GESTURE_ZOOM: f32 = 20.0;

/// What a cap cut: the thing, what it was, what it may be. The UI shows it; nothing crashes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capped {
    /// The thing that was too big (`"button text"`, `"rows"`).
    pub what: &'static str,
    /// What it was, in the unit of the limit.
    pub got: usize,
    /// What it may be.
    pub limit: usize,
}

impl core::fmt::Display for Capped {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}: {} is over the limit of {}, the rest was cut", self.what, self.got, self.limit)
    }
}

/// `text` with control characters removed (a newline kept when `keep_newlines`) and cut to `max_chars` characters.
/// Says what it cut.
#[must_use]
pub fn clean_text(text: &str, max_chars: usize, keep_newlines: bool, what: &'static str) -> (String, Option<Capped>) {
    let mut out = String::new();
    let mut chars = 0usize;
    let mut total = 0usize;
    for c in text.chars() {
        if c == '\n' && keep_newlines {
            // a newline stays a newline
        } else if c.is_control() {
            continue;
        }
        total += 1;
        if chars < max_chars {
            out.push(c);
            chars += 1;
        }
    }
    let capped = (total > max_chars).then_some(Capped { what, got: total, limit: max_chars });
    (out, capped)
}

/// `text` cut to at most `max_bytes` bytes on a character boundary. Says what it cut.
#[must_use]
pub fn cut_bytes(text: &str, max_bytes: usize, what: &'static str) -> (String, Option<Capped>) {
    if text.len() <= max_bytes {
        return (text.to_string(), None);
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_string(), Some(Capped { what, got: text.len(), limit: max_bytes }))
}

#[cfg(test)]
#[path = "limits_test.rs"]
mod limits_test;
