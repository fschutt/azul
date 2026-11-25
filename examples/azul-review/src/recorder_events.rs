//! The events of a recording: what the review did, on the session clock, one JSON object a line
//! (`events.jsonl`).
//!
//! The pen, the navigation, the conversations, the diffs and the whiteboard each say what happened
//! through ONE call, [`EventSink::event`]`(kind, fields)`, from any thread. The call stamps the
//! event with the session clock where it is made, checks it against the caps (a rate per kind and
//! overall, a size per line, a depth of queue: [`Caps`]) and puts it on a channel; the writer
//! thread owns the file ([`EventLog`]). Nothing here blocks the caller or grows without bound: an
//! event that is over a cap is DROPPED and counted, and the log says so once a second
//! (`recorder.drop`), so a recording never stalls the window and never lies about what it lost.
//!
//! The VOCABULARY is [`KINDS`]: every kind the app records, its rate, and what its fields mean
//! (`RECORDER.md` is written from it and a test keeps them equal). A kind not in the table is
//! recorded at a low default rate, so a new kind never silently vanishes; its name must be made of
//! `a-z`, `0-9`, `_` and `.`.
//!
//! A line is `{"t":<microseconds>,"k":"<kind>",<the fields>}`; the fields are one JSON object whose
//! members are merged in after `t` and `k` (a field of those names is dropped).
//!
//! Plain `std` and serde_json (which validates the fields on the writer thread, never on the
//! caller's), no azul.

use std::{
    collections::BTreeMap,
    io::{self, Write},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender, TryRecvError},
        Arc, Mutex,
    },
};

use crate::recorder_clock::Clock;

/// One kind of event: its name, how often it may be recorded, how big a line may be, and what its
/// fields are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindSpec {
    /// `group.name`, the `k` of a line.
    pub name: &'static str,
    /// The events a second it sustains.
    pub per_second: u32,
    /// The events it may send at once after a quiet time.
    pub burst: u32,
    /// The longest a line of this kind may be, bytes.
    pub max_line: usize,
    /// The fields, in words: `name (type)` separated by commas.
    pub fields: &'static str,
}

const fn kind(name: &'static str, per_second: u32, burst: u32, max_line: usize, fields: &'static str) -> KindSpec {
    KindSpec {
        name,
        per_second,
        burst,
        max_line,
        fields,
    }
}

/// A line of a kind the table does not know may be this long.
pub const DEFAULT_MAX_LINE: usize = 8 * 1024;
/// The rate of a kind the table does not know.
pub static UNKNOWN_KIND: KindSpec = kind("unknown", 10, 20, DEFAULT_MAX_LINE, "anything");

