//! The export task: an editing agent that cuts a recorded review session into a video.
//!
//! The agent never sees video. It gets what the recorder wrote down (`RECORDER.md`, sections 3 to
//! 6): the facts of the session (its length, the runs of the one clock, the size of the screen, the
//! camera), a summary of what happened (the files visited and for how long, the conversations and how
//! they ended, the boxes of the pen's strokes, the markers the reviewer set, the idle gaps), what was
//! said, and what the user wants (a length, a style). It answers with a CUT LIST, thin on purpose:
//! segments (in and out, a zoom, the camera, a title) and chapters, in the fields of the recorder's
//! EDL schema 1, with every time in microseconds on the session clock.
//!
//! The model answers in SECONDS (it reads seconds in the summary: a multiplication by a million is
//! where a model slips), [`parse_body`] converts. What it does not trust, it fixes and says so: a
//! segment is cut at the gaps between runs (the clock stops there), put in time order and trimmed
//! where it overlaps the one before, moved to a word boundary when its edge is inside a word, its zoom
//! fitted to the screen's aspect ratio inside the screen, its speed held to 0.5 to 4; the captions are
//! not asked for at all, they are made from the transcript (the words that fall inside the segment,
//! with their times), so a caption can never say what was not said.
//!
//! [`CutList::to_edl`] puts the cut list on the recorder's own [`Edl`] (`recorder_edl.rs`: the zoom,
//! the caption and its words are that module's types, shared and not copied), on top of the cut list
//! the recorder makes for the session without judgement: the picture's size, the style and the
//! captions' setting are the recorder's, the segments, the chapters and the title are the agent's.
//! [`cut_list_json`] writes it as the agent's own answer (what the mock sends).

pub use crate::recorder_edl::{Caption, CaptionWord, Zoom};
use crate::{
    agent_backend::{AgentError, Call, Request, Tools},
    agent_json::{self, json, Json},
    agent_limits::{cap_chars, Limits, Notes},
    recorder_edl::{AudioCut, Chapter as EdlChapter, Edl, EdlSegment, Generator},
    recorder_manifest::{Manifest, TranscriptLine},
};

// ==== The request ====

/// One word of the transcript, with its place on the session clock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub w: String,
    pub t0_us: u64,
    pub t1_us: u64,
}

/// A stretch of speech.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Speech {
    pub t0_us: u64,
    pub t1_us: u64,
    pub text: String,
    pub words: Vec<Word>,
}

/// What the manifest says of the session.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionFacts {
    /// The session's folder name (`2026-11-26_10-00-00`).
    pub id: String,
    /// How long the clock ran.
    pub duration_us: u64,
    /// The stretches that were recorded, in order: the clock stops between them.
    pub runs: Vec<(u64, u64)>,
    /// The screen track in its own pixels.
    pub screen: (u32, u32),
    /// Device pixels to a point on the screen track (2.0 on a Retina display): the events' places
    /// are points, the zoom's are the track's pixels.
    pub scale: f32,
    /// There is a camera track.
    pub camera: bool,
}

/// A file the reviewer had open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSpan {
    pub file: String,
    pub t0_us: u64,
    pub t1_us: u64,
}

/// A conversation with an agent and how it ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvSpan {
    pub id: u64,
    pub file: String,
    pub line: usize,
    pub t0_us: u64,
    pub t1_us: u64,
    /// `accepted`, `rejected`, `answered`, `open`.
    pub outcome: String,
}

/// Where the pen worked.
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeBox {
    pub t_us: u64,
    /// `[x0, y0, x1, y1]` in the screen track's pixels.
    pub bbox: [u32; 4],
}

/// A moment the reviewer marked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub t_us: u64,
    pub label: String,
}

/// What happened, boiled down by the recorder.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub files: Vec<FileSpan>,
    pub conversations: Vec<ConvSpan>,
    pub strokes: Vec<StrokeBox>,
    pub markers: Vec<Marker>,
    /// Stretches of the clock in which nothing happened.
    pub idle: Vec<(u64, u64)>,
}

/// What the user wants of the video.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Target {
    /// About this long, seconds (none: as long as the story needs).
    pub length_s: Option<u32>,
    /// A few words: "a tight walkthrough for colleagues".
    pub style: String,
}

/// What an export is asked.
#[derive(Debug, Clone, PartialEq)]
pub struct ExportRequest {
    pub conversation: u64,
    /// The language of the titles.
    pub language: String,
    pub session: SessionFacts,
    pub summary: Summary,
    pub transcript: Vec<Speech>,
    pub target: Target,
}

// ==== The answer ====

/// One piece of the video: a stretch of the session clock.
#[derive(Debug, Clone, PartialEq)]
pub struct CutSegment {
    pub in_us: u64,
    pub out_us: u64,
    /// The camera is in the picture.
    pub camera: bool,
    /// 0.5 to 4.
    pub speed: f32,
    pub zoom: Option<Zoom>,
    pub caption: Option<Caption>,
    pub title: Option<String>,
}

/// A chapter of the video as the agent places it: on the SESSION clock, where the thing it names
/// happened (the recorder's own chapters are on the video's timeline: [`CutList::to_edl`] moves them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chapter {
    pub t_us: u64,
    pub title: String,
}

/// What the agent decided.
#[derive(Debug, Clone, PartialEq)]
pub struct CutList {
    pub title: String,
    pub segments: Vec<CutSegment>,
    pub chapters: Vec<Chapter>,
}

