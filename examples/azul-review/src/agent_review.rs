//! The review tasks - exploration and editing - as a request and as an answer.
//!
//! A [`ReviewRequest`] is what the window knows when the reviewer's pen has said something: the
//! words, the strokes (as samples), the code around them, the conversation so far. [`build_call`]
//! turns it into the prompt one model is sent: the strokes become an SVG shape with the pen's
//! widths and times, the words are tied to the strokes that were drawn while they were said, the
//! code is numbered, the columns of the words under the strokes are listed. [`parse_body`] reads
//! the answer: the reviewer's text, and the suggestions ([`Suggestion`]: insert lines after line N,
//! replace a range, remove a range) - checked against the code that was sent, bounded, and
//! tolerant of what models do (a code fence, a sentence before the object, a column that is a few
//! bytes off).
//!
//! Nothing here runs a process or opens a socket: it is text in and text out, and every cut it
//! makes (a long line, too many strokes, a suggestion that points at text that is not there) is a
//! note in the answer's diagnostics.

use std::path::PathBuf;

use crate::{
    agent_backend::{AgentError, Call, Request, Task, Tools},
    agent_json::{self, json, Json},
    agent_limits::{cap_bytes, cap_chars, Limits, Notes},
    agent_stroke::{fmt_num, shape, svg_document, Meaning, Shape, StrokeInput},
    stroke_features::{analyze, describe},
};

// ==== The request ====

/// A word of what the reviewer said, as the recogniser heard it.
#[derive(Debug, Clone, PartialEq)]
pub struct SaidWord {
    pub w: String,
    /// How sure the recogniser was, 0 to 1 (none: it does not say). The raw value, whatever the word.
    pub p: Option<f32>,
    /// The recogniser was told to listen for this word (it is in the vocabulary it was given), so its
    /// probability is often low although the word is right. [`is_biased`] also counts a word that is in
    /// the request's own vocabulary.
    pub biased: bool,
}

/// Something the reviewer said, on the session's clock.
#[derive(Debug, Clone, PartialEq)]
pub struct Utterance {
    /// The transcript as the recogniser wrote it (never changed on its way: what the agent made of it
    /// is [`Reply::heard_as`]).
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    /// The words with their probabilities, when the recogniser gives them (empty: it does not).
    pub words: Vec<SaidWord>,
}

/// A word of the code a stroke is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordRef {
    /// The stroke that is on it.
    pub stroke: u64,
    /// 1-based line of the file.
    pub line: usize,
    /// Byte offset of the word in its line.
    pub col: usize,
    pub text: String,
}

/// Where the code sits on the page, so the agent can read the strokes' coordinates against it (a
/// code page is a grid: one line is one row, one column is one character wide).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageGrid {
    /// The line that starts at `origin_y`.
    pub first_line: usize,
    pub origin_x: f32,
    pub origin_y: f32,
    pub line_height: f32,
    pub char_width: f32,
}

/// The code around the strokes.
#[derive(Debug, Clone, PartialEq)]
pub struct Context {
    /// Workspace-relative path.
    pub file: String,
    /// The page of the file the strokes are on (0-based).
    pub page: usize,
    /// The number of the first of `lines` (1-based).
    pub first_line: usize,
    pub lines: Vec<String>,
    /// How many lines the file has (0: not known).
    pub total_lines: usize,
    /// The lines the strokes cover.
    pub focus: Option<(usize, usize)>,
    pub words_under: Vec<WordRef>,
    pub grid: Option<PageGrid>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Speaker {
    Reviewer,
    Agent,
}

/// One turn of the conversation so far.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub from: Speaker,
    pub text: String,
}

/// What an exploration or an editing is asked.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRequest {
    /// [`Task::Exploration`] or [`Task::Editing`].
    pub task: Task,
    pub conversation: u64,
    /// The reviewer's language (a BCP 47 tag; empty: the language of their words).
    pub language: String,
    /// What the reviewer said so far, timed.
    pub utterances: Vec<Utterance>,
    pub strokes: Vec<StrokeInput>,
    pub context: Context,
    pub turns: Vec<Turn>,
    /// The workspace's folder, where an agent with tools reads from.
    pub workspace: Option<PathBuf>,
}

// ==== The answer ====

/// A place in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    /// 1-based.
    pub line: usize,
    /// Byte offset in the line, 0-based.
    pub col: usize,
}

/// What an agent proposes. The reviewer accepts or rejects each.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Suggestion {
    /// Lines to put after line `after_line` (0: before the first).
    InsertLines {
        after_line: usize,
        lines: Vec<String>,
    },
    /// `old` (the text now in the range) becomes `text`; `end` is exclusive.
    ReplaceRange {
        start: Pos,
        end: Pos,
        old: String,
        text: String,
    },
    /// The text of the range goes.
    RemoveRange { start: Pos, end: Pos, old: String },
}

/// What the agent thinks the strokes without a chosen meaning mean.
#[derive(Debug, Clone, PartialEq)]
pub struct Intent {
    /// None: something else.
    pub meaning: Option<Meaning>,
    /// 0 to 1.
    pub confidence: f32,
    pub rationale: String,
}

/// An agent's answer to a review request.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reply {
    pub answer: String,
    pub suggestions: Vec<Suggestion>,
    pub intent: Option<Intent>,
    /// What the agent understood the reviewer said, when the transcript had to be corrected (a
    /// misheard name resolved against the vocabulary and the code). The raw transcript is not touched.
    pub heard_as: Option<String>,
    /// The agent chose to do nothing, and why (the reason is for the log, never for the reviewer):
    /// then there is no answer, no suggestion and no intent, and the window shows nothing - the
    /// conversation ends quietly.
    pub no_action: Option<String>,
}

impl Reply {
    /// A reply that does nothing, for `reason`.
    #[must_use]
    pub fn nothing(reason: &str) -> Self {
        Self {
            no_action: Some(reason.trim().to_string()),
            ..Self::default()
        }
    }

    /// The agent chose to do nothing.
    #[must_use]
    pub fn is_no_action(&self) -> bool {
        self.no_action.is_some()
    }
}

// ==== The prompt ====