/// Every kind of event the app records.
pub static KINDS: [KindSpec; 39] = [
    kind("session.start", 5, 10, DEFAULT_MAX_LINE, "app (string), version (string), workspace (string)"),
    kind("session.stop", 5, 10, DEFAULT_MAX_LINE, "reason (string: user, quit, error)"),
    kind("session.pause", 5, 10, DEFAULT_MAX_LINE, "reason (string: user, hotkey)"),
    kind("session.resume", 5, 10, DEFAULT_MAX_LINE, "reason (string: user, hotkey, activity)"),
    kind("session.idle_pause", 5, 10, DEFAULT_MAX_LINE, "idle_ms (number)"),
    kind("session.quota_pause", 5, 10, DEFAULT_MAX_LINE, "free_bytes (number), min_free_bytes (number)"),
    kind("layout.window", 2, 4, DEFAULT_MAX_LINE, "w (number), h (number), scale (number): logical size and device scale"),
    kind("layout.toggle", 5, 10, DEFAULT_MAX_LINE, "panel (string: tree, whiteboard, agents, recorder, notes), open (bool)"),
    kind("nav.workspace", 2, 4, DEFAULT_MAX_LINE, "root (string), files (number)"),
    kind("nav.file", 5, 10, DEFAULT_MAX_LINE, "file (string, workspace-relative), lines (number)"),
    kind("nav.page", 10, 20, DEFAULT_MAX_LINE, "file (string), page (number, 0-based), of (number)"),
    kind("nav.lines", 5, 10, DEFAULT_MAX_LINE, "file (string), first (number), last (number): the lines in view"),
    kind("pointer.move", 20, 20, DEFAULT_MAX_LINE, "x (number), y (number): window logical px; page (number), px (number), py (number): on the page; pen (bool), down (bool)"),
    kind("pen.select", 10, 20, DEFAULT_MAX_LINE, "meaning (string: scope, issue, question, duplicate, praise), nib (string: marker, pen, audio)"),
    kind("pen.stroke.begin", 30, 60, DEFAULT_MAX_LINE, "id (number), file (string), page (number), x (number), y (number)"),
    kind("pen.stroke", 30, 60, 128 * 1024, "id (number), file (string), page (number), meaning (string), nib (string), n (number), t0_us (number), bbox ([x0,y0,x1,y1]), s ([[dt_ms,x,y,pressure],...] at most 2000)"),
    kind("conv.start", 5, 10, DEFAULT_MAX_LINE, "id (number), file (string), line (number), page (number), x (number), y (number)"),
    kind("conv.word", 20, 40, DEFAULT_MAX_LINE, "id (number), w (string), t0_us (number), t1_us (number), p (number, 0-1)"),
    kind("conv.working", 10, 20, DEFAULT_MAX_LINE, "id (number), task (string: exploration, editing)"),
    kind("conv.suggestion", 10, 20, DEFAULT_MAX_LINE, "id (number), op (string: insert, replace, remove, answer), file (string), first (number), last (number)"),
    kind("conv.accept", 10, 20, DEFAULT_MAX_LINE, "id (number), op (string), file (string), first (number), last (number)"),
    kind("conv.reject", 10, 20, DEFAULT_MAX_LINE, "id (number)"),
    kind("conv.done", 10, 20, DEFAULT_MAX_LINE, "id (number)"),
    kind("dictation.start", 5, 10, DEFAULT_MAX_LINE, "conv (number), engine (string)"),
    kind("dictation.end", 5, 10, DEFAULT_MAX_LINE, "conv (number), cause (string)"),
    kind("diff.delete", 20, 40, DEFAULT_MAX_LINE, "file (string), line (number), from (number), to (number), text (string)"),
    kind("diff.accept", 20, 40, DEFAULT_MAX_LINE, "file (string), op (string), first (number), last (number)"),
    kind("wb.open", 5, 10, DEFAULT_MAX_LINE, "width (number)"),
    kind("wb.close", 5, 10, DEFAULT_MAX_LINE, "width (number)"),
    kind("wb.resize", 10, 20, DEFAULT_MAX_LINE, "width (number)"),
    kind("wb.cutout", 20, 40, DEFAULT_MAX_LINE, "id (number), file (string), first (number), last (number)"),
    kind("wb.arrow", 20, 40, DEFAULT_MAX_LINE, "id (number), from (number), to (number)"),
    kind("wb.jump", 10, 20, DEFAULT_MAX_LINE, "id (number), file (string), first (number), last (number)"),
    kind("marker", 5, 10, DEFAULT_MAX_LINE, "label (string): a chapter the user marked"),
    kind("recorder.track", 5, 10, DEFAULT_MAX_LINE, "track (string: screen, camera, audio), status (string), note (string)"),
    kind("recorder.segment", 5, 10, DEFAULT_MAX_LINE, "track (string), file (string), start_us (number)"),
    kind("recorder.drop", 2, 4, DEFAULT_MAX_LINE, "kind (string), n (number), why (string: rate, queue, size, invalid, capped)"),
    kind("recorder.events_capped", 1, 1, DEFAULT_MAX_LINE, "bytes (number): the file reached its size cap"),
    kind("recorder.note", 5, 10, DEFAULT_MAX_LINE, "text (string): something the recorder says about itself"),
];

