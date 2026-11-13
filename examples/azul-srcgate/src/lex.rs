//! A small Rust lexer for the gates: comments left out, string literals resolved, the
//! `#[cfg(test)]` items found and blanked.
//!
//! It reads source text, it does not parse Rust: enough for "which string literals does the
//! shipped code hold", "which calls does it make", not a compiler. Each gate states what it
//! relies on. Ported from the backend's `azlin-proto` `pins::source` (the azul-apps repository),
//! which has been reading that tree's code the same way; changes here: a `#[test]` or `#[bench]`
//! function outside a test module is a test item too, and the lexer carries no `Item` or `use`
//! walkers (a gate here asks for calls, names and strings: [`crate::find`]).

use std::ops::Range;

/// One token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tok {
    /// An identifier or keyword; also a number's text.
    Ident(String),
    /// A string literal's value (escapes resolved, a line continuation skipped as Rust does).
    Str(String),
    /// One punctuation character.
    Punct(char),
}

impl Tok {
    /// Whether this is the identifier `s`.
    #[must_use]
    pub fn is_ident(&self, s: &str) -> bool {
        matches!(self, Tok::Ident(i) if i == s)
    }

    /// Whether this is the punctuation character `c`.
    #[must_use]
    pub fn is_punct(&self, c: char) -> bool {
        matches!(self, Tok::Punct(p) if *p == c)
    }

    /// The identifier's text, when this is one.
    #[must_use]
    pub fn ident(&self) -> Option<&str> {
        match self {
            Tok::Ident(i) => Some(i),
            _ => None,
        }
    }
}

fn is_ident_start(c: char) -> bool {
    c.is_alphabetic() || c == '_'
}

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The tokens of Rust source `src`; comments (doc comments too) are left out.
#[must_use]
pub fn lex(src: &str) -> Vec<Tok> {
    lex_spans(src).0.into_iter().map(|(t, _)| t).collect()
}

/// A token and the byte range it came from.
pub type Spanned = (Tok, Range<usize>);

/// The tokens of `src` with the byte range each came from, and the byte ranges of its comments
/// (doc comments too). A char literal is no token; its bytes belong to neither list.
#[must_use]
pub fn lex_spans(src: &str) -> (Vec<Spanned>, Vec<Range<usize>>) {
    let c: Vec<char> = src.chars().collect();
    // the byte offset of each char, and of the end
    let mut byte = Vec::with_capacity(c.len() + 1);
    let mut b = 0;
    for ch in &c {
        byte.push(b);
        b += ch.len_utf8();
    }
    byte.push(b);
    let n = c.len();
    let at = |i: usize| c.get(i).copied().unwrap_or('\0');
    let span = |a: usize, z: usize| byte[a.min(n)]..byte[z.min(n)];
    let mut out = Vec::new();
    let mut comments = Vec::new();
    let mut i = 0;
    while i < n {
        let ch = c[i];
        let from = i;
        if ch.is_whitespace() {
            i += 1;
        } else if ch == '/' && at(i + 1) == '/' {
            while i < n && c[i] != '\n' {
                i += 1;
            }
            comments.push(span(from, i));
        } else if ch == '/' && at(i + 1) == '*' {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if c[i] == '/' && at(i + 1) == '*' {
                    depth += 1;
                    i += 2;
                } else if c[i] == '*' && at(i + 1) == '/' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            comments.push(span(from, i));
        } else if let Some((value, next)) = raw_string(&c, i) {
            out.push((Tok::Str(value), span(from, next)));
            i = next;
        } else if ch == '"' || (ch == 'b' && at(i + 1) == '"') || (ch == 'c' && at(i + 1) == '"') {
            let (value, next) = string(&c, if ch == '"' { i } else { i + 1 });
            out.push((Tok::Str(value), span(from, next)));
            i = next;
        } else if ch == '\'' {
            if at(i + 1) == '\\' {
                // an escaped char literal: '\n', '\'', '\u{..}'
                i += 3;
                while i < n && c[i] != '\'' {
                    i += 1;
                }
                i += 1;
            } else if at(i + 2) == '\'' {
                i += 3;
            } else {
                // a lifetime or a label
                i += 1;
                while i < n && is_ident_char(c[i]) {
                    i += 1;
                }
            }
        } else if ch == 'r' && at(i + 1) == '#' && is_ident_start(at(i + 2)) {
            // a raw identifier
            let start = i + 2;
            i = start;
            while i < n && is_ident_char(c[i]) {
                i += 1;
            }
            out.push((Tok::Ident(c[start..i].iter().collect()), span(from, i)));
        } else if is_ident_char(ch) {
            while i < n && is_ident_char(c[i]) {
                i += 1;
            }
            out.push((Tok::Ident(c[from..i].iter().collect()), span(from, i)));
        } else {
            out.push((Tok::Punct(ch), span(from, i + 1)));
            i += 1;
        }
    }
    (out, comments)
}

