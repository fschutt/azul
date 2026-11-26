//! The words the speech recogniser is told to favour, from the code the reviewer is looking at: the
//! identifiers on the page, the symbols of the file, what was said lately, the symbols of the folder and
//! the project's own names (its types, its packages, what they depend on), within the number of words
//! the speech layer takes ([`VocabOptions::max_words`], the speech API's `max_keywords`, 200), and the
//! few keywords of the code that a recogniser writes as other words (`enum` for "enemy").
//!
//! Each entry is a name AS WRITTEN (`CssError`, `unwrap_or_else`, `azul-css`): the speech layer shapes
//! the list for each recogniser (the system's takes the compounds literally; the built-in one gets
//! the spoken parts added), and the speech agent measured that a compound, its spoken parts and an
//! acronym of the same name do not need to be passed twice. What is left out: an everyday word (a
//! recogniser pulled toward `error` or `value` writes them where nobody said them), a name of fewer than three
//! characters or more than [`VocabOptions::max_chars`], a name in a test unless the page shows it.
//!
//! Pure and cheap: one pass over the identifiers of the page (at most 64 KB of it), the defs of the file
//! and its folder, and, for the project's own names, the keys of the index that can matter.

use std::collections::HashMap;

use crate::{
    symbols::{Index, Kind},
    symbols_spoken::is_common_word,
    symbols_tokens::{key_of, name_key, split_identifier},
};

/// The keywords a recogniser writes as other words; they take the last places of the list.
pub static KEYWORDS: [&str; 11] = ["async", "await", "crate", "dyn", "enum", "impl", "mutex", "struct", "trait", "unwrap", "vec"];

/// How much of the page is read.
const MAX_PAGE_BYTES: usize = 64 * 1024;
/// The most distinct identifiers taken from the page.
const MAX_PAGE_IDENTIFIERS: usize = 400;

const TIER_PAGE: i64 = 1_000;
const TIER_FILE: i64 = 800;
const TIER_RECENT: i64 = 700;
const TIER_FOLDER: i64 = 500;
const TIER_DOMAIN: i64 = 200;

/// The list and where it came from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vocabulary {
    /// The words, the best first, the keywords last.
    pub words: Vec<String>,
    /// How many names were left out for the limits (the list's own and what each source may give).
    pub cut: usize,
    pub from_page: usize,
    pub from_file: usize,
    pub from_recent: usize,
    pub from_folder: usize,
    pub from_domain: usize,
}

/// What may go in.
#[derive(Clone, Debug)]
pub struct VocabOptions {
    /// The most words, keywords included.
    pub max_words: usize,
    /// The longest name.
    pub max_chars: usize,
    /// End the list with [`KEYWORDS`].
    pub keywords: bool,
    /// The most project-wide names.
    pub domain_terms: usize,
    /// The most names of the folder's other files.
    pub folder_names: usize,
    /// The most names from the page's text (the rest of the list is the file, what was said, the folder and the project).
    pub page_names: usize,
    /// The most names of the file shown.
    pub file_names: usize,
}

impl Default for VocabOptions {
    fn default() -> Self {
        Self { max_words: 200, max_chars: 64, keywords: true, domain_terms: 60, folder_names: 40, page_names: 60, file_names: 60 }
    }
}

/// What the reviewer is looking at.
#[derive(Clone, Copy, Debug)]
pub struct VocabContext<'a> {
    /// The text of the visible page.
    pub page_text: &'a str,
    /// The file shown (workspace-relative).
    pub file: Option<&'a str>,
    /// Names found or said lately, the latest first.
    pub recent: &'a [String],
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Page,
    File,
    Recent,
    Folder,
    Domain,
}

struct Entry {
    name: String,
    score: i64,
    source: Source,
    /// A type, spelled in preference to a function of the same words.
    type_like: bool,
}

/// What a name was seen as.
struct Seen {
    score: i64,
    source: Source,
    /// A type, spelled in preference to a function of the same words.
    type_like: bool,
    test: bool,
    /// The name stands on the page being reviewed.
    page: bool,
    /// A package or a dependency: its name may be one everyday word.
    package: bool,
}

impl Seen {
    const fn new(score: i64, source: Source) -> Self {
        Self { score, source, type_like: false, test: false, page: false, package: false }
    }
}

struct Collector<'a> {
    index: &'a Index,
    options: &'a VocabOptions,
    by_key: HashMap<String, usize>,
    entries: Vec<Entry>,
}

impl Collector<'_> {
    /// Takes a name: left out when it is no use to a recogniser; else kept with the best score it had.
    fn consider(&mut self, name: &str, seen: &Seen) {
        let chars = name.chars().count();
        if chars < 3 || chars > self.options.max_chars || (seen.test && !seen.page) {
            return;
        }
        let words = split_identifier(name);
        if words.is_empty() || (words.len() == 1 && is_common_word(&words[0]) && !seen.package) {
            return;
        }
        let key = key_of(&words);
        // Rarer words are worth more of the recogniser's attention.
        let rarity: i64 = words
            .iter()
            .map(|w| i64::from(self.index.token_count(w).min(2_000)))
            .map(|n| (20 - (n as f64 + 1.0).ln() as i64 * 3).max(0))
            .sum::<i64>()
            .min(90);
        let score = seen.score + rarity;
        match self.by_key.get(&key) {
            Some(&at) => {
                let e = &mut self.entries[at];
                if score > e.score {
                    e.score = score;
                    e.source = seen.source;
                }
                if seen.type_like && !e.type_like {
                    e.type_like = true;
                    e.name = name.to_string();
                }
            }
            None => {
                self.by_key.insert(key, self.entries.len());
                self.entries.push(Entry { name: name.to_string(), score, source: seen.source, type_like: seen.type_like });
            }
        }
    }
}