/// The spec of `name`: its entry in [`KINDS`], else [`UNKNOWN_KIND`].
#[must_use]
pub fn spec_of(name: &str) -> &'static KindSpec {
    KINDS.iter().find(|k| k.name == name).unwrap_or(&UNKNOWN_KIND)
}

/// Whether `name` is a kind name: `a-z`, `0-9`, `_` and `.`, 1 to 48 characters, not starting or
/// ending with a dot.
#[must_use]
pub fn is_kind_name(name: &str) -> bool {
    (1..=48).contains(&name.len())
        && !name.starts_with('.')
        && !name.ends_with('.')
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// The caps on what a recording takes in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// Events a second over all kinds.
    pub total_per_second: u32,
    /// Events that may be sent at once over all kinds.
    pub total_burst: u32,
    /// Events waiting for the writer: past this, new ones are dropped.
    pub queue: usize,
    /// The size the file may reach, bytes: past it only the recorder's own lines are written.
    pub file_bytes: u64,
}

impl Default for Caps {
    /// 500 events a second (burst 1000), 10 000 waiting, 200 MB a session.
    fn default() -> Self {
        Self {
            total_per_second: 500,
            total_burst: 1_000,
            queue: 10_000,
            file_bytes: 200 * 1024 * 1024,
        }
    }
}

/// Why an event was not recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dropped {
    /// Its kind sent too many.
    Rate,
    /// Everything together sent too many.
    Total,
    /// The writer had too many waiting.
    Queue,
    /// The line would be too long.
    Size,
    /// The kind's name or the fields are not valid.
    Invalid,
    /// The file reached its size cap.
    Capped,
}

impl Dropped {
    /// The word in the `recorder.drop` line.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Rate | Self::Total => "rate",
            Self::Queue => "queue",
            Self::Size => "size",
            Self::Invalid => "invalid",
            Self::Capped => "capped",
        }
    }
}

/// A token bucket on microseconds: `per_second` tokens a second up to `burst`.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens_milli: u64,
    at_us: u64,
}

impl Bucket {
    fn full(burst: u32, at_us: u64) -> Self {
        Self {
            tokens_milli: u64::from(burst) * 1_000,
            at_us,
        }
    }

    /// Brings the tokens the time since the last look has earned.
    fn refill(&mut self, per_second: u32, burst: u32, now_us: u64) {
        let elapsed = now_us.saturating_sub(self.at_us);
        self.at_us = self.at_us.max(now_us);
        // per_second tokens a second = per_second milli-tokens a millisecond = per_second / 1000
        // milli-tokens a microsecond.
        let added = elapsed.saturating_mul(u64::from(per_second)) / 1_000;
        self.tokens_milli = self.tokens_milli.saturating_add(added).min(u64::from(burst) * 1_000);
    }

    const fn has_token(&self) -> bool {
        self.tokens_milli >= 1_000
    }

    fn consume(&mut self) {
        self.tokens_milli = self.tokens_milli.saturating_sub(1_000);
    }
}

/// What the callers share: the clock, the caps, the buckets, the counters.
struct Shared {
    clock: Arc<dyn Clock>,
    caps: Caps,
    buckets: Mutex<Buckets>,
    queued: AtomicU64,
    sent: AtomicU64,
    dropped: Mutex<BTreeMap<(String, Dropped), u64>>,
}

struct Buckets {
    total: Bucket,
    by_kind: BTreeMap<String, Bucket>,
}

/// A line on its way to the writer.
#[derive(Debug)]
enum Msg {
    Event { t_us: u64, kind: String, fields: String },
    Flush,
}

/// Where events are put: cheap to clone, usable from any thread.
#[derive(Clone)]
pub struct EventSink {
    shared: Arc<Shared>,
    tx: Sender<Msg>,
}

/// The writer's end of the channel.
pub struct EventReceiver {
    rx: Receiver<Msg>,
    shared: Arc<Shared>,
}

