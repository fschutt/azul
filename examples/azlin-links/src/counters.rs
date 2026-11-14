//! The counters of a link and when they tick (planning/architecture.md section 5.12: "the counters
//! (`downloads`, `bytes_served`, `devices_seen` as hashed tokens, `first_open_at`)"; "Scanners and
//! previews": "an open counts only after the human step (the "Open" or "Download" button in the app
//! or in `drive.azlin.io`), and `one_time` is used up when the first complete download ends, not at
//! `link_resolve`").
//!
//! Three moments change a link's counters, and `resolve` is none of them:
//!
//! | moment | function | what changes |
//! |---|---|---|
//! | the human step (`link_open_confirm`, X-D-03) | `count_open` | `first_open_at` (once), `last_open_at`, `devices_seen` (a link with `max_devices` only); returns the `link_used` notice and the open-log entry |
//! | while serving | `admit_bytes` (reads), `count_bytes_served` | `bytes_served` |
//! | the last chunk of a download | `count_download_complete` | `downloads`, `bytes_served`, and a `one_time` link is used up (`revoked_at`) |
//!
//! **A download is counted when it completes.** `count_download_complete` is the one place a
//! download is admitted: it refuses (`Completed::Refused`) when the link has ended or has already had
//! its `max_downloads`, so `max_downloads` N admits exactly N completions however the downloads
//! interleave, and a `one_time` link is used up by the first one. The node calls it BEFORE it sends
//! the final chunk and sends nothing when it refuses. `admit_bytes`, asked before every chunk, stops
//! the downloads in flight as soon as the Nth has completed or the link has ended.
//!
//! **Bytes count as they are served, not only when a download completes**: `count_bytes_served`
//! adds the bytes of a part of a download (an aborted one too), or `bytes_cap` and the daily cap
//! could be beaten by a client that never finishes. The record keeps `bytes_served`; the day's bytes
//! (`ResolveCtx::served_today`) are the node's own count.
//!
//! **No address and no token is stored**: a device is `DeviceToken::seen_by(link)`, a hash of the
//! random token each device keeps with the link's id, and only for a link that counts devices.
//!
//! `LinkRecord::state_hash` is the hash of the whole row, for the replicas' agreement (S9's
//! snapshots, S47d's "resolve changes nothing").
//!
//! Implements planning/architecture.md section 5.12 (the counters, "Scanners and previews"), section
//! 5.15 (M7: link counters and caps only) and section 12 (E5).

use azlin_seams::{hash32, ByteCount, Hash32, Span, Stamp};

use crate::audience::Audience;
use crate::notify::{link_used, LinkUsed};
use crate::open_log::{self, OpenLogEntry};
use crate::options::{LinkOptions, Sealed};
use crate::record::{LinkRecord, OpaqueRecord, PausedBy};
use crate::resolve::{
    check_open, gate_downloads, gate_ended, is_public, ResolveCtx, ResolveRefusal,
};

/// The hash domain of a link's state: `LinkRecord::state_hash`.
///
/// Implements planning/architecture.md section 5.12 (`reg_link`) and section 2.5 (a domain string
/// per structure).
///
/// Caller: `LinkRecord::state_hash`.
pub const STATE_DOMAIN: &str = "azlin-link-state-v1";

/// Why an open is not counted. The record is not changed.
///
/// Implements planning/architecture.md section 5.12 ("Scanners and previews"; `open_log`: "the
/// recipient is told before opening") and section 5.15 (M7).
///
/// Caller: `count_open`; Seam: LINKS-NODE (it answers the status and the reason).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OpenRefusal {
    /// The request is not the human step: a page load or a preview counts nothing.
    #[error("an open counts only after the human step")]
    NotTheHumanStep,
    /// A rule of the link refuses the caller (the same rules as `resolve`).
    #[error("{0}")]
    Link(ResolveRefusal),
    /// The link keeps an open log and the recipient has not been told: the app shows the notice
    /// first and sends the acknowledgement.
    #[error("the recipient has not been told about the open log")]
    OpenLogNotAcknowledged,
}

