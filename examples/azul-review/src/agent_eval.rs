//! What the agents must do with what the reviewer says, as a TABLE: the same cases are run through the
//! mock (the reference of the rules a program can know, a normal test) and, by hand, through the real
//! `claude -p` several times each (a model does not answer twice alike), and the pass rate of every case
//! is the measure of the prompts.
//!
//! A case is a scene (a few lines of code, the word under the stroke), what the reviewer said (with the
//! probabilities the recogniser gave and the words it was told to listen for) and the outcome that is
//! right: an agent that does nothing for a test phrase, nonsense, words too uncertain or too ambiguous to
//! act on; the suggestion a clear instruction asks for; a misheard name put right and said in `heard_as`;
//! a low probability on a word the recogniser was told to listen for that is no doubt, and the same
//! probability on an unlisted word that nothing in the code explains, which is. [`judge`] decides a
//! reply against its case, [`report`] makes the table of pass rates.
//!
//! A test module: it ships nothing.

use std::path::PathBuf;

use crate::{
    agent_backend::{AgentError, Task},
    agent_export::{
        ConvSpan, CutList, ExportRequest, FileSpan, Marker, SessionFacts, Speech, StrokeBox,
        Summary, Target, Word,
    },
    agent_review::{
        Context, PageGrid, Reply, ReviewRequest, SaidWord, Speaker, Suggestion, Turn, Utterance,
        WordRef,
    },
    agent_stroke::{Meaning, Sample, StrokeInput},
    recorder_edl::identity_edl,
    recorder_manifest::{
        AppInfo, ClockInfo, HostInfo, Manifest, Run as ManifestRun, Segment, Track, TrackStatus,
        Workspace,
    },
};

/// The code the reviewer is looking at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scene {
    /// A function with the stroke under `foo`.
    Foo,
    /// The same function with the stroke under `stale`.
    Stale,
    /// The same function with the stroke under `compute_stale_flag`.
    Compute,
    /// An enum with the stroke under its name, `Kind`.
    Enum,
    /// Code that has no `Kind` and nothing under the stroke (the stroke is off any word).
    Plain,
    /// A greeting in a string, with the stroke under `hello`.
    Hello,
}

/// What kind of suggestion a case wants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Remove,
    Replace,
    Insert,
}

/// The outcome that is right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// The agent does nothing: no answer, no suggestion.
    NoAction,
    /// The first suggestion is of `kind` and (for a replace or a remove) is about `touching`.
    Suggests { kind: Kind, touching: &'static str },
    /// The question is answered in words, nothing is edited, and the agent did not do nothing.
    Answers,
    /// `Suggests`, and the agent said it heard `heard` (a name it put right).
    Corrects {
        heard: &'static str,
        kind: Kind,
        touching: &'static str,
    },
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub struct Case {
    pub name: &'static str,
    pub task: Task,
    pub scene: Scene,
    /// The transcript as the recogniser wrote it.
    pub said: &'static str,
    /// Its words: the word, the probability (none: not given), told to listen for it.
    pub words: &'static [(&'static str, Option<f32>, bool)],
    pub expect: Expect,
    /// Run on the stronger model too.
    pub important: bool,
    /// The mock answers it right: a rule a program can know (the rest takes a model's judgement).
    pub reference: bool,
}

const fn case(
    name: &'static str,
    scene: Scene,
    said: &'static str,
    expect: Expect,
    important: bool,
    reference: bool,
) -> Case {
    Case {
        name,
        task: Task::Editing,
        scene,
        said,
        words: &[],
        expect,
        important,
        reference,
    }
}

const fn with_words(case: Case, words: &'static [(&'static str, Option<f32>, bool)]) -> Case {
    Case { words, ..case }
}

const fn exploring(case: Case) -> Case {
    Case {
        task: Task::Exploration,
        ..case
    }
}

/// Words the recogniser was sure of, for a sentence.
const fn sure(w: &'static str) -> (&'static str, Option<f32>, bool) {
    (w, Some(0.93), false)
}

