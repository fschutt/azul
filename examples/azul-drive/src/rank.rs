//! How the search ranks and groups its results, as plain data. A query's terms are folded (lower
//! case, accents dropped) and split at anything that is not a letter or a digit; a name matches
//! when every term appears in it. The results are ranked by BM25 over what the search knows of
//! each file (its name, which counts three times, and the lines its contents matched on), with 2
//! added for a name match, and a term matches any word that holds it: German runs words together
//! and "Einkommensteuer" holds "steuer". The ranked results are grouped by their folder, the
//! folder of the best match first. A query that finds nothing gets a spelling the drive knows
//! (within two edits) as a suggestion.
//!
//! The search runs on worker threads and hands its rows over in batches; `FindState` keeps one
//! [`Doc`] per row and ranks again after every batch. No window types here.

use std::collections::HashMap;

/// BM25's term frequency saturation.
const K1: f32 = 1.2;
/// BM25's length normalisation.
const B: f32 = 0.75;
/// How much a word of the name counts against a word of the contents.
const NAME_WEIGHT: f32 = 3.0;
/// What a match of every term in the name adds to the score.
const NAME_BONUS: f32 = 2.0;
/// The snippet keeps this many characters before the first match ...
const SNIPPET_BEFORE: usize = 42;
/// ... and this many after its start.
const SNIPPET_AFTER: usize = 90;
/// The most edits a suggested spelling is away from the term typed.
const MAX_EDITS: usize = 2;

/// `text` lower case and without accents (`Ärger` -> `arger`), one character for each of
/// `text`'s, so a position in the folded text is the same position in `text`.
#[must_use]
pub fn fold(text: &str) -> String {
    text.chars().map(fold_char).collect()
}

/// One character lower case and without its accent: the first character of its lower case,
/// decomposed, as a browser's `normalize("NFD")` leaves it (letters without a decomposition,
/// such as ø or ł, stay).
fn fold_char(c: char) -> char {
    match c.to_lowercase().next().unwrap_or(c) {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => 'c',
        'ď' => 'd',
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => 'e',
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => 'g',
        'ĥ' => 'h',
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' => 'i',
        'ĵ' => 'j',
        'ķ' => 'k',
        'ĺ' | 'ļ' | 'ľ' => 'l',
        'ñ' | 'ń' | 'ņ' | 'ň' => 'n',
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ō' | 'ŏ' | 'ő' => 'o',
        'ŕ' | 'ŗ' | 'ř' => 'r',
        'ś' | 'ŝ' | 'ş' | 'š' => 's',
        'ţ' | 'ť' => 't',
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => 'u',
        'ŵ' => 'w',
        'ý' | 'ÿ' | 'ŷ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        other => other,
    }
}

/// The folded words of `text` of at least `min_chars` characters.
fn split_words(text: &str, min_chars: usize) -> Vec<String> {
    fold(text)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= min_chars)
        .map(str::to_string)
        .collect()
}

/// Where `term` first appears in `folded` (a position in characters).
fn find_chars(folded: &[char], term: &[char]) -> Option<usize> {
    if term.is_empty() || term.len() > folded.len() {
        return None;
    }
    (0..=folded.len() - term.len()).find(|&i| folded[i..i + term.len()] == *term)
}

/// The terms of a query: its folded words, split at anything but letters and digits.
#[must_use]
pub fn tokens(query: &str) -> Vec<String> {
    split_words(query, 1)
}

/// Whether every term of `query` appears in `name` (an empty query matches every name).
#[must_use]
pub fn name_matches(name: &str, query: &str) -> bool {
    let folded = fold(name);
    tokens(query).iter().all(|term| folded.contains(term.as_str()))
}

/// The parts of `text` the terms of `query` cover, as byte ranges of `text`, in order and
/// merged where they touch: what a result's name and snippet highlight.
#[must_use]
pub fn marks(text: &str, query: &str) -> Vec<(usize, usize)> {
    let folded: Vec<char> = fold(text).chars().collect();
    let count = folded.len();
    let mut marked = vec![false; count];
    for term in tokens(query) {
        let term: Vec<char> = term.chars().collect();
        if term.len() > count {
            continue;
        }
        for start in 0..=count - term.len() {
            if folded[start..start + term.len()] == term[..] {
                marked[start..start + term.len()].fill(true);
            }
        }
    }
    // Folding keeps one character per character, so a position in `folded` is one in `text`.
    let bytes: Vec<usize> = text
        .char_indices()
        .map(|(at, _)| at)
        .chain(std::iter::once(text.len()))
        .collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < count {
        if marked[i] {
            let start = i;
            while i < count && marked[i] {
                i += 1;
            }
            out.push((bytes[start], bytes[i]));
        } else {
            i += 1;
        }
    }
    out
}