impl CutList {
    /// How long the video is: the segments one after the other, each at its speed.
    #[must_use]
    pub fn output_us(&self) -> u64 {
        self.segments
            .iter()
            .map(|s| {
                let length = s.out_us.saturating_sub(s.in_us) as f64;
                (length / f64::from(s.speed.max(0.1))) as u64
            })
            .sum()
    }
}

// ==== From the recorder's files ====

impl SessionFacts {
    /// What `manifest` says: how long the clock ran, the runs (an open one ends where the session
    /// does), the screen track's size and scale, whether there is a camera track.
    #[must_use]
    pub fn from_manifest(manifest: &Manifest) -> Self {
        let end = manifest.end_us();
        Self {
            id: manifest.session.clone(),
            duration_us: end,
            runs: manifest
                .runs
                .iter()
                .map(|r| (r.start_us, r.end_us.unwrap_or(end)))
                .filter(|(a, b)| b > a)
                .collect(),
            screen: manifest
                .tracks
                .screen
                .as_ref()
                .map_or((0, 0), |t| (t.width, t.height)),
            scale: manifest.tracks.screen.as_ref().map_or(1.0, |t| t.scale),
            camera: manifest
                .tracks
                .camera
                .as_ref()
                .is_some_and(|t| !t.segments.is_empty()),
        }
    }
}

/// The lines of `transcript.jsonl` (`recorder_manifest::parse_transcript`) as a request has them.
#[must_use]
pub fn speeches(lines: &[TranscriptLine]) -> Vec<Speech> {
    lines
        .iter()
        .map(|l| Speech {
            t0_us: l.t0_us,
            t1_us: l.t1_us,
            text: l.text.clone(),
            words: l
                .words
                .iter()
                .map(|w| Word {
                    w: w.w.clone(),
                    t0_us: w.t0_us,
                    t1_us: w.t1_us,
                })
                .collect(),
        })
        .collect()
}

/// The most events of the log that are read, and the longest line of it (a pen stroke's line is the
/// longest the recorder writes, and the summary needs none of those).
const MAX_EVENT_LINES: usize = 4_000_000;
const MAX_EVENT_LINE_BYTES: usize = 64 * 1024;
/// The most of each list a summary keeps (more are spread over the session).
const MAX_SUMMARY_ITEMS: usize = 2_000;
/// The most conversations that are followed.
const MAX_CONVERSATIONS: usize = 20_000;
/// A pen that is down and sends nothing for this long has drawn two strokes.
const STROKE_GAP_US: u64 = 1_500_000;