/// The table.
pub static CASES: [Case; 25] = [
    // ---- nothing to do ----
    case(
        "test-phrase-hello-world",
        Scene::Foo,
        "Hello world, this is a test",
        Expect::NoAction,
        true,
        true,
    ),
    case(
        "test-phrase-testing-one-two",
        Scene::Foo,
        "testing one two",
        Expect::NoAction,
        true,
        true,
    ),
    case(
        "test-phrase-can-you-hear-me",
        Scene::Foo,
        "can you hear me",
        Expect::NoAction,
        false,
        true,
    ),
    case(
        "test-phrase-german",
        Scene::Foo,
        "hallo welt, das ist ein test",
        Expect::NoAction,
        false,
        true,
    ),
    case("empty", Scene::Foo, "", Expect::NoAction, false, true),
    case(
        "filler-only",
        Scene::Foo,
        "uh um er hmm",
        Expect::NoAction,
        false,
        true,
    ),
    case(
        "nonsense-unrelated",
        Scene::Foo,
        "the weather is lovely today and I would like a pizza",
        Expect::NoAction,
        true,
        false,
    ),
    case(
        "gibberish",
        Scene::Foo,
        "asdf qwer zxcv jkl",
        Expect::NoAction,
        false,
        false,
    ),
    with_words(
        case(
            "low-probability-guesses",
            Scene::Foo,
            "rename widget gadget",
            Expect::NoAction,
            true,
            true,
        ),
        &[
            ("rename", Some(0.2), false),
            ("widget", Some(0.15), false),
            ("gadget", Some(0.2), false),
        ],
    ),
    case(
        "ambiguous-change-that",
        Scene::Foo,
        "change that",
        Expect::NoAction,
        false,
        true,
    ),
    case(
        "ambiguous-fix-this",
        Scene::Foo,
        "fix this",
        Expect::NoAction,
        false,
        true,
    ),
    case(
        "ambiguous-make-it-better",
        Scene::Foo,
        "make it better",
        Expect::NoAction,
        true,
        false,
    ),
    with_words(
        case(
            "unlisted-low-probability-word",
            Scene::Plain,
            "rename Kind to Size",
            Expect::NoAction,
            true,
            true,
        ),
        &[
            sure("rename"),
            ("Kind", Some(0.32), false),
            sure("to"),
            sure("Size"),
        ],
    ),
    exploring(case(
        "exploration-test-phrase",
        Scene::Foo,
        "testing one two",
        Expect::NoAction,
        false,
        true,
    )),
    // ---- a clear instruction is done ----
    case(
        "clear-remove-this-line",
        Scene::Stale,
        "remove this line",
        Expect::Suggests {
            kind: Kind::Remove,
            touching: "stale",
        },
        true,
        true,
    ),
    case(
        "clear-remove-this",
        Scene::Stale,
        "remove this",
        Expect::Suggests {
            kind: Kind::Remove,
            touching: "stale",
        },
        true,
        true,
    ),
    case(
        "clear-rename",
        Scene::Foo,
        "rename foo to count",
        Expect::Suggests {
            kind: Kind::Replace,
            touching: "foo",
        },
        true,
        true,
    ),
    case(
        "clear-comment",
        Scene::Foo,
        "add a comment above this line saying that it scales the width",
        Expect::Suggests {
            kind: Kind::Insert,
            touching: "",
        },
        false,
        true,
    ),
    case(
        "clear-german-remove",
        Scene::Stale,
        "entferne das",
        Expect::Suggests {
            kind: Kind::Remove,
            touching: "stale",
        },
        false,
        true,
    ),
    case(
        "clear-with-test-words-in-it",
        Scene::Hello,
        "replace hello world with goodbye world",
        Expect::Suggests {
            kind: Kind::Replace,
            touching: "hello",
        },
        true,
        true,
    ),
    exploring(case(
        "exploration-question",
        Scene::Foo,
        "what does foo hold",
        Expect::Answers,
        false,
        true,
    )),
    // ---- what was heard is put right ----
    with_words(
        case(
            "misheard-identifier",
            Scene::Foo,
            "rename food to count",
            Expect::Corrects {
                heard: "foo",
                kind: Kind::Replace,
                touching: "foo",
            },
            true,
            true,
        ),
        &[
            sure("rename"),
            ("food", Some(0.35), false),
            sure("to"),
            sure("count"),
        ],
    ),
    case(
        "misheard-spoken-compound",
        Scene::Compute,
        "rename compute stale flag to check freshness",
        Expect::Corrects {
            heard: "compute_stale_flag",
            kind: Kind::Replace,
            touching: "compute_stale_flag",
        },
        true,
        false,
    ),
    with_words(
        case(
            "biased-low-probability-identifier",
            Scene::Enum,
            "rename Kind to Size",
            Expect::Suggests {
                kind: Kind::Replace,
                touching: "Kind",
            },
            true,
            true,
        ),
        &[
            sure("rename"),
            ("Kind", Some(0.32), true),
            sure("to"),
            sure("Size"),
        ],
    ),
    case(
        "question-is-answered",
        Scene::Stale,
        "why is this here",
        Expect::Answers,
        false,
        false,
    ),
];