/// When to do nothing, as ONE block of both system prompts.
macro_rules! no_action_rules {
    () => {
        "\
When to do nothing: reply with exactly {\"action\": \"none\", \"reason\": \"one short sentence for the log\"} and nothing else (no answer, no suggestions, no code fence) when:
- the words are nonsense, unrelated to the code at hand, or a test phrase (\"hello world, this is a test\", \"testing one two\", \"can you hear me\");
- they are empty, too short, or made of words the recogniser was unsure of, so that nothing can be acted on;
- they are ambiguous between several different edits, or you cannot resolve them with confidence.
When in doubt, do nothing: never change code on a doubtful input (a wrong edit costs the reviewer more than none, and an answer to nonsense is noise). A clear instruction is not doubtful: do it."
    };
}

/// How to read the words, as ONE block of both system prompts.
macro_rules! speech_rules {
    () => {
        "\
The words are the output of speech-to-text, which mishears: names of identifiers, short words, numbers, words of another language. Each word may come with the probability the recogniser gave it (0 to 1; a low one is a guess), and the request lists a vocabulary of names near the strokes (how a name is spoken is in brackets). Read the words as speech with likely mishearings: resolve a misheard word against that vocabulary and the code shown (a word that sounds like one of those names is that name), and say what you understood, with the mishearings corrected, in \"heard_as\" (one sentence; in a plain-text answer, a first line \"Heard as: ...\"); the raw transcript stays untouched in the note. Leave heard_as out when the words were clear. A word marked biased is one the recogniser was told to listen for (it is in the vocabulary): its probability is often low although the word is right, so a low probability on a biased word is no reason to do nothing. If you cannot resolve the words with confidence, do nothing."
    };
}

/// The block of both system prompts that says when to do nothing.
pub const NO_ACTION_RULES: &str = no_action_rules!();
/// The block of both system prompts that says how to read the words.
pub const SPEECH_RULES: &str = speech_rules!();

/// The standing instructions of the editing agent.
pub const EDITING_SYSTEM: &str = concat!(
    "\
You are the editing agent of AzReview, an ink-first code review app. A reviewer reads source code on pages, marks it with a pen and talks while drawing. You get the strokes, the words said while they were drawn and the code around them. You PROPOSE edits; the reviewer accepts or rejects each one. You never write files and you never say that something was changed.

Reply with exactly one JSON object and nothing else (no prose, no code fence):
{\"answer\": \"one or two plain sentences for the reviewer\",
 \"heard_as\": \"what you understood the reviewer said (only when the words were misheard)\",
 \"suggestions\": [
  {\"kind\": \"insert_lines\", \"after_line\": 41, \"lines\": [\"    let n = items.len();\"]},
  {\"kind\": \"replace\", \"start\": {\"line\": 39, \"col\": 8}, \"end\": {\"line\": 39, \"col\": 11}, \"old\": \"foo\", \"text\": \"bar\"},
  {\"kind\": \"remove\", \"start\": {\"line\": 40, \"col\": 4}, \"end\": {\"line\": 40, \"col\": 9}, \"old\": \"stale\"}
 ]}

Rules:
- Line numbers are the ones shown (1-based). after_line 0 inserts before the first line. col is a byte offset from the start of the line (0-based) and end is exclusive. Copy columns from the list of words under the strokes when you can.
- old is the exact text now in the range: always give it for replace and remove, so the app can check the range.
- Propose only what the reviewer asked for or clearly meant with their words and strokes, and keep each edit small. A comment you write follows the comment syntax of the file.
- If the reviewer asks a question, or you are not sure what they mean, put your reply in answer and send no suggestions.
- \"Shape features\" are hints computed on the reviewer's machine from the strokes (an underline, a circle, a scribble, the words a stroke covers): read them with the SVG; they can be wrong.
- Write answer in the reviewer's language.

",
    no_action_rules!(),
    "

",
    speech_rules!()
);

/// The standing instructions of the exploration agent.
pub const EXPLORATION_SYSTEM: &str = concat!(
    "\
You are the exploration agent of AzReview, an ink-first code review app. A reviewer reads source code on pages, marks it with a pen and talks while drawing. You get the strokes, the words said while they were drawn and the code around them, and you answer their questions about the code. The request also lists \"Shape features\" of the strokes: hints computed on the reviewer's machine (an underline, a circle, the words a stroke covers); they can be wrong. You may read the files of the workspace with the read-only tools you have; you cannot change anything and you never say that something was changed.

Answer in plain text (Markdown is fine): short, concrete, in the reviewer's language; name files and line numbers.
Only if the reviewer asks for a change, end your answer with one fenced json block {\"suggestions\": [...]} whose entries are {\"kind\": \"insert_lines\", \"after_line\": N, \"lines\": [...]}, {\"kind\": \"replace\", \"start\": {\"line\": L, \"col\": C}, \"end\": {\"line\": L, \"col\": C}, \"old\": \"...\", \"text\": \"...\"} or {\"kind\": \"remove\", \"start\": {...}, \"end\": {...}, \"old\": \"...\"}: lines are 1-based as shown, col is a 0-based byte offset, end is exclusive, old is the exact text now in the range.

",
    no_action_rules!(),
    "

",
    speech_rules!()
);

/// The clause that asks for an intent when a stroke has no chosen meaning.
pub const INTENT_CLAUSE: &str = "\
- Some strokes carry no meaning the reviewer chose. Add \"intent\": {\"meaning\": \"scope|issue|question|duplicate|praise|other\", \"confidence\": 0.0 to 1.0, \"rationale\": \"one short sentence\"} saying what you think those strokes mean. A meaning the reviewer chose is final: never change them.";

// ==== The vocabulary ====

/// Names of code that are no name worth listening for (the words every line of a language has).
static PLAIN_WORDS: &[&str] = &[
    "let", "mut", "pub", "use", "mod", "impl", "else", "for", "while", "loop", "return", "self",
    "Self", "true", "false", "def", "class", "import", "from", "var", "const", "function", "int",
    "str", "void", "null", "None", "new", "this", "then", "end", "not", "and", "with", "try",
    "pass", "the", "ref", "dyn", "move", "elif", "bool", "char", "float",
];

/// The names of a line of code, in order: three to forty-eight characters, no number, no word every
/// line has.
fn identifiers(line: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in line.char_indices() {
        let part = c.is_alphanumeric() || c == '_';
        match (part, start) {
            (true, None) => start = Some(i),
            (false, Some(begin)) => {
                out.push(&line[begin..i]);
                start = None;
            }
            _ => {}
        }
    }
    if let Some(begin) = start {
        out.push(&line[begin..]);
    }
    out.retain(|w| {
        (3..=48).contains(&w.chars().count())
            && w.chars()
                .next()
                .is_some_and(|c| c.is_alphabetic() || c == '_')
            && !PLAIN_WORDS.contains(w)
    });
    out
}

/// How a name is spoken: `get_user_name` and `getUserName` are "get user name", `AzReview` is "az
/// review" (none for a name of one part).
#[must_use]
pub fn spoken(name: &str) -> Option<String> {
    let chars: Vec<char> = name.chars().collect();
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, &c) in chars.iter().enumerate() {
        if matches!(c, '_' | '-' | '.' | '/') {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            continue;
        }
        let boundary = i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase()
                || chars[i - 1].is_ascii_digit()
                || (chars[i - 1].is_uppercase()
                    && chars.get(i + 1).is_some_and(|n| n.is_lowercase())));
        if boundary && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
        current.push(c);
    }
    if !current.is_empty() {
        parts.push(current);
    }
    (parts.len() >= 2).then(|| {
        parts
            .iter()
            .map(|p| p.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// The names near the strokes that the reviewer may have said: what the words are read against, and
/// what a recogniser can be told to listen for. The words under the strokes come first, then the names
/// on the lines they cover and on the lines nearby (the nearest first), then the parts of the file's
/// path; each once, at most `max`.
#[must_use]
pub fn vocabulary(ctx: &Context, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut add = |name: &str| {
        if out.len() < max && !out.iter().any(|n| n == name) {
            out.push(name.to_string());
        }
    };
    for w in &ctx.words_under {
        for name in identifiers(&w.text) {
            add(name);
        }
    }
    let middle = ctx
        .focus
        .map_or(ctx.first_line + ctx.lines.len() / 2, |(a, b)| (a + b) / 2);
    let mut order: Vec<usize> = (0..ctx.lines.len()).collect();
    order.sort_by_key(|i| (ctx.first_line + i).abs_diff(middle));
    for i in order {
        for name in identifiers(&ctx.lines[i]) {
            add(name);
        }
    }
    for part in ctx.file.split('/') {
        for name in identifiers(part.split('.').next().unwrap_or(part)) {
            add(name);
        }
    }
    out
}

/// Whether the recogniser was told to listen for `word`: it says so, or the word is a name of the
/// request's `vocabulary` (compared without regard to case, punctuation round it set aside).
#[must_use]
pub fn is_biased(word: &SaidWord, vocabulary: &[String]) -> bool {
    let bare = word
        .w
        .trim_matches(|c: char| !c.is_alphanumeric() && c != '_');
    word.biased || (!bare.is_empty() && vocabulary.iter().any(|v| v.eq_ignore_ascii_case(bare)))
}

/// The most characters of a language tag and of a file path that are sent.
const MAX_LANGUAGE: usize = 35;
const MAX_FILE: usize = 400;

/// The part of `lines` (of `len`) of at most `size` lines around line index `mid`.
fn window(len: usize, mid: usize, size: usize) -> std::ops::Range<usize> {
    if len <= size {
        return 0..len;
    }
    let start = mid.saturating_sub(size / 2).min(len - size);
    start..start + size
}

/// What the prompt is made of, after the caps.
struct Parts<'a> {
    review: &'a ReviewRequest,
    language: String,
    file: String,
    total_lines: usize,
    /// (line number, text) of the window.
    code: Vec<(usize, String)>,
    words: &'a [WordRef],
    svg: String,
    /// What each stroke looks like, as a line each.
    features: Vec<String>,
    stroke_ids: Vec<(u64, u64, u64)>,
    strokes: &'a [StrokeInput],
    utterances: &'a [Utterance],
    /// The names the words are read against.
    vocabulary: Vec<String>,
    /// The most words of one utterance that are listed with their probabilities.
    said_words_cap: usize,
    turns: &'a [Turn],
    workspace: bool,
}

fn seconds(ms: u64) -> String {
    format!("{:.1}", ms as f64 / 1000.0)
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

/// The words of an utterance with the probability the recogniser gave each (`rename 0.91, enum 0.32
/// (biased)`), when it gave any; at most `cap` words.
fn word_probabilities(u: &Utterance, vocabulary: &[String], cap: usize) -> Option<String> {
    if u.words.iter().all(|w| w.p.is_none()) {
        return None;
    }
    let mut items: Vec<String> = u
        .words
        .iter()
        .take(cap)
        .map(|w| {
            let word = one_line(&w.w);
            let biased = if is_biased(w, vocabulary) {
                " (biased)"
            } else {
                ""
            };
            match w.p {
                Some(p) if p.is_finite() => format!("{word} {:.2}{biased}", p.clamp(0.0, 1.0)),
                _ => format!("{word}{biased}"),
            }
        })
        .collect();
    if u.words.len() > cap {
        items.push(format!("... ({} more words)", u.words.len() - cap));
    }
    Some(items.join(", "))
}

/// The user's part of the prompt.
fn assemble(parts: &Parts<'_>) -> String {
    let review = parts.review;
    let mut out = format!("Task: {}\n", review.task.key());
    if !parts.language.is_empty() {
        out.push_str(&format!("Language of the reviewer: {}\n", parts.language));
    }
    out.push_str(&format!(
        "File: {} (page {}, {} lines)\n",
        parts.file,
        review.context.page + 1,
        parts.total_lines
    ));

    out.push_str(if review.context.focus.is_some() {
        "\nCode (line numbers as in the file; > marks the lines the strokes cover):\n"
    } else {
        "\nCode (line numbers as in the file):\n"
    });
    let width = parts.code.last().map_or(1, |(n, _)| n.to_string().len());
    for (number, text) in &parts.code {
        let mark = match review.context.focus {
            Some((a, b)) if (a..=b).contains(number) => '>',
            _ => ' ',
        };
        out.push_str(&format!("{mark} {number:>width$}| {text}\n"));
    }
    if let Some(grid) = review.context.grid {
        out.push_str(&format!(
            "Layout of the page: line {} starts at y={} and every line is {} px high; column 0 starts at x={} and every column is {} px wide.\n",
            grid.first_line,
            fmt_num(grid.origin_y),
            fmt_num(grid.line_height),
            fmt_num(grid.origin_x),
            fmt_num(grid.char_width)
        ));
    }
    if !parts.words.is_empty() {
        out.push_str("\nWords under the strokes:\n");
        for w in parts.words {
            out.push_str(&format!(
                "- stroke {}: \"{}\" at line {}, columns {} to {}\n",
                w.stroke,
                one_line(&w.text),
                w.line,
                w.col,
                w.col + w.text.len()
            ));
        }
    }
    if !parts.svg.is_empty() {
        out.push_str("\nStrokes (SVG; page coordinates in px; data-t is ms since the stroke began, data-w the width the pen made in px at that point):\n");
        out.push_str(&parts.svg);
        out.push('\n');
    }
    if !parts.features.is_empty() {
        out.push_str(
            "\nShape features of the strokes (computed on this machine from the strokes: hints, not facts):\n",
        );
        for line in &parts.features {
            out.push_str(line);
            out.push('\n');
        }
    }
    if !parts.utterances.is_empty() {
        out.push_str("\nWhat the reviewer said (seconds on the session's clock):\n");
        for u in parts.utterances {
            let during: Vec<String> = parts
                .stroke_ids
                .iter()
                .filter(|(_, from, to)| u.start_ms <= *to && u.end_ms >= *from)
                .map(|(id, _, _)| id.to_string())
                .collect();
            let tie = match during.len() {
                0 => String::new(),
                1 => format!(" [during stroke {}]", during[0]),
                _ => format!(" [during strokes {}]", during.join(", ")),
            };
            out.push_str(&format!(
                "- {} to {} s: \"{}\"{tie}\n",
                seconds(u.start_ms),
                seconds(u.end_ms),
                one_line(&u.text)
            ));
            if let Some(line) = word_probabilities(u, &parts.vocabulary, parts.said_words_cap) {
                out.push_str("  word probabilities: ");
                out.push_str(&line);
                out.push('\n');
            }
        }
    }
    if !parts.vocabulary.is_empty() {
        out.push_str("\nVocabulary near the strokes (names the reviewer may have said; how a name is spoken is in brackets):\n");
        let names: Vec<String> = parts
            .vocabulary
            .iter()
            .map(|n| match spoken(n) {
                Some(said) => format!("{n} [{said}]"),
                None => n.clone(),
            })
            .collect();
        out.push_str(&names.join(", "));
        out.push('\n');
    }
    if !parts.strokes.is_empty() {
        out.push_str("\nMeaning of the strokes:\n");
        for s in parts.strokes {
            match (s.meaning, s.pinned) {
                (Some(m), true) => out.push_str(&format!(
                    "- stroke {}: {} (chosen by the reviewer, final: {})\n",
                    s.id,
                    m.key(),
                    m.gloss()
                )),
                (Some(m), false) => out.push_str(&format!(
                    "- stroke {}: {} (the pen's own, not chosen on purpose: {})\n",
                    s.id,
                    m.key(),
                    m.gloss()
                )),
                (None, _) => out.push_str(&format!(
                    "- stroke {}: no meaning chosen: infer it from the shape and the words\n",
                    s.id
                )),
            }
        }
    }
    if !parts.turns.is_empty() {
        out.push_str("\nConversation so far:\n");
        for t in parts.turns {
            let who = match t.from {
                Speaker::Reviewer => "Reviewer",
                Speaker::Agent => "Agent",
            };
            out.push_str(&format!("{who}: {}\n", t.text.trim()));
        }
    }
    if parts.workspace {
        out.push_str(
            "\nYou are in the folder of the workspace: paths are relative to the workspace.\n",
        );
    }
    out.push('\n');
    out.push_str(match review.task {
        Task::Exploration => "Answer the reviewer.",
        _ => "Reply with the JSON object.",
    });
    out.push('\n');
    out
}

/// The prompt for `review` (the call keeps `request` for the mock).
///
/// # Errors
/// [`AgentError::Settings`] for a task this builder does not build; [`AgentError::TooLarge`] when
/// even the trimmed prompt does not fit `limits.max_prompt_bytes`.
pub fn build_call<'a>(
    request: &'a Request,
    review: &ReviewRequest,
    limits: &Limits,
) -> Result<Call<'a>, AgentError> {
    let task = review.task;
    if !matches!(task, Task::Exploration | Task::Editing) {
        return Err(AgentError::Settings {
            why: format!("the {} task is not a review request", task.key()),
        });
    }
    let ctx = &review.context;
    let mut notes = Notes::new();

    // the newest strokes, as shapes
    let strokes: &[StrokeInput] = if review.strokes.len() > limits.max_strokes {
        notes.push(format!(
            "{} strokes were given, the newest {} are sent",
            review.strokes.len(),
            limits.max_strokes
        ));
        &review.strokes[review.strokes.len() - limits.max_strokes..]
    } else {
        &review.strokes
    };
    let shapes: Vec<Shape> = strokes
        .iter()
        .map(|s| shape(s, limits, &mut notes))
        .collect();
    let stroke_ids: Vec<(u64, u64, u64)> = strokes
        .iter()
        .zip(&shapes)
        .map(|(s, sh)| {
            (
                s.id,
                s.t0_ms,
                s.t0_ms.saturating_add(u64::from(sh.duration_ms)),
            )
        })
        .collect();
    let pairs: Vec<(&StrokeInput, Shape)> = strokes.iter().zip(shapes).collect();
    let svg = svg_document(&pairs);

    // the code around the strokes, long lines cut
    let mid = ctx
        .focus
        .map_or(0, |(a, b)| ((a + b) / 2).saturating_sub(ctx.first_line))
        .min(ctx.lines.len().saturating_sub(1));
    let mut size = ctx.lines.len().min(limits.max_context_lines.max(1));
    if size < ctx.lines.len() {
        notes.push(format!(
            "only {size} of {} lines of code are sent, around the strokes",
            ctx.lines.len()
        ));
    }
    let code_for = |size: usize| -> (Vec<(usize, String)>, usize) {
        let range = window(ctx.lines.len(), mid, size);
        let mut cut = 0;
        let code: Vec<(usize, String)> = ctx.lines[range.clone()]
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let (text, was_cut) =
                    cap_chars(line.trim_end_matches(['\r', '\n']), limits.max_line_chars);
                cut += usize::from(was_cut);
                (ctx.first_line + range.start + i, text)
            })
            .collect();
        (code, cut)
    };
    let (mut code, cut_lines) = code_for(size);
    if cut_lines > 0 {
        notes.push(format!(
            "{cut_lines} lines were cut at {} characters",
            limits.max_line_chars
        ));
    }

    // what the strokes look like and the names near them, on the lines that are sent (not on a whole
    // file of them)
    let view = Context {
        file: ctx.file.clone(),
        page: ctx.page,
        first_line: code.first().map_or(ctx.first_line, |(n, _)| *n),
        lines: code.iter().map(|(_, text)| text.clone()).collect(),
        total_lines: ctx.total_lines,
        focus: ctx.focus,
        words_under: ctx
            .words_under
            .iter()
            .take(limits.max_words_under)
            .cloned()
            .collect(),
        grid: ctx.grid,
    };
    let features: Vec<String> = pairs
        .iter()
        .filter(|(_, sh)| !sh.points.is_empty())
        .map(|(s, sh)| describe(&analyze(s, sh, &view)))
        .collect();
    let vocabulary = vocabulary(&view, limits.max_vocabulary);

    // the words under the strokes
    let words: &[WordRef] = if ctx.words_under.len() > limits.max_words_under {
        notes.push(format!(
            "{} words under the strokes were given, {} are sent",
            ctx.words_under.len(),
            limits.max_words_under
        ));
        &ctx.words_under[..limits.max_words_under]
    } else {
        &ctx.words_under
    };

    // what was said: the newest, within the characters
    let mut utterances: &[Utterance] = &review.utterances;
    if utterances.len() > limits.max_utterances {
        notes.push(format!(
            "the newest {} of {} utterances are sent",
            limits.max_utterances,
            utterances.len()
        ));
        utterances = &utterances[utterances.len() - limits.max_utterances..];
    }
    let mut chars = 0;
    let mut keep_from = utterances.len();
    for (i, u) in utterances.iter().enumerate().rev() {
        chars += u.text.chars().count();
        if chars > limits.max_transcript_chars && keep_from < utterances.len() {
            break;
        }
        keep_from = i;
    }
    if keep_from > 0 {
        notes.push(format!("the oldest {keep_from} utterances were left out: the words are capped at {} characters", limits.max_transcript_chars));
        utterances = &utterances[keep_from..];
    }

    // the conversation: the newest turns, each cut
    let mut turns_kept: Vec<Turn> = review.turns.to_vec();
    if turns_kept.len() > limits.max_turns {
        notes.push(format!(
            "the oldest {} of {} turns were left out",
            turns_kept.len() - limits.max_turns,
            turns_kept.len()
        ));
        turns_kept.drain(..turns_kept.len() - limits.max_turns);
    }
    for t in &mut turns_kept {
        let (text, was_cut) = cap_chars(&t.text, limits.max_turn_chars);
        if was_cut {
            notes.push(format!(
                "a turn was cut at {} characters",
                limits.max_turn_chars
            ));
            t.text = text;
        }
    }

    let (language, _) = cap_chars(review.language.trim(), MAX_LANGUAGE);
    let (file, _) = cap_chars(&ctx.file, MAX_FILE);
    let tools = task.spec().tools;
    let workdir = if tools == Tools::ReadOnly {
        review.workspace.clone()
    } else {
        None
    };
    let mut system = match task {
        Task::Exploration => EXPLORATION_SYSTEM.to_string(),
        _ => EDITING_SYSTEM.to_string(),
    };
    if strokes.iter().any(|s| !s.pinned || s.meaning.is_none()) && task == Task::Editing {
        system.push('\n');
        system.push_str(INTENT_CLAUSE);
    }
    let total_lines = if ctx.total_lines > 0 {
        ctx.total_lines
    } else {
        ctx.first_line + ctx.lines.len().saturating_sub(1)
    };

    // trimmed until it fits: the oldest turns first, then the code window, then the words
    let mut trimmed = Vec::new();
    let mut turn_from = 0;
    let mut utter_from = 0;
    loop {
        let parts = Parts {
            review,
            language: language.clone(),
            file: file.clone(),
            total_lines,
            code: code.clone(),
            words,
            svg: svg.clone(),
            features: features.clone(),
            stroke_ids: stroke_ids.clone(),
            strokes,
            utterances: &utterances[utter_from..],
            vocabulary: vocabulary.clone(),
            said_words_cap: limits.max_said_words,
            turns: &turns_kept[turn_from..],
            workspace: workdir.is_some(),
        };
        let prompt = assemble(&parts);
        let bytes = system.len() + prompt.len();
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
                system,
                prompt,
                workdir,
                tools,
                diagnostics: notes.finish(),
            });
        }
        if turn_from < turns_kept.len() {
            turn_from += 1;
            trimmed.retain(|t: &String| !t.ends_with("older turns left out"));
            trimmed.push(format!("{turn_from} older turns left out"));
        } else if size > 5 {
            size = (size / 2).max(5);
            code = code_for(size).0;
            trimmed.retain(|t: &String| !t.starts_with("the code was cut to"));
            trimmed.push(format!(
                "the code was cut to {size} lines around the strokes"
            ));
        } else if utter_from < utterances.len() {
            utter_from += 1;
            trimmed.retain(|t: &String| !t.ends_with("older utterances left out"));
            trimmed.push(format!("{utter_from} older utterances left out"));
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

/// The first string under one of `keys`.
fn first_str<'a>(value: &'a Json, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| agent_json::str_of(value, k))
}