/// What the human step did: whether this was the link's first open, the notice for the sharer, and
/// the open-log entry.
///
/// Implements planning/architecture.md section 5.12 (`notify`, `open_log`) and section 5.13
/// (`link_used`).
///
/// Caller: `count_open`; Seam: LINKS-NODE (it hands `notify` to S33 and appends `log_entry` to the
/// link's open log).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opened {
    /// This open was the first one the link counted.
    pub first_open: bool,
    /// The `link_used` event, when the `notify` option says the sharer is told.
    pub notify: Option<LinkUsed>,
    /// The open-log entry, when the link keeps an open log.
    pub log_entry: Option<OpenLogEntry>,
}

/// Counts an open: the HUMAN STEP ("Open" or "Download" button; X-D-03: the verb `link_open_confirm`).
/// It checks every rule of `resolve` again (the link may have changed since the page loaded), asks
/// the acknowledgement of the open log, and only then counts: `first_open_at` is set once, the last
/// open moves forward, the device is remembered when the link counts devices, and `Opened` carries
/// what the sharer is to be told. A request that is not the human step (`ResolveCtx::human_step`)
/// counts nothing. On every `Err` the record is as it was; a `one_time` link is NOT used up here but
/// when its first complete download ends (`count_download_complete`).
///
/// Implements planning/architecture.md section 5.12 ("Scanners and previews", `max_devices`,
/// `notify`, `open_log`) and section 5.15 (M7).
///
/// Seam: LINKS-NODE (the verb `link_open_confirm`, applied in the metadata group's log).
pub fn count_open(
    r: &mut LinkRecord,
    c: &ResolveCtx<'_>,
    acknowledged_open_log: bool,
) -> Result<Opened, OpenRefusal> {
    if !c.human_step && !azlin_seams::mutant!("links_resolve_counts_open") {
        return Err(OpenRefusal::NotTheHumanStep);
    }
    check_open(r, c, c.password_verified).map_err(OpenRefusal::Link)?;
    if open_log::is_logged(r)
        && !acknowledged_open_log
        && !azlin_seams::mutant!("links_open_log_unacknowledged")
    {
        return Err(OpenRefusal::OpenLogNotAcknowledged);
    }
    let link = *r.id();
    let notify = r.options().notify;
    let keeps_devices = r.options().max_devices.is_some();
    let one_time = r.options().one_time;
    let seen = c.device.map(|token| token.seen_by(&link));
    let log_entry = open_log::entry_for(r, &c.caller, c.now);
    let counters = r.counters_mut();
    let first_open = counters.first_open_at.is_none();
    if first_open {
        counters.first_open_at = Some(c.now);
    }
    counters.last_open_at = Some(match counters.last_open_at {
        Some(at) if at > c.now => at,
        _ => c.now,
    });
    if keeps_devices {
        if let Some(device) = seen {
            counters.devices_seen.insert(device);
        }
    }
    let downloads = counters.downloads;
    if one_time && azlin_seams::mutant!("links_one_time_used_at_resolve") {
        r.mark_revoked(c.now);
    }
    Ok(Opened {
        first_open,
        notify: link_used(notify, link, first_open, downloads),
        log_entry,
    })
}

/// How many of the `want` bytes the node may serve now: every rule of `resolve` is asked again (a
/// link that was revoked, paused, blocked or expired since the last chunk serves nothing: "the old
/// URL stops at once"), and the bytes are cut at what `bytes_cap` has left and, for a public link,
/// at what the day's cap has left. Zero when the link may not serve at all.
///
/// Implements planning/architecture.md section 5.12 (`bytes_cap`, "Links used as a CDN": "each public
/// link has a daily download cap set per plan").
///
/// Seam: LINKS-NODE (`object_get` with a link credential, before every chunk).
pub fn admit_bytes(r: &LinkRecord, want: ByteCount, c: &ResolveCtx<'_>) -> ByteCount {
    if check_open(r, c, c.password_verified).is_err() {
        return ByteCount::ZERO;
    }
    if azlin_seams::mutant!("links_admit_bytes_unbounded") {
        return want;
    }
    let mut allowed = want;
    if let Some(cap) = r.options().bytes_cap {
        allowed = allowed.min(cap.saturating_sub(r.counters().bytes_served));
    }
    if is_public(r) {
        allowed = allowed.min(c.daily_cap.saturating_sub(c.served_today));
    }
    allowed
}

/// Adds the bytes of a part of a download to `bytes_served`, a download that never completes
/// included.
///
/// Implements planning/architecture.md section 5.12 (`bytes_cap`; the counter `bytes_served`).
///
/// Caller: `count_download_complete`; Seam: LINKS-NODE (it counts the bytes it has served at the end
/// of each pack and when a connection ends).
pub fn count_bytes_served(r: &mut LinkRecord, bytes: ByteCount) {
    let counters = r.counters_mut();
    counters.bytes_served = counters.bytes_served.saturating_add(bytes);
}