// ==== The scenes ====

/// The lines of a scene, the number of its first line, the word under the stroke (line, column, text)
/// and its file.
fn scene_code(
    scene: Scene,
) -> (
    &'static str,
    usize,
    &'static [&'static str],
    Option<(usize, usize, &'static str)>,
) {
    const RENDER: &[&str] = &[
        "fn render_frame(frame: &Frame, scale: f32) -> Image {",
        "    let foo = frame.width() * scale;",
        "    let stale = compute_stale_flag(frame);",
        "    // TODO remove once the cache is gone",
        "    draw(frame, foo)",
        "}",
    ];
    match scene {
        Scene::Foo => ("src/render.rs", 10, RENDER, Some((11, 8, "foo"))),
        Scene::Stale => ("src/render.rs", 10, RENDER, Some((12, 8, "stale"))),
        Scene::Compute => (
            "src/render.rs",
            10,
            RENDER,
            Some((12, 20, "compute_stale_flag")),
        ),
        Scene::Enum => (
            "src/kind.rs",
            3,
            &[
                "enum Kind { Small, Large }",
                "",
                "fn size_of(kind: Kind) -> u32 {",
                "    match kind { Kind::Small => 1, Kind::Large => 2 }",
                "}",
            ],
            Some((3, 5, "Kind")),
        ),
        Scene::Plain => (
            "src/lib.rs",
            1,
            &[
                "pub fn run(input: &str) -> usize {",
                "    let value = compute(input);",
                "    let other = value + 2;",
                "    other",
                "}",
            ],
            None,
        ),
        Scene::Hello => (
            "src/main.rs",
            1,
            &["fn main() {", "    println!(\"hello world\");", "}"],
            Some((2, 14, "hello")),
        ),
    }
}

/// The request of `case`: the scene's code, one stroke under the word, the words as the recogniser gave
/// them.
#[must_use]
pub fn request_of(case: &Case) -> ReviewRequest {
    let (file, first_line, lines, under) = scene_code(case.scene);
    let grid = PageGrid {
        first_line,
        origin_x: 48.0,
        origin_y: 100.0,
        line_height: 16.0,
        char_width: 8.0,
    };
    // a level line under the word (or across a few columns of the first line when there is none)
    let (line, col, len) = under.map_or((first_line, 0, 4), |(l, c, w)| (l, c, w.len()));
    let y = grid.origin_y + (line - first_line) as f32 * grid.line_height + 14.0;
    let (x0, x1) = (
        grid.origin_x + col as f32 * grid.char_width - 2.0,
        grid.origin_x + (col + len) as f32 * grid.char_width + 2.0,
    );
    let stroke = StrokeInput {
        id: 1,
        page: 0,
        meaning: Some(Meaning::Issue),
        pinned: true,
        nib: "pen".to_string(),
        nib_width: 2.0,
        t0_ms: 1_000,
        samples: (0..12)
            .map(|i| Sample {
                x: x0 + (x1 - x0) * i as f32 / 11.0,
                y,
                pressure: 0.5,
                dt_ms: i * 40,
                tilt: None,
            })
            .collect(),
    };
    let words: Vec<SaidWord> = case
        .words
        .iter()
        .map(|(w, p, biased)| SaidWord {
            w: (*w).to_string(),
            p: *p,
            biased: *biased,
        })
        .collect();
    ReviewRequest {
        task: case.task,
        conversation: 1,
        language: "en".to_string(),
        utterances: if case.said.is_empty() {
            Vec::new()
        } else {
            vec![Utterance {
                text: case.said.to_string(),
                start_ms: 1_000,
                end_ms: 1_600,
                words,
            }]
        },
        strokes: vec![stroke],
        context: Context {
            file: file.to_string(),
            page: 0,
            first_line,
            lines: lines.iter().map(|l| (*l).to_string()).collect(),
            total_lines: first_line + lines.len(),
            focus: Some((line, line)),
            words_under: under
                .map(|(l, c, w)| {
                    vec![WordRef {
                        stroke: 1,
                        line: l,
                        col: c,
                        text: w.to_string(),
                    }]
                })
                .unwrap_or_default(),
            grid: Some(grid),
        },
        turns: if case.said.is_empty() {
            Vec::new()
        } else {
            vec![Turn {
                from: Speaker::Reviewer,
                text: case.said.to_string(),
            }]
        },
        workspace: None::<PathBuf>,
    }
}