/// A raw string at `i` (`r"…"`, `r#"…"#`, `br"…"`, `cr"…"`): its value and the index after it.
fn raw_string(c: &[char], i: usize) -> Option<(String, usize)> {
    let mut j = i;
    if matches!(c.get(j), Some('b' | 'c')) {
        j += 1;
    }
    if c.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while c.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    if c.get(j) != Some(&'"') {
        return None;
    }
    j += 1;
    let start = j;
    while j < c.len() {
        if c[j] == '"' && (1..=hashes).all(|k| c.get(j + k) == Some(&'#')) {
            return Some((c[start..j].iter().collect(), j + 1 + hashes));
        }
        j += 1;
    }
    Some((c[start..].iter().collect(), c.len()))
}

/// A string literal whose opening quote is at `i`: its value and the index after it.
fn string(c: &[char], i: usize) -> (String, usize) {
    let mut s = String::new();
    let mut j = i + 1;
    while j < c.len() && c[j] != '"' {
        if c[j] == '\\' && j + 1 < c.len() {
            match c[j + 1] {
                '\n' | '\r' => {
                    // a line continuation: the line end and the next line's indentation go
                    j += 2;
                    while j < c.len() && c[j].is_whitespace() {
                        j += 1;
                    }
                    continue;
                }
                'n' => s.push('\n'),
                't' => s.push('\t'),
                'r' => s.push('\r'),
                '0' => s.push('\0'),
                '\\' => s.push('\\'),
                '"' => s.push('"'),
                '\'' => s.push('\''),
                other => {
                    s.push('\\');
                    s.push(other);
                }
            }
            j += 2;
        } else {
            s.push(c[j]);
            j += 1;
        }
    }
    (s, j + 1)
}

/// The index after the attribute `#[…]` (or `#![…]`) that starts at `i`, if one does.
pub(crate) fn attribute_end(t: &[Tok], i: usize) -> Option<usize> {
    if !t.get(i)?.is_punct('#') {
        return None;
    }
    let mut j = i + 1;
    if t.get(j)?.is_punct('!') {
        j += 1;
    }
    if !t.get(j)?.is_punct('[') {
        return None;
    }
    let mut depth = 0;
    while j < t.len() {
        if t[j].is_punct('[') {
            depth += 1;
        } else if t[j].is_punct(']') {
            depth -= 1;
            if depth == 0 {
                return Some(j + 1);
            }
        }
        j += 1;
    }
    Some(t.len())
}

/// Whether the attribute tokens `attr` (`#`, `[`, …, `]`) compile their item into tests only:
/// `cfg(test)`, or `cfg(all(…))` naming `test` without `not`; and a `#[test]` or `#[bench]`
/// function. `cfg(any(test, …))` does not.
fn tests_only(attr: &[Tok]) -> bool {
    let body: Vec<&Tok> = attr
        .iter()
        .skip_while(|t| t.is_punct('#') || t.is_punct('!') || t.is_punct('['))
        .collect();
    if body.len() == 2 && (body[0].is_ident("test") || body[0].is_ident("bench")) {
        // `test` and the closing bracket
        return body[1].is_punct(']');
    }
    if !body.first().is_some_and(|t| t.is_ident("cfg")) {
        return false;
    }
    let names = |s: &str| body.iter().any(|t| t.is_ident(s));
    let second = body.get(2).and_then(|t| t.ident());
    match second {
        Some("test") => body.get(3).is_some_and(|t| t.is_punct(')')),
        Some("all") => names("test") && !names("not"),
        _ => false,
    }
}