/// The kind of an event line (`{"t": 12, "k": "nav.file", ...}`) without parsing the line.
fn kind_of(line: &str) -> Option<&str> {
    let after = line[line.find("\"k\"")? + 3..]
        .trim_start()
        .strip_prefix(':')?;
    let rest = after.trim_start().strip_prefix('"')?;
    let kind = &rest[..rest.find('"')?];
    (!kind.is_empty()
        && kind
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.'))
    .then_some(kind)
}

/// A pen stroke being followed: where the pointer was while it was down.
struct Pen {
    t0_us: u64,
    last_us: u64,
    samples: u32,
    /// `[x0, y0, x1, y1]` in the screen track's pixels.
    bbox: [f64; 4],
}

fn stroke_box(pen: &Pen, screen: (u32, u32)) -> Option<StrokeBox> {
    let [x0, y0, x1, y1] = pen.bbox;
    if pen.samples < 2 && (x1 - x0).max(y1 - y0) < 4.0 {
        return None;
    }
    let clamp = |v: f64, max: u32| -> u32 {
        let v = v.max(0.0) as u32;
        if max == 0 {
            v
        } else {
            v.min(max)
        }
    };
    Some(StrokeBox {
        t_us: pen.t0_us,
        bbox: [
            clamp(x0.floor(), screen.0),
            clamp(y0.floor(), screen.1),
            clamp(x1.ceil(), screen.0),
            clamp(y1.ceil(), screen.1),
        ],
    })
}

/// What `events_jsonl` (the recorder's `events.jsonl`, `RECORDER.md` section 4) says happened,
/// boiled down for an agent that never sees the video: the files that were open and for how long
/// (`nav.file`), the conversations with an agent and how each ended (`conv.*`), where the pen worked
/// (the pointer's places while it was down, in the screen track's pixels), the moments the reviewer
/// marked (`marker`) and the stretches the recorder paused for idleness (`session.idle_pause`). A
/// line that cannot be read, one that is too long and a kind it does not need are passed over; at
/// most [`MAX_SUMMARY_ITEMS`] of each list are kept, spread over the whole session. The second value
/// says what was passed over and cut.
#[must_use]
pub fn summarize(events_jsonl: &str, facts: &SessionFacts) -> (Summary, Vec<String>) {
    use std::collections::HashMap;

    let mut notes = Notes::new();
    let scale = if facts.scale.is_finite() && facts.scale > 0.0 {
        f64::from(facts.scale)
    } else {
        1.0
    };
    let mut opened: Vec<(u64, String)> = Vec::new();
    let mut conversations: Vec<ConvSpan> = Vec::new();
    let mut by_id: HashMap<u64, usize> = HashMap::new();
    let mut markers: Vec<Marker> = Vec::new();
    let mut idle: Vec<(u64, u64)> = Vec::new();
    let mut strokes: Vec<StrokeBox> = Vec::new();
    let mut pen: Option<Pen> = None;
    let (mut unreadable, mut too_long, mut read) = (0_usize, 0_usize, 0_usize);
    for line in events_jsonl.lines() {
        if line.trim().is_empty() {
            continue;
        }
        read += 1;
        if read > MAX_EVENT_LINES {
            notes.push(format!("only the first {MAX_EVENT_LINES} events were read"));
            break;
        }
        let Some(kind) = kind_of(line) else {
            unreadable += 1;
            continue;
        };
        let wanted = matches!(
            kind,
            "nav.file" | "marker" | "session.idle_pause" | "pointer.move"
        ) || kind.starts_with("conv.");
        if !wanted {
            continue;
        }
        if line.len() > MAX_EVENT_LINE_BYTES {
            too_long += 1;
            continue;
        }
        let Ok(v) = agent_json::parse(line, MAX_EVENT_LINE_BYTES) else {
            unreadable += 1;
            continue;
        };
        let t = agent_json::u64_of(&v, "t").unwrap_or(0);
        match kind {
            "nav.file" => {
                if let Some(file) = agent_json::str_of(&v, "file") {
                    if opened.last().is_none_or(|(_, f)| f != file) {
                        opened.push((t, cap_chars(file, 400).0));
                    }
                }
            }
            "marker" => {
                let label = agent_json::str_of(&v, "label").unwrap_or("").trim();
                markers.push(Marker {
                    t_us: t,
                    label: if label.is_empty() {
                        "marker".to_string()
                    } else {
                        cap_chars(label, 200).0
                    },
                });
            }
            "session.idle_pause" => {
                let ms = agent_json::u64_of(&v, "idle_ms").unwrap_or(0);
                if ms > 0 {
                    idle.push((t.saturating_sub(ms.saturating_mul(1_000)), t));
                }
            }
            "pointer.move" => {
                let down = agent_json::bool_of(&v, "down").unwrap_or(false);
                let place = agent_json::f64_of(&v, "x")
                    .zip(agent_json::f64_of(&v, "y"))
                    .filter(|(x, y)| x.is_finite() && y.is_finite());
                match (down, place) {
                    (true, Some((x, y))) => {
                        let (x, y) = (x * scale, y * scale);
                        match &mut pen {
                            Some(p) if t.saturating_sub(p.last_us) <= STROKE_GAP_US => {
                                p.last_us = t;
                                p.samples += 1;
                                p.bbox = [
                                    p.bbox[0].min(x),
                                    p.bbox[1].min(y),
                                    p.bbox[2].max(x),
                                    p.bbox[3].max(y),
                                ];
                            }
                            _ => {
                                if let Some(done) =
                                    pen.take().and_then(|p| stroke_box(&p, facts.screen))
                                {
                                    strokes.push(done);
                                }
                                pen = Some(Pen {
                                    t0_us: t,
                                    last_us: t,
                                    samples: 1,
                                    bbox: [x, y, x, y],
                                });
                            }
                        }
                    }
                    _ => {
                        if let Some(done) = pen.take().and_then(|p| stroke_box(&p, facts.screen)) {
                            strokes.push(done);
                        }
                    }
                }
            }
            conv => {
                let Some(id) = agent_json::u64_of(&v, "id") else {
                    continue;
                };
                if conv == "conv.start" {
                    if conversations.len() < MAX_CONVERSATIONS && !by_id.contains_key(&id) {
                        by_id.insert(id, conversations.len());
                        conversations.push(ConvSpan {
                            id,
                            file: cap_chars(agent_json::str_of(&v, "file").unwrap_or(""), 400).0,
                            line: agent_json::u64_of(&v, "line")
                                .and_then(|n| usize::try_from(n).ok())
                                .unwrap_or(0),
                            t0_us: t,
                            t1_us: t,
                            outcome: "open".to_string(),
                        });
                    }
                } else if let Some(c) = by_id.get(&id).and_then(|i| conversations.get_mut(*i)) {
                    let word_end = if conv == "conv.word" {
                        agent_json::u64_of(&v, "t1_us").unwrap_or(0)
                    } else {
                        0
                    };
                    c.t1_us = c.t1_us.max(t).max(word_end);
                    match conv {
                        "conv.accept" => c.outcome = "accepted".to_string(),
                        "conv.reject" if c.outcome != "accepted" => {
                            c.outcome = "rejected".to_string();
                        }
                        "conv.suggestion"
                            if agent_json::str_of(&v, "op") == Some("answer")
                                && c.outcome == "open" =>
                        {
                            c.outcome = "answered".to_string();
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    if let Some(done) = pen.take().and_then(|p| stroke_box(&p, facts.screen)) {
        strokes.push(done);
    }
    if unreadable > 0 {
        notes.push(format!("{unreadable} event lines could not be read"));
    }
    if too_long > 0 {
        notes.push(format!(
            "{too_long} event lines over {MAX_EVENT_LINE_BYTES} bytes were passed over"
        ));
    }
    // the files: each is open until the next one is opened (the last one until the session's end)
    let mut files: Vec<FileSpan> = opened
        .iter()
        .enumerate()
        .map(|(i, (t0, file))| FileSpan {
            file: file.clone(),
            t0_us: *t0,
            t1_us: opened
                .get(i + 1)
                .map_or(facts.duration_us.max(*t0), |next| next.0.max(*t0)),
        })
        .collect();
    files.sort_by_key(|f| f.t0_us);
    conversations.sort_by_key(|c| c.t0_us);
    markers.sort_by_key(|m| m.t_us);
    idle.sort_unstable();
    strokes.sort_by_key(|b| b.t_us);
    let mut keep = |what: &str, n: usize| -> usize {
        if n > MAX_SUMMARY_ITEMS {
            notes.push(format!(
                "{n} {what} were found, {MAX_SUMMARY_ITEMS} are kept, spread over the session"
            ));
        }
        n.min(MAX_SUMMARY_ITEMS)
    };
    let summary = Summary {
        files: spread(&files, keep("files", files.len())),
        conversations: spread(&conversations, keep("conversations", conversations.len())),
        strokes: spread(&strokes, keep("strokes", strokes.len())),
        markers: spread(&markers, keep("markers", markers.len())),
        idle: spread(&idle, keep("idle gaps", idle.len())),
    };
    (summary, notes.finish())
}

// ==== The prompt ====

/// The standing instructions of the export agent.
pub const EXPORT_SYSTEM: &str = "\
You are the editing agent of AzReview's video export. A reviewer recorded a session of reviewing code: the screen, their camera and their voice, and AzReview wrote down everything they did. You never see video: you get the facts of the session, a summary of what happened and what was said, and you return a CUT LIST for a video a colleague will watch. You decide only what the video contains; a program renders it.

Reply with exactly one JSON object and nothing else (no prose, no code fence):
{\"title\": \"a short title for the video\",
 \"segments\": [
  {\"in_s\": 0.0, \"out_s\": 93.5, \"title\": \"src/lib.rs\", \"camera\": true,
   \"zoom\": {\"x\": 400, \"y\": 300, \"w\": 1440, \"h\": 900}}
 ],
 \"chapters\": [{\"t_s\": 0.0, \"title\": \"src/lib.rs\"}]}

Rules:
- Every time is in seconds on the session clock, the same clock as the summary and the transcript.
- Segments are in time order, do not overlap, and never span a gap between two runs: the video is the segments one after the other.
- Cut what a viewer does not need: silence, long pauses, idle gaps, dead ends, repeats. Cut between sentences, never in the middle of one. Keep what the reviewer explains, the pen at work, the conversations with an agent and how they ended.
- Aim for the target length when one is given (up to a third more is fine); otherwise keep the story whole and tight.
- zoom is a rectangle of the screen in the screen's own pixels, with the screen's aspect ratio; use it where the action is small (the boxes of the pen's strokes are in the summary); leave it out to show the whole screen. speed (0.5 to 4) is optional; leave it out for normal speed.
- camera is true while the reviewer speaks to the viewer, false when the camera adds nothing. title (of a segment, of a chapter, of the video) is short and in the language of the titles.
- Make at least one chapter for every file that matters. Do not write captions: they are made from the transcript.";

/// The seconds of a time, one decimal.
fn secs(us: u64) -> String {
    format!("{:.1}", us as f64 / 1_000_000.0)
}

/// What the prompt is made of, after the caps.
struct Parts<'a> {
    request: &'a ExportRequest,
    strokes: &'a [StrokeBox],
    files: &'a [FileSpan],
    conversations: &'a [ConvSpan],
    markers: &'a [Marker],
    idle: &'a [(u64, u64)],
    speeches: Vec<(u64, u64, String)>,
}

fn assemble(p: &Parts<'_>) -> String {
    let r = p.request;
    let mut out = String::from("Task: export\n");
    if !r.language.trim().is_empty() {
        out.push_str(&format!(
            "Language of the titles: {}\n",
            cap_chars(r.language.trim(), 35).0
        ));
    }
    let s = &r.session;
    out.push_str(&format!(
        "Session {}: {} s on the clock; the screen is {}x{} px; camera: {}\n",
        cap_chars(&s.id, 80).0,
        secs(s.duration_us),
        s.screen.0,
        s.screen.1,
        if s.camera { "yes" } else { "no" }
    ));
    let runs: Vec<String> = s
        .runs
        .iter()
        .map(|(a, b)| format!("{}-{}", secs(*a), secs(*b)))
        .collect();
    out.push_str(&format!(
        "Runs (the clock stops between them; a segment may not cross a gap): {} s\n",
        runs.join(", ")
    ));
    let target = match r.target.length_s {
        Some(n) => format!("about {n} s"),
        None => "as long as the story needs".to_string(),
    };
    out.push_str(&format!("Target: {target}"));
    if !r.target.style.trim().is_empty() {
        out.push_str(&format!(
            "; style: {}",
            cap_chars(&r.target.style.replace('\n', " "), 160).0
        ));
    }
    out.push('\n');
    if !p.files.is_empty() {
        out.push_str("\nFiles open (seconds on the clock):\n");
        for f in p.files {
            out.push_str(&format!(
                "- {}-{}: {}\n",
                secs(f.t0_us),
                secs(f.t1_us),
                cap_chars(&f.file, 200).0
            ));
        }
    }
    if !p.conversations.is_empty() {
        out.push_str("\nConversations with an agent:\n");
        for c in p.conversations {
            out.push_str(&format!(
                "- {}-{}: #{} at {} line {}: {}\n",
                secs(c.t0_us),
                secs(c.t1_us),
                c.id,
                cap_chars(&c.file, 200).0,
                c.line,
                cap_chars(&c.outcome, 40).0
            ));
        }
    }
    if !p.strokes.is_empty() {
        out.push_str("\nThe pen's strokes (second, box x0,y0,x1,y1 in screen px):\n");
        for b in p.strokes {
            out.push_str(&format!(
                "- {}: {},{},{},{}\n",
                secs(b.t_us),
                b.bbox[0],
                b.bbox[1],
                b.bbox[2],
                b.bbox[3]
            ));
        }
    }
    if !p.markers.is_empty() {
        out.push_str("\nMoments the reviewer marked:\n");
        for m in p.markers {
            out.push_str(&format!(
                "- {}: {}\n",
                secs(m.t_us),
                cap_chars(&m.label, 120).0
            ));
        }
    }
    if !p.idle.is_empty() {
        let gaps: Vec<String> = p
            .idle
            .iter()
            .map(|(a, b)| format!("{}-{}", secs(*a), secs(*b)))
            .collect();
        out.push_str(&format!(
            "\nIdle (nothing happened): {} s\n",
            gaps.join(", ")
        ));
    }
    if !p.speeches.is_empty() {
        out.push_str("\nWhat was said (seconds on the clock):\n");
        for (a, b, text) in &p.speeches {
            out.push_str(&format!("[{}-{}] {}\n", secs(*a), secs(*b), text));
        }
    }
    out.push_str("\nReply with the JSON object.\n");
    out
}

/// `n` of `items`, at even steps from the first to the last (all of them when `n` is as many).
fn spread<T: Clone>(items: &[T], n: usize) -> Vec<T> {
    match n {
        0 => Vec::new(),
        1 => items.first().cloned().into_iter().collect(),
        _ if n >= items.len() => items.to_vec(),
        _ => (0..n)
            .map(|k| items[k * (items.len() - 1) / (n - 1)].clone())
            .collect(),
    }
}

/// The prompt for `export` (the call keeps `request` for the mock).
///
/// # Errors
/// [`AgentError::TooLarge`] when even the thinned prompt does not fit `limits.max_prompt_bytes`.
pub fn build_call<'a>(
    request: &'a Request,
    export: &ExportRequest,
    limits: &Limits,
) -> Result<Call<'a>, AgentError> {
    let mut notes = Notes::new();
    let cap = |n: usize, what: &str, notes: &mut Notes, limit: usize| -> usize {
        if n > limit {
            notes.push(format!("{n} {what} were given, {limit} are sent"));
        }
        n.min(limit)
    };
    let items = limits.max_export_items;
    let strokes_n = cap(export.summary.strokes.len(), "strokes", &mut notes, items);
    let files_n = cap(export.summary.files.len(), "files", &mut notes, items);
    let convs_n = cap(
        export.summary.conversations.len(),
        "conversations",
        &mut notes,
        items,
    );
    let markers_n = cap(export.summary.markers.len(), "markers", &mut notes, items);
    let idle_n = cap(export.summary.idle.len(), "idle gaps", &mut notes, items);
    let speech_n = cap(
        export.transcript.len(),
        "stretches of speech",
        &mut notes,
        limits.max_export_lines,
    );
    let mut speech_chars = limits.max_line_chars;
    let mut keep_strokes = strokes_n;
    let mut keep_speech = speech_n;
    let mut trimmed: Vec<String> = Vec::new();
    // what is kept of a list that is too long is spread over the whole session (the first of them
    // would be the first minutes of a session of hours)
    let files = spread(&export.summary.files, files_n);
    let conversations = spread(&export.summary.conversations, convs_n);
    let markers = spread(&export.summary.markers, markers_n);
    let idle = spread(&export.summary.idle, idle_n);
    loop {
        let strokes = spread(&export.summary.strokes, keep_strokes);
        let parts = Parts {
            request: export,
            strokes: &strokes,
            files: &files,
            conversations: &conversations,
            markers: &markers,
            idle: &idle,
            speeches: spread(&export.transcript, keep_speech)
                .iter()
                .map(|s| (s.t0_us, s.t1_us, cap_chars(s.text.trim(), speech_chars).0))
                .collect(),
        };
        let prompt = assemble(&parts);
        let bytes = EXPORT_SYSTEM.len() + prompt.len();
        if bytes <= limits.max_prompt_bytes {
            if !trimmed.is_empty() {
                notes.push(format!(
                    "the prompt was trimmed to fit {} bytes: {}",
                    limits.max_prompt_bytes,
                    trimmed.join(", ")
                ));
            }
            return Ok(Call {
                request,
                system: EXPORT_SYSTEM.to_string(),
                prompt,
                workdir: None,
                tools: Tools::None,
                diagnostics: notes.finish(),
            });
        }
        if keep_strokes > 8 {
            keep_strokes /= 2;
            trimmed.retain(|t| !t.starts_with("strokes"));
            trimmed.push(format!("strokes down to {keep_strokes}"));
        } else if speech_chars > 60 {
            speech_chars = (speech_chars / 2).max(60);
            trimmed.retain(|t| !t.starts_with("speech lines"));
            trimmed.push(format!("speech lines cut to {speech_chars} characters"));
        } else if keep_speech > 20 {
            keep_speech = (keep_speech * 3 / 4).max(20);
            trimmed.retain(|t| !t.starts_with("stretches"));
            trimmed.push(format!("stretches of speech down to {keep_speech}"));
        } else {
            return Err(AgentError::TooLarge {
                what: "the prompt".to_string(),
                size: bytes,
                limit: limits.max_prompt_bytes,
            });
        }
    }
}

// ==== Reading the answer ====

/// The shortest segment worth keeping, microseconds.
const MIN_SEGMENT_US: u64 = 200_000;
/// How far a cut is moved to a word boundary, microseconds.
const SNAP_US: u64 = 400_000;
/// The speeds a segment may have.
const MIN_SPEED: f32 = 0.5;
const MAX_SPEED: f32 = 4.0;
/// The chapters kept.
const MAX_CHAPTERS: usize = 100;
/// The time a zoom eases in by default, milliseconds.
const DEFAULT_EASE_MS: u32 = 400;

fn first_f64(v: &Json, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|k| agent_json::f64_of(v, k))
}

/// A time in microseconds from the first of `us_keys` (microseconds), `ms_keys` (milliseconds) or
/// `s_keys` (seconds) the object has.
fn time_us(v: &Json, us_keys: &[&str], ms_keys: &[&str], s_keys: &[&str]) -> Option<u64> {
    let (value, scale) = first_f64(v, s_keys)
        .map(|x| (x, 1_000_000.0))
        .or_else(|| first_f64(v, ms_keys).map(|x| (x, 1_000.0)))
        .or_else(|| first_f64(v, us_keys).map(|x| (x, 1.0)))?;
    (value.is_finite() && value >= 0.0 && value < 1.0e15).then(|| (value * scale).round() as u64)
}

/// Where `[a, b)` falls inside the runs: the pieces, in order.
fn within_runs(a: u64, b: u64, runs: &[(u64, u64)]) -> Vec<(u64, u64)> {
    if runs.is_empty() {
        return vec![(a, b)];
    }
    runs.iter()
        .filter_map(|&(r0, r1)| {
            let (x, y) = (a.max(r0), b.min(r1));
            (x < y).then_some((x, y))
        })
        .collect()
}

/// `t` moved back to the start of the word it is inside (a segment does not begin in the middle
/// of a word).
fn snap_in(t: u64, words: &[Word]) -> u64 {
    words
        .iter()
        .find(|w| w.t0_us < t && t < w.t1_us && t - w.t0_us <= SNAP_US)
        .map_or(t, |w| w.t0_us)
}

/// `t` moved on to the end of the word it is inside (nor does it end in the middle of one).
fn snap_out(t: u64, words: &[Word]) -> u64 {
    words
        .iter()
        .find(|w| w.t0_us < t && t < w.t1_us && w.t1_us - t <= SNAP_US)
        .map_or(t, |w| w.t1_us)
}

/// A zoom made to have the screen's aspect ratio and to lie inside the screen.
fn fit_zoom(x: f64, y: f64, w: f64, h: f64, screen: (u32, u32)) -> Option<Zoom> {
    let (sw, sh) = (f64::from(screen.0), f64::from(screen.1));
    if sw < 1.0
        || sh < 1.0
        || !(x.is_finite() && y.is_finite() && w.is_finite() && h.is_finite())
        || w < 1.0
        || h < 1.0
    {
        return None;
    }
    let aspect = sw / sh;
    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
    let mut width = w.max(h * aspect).min(sw);
    let mut height = width / aspect;
    if height > sh {
        height = sh;
        width = height * aspect;
    }
    let left = (cx - width / 2.0).clamp(0.0, sw - width);
    let top = (cy - height / 2.0).clamp(0.0, sh - height);
    Some(Zoom {
        x: left.round() as u32,
        y: top.round() as u32,
        w: width.round().max(1.0) as u32,
        h: height.round().max(1.0) as u32,
        ease_ms: DEFAULT_EASE_MS,
    })
}

/// The words of the transcript that fall inside `[a, b)`, as a caption.
fn caption_of(a: u64, b: u64, words: &[Word], max_words: usize) -> Option<Caption> {
    let inside: Vec<&Word> = words
        .iter()
        .filter(|w| w.t0_us >= a && w.t1_us <= b)
        .take(max_words)
        .collect();
    if inside.is_empty() {
        return None;
    }
    Some(Caption {
        text: inside
            .iter()
            .map(|w| w.w.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        words: inside
            .iter()
            .map(|w| CaptionWord {
                w: w.w.clone(),
                t0_us: w.t0_us,
                t1_us: w.t1_us,
            })
            .collect(),
    })
}

fn segment_of(
    index: usize,
    item: &Json,
    export: &ExportRequest,
    notes: &mut Notes,
) -> Option<CutSegment> {
    let (Some(a), Some(b)) = (
        time_us(
            item,
            &["in_us", "start_us"],
            &["in_ms", "start_ms"],
            &["in_s", "in", "start_s", "start"],
        ),
        time_us(
            item,
            &["out_us", "end_us"],
            &["out_ms", "end_ms"],
            &["out_s", "out", "end_s", "end"],
        ),
    ) else {
        notes.push(format!(
            "segment {index}: dropped: it has no in and out time"
        ));
        return None;
    };
    if b <= a {
        notes.push(format!(
            "segment {index}: dropped: it ends ({} s) before it begins ({} s)",
            secs(b),
            secs(a)
        ));
        return None;
    }
    let speed = agent_json::f64_of(item, "speed").map_or(1.0, |s| s as f32);
    let held = if speed.is_finite() {
        speed.clamp(MIN_SPEED, MAX_SPEED)
    } else {
        1.0
    };
    if (held - speed).abs() > f32::EPSILON {
        notes.push(format!("segment {index}: its speed was held to {held}"));
    }
    let wants_camera = agent_json::bool_of(item, "camera").unwrap_or(export.session.camera);
    if wants_camera && !export.session.camera {
        notes.push(format!(
            "segment {index}: the camera was asked for, the session has none"
        ));
    }
    let zoom = agent_json::get(item, "zoom")
        .filter(|z| z.is_object())
        .and_then(|z| {
            let f = |k: &str| agent_json::f64_of(z, k);
            let fitted = fit_zoom(
                f("x")?,
                f("y")?,
                f("w").or_else(|| f("width"))?,
                f("h").or_else(|| f("height"))?,
                export.session.screen,
            );
            if fitted.is_none() {
                notes.push(format!(
                    "segment {index}: its zoom was dropped: it is no region of the screen"
                ));
            }
            fitted.map(|mut zoom| {
                if let Some(ease) = agent_json::u64_of(z, "ease_ms") {
                    zoom.ease_ms = u32::try_from(ease.min(5_000)).unwrap_or(DEFAULT_EASE_MS);
                }
                zoom
            })
        });
    let title = agent_json::str_of(item, "title")
        .map(|t| cap_chars(t.trim(), 120).0)
        .filter(|t| !t.is_empty());
    Some(CutSegment {
        in_us: a,
        out_us: b,
        camera: wants_camera && export.session.camera,
        speed: held,
        zoom,
        caption: None,
        title,
    })
}

/// Reads the text of a reply as the cut list for `export`.
///
/// # Errors
/// [`AgentError::BadOutput`] when there is no cut list in it, or none of its segments is usable.
pub fn parse_body(
    export: &ExportRequest,
    text: &str,
    limits: &Limits,
    notes: &mut Notes,
) -> Result<CutList, AgentError> {
    let bad = |why: &str| AgentError::BadOutput {
        why: why.to_string(),
        excerpt: cap_chars(text.trim(), 200).0,
    };
    let (text, cut) = crate::agent_limits::cap_bytes(text, limits.max_reply_bytes);
    if cut {
        notes.push(format!(
            "the reply was cut at {} bytes",
            limits.max_reply_bytes
        ));
    }
    let found = agent_json::find_object_where(text, limits.max_reply_bytes, &|v| {
        agent_json::array_of(v, "segments").is_some() || agent_json::array_of(v, "cuts").is_some()
    })
    .map_err(|e| bad(&format!("{e:?}")))?
    .ok_or_else(|| bad("no JSON object with segments could be read from it"))?;
    if found.repaired {
        notes.push("the reply's JSON had a line break inside a string or a comma before a bracket: it was repaired before it was read");
    }
    let value = found.value;
    let items = agent_json::array_of(&value, "segments")
        .or_else(|| agent_json::array_of(&value, "cuts"))
        .unwrap_or(&[]);
    if items.len() > limits.max_cut_segments {
        notes.push(format!(
            "{} segments were given, the first {} are kept",
            items.len(),
            limits.max_cut_segments
        ));
    }
    let words: Vec<Word> = export
        .transcript
        .iter()
        .flat_map(|s| s.words.iter().cloned())
        .collect();
    // an unknown length (0) is no limit; no runs is one run that is all of it
    let duration = if export.session.duration_us == 0 {
        u64::MAX
    } else {
        export.session.duration_us
    };
    let runs: Vec<(u64, u64)> = if export.session.runs.is_empty() {
        vec![(0, duration)]
    } else {
        export.session.runs.clone()
    };
    let mut raw: Vec<(u64, u64, CutSegment)> = Vec::new();
    for (i, item) in items.iter().enumerate().take(limits.max_cut_segments) {
        let Some(segment) = segment_of(i + 1, item, export, notes) else {
            continue;
        };
        let (a, b) = (segment.in_us.min(duration), segment.out_us.min(duration));
        // cut at the gaps between runs: the clock stops there
        let pieces = within_runs(a, b, &runs);
        if pieces.len() > 1 {
            notes.push(format!(
                "segment {}: it spanned a gap between two runs and was cut there",
                i + 1
            ));
        }
        if pieces.is_empty() {
            notes.push(format!(
                "segment {}: dropped: it lies outside the recorded runs",
                i + 1
            ));
        }
        for (x, y) in pieces {
            raw.push((
                x,
                y,
                CutSegment {
                    in_us: x,
                    out_us: y,
                    ..segment.clone()
                },
            ));
        }
    }
    raw.sort_by_key(|(a, _, _)| *a);
    let mut segments: Vec<CutSegment> = Vec::new();
    for (a, b, mut segment) in raw {
        let a = snap_in(a, &words);
        let b = snap_out(b, &words);
        let start = segments.last().map_or(a, |prev| a.max(prev.out_us));
        if start != a {
            notes.push(format!(
                "a segment began at {} s inside the one before and was moved to {} s",
                secs(a),
                secs(start)
            ));
        }
        if b <= start || b - start < MIN_SEGMENT_US {
            notes.push(format!(
                "a segment of {} to {} s was dropped: nothing is left of it",
                secs(a),
                secs(b)
            ));
            continue;
        }
        segment.in_us = start;
        segment.out_us = b;
        segment.caption = caption_of(start, b, &words, 400);
        segments.push(segment);
    }
    if segments.is_empty() {
        return Err(bad("none of the segments could be used"));
    }
    let title = cap_chars(
        agent_json::str_of(&value, "title").unwrap_or("").trim(),
        200,
    )
    .0;
    let mut chapters: Vec<Chapter> = agent_json::array_of(&value, "chapters")
        .unwrap_or(&[])
        .iter()
        .filter_map(|c| {
            let t = time_us(c, &["t_us"], &["t_ms"], &["t_s", "t", "at_s"])?;
            let title = agent_json::str_of(c, "title")?.trim();
            (!title.is_empty() && t <= duration).then(|| Chapter {
                t_us: t,
                title: cap_chars(title, 120).0,
            })
        })
        .take(MAX_CHAPTERS)
        .collect();
    chapters.sort_by_key(|c| c.t_us);
    chapters.dedup_by(|b, a| a.t_us == b.t_us && a.title == b.title);
    if chapters.is_empty() {
        // one chapter where the segments' title changes
        let mut last: Option<&str> = None;
        for s in &segments {
            if let Some(t) = s.title.as_deref() {
                if last != Some(t) {
                    chapters.push(Chapter {
                        t_us: s.in_us,
                        title: t.to_string(),
                    });
                    last = Some(t);
                }
            }
        }
        if !chapters.is_empty() {
            notes.push("no chapters were given: they were made from the titles of the segments");
        }
    }
    let list = CutList {
        title,
        segments,
        chapters,
    };
    if let Some(target) = export.target.length_s {
        let out = list.output_us() / 1_000_000;
        if out > u64::from(target) * 4 / 3 + 1 {
            notes.push(format!(
                "the cut list is {out} s long, the target was about {target} s"
            ));
        }
    }
    Ok(list)
}

// ==== Writing it ====

fn zoom_json(z: &Zoom) -> Json {
    json!({"x": z.x, "y": z.y, "w": z.w, "h": z.h, "ease_ms": z.ease_ms})
}

/// A cut list as the agent's own answer (what the mock sends and a test reads back): times in seconds.
#[must_use]
pub fn cut_list_json(list: &CutList) -> Json {
    let segments: Vec<Json> = list
        .segments
        .iter()
        .map(|s| {
            let mut o = json!({
                "in_s": s.in_us as f64 / 1_000_000.0,
                "out_s": s.out_us as f64 / 1_000_000.0,
                "camera": s.camera,
            });
            if let Some(map) = o.as_object_mut() {
                if (s.speed - 1.0).abs() > f32::EPSILON {
                    map.insert("speed".to_string(), json!(f64::from(s.speed)));
                }
                if let Some(z) = &s.zoom {
                    map.insert("zoom".to_string(), zoom_json(z));
                }
                if let Some(t) = &s.title {
                    map.insert("title".to_string(), json!(t));
                }
            }
            o
        })
        .collect();
    let chapters: Vec<Json> = list
        .chapters
        .iter()
        .map(|c| json!({"t_s": c.t_us as f64 / 1_000_000.0, "title": c.title}))
        .collect();
    json!({"title": list.title, "segments": segments, "chapters": chapters})
}

/// `list` as text, as a model would write it.
#[must_use]
pub fn cut_list_text(list: &CutList) -> String {
    agent_json::to_text(&cut_list_json(list))
}

impl CutList {
    /// Where `t` of the session clock is on the video's timeline, given the pieces kept (`kept`: each
    /// with the video's time at which it starts): inside a piece, its place in it; in a stretch that
    /// was cut away, where the next piece begins; after the last piece: none.
    fn on_video(kept: &[(u64, &EdlSegment)], t: u64) -> Option<u64> {
        for (start, seg) in kept {
            if t < seg.out_us {
                if t <= seg.in_us {
                    return Some(*start);
                }
                let speed = if seg.speed.is_finite() && seg.speed > 0.0 {
                    seg.speed
                } else {
                    1.0
                };
                let inside = ((t - seg.in_us) as f64 / speed).round() as u64;
                return Some(*start + inside.min(seg.duration_us()));
            }
        }
        None
    }

    /// The cut list as the recorder's [`Edl`] (schema 1): `base` is the cut list the recorder makes for
    /// the same session without judgement (`identity_edl`), which has what the agent does not decide
    /// (the picture's size, the style, the captions' setting, the session's name); this list's segments
    /// (named `s1`, `s2`, ..., the sound as recorded), chapters and title are put on it, and `note` (the
    /// backend's name, say) is kept in the generator. Chapters move from the session's clock to the
    /// video's: one in a stretch that was cut away begins where the next piece does (the last of
    /// several there is kept), one after the last piece is left out; the order is the video's.
    ///
    /// The result still has to be checked against the session (`Edl::problems`): this does not know
    /// the manifest.
    #[must_use]
    pub fn to_edl(&self, base: &Edl, note: Option<&str>) -> Edl {
        let segments: Vec<EdlSegment> = self
            .segments
            .iter()
            .enumerate()
            .map(|(i, s)| EdlSegment {
                id: format!("s{}", i + 1),
                in_us: s.in_us,
                out_us: s.out_us,
                camera: s.camera,
                speed: (f64::from(s.speed) * 1000.0).round() / 1000.0,
                zoom: s.zoom.clone(),
                // the sound is as recorded: the agent decides the picture and the order, and a later
                // step, not the agent, crossfades the cuts
                audio: AudioCut::default(),
                caption: s.caption.clone(),
                title: s.title.clone(),
            })
            .collect();
        let mut starts = Vec::with_capacity(segments.len());
        let mut offset = 0_u64;
        for s in &segments {
            starts.push(offset);
            offset += s.duration_us();
        }
        let kept: Vec<(u64, &EdlSegment)> = starts.iter().copied().zip(segments.iter()).collect();
        let mut chapters: Vec<EdlChapter> = Vec::new();
        for c in &self.chapters {
            if let Some(t_us) = Self::on_video(&kept, c.t_us) {
                chapters.push(EdlChapter {
                    t_us,
                    title: c.title.clone(),
                });
            }
        }
        chapters.sort_by_key(|c| c.t_us);
        // two chapters at one place: the one that began last is what the video shows there
        let mut deduped: Vec<EdlChapter> = Vec::with_capacity(chapters.len());
        for c in chapters {
            if deduped.last().is_some_and(|last| last.t_us == c.t_us) {
                deduped.pop();
            }
            deduped.push(c);
        }
        let title = self.title.trim();
        Edl {
            title: if title.is_empty() {
                base.title.clone()
            } else {
                Some(title.to_string())
            },
            segments,
            chapters: deduped,
            generator: Generator {
                name: String::from("agent"),
                version: 1,
                notes: note.map(str::to_string),
            },
            ..base.clone()
        }
    }
}

#[cfg(test)]
#[path = "agent_export_test.rs"]
mod agent_export_test;