/// The first array under one of `keys`.
fn first_array<'a>(value: &'a Json, keys: &[&str]) -> Option<&'a [Json]> {
    keys.iter().find_map(|k| agent_json::array_of(value, k))
}

/// The first whole number under one of `keys`.
fn first_u64(value: &Json, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|k| agent_json::u64_of(value, k))
}

fn as_usize(n: u64) -> usize {
    usize::try_from(n).unwrap_or(usize::MAX)
}

/// A position: `{"line": 39, "col": 12}`, `[39, 12]`.
fn pos_of(value: &Json) -> Option<Pos> {
    match value {
        Json::Object(_) => Some(Pos {
            line: as_usize(first_u64(value, &["line", "row"])?),
            col: as_usize(first_u64(value, &["col", "column", "character", "offset"])?),
        }),
        Json::Array(items) if items.len() == 2 => Some(Pos {
            line: as_usize(items[0].as_u64()?),
            col: as_usize(items[1].as_u64()?),
        }),
        _ => None,
    }
}

/// The range of a replace or remove: `start` and `end`, or the flat `line`, `start_col`, `end_col`.
fn range_of(item: &Json) -> Option<(Pos, Pos)> {
    if let (Some(start), Some(end)) = (
        agent_json::get(item, "start").and_then(pos_of),
        agent_json::get(item, "end").and_then(pos_of),
    ) {
        return Some((start, end));
    }
    let line = as_usize(first_u64(item, &["line", "row"])?);
    let from = as_usize(first_u64(item, &["start_col", "col_start", "col", "from"])?);
    let to = as_usize(first_u64(item, &["end_col", "col_end", "to"])?);
    Some((Pos { line, col: from }, Pos { line, col: to }))
}