/// What the ranking knows of one result: how often each word occurs (a name's word three
/// times), how long it is in words, and whether its name matches the query.
#[derive(Debug, Clone, Default)]
pub struct Doc {
    /// Each folded word and how much it counts.
    tf: HashMap<String, f32>,
    /// The words counted, a name's three times: BM25's document length.
    len: f32,
}

impl Doc {
    /// The document of a result named `name` whose contents matched on `text` (empty when it
    /// was found by its name alone).
    #[must_use]
    pub fn new(name: &str, text: &str) -> Doc {
        let mut doc = Doc::default();
        for word in split_words(name, 2) {
            *doc.tf.entry(word).or_insert(0.0) += NAME_WEIGHT;
            doc.len += NAME_WEIGHT;
        }
        for word in split_words(text, 2) {
            *doc.tf.entry(word).or_insert(0.0) += 1.0;
            doc.len += 1.0;
        }
        doc
    }

    /// How often the words holding `term` occur.
    fn tf_of(&self, term: &str) -> f32 {
        self.tf
            .iter()
            .filter(|(word, _)| word.contains(term))
            .map(|(_, count)| count)
            .sum()
    }
}

/// The BM25 scores of `docs` for `query` (each with [`NAME_BONUS`] when its name matches),
/// `searched` being how many files the search read (at least the results): the corpus the
/// terms' rarity is counted against.
#[must_use]
pub fn scores(docs: &[(&Doc, bool)], query: &str, searched: usize) -> Vec<f32> {
    let terms = tokens(query);
    let corpus = searched.max(docs.len()).max(1) as f32;
    let average = docs.iter().map(|(doc, _)| doc.len).sum::<f32>() / docs.len().max(1) as f32;
    // How many documents hold each word; a term's count is that of the most common word
    // holding it.
    let mut df: HashMap<&str, usize> = HashMap::new();
    for (doc, _) in docs {
        for word in doc.tf.keys() {
            *df.entry(word.as_str()).or_insert(0) += 1;
        }
    }
    let idf: Vec<f32> = terms
        .iter()
        .map(|term| {
            let held = df
                .iter()
                .filter(|(word, _)| word.contains(term.as_str()))
                .map(|(_, count)| *count)
                .max()
                .unwrap_or(0) as f32;
            (1.0 + (corpus - held + 0.5) / (held + 0.5)).ln()
        })
        .collect();
    docs.iter()
        .map(|(doc, named)| {
            let norm = if average > 0.0 { doc.len / average } else { 0.0 };
            let mut score = 0.0;
            for (term, idf) in terms.iter().zip(&idf) {
                let tf = doc.tf_of(term);
                if tf == 0.0 {
                    // Every term must be in the file for its contents to count.
                    score = 0.0;
                    break;
                }
                score += idf * (tf * (K1 + 1.0)) / (tf + K1 * (1.0 - B + B * norm));
            }
            score + if *named { NAME_BONUS } else { 0.0 }
        })
        .collect()
}

/// One folder of the results and the results in it (their positions in the list handed in),
/// best first.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub folder: String,
    pub positions: Vec<usize>,
}

/// The results grouped by their `folders`, ranked by `scores`: the folder of the best match
/// first, the best match first within a folder; equal scores keep the order they came in.
#[must_use]
pub fn group_by_folder(folders: &[&str], scores: &[f32]) -> Vec<Group> {
    let score = |i: usize| scores.get(i).copied().unwrap_or(0.0);
    let mut order: Vec<usize> = (0..folders.len()).collect();
    // A stable sort: equal scores keep the order they came in.
    order.sort_by(|&a, &b| score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal));
    let mut groups: Vec<Group> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for i in order {
        let folder = folders[i];
        match index.get(folder) {
            Some(&group) => groups[group].positions.push(i),
            None => {
                index.insert(folder, groups.len());
                groups.push(Group {
                    folder: folder.to_string(),
                    positions: vec![i],
                });
            }
        }
    }
    groups
}

