//! The words of an identifier and what they sound like: the one place that says `DataTableCell`,
//! `data_table_cell` and `DATA-TABLE-CELL` are the same name, that "as review" sounds like
//! `AzReview`, and how far two spellings are apart. Pure and std-only: the index, the spoken-code
//! normaliser and the resolver all key on what is made here.
//!
//! - [`split_identifier`]: the lower-case words of a name (camel, Pascal, snake, kebab, screaming,
//!   acronyms (`HTTPRequest` is `http request`), digits as words of their own (`utf8` is `utf 8`)).
//! - [`key_of`] / [`name_key`]: the words joined by one space, the key two spellings share;
//!   [`concat_of`] drops the spaces again (the form an edit distance reads).
//! - [`phonetic`]: a coarse skeleton of the sound of the concatenation (vowels gone, voiced and
//!   unvoiced pairs merged), so "enemy" and `enum`, "as review" and `AzReview` have one. It is a
//!   hint's tool, never a proof: two names with one skeleton are only candidates.
//! - [`edit_distance_bounded`]: Levenshtein over letters, giving up past a bound, on at most
//!   [`MAX_DISTANCE_CHARS`] letters (a longer input is never "close").
//!
//! Every function is bounded in its input (a name of a megabyte costs what 128 characters cost) and
//! none panics on odd input.

/// The most characters of a name that are read; the rest is cut (on a character boundary).
pub const MAX_NAME_CHARS: usize = 128;
/// The most words one name gives.
pub const MAX_TOKENS: usize = 16;
/// The longest phonetic skeleton.
pub const MAX_PHONETIC: usize = 12;
/// The most letters either side of an edit distance holds.
pub const MAX_DISTANCE_CHARS: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Lower,
    Upper,
    Digit,
    Other,
}

fn class_of(c: char) -> Class {
    if c.is_ascii_digit() {
        Class::Digit
    } else if c.is_alphabetic() {
        if c.is_uppercase() {
            Class::Upper
        } else {
            Class::Lower
        }
    } else {
        Class::Other
    }
}

/// The lower-case words of `name`: `DataTableCell`, `dataTableCell`, `data_table_cell`,
/// `DATA_TABLE_CELL` and `data-table-cell` all give `data table cell`.
///
/// A run of capitals is one acronym word unless a lower-case letter follows it, which then
/// belongs to the next word (`HTTPRequest`: `http`, `request`); a digit run is a word of its own
/// (`sha256sum`: `sha`, `256`, `sum`); anything that is no letter or digit separates. At most
/// [`MAX_NAME_CHARS`] characters are read and [`MAX_TOKENS`] words are given.
#[must_use]
pub fn split_identifier(name: &str) -> Vec<String> {
    let chars: Vec<char> = name.chars().take(MAX_NAME_CHARS).collect();
    let classes: Vec<Class> = chars.iter().map(|&c| class_of(c)).collect();
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let class = classes[i];
        let prev = if i > 0 { classes[i - 1] } else { Class::Other };
        let next = classes.get(i + 1).copied().unwrap_or(Class::Other);
        let boundary = match class {
            Class::Other => true,
            Class::Digit => prev != Class::Digit,
            Class::Lower => prev == Class::Digit,
            Class::Upper => match prev {
                Class::Upper => next == Class::Lower,
                Class::Lower | Class::Digit | Class::Other => true,
            },
        };
        if boundary && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
            if out.len() >= MAX_TOKENS {
                return out;
            }
        }
        if class != Class::Other {
            cur.extend(c.to_lowercase());
        }
    }
    if !cur.is_empty() && out.len() < MAX_TOKENS {
        out.push(cur);
    }
    out
}

/// The key of a word sequence: the words joined by one space (`css error`). The empty sequence
/// has the empty key.
#[must_use]
pub fn key_of<S: AsRef<str>>(tokens: &[S]) -> String {
    let mut key = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            key.push(' ');
        }
        key.push_str(token.as_ref());
    }
    key
}

/// The key of a name: [`key_of`] its [`split_identifier`] words.
#[must_use]
pub fn name_key(name: &str) -> String {
    key_of(&split_identifier(name))
}

/// A key without its spaces (`datatablecell`): the form edit distances and the phonetic skeleton read.
#[must_use]
pub fn concat_of(key: &str) -> String {
    key.chars().filter(|c| *c != ' ').collect()
}

fn is_vowel(b: u8) -> bool {
    matches!(b, b'a' | b'e' | b'i' | b'o' | b'u')
}