/// The identifier-shaped runs of the page text.
fn page_identifiers(text: &str, max_chars: usize) -> Vec<&str> {
    let mut end = text.len().min(MAX_PAGE_BYTES);
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let text = &text[..end];
    let mut out: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && out.len() < MAX_PAGE_IDENTIFIERS {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &text[start..i];
            if (3..=max_chars).contains(&word.len()) && seen.insert(word) {
                out.push(word);
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Whether a run of a page is the shape of a type or a camel-cased name (`CssError`, `cssError`): a name written with
/// underscores is a module path or an attribute as often as a name, it is taken when the index defines it.
fn identifier_shaped(word: &str) -> bool {
    word.as_bytes().windows(2).any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase())
}

/// The list for what the reviewer is looking at.
#[must_use]
pub fn build(index: &Index, ctx: &VocabContext<'_>, options: &VocabOptions) -> Vocabulary {
    let mut c = Collector { index, options, by_key: HashMap::new(), entries: Vec::new() };
    // The page: what is written on it, when it is a name or is defined.
    let mut from_page = 0;
    let mut capped = 0usize;
    let page_words = page_identifiers(ctx.page_text, options.max_chars);
    for (at, word) in page_words.iter().copied().enumerate() {
        if from_page >= options.page_names {
            capped += page_words.len() - at;
            break;
        }
        let defined = !index.lookup_key(&name_key(word)).is_empty();
        if identifier_shaped(word) || defined {
            from_page += 1;
            let type_like = index.lookup_key(&name_key(word)).iter().any(|d| d.kind.is_type_like());
            c.consider(word, &Seen { type_like, page: true, ..Seen::new(TIER_PAGE + if defined { 10 } else { 0 }, Source::Page) });
        }
    }
    // The file shown.
    if let Some(file) = ctx.file {
        let defs = index.defs_of_file(file);
        capped += defs.len().saturating_sub(options.file_names);
        for d in defs.into_iter().take(options.file_names) {
            let bonus = i64::from(d.vis == crate::symbols::Vis::Public) * 5 + i64::from(d.kind.is_type_like()) * 5;
            c.consider(&d.name, &Seen { type_like: d.kind.is_type_like(), test: d.test, ..Seen::new(TIER_FILE + bonus, Source::File) });
        }
    }
    // What was said lately: the latest the most.
    for (n, name) in ctx.recent.iter().enumerate() {
        let found = index.lookup_key(&name_key(name));
        let type_like = found.iter().any(|d| d.kind.is_type_like());
        c.consider(name, &Seen { type_like, ..Seen::new(TIER_RECENT - i64::try_from(n.min(50)).unwrap_or(50), Source::Recent) });
    }
    // The other files of the folder.
    if let Some(file) = ctx.file {
        let dir = crate::symbols::parent_dir(file).to_string();
        let mut taken = 0;
        index.for_each_file(|path| {
            if taken >= options.folder_names || path == file || crate::symbols::parent_dir(path) != dir {
                return;
            }
            for d in index.defs_of_file(path) {
                if taken >= options.folder_names {
                    break;
                }
                if d.vis == crate::symbols::Vis::Public && !d.test {
                    taken += 1;
                    c.consider(&d.name, &Seen { type_like: d.kind.is_type_like(), ..Seen::new(TIER_FOLDER, Source::Folder) });
                }
            }
        });
    }
    // The project's own names: types of two words or more, packages and dependencies.
    let mut domain: Vec<(i64, String, bool)> = Vec::new();
    index.for_each_key(|k| {
        let Some(summary) = index.key_summary(k.id) else {
            return;
        };
        if !summary.non_test {
            return;
        }
        let wanted = summary.package || (summary.type_like && summary.public && k.ntokens >= 2);
        if !wanted {
            return;
        }
        let score = TIER_DOMAIN + i64::from(summary.type_like) * 10 + i64::from(summary.public) * 5 + i64::from(summary.package) * 15;
        domain.push((score, index.spelling(summary.best).to_string(), summary.type_like));
    });
    domain.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    capped += domain.len().saturating_sub(options.domain_terms);
    for (score, name, type_like) in domain.into_iter().take(options.domain_terms) {
        let package = index.lookup_key(&name_key(&name)).iter().any(|d| matches!(d.kind, Kind::Package | Kind::Dependency));
        c.consider(&name, &Seen { type_like, package, ..Seen::new(score, Source::Domain) });
    }
    // The best first; the keywords keep the last places.
    let mut entries = c.entries;
    entries.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.name.cmp(&b.name)));
    let keywords: &[&str] = if options.keywords { &KEYWORDS } else { &[] };
    let room = options.max_words.saturating_sub(keywords.len());
    let mut vocabulary = Vocabulary { cut: entries.len().saturating_sub(room) + capped, ..Vocabulary::default() };
    for e in entries.into_iter().take(room) {
        match e.source {
            Source::Page => vocabulary.from_page += 1,
            Source::File => vocabulary.from_file += 1,
            Source::Recent => vocabulary.from_recent += 1,
            Source::Folder => vocabulary.from_folder += 1,
            Source::Domain => vocabulary.from_domain += 1,
        }
        vocabulary.words.push(e.name);
    }
    if options.max_words > 0 {
        vocabulary.words.extend(keywords.iter().take(options.max_words).map(|k| (*k).to_string()));
    }
    vocabulary.words.truncate(options.max_words);
    vocabulary
}

#[cfg(test)]
#[path = "symbols_vocab_test.rs"]
mod symbols_vocab_test;