/// The lines of an insertion: `lines` (each may hold line breaks) or one `text`.
fn lines_of(item: &Json) -> Vec<String> {
    let mut lines = Vec::new();
    let mut push = |text: &str| {
        for line in text.split('\n') {
            lines.push(line.trim_end_matches('\r').to_string());
        }
    };
    if let Some(items) = first_array(item, &["lines"]) {
        for entry in items {
            if let Some(text) = entry.as_str() {
                push(text);
            }
        }
    } else if let Some(text) = first_str(item, &["text", "content", "code"]) {
        push(text);
    }
    lines
}

/// The text of the file between `start` and `end` (exclusive) within the lines that were sent;
/// `None` when a line or a column is not among them.
fn text_between(ctx: &Context, start: Pos, end: Pos) -> Option<String> {
    let line_at = |n: usize| n.checked_sub(ctx.first_line).and_then(|i| ctx.lines.get(i));
    if start.line == end.line {
        return line_at(start.line)?
            .get(start.col..end.col)
            .map(str::to_string);
    }
    let mut out = String::from(line_at(start.line)?.get(start.col..)?);
    for n in start.line + 1..end.line {
        out.push('\n');
        out.push_str(line_at(n)?);
    }
    out.push('\n');
    out.push_str(line_at(end.line)?.get(..end.col)?);
    Some(out)
}