/// What the end of a download did.
///
/// Implements planning/architecture.md section 5.12 (`max_downloads`, `one_time`: "the first
/// complete download revokes it").
///
/// Caller: `count_download_complete`; Seam: LINKS-NODE (it sends the last chunk only for `Counted`,
/// and ends the paired request of a link that is `used_up`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completed {
    /// The download is counted: `downloads` is the new count and `used_up` says that a `one_time`
    /// link is now revoked.
    Counted {
        /// The complete downloads of the link now.
        downloads: u64,
        /// This download used up a `one_time` link.
        used_up: bool,
    },
    /// The download is not counted: the link has ended (`Revoked`, `OneTimeUsed`) or has had its
    /// `max_downloads` (`DownloadsExhausted`) before this one. The bytes still count.
    Refused(ResolveRefusal),
}

/// Counts a complete download: its bytes are added, and, unless the link has ended or has already had
/// its `max_downloads`, the download count rises by one. A `one_time` link is used up HERE ("the first
/// complete download revokes it"), not at `link_resolve` and not at the open. A refused completion
/// changes nothing but the bytes, so `max_downloads` N admits exactly N completions.
///
/// Implements planning/architecture.md section 5.12 (`one_time`, `max_downloads`, "Scanners and
/// previews") and section 12 (E5).
///
/// Seam: LINKS-NODE (when a download's last chunk is about to be sent).
pub fn count_download_complete(r: &mut LinkRecord, bytes: ByteCount, now: Stamp) -> Completed {
    count_bytes_served(r, bytes);
    if !azlin_seams::mutant!("links_ended_download_counted") {
        if let Err(refusal) = gate_ended(r) {
            return Completed::Refused(refusal);
        }
    }
    if !azlin_seams::mutant!("links_download_beyond_cap_counted") {
        if let Err(refusal) = gate_downloads(r) {
            return Completed::Refused(refusal);
        }
    }
    let one_time = r.options().one_time;
    let counters = r.counters_mut();
    counters.downloads = counters.downloads.saturating_add(1);
    let downloads = counters.downloads;
    if one_time && !azlin_seams::mutant!("links_one_time_never_used_up") {
        r.mark_revoked(now);
    }
    Completed::Counted {
        downloads,
        used_up: one_time,
    }
}

/// A growing buffer of the canonical bytes of a state: fixed-size values as they are, counts and
/// lengths as eight big-endian bytes, `Option`s with a tag byte.
struct Frame(Vec<u8>);

impl Frame {
    fn new() -> Frame {
        Frame(Vec::new())
    }

    fn byte(&mut self, value: u8) {
        self.0.push(value);
    }

    fn flag(&mut self, value: bool) {
        self.byte(u8::from(value));
    }