/// A sink for the callers and a receiver for the writer.
#[must_use]
pub fn event_channel(clock: Arc<dyn Clock>, caps: Caps) -> (EventSink, EventReceiver) {
    let now = clock.now_us();
    let (tx, rx) = mpsc::channel();
    let shared = Arc::new(Shared {
        clock,
        caps,
        buckets: Mutex::new(Buckets {
            total: Bucket::full(caps.total_burst, now),
            by_kind: BTreeMap::new(),
        }),
        queued: AtomicU64::new(0),
        sent: AtomicU64::new(0),
        dropped: Mutex::new(BTreeMap::new()),
    });
    (
        EventSink {
            shared: Arc::clone(&shared),
            tx,
        },
        EventReceiver { rx, shared },
    )
}

impl EventSink {
    /// Records an event of `kind` with `fields`, a JSON object as text (`{"id":5}`; `{}` for none),
    /// stamped with the session clock now. Returns why it was not taken, when it was not. Never
    /// blocks and never fails the caller.
    pub fn event(&self, kind: &str, fields: &str) -> Result<(), Dropped> {
        let t_us = self.shared.clock.now_us();
        self.event_at(t_us, kind, fields)
    }

    /// [`Self::event`] for a moment already read from the clock (the time a stroke began, not the
    /// time it ended).
    ///
    /// # Errors
    /// Why the event was dropped.
    pub fn event_at(&self, t_us: u64, kind: &str, fields: &str) -> Result<(), Dropped> {
        let spec = spec_of(kind);
        let reject = |why: Dropped| {
            self.count_drop(kind, why);
            Err(why)
        };
        if !is_kind_name(kind) {
            return reject(Dropped::Invalid);
        }
        if kind.len() + fields.len() + 48 > spec.max_line {
            return reject(Dropped::Size);
        }
        if self.shared.queued.load(Ordering::Relaxed) >= self.shared.caps.queue as u64 {
            return reject(Dropped::Queue);
        }
        let allowed = {
            let Ok(mut guard) = self.shared.buckets.lock() else {
                return reject(Dropped::Invalid);
            };
            let caps = self.shared.caps;
            let Buckets { total, by_kind } = &mut *guard;
            // Many different made-up names must not grow the table without end.
            let key = if KINDS.iter().any(|k| k.name == kind) || by_kind.contains_key(kind) || by_kind.len() < 128 {
                kind
            } else {
                UNKNOWN_KIND.name
            };
            total.refill(caps.total_per_second, caps.total_burst, t_us);
            let bucket = by_kind
                .entry(key.to_string())
                .or_insert_with(|| Bucket::full(spec.burst, t_us));
            bucket.refill(spec.per_second, spec.burst, t_us);
            // A kind over its own rate is refused before it can use up what all the kinds share.
            if !bucket.has_token() {
                Err(Dropped::Rate)
            } else if !total.has_token() {
                Err(Dropped::Total)
            } else {
                bucket.consume();
                total.consume();
                Ok(())
            }
        };
        if let Err(why) = allowed {
            return reject(why);
        }
        self.shared.queued.fetch_add(1, Ordering::Relaxed);
        if self
            .tx
            .send(Msg::Event {
                t_us,
                kind: kind.to_string(),
                fields: fields.to_string(),
            })
            .is_err()
        {
            self.shared.queued.fetch_sub(1, Ordering::Relaxed);
            return reject(Dropped::Queue);
        }
        self.shared.sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    /// Asks the writer to flush what it has to the file.
    pub fn flush(&self) {
        let _ = self.tx.send(Msg::Flush);
    }

    /// Events accepted so far.
    #[must_use]
    pub fn accepted(&self) -> u64 {
        self.shared.sent.load(Ordering::Relaxed)
    }

    /// Events dropped so far, over every kind and reason.
    #[must_use]
    pub fn dropped_total(&self) -> u64 {
        self.shared.dropped.lock().map_or(0, |d| d.values().sum())
    }

    fn count_drop(&self, kind: &str, why: Dropped) {
        if let Ok(mut d) = self.shared.dropped.lock() {
            // A kind that is not a name could be anything: one counter for all of them.
            let key = if is_kind_name(kind) { kind.to_string() } else { String::from("?") };
            *d.entry((key, why)).or_insert(0) += 1;
        }
    }
}

/// The JSON string for `s`: quoted, with what must be escaped.
#[must_use]
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A number as JSON: finite, three decimals at most, no exponent; a non-number is 0.
#[must_use]
pub fn json_number(v: f64) -> String {
    if !v.is_finite() {
        return String::from("0");
    }
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" || s == "-0" {
        String::from("0")
    } else {
        s.to_string()
    }
}