/// Where `old` is in the lines that were sent.
enum Where {
    One(Pos, Pos),
    Many,
    Nowhere,
}

/// Looks for `old` near the claimed line, then in all the lines that were sent.
fn find_old(ctx: &Context, claimed: usize, old: &str) -> Where {
    let end_of = |line: usize, col: usize| -> (Pos, Pos) {
        let lines = old.split('\n').count();
        if lines == 1 {
            (
                Pos { line, col },
                Pos {
                    line,
                    col: col + old.len(),
                },
            )
        } else {
            let last = old.rsplit('\n').next().map_or(0, str::len);
            (
                Pos { line, col },
                Pos {
                    line: line + lines - 1,
                    col: last,
                },
            )
        }
    };
    if old.contains('\n') {
        // a range over several lines: the joined lines are searched
        let joined = ctx.lines.join("\n");
        let mut starts = Vec::new();
        let mut offset = 0;
        for line in &ctx.lines {
            starts.push(offset);
            offset += line.len() + 1;
        }
        let hits: Vec<usize> = joined.match_indices(old).map(|(i, _)| i).collect();
        return match hits.as_slice() {
            [] => Where::Nowhere,
            [at] => {
                let index = starts.partition_point(|s| s <= at).saturating_sub(1);
                let (a, b) = end_of(ctx.first_line + index, at - starts[index]);
                Where::One(a, b)
            }
            _ => Where::Many,
        };
    }
    let hits_in = |line: &str| -> Vec<usize> { line.match_indices(old).map(|(i, _)| i).collect() };
    if let Some(line) = claimed
        .checked_sub(ctx.first_line)
        .and_then(|i| ctx.lines.get(i))
    {
        match hits_in(line).as_slice() {
            [col] => {
                let (a, b) = end_of(claimed, *col);
                return Where::One(a, b);
            }
            [] => {}
            _ => return Where::Many,
        }
    }
    let mut all: Vec<(usize, usize)> = Vec::new();
    for (i, line) in ctx.lines.iter().enumerate() {
        for col in hits_in(line) {
            all.push((ctx.first_line + i, col));
        }
    }
    match all.as_slice() {
        [] => Where::Nowhere,
        [(line, col)] => {
            let (a, b) = end_of(*line, *col);
            Where::One(a, b)
        }
        many => {
            let distance = |line: usize| line.abs_diff(claimed);
            let best = many
                .iter()
                .map(|(l, _)| distance(*l))
                .min()
                .unwrap_or(usize::MAX);
            let nearest: Vec<&(usize, usize)> =
                many.iter().filter(|(l, _)| distance(*l) == best).collect();
            match nearest.as_slice() {
                [(line, col)] if best <= 2 => {
                    let (a, b) = end_of(*line, *col);
                    Where::One(a, b)
                }
                _ => Where::Many,
            }
        }
    }
}