// ==== Judging ====

/// What a reply is worth against its case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub pass: bool,
    /// What was wrong (empty on a pass).
    pub why: String,
}

fn verdict(why: &str) -> Verdict {
    Verdict {
        pass: why.is_empty(),
        why: why.to_string(),
    }
}

/// The text a suggestion is about: what a replace or a remove takes out.
fn about(s: &Suggestion) -> &str {
    match s {
        Suggestion::ReplaceRange { old, .. } | Suggestion::RemoveRange { old, .. } => old,
        Suggestion::InsertLines { .. } => "",
    }
}

fn kind_of(s: &Suggestion) -> Kind {
    match s {
        Suggestion::InsertLines { .. } => Kind::Insert,
        Suggestion::ReplaceRange { .. } => Kind::Replace,
        Suggestion::RemoveRange { .. } => Kind::Remove,
    }
}

fn suggests(reply: &Reply, kind: Kind, touching: &str) -> String {
    if reply.is_no_action() {
        return format!(
            "it did nothing ({})",
            reply.no_action.as_deref().unwrap_or("")
        );
    }
    let Some(first) = reply.suggestions.first() else {
        return "it made no suggestion".to_string();
    };
    if kind_of(first) != kind {
        return format!("its suggestion is a {:?}, not a {kind:?}", kind_of(first));
    }
    if !touching.is_empty() && !about(first).contains(touching) {
        return format!(
            "its suggestion is about {:?}, not {touching:?}",
            about(first)
        );
    }
    String::new()
}

/// Whether `text` says `name` as a whole word (without regard to case), or how the name is spoken
/// (`compute stale flag` for `compute_stale_flag`): `food` is not `foo`.
fn says_name(text: &str, name: &str) -> bool {
    let words = |t: &str| -> String {
        let parts: Vec<String> = t
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .map(str::to_lowercase)
            .collect();
        format!(" {} ", parts.join(" "))
    };
    let said = words(text);
    let spoken = crate::agent_review::spoken(name);
    [Some(name.to_string()), spoken]
        .into_iter()
        .flatten()
        .any(|n| said.contains(&words(&n)))
}