/// A builder of the fields object: `Fields::new().num("id", 5.0).string("file", "a.rs").build()`.
#[derive(Debug, Default, Clone)]
pub struct Fields {
    body: String,
}

impl Fields {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn key(&mut self, key: &str) {
        if !self.body.is_empty() {
            self.body.push(',');
        }
        self.body.push_str(&json_string(key));
        self.body.push(':');
    }

    /// A number member (whole numbers print without decimals).
    #[must_use]
    pub fn num(mut self, key: &str, v: f64) -> Self {
        self.key(key);
        self.body.push_str(&json_number(v));
        self
    }

    /// An integer member.
    #[must_use]
    pub fn int(mut self, key: &str, v: i64) -> Self {
        self.key(key);
        self.body.push_str(&v.to_string());
        self
    }

    /// A text member.
    #[must_use]
    pub fn string(mut self, key: &str, v: &str) -> Self {
        self.key(key);
        self.body.push_str(&json_string(v));
        self
    }

    /// A true-or-false member.
    #[must_use]
    pub fn flag(mut self, key: &str, v: bool) -> Self {
        self.key(key);
        self.body.push_str(if v { "true" } else { "false" });
        self
    }

    /// A member whose value is JSON text the caller made (an array of samples): the writer checks
    /// that the whole line parses and drops it with a `recorder.drop` when it does not.
    #[must_use]
    pub fn raw(mut self, key: &str, json: &str) -> Self {
        self.key(key);
        self.body.push_str(json);
        self
    }

    /// The object as text.
    #[must_use]
    pub fn build(&self) -> String {
        format!("{{{}}}", self.body)
    }
}

/// The writer's side of the events: turns what the callers sent into lines and appends them to a
/// file (or anything that writes), keeps its size under the cap and says what was dropped.
pub struct EventLog<W: Write> {
    out: W,
    rx: EventReceiver,
    bytes: u64,
    lines: u64,
    capped: bool,
    reported: BTreeMap<(String, Dropped), u64>,
    last_report_us: u64,
    unflushed: usize,
}

/// What the log did in one [`EventLog::pump`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pumped {
    /// Lines written.
    pub written: u64,
    /// The channel is closed and empty: every sink was dropped.
    pub closed: bool,
}

impl<W: Write> EventLog<W> {
    /// A log that writes to `out`, which already holds `bytes` bytes (an existing file's length).
    pub fn new(out: W, rx: EventReceiver, bytes: u64) -> Self {
        Self {
            out,
            rx,
            bytes,
            lines: 0,
            capped: false,
            reported: BTreeMap::new(),
            last_report_us: 0,
            unflushed: 0,
        }
    }

    /// Lines written so far.
    #[must_use]
    pub const fn lines(&self) -> u64 {
        self.lines
    }