/// Checks the range of a replace or a remove against the lines that were sent; the checked range
/// and `old` (filled in when it was missing), or `None` when the suggestion is dropped.
fn verify_range(
    ctx: &Context,
    index: usize,
    start: Pos,
    end: Pos,
    old: &str,
    notes: &mut Notes,
) -> Option<(Pos, Pos, String)> {
    let total = if ctx.total_lines > 0 {
        ctx.total_lines
    } else {
        ctx.first_line + ctx.lines.len().saturating_sub(1)
    };
    if start.line == 0 || end.line == 0 {
        notes.push(format!(
            "suggestion {index}: dropped: lines are numbered from 1"
        ));
        return None;
    }
    if (start.line, start.col) > (end.line, end.col) {
        notes.push(format!(
            "suggestion {index}: dropped: its range runs backwards"
        ));
        return None;
    }
    if end.line > total {
        notes.push(format!(
            "suggestion {index}: dropped: line {} is past the end of the file ({total} lines)",
            end.line
        ));
        return None;
    }
    let last_sent = ctx.first_line + ctx.lines.len().saturating_sub(1);
    let inside = !ctx.lines.is_empty() && start.line >= ctx.first_line && end.line <= last_sent;
    if !inside {
        notes.push(format!(
            "suggestion {index}: unverified: lines {} to {} were not among the lines sent",
            start.line, end.line
        ));
        return Some((start, end, old.to_string()));
    }
    match text_between(ctx, start, end) {
        Some(actual) if old.is_empty() || actual == old => Some((start, end, actual)),
        _ => {
            if old.is_empty() {
                notes.push(format!(
                    "suggestion {index}: dropped: its columns are not inside line {}",
                    start.line
                ));
                return None;
            }
            match find_old(ctx, start.line, old) {
                Where::One(a, b) => {
                    notes.push(format!(
                        "suggestion {index}: moved its range to line {}, column {}: that is where {:?} is",
                        a.line, a.col, old
                    ));
                    Some((a, b, old.to_string()))
                }
                Where::Many => {
                    notes.push(format!(
                        "suggestion {index}: dropped: {:?} is not at line {}, column {}, and it appears more than once near it",
                        old, start.line, start.col
                    ));
                    None
                }
                Where::Nowhere => {
                    notes.push(format!(
                        "suggestion {index}: dropped: {:?} is not at line {}, column {} and nowhere else in the lines sent",
                        old, start.line, start.col
                    ));
                    None
                }
            }
        }
    }
}