/// Decides `result` against `case`.
#[must_use]
pub fn judge(case: &Case, result: &Result<Reply, AgentError>) -> Verdict {
    let reply = match result {
        Ok(reply) => reply,
        Err(error) => return verdict(&format!("an error: {}", error.message())),
    };
    verdict(&match case.expect {
        Expect::NoAction => {
            if !reply.is_no_action() {
                format!(
                    "it acted: {} suggestion(s), answer {:?}",
                    reply.suggestions.len(),
                    crate::agent_limits::cap_chars(&reply.answer, 60).0
                )
            } else {
                String::new()
            }
        }
        Expect::Suggests { kind, touching } => suggests(reply, kind, touching),
        Expect::Answers => {
            if reply.is_no_action() {
                format!(
                    "it did nothing ({})",
                    reply.no_action.as_deref().unwrap_or("")
                )
            } else if !reply.suggestions.is_empty() {
                "it edited where it was asked a question".to_string()
            } else if reply.answer.trim().is_empty() {
                "it did not answer".to_string()
            } else {
                String::new()
            }
        }
        Expect::Corrects {
            heard,
            kind,
            touching,
        } => {
            let first = suggests(reply, kind, touching);
            if !first.is_empty() {
                first
            } else if !reply
                .heard_as
                .as_deref()
                .is_some_and(|h| says_name(h, heard))
            {
                format!(
                    "heard_as does not say {heard:?}: {:?}",
                    reply.heard_as.as_deref().unwrap_or("")
                )
            } else {
                String::new()
            }
        }
    })
}

/// One run of a case.
#[derive(Debug, Clone)]
pub struct Run {
    pub case: &'static str,
    pub model: String,
    pub verdict: Verdict,
}

/// The table of pass rates: a row for each case and model, `passed/runs`, and what went wrong.
#[must_use]
pub fn report(runs: &[Run]) -> String {
    let mut rows: Vec<(String, String)> = Vec::new();
    for run in runs {
        let key = (run.case.to_string(), run.model.clone());
        if !rows.contains(&key) {
            rows.push(key);
        }
    }
    let mut out = String::from("| case | model | passed | what went wrong |\n|---|---|---|---|\n");
    let (mut total, mut passed) = (0_usize, 0_usize);
    for (case, model) in rows {
        let of: Vec<&Run> = runs
            .iter()
            .filter(|r| r.case == case && r.model == model)
            .collect();
        let ok = of.iter().filter(|r| r.verdict.pass).count();
        total += of.len();
        passed += ok;
        let mut whys: Vec<String> = Vec::new();
        for r in of.iter().filter(|r| !r.verdict.pass) {
            if !whys.contains(&r.verdict.why) {
                whys.push(r.verdict.why.clone());
            }
        }
        out.push_str(&format!(
            "| {case} | {model} | {ok}/{} | {} |\n",
            of.len(),
            whys.join("; ").replace('|', "/")
        ));
    }
    out.push_str(&format!("\ntotal: {passed}/{total}\n"));
    out
}

// ==== The export ====

const S: u64 = 1_000_000;

fn track(video: bool, w: u32, h: u32, segments: Vec<Segment>) -> Track {
    Track {
        source: String::from("mock"),
        device: None,
        codec: String::from(if video { "h264" } else { "opus" }),
        encoder: String::from("mock"),
        hardware: false,
        width: w,
        height: h,
        scale: 2.0,
        fps: 10,
        vfr: video,
        bitrate_kbps: 100,
        sample_rate: if video { 0 } else { 48_000 },
        channels: if video { 0 } else { 1 },
        timescale: if video { 90_000 } else { 48_000 },
        status: TrackStatus::Stopped,
        note: None,
        segments,
    }
}

fn piece(file: &str, from_s: u64, to_s: u64) -> Segment {
    Segment {
        file: file.into(),
        start_us: from_s * S,
        end_us: Some(to_s * S),
        bytes: 1_000,
        samples: 10,
        complete: true,
        recovered: false,
        width: 0,
        height: 0,
    }
}

