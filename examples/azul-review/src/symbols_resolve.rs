//! What the reviewer says, tied to the code: CERTAIN or a HINT, never a guess.
//!
//! The recogniser's words (with the probability and the `biased` flag of each) go in; out come
//!
//! - [`Certain`]: a definition the words name, found because the spoken words, read as the words of
//!   a name ([`crate::symbols_spoken`]: spelled-out acronyms, number words, the mishearings of code
//!   words, identifiers the recogniser wrote), EQUAL the words of exactly one definition's name, and
//!   something besides the spelling says a person meant the code: the words were written as an
//!   identifier or an acronym, were spelled out, are a mishearing of a code word, were favoured by
//!   the recogniser, follow or precede a kind word (`struct`, `function`, `crate` ...), or are
//!   several words that are not all everyday English (or three or more words). One everyday word
//!   (`error`, `value`, `data`) is never found by its spelling: it needs a kind word, the bias or the
//!   identifier shape. "Exactly one" is within a scope: the whole workspace, else the package the
//!   reviewer is in, else the file, else what was found earlier in the conversation; a name in
//!   tests only is found only in the file that has it.
//! - A word the recogniser doubted (probability at most [`Options::low_probability`]) next to a
//!   spelled-out acronym may be REPAIRED: `see ess ess ever` with a doubted `ever` is `css` and the
//!   one name `css <word>` whose word sounds like it ("error"): [`Certain::repaired`]. A word the
//!   recogniser was sure of, or gave no probability for, is never repaired.
//! - [`Hint`]: everything that is not certain but may help an agent that has to search: the
//!   candidates when several definitions have the spoken name (at most three, with their files),
//!   roughly what to grep for (`css.?error`, the words joined by any separator, case-insensitive,
//!   from the words as heard and from the names that sound or look like them), the kinds a kind word
//!   named, and the folders. A hint is never shown as a found definition; there are at most
//!   [`Options::max_hints`] per utterance and each is short.
//!
//! The matching of names that merely look or sound like the words (a bounded edit distance on the
//! letters, a phonetic skeleton) only ever makes hints. [`Resolution::corrected`] is the sentence with
//! each certain mention written as its name (and a spelled-out acronym from the list written as
//! the acronym: `aitch tee tee pee` is `HTTP`), next to [`Resolution::raw`], which is what was heard.
//!
//! Bounded: at most [`Options::max_words`] words are read, [`MAX_MENTIONS`] mentions are kept, and a
//! pass over every key of the index is made at most once per utterance for the fuzzy hints.

use std::ops::Range;

use crate::{
    symbols::{parent_dir, DefInfo, Index, Kind},
    symbols_spoken::{
        acronym_run_at, clean_words, is_common_word, is_stop_word, kind_words, plural_letter_readings, readings, HeardWord, Reading, SpokenWord, VIA_DIGITS, VIA_HOMOPHONE,
        VIA_IDENTIFIER, VIA_JOINED, VIA_LETTERS,
    },
    symbols_tokens::{concat_of, edit_distance_bounded, key_of, phonetic},
};

fn symbols_tokens_of(name: &str) -> Vec<String> {
    crate::symbols_tokens::split_identifier(name)
}

/// The most certain mentions of one utterance.
pub const MAX_MENTIONS: usize = 64;
/// The longest `said` of a hint, characters.
const MAX_SAID: usize = 80;
/// The words that join a kind word to the name after it ("a struct called CssError").
static KIND_CONNECTIVES: [&str; 10] = ["a", "an", "called", "named", "that", "the", "this", "genannt", "namens", "of"];

/// How far a certain mention was narrowed: the scope in which it is the only definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// The reviewer is in the file that has it.
    File,
    /// The package the reviewer is in has it (the other packages may too).
    Crate,
    /// It was found earlier in this conversation.
    Recent,
    /// The workspace has no other.
    Workspace,
}

/// What a correction of the transcript is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorrectionKind {
    /// The words are a certain mention: written as the name.
    Symbol,
    /// The words are a spelled-out acronym of the list: written as the acronym.
    Acronym,
}

/// A definition the words name, for certain.
#[derive(Clone, Debug, PartialEq)]
pub struct Certain {
    /// The heard words it covers.
    pub words: Range<usize>,
    pub def: DefInfo,
    pub scope: Scope,
    /// How the words became the name's words: the `VIA_` bits of [`crate::symbols_spoken`] (0: as heard).
    pub via: u8,
    /// A word the recogniser doubted was read as the name's word.
    pub repaired: bool,
    /// How much the words said, 0 to 1.
    pub score: f32,
}