/// One entry of the `suggestions` array, checked and capped; `None` (with a note) when it is no
/// usable suggestion.
fn suggestion_of(
    index: usize,
    item: &Json,
    ctx: &Context,
    limits: &Limits,
    notes: &mut Notes,
) -> Option<Suggestion> {
    let word = first_str(item, &["kind", "type", "op", "action"])
        .unwrap_or("")
        .to_ascii_lowercase();
    let is = |parts: &[&str]| parts.iter().any(|p| word.contains(p));
    let old = first_str(item, &["old", "original", "from_text", "current"]).unwrap_or("");
    if is(&["insert", "add", "append"]) {
        let Some(after) = first_u64(item, &["after_line", "after"]) else {
            notes.push(format!(
                "suggestion {index}: dropped: an insertion needs after_line"
            ));
            return None;
        };
        let after_line = as_usize(after);
        let mut lines = lines_of(item);
        if lines.is_empty() {
            notes.push(format!(
                "suggestion {index}: dropped: an insertion without lines"
            ));
            return None;
        }
        if lines.len() > limits.max_insert_lines {
            notes.push(format!(
                "suggestion {index}: its {} lines were cut to {}",
                lines.len(),
                limits.max_insert_lines
            ));
            lines.truncate(limits.max_insert_lines);
        }
        let mut bytes = 0;
        let keep = lines
            .iter()
            .take_while(|l| {
                bytes += l.len() + 1;
                bytes <= limits.max_suggestion_bytes
            })
            .count();
        if keep < lines.len() {
            notes.push(format!(
                "suggestion {index}: its text was cut at {} bytes",
                limits.max_suggestion_bytes
            ));
            lines.truncate(keep.max(1));
        }
        let total = if ctx.total_lines > 0 {
            ctx.total_lines
        } else {
            usize::MAX
        };
        if after_line > total {
            notes.push(format!("suggestion {index}: dropped: after line {after_line} is past the end of the file ({total} lines)"));
            return None;
        }
        return Some(Suggestion::InsertLines { after_line, lines });
    }
    let replace = is(&["replace", "rewrite", "change", "edit", "modify"]);
    let remove = !replace && is(&["remove", "delete"]);
    if !replace && !remove {
        notes.push(format!(
            "suggestion {index}: dropped: unknown kind {word:?}"
        ));
        return None;
    }
    let Some((start, end)) = range_of(item) else {
        notes.push(format!("suggestion {index}: dropped: no range"));
        return None;
    };
    let new_text = first_str(item, &["text", "new", "replacement", "with", "new_text"]);
    if replace && new_text.is_none() {
        notes.push(format!(
            "suggestion {index}: dropped: a replacement without text"
        ));
        return None;
    }
    if remove && start == end {
        notes.push(format!(
            "suggestion {index}: dropped: an empty range removes nothing"
        ));
        return None;
    }
    let (start, end, old) = verify_range(ctx, index, start, end, old, notes)?;
    if old.len() > limits.max_suggestion_bytes {
        notes.push(format!(
            "suggestion {index}: dropped: its range is longer than {} bytes",
            limits.max_suggestion_bytes
        ));
        return None;
    }
    if replace {
        let text = new_text.unwrap_or("");
        let (text, cut) = cap_bytes(text, limits.max_suggestion_bytes);
        if cut {
            notes.push(format!(
                "suggestion {index}: its text was cut at {} bytes",
                limits.max_suggestion_bytes
            ));
        }
        Some(Suggestion::ReplaceRange {
            start,
            end,
            old,
            text: text.to_string(),
        })
    } else {
        Some(Suggestion::RemoveRange { start, end, old })
    }
}

/// The intent of a reply: its meaning, its confidence held to 0 to 1, its reason cut.
fn intent_of(value: &Json, limits: &Limits, notes: &mut Notes) -> Option<Intent> {
    if !value.is_object() {
        notes.push("the intent was not an object: it was dropped");
        return None;
    }
    let meaning = agent_json::str_of(value, "meaning").and_then(Meaning::from_key);
    let confidence =
        agent_json::f64_of(value, "confidence").map_or(0.5, |c| c.clamp(0.0, 1.0)) as f32;
    let (rationale, _) = cap_chars(
        agent_json::str_of(value, "rationale").unwrap_or("").trim(),
        limits.max_rationale_chars,
    );
    Some(Intent {
        meaning,
        confidence,
        rationale,
    })
}

/// What a model calls "do nothing" in the `action` of a reply.
static NO_ACTION_WORDS: &[&str] = &[
    "none",
    "no_action",
    "no-action",
    "noaction",
    "nothing",
    "do_nothing",
    "do-nothing",
    "ignore",
    "skip",
    "noop",
    "no_op",
];

/// The longest `heard_as` that is kept, in characters.
const MAX_HEARD_AS: usize = 400;

/// Whether the object says to do nothing (`{"action": "none", "reason": "..."}`).
fn action_is_none(value: &Json) -> bool {
    agent_json::str_of(value, "action")
        .is_some_and(|a| NO_ACTION_WORDS.contains(&a.trim().to_ascii_lowercase().as_str()))
}

/// A first line `Heard as: ...` of a plain-text answer: what the agent understood, and the answer
/// without that line.
fn take_heard_as(answer: &str) -> (String, Option<String>) {
    const LABEL: &str = "heard as:";
    let text = answer.trim_start();
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let first = first.trim_start();
    if first.len() >= LABEL.len()
        && first.is_char_boundary(LABEL.len())
        && first[..LABEL.len()].eq_ignore_ascii_case(LABEL)
    {
        let heard = cap_chars(first[LABEL.len()..].trim(), MAX_HEARD_AS).0;
        return (
            rest.trim().to_string(),
            Some(heard).filter(|h| !h.is_empty()),
        );
    }
    (answer.to_string(), None)
}

/// A JSON object that can be a reply: the exploration wants the words of the format, the editing
/// takes what models call them as well.
fn is_reply_object(value: &Json, task: Task) -> bool {
    if action_is_none(value) {
        return true;
    }
    let keys: &[&str] = if task == Task::Exploration {
        &["answer", "suggestions", "intent", "heard_as"]
    } else {
        &[
            "answer",
            "reply",
            "response",
            "message",
            "text",
            "suggestions",
            "edits",
            "changes",
            "intent",
            "heard_as",
        ]
    };
    keys.iter().any(|k| agent_json::get(value, k).is_some())
}

fn reply_from_json(
    value: &Json,
    review: &ReviewRequest,
    limits: &Limits,
    notes: &mut Notes,
) -> Reply {
    if action_is_none(value) {
        let reason = first_str(value, &["reason", "why", "explanation", "message", "note"])
            .unwrap_or("")
            .trim();
        let reason = cap_chars(&one_line(reason), limits.max_rationale_chars).0;
        let said_something = !first_str(value, &["answer", "reply", "response", "text"])
            .unwrap_or("")
            .trim()
            .is_empty()
            || first_array(value, &["suggestions", "edits", "changes"])
                .is_some_and(|a| !a.is_empty());
        if said_something {
            notes.push("the agent said to do nothing and gave an answer or suggestions as well: they were dropped");
        }
        return Reply::nothing(if reason.is_empty() {
            "the agent chose to do nothing"
        } else {
            &reason
        });
    }
    let (answer, cut) = cap_chars(
        first_str(value, &["answer", "reply", "response", "message", "text"])
            .unwrap_or("")
            .trim(),
        limits.max_answer_chars,
    );
    if cut {
        notes.push(format!(
            "the answer was cut at {} characters",
            limits.max_answer_chars
        ));
    }
    let mut suggestions: Vec<Suggestion> = Vec::new();
    if let Some(items) = first_array(value, &["suggestions", "edits", "changes"]) {
        if items.len() > limits.max_suggestions {
            notes.push(format!(
                "{} suggestions were given, the first {} are kept",
                items.len(),
                limits.max_suggestions
            ));
        }
        for (i, item) in items.iter().enumerate().take(limits.max_suggestions) {
            if let Some(s) = suggestion_of(i + 1, item, &review.context, limits, notes) {
                if !suggestions.contains(&s) {
                    suggestions.push(s);
                }
            }
        }
    }
    let intent = agent_json::get(value, "intent").and_then(|i| intent_of(i, limits, notes));
    let heard_as = first_str(value, &["heard_as", "heard", "understood_as"])
        .map(|t| cap_chars(&one_line(t.trim()), MAX_HEARD_AS).0)
        .filter(|t| !t.is_empty());
    Reply {
        answer,
        suggestions,
        intent,
        heard_as,
        no_action: None,
    }
}