/// A ten-minute session of two runs (0 to 300 s and 320 to 600 s) on a screen of 2880 x 1800, with a
/// camera and a voice, and the request to cut it into a video of about three minutes.
#[must_use]
pub fn export_fixture() -> (ExportRequest, Manifest) {
    let mut m = Manifest::begin(
        AppInfo {
            name: String::from("AzReview"),
            version: String::from("0.1.0"),
        },
        "2026-11-26_10-00-00",
        Workspace {
            root: String::from("/w"),
            name: String::from("azul"),
        },
        ClockInfo {
            unit: String::from("us"),
            origin_unix_ms: 0,
            origin_iso8601: String::from("1970-01-01T00:00:00Z"),
            utc_offset_minutes: 0,
        },
        HostInfo {
            os: String::from("macos"),
            arch: String::from("aarch64"),
        },
    );
    let both = |dir: &str| {
        vec![
            piece(&format!("{dir}/0001.mp4"), 0, 300),
            piece(&format!("{dir}/0002.mp4"), 320, 600),
        ]
    };
    m.tracks.screen = Some(track(true, 2880, 1800, both("screen")));
    m.tracks.camera = Some(track(true, 1280, 720, both("camera")));
    m.tracks.audio = Some(track(false, 0, 0, both("audio")));
    m.runs = vec![
        ManifestRun {
            start_us: 0,
            end_us: Some(300 * S),
            end_reason: Some(String::from("pause")),
        },
        ManifestRun {
            start_us: 320 * S,
            end_us: Some(600 * S),
            end_reason: Some(String::from("stop")),
        },
    ];
    let word = |w: &str, t0: f64, t1: f64| Word {
        w: w.to_string(),
        t0_us: (t0 * 1e6).round() as u64,
        t1_us: (t1 * 1e6).round() as u64,
    };
    let speech = |text: &str, words: Vec<Word>| Speech {
        t0_us: words.first().map_or(0, |w| w.t0_us),
        t1_us: words.last().map_or(0, |w| w.t1_us),
        text: text.to_string(),
        words,
    };
    let request = ExportRequest {
        conversation: 0,
        language: "en".to_string(),
        session: SessionFacts::from_manifest(&m),
        summary: Summary {
            files: vec![
                FileSpan {
                    file: "src/lib.rs".to_string(),
                    t0_us: 0,
                    t1_us: 250 * S,
                },
                FileSpan {
                    file: "src/main.rs".to_string(),
                    t0_us: 250 * S,
                    t1_us: 600 * S,
                },
            ],
            conversations: vec![ConvSpan {
                id: 3,
                file: "src/lib.rs".to_string(),
                line: 39,
                t0_us: 20 * S,
                t1_us: 60 * S,
                outcome: "accepted".to_string(),
            }],
            strokes: vec![
                StrokeBox {
                    t_us: 15 * S,
                    bbox: [200, 400, 1800, 520],
                },
                StrokeBox {
                    t_us: 210 * S,
                    bbox: [600, 900, 2200, 1000],
                },
            ],
            markers: vec![Marker {
                t_us: 100 * S,
                label: "the interesting part".to_string(),
            }],
            idle: vec![(120 * S, 180 * S)],
        },
        transcript: vec![
            speech(
                "this function leaks the buffer",
                vec![
                    word("this", 10.0, 10.4),
                    word("function", 10.5, 11.1),
                    word("leaks", 11.2, 11.7),
                    word("the", 11.8, 11.9),
                    word("buffer", 12.0, 12.8),
                ],
            ),
            speech(
                "and here is the fix",
                vec![
                    word("and", 200.0, 200.2),
                    word("here", 200.3, 200.6),
                    word("is", 200.7, 200.8),
                    word("the", 200.9, 201.0),
                    word("fix", 201.1, 201.6),
                ],
            ),
        ],
        target: Target {
            length_s: Some(180),
            style: "a tight walkthrough for colleagues".to_string(),
        },
    };
    (request, m)
}

/// Decides a cut list (or the failure to make one) against the session it is for: it must be read, hold
/// at least one segment and at most the cap, and be sound for the session by the recorder's own check
/// once it is put on the recorder's cut list.
#[must_use]
pub fn judge_export(
    result: &Result<CutList, AgentError>,
    manifest: &Manifest,
    cap: usize,
) -> Verdict {
    let list = match result {
        Ok(list) => list,
        Err(error) => return verdict(&format!("an error: {}", error.message())),
    };
    if list.segments.is_empty() {
        return verdict("no segment");
    }
    if list.segments.len() > cap {
        return verdict(&format!(
            "{} segments, the cap is {cap}",
            list.segments.len()
        ));
    }
    let edl = list.to_edl(&identity_edl(manifest, &[]), Some("evaluation"));
    let problems = edl.problems(manifest);
    verdict(&problems.join("; "))
}

#[cfg(test)]
#[path = "agent_eval_test.rs"]
mod agent_eval_test;