/// A coarse skeleton of how `cat` (a lower- or mixed-case concatenation of words) sounds, upper
/// case, at most [`MAX_PHONETIC`] long: vowels are dropped (a leading one stays as `A`), `y`, `w`
/// and `h` are dropped inside a word, letters that differ only in voicing merge (`b`/`p`, `d`/`t`,
/// `g`/`k`, `v`/`f`, `z`/`s`), the common digraphs fold (`ph` to `F`, `ch` and `sh` to `X`, `ck`
/// to `K`, a silent `gh`, the `kn`/`wr`/`ps` of a start) and a run of one sound is one.
/// Letters outside ASCII are ignored; digits are kept.
///
/// "Same skeleton" means "may be what was said", never "is".
#[must_use]
pub fn phonetic(cat: &str) -> String {
    let s: Vec<u8> = cat
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|b| b.to_ascii_lowercase())
        .take(MAX_DISTANCE_CHARS)
        .collect();
    let at = |i: usize| s.get(i).copied().unwrap_or(0);
    let mut out: Vec<u8> = Vec::with_capacity(MAX_PHONETIC + 2);
    let push = |out: &mut Vec<u8>, code: u8| {
        if out.last() != Some(&code) && out.len() < MAX_PHONETIC {
            out.push(code);
        }
    };
    let mut i = 0;
    // The sounds of a start.
    match (at(0), at(1)) {
        (b'k' | b'g' | b'p', b'n') | (b'w', b'r') | (b'p', b's') => i = 1,
        (b'x', _) => {
            push(&mut out, b'S');
            i = 1;
        }
        (b'w', b'h') => i = 1,
        _ => {}
    }
    while i < s.len() && out.len() < MAX_PHONETIC {
        let c = s[i];
        let next = at(i + 1);
        let next2 = at(i + 2);
        let start = i == 0;
        i += 1;
        match c {
            _ if is_vowel(c) => {
                if start {
                    push(&mut out, b'A');
                }
            }
            b'y' => {
                if start && is_vowel(next) {
                    push(&mut out, b'Y');
                }
            }
            b'w' => {
                if start && is_vowel(next) {
                    push(&mut out, b'W');
                }
            }
            b'h' => {
                if start && is_vowel(next) {
                    push(&mut out, b'H');
                }
            }
            b'b' | b'p' => {
                if c == b'p' && next == b'h' {
                    i += 1;
                    push(&mut out, b'F');
                } else {
                    push(&mut out, b'P');
                }
            }
            b'c' => {
                if matches!(next, b'e' | b'i' | b'y') {
                    push(&mut out, b'S');
                } else if next == b'h' {
                    i += 1;
                    push(&mut out, b'X');
                } else if next == b'k' {
                    i += 1;
                    push(&mut out, b'K');
                } else {
                    push(&mut out, b'K');
                }
            }
            b'd' | b't' => {
                if c == b'd' && next == b'g' && matches!(next2, b'e' | b'i' | b'y') {
                    i += 2;
                    push(&mut out, b'J');
                } else if c == b't' && next == b'h' {
                    i += 1;
                    push(&mut out, b'T');
                } else if c == b't' && next == b'i' && matches!(next2, b'o' | b'a') {
                    push(&mut out, b'X');
                } else {
                    push(&mut out, b'T');
                }
            }
            b'f' | b'v' => push(&mut out, b'F'),
            b'g' | b'k' | b'q' => {
                if c == b'g' && next == b'h' {
                    // A silent gh (night, high), a hard one at the start (ghost).
                    i += 1;
                    if start {
                        push(&mut out, b'K');
                    }
                } else if c == b'g' && matches!(next, b'e' | b'i' | b'y') {
                    push(&mut out, b'J');
                } else {
                    push(&mut out, b'K');
                }
            }
            b'j' => push(&mut out, b'J'),
            b'l' => push(&mut out, b'L'),
            b'm' => push(&mut out, b'M'),
            b'n' => push(&mut out, b'N'),
            b'r' => push(&mut out, b'R'),
            b's' | b'z' => {
                if c == b's' && next == b'h' {
                    i += 1;
                    push(&mut out, b'X');
                } else if c == b's' && next == b'c' && next2 == b'h' {
                    i += 2;
                    push(&mut out, b'S');
                    push(&mut out, b'K');
                } else {
                    push(&mut out, b'S');
                }
            }
            b'x' => {
                push(&mut out, b'K');
                push(&mut out, b'S');
            }
            b'0'..=b'9' => push(&mut out, c),
            _ => {}
        }
    }
    // Every code pushed is an ASCII upper-case letter or a digit.
    String::from_utf8_lossy(&out).into_owned()
}

/// The number of single-letter edits (insert, delete, replace) between `a` and `b`, or `None`
/// when it is more than `max`. Letters are `char`s; an input over [`MAX_DISTANCE_CHARS`] letters
/// is never close to anything (`None`).
#[must_use]
pub fn edit_distance_bounded(a: &str, b: &str, max: usize) -> Option<usize> {
    let a: Vec<char> = a.chars().take(MAX_DISTANCE_CHARS + 1).collect();
    let b: Vec<char> = b.chars().take(MAX_DISTANCE_CHARS + 1).collect();
    if a.len() > MAX_DISTANCE_CHARS || b.len() > MAX_DISTANCE_CHARS {
        return None;
    }
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    if a.is_empty() || b.is_empty() {
        let n = a.len().max(b.len());
        return (n <= max).then_some(n);
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, &ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        let mut row_min = cur[0];
        for (j, &cb) in b.iter().enumerate() {
            let sub = prev[j] + usize::from(ca != cb);
            let del = prev[j + 1] + 1;
            let ins = cur[j] + 1;
            cur[j + 1] = sub.min(del).min(ins);
            row_min = row_min.min(cur[j + 1]);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let d = prev[b.len()];
    (d <= max).then_some(d)
}

#[cfg(test)]
#[path = "symbols_tokens_test.rs"]
mod symbols_tokens_test;