/// A folder's group header: its drive and its folders from the drive's root, joined by `›`
/// (`Home › Documents › Steuer`).
#[must_use]
pub fn group_label(drive_name: &str, folder: &str) -> String {
    std::iter::once(drive_name)
        .chain(folder.split('/').filter(|part| !part.is_empty()))
        .collect::<Vec<&str>>()
        .join(" \u{203A} ")
}

/// The number of single-character edits (insert, delete, replace) between `a` and `b`.
#[must_use]
pub fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let replace = previous[j] + usize::from(ca != *cb);
            current[j + 1] = replace.min(previous[j + 1] + 1).min(current[j] + 1);
        }
        previous = current;
    }
    previous[b.len()]
}

/// "Did you mean": the word of `vocabulary` nearest to the last term of `query`, at most
/// [`MAX_EDITS`] edits away but not the term itself; terms shorter than three letters get
/// none. The first of equally near words wins.
#[must_use]
pub fn did_you_mean<'a>(query: &str, vocabulary: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let term = tokens(query).pop()?;
    let length = term.chars().count();
    if length < 3 {
        return None;
    }
    let mut best: Option<(usize, String)> = None;
    for word in vocabulary.into_iter().flat_map(|text| split_words(text, 3)) {
        if word.chars().count().abs_diff(length) > MAX_EDITS {
            continue;
        }
        let edits = edit_distance(&term, &word);
        if edits > 0 && edits <= MAX_EDITS && best.as_ref().is_none_or(|(b, _)| edits < *b) {
            best = Some((edits, word));
        }
    }
    best.map(|(_, word)| word)
}