/// Roughly what to search for: not a definition.
#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    /// The heard words it is about.
    pub words: Range<usize>,
    /// What was heard (short).
    pub said: String,
    /// Patterns to grep for, case-insensitive: the words joined by `.?`.
    pub patterns: Vec<String>,
    /// The kinds a kind word named.
    pub kinds: Vec<Kind>,
    /// Definitions that have exactly the spoken name when there are several (at most three).
    pub candidates: Vec<DefInfo>,
    /// Names that look or sound like the words (at most three): never definitions found.
    pub similar: Vec<String>,
    /// Folders the candidates or the similar names are in (at most three).
    pub folders: Vec<String>,
}

/// One change of the transcript.
#[derive(Clone, Debug, PartialEq)]
pub struct Correction {
    pub words: Range<usize>,
    pub from: String,
    pub to: String,
    pub kind: CorrectionKind,
}

/// What an utterance resolves to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Resolution {
    pub certain: Vec<Certain>,
    pub hints: Vec<Hint>,
    pub corrections: Vec<Correction>,
    /// The words as heard, joined by spaces.
    pub raw: String,
    /// The words with the corrections made.
    pub corrected: String,
}

/// Where the reviewer is and what was found before.
#[derive(Clone, Debug, Default)]
pub struct Context<'a> {
    /// The file shown (workspace-relative).
    pub file: Option<&'a str>,
    /// Definitions found earlier in this conversation or file.
    pub recent: &'a [DefInfo],
}

/// The limits of a resolution.
#[derive(Clone, Debug)]
pub struct Options {
    /// The longest run of words matched against a name.
    pub max_window: usize,
    /// The most hints.
    pub max_hints: usize,
    /// The most words read of one utterance.
    pub max_words: usize,
    /// A word with a probability at most this is doubted.
    pub low_probability: f32,
}

impl Default for Options {
    fn default() -> Self {
        Self { max_window: 8, max_hints: 4, max_words: 400, low_probability: 0.35 }
    }
}

/// Resolves utterances against an index.
pub struct Resolver<'a> {
    index: &'a Index,
    options: Options,
}

/// One window of words whose reading is the key of at least one definition.
struct Cand {
    a: usize,
    b: usize,
    reading: Reading,
    defs: Vec<DefInfo>,
    /// The kinds of the kind words next to it.
    kinds: Vec<Kind>,
    biased: bool,
    identifier: bool,
    /// The words as heard, with a word that only joins at an edge and nothing else that says the code
    /// is meant: never certain by itself (`is empty`, `get margin top`), but it may be what a shorter
    /// certain mention is part of.
    edge_literal: bool,
}

impl Cand {
    /// The candidate is made of the words as heard (not a joined word, not a spelled-out reading): it
    /// is a longer name the same words may be part of.
    fn via_words(&self) -> bool {
        self.reading.via & VIA_JOINED == 0
    }
}

enum Narrowed {
    Unique(DefInfo, Scope),
    Ambiguous(Vec<DefInfo>),
}

struct Eval {
    code_like: bool,
    kind_ok: bool,
    hint_worthy: bool,
    narrowed: Narrowed,
    score: f32,
}

/// A window of words to look for similar names of.
struct Query {
    a: usize,
    b: usize,
    key: String,
    cat: String,
    skeleton: Vec<u8>,
    ntokens: usize,
    strength: u8,
    kinds: Vec<Kind>,
    patterns: Vec<String>,
}

const MAX_SKELETON: usize = 12;

fn within_one(a: &[u8], b: &[u8]) -> Option<usize> {
    if a == b {
        return Some(0);
    }
    if a.len().abs_diff(b.len()) > 1 {
        return None;
    }
    let (mut i, mut j, mut edits) = (0, 0, 0);
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            i += 1;
            j += 1;
            continue;
        }
        edits += 1;
        if edits > 1 {
            return None;
        }
        match a.len().cmp(&b.len()) {
            std::cmp::Ordering::Greater => i += 1,
            std::cmp::Ordering::Less => j += 1,
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
        }
    }
    edits += (a.len() - i) + (b.len() - j);
    (edits <= 1).then_some(edits)
}

fn pattern_of(tokens: &[String]) -> String {
    tokens.join(".?")
}

fn push_unique(list: &mut Vec<String>, item: String, max: usize) {
    if list.len() < max && !list.contains(&item) {
        list.push(item);
    }
}

fn shorten(text: &str) -> String {
    if text.chars().count() <= MAX_SAID {
        text.to_string()
    } else {
        text.chars().take(MAX_SAID).collect()
    }
}

impl<'a> Resolver<'a> {
    #[must_use]
    pub fn new(index: &'a Index) -> Self {
        Self { index, options: Options::default() }
    }

    #[must_use]
    pub const fn with_options(index: &'a Index, options: Options) -> Self {
        Self { index, options }
    }