/// The index after the item that starts at `i`: its `;` or the `}` that closes its body, outside
/// any parentheses or brackets.
#[must_use]
pub fn item_end(t: &[Tok], i: usize) -> usize {
    let (mut braces, mut other) = (0i32, 0i32);
    let mut j = i;
    while j < t.len() {
        match &t[j] {
            Tok::Punct('(' | '[') => other += 1,
            Tok::Punct(')' | ']') => other -= 1,
            Tok::Punct('{') => braces += 1,
            Tok::Punct('}') => {
                braces -= 1;
                if braces < 0 {
                    // the block around it closes (a field's or an arm's item): it ends before
                    return j;
                }
                if braces == 0 && other <= 0 {
                    return j + 1;
                }
            }
            Tok::Punct(';') if braces <= 0 && other <= 0 => return j + 1,
            _ => {}
        }
        j += 1;
    }
    t.len()
}

/// The token ranges of `t`'s test items, each from its first attribute through its end (all of
/// `t` for a file under `#![cfg(test)]`).
#[must_use]
pub fn test_items(t: &[Tok]) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < t.len() {
        if let Some(end) = attribute_end(t, i) {
            if tests_only(&t[i..end]) {
                if t.get(i + 1).is_some_and(|x| x.is_punct('!')) {
                    return std::iter::once(0..t.len()).collect();
                }
                // the item's other attributes, then the item
                let mut j = end;
                while let Some(e) = attribute_end(t, j) {
                    j = e;
                }
                let stop = item_end(t, j).max(i + 1);
                out.push(i..stop);
                i = stop;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// `t` without its test items (and with nothing at all for a file under `#![cfg(test)]`).
#[must_use]
pub fn without_tests(t: &[Tok]) -> Vec<Tok> {
    let mut cut = vec![false; t.len()];
    for r in test_items(t) {
        for gone in cut.iter_mut().take(r.end).skip(r.start) {
            *gone = true;
        }
    }
    t.iter()
        .zip(cut)
        .filter(|(_, gone)| !gone)
        .map(|(x, _)| x.clone())
        .collect()
}

/// `src` as a gate reads it: its test items and its comments blanked, every other byte where it
/// was and every line break kept, so a line number is the file's.
///
/// # Panics
///
/// Never on source the lexer reads; a blanking that would cut a char in two is a bug here.
#[must_use]
pub fn code(src: &str) -> String {
    let (toks, comments) = lex_spans(src);
    let plain: Vec<Tok> = toks.iter().map(|(t, _)| t.clone()).collect();
    let mut blank: Vec<Range<usize>> = comments;
    for r in test_items(&plain) {
        if r.start < r.end {
            let from = toks[r.start].1.start;
            let to = toks[r.end - 1].1.end;
            blank.push(from..to);
        }
    }
    let mut b = src.as_bytes().to_vec();
    for r in blank {
        for x in b.iter_mut().take(r.end).skip(r.start) {
            if *x != b'\n' {
                *x = b' ';
            }
        }
    }
    // whole chars were blanked (ranges start and end at char boundaries), so this is UTF-8
    String::from_utf8(b).unwrap_or_else(|e| panic!("code(): blanking broke a char: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(t: &[Tok]) -> Vec<String> {
        t.iter()
            .filter_map(|x| match x {
                Tok::Str(s) => Some(s.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_lexer_reads_strings_and_skips_comments_chars_and_lifetimes() {
        let src = r##"
            // "not a string"
            /* "nor this" /* nested */ */
            fn f<'a>(x: &'a str) -> char {
                let _ = "select a \
                         from b";
                let _ = r#"raw "quoted""#;
                let _ = b"bytes";
                let _ = c"cstr";
                let _ = "esc \"q\" \\ {x}";
                '{'
            }
        "##;
        let t = lex(src);
        assert_eq!(
            strs(&t),
            vec![
                "select a from b".to_string(),
                "raw \"quoted\"".to_string(),
                "bytes".to_string(),
                "cstr".to_string(),
                "esc \"q\" \\ {x}".to_string(),
            ]
        );
        // the char literal '{' is no brace: the function is one item
        let opens = t.iter().filter(|x| x.is_punct('{')).count();
        let closes = t.iter().filter(|x| x.is_punct('}')).count();
        assert_eq!((opens, closes), (1, 1));
    }

    #[test]
    fn test_items_are_left_out_and_any_test_is_not() {
        let src = r#"
            const A: &str = "a";
            #[cfg(test)]
            mod tests { const B: &str = "b"; }
            #[cfg(all(test, feature = "dev"))]
            #[allow(dead_code)]
            fn c() { let _ = "c"; }
            #[cfg(any(test, feature = "pins"))]
            pub mod pins;
            #[cfg(not(test))]
            const D: [u8; 2] = [0; 2];
            #[cfg(test)]
            const E: [u8; 2] = [0; 2];
            #[test]
            fn t() { let _ = "t"; }
            const F: &str = "f";
        "#;
        let t = without_tests(&lex(src));
        // "pins" is the kept attribute's feature name
        assert_eq!(
            strs(&t),
            vec!["a".to_string(), "pins".to_string(), "f".to_string()]
        );
        assert!(t.iter().any(|x| x.is_ident("pins")), "any(test, ..) stays");
        assert!(t.iter().any(|x| x.is_ident("D")), "not(test) stays");
        assert!(
            !t.iter().any(|x| x.is_ident("E")),
            "a const with a `;` in its type"
        );
        assert!(!t.iter().any(|x| x.is_ident("t")), "a #[test] function");
        assert!(without_tests(&lex("#![cfg(test)]\nconst X: u8 = 1;")).is_empty());
    }

    /// The gates' reading: test items and comments are blanked by the lexer, wherever they are,
    /// line breaks kept; the code after a test module, a test item at the top and
    /// `cfg(all(test, ..))` are handled; a brace in a string or a char ends nothing.
    #[test]
    fn code_blanks_test_items_and_comments_and_keeps_every_line() {
        let src = "#[cfg(test)]\nstatic X: u8 = 0;\nfn a() { b(); } // note\n/* block\n */\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = \"}\"; let _ = '}'; c(); }\n}\nfn d() { e(); }\n#[cfg(all(test, unix))]\nfn f() { g(); }\n#[cfg(any(test, feature = \"x\"))]\nfn h() { i(); }\n";
        let out = code(src);
        assert_eq!(out.lines().count(), src.lines().count());
        let lines: Vec<&str> = out.lines().map(str::trim).collect();
        assert_eq!(lines[0], "", "the test static's attribute");
        assert_eq!(lines[1], "", "the test static");
        assert_eq!(lines[2], "fn a() { b(); }", "the comment goes");
        assert_eq!((lines[3], lines[4]), ("", ""), "the block comment");
        assert!(lines[5..9].iter().all(|l| l.is_empty()), "{lines:?}");
        assert_eq!(lines[9], "fn d() { e(); }", "read after the test module");
        assert_eq!((lines[10], lines[11]), ("", ""), "cfg(all(test, ..))");
        assert_eq!(
            lines[13], "fn h() { i(); }",
            "any(test, ..) is compiled without tests"
        );
        // a non-ASCII char in a blanked comment keeps the text UTF-8 and the lines
        assert_eq!(
            code("fn a() {} // grüße\nfn b() {}\n").lines().nth(1),
            Some("fn b() {}")
        );
        assert_eq!(code("#![cfg(test)]\nfn x() {}\n").trim(), "");
    }

    #[test]
    fn a_test_function_outside_a_test_module_is_blanked_too() {
        let out = code("fn a() {}\n#[test]\nfn t() {\n    todo!();\n}\nfn b() {}\n");
        assert!(!out.contains("todo"), "{out}");
        assert!(out.contains("fn a()") && out.contains("fn b()"), "{out}");
    }
}