fn bad(why: &str, text: &str) -> AgentError {
    AgentError::BadOutput {
        why: why.to_string(),
        excerpt: cap_chars(text.trim(), 200).0,
    }
}

/// The text before a reply object, with the line that opens its code fence taken off.
fn prose_before(text: &str) -> String {
    let before = text.trim_end();
    let before = before
        .strip_suffix("```json")
        .or_else(|| before.strip_suffix("```JSON"))
        .or_else(|| before.strip_suffix("```"))
        .unwrap_or(before);
    before.trim().to_string()
}

/// Reads the text of a reply as an answer to `review`.
///
/// # Errors
/// [`AgentError::BadOutput`] when there is no answer in it.
pub fn parse_body(
    review: &ReviewRequest,
    text: &str,
    limits: &Limits,
    notes: &mut Notes,
) -> Result<Reply, AgentError> {
    let (text, cut) = cap_bytes(text, limits.max_reply_bytes);
    if cut {
        notes.push(format!(
            "the reply was cut at {} bytes",
            limits.max_reply_bytes
        ));
    }
    let text = text.trim();
    if text.is_empty() {
        return Err(bad("the reply is empty", ""));
    }
    let task = review.task;
    // an object with nothing in it, alone (a `{}` in a text of prose is code, not a reply)
    let bare = text
        .trim_start_matches("```json")
        .trim_start_matches("```JSON")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    if bare == "{}" {
        notes.push("the reply was an empty object: it is read as doing nothing");
        return Ok(Reply::nothing("the agent answered with nothing"));
    }
    let found =
        agent_json::find_object_where(text, limits.max_reply_bytes, &|v| is_reply_object(v, task))
            .map_err(|e| bad(&format!("{e:?}"), text))?;
    let Some(found) = found else {
        let starts_like_json =
            text.starts_with('{') || text.starts_with("```json") || text.starts_with("```JSON");
        let names_the_format = task == Task::Editing
            && (text.contains("\"suggestions\"") || text.contains("\"answer\""));
        if starts_like_json || names_the_format {
            return Err(bad(
                "no JSON object with an answer or suggestions could be read from it",
                text,
            ));
        }
        let (answer, was_cut) = cap_chars(text, limits.max_answer_chars);
        if was_cut {
            notes.push(format!(
                "the answer was cut at {} characters",
                limits.max_answer_chars
            ));
        }
        if task == Task::Editing {
            notes.push("the agent answered in plain text, without suggestions");
        }
        let (answer, heard_as) = take_heard_as(&answer);
        if answer.trim().is_empty() {
            return Err(bad(
                "the reply has no answer but the line of what was heard",
                text,
            ));
        }
        return Ok(Reply {
            answer,
            suggestions: Vec::new(),
            intent: None,
            heard_as,
            no_action: None,
        });
    };
    if found.repaired {
        notes.push("the reply's JSON had a line break inside a string or a comma before a bracket: it was repaired before it was read");
    }
    let mut reply = reply_from_json(&found.value, review, limits, notes);
    if reply.is_no_action() {
        if !text[..found.range.start].trim().is_empty() && task == Task::Exploration {
            notes.push(
                "the agent said to do nothing and wrote a text before it: the text was dropped",
            );
        }
        return Ok(reply);
    }
    if reply.answer.is_empty() && task == Task::Exploration {
        reply.answer = cap_chars(
            &prose_before(&text[..found.range.start]),
            limits.max_answer_chars,
        )
        .0;
    }
    if reply.heard_as.is_none() {
        let (answer, heard_as) = take_heard_as(&reply.answer);
        if heard_as.is_some() {
            reply.answer = answer;
            reply.heard_as = heard_as;
        }
    }
    if reply.answer.trim().is_empty() && reply.suggestions.is_empty() {
        let tried = first_array(&found.value, &["suggestions", "edits", "changes"])
            .is_some_and(|items| !items.is_empty());
        if tried {
            return Err(bad(
                "the reply has neither an answer nor a usable suggestion",
                text,
            ));
        }
        // a reply that is well formed and says nothing at all is an agent that does nothing
        notes.push("the reply was empty: it is read as doing nothing");
        return Ok(Reply::nothing("the agent answered with nothing"));
    }
    Ok(reply)
}

/// A reply as the JSON the agents are asked to write (what the mock sends, what a test parses).
#[must_use]
pub fn reply_json(reply: &Reply) -> Json {
    if let Some(reason) = &reply.no_action {
        return json!({"action": "none", "reason": reason});
    }
    let pos = |p: &Pos| json!({"line": p.line, "col": p.col});
    let suggestions: Vec<Json> = reply
        .suggestions
        .iter()
        .map(|s| match s {
            Suggestion::InsertLines { after_line, lines } => {
                json!({"kind": "insert_lines", "after_line": after_line, "lines": lines})
            }
            Suggestion::ReplaceRange {
                start,
                end,
                old,
                text,
            } => json!({
                "kind": "replace", "start": pos(start), "end": pos(end), "old": old, "text": text
            }),
            Suggestion::RemoveRange { start, end, old } => json!({
                "kind": "remove", "start": pos(start), "end": pos(end), "old": old
            }),
        })
        .collect();
    let mut object = json!({"answer": reply.answer, "suggestions": suggestions});
    if let (Some(heard), Some(map)) = (&reply.heard_as, object.as_object_mut()) {
        map.insert("heard_as".to_string(), json!(heard));
    }
    if let (Some(intent), Some(map)) = (&reply.intent, object.as_object_mut()) {
        map.insert(
            "intent".to_string(),
            json!({
                "meaning": intent.meaning.map_or("other", Meaning::key),
                "confidence": f64::from(intent.confidence),
                "rationale": intent.rationale,
            }),
        );
    }
    object
}

/// `reply` as the text of a model's answer.
#[must_use]
pub fn reply_text(reply: &Reply) -> String {
    agent_json::to_text(&reply_json(reply))
}

/// Whether `tools` ask for a workspace to read from.
#[must_use]
pub const fn reads_workspace(tools: Tools) -> bool {
    matches!(tools, Tools::ReadOnly)
}

#[cfg(test)]
#[path = "agent_review_test.rs"]
mod agent_review_test;