    fn number(&mut self, value: u64) {
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    fn count(&mut self, len: usize) {
        self.number(ByteCount::of_len(len).get());
    }

    fn raw(&mut self, bytes: &[u8]) {
        self.0.extend_from_slice(bytes);
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.count(bytes.len());
        self.raw(bytes);
    }

    fn stamp(&mut self, value: Stamp) {
        self.number(value.as_secs());
    }

    fn maybe_stamp(&mut self, value: Option<Stamp>) {
        match value {
            Some(stamp) => {
                self.byte(1);
                self.stamp(stamp);
            }
            None => self.byte(0),
        }
    }

    fn maybe_span(&mut self, value: Option<Span>) {
        match value {
            Some(span) => {
                self.byte(1);
                self.number(span.as_secs());
            }
            None => self.byte(0),
        }
    }

    fn maybe_number(&mut self, value: Option<u64>) {
        match value {
            Some(number) => {
                self.byte(1);
                self.number(number);
            }
            None => self.byte(0),
        }
    }

    fn maybe_raw(&mut self, value: Option<&[u8]>) {
        match value {
            Some(bytes) => {
                self.byte(1);
                self.raw(bytes);
            }
            None => self.byte(0),
        }
    }

    fn maybe_bytes(&mut self, value: Option<&[u8]>) {
        match value {
            Some(bytes) => {
                self.byte(1);
                self.bytes(bytes);
            }
            None => self.byte(0),
        }
    }
}

/// Writes every option, in the order of the table of section 5.12.
fn put_options(frame: &mut Frame, o: &LinkOptions) {
    match &o.audience {
        Audience::Anyone => frame.byte(0),
        Audience::SignedIn => frame.byte(1),
        Audience::Members { groups } => {
            frame.byte(2);
            frame.count(groups.len());
            for group in groups {
                frame.raw(group.as_bytes());
            }
        }
        Audience::People { people } => {
            frame.byte(3);
            frame.count(people.len());
            for person in people {
                frame.raw(person.as_bytes());
            }
        }
        Audience::Approval => frame.byte(4),
    }
    frame.flag(o.password);
    frame.flag(o.app_only);
    frame.bytes(o.kind.as_str().as_bytes());
    frame.flag(o.subfolders);
    frame.maybe_stamp(o.not_before);
    frame.maybe_stamp(o.expires_at);
    frame.maybe_span(o.expires_after_open);
    frame.maybe_span(o.idle_expiry);
    frame.flag(o.one_time);
    frame.maybe_number(o.max_downloads.map(u64::from));
    frame.maybe_number(o.bytes_cap.map(ByteCount::get));
    frame.maybe_number(o.max_devices.map(u64::from));
    frame.bytes(o.rights.as_str().as_bytes());
    frame.flag(o.allow_upload);
    frame.flag(o.edit);
    frame.flag(o.comment);
    frame.flag(o.allow_reshare);
    frame.flag(o.watermark);
    frame.bytes(o.notify.as_str().as_bytes());
    frame.flag(o.open_log);
    frame.maybe_bytes(o.title.as_ref().map(Sealed::as_bytes));
    frame.maybe_bytes(o.message.as_ref().map(Sealed::as_bytes));
    frame.maybe_bytes(o.slug.as_ref().map(|slug| slug.as_str().as_bytes()));
}

impl LinkRecord {
    /// The hash of the whole row: every column, in a fixed order, sets in their canonical order, under
    /// `STATE_DOMAIN`. Two replicas that applied the same calls hold the same hash, and any change of
    /// any column (an option, a counter, a pause, a wrap, a time) changes it.
    ///
    /// Implements planning/architecture.md section 5.12 (`reg_link`) and section 12 (E5: "the record's
    /// `state_hash` is unchanged" by a resolve).
    ///
    /// Seam: S9 (the snapshot digest of the metadata group), S47d (the escalation tests).
    pub fn state_hash(&self) -> Hash32 {
        let row = self.row();
        let mut frame = Frame::new();
        frame.raw(row.id.as_bytes());
        frame.raw(row.space.as_bytes());
        frame.raw(row.created_by.as_bytes());
        put_options(&mut frame, &row.options);
        frame.raw(row.manifest_object.as_hash().as_bytes());
        frame.count(row.grants.len());
        for grant in row.grants.as_slice() {
            frame.raw(grant.object.as_hash().as_bytes());
            match grant.range {
                Some(range) => {
                    frame.byte(1);
                    frame.number(range.start().get());
                    frame.number(range.size().get());
                }
                None => frame.byte(0),
            }
        }
        frame.maybe_bytes(row.opaque_record.as_ref().map(OpaqueRecord::as_bytes));
        frame.count(row.wraps.len());
        for (member, wrap) in &row.wraps {
            frame.raw(member.as_bytes());
            frame.raw(wrap.key().as_bytes());
            frame.bytes(wrap.sealed().as_bytes());
        }
        frame.maybe_raw(row.request_id.as_ref().map(|id| id.as_bytes().as_slice()));
        frame.maybe_raw(row.comment_log.as_ref().map(|id| id.as_bytes().as_slice()));
        let counters = &row.counters;
        frame.number(counters.downloads);
        frame.number(counters.bytes_served.get());
        frame.count(counters.devices_seen.len());
        for device in &counters.devices_seen {
            frame.raw(device.as_bytes());
        }
        frame.maybe_stamp(counters.first_open_at);
        frame.maybe_stamp(counters.last_open_at);
        for by in PausedBy::ALL {
            frame.flag(row.pauses.has(*by));
        }
        frame.maybe_stamp(row.revoked_at);
        frame.stamp(row.created_at);
        frame.stamp(row.keyed_at);
        hash32(STATE_DOMAIN, &[frame.0.as_slice()])
    }
}