    /// Resolves the words of one utterance.
    #[must_use]
    pub fn resolve(&self, words: &[HeardWord], ctx: &Context<'_>) -> Resolution {
        let raw = words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
        let used = &words[..words.len().min(self.options.max_words)];
        let spoken = clean_words(used);
        let mut work = Work { r: self, ctx, words: used, claimed: vec![false; spoken.len()], spoken };
        let mut resolution = work.run();
        resolution.raw = raw;
        resolution
    }
}

struct Work<'a, 'c> {
    r: &'a Resolver<'a>,
    ctx: &'a Context<'c>,
    words: &'a [HeardWord],
    spoken: Vec<SpokenWord>,
    claimed: Vec<bool>,
}

impl Work<'_, '_> {
    fn index(&self) -> &Index {
        self.r.index
    }

    fn known(&self, token: &str) -> bool {
        self.r.index.has_token(token)
    }

    /// The heard words `a..b` of the spoken words.
    fn orig_range(&self, a: usize, b: usize) -> Range<usize> {
        let start = self.spoken[a].orig;
        let end = self.spoken[b - 1].orig + 1;
        start..end
    }

    fn heard_text(&self, range: &Range<usize>) -> String {
        shorten(&self.words[range.clone()].iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" "))
    }

    fn free(&self, a: usize, b: usize) -> bool {
        self.claimed[a..b].iter().all(|c| !c)
    }

    fn claim(&mut self, a: usize, b: usize) {
        for c in &mut self.claimed[a..b] {
            *c = true;
        }
    }

    /// The kinds of the kind words next to the window `a..b`: the one before it (through a
    /// connective such as "called") and the one right after it.
    fn adjacent_kinds(&self, a: usize, b: usize) -> Vec<Kind> {
        let mut kinds: Vec<Kind> = Vec::new();
        let mut j = a;
        for _ in 0..3 {
            if j == 0 {
                break;
            }
            j -= 1;
            let word = self.spoken[j].clean.as_str();
            let found = kind_words(word);
            if !found.is_empty() {
                kinds.extend_from_slice(found);
                break;
            }
            if !KIND_CONNECTIVES.contains(&word) {
                break;
            }
        }
        if b < self.spoken.len() {
            kinds.extend_from_slice(kind_words(&self.spoken[b].clean));
        }
        kinds.dedup();
        kinds
    }

    fn crate_of(&self, file: &str) -> Option<String> {
        self.index().crate_dir_of(file).map(str::to_string)
    }

    fn scope_of(&self, def: &DefInfo) -> Scope {
        match self.ctx.file {
            Some(f) if f == def.file => Scope::File,
            Some(f) => {
                let here = self.crate_of(f);
                if here.is_some() && here == self.crate_of(&def.file) {
                    Scope::Crate
                } else {
                    Scope::Workspace
                }
            }
            None => Scope::Workspace,
        }
    }

    /// The definitions that remain when the scope is narrowed: one, or the lot.
    fn narrow(&self, defs: Vec<DefInfo>) -> Narrowed {
        let mut pool: Vec<DefInfo> = Vec::with_capacity(defs.len());
        for d in defs {
            let same = |e: &DefInfo| e.file == d.file && e.line == d.line && e.name == d.name && e.kind == d.kind;
            if !pool.iter().any(same) {
                pool.push(d);
            }
        }
        let cur = self.ctx.file;
        if pool.iter().any(|d| !d.test) {
            pool.retain(|d| !d.test || Some(d.file.as_str()) == cur);
        }
        if pool.is_empty() {
            return Narrowed::Ambiguous(pool);
        }
        // A dependency is declared in many manifests: the one of the package the reviewer is in, else the first.
        if pool.len() > 1 && pool.iter().all(|d| matches!(d.kind, Kind::Dependency | Kind::Package) && d.name == pool[0].name) {
            pool.sort_by(|x, y| x.file.cmp(&y.file));
            let near = pool.iter().find(|d| matches!(self.scope_of(d), Scope::File | Scope::Crate)).unwrap_or(&pool[0]).clone();
            let scope = self.scope_of(&near);
            return Narrowed::Unique(near, scope);
        }
        if pool.len() == 1 {
            let d = pool.remove(0);
            let scope = self.scope_of(&d);
            if d.test && scope != Scope::File {
                return Narrowed::Ambiguous(vec![d]);
            }
            return Narrowed::Unique(d, scope);
        }
        let in_file: Vec<&DefInfo> = pool.iter().filter(|d| Some(d.file.as_str()) == cur).collect();
        if in_file.len() == 1 {
            return Narrowed::Unique(in_file[0].clone(), Scope::File);
        }
        let in_crate: Vec<&DefInfo> = pool.iter().filter(|d| self.scope_of(d) == Scope::Crate).collect();
        if in_crate.len() == 1 {
            return Narrowed::Unique(in_crate[0].clone(), Scope::Crate);
        }
        let recent: Vec<&DefInfo> = pool.iter().filter(|d| self.ctx.recent.contains(d)).collect();
        if recent.len() == 1 {
            return Narrowed::Unique(recent[0].clone(), Scope::Recent);
        }
        Narrowed::Ambiguous(pool)
    }

    /// Every window of words whose reading is the key of a definition.
    fn candidates(&self) -> Vec<Cand> {
        let n = self.spoken.len();
        let known = |t: &str| self.known(t);
        let mut out = Vec::new();
        for a in 0..n {
            // A name written as one word, or as pieces that are not its words: `scrollmanager` for
            // `ScrollManager`, `unwrapor else` for `unwrap_or_else`.
            for len in 1..=3.min(n - a) {
                let window = &self.spoken[a..a + len];
                if window.iter().any(|w| w.identifier_like) {
                    continue;
                }
                let joined: String = window.iter().map(|w| w.clean.as_str()).collect();
                if joined.chars().count() < 6 || is_common_word(&joined) {
                    continue;
                }
                let defs = self.index().lookup_joined(&joined);
                if let Some(first) = defs.first() {
                    let tokens = symbols_tokens_of(&first.name);
                    // Words that ARE the name's words are the plain match (with its rule for a joining word).
                    if window.iter().map(|w| w.clean.as_str()).eq(tokens.iter().map(String::as_str)) {
                        continue;
                    }
                    let kinds = self.adjacent_kinds(a, a + len);
                    out.push(Cand {
                        a,
                        b: a + len,
                        reading: Reading { tokens, via: VIA_JOINED },
                        defs,
                        kinds,
                        biased: window.iter().any(|w| w.biased),
                        identifier: window.iter().any(|w| w.allcaps),
                        edge_literal: false,
                    });
                }
            }
            for len in 1..=self.r.options.max_window.min(n - a) {
                let b = a + len;
                let window = &self.spoken[a..b];
                let kinds = self.adjacent_kinds(a, b);
                let biased = window.iter().any(|w| w.biased);
                let identifier = window.iter().any(|w| w.identifier_like || w.allcaps);
                let first_stop = is_stop_word(&window[0].clean);
                let last_stop = is_stop_word(&window[len - 1].clean);
                for reading in readings(window, &known) {
                    // A name begins with a joining word (`is_empty`, `get_margin_top`) as often as a
                    // sentence does, and ends with one (`unwrap_or_else`) as it ends one only when it
                    // is all everyday words.
                    let everyday = reading.tokens.iter().all(|t| is_common_word(t));
                    let edge_stop = first_stop || (last_stop && everyday);
                    let edge_literal = reading.via == 0 && edge_stop && kinds.is_empty() && !biased;
                    let defs = self.index().lookup_key(&key_of(&reading.tokens));
                    if defs.is_empty() {
                        continue;
                    }
                    out.push(Cand { a, b, reading, defs, kinds: kinds.clone(), biased, identifier, edge_literal });
                }
            }
        }
        out
    }

    fn evaluate(&self, c: &Cand) -> Eval {
        let tokens = &c.reading.tokens;
        let all_common = tokens.iter().all(|t| is_common_word(t));
        let kind_ok = !c.kinds.is_empty() && c.defs.iter().any(|d| c.kinds.contains(&d.kind));
        let defs: Vec<DefInfo> = if kind_ok { c.defs.iter().filter(|d| c.kinds.contains(&d.kind)).cloned().collect() } else { c.defs.clone() };
        // A name written as one word is evidence when the name is more than everyday words.
        let spelled = c.reading.via & (VIA_LETTERS | VIA_HOMOPHONE | VIA_DIGITS) != 0 || (c.reading.via & VIA_JOINED != 0 && !all_common);
        let identifier = c.identifier || c.reading.via & VIA_IDENTIFIER != 0;
        let named_package = defs.iter().all(|d| matches!(d.kind, Kind::Dependency | Kind::Package));
        let strong = kind_ok || c.biased || spelled || identifier;
        let code_like = if tokens.len() == 1 {
            if all_common {
                kind_ok || c.biased || identifier
            } else {
                strong || named_package
            }
        } else {
            strong || tokens.len() >= 3 || !all_common
        };
        let narrowed = self.narrow(defs);
        let scope_bonus = match &narrowed {
            Narrowed::Unique(_, Scope::File) => 0.15,
            Narrowed::Unique(_, Scope::Crate) => 0.05,
            _ => 0.0,
        };
        let score = (0.30
            + 0.25 * f32::from(u8::from(kind_ok))
            + 0.20 * f32::from(u8::from(c.biased))
            + 0.20 * f32::from(u8::from(spelled))
            + 0.25 * f32::from(u8::from(identifier))
            + 0.05 * (tokens.len().min(4) as f32)
            + scope_bonus)
            .clamp(0.0, 1.0);
        Eval { code_like, kind_ok, hint_worthy: tokens.len() >= 3 || (tokens.len() == 2 && !all_common), narrowed, score }
    }

    fn run(&mut self) -> Resolution {
        let mut resolution = Resolution::default();
        if self.spoken.is_empty() {
            resolution.corrected = self.words.iter().map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
            return resolution;
        }
        let cands = self.candidates();
        let evals: Vec<Eval> = cands.iter().map(|c| self.evaluate(c)).collect();
        // The certain mentions: the longest first, then the one that said more.
        let mut order: Vec<usize> = (0..cands.len())
            .filter(|&i| !cands[i].edge_literal && evals[i].code_like && matches!(evals[i].narrowed, Narrowed::Unique(..)))
            .collect();
        order.sort_by(|&x, &y| {
            let (cx, cy) = (&cands[x], &cands[y]);
            (cy.b - cy.a)
                .cmp(&(cx.b - cx.a))
                .then_with(|| evals[y].score.partial_cmp(&evals[x].score).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| cx.a.cmp(&cy.a))
        });
        let mut mentions: Vec<(usize, Certain)> = Vec::new();
        for i in order {
            let c = &cands[i];
            if mentions.len() >= MAX_MENTIONS {
                break;
            }
            if !self.free(c.a, c.b) {
                continue;
            }
            if let Narrowed::Unique(def, scope) = &evals[i].narrowed {
                self.claim(c.a, c.b);
                mentions.push((
                    i,
                    Certain {
                        words: self.orig_range(c.a, c.b),
                        def: def.clone(),
                        scope: *scope,
                        via: c.reading.via,
                        repaired: false,
                        score: evals[i].score,
                    },
                ));
            }
        }
        // A mention that is part of a longer name that begins or ends with a joining word (`margin top`
        // in `get margin top`) is not certain: the words may name either.
        let mut extra: Vec<(u8, usize, usize, Hint)> = Vec::new();
        let mut kept: Vec<(usize, Certain)> = Vec::new();
        for (i, cert) in mentions {
            let c = &cands[i];
            let longer = cands.iter().find(|f| f.via_words() && f.a <= c.a && c.b <= f.b && (f.a, f.b) != (c.a, c.b));
            match longer {
                Some(f) => {
                    for flag in &mut self.claimed[c.a..c.b] {
                        *flag = false;
                    }
                    let mut pool = f.defs.clone();
                    pool.push(cert.def.clone());
                    let candidates = self.candidates_of(&pool);
                    let words = self.orig_range(f.a, f.b);
                    extra.push((
                        3,
                        f.a,
                        f.b,
                        Hint {
                            said: self.heard_text(&words),
                            words,
                            patterns: vec![pattern_of(&f.reading.tokens), pattern_of(&c.reading.tokens)],
                            kinds: Vec::new(),
                            folders: self.folders_of(&candidates),
                            candidates,
                            similar: Vec::new(),
                        },
                    ));
                }
                None => kept.push((i, cert)),
            }
        }
        let chosen: Vec<usize> = kept.iter().map(|(i, _)| *i).collect();
        resolution.certain = kept.into_iter().map(|(_, c)| c).collect();
        self.repair(&mut resolution.certain);
        // Acronyms of the list that are no name: written as the acronym.
        let acronyms = self.acronym_corrections();
        resolution.hints = self.hints(&cands, &evals, &chosen, extra);
        resolution.certain.sort_by_key(|c| c.words.start);
        let mut corrections: Vec<Correction> = resolution
            .certain
            .iter()
            .map(|c| Correction { words: c.words.clone(), from: self.heard_text(&c.words), to: c.def.name.clone(), kind: CorrectionKind::Symbol })
            .collect();
        corrections.extend(acronyms);
        corrections.sort_by_key(|c| c.words.start);
        resolution.corrected = self.corrected_text(&corrections);
        resolution.corrections = corrections;
        resolution
    }

    fn doubted(&self, i: usize) -> bool {
        self.spoken[i].probability.is_some_and(|p| p <= self.r.options.low_probability)
    }

    /// A spelled-out acronym next to a word the recogniser doubted: the name whose first or last word
    /// is the acronym and whose other word sounds like the doubted one.
    fn repair(&mut self, certain: &mut Vec<Certain>) {
        let n = self.spoken.len();
        let mut a = 0;
        while a < n && certain.len() < MAX_MENTIONS {
            let known = |t: &str| self.known(t);
            let run = if self.claimed[a] { None } else { acronym_run_at(&self.spoken, a, &known) };
            let Some((end, acronym)) = run else {
                a += 1;
                continue;
            };
            if !self.free(a, end) {
                a += 1;
                continue;
            }
            let mut found: Option<(usize, usize, DefInfo, Scope)> = None;
            if end < n && self.free(end, end + 1) && self.doubted(end) {
                found = self.repair_pair(&acronym, end, true).map(|(d, s)| (a, end + 1, d, s));
            }
            if found.is_none() && a > 0 && self.free(a - 1, a) && self.doubted(a - 1) {
                found = self.repair_pair(&acronym, a - 1, false).map(|(d, s)| (a - 1, end, d, s));
            }
            if let Some((from, to, def, scope)) = found {
                self.claim(from, to);
                certain.push(Certain { words: self.orig_range(from, to), def, scope, via: VIA_LETTERS, repaired: true, score: 0.55 });
                a = to;
            } else {
                a += 1;
            }
        }
    }

    fn repair_pair(&self, acronym: &str, word_at: usize, acronym_first: bool) -> Option<(DefInfo, Scope)> {
        let heard = self.spoken[word_at].clean.as_str();
        if heard.chars().count() < 3 {
            return None;
        }
        let heard_skeleton = phonetic(heard);
        let mut keys: Vec<String> = Vec::new();
        self.index().for_each_key(|k| {
            if k.ntokens != 2 || keys.len() >= 8 {
                return;
            }
            let Some((first, second)) = k.key.split_once(' ') else {
                return;
            };
            let (acr, word) = if acronym_first { (first, second) } else { (second, first) };
            if acr != acronym || word.chars().count() < 3 {
                return;
            }
            let skeleton = phonetic(word);
            if within_one(heard_skeleton.as_bytes(), skeleton.as_bytes()).is_some() || edit_distance_bounded(heard, word, 2).is_some() {
                keys.push(k.key.to_string());
            }
        });
        let defs: Vec<DefInfo> = keys.iter().flat_map(|k| self.index().lookup_key(k)).collect();
        match self.narrow(defs) {
            Narrowed::Unique(def, scope) => Some((def, scope)),
            Narrowed::Ambiguous(_) => None,
        }
    }

    /// Spelled-out acronyms of the list in words that are not part of a mention.
    fn acronym_corrections(&mut self) -> Vec<Correction> {
        let n = self.spoken.len();
        let mut out = Vec::new();
        let mut a = 0;
        while a < n {
            if self.claimed[a] {
                a += 1;
                continue;
            }
            let Some((end, acronym)) = acronym_run_at(&self.spoken, a, &|_| false) else {
                a += 1;
                continue;
            };
            if !self.free(a, end) || end - a < 2 {
                a += 1;
                continue;
            }
            let words = self.orig_range(a, end);
            out.push(Correction { from: self.heard_text(&words), to: acronym.to_uppercase(), words, kind: CorrectionKind::Acronym });
            self.claim(a, end);
            a = end;
        }
        out
    }

    fn corrected_text(&self, corrections: &[Correction]) -> String {
        let mut out: Vec<String> = Vec::with_capacity(self.words.len());
        let mut i = 0;
        while i < self.words.len() {
            if let Some(c) = corrections.iter().find(|c| c.words.start == i && c.words.end > i) {
                let first = &self.words[c.words.start].text;
                let last = &self.words[c.words.end - 1].text;
                let prefix: String = first.chars().take_while(|ch| !ch.is_alphanumeric()).collect();
                let suffix: String = {
                    let rev: Vec<char> = last.chars().rev().take_while(|ch| !ch.is_alphanumeric()).collect();
                    rev.into_iter().rev().collect()
                };
                out.push(format!("{prefix}{}{suffix}", c.to));
                i = c.words.end;
            } else {
                out.push(self.words[i].text.clone());
                i += 1;
            }
        }
        out.join(" ")
    }

    // ---- hints ----

    fn candidates_of(&self, pool: &[DefInfo]) -> Vec<DefInfo> {
        let mut list: Vec<DefInfo> = pool.to_vec();
        list.sort_by(|x, y| {
            let near = |d: &DefInfo| u8::from(self.scope_of(d) == Scope::Workspace);
            x.test.cmp(&y.test).then(near(x).cmp(&near(y))).then_with(|| x.file.cmp(&y.file)).then(x.line.cmp(&y.line))
        });
        list.truncate(3);
        list
    }

    fn folders_of(&self, defs: &[DefInfo]) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for d in defs {
            push_unique(&mut out, parent_dir(&d.file).to_string(), 3);
        }
        out
    }

    fn hints(&self, cands: &[Cand], evals: &[Eval], chosen: &[usize], extra: Vec<(u8, usize, usize, Hint)>) -> Vec<Hint> {
        // (strength, first word, end, hint)
        let mut found: Vec<(u8, usize, usize, Hint)> = extra;
        // 1. Names that were spoken exactly but are not certain: several, or only words.
        let mut seen_windows: Vec<(usize, usize)> = found.iter().map(|(_, a, b, _)| (*a, *b)).collect();
        for (i, c) in cands.iter().enumerate() {
            if chosen.contains(&i) || !self.free(c.a, c.b) || seen_windows.contains(&(c.a, c.b)) {
                continue;
            }
            let e = &evals[i];
            if !(e.code_like || e.kind_ok || e.hint_worthy) || (c.edge_literal && c.reading.tokens.len() < 3 && !e.kind_ok) {
                continue;
            }
            let pool: Vec<DefInfo> = match &e.narrowed {
                Narrowed::Ambiguous(p) => p.clone(),
                Narrowed::Unique(d, _) => vec![d.clone()],
            };
            if pool.is_empty() {
                continue;
            }
            seen_windows.push((c.a, c.b));
            let words = self.orig_range(c.a, c.b);
            let candidates = self.candidates_of(&pool);
            found.push((
                if e.code_like || e.kind_ok { 3 } else { 1 },
                c.a,
                c.b,
                Hint {
                    said: self.heard_text(&words),
                    words,
                    patterns: vec![pattern_of(&c.reading.tokens)],
                    kinds: c.kinds.clone(),
                    folders: self.folders_of(&candidates),
                    candidates,
                    similar: Vec::new(),
                },
            ));
        }
        // 2. Windows for the fuzzy search: after a kind word, and every run of two to four words.
        let mut queries: Vec<Query> = Vec::new();
        let n = self.spoken.len();
        let known = |t: &str| self.known(t);
        for k in 0..n {
            let kinds = kind_words(&self.spoken[k].clean);
            if kinds.is_empty() {
                continue;
            }
            let mut s = k + 1;
            while s < n && KIND_CONNECTIVES.contains(&self.spoken[s].clean.as_str()) {
                s += 1;
            }
            let mut e = s;
            while e < n && e - s < 3 && !is_stop_word(&self.spoken[e].clean) && kind_words(&self.spoken[e].clean).is_empty() {
                e += 1;
            }
            if e > s && self.free(s, e) {
                let window = &self.spoken[s..e];
                if let Some(r) = readings(window, &known).into_iter().find(|r| !r.tokens.is_empty()) {
                    queries.push(self.query(s, e, &r, 3, kinds.to_vec()));
                }
            }
        }
        for a in 0..n {
            for len in 2..=4.min(n - a) {
                let b = a + len;
                if !self.free(a, b) {
                    continue;
                }
                let window = &self.spoken[a..b];
                let edge_stop = is_stop_word(&window[0].clean) || is_stop_word(&window[len - 1].clean);
                if window.iter().any(|w| !kind_words(&w.clean).is_empty()) {
                    continue;
                }
                // Everyday words alone are not worth a search; a reading that spelled something out is.
                let all_common = window.iter().all(|w| is_common_word(&w.clean));
                let found = readings(window, &known).into_iter().take(3).chain(plural_letter_readings(window));
                for r in found {
                    if all_common && r.via == 0 {
                        continue;
                    }
                    if r.tokens.len() >= 2 && r.tokens.len() <= 5 {
                        let mut q = self.query(a, b, &r, if r.via == 0 { 1 } else { 2 }, Vec::new());
                        if edge_stop {
                            q.strength = 0; // only a skeleton that is equal counts: see `fuzzy`
                        }
                        queries.push(q);
                    }
                }
            }
        }
        let matches = self.fuzzy(&queries);
        for (q, m) in queries.iter().zip(matches) {
            if !self.free(q.a, q.b) || found.iter().any(|(_, a, b, _)| *a < q.b && q.a < *b) {
                continue;
            }
            let kind_led = !q.kinds.is_empty();
            if m.is_empty() && !kind_led {
                continue;
            }
            let mut patterns: Vec<String> = Vec::new();
            let mut similar: Vec<String> = Vec::new();
            let mut defs: Vec<DefInfo> = Vec::new();
            for key in &m {
                let found_defs = self.index().lookup_key(key);
                if let Some(first) = found_defs.iter().find(|d| !d.test).or_else(|| found_defs.first()) {
                    push_unique(&mut similar, first.name.clone(), 3);
                    defs.push(first.clone());
                }
                push_unique(&mut patterns, key.replace(' ', ".?"), 2);
            }
            for p in &q.patterns {
                push_unique(&mut patterns, p.clone(), 3);
            }
            let words = self.orig_range(q.a, q.b);
            let folders = self.crate_folder(&defs);
            found.push((
                q.strength.max(u8::from(kind_led) * 3).max(1),
                q.a,
                q.b,
                Hint { said: self.heard_text(&words), words, patterns, kinds: q.kinds.clone(), candidates: Vec::new(), similar, folders },
            ));
        }
        // The strongest first, none over another, at most `max_hints`.
        found.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.cmp(&y.1)));
        let mut taken: Vec<(usize, usize)> = Vec::new();
        let mut out: Vec<(usize, Hint)> = Vec::new();
        for (strength, a, b, hint) in found {
            if strength == 0 || out.len() >= self.r.options.max_hints || taken.iter().any(|(x, y)| *x < b && a < *y) {
                continue;
            }
            taken.push((a, b));
            out.push((a, hint));
        }
        out.sort_by_key(|(a, _)| *a);
        out.into_iter().map(|(_, h)| h).collect()
    }

    /// The folder of the package that has all of `defs`, when they are in one.
    fn crate_folder(&self, defs: &[DefInfo]) -> Vec<String> {
        let mut dirs: Vec<Option<String>> = defs.iter().map(|d| self.crate_of(&d.file)).collect();
        dirs.dedup();
        match dirs.as_slice() {
            [Some(dir)] if !dir.is_empty() => vec![dir.clone()],
            _ => Vec::new(),
        }
    }

    fn query(&self, a: usize, b: usize, reading: &Reading, strength: u8, kinds: Vec<Kind>) -> Query {
        let key = key_of(&reading.tokens);
        let cat = concat_of(&key);
        let mut skeleton = phonetic(&cat).into_bytes();
        skeleton.truncate(MAX_SKELETON);
        Query { a, b, ntokens: reading.tokens.len(), patterns: vec![pattern_of(&reading.tokens)], key, cat, skeleton, strength, kinds }
    }

    /// For each query the keys of the index that look or sound like it (at most three, the closest
    /// first): the same number of words, a skeleton at most one edit away, the letters at most three
    /// (four when the skeleton is equal) edits away. One pass over the keys for all the queries.
    fn fuzzy(&self, queries: &[Query]) -> Vec<Vec<String>> {
        let mut results: Vec<Vec<(usize, usize, String)>> = vec![Vec::new(); queries.len()];
        if queries.is_empty() {
            return Vec::new();
        }
        // table[ntokens][skeleton length] -> the queries
        let mut table: Vec<Vec<Vec<usize>>> = vec![vec![Vec::new(); MAX_SKELETON + 2]; 7];
        for (i, q) in queries.iter().enumerate() {
            if q.ntokens < 7 && q.skeleton.len() <= MAX_SKELETON + 1 {
                table[q.ntokens][q.skeleton.len()].push(i);
            }
        }
        self.index().for_each_key(|k| {
            if k.ntokens >= 7 {
                return;
            }
            let len = k.phonetic.len();
            let from = len.saturating_sub(1);
            let to = (len + 1).min(MAX_SKELETON + 1);
            for bucket in table[k.ntokens].iter().skip(from).take(to + 1 - from) {
                for &qi in bucket {
                    let q = &queries[qi];
                    if k.key == q.key {
                        continue;
                    }
                    let Some(dist) = within_one(&q.skeleton, k.phonetic) else {
                        continue;
                    };
                    if q.strength == 0 && dist != 0 {
                        continue;
                    }
                    // A skeleton of two letters is only worth an equal one; the name's own may be longer by one.
                    if dist != 0 && (q.skeleton.len() < 2 || k.phonetic.len() < 3) {
                        continue;
                    }
                    let limit = if dist == 0 { 4 } else { 3 };
                    let Some(edit) = edit_distance_bounded(&q.cat, &concat_of(k.key), limit) else {
                        continue;
                    };
                    // Every word is close to the name's word at its place (one letter off at most).
                    if !q.key.split(' ').zip(k.key.split(' ')).all(|(x, y)| x == y || edit_distance_bounded(x, y, 1).is_some() || (x.len() >= 2 && phonetic(x) == phonetic(y))) {
                        continue;
                    }
                    let list = &mut results[qi];
                    list.push((dist, edit, k.key.to_string()));
                    if list.len() > 24 {
                        list.sort();
                        list.truncate(3);
                    }
                }
            }
        });
        results
            .into_iter()
            .map(|mut list| {
                list.sort();
                list.dedup();
                list.into_iter().take(3).map(|(_, _, key)| key).collect()
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "symbols_resolve_test.rs"]
mod symbols_resolve_test;