    /// Bytes in the file (what it held before plus what was written).
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Writes every event waiting and, when a second has gone by since the last time, the
    /// `recorder.drop` lines for what was dropped since. The caller calls it in a loop with a short
    /// wait (the writer thread) or once per tick (a test).
    ///
    /// # Errors
    /// The file could not be written.
    pub fn pump(&mut self) -> io::Result<Pumped> {
        let mut pumped = Pumped::default();
        loop {
            match self.rx.rx.try_recv() {
                Ok(Msg::Event { t_us, kind, fields }) => {
                    self.rx.shared.queued.fetch_sub(1, Ordering::Relaxed);
                    if self.write_event(t_us, &kind, &fields)? {
                        pumped.written += 1;
                    }
                }
                Ok(Msg::Flush) => self.flush()?,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    pumped.closed = true;
                    break;
                }
            }
        }
        let now = self.rx.shared.clock.now_us();
        if now.saturating_sub(self.last_report_us) >= 1_000_000 || pumped.closed {
            pumped.written += self.report_drops(now)?;
            self.last_report_us = now;
        }
        if pumped.closed || self.unflushed >= 256 {
            self.flush()?;
        }
        Ok(pumped)
    }

    /// Flushes the file.
    ///
    /// # Errors
    /// The file could not be written.
    pub fn flush(&mut self) -> io::Result<()> {
        self.unflushed = 0;
        self.out.flush()
    }

    /// What the log writes to.
    #[must_use]
    pub const fn get_ref(&self) -> &W {
        &self.out
    }

    /// The file, back.
    pub fn into_inner(self) -> W {
        self.out
    }

    fn write_event(&mut self, t_us: u64, kind: &str, fields: &str) -> io::Result<bool> {
        let own = kind.starts_with("session.") || kind.starts_with("recorder.");
        if self.capped && !own {
            self.drop_counted(kind, Dropped::Capped);
            return Ok(false);
        }
        let Some(members) = object_members(fields) else {
            self.drop_counted(kind, Dropped::Invalid);
            return Ok(false);
        };
        let mut line = format!("{{\"t\":{t_us},\"k\":{}", json_string(kind));
        if !members.is_empty() {
            line.push(',');
            line.push_str(&members);
        }
        line.push_str("}\n");
        if !own && self.bytes + line.len() as u64 > self.rx.shared.caps.file_bytes {
            self.capped = true;
            self.drop_counted(kind, Dropped::Capped);
            let note = format!(
                "{{\"t\":{t_us},\"k\":\"recorder.events_capped\",\"bytes\":{}}}\n",
                self.bytes
            );
            self.append(&note)?;
            return Ok(false);
        }
        self.append(&line)?;
        Ok(true)
    }

    fn append(&mut self, line: &str) -> io::Result<()> {
        self.out.write_all(line.as_bytes())?;
        self.bytes += line.len() as u64;
        self.lines += 1;
        self.unflushed += 1;
        Ok(())
    }

    fn drop_counted(&mut self, kind: &str, why: Dropped) {
        if let Ok(mut d) = self.rx.shared.dropped.lock() {
            *d.entry((kind.to_string(), why)).or_insert(0) += 1;
        }
    }

    /// One `recorder.drop` line per (kind, reason) that dropped something since the last report.
    fn report_drops(&mut self, now_us: u64) -> io::Result<u64> {
        let totals: Vec<((String, Dropped), u64)> = match self.rx.shared.dropped.lock() {
            Ok(d) => d.iter().map(|(k, v)| (k.clone(), *v)).collect(),
            Err(_) => return Ok(0),
        };
        let mut written = 0;
        for ((kind, why), total) in totals {
            let before = self.reported.get(&(kind.clone(), why)).copied().unwrap_or(0);
            if total > before {
                let line = format!(
                    "{{\"t\":{now_us},\"k\":\"recorder.drop\",\"kind\":{},\"n\":{},\"why\":\"{}\"}}\n",
                    json_string(&kind),
                    total - before,
                    why.word()
                );
                self.append(&line)?;
                self.reported.insert((kind, why), total);
                written += 1;
            }
        }
        Ok(written)
    }
}

/// The members of a JSON object given as text, without its braces (`{"a":1}` gives `"a":1`), with
/// the members named `t` and `k` taken out; `None` when the text is not one JSON object.
#[must_use]
pub fn object_members(fields: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(fields).ok()?;
    let serde_json::Value::Object(map) = value else {
        return None;
    };
    let mut out = String::new();
    for (key, v) in map {
        if key == "t" || key == "k" {
            continue;
        }
        if !out.is_empty() {
            out.push(',');
        }
        out.push_str(&json_string(&key));
        out.push(':');
        out.push_str(&serde_json::to_string(&v).ok()?);
    }
    Some(out)
}

#[cfg(test)]
#[path = "recorder_events_test.rs"]
mod recorder_events_test;