/// The part of `text` around the first match of `query`: up to [`SNIPPET_BEFORE`] characters
/// before it and [`SNIPPET_AFTER`] from it, cut at spaces, with an ellipsis where text was cut;
/// and the marks of the terms in it. `None` when no term appears in `text`.
#[must_use]
pub fn snippet(text: &str, query: &str) -> Option<(String, Vec<(usize, usize)>)> {
    let folded: Vec<char> = fold(text).chars().collect();
    let chars: Vec<char> = text.chars().collect();
    let at = tokens(query)
        .iter()
        .filter_map(|term| find_chars(&folded, &term.chars().collect::<Vec<char>>()))
        .min()?;
    let count = chars.len();
    let mut start = at.saturating_sub(SNIPPET_BEFORE);
    let mut end = (at + SNIPPET_AFTER).min(count);
    // Cut at a space: after the first one past the start, before the last one before the end.
    if start > 0 {
        if let Some(space) = (start..count).find(|&i| chars[i] == ' ') {
            if space < at {
                start = space + 1;
            }
        }
    }
    if end < count {
        if let Some(space) = (0..=end).rev().find(|&i| chars[i] == ' ') {
            if space > at {
                end = space;
            }
        }
    }
    let mut cut = String::new();
    if start > 0 {
        cut.push_str("\u{2026} ");
    }
    cut.extend(&chars[start..end]);
    if end < count {
        cut.push_str(" \u{2026}");
    }
    let marks = marks(&cut, query);
    Some((cut, marks))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folding_drops_the_case_and_the_accents_and_keeps_every_position() {
        assert_eq!(fold("Größe Ärger Café"), "große arger cafe");
        assert_eq!(fold("ÉLAN naïve"), "elan naive");
        let text = "Übersicht";
        assert_eq!(fold(text).chars().count(), text.chars().count());
    }

    #[test]
    fn a_query_splits_into_folded_terms_at_anything_but_letters_and_digits() {
        assert_eq!(tokens("Steuer-Erklärung 2025!"), vec!["steuer", "erklarung", "2025"]);
        assert!(tokens("  -- ").is_empty());
    }

    #[test]
    fn every_term_of_the_query_must_appear_in_the_name() {
        assert!(name_matches("Einkommensteuererklärung 2025.pdf", "steuer 2025"));
        assert!(name_matches("Einkommensteuererklärung 2025.pdf", "ERKLARUNG"));
        assert!(!name_matches("Einkommensteuererklärung 2025.pdf", "steuer 2024"));
        assert!(name_matches("Rechnung.pdf", ""), "an empty query matches every name");
    }

    #[test]
    fn the_marks_are_byte_ranges_of_the_original_text_merged_where_they_touch() {
        assert_eq!(marks("Einkommensteuer.pdf", "steuer pdf"), vec![(9, 15), (16, 19)]);
        assert_eq!(marks("Ärger.txt", "arger"), vec![(0, 6)], "Ä is two bytes");
        assert_eq!(marks("abcabc", "abc bca"), vec![(0, 6)], "overlapping terms merge");
        assert!(marks("Rechnung.pdf", "steuer").is_empty());
    }

    /// The design's BM25: k1 1.2, b 0.75, idf = ln(1 + (N - df + .5) / (df + .5)), a name's
    /// words three times, a term counting every word that holds it - and 2 for a name match.
    #[test]
    fn the_score_is_bm25_with_the_names_counted_three_times_and_two_for_a_name_match() {
        let named = Doc::new("steuer.pdf", "");
        let read = Doc::new("notes.txt", "steuer steuer bescheid");
        let got = scores(&[(&named, true), (&read, false)], "steuer", 2);
        assert!((got[0] - (0.299_333_9 + 2.0)).abs() < 1e-4, "{got:?}");
        assert!((got[1] - 0.237_341_67).abs() < 1e-4, "{got:?}");
        let compound = Doc::new("notes.txt", "die Einkommensteuer ist fällig");
        let other = Doc::new("Urlaub.jpg", "");
        let got = scores(&[(&compound, false), (&other, false)], "steuer", 2);
        assert!(got[0] > 0.0, "a term matches any word holding it");
        assert_eq!(got[1], 0.0, "a file without the term scores nothing");
    }

    #[test]
    fn the_results_group_by_folder_with_the_folder_of_the_best_match_first() {
        let folders = ["Docs", "Tax", "Docs", "Tax", "Pics"];
        let groups = group_by_folder(&folders, &[1.0, 3.0, 0.5, 2.0, 0.1]);
        assert_eq!(
            groups,
            vec![
                Group {
                    folder: String::from("Tax"),
                    positions: vec![1, 3]
                },
                Group {
                    folder: String::from("Docs"),
                    positions: vec![0, 2]
                },
                Group {
                    folder: String::from("Pics"),
                    positions: vec![4]
                },
            ]
        );
        let ties = group_by_folder(&["A", "B", "A"], &[0.0, 0.0, 0.0]);
        assert_eq!(ties[0].positions, vec![0, 2], "equal scores keep the order they came in");
        assert_eq!(ties[1].folder, "B");
    }

    #[test]
    fn a_group_header_names_the_drive_and_the_folders_from_its_root() {
        assert_eq!(group_label("Home", "Documents/Steuer"), "Home › Documents › Steuer");
        assert_eq!(group_label("Home", ""), "Home");
    }

    #[test]
    fn did_you_mean_offers_a_known_word_within_two_edits() {
        let words = ["rechnung", "steuer", "urlaub"];
        assert_eq!(did_you_mean("steur", words), Some(String::from("steuer")));
        assert_eq!(did_you_mean("2025 stuer", words), Some(String::from("steuer")));
        assert_eq!(did_you_mean("xyzzy", words), None, "nothing near");
        assert_eq!(did_you_mean("st", words), None, "too short to guess");
        assert_eq!(did_you_mean("steuer", words), None, "the word itself is no suggestion");
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("", "abc"), 3);
    }

    #[test]
    fn a_snippet_is_the_text_around_the_first_match_cut_at_spaces() {
        let (short, marks) = snippet("Die Einkommensteuer ist fällig.", "steuer").expect("a match");
        assert_eq!(short, "Die Einkommensteuer ist fällig.");
        assert_eq!(&short[marks[0].0..marks[0].1], "steuer");
        let long = format!(
            "{} Die Einkommensteuer für 2025 ist bis Ende Juli fällig, {}",
            "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod",
            "sagt das Finanzamt in seinem Brief vom Juni, und die Frist lässt sich nicht verlängern."
        );
        let (cut, marks) = snippet(&long, "STEUER").expect("a match");
        assert!(cut.starts_with("\u{2026} "), "{cut}");
        assert!(cut.ends_with(" \u{2026}"), "{cut}");
        assert_eq!(&cut[marks[0].0..marks[0].1], "steuer");
        assert!(snippet("nothing here", "steuer").is_none());
    }
}
