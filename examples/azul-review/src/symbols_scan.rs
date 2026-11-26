//! The scanners: the definitions a file holds, without a parser. A byte lexer that skips comments,
//! strings, raw strings, character literals and (for C) preprocessor lines turns the file into a
//! flat list of identifier and punctuation tokens with their lines; a small state machine per
//! language family reads the tokens for the keywords that start a definition (`struct`, `fn`,
//! `class`, `def`, `namespace` ...), the name after it, how far the definition reaches (to the
//! brace that closes it, the semicolon that ends it, the dedent that leaves it) and the type or class
//! it belongs to (the `impl` header, the class body, the namespace).
//!
//! Rough on purpose: no types, no generics, no overload resolution, no macro expansion. A macro
//! DEFINITION is skipped (its body is a template, not code), a macro INVOCATION is read as the
//! code it holds, an item inside a function body is not recorded, and what a scanner cannot tell is
//! left out, never guessed. Every function here is total: hostile input (an unterminated string, a
//! hundred thousand open braces, bytes that are not text) gives what was found before it, and costs
//! time linear in the input.
//!
//! [`scan_code`] is the entry: it counts the lines, marks a test file and dispatches on the
//! language. The language families: Rust and C/C++ (token state machines), JavaScript and TypeScript, Go,
//! Java, Kotlin, C# and Swift (one braced-language state machine with a table of the keywords of each),
//! Python (a line scanner that tracks indentation and triple-quoted strings) and shell (a line scanner).

use crate::{
    symbols::{Kind, RawDef, Scanned, Vis},
    symbols_source::{is_test_path, Lang},
};

/// What a scan may produce.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanCaps {
    /// The most definitions kept of one file; the scan stops there.
    pub max_defs: usize,
    /// The most imports kept of one file.
    pub max_imports: usize,
}

impl Default for ScanCaps {
    fn default() -> Self {
        Self { max_defs: 4_000, max_imports: 2_000 }
    }
}

/// The most bytes a scan reads: token positions are `u32`.
const MAX_SCAN_BYTES: usize = 256 * 1024 * 1024;
/// The longest import path kept.
const MAX_IMPORT_CHARS: usize = 256;

/// Scans one file of language `lang`. `rel` is its workspace path (it tells a test file).
#[must_use]
pub fn scan_code(lang: Lang, rel: &str, bytes: &[u8], caps: &ScanCaps) -> Scanned {
    let bytes = &bytes[..bytes.len().min(MAX_SCAN_BYTES)];
    let (defs, imports) = match lang {
        Lang::Rust => scan_rust(bytes, caps),
        Lang::C | Lang::Cpp => scan_c(bytes, caps),
        Lang::Python => scan_python(bytes, caps),
        Lang::JavaScript | Lang::TypeScript => scan_braced(bytes, caps, Family::Script),
        Lang::Go => scan_braced(bytes, caps, Family::Go),
        Lang::Java => scan_braced(bytes, caps, Family::Java),
        Lang::Kotlin => scan_braced(bytes, caps, Family::Kotlin),
        Lang::CSharp => scan_braced(bytes, caps, Family::CSharp),
        Lang::Swift => scan_braced(bytes, caps, Family::Swift),
        Lang::Shell => scan_shell(bytes, caps),
    };
    Scanned {
        lang: Some(lang),
        size: bytes.len() as u64,
        mtime_ms: 0,
        lines: count_lines(bytes),
        defs,
        imports,
        test_file: is_test_path(rel),
    }
}

/// The number of lines: a last line without a newline counts.
#[must_use]
pub fn count_lines(bytes: &[u8]) -> u32 {
    let newlines = bytes.iter().filter(|&&b| b == b'\n').count();
    let last_open = bytes.last().is_some_and(|&b| b != b'\n');
    u32::try_from(newlines + usize::from(last_open)).unwrap_or(u32::MAX)
}

// ---------------------------------------------------------------------------------------------
// The lexer
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum T {
    Ident,
    /// One punctuation byte (`Tok::c`).
    Punct,
    /// A string, a character or a raw string: its contents are never read.
    Str,
    Num,
    /// A Rust lifetime or label.
    Life,
    /// The end of a preprocessor line.
    Eol,
}

#[derive(Clone, Copy, Debug)]
struct Tok {
    t: T,
    /// The byte of a `Punct`; for an `Ident`, 1 when it was written `r#name`.
    c: u8,
    s: u32,
    e: u32,
    line: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dialect {
    Rust,
    C,
    /// JavaScript and TypeScript: template literals over lines, regular expression literals.
    Script,
    /// Go: raw strings in backticks, runes.
    Go,
    /// Java, Kotlin, C#, Swift: text blocks, verbatim strings, nested comments.
    Jvm,
}

struct Lexer<'a> {
    src: &'a [u8],
    i: usize,
    line: u32,
    toks: Vec<Tok>,
    dialect: Dialect,
    /// Inside a preprocessor line (C): a newline ends it unless escaped.
    in_directive: bool,
    /// Nothing but blanks since the last newline (C: a `#` here starts a directive).
    line_start: bool,
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$' || b >= 0x80
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

fn newlines_in(bytes: &[u8]) -> u32 {
    u32::try_from(bytes.iter().filter(|&&b| b == b'\n').count()).unwrap_or(u32::MAX)
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u8], dialect: Dialect) -> Self {
        Self {
            src,
            i: if src.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 },
            line: 1,
            toks: Vec::with_capacity((src.len() / 5).min(1 << 20) + 16),
            dialect,
            in_directive: false,
            line_start: true,
        }
    }

    fn at(&self, i: usize) -> u8 {
        self.src.get(i).copied().unwrap_or(0)
    }

    fn push(&mut self, t: T, c: u8, s: usize, e: usize, line: u32) {
        self.toks.push(Tok {
            t,
            c,
            s: u32::try_from(s).unwrap_or(u32::MAX),
            e: u32::try_from(e).unwrap_or(u32::MAX),
            line,
        });
        self.line_start = false;
    }

    /// Skips the quoted literal that opens at `self.i` (a quote byte `q`), with backslash escapes. A
    /// C literal ends at an unescaped newline; a Rust string may run over lines.
    fn skip_quoted(&mut self, q: u8, stop_at_newline: bool) {
        let start = self.i;
        let n = self.src.len();
        let mut j = self.i + 1;
        while j < n {
            let b = self.src[j];
            if b == b'\\' {
                j += 2;
            } else if b == q {
                j += 1;
                break;
            } else if b == b'\n' && stop_at_newline {
                break;
            } else {
                j += 1;
            }
        }
        let end = j.min(n);
        self.line = self.line.saturating_add(newlines_in(&self.src[start..end]));
        self.i = end;
    }

    /// Skips a raw string whose opening quote is at `quote_at` and which closes with a quote and
    /// `hashes` hashes (Rust); returns false when there is no closing one (the rest of the file is
    /// the string).
    fn skip_raw_rust(&mut self, quote_at: usize, hashes: usize) {
        let n = self.src.len();
        let mut j = quote_at + 1;
        let mut end = n;
        while j < n {
            if self.src[j] == b'"' && (1..=hashes).all(|k| self.at(j + k) == b'#') {
                end = j + 1 + hashes;
                break;
            }
            j += 1;
        }
        let end = end.min(n);
        self.line = self.line.saturating_add(newlines_in(&self.src[self.i..end]));
        self.i = end;
    }

    /// Skips a C++ raw string `R"delim( ... )delim"` whose opening quote is at `quote_at`.
    fn skip_raw_c(&mut self, quote_at: usize) -> bool {
        let n = self.src.len();
        let mut j = quote_at + 1;
        while j < n && j - quote_at <= 17 && self.src[j] != b'(' && !matches!(self.src[j], b' ' | b')' | b'\\' | b'\n') {
            j += 1;
        }
        if self.at(j) != b'(' {
            return false;
        }
        let delim = &self.src[quote_at + 1..j];
        let mut k = j + 1;
        let mut end = n;
        while k < n {
            if self.src[k] == b')' && self.src[k + 1..].starts_with(delim) && self.at(k + 1 + delim.len()) == b'"' {
                end = k + delim.len() + 2;
                break;
            }
            k += 1;
        }
        let end = end.min(n);
        self.line = self.line.saturating_add(newlines_in(&self.src[self.i..end]));
        self.i = end;
        true
    }

    fn skip_block_comment(&mut self, nested: bool) {
        let n = self.src.len();
        let start = self.i;
        let mut j = self.i + 2;
        let mut depth = 1u32;
        while j < n {
            if self.src[j] == b'*' && self.at(j + 1) == b'/' {
                depth -= 1;
                j += 2;
                if depth == 0 {
                    break;
                }
            } else if nested && self.src[j] == b'/' && self.at(j + 1) == b'*' {
                depth += 1;
                j += 2;
            } else {
                j += 1;
            }
        }
        let end = j.min(n);
        self.line = self.line.saturating_add(newlines_in(&self.src[start..end]));
        self.i = end;
    }

    /// A text block `"""..."""` (Java, Kotlin, C#, Swift): over lines, to the closing three quotes.
    fn skip_text_block(&mut self) {
        let start = self.i;
        let n = self.src.len();
        let mut j = self.i + 3;
        while j < n {
            if self.src[j] == b'\\' {
                j += 2;
            } else if self.src[j] == b'"' && self.at(j + 1) == b'"' && self.at(j + 2) == b'"' {
                j += 3;
                break;
            } else {
                j += 1;
            }
        }
        let end = j.min(n);
        self.line = self.line.saturating_add(newlines_in(&self.src[start..end]));
        self.i = end;
    }

    /// A C# verbatim string `@"..."`: a doubled quote is a quote.
    fn skip_verbatim(&mut self) {
        let start = self.i;
        let n = self.src.len();
        let mut j = self.i + 2;
        while j < n {
            if self.src[j] == b'"' {
                if self.at(j + 1) == b'"' {
                    j += 2;
                    continue;
                }
                j += 1;
                break;
            }
            j += 1;
        }
        let end = j.min(n);
        self.line = self.line.saturating_add(newlines_in(&self.src[start..end]));
        self.i = end;
    }

    /// Whether a `/` here starts a regular expression (JavaScript): after an operator, an opening bracket,
    /// a separator or a keyword that takes a value, not after a name, a number, a string or a closing bracket.
    fn regex_allowed(&self) -> bool {
        match self.toks.last() {
            None => true,
            Some(t) => match t.t {
                T::Punct => matches!(t.c, b'(' | b',' | b'=' | b':' | b'[' | b'!' | b'&' | b'|' | b'?' | b'{' | b'}' | b';' | b'+' | b'-' | b'*' | b'%' | b'<' | b'>' | b'~' | b'^'),
                T::Ident => matches!(
                    &self.src[t.s as usize..(t.e as usize).min(self.src.len())],
                    b"return" | b"typeof" | b"instanceof" | b"in" | b"of" | b"new" | b"delete" | b"void" | b"throw" | b"case" | b"do" | b"else" | b"yield" | b"await"
                ),
                _ => false,
            },
        }
    }

    /// A regular expression literal: to the closing `/` on the same line, brackets and escapes respected,
    /// then its flags. When there is none on the line the `/` is a division.
    fn lex_regex(&mut self) {
        let start = self.i;
        let line = self.line;
        let n = self.src.len();
        let mut j = self.i + 1;
        let mut in_class = false;
        while j < n {
            match self.src[j] {
                b'\n' => break,
                b'\\' => j += 1,
                b'[' => in_class = true,
                b']' => in_class = false,
                b'/' if !in_class => {
                    j += 1;
                    while j < n && self.src[j].is_ascii_alphabetic() {
                        j += 1;
                    }
                    self.i = j;
                    self.push(T::Str, 0, start, j, line);
                    return;
                }
                _ => {}
            }
            j += 1;
        }
        self.i += 1;
        self.push(T::Punct, b'/', start, start + 1, line);
    }

    fn lex_number(&mut self) {
        let start = self.i;
        let line = self.line;
        let mut j = self.i;
        while j < self.src.len() {
            let b = self.src[j];
            let part_of_number = b.is_ascii_alphanumeric()
                || b == b'_'
                || (b == b'.' && self.at(j + 1).is_ascii_digit())
                || (self.dialect == Dialect::C && b == b'\'' && self.at(j + 1).is_ascii_alphanumeric());
            if !part_of_number {
                break;
            }
            j += 1;
        }
        self.i = j;
        self.push(T::Num, 0, start, j, line);
    }

    /// A Rust `'`: a lifetime, a label or a character literal.
    fn lex_rust_quote(&mut self) {
        let start = self.i;
        let line = self.line;
        let n = self.src.len();
        let next = self.at(self.i + 1);
        if next == b'\\' {
            // '\n', '\'', '\u{..}'
            let mut j = self.i + 2;
            while j < n && self.src[j] != b'\'' && self.src[j] != b'\n' {
                if self.src[j] == b'\\' {
                    j += 1;
                }
                j += 1;
            }
            let end = (j + 1).min(n);
            self.i = end;
            self.push(T::Str, 0, start, end, line);
            return;
        }
        // The length of the one character after the quote.
        let len = match next {
            0x00..=0x7F => 1,
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            _ => 4,
        };
        if next != 0 && self.at(self.i + 1 + len) == b'\'' {
            let end = self.i + 2 + len;
            self.i = end;
            self.push(T::Str, 0, start, end, line);
        } else {
            // A lifetime or a label: the quote and the name.
            let mut j = self.i + 1;
            while j < n && is_ident_byte(self.src[j]) {
                j += 1;
            }
            self.i = j.max(self.i + 1);
            self.push(T::Life, 0, start, self.i, line);
        }
    }

    fn lex_ident(&mut self) {
        let start = self.i;
        let line = self.line;
        let n = self.src.len();
        let mut j = self.i;
        while j < n && is_ident_byte(self.src[j]) {
            j += 1;
        }
        let word = &self.src[start..j];
        let next = self.at(j);
        match self.dialect {
            Dialect::Rust => {
                if matches!(word, b"r" | b"br" | b"cr") && (next == b'"' || next == b'#') {
                    let mut hashes = 0;
                    while self.at(j + hashes) == b'#' {
                        hashes += 1;
                    }
                    if self.at(j + hashes) == b'"' {
                        self.i = start;
                        self.skip_raw_rust(j + hashes, hashes);
                        self.push(T::Str, 0, start, self.i, line);
                        return;
                    }
                    if word == b"r" && hashes == 1 && is_ident_start(self.at(j + 1)) {
                        // r#name: a raw identifier; the name is what follows the hash.
                        let mut k = j + 1;
                        while k < n && is_ident_byte(self.src[k]) {
                            k += 1;
                        }
                        self.i = k;
                        self.push(T::Ident, 1, j + 1, k, line);
                        return;
                    }
                }
                if matches!(word, b"b" | b"c") && next == b'"' {
                    self.i = j;
                    self.skip_quoted(b'"', false);
                    self.push(T::Str, 0, start, self.i, line);
                    return;
                }
                if word == b"b" && next == b'\'' {
                    self.i = j;
                    self.skip_quoted(b'\'', true);
                    self.push(T::Str, 0, start, self.i, line);
                    return;
                }
            }
            Dialect::C => {
                if matches!(word, b"R" | b"LR" | b"uR" | b"UR" | b"u8R") && next == b'"' {
                    self.i = start;
                    if self.skip_raw_c(j) {
                        self.push(T::Str, 0, start, self.i, line);
                        return;
                    }
                }
                if matches!(word, b"L" | b"u" | b"U" | b"u8") && (next == b'"' || next == b'\'') {
                    self.i = j;
                    self.skip_quoted(next, true);
                    self.push(T::Str, 0, start, self.i, line);
                    return;
                }
            }
            Dialect::Script | Dialect::Go | Dialect::Jvm => {}
        }
        self.i = j;
        self.push(T::Ident, 0, start, j, line);
    }

    /// A preprocessor line: `#define`, `#include` and the others are tokens up to an `Eol`.
    fn lex_directive(&mut self) {
        let line = self.line;
        let hash = self.i;
        self.i += 1;
        self.push(T::Punct, b'#', hash, hash + 1, line);
        while matches!(self.at(self.i), b' ' | b'\t') {
            self.i += 1;
        }
        let name_start = self.i;
        while is_ident_byte(self.at(self.i)) {
            self.i += 1;
        }
        let name = self.src[name_start..self.i].to_vec();
        if self.i > name_start {
            let (s, e) = (name_start, self.i);
            self.push(T::Ident, 0, s, e, line);
        }
        self.in_directive = true;
        if name == b"include" || name == b"include_next" || name == b"import" {
            while matches!(self.at(self.i), b' ' | b'\t') {
                self.i += 1;
            }
            let open = self.at(self.i);
            if open == b'<' || open == b'"' {
                let close = if open == b'<' { b'>' } else { b'"' };
                let s = self.i + 1;
                let mut j = s;
                while j < self.src.len() && self.src[j] != close && self.src[j] != b'\n' {
                    j += 1;
                }
                self.push(T::Str, open, s, j, line);
                self.i = (j + 1).min(self.src.len());
            }
        } else if name != b"define" {
            // Any other directive is skipped to its end.
            self.skip_directive_rest();
        }
    }

    fn skip_directive_rest(&mut self) {
        let n = self.src.len();
        while self.i < n {
            match self.src[self.i] {
                b'\\' if self.at(self.i + 1) == b'\n' || (self.at(self.i + 1) == b'\r' && self.at(self.i + 2) == b'\n') => {
                    self.i += if self.at(self.i + 1) == b'\r' { 3 } else { 2 };
                    self.line = self.line.saturating_add(1);
                }
                b'\n' => break,
                b'/' if self.at(self.i + 1) == b'*' => self.skip_block_comment(false),
                _ => self.i += 1,
            }
        }
    }

    fn run(mut self) -> Vec<Tok> {
        let n = self.src.len();
        while self.i < n {
            let b = self.src[self.i];
            match b {
                b'\n' => {
                    if self.in_directive {
                        let line = self.line;
                        let at = self.i;
                        self.push(T::Eol, 0, at, at, line);
                        self.in_directive = false;
                    }
                    self.line = self.line.saturating_add(1);
                    self.i += 1;
                    self.line_start = true;
                }
                b' ' | b'\t' | b'\r' | 0x0B | 0x0C => self.i += 1,
                b'/' if self.at(self.i + 1) == b'/' => {
                    while self.i < n && self.src[self.i] != b'\n' {
                        // A continued line comment in a directive is rare enough to leave alone.
                        self.i += 1;
                    }
                }
                b'/' if self.at(self.i + 1) == b'*' => self.skip_block_comment(matches!(self.dialect, Dialect::Rust | Dialect::Jvm)),
                b'/' if self.dialect == Dialect::Script && self.regex_allowed() => self.lex_regex(),
                b'`' if matches!(self.dialect, Dialect::Script | Dialect::Go) => {
                    let (start, line) = (self.i, self.line);
                    self.skip_quoted(b'`', false);
                    self.push(T::Str, 0, start, self.i, line);
                }
                b'@' if self.dialect == Dialect::Jvm && self.at(self.i + 1) == b'"' => {
                    let (start, line) = (self.i, self.line);
                    self.skip_verbatim();
                    self.push(T::Str, 0, start, self.i, line);
                }
                b'#' if self.dialect == Dialect::C && self.line_start && !self.in_directive => self.lex_directive(),
                b'"' => {
                    let (start, line) = (self.i, self.line);
                    if self.dialect == Dialect::Jvm && self.at(self.i + 1) == b'"' && self.at(self.i + 2) == b'"' {
                        self.skip_text_block();
                    } else {
                        self.skip_quoted(b'"', self.dialect != Dialect::Rust);
                    }
                    self.push(T::Str, 0, start, self.i, line);
                }
                b'\'' => {
                    if self.dialect == Dialect::Rust {
                        self.lex_rust_quote();
                    } else {
                        let (start, line) = (self.i, self.line);
                        self.skip_quoted(b'\'', true);
                        self.push(T::Str, 0, start, self.i, line);
                    }
                }
                b'\\' if self.in_directive
                    && (self.at(self.i + 1) == b'\n' || (self.at(self.i + 1) == b'\r' && self.at(self.i + 2) == b'\n')) =>
                {
                    self.i += if self.at(self.i + 1) == b'\r' { 3 } else { 2 };
                    self.line = self.line.saturating_add(1);
                }
                b'0'..=b'9' => self.lex_number(),
                _ if is_ident_start(b) => self.lex_ident(),
                0x00..=0x1F | 0x7F => self.i += 1,
                _ => {
                    let (at, line) = (self.i, self.line);
                    self.i += 1;
                    self.push(T::Punct, b, at, at + 1, line);
                }
            }
        }
        if self.in_directive {
            let (line, at) = (self.line, self.src.len());
            self.push(T::Eol, 0, at, at, line);
        }
        self.toks
    }
}

fn lex(src: &[u8], dialect: Dialect) -> Vec<Tok> {
    Lexer::new(src, dialect).run()
}

fn name_of(src: &[u8], t: &Tok) -> String {
    String::from_utf8_lossy(&src[t.s as usize..(t.e as usize).min(src.len())]).into_owned()
}

/// The index of the token that closes the group opened at `open` (`(`, `[` or `{`), or `None`.
/// Every kind of bracket counts, so a group is closed by the bracket that balances it.
fn matching_close(toks: &[Tok], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (k, t) in toks.iter().enumerate().skip(open) {
        if t.t != T::Punct {
            continue;
        }
        match t.c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_punct(toks: &[Tok], i: usize, c: u8) -> bool {
    toks.get(i).is_some_and(|t| t.t == T::Punct && t.c == c)
}

fn is_ident(toks: &[Tok], i: usize) -> bool {
    toks.get(i).is_some_and(|t| t.t == T::Ident)
}

/// Two punctuation tokens that touch (`::`, `->`).
fn adjacent_punct(toks: &[Tok], i: usize, a: u8, b: u8) -> bool {
    is_punct(toks, i, a) && is_punct(toks, i + 1, b) && toks[i].e == toks[i + 1].s
}

// ---------------------------------------------------------------------------------------------
// Rust
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
enum Sk {
    Mod(String),
    Impl(Option<String>),
    Trait(String),
    Fn,
    Other,
}

struct Scope {
    kind: Sk,
    def: Option<usize>,
    test: bool,
}

struct Pending {
    def: Option<usize>,
    kind: Sk,
    paren: u32,
    test: bool,
}

struct Rs<'a> {
    src: &'a [u8],
    toks: Vec<Tok>,
    out: Vec<RawDef>,
    imports: Vec<String>,
    caps: &'a ScanCaps,
    scopes: Vec<Scope>,
    pending: Option<Pending>,
    paren: u32,
    vis: Vis,
    attr_test: bool,
    test_depth: u32,
    fn_depth: u32,
}

fn scan_rust(src: &[u8], caps: &ScanCaps) -> (Vec<RawDef>, Vec<String>) {
    let toks = lex(src, Dialect::Rust);
    let mut rs = Rs {
        src,
        toks,
        out: Vec::new(),
        imports: Vec::new(),
        caps,
        scopes: Vec::new(),
        pending: None,
        paren: 0,
        vis: Vis::Private,
        attr_test: false,
        test_depth: 0,
        fn_depth: 0,
    };
    rs.run();
    (rs.out, rs.imports)
}

impl Rs<'_> {
    fn run(&mut self) {
        let mut i = 0;
        while i < self.toks.len() {
            if self.out.len() >= self.caps.max_defs {
                break;
            }
            let tok = self.toks[i];
            i = match tok.t {
                T::Punct => self.punct(i, tok),
                T::Ident => self.ident(i, tok),
                _ => i + 1,
            };
        }
        // What is still open reaches the end of the file.
        let last = self.toks.last().map_or(1, |t| t.line);
        let open: Vec<usize> = self.scopes.iter().filter_map(|s| s.def).chain(self.pending.as_ref().and_then(|p| p.def)).collect();
        for d in open {
            self.out[d].lines = last.saturating_sub(self.out[d].line) + 1;
        }
    }

    fn punct(&mut self, i: usize, tok: Tok) -> usize {
        match tok.c {
            b'#' => self.attribute(i),
            b'{' => {
                let scope = match self.pending.take() {
                    Some(p) if p.paren == self.paren => Scope { kind: p.kind, def: p.def, test: p.test },
                    other => {
                        self.pending = other;
                        Scope { kind: Sk::Other, def: None, test: false }
                    }
                };
                if matches!(scope.kind, Sk::Fn) {
                    self.fn_depth += 1;
                }
                if scope.test {
                    self.test_depth += 1;
                }
                self.scopes.push(scope);
                self.vis = Vis::Private;
                self.attr_test = false;
                i + 1
            }
            b'}' => {
                if let Some(scope) = self.scopes.pop() {
                    if let Some(d) = scope.def {
                        self.out[d].lines = tok.line.saturating_sub(self.out[d].line) + 1;
                    }
                    if matches!(scope.kind, Sk::Fn) {
                        self.fn_depth = self.fn_depth.saturating_sub(1);
                    }
                    if scope.test {
                        self.test_depth = self.test_depth.saturating_sub(1);
                    }
                }
                self.pending = None;
                self.paren = 0;
                self.vis = Vis::Private;
                self.attr_test = false;
                i + 1
            }
            b';' => {
                if self.pending.as_ref().is_some_and(|p| p.paren == self.paren) {
                    if let Some(p) = self.pending.take() {
                        if let Some(d) = p.def {
                            self.out[d].lines = tok.line.saturating_sub(self.out[d].line) + 1;
                        }
                    }
                }
                if self.paren == 0 {
                    self.vis = Vis::Private;
                    self.attr_test = false;
                }
                i + 1
            }
            b'(' | b'[' => {
                self.paren += 1;
                i + 1
            }
            b')' | b']' => {
                self.paren = self.paren.saturating_sub(1);
                i + 1
            }
            _ => i + 1,
        }
    }

    /// `#[...]` and `#![...]`: whether it marks the next item as a test.
    fn attribute(&mut self, i: usize) -> usize {
        let mut j = i + 1;
        if is_punct(&self.toks, j, b'!') {
            j += 1;
        }
        if !is_punct(&self.toks, j, b'[') {
            return i + 1;
        }
        let Some(close) = matching_close(&self.toks, j) else {
            return self.toks.len();
        };
        if self.is_test_attribute(j + 1, close) {
            self.attr_test = true;
        }
        close + 1
    }

    fn is_test_attribute(&self, from: usize, to: usize) -> bool {
        let words: Vec<&[u8]> = self.toks[from..to]
            .iter()
            .filter(|t| t.t == T::Ident)
            .map(|t| &self.src[t.s as usize..(t.e as usize).min(self.src.len())])
            .collect();
        match words.first() {
            Some(&b"test") => true,
            Some(&b"cfg") => words.contains(&&b"test"[..]) && !words.contains(&&b"not"[..]),
            Some(&b"cfg_attr") | None => false,
            Some(_) => {
                // A path attribute such as `tokio::test`: its last segment, before any `(`.
                let mut last = None;
                let mut k = from;
                while k < to {
                    let t = self.toks[k];
                    if t.t == T::Ident {
                        last = Some(&self.src[t.s as usize..(t.e as usize).min(self.src.len())]);
                    } else if !(t.t == T::Punct && t.c == b':') {
                        break;
                    }
                    k += 1;
                }
                matches!(last, Some(b"test" | b"bench"))
            }
        }
    }

    /// The type, trait or module whose body the next item is directly in.
    fn owner(&self) -> Option<String> {
        match self.scopes.last().map(|s| &s.kind) {
            Some(Sk::Impl(owner)) => owner.clone(),
            Some(Sk::Trait(name) | Sk::Mod(name)) => Some(name.clone()),
            _ => None,
        }
    }

    fn in_impl_or_trait(&self) -> bool {
        matches!(self.scopes.last().map(|s| &s.kind), Some(Sk::Impl(_) | Sk::Trait(_)))
    }

    fn record(&mut self, name: &Tok, kind: Kind, parent: Option<String>) -> Option<usize> {
        let text = name_of(self.src, name);
        if text.is_empty() {
            return None;
        }
        let test = self.attr_test || self.test_depth > 0;
        self.out.push(RawDef {
            name: text,
            kind,
            vis: self.vis,
            test,
            line: name.line,
            lines: 1,
            parent,
        });
        self.vis = Vis::Private;
        self.attr_test = false;
        Some(self.out.len() - 1)
    }

    fn push_import(&mut self, path: &str) {
        if self.imports.len() < self.caps.max_imports && !path.is_empty() && path.len() <= MAX_IMPORT_CHARS {
            self.imports.push(path.to_string());
        }
    }

    fn ident(&mut self, i: usize, tok: Tok) -> usize {
        if tok.c == 1 {
            return i + 1;
        }
        let word = &self.src[tok.s as usize..(tok.e as usize).min(self.src.len())];
        if word == b"pub" {
            self.vis = Vis::Public;
            if is_punct(&self.toks, i + 1, b'(') {
                self.vis = Vis::Restricted;
                return matching_close(&self.toks, i + 1).map_or(self.toks.len(), |k| k + 1);
            }
            return i + 1;
        }
        // An item cannot start while another item's header is open, inside parentheses, or inside a
        // function body.
        if self.pending.is_some() || self.paren > 0 || self.fn_depth > 0 {
            return i + 1;
        }
        match word {
            b"struct" => self.named(i, Kind::Struct),
            b"enum" => self.named(i, Kind::Enum),
            b"union" => {
                if is_ident(&self.toks, i + 1) && (is_punct(&self.toks, i + 2, b'{') || is_punct(&self.toks, i + 2, b'<')) {
                    self.named(i, Kind::Union)
                } else {
                    i + 1
                }
            }
            b"trait" => self.named(i, Kind::Trait),
            b"type" => self.named(i, Kind::Type),
            b"const" => self.const_or_static(i, Kind::Const),
            b"static" => self.const_or_static(i, Kind::Static),
            b"fn" => self.function(i),
            b"mod" => self.module(i),
            b"macro_rules" => self.macro_rules(i),
            b"impl" => self.implementation(i),
            b"use" => {
                let end = self.parse_use(i + 1);
                self.vis = Vis::Private;
                self.attr_test = false;
                end
            }
            b"extern" => {
                if is_ident(&self.toks, i + 1)
                    && &self.src[self.toks[i + 1].s as usize..self.toks[i + 1].e as usize] == b"crate"
                    && is_ident(&self.toks, i + 2)
                {
                    let name = name_of(self.src, &self.toks[i + 2]);
                    self.push_import(&name);
                    return i + 3;
                }
                i + 1
            }
            _ => i + 1,
        }
    }

    /// `struct X`, `enum X`, `trait X`, `type X`, `union X`.
    fn named(&mut self, i: usize, kind: Kind) -> usize {
        if !is_ident(&self.toks, i + 1) {
            return i + 1;
        }
        let name = self.toks[i + 1];
        let parent = self.owner();
        let test = self.attr_test || self.test_depth > 0;
        let scope = if kind == Kind::Trait { Sk::Trait(name_of(self.src, &name)) } else { Sk::Other };
        let def = self.record(&name, kind, parent);
        self.pending = Some(Pending { def, kind: scope, paren: self.paren, test });
        i + 2
    }

    fn const_or_static(&mut self, i: usize, kind: Kind) -> usize {
        // `static mut NAME:`
        let mut at = i + 1;
        if kind == Kind::Static && is_ident(&self.toks, at) && &self.src[self.toks[at].s as usize..self.toks[at].e as usize] == b"mut" {
            at += 1;
        }
        let is_generic_param = i > 0 && (is_punct(&self.toks, i - 1, b'<') || is_punct(&self.toks, i - 1, b','));
        if !is_ident(&self.toks, at) || !is_punct(&self.toks, at + 1, b':') || is_generic_param {
            return i + 1;
        }
        let name = self.toks[at];
        if &self.src[name.s as usize..name.e as usize] == b"_" {
            return i + 1;
        }
        let parent = self.owner();
        let test = self.attr_test || self.test_depth > 0;
        let def = self.record(&name, kind, parent);
        self.pending = Some(Pending { def, kind: Sk::Other, paren: self.paren, test });
        at + 1
    }

    fn function(&mut self, i: usize) -> usize {
        if !is_ident(&self.toks, i + 1) {
            return i + 1;
        }
        let name = self.toks[i + 1];
        let kind = if self.in_impl_or_trait() { Kind::Method } else { Kind::Fn };
        let parent = self.owner();
        let def = self.record(&name, kind, parent);
        self.pending = Some(Pending { def, kind: Sk::Fn, paren: self.paren, test: false });
        i + 2
    }

    fn module(&mut self, i: usize) -> usize {
        if !is_ident(&self.toks, i + 1) {
            return i + 1;
        }
        let name = self.toks[i + 1];
        let text = name_of(self.src, &name);
        let parent = self.owner();
        let test = self.attr_test || self.test_depth > 0;
        let def = self.record(&name, Kind::Module, parent);
        if is_punct(&self.toks, i + 2, b';') {
            self.push_import(&format!("self::{text}"));
            return i + 3;
        }
        self.pending = Some(Pending { def, kind: Sk::Mod(text), paren: self.paren, test });
        i + 2
    }

    fn macro_rules(&mut self, i: usize) -> usize {
        if !is_punct(&self.toks, i + 1, b'!') || !is_ident(&self.toks, i + 2) {
            return i + 1;
        }
        let name = self.toks[i + 2];
        let def = self.record(&name, Kind::Macro, None);
        let open = i + 3;
        if !(is_punct(&self.toks, open, b'{') || is_punct(&self.toks, open, b'(') || is_punct(&self.toks, open, b'[')) {
            return i + 3;
        }
        let end = matching_close(&self.toks, open);
        if let Some(d) = def {
            let last = end.map_or_else(|| self.toks.last().map_or(1, |t| t.line), |k| self.toks[k].line);
            self.out[d].lines = last.saturating_sub(self.out[d].line) + 1;
        }
        end.map_or(self.toks.len(), |k| k + 1)
    }

    /// `impl<..> Trait for Type<..> {` and `impl Type {`: the owner is the last name of the type
    /// after `for`, or of the type itself. The scope opens at the brace the main loop meets next.
    fn implementation(&mut self, i: usize) -> usize {
        let mut j = i + 1;
        let mut angle = 0i32;
        let mut owner: Option<usize> = None;
        while j < self.toks.len() {
            let t = self.toks[j];
            match t.t {
                T::Punct => match t.c {
                    b'<' => angle += 1,
                    b'>' => {
                        if angle > 0 && !(j > 0 && adjacent_punct(&self.toks, j - 1, b'-', b'>')) {
                            angle -= 1;
                        }
                    }
                    b'{' | b';' if angle <= 0 => break,
                    b'(' | b'[' if angle == 0 => {
                        owner = None;
                        match matching_close(&self.toks, j) {
                            Some(k) => j = k,
                            None => {
                                j = self.toks.len();
                                break;
                            }
                        }
                    }
                    _ => {}
                },
                T::Ident if angle == 0 => {
                    let w = &self.src[t.s as usize..(t.e as usize).min(self.src.len())];
                    if w == b"where" {
                        break;
                    }
                    if w == b"for" {
                        owner = None;
                    } else if !matches!(w, b"dyn" | b"mut" | b"const" | b"unsafe" | b"impl" | b"crate" | b"super" | b"self") {
                        owner = Some(j);
                    }
                }
                _ => {}
            }
            j += 1;
        }
        let owner = owner.map(|k| name_of(self.src, &self.toks[k]));
        let test = self.attr_test || self.test_depth > 0;
        self.pending = Some(Pending { def: None, kind: Sk::Impl(owner), paren: self.paren, test });
        self.attr_test = false;
        self.vis = Vis::Private;
        j.max(i + 1)
    }

    /// `use` trees: every leaf path, groups expanded, `self` in a group meaning the group's prefix.
    fn parse_use(&mut self, start: usize) -> usize {
        let mut j = start;
        let mut cur = String::new();
        let mut stack: Vec<String> = Vec::new();
        let n = self.toks.len();
        while j < n {
            let t = self.toks[j];
            match t.t {
                T::Ident => {
                    let w = name_of(self.src, &t);
                    if w == "as" {
                        j += 1;
                        if is_ident(&self.toks, j) {
                            j += 1;
                        }
                        continue;
                    }
                    if w == "self" && cur.ends_with("::") && stack.last().is_some_and(|p| *p == cur) {
                        let path = cur.trim_end_matches(':').to_string();
                        self.push_import(&path);
                    } else {
                        cur.push_str(&w);
                    }
                }
                T::Punct => match t.c {
                    b':' => {
                        cur.push(':');
                    }
                    b'{' => stack.push(cur.clone()),
                    b',' | b'}' => {
                        let prefix = stack.last().cloned().unwrap_or_default();
                        if cur != prefix && !cur.is_empty() {
                            let path = cur.trim_end_matches(':').to_string();
                            self.push_import(&path);
                        }
                        if t.c == b'}' {
                            stack.pop();
                        }
                        cur = stack.last().cloned().unwrap_or_default();
                    }
                    b'*' => cur.push('*'),
                    b';' if stack.is_empty() => {
                        if !cur.is_empty() {
                            let path = cur.trim_end_matches(':').to_string();
                            self.push_import(&path);
                        }
                        return j + 1;
                    }
                    _ => {}
                },
                _ => {}
            }
            j += 1;
        }
        n
    }
}

// ---------------------------------------------------------------------------------------------
// C and C++
// ---------------------------------------------------------------------------------------------

#[derive(Clone)]
enum Ck {
    Namespace(String),
    Class(String),
    Enum,
    ExternC,
    Fn,
    Other,
}

struct CScope {
    kind: Ck,
    def: Option<usize>,
}

struct CPending {
    kind: Ck,
    def: Option<usize>,
}

struct Cs<'a> {
    src: &'a [u8],
    toks: Vec<Tok>,
    out: Vec<RawDef>,
    imports: Vec<String>,
    caps: &'a ScanCaps,
    scopes: Vec<CScope>,
    pending: Option<CPending>,
    paren: u32,
    stmt_start: usize,
    typedef: Option<usize>,
    seen_eq: bool,
}

fn scan_c(src: &[u8], caps: &ScanCaps) -> (Vec<RawDef>, Vec<String>) {
    let toks = lex(src, Dialect::C);
    let mut cs = Cs {
        src,
        toks,
        out: Vec::new(),
        imports: Vec::new(),
        caps,
        scopes: Vec::new(),
        pending: None,
        paren: 0,
        stmt_start: 0,
        typedef: None,
        seen_eq: false,
    };
    cs.run();
    (cs.out, cs.imports)
}

static C_NOT_A_FUNCTION: [&str; 40] = [
    "if", "for", "while", "switch", "return", "sizeof", "catch", "defined", "alignof", "decltype", "static_assert", "typeof",
    "__attribute__", "__declspec", "throw", "new", "delete", "do", "else", "case", "int", "void", "char", "short", "long", "float",
    "double", "signed", "unsigned", "bool", "auto", "operator", "noexcept", "alignas", "_Alignas", "_Static_assert", "__typeof__",
    "asm", "__asm__", "using",
];

impl Cs<'_> {
    fn word(&self, i: usize) -> &[u8] {
        self.toks
            .get(i)
            .filter(|t| t.t == T::Ident)
            .map_or(&[][..], |t| &self.src[t.s as usize..(t.e as usize).min(self.src.len())])
    }

    fn decl_ok(&self) -> bool {
        matches!(self.scopes.last().map(|s| &s.kind), None | Some(Ck::Namespace(_) | Ck::Class(_) | Ck::ExternC))
    }

    fn owner(&self) -> Option<String> {
        match self.scopes.last().map(|s| &s.kind) {
            Some(Ck::Namespace(name) | Ck::Class(name)) if !name.is_empty() => Some(name.clone()),
            _ => None,
        }
    }

    fn in_class(&self) -> bool {
        matches!(self.scopes.last().map(|s| &s.kind), Some(Ck::Class(_)))
    }

    fn record(&mut self, name: &Tok, kind: Kind, parent: Option<String>) -> Option<usize> {
        let text = name_of(self.src, name);
        if text.is_empty() {
            return None;
        }
        self.out.push(RawDef {
            name: text,
            kind,
            vis: Vis::Public,
            test: false,
            line: name.line,
            lines: 1,
            parent,
        });
        Some(self.out.len() - 1)
    }

    fn push_import(&mut self, path: &str) {
        if self.imports.len() < self.caps.max_imports && !path.is_empty() && path.len() <= MAX_IMPORT_CHARS {
            self.imports.push(path.to_string());
        }
    }

    fn run(&mut self) {
        let mut i = 0;
        while i < self.toks.len() {
            if self.out.len() >= self.caps.max_defs {
                break;
            }
            let tok = self.toks[i];
            i = match tok.t {
                T::Punct => self.punct(i, tok),
                T::Ident => self.ident(i),
                T::Eol => {
                    self.stmt_start = i + 1;
                    i + 1
                }
                _ => i + 1,
            };
        }
        let last = self.toks.last().map_or(1, |t| t.line);
        let open: Vec<usize> = self.scopes.iter().filter_map(|s| s.def).chain(self.pending.as_ref().and_then(|p| p.def)).collect();
        for d in open {
            self.out[d].lines = last.saturating_sub(self.out[d].line) + 1;
        }
    }

    fn punct(&mut self, i: usize, tok: Tok) -> usize {
        match tok.c {
            b'#' => self.directive(i),
            b'{' => {
                let scope = match self.pending.take() {
                    Some(p) => CScope { kind: p.kind, def: p.def },
                    None => CScope { kind: Ck::Other, def: None },
                };
                self.scopes.push(scope);
                self.stmt_start = i + 1;
                self.seen_eq = false;
                i + 1
            }
            b'}' => {
                if let Some(scope) = self.scopes.pop() {
                    if let Some(d) = scope.def {
                        self.out[d].lines = tok.line.saturating_sub(self.out[d].line) + 1;
                    }
                }
                self.pending = None;
                self.paren = 0;
                if self.typedef.is_none() {
                    self.stmt_start = i + 1;
                }
                self.seen_eq = false;
                i + 1
            }
            b';' => {
                if self.paren == 0 {
                    if let Some(depth) = self.typedef {
                        if depth == self.scopes.len() {
                            self.finish_typedef(i);
                            self.typedef = None;
                        }
                    }
                    if self.pending.is_some() {
                        // A declaration that never opened a body (a prototype's pending scope).
                        self.pending = None;
                    }
                    if self.typedef.is_none() {
                        self.stmt_start = i + 1;
                    }
                    self.seen_eq = false;
                }
                i + 1
            }
            b'(' => {
                if self.paren == 0 && self.decl_ok() && self.typedef.is_none() {
                    self.function_candidate(i);
                }
                self.paren += 1;
                i + 1
            }
            b'[' => {
                self.paren += 1;
                i + 1
            }
            b')' | b']' => {
                self.paren = self.paren.saturating_sub(1);
                i + 1
            }
            b'=' => {
                if self.paren == 0 {
                    self.seen_eq = true;
                }
                i + 1
            }
            b':' => {
                // `public:` and the like start a new statement.
                if i > 0 && matches!(self.word(i - 1), b"public" | b"private" | b"protected") && !is_punct(&self.toks, i + 1, b':') {
                    self.stmt_start = i + 1;
                }
                i + 1
            }
            _ => i + 1,
        }
    }

    fn directive(&mut self, i: usize) -> usize {
        // `# directive ... Eol`
        let name = self.word(i + 1).to_vec();
        let mut j = i + 2;
        if name == b"define" && is_ident(&self.toks, j) {
            let name_tok = self.toks[j];
            let def = self.record(&name_tok, Kind::Macro, None);
            while j < self.toks.len() && self.toks[j].t != T::Eol {
                j += 1;
            }
            if let (Some(d), Some(eol)) = (def, self.toks.get(j)) {
                self.out[d].lines = eol.line.saturating_sub(self.out[d].line) + 1;
            }
        } else if name == b"include" || name == b"include_next" || name == b"import" {
            if let Some(t) = self.toks.get(j).filter(|t| t.t == T::Str) {
                let path = String::from_utf8_lossy(&self.src[t.s as usize..(t.e as usize).min(self.src.len())]).into_owned();
                let shown = if t.c == b'<' { format!("<{path}>") } else { path };
                self.push_import(&shown);
            }
        }
        while j < self.toks.len() && self.toks[j].t != T::Eol {
            j += 1;
        }
        self.stmt_start = j + 1;
        j + 1
    }

    fn ident(&mut self, i: usize) -> usize {
        if self.paren > 0 {
            return i + 1;
        }
        let word = self.word(i).to_vec();
        match word.as_slice() {
            b"typedef" if self.decl_ok() => {
                self.typedef = Some(self.scopes.len());
                i + 1
            }
            b"template" if self.decl_ok() => {
                // Skip the parameter list: `class T` in it is no class.
                if is_punct(&self.toks, i + 1, b'<') {
                    let mut depth = 0i32;
                    let mut j = i + 1;
                    while j < self.toks.len() {
                        if is_punct(&self.toks, j, b'<') {
                            depth += 1;
                        } else if is_punct(&self.toks, j, b'>') {
                            depth -= 1;
                            if depth == 0 {
                                return j + 1;
                            }
                        } else if is_punct(&self.toks, j, b';') || is_punct(&self.toks, j, b'{') {
                            break;
                        }
                        j += 1;
                    }
                }
                i + 1
            }
            b"struct" | b"class" | b"union" if self.decl_ok() && self.pending.is_none() => self.class_like(i, &word),
            b"enum" if self.decl_ok() && self.pending.is_none() => self.enumeration(i),
            b"namespace" if self.decl_ok() && self.pending.is_none() => self.namespace(i),
            b"extern" if self.decl_ok() && self.pending.is_none() => {
                if self.toks.get(i + 1).is_some_and(|t| t.t == T::Str) && is_punct(&self.toks, i + 2, b'{') {
                    self.pending = Some(CPending { kind: Ck::ExternC, def: None });
                    return i + 2;
                }
                i + 1
            }
            _ => i + 1,
        }
    }

    fn class_like(&mut self, i: usize, word: &[u8]) -> usize {
        let kind = match word {
            b"struct" => Kind::Struct,
            b"union" => Kind::Union,
            _ => Kind::Class,
        };
        let mut j = i + 1;
        let name = if is_ident(&self.toks, j) && self.word(j) != b"final" { Some(self.toks[j]) } else { None };
        if name.is_some() {
            j += 1;
        }
        if self.word(j) == b"final" {
            j += 1;
        }
        // Base classes: `: public A, private B`
        if is_punct(&self.toks, j, b':') && !is_punct(&self.toks, j + 1, b':') {
            while j < self.toks.len() && !is_punct(&self.toks, j, b'{') && !is_punct(&self.toks, j, b';') {
                j += 1;
            }
        }
        if !is_punct(&self.toks, j, b'{') {
            return i + 1;
        }
        let owner = self.owner();
        let text = name.as_ref().map_or_else(String::new, |t| name_of(self.src, t));
        let def = name.and_then(|t| self.record(&t, kind, owner));
        self.pending = Some(CPending { kind: Ck::Class(text), def });
        j
    }

    fn enumeration(&mut self, i: usize) -> usize {
        let mut j = i + 1;
        if matches!(self.word(j), b"class" | b"struct") {
            j += 1;
        }
        let name = if is_ident(&self.toks, j) { Some(self.toks[j]) } else { None };
        if name.is_some() {
            j += 1;
        }
        if is_punct(&self.toks, j, b':') && !is_punct(&self.toks, j + 1, b':') {
            while j < self.toks.len() && !is_punct(&self.toks, j, b'{') && !is_punct(&self.toks, j, b';') {
                j += 1;
            }
        }
        if !is_punct(&self.toks, j, b'{') {
            return i + 1;
        }
        let owner = self.owner();
        let def = name.and_then(|t| self.record(&t, Kind::Enum, owner));
        self.pending = Some(CPending { kind: Ck::Enum, def });
        j
    }

    fn namespace(&mut self, i: usize) -> usize {
        let mut j = i + 1;
        let mut last: Option<Tok> = None;
        while is_ident(&self.toks, j) || adjacent_punct(&self.toks, j, b':', b':') {
            if is_ident(&self.toks, j) {
                last = Some(self.toks[j]);
                j += 1;
            } else {
                j += 2;
            }
        }
        if !is_punct(&self.toks, j, b'{') {
            return i + 1;
        }
        let owner = self.owner();
        let text = last.as_ref().map_or_else(String::new, |t| name_of(self.src, t));
        let def = last.and_then(|t| self.record(&t, Kind::Module, owner));
        self.pending = Some(CPending { kind: Ck::Namespace(text), def });
        j
    }

    /// The `(` at `open` may start a function's parameter list: look at the name before it and at
    /// what follows the closing parenthesis.
    fn function_candidate(&mut self, open: usize) {
        if open == 0 || !is_ident(&self.toks, open - 1) || self.seen_eq {
            return;
        }
        let name_idx = open - 1;
        let name = self.toks[name_idx];
        let text = &self.src[name.s as usize..(name.e as usize).min(self.src.len())];
        if C_NOT_A_FUNCTION.iter().any(|k| k.as_bytes() == text) {
            return;
        }
        let qualified = name_idx >= 2 && adjacent_punct(&self.toks, name_idx - 2, b':', b':');
        let has_type = name_idx > self.stmt_start;
        if !(has_type || self.in_class() || qualified) {
            return;
        }
        if name_idx > 0 && is_punct(&self.toks, name_idx - 1, b'~') {
            return;
        }
        let Some(close) = matching_close(&self.toks, open) else {
            return;
        };
        // What follows: qualifiers, a trailing return type, `= 0`, an initialiser list, then a body
        // or a semicolon.
        let mut j = close + 1;
        let mut body = false;
        while j < self.toks.len() {
            let t = self.toks[j];
            match t.t {
                T::Ident => {
                    let w = self.word(j);
                    if matches!(w, b"const" | b"noexcept" | b"override" | b"final" | b"volatile" | b"throw" | b"__attribute__" | b"mutable" | b"constexpr" | b"try") {
                        j += 1;
                        if is_punct(&self.toks, j, b'(') {
                            match matching_close(&self.toks, j) {
                                Some(k) => j = k + 1,
                                None => return,
                            }
                        }
                    } else {
                        return;
                    }
                }
                T::Punct => match t.c {
                    b'{' => {
                        body = true;
                        break;
                    }
                    b';' => break,
                    b'=' => {
                        // `= 0`, `= default`, `= delete`
                        j += 2;
                        if !is_punct(&self.toks, j, b';') {
                            return;
                        }
                        break;
                    }
                    b'-' if is_punct(&self.toks, j + 1, b'>') => {
                        j += 2;
                        while j < self.toks.len() && !is_punct(&self.toks, j, b'{') && !is_punct(&self.toks, j, b';') {
                            j += 1;
                        }
                    }
                    b':' if !is_punct(&self.toks, j + 1, b':') => {
                        // A constructor's initialiser list: up to the brace that follows `)` or `}`.
                        j += 1;
                        let mut depth = 0i32;
                        while j < self.toks.len() {
                            let u = self.toks[j];
                            if u.t == T::Punct {
                                match u.c {
                                    b'(' => depth += 1,
                                    b')' => depth -= 1,
                                    b'{' if depth == 0 && j > 0 && (is_punct(&self.toks, j - 1, b')') || is_punct(&self.toks, j - 1, b'}')) => {
                                        break;
                                    }
                                    b';' if depth == 0 => break,
                                    _ => {}
                                }
                            }
                            j += 1;
                        }
                    }
                    _ => return,
                },
                _ => return,
            }
        }
        let parent = if qualified && name_idx >= 3 && is_ident(&self.toks, name_idx - 3) {
            Some(name_of(self.src, &self.toks[name_idx - 3]))
        } else {
            self.owner()
        };
        let kind = if self.in_class() || qualified { Kind::Method } else { Kind::Fn };
        let def = self.record(&name, kind, parent);
        if body {
            self.pending = Some(CPending { kind: Ck::Fn, def });
        }
    }

    /// The statement `typedef ... NAME;` that ends at token `end`: the name is the one inside
    /// `(*NAME)` of a function-pointer type, else the last identifier before the semicolon.
    fn finish_typedef(&mut self, end: usize) {
        // Skip the part inside braces (`typedef struct { ... } Name;`): look from the last `}` on.
        let mut from = self.stmt_start;
        let mut depth = 0i32;
        for k in self.stmt_start..end {
            if is_punct(&self.toks, k, b'{') {
                depth += 1;
            } else if is_punct(&self.toks, k, b'}') {
                depth -= 1;
                if depth == 0 {
                    from = k + 1;
                }
            }
        }
        let mut name: Option<Tok> = None;
        // A function-pointer or function type: the first `(` holds the name after `*`.
        let paren = (from..end).find(|&k| is_punct(&self.toks, k, b'('));
        if let Some(p) = paren {
            let mut k = p + 1;
            while k < end && (is_punct(&self.toks, k, b'*') || is_punct(&self.toks, k, b'&') || is_punct(&self.toks, k, b'^')) {
                k += 1;
            }
            if is_ident(&self.toks, k) && k < end {
                name = Some(self.toks[k]);
            }
        }
        if name.is_none() {
            let mut k = end;
            while k > from {
                k -= 1;
                if is_punct(&self.toks, k, b']') {
                    // An array typedef: step back over `[N]`.
                    while k > from && !is_punct(&self.toks, k, b'[') {
                        k -= 1;
                    }
                    continue;
                }
                if is_ident(&self.toks, k) {
                    name = Some(self.toks[k]);
                    break;
                }
            }
        }
        let Some(name) = name else {
            return;
        };
        let text = name_of(self.src, &name);
        // `typedef struct Point {..} Point;` names the struct already.
        if self.out.iter().rev().take(8).any(|d| d.name == text && d.line == name.line) {
            return;
        }
        let owner = self.owner();
        self.record(&name, Kind::Type, owner);
    }
}

// ---------------------------------------------------------------------------------------------
// JavaScript and TypeScript, Go, Java, Kotlin, C#, Swift
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Script,
    Go,
    Java,
    Kotlin,
    CSharp,
    Swift,
}

#[derive(Clone)]
enum Bk {
    /// The body of a class, an interface, an enum, an `extension`: what is a function in it is a method.
    Type(String),
    Module(String),
    Fn,
    Other,
}

struct BScope {
    kind: Bk,
    def: Option<usize>,
}

struct BPending {
    def: Option<usize>,
    kind: Bk,
    paren: u32,
    line: u32,
}

struct Br<'a> {
    src: &'a [u8],
    toks: Vec<Tok>,
    out: Vec<RawDef>,
    imports: Vec<String>,
    caps: &'a ScanCaps,
    family: Family,
    scopes: Vec<BScope>,
    pending: Option<BPending>,
    paren: u32,
    exported: bool,
    fn_depth: u32,
    /// Inside `const ( ... )` or `var ( ... )` of Go: the kind of each name that starts a line.
    group: Option<(Kind, u32)>,
}

fn scan_braced(src: &[u8], caps: &ScanCaps, family: Family) -> (Vec<RawDef>, Vec<String>) {
    let dialect = match family {
        Family::Script => Dialect::Script,
        Family::Go => Dialect::Go,
        _ => Dialect::Jvm,
    };
    let toks = lex(src, dialect);
    let mut br = Br {
        src,
        toks,
        out: Vec::new(),
        imports: Vec::new(),
        caps,
        family,
        scopes: Vec::new(),
        pending: None,
        paren: 0,
        exported: false,
        fn_depth: 0,
        group: None,
    };
    br.run();
    (br.out, br.imports)
}

/// Words of a braced language that begin a statement and are never the name of a method.
static NOT_A_METHOD: [&str; 22] = [
    "if", "for", "while", "switch", "catch", "function", "return", "new", "typeof", "super", "this", "import", "await", "delete", "throw", "synchronized",
    "using", "lock", "foreach", "when", "guard", "else",
];

impl Br<'_> {
    fn word(&self, i: usize) -> &[u8] {
        self.toks
            .get(i)
            .filter(|t| t.t == T::Ident)
            .map_or(&[][..], |t| &self.src[t.s as usize..(t.e as usize).min(self.src.len())])
    }

    fn run(&mut self) {
        let mut i = 0;
        while i < self.toks.len() {
            if self.out.len() >= self.caps.max_defs {
                break;
            }
            let tok = self.toks[i];
            self.expire(&tok);
            i = match tok.t {
                T::Punct => self.punct(i, tok),
                T::Ident => self.ident(i, tok),
                _ => i + 1,
            };
        }
        let last = self.toks.last().map_or(1, |t| t.line);
        let open: Vec<usize> = self.scopes.iter().filter_map(|s| s.def).chain(self.pending.as_ref().and_then(|p| p.def)).collect();
        for d in open {
            self.out[d].lines = last.saturating_sub(self.out[d].line) + 1;
        }
    }

    /// A header without a body or a semicolon (`data class P(val x: Int)`, `fun f() = 1`, `type Id = string`) ends
    /// where the next line begins a declaration of its own.
    fn expire(&mut self, tok: &Tok) {
        let Some(p) = &self.pending else {
            return;
        };
        if tok.line <= p.line || self.paren > p.paren || tok.t != T::Ident {
            return;
        }
        let word = &self.src[tok.s as usize..(tok.e as usize).min(self.src.len())];
        let starter = match self.family {
            Family::Script => matches!(word, b"class" | b"interface" | b"enum" | b"function" | b"const" | b"let" | b"var" | b"type" | b"export" | b"import" | b"namespace" | b"declare"),
            Family::Go => matches!(word, b"func" | b"type" | b"const" | b"var" | b"import"),
            Family::Kotlin => matches!(word, b"class" | b"interface" | b"object" | b"fun" | b"val" | b"var" | b"typealias" | b"import" | b"data" | b"sealed" | b"private" | b"public" | b"internal" | b"override"),
            Family::Swift => matches!(word, b"class" | b"struct" | b"enum" | b"protocol" | b"extension" | b"func" | b"var" | b"let" | b"typealias" | b"import" | b"actor" | b"init"),
            Family::Java | Family::CSharp => false,
        };
        if starter {
            self.pending = None;
        }
    }

    fn punct(&mut self, i: usize, tok: Tok) -> usize {
        match tok.c {
            b'{' => {
                let scope = match self.pending.take() {
                    Some(p) if p.paren == self.paren => BScope { kind: p.kind, def: p.def },
                    other => {
                        self.pending = other;
                        BScope { kind: Bk::Other, def: None }
                    }
                };
                if matches!(scope.kind, Bk::Fn) {
                    self.fn_depth += 1;
                }
                self.scopes.push(scope);
                self.exported = false;
                i + 1
            }
            b'}' => {
                if let Some(scope) = self.scopes.pop() {
                    if let Some(d) = scope.def {
                        self.out[d].lines = tok.line.saturating_sub(self.out[d].line) + 1;
                    }
                    if matches!(scope.kind, Bk::Fn) {
                        self.fn_depth = self.fn_depth.saturating_sub(1);
                    }
                }
                // `fun find() = null` inside a body ends at the brace that closes the body.
                if let Some(p) = self.pending.take() {
                    if let Some(d) = p.def {
                        self.out[d].lines = tok.line.saturating_sub(self.out[d].line).max(1);
                    }
                }
                self.paren = 0;
                self.exported = false;
                i + 1
            }
            b';' => {
                if self.pending.as_ref().is_some_and(|p| p.paren == self.paren) {
                    if let Some(p) = self.pending.take() {
                        if let Some(d) = p.def {
                            self.out[d].lines = tok.line.saturating_sub(self.out[d].line) + 1;
                        }
                    }
                }
                if self.paren == 0 {
                    self.exported = false;
                }
                i + 1
            }
            b'(' | b'[' => {
                self.paren += 1;
                i + 1
            }
            b')' | b']' => {
                self.paren = self.paren.saturating_sub(1);
                if self.paren == 0 {
                    self.group = None;
                }
                i + 1
            }
            _ => i + 1,
        }
    }

    fn record(&mut self, name: &Tok, kind: Kind, parent: Option<String>) -> Option<usize> {
        let text = name_of(self.src, name);
        if text.is_empty() {
            return None;
        }
        let public = self.exported || (self.family == Family::Go && text.chars().next().is_some_and(char::is_uppercase));
        self.out.push(RawDef {
            name: text,
            kind,
            vis: if public { Vis::Public } else { Vis::Private },
            test: false,
            line: name.line,
            lines: 1,
            parent,
        });
        self.exported = false;
        Some(self.out.len() - 1)
    }

    fn owner(&self) -> Option<String> {
        match self.scopes.last().map(|s| &s.kind) {
            Some(Bk::Type(name) | Bk::Module(name)) if !name.is_empty() => Some(name.clone()),
            _ => None,
        }
    }

    fn in_type(&self) -> bool {
        matches!(self.scopes.last().map(|s| &s.kind), Some(Bk::Type(_)))
    }

    fn push_import(&mut self, path: &str) {
        if self.imports.len() < self.caps.max_imports && !path.is_empty() && path.len() <= MAX_IMPORT_CHARS {
            self.imports.push(path.to_string());
        }
    }

    fn str_content(&self, t: &Tok) -> String {
        let s = (t.s as usize + 1).min(self.src.len());
        let e = (t.e as usize).saturating_sub(1).max(s).min(self.src.len());
        String::from_utf8_lossy(&self.src[s..e]).into_owned()
    }

    fn ident(&mut self, i: usize, tok: Tok) -> usize {
        let word = self.word(i).to_vec();
        match word.as_slice() {
            b"export" | b"public" | b"open" => {
                self.exported = true;
                return i + 1;
            }
            b"import" | b"using" | b"require" => {
                if let Some(next) = self.imports_at(i, &word) {
                    return next;
                }
            }
            _ => {}
        }
        if self.paren > 0 && self.group.is_none() {
            return i + 1;
        }
        // Go's `const ( A = 1 \n B )`: a name that begins a line.
        if let Some((kind, _)) = self.group {
            if self.paren == 1 && self.pending.is_none() && (i == 0 || self.toks[i - 1].line < tok.line) {
                let parent = self.owner();
                self.record(&tok, kind, parent);
            }
            return i + 1;
        }
        if self.pending.is_some() {
            return i + 1;
        }
        match self.family {
            Family::Script => self.script(i, &word),
            Family::Go => self.go(i, &word),
            Family::Java | Family::CSharp => self.java_like(i, &word),
            Family::Kotlin => self.kotlin(i, &word),
            Family::Swift => self.swift(i, &word),
        }
    }

    /// The imports an `import`, `using` or `require` names; the index after what was read.
    fn imports_at(&mut self, i: usize, word: &[u8]) -> Option<usize> {
        match self.family {
            Family::Script => {
                if word == b"require" {
                    if is_punct(&self.toks, i + 1, b'(') && self.toks.get(i + 2).is_some_and(|t| t.t == T::Str) {
                        let path = self.str_content(&self.toks[i + 2]);
                        self.push_import(&path);
                        return Some(i + 3);
                    }
                    return None;
                }
                if word != b"import" {
                    return None;
                }
                let mut j = i + 1;
                while j < self.toks.len() && j < i + 80 && !is_punct(&self.toks, j, b';') {
                    if self.toks[j].t == T::Str {
                        let path = self.str_content(&self.toks[j]);
                        self.push_import(&path);
                        return Some(j + 1);
                    }
                    if is_punct(&self.toks, j, b'(') {
                        return None;
                    }
                    j += 1;
                }
                None
            }
            Family::Go => {
                if word != b"import" {
                    return None;
                }
                let mut j = i + 1;
                let grouped = is_punct(&self.toks, j, b'(');
                if grouped {
                    j += 1;
                }
                while j < self.toks.len() {
                    let t = self.toks[j];
                    if t.t == T::Str {
                        let path = self.str_content(&t);
                        self.push_import(&path);
                        if !grouped {
                            return Some(j + 1);
                        }
                    } else if grouped && is_punct(&self.toks, j, b')') {
                        return Some(j + 1);
                    } else if !grouped && t.t != T::Ident && !is_punct(&self.toks, j, b'.') && !is_punct(&self.toks, j, b'_') {
                        return None;
                    }
                    j += 1;
                }
                None
            }
            Family::Java | Family::Kotlin | Family::CSharp | Family::Swift => {
                let wanted: &[u8] = if self.family == Family::CSharp { b"using" } else { b"import" };
                if word != wanted || !is_ident(&self.toks, i + 1) {
                    return None;
                }
                let line = self.toks[i].line;
                let mut j = i + 1;
                let mut path = String::new();
                while j < self.toks.len() && self.toks[j].line == line && !is_punct(&self.toks, j, b';') {
                    let t = self.toks[j];
                    if t.t == T::Ident {
                        let w = name_of(self.src, &t);
                        if w == "static" && path.is_empty() {
                            j += 1;
                            continue;
                        }
                        path.push_str(&w);
                    } else if is_punct(&self.toks, j, b'.') {
                        path.push('.');
                    } else if is_punct(&self.toks, j, b'*') {
                        path.push('*');
                    } else if is_punct(&self.toks, j, b'=') {
                        // `using Alias = A.B;`
                        path.clear();
                    } else {
                        break;
                    }
                    j += 1;
                }
                self.push_import(path.trim_end_matches('.'));
                Some(j)
            }
        }
    }

    fn start_type(&mut self, name: &Tok, kind: Kind, bk: Bk) -> usize {
        let parent = self.owner();
        let def = self.record(name, kind, parent);
        self.pending = Some(BPending { def, kind: bk, paren: self.paren, line: name.line });
        0
    }

    fn start_fn(&mut self, name: &Tok, kind: Kind) {
        let parent = self.owner();
        let def = self.record(name, kind, parent);
        self.pending = Some(BPending { def, kind: Bk::Fn, paren: self.paren, line: name.line });
    }

    fn script(&mut self, i: usize, word: &[u8]) -> usize {
        if self.fn_depth > 0 {
            return i + 1;
        }
        let name = self.toks.get(i + 1).copied().filter(|t| t.t == T::Ident);
        match word {
            b"class" | b"interface" | b"enum" => {
                if let Some(n) = name {
                    let kind = match word {
                        b"class" => Kind::Class,
                        b"interface" => Kind::Interface,
                        _ => Kind::Enum,
                    };
                    let text = name_of(self.src, &n);
                    self.start_type(&n, kind, Bk::Type(text));
                    return i + 2;
                }
            }
            b"namespace" | b"module" => {
                if let Some(n) = name {
                    if self.word(i + 2) == b"." || is_punct(&self.toks, i + 2, b'{') || is_punct(&self.toks, i + 2, b'.') {
                        let mut last = n;
                        let mut j = i + 1;
                        while is_ident(&self.toks, j) {
                            last = self.toks[j];
                            if is_punct(&self.toks, j + 1, b'.') {
                                j += 2;
                            } else {
                                break;
                            }
                        }
                        let text = name_of(self.src, &last);
                        self.start_type(&last, Kind::Module, Bk::Module(text));
                        return j + 1;
                    }
                }
            }
            b"type" => {
                if let Some(n) = name {
                    if is_punct(&self.toks, i + 2, b'=') || is_punct(&self.toks, i + 2, b'<') {
                        self.start_type(&n, Kind::Type, Bk::Other);
                        return i + 2;
                    }
                }
            }
            b"function" => {
                let at = if is_punct(&self.toks, i + 1, b'*') { i + 2 } else { i + 1 };
                if let Some(n) = self.toks.get(at).copied().filter(|t| t.t == T::Ident) {
                    if is_punct(&self.toks, at + 1, b'(') || is_punct(&self.toks, at + 1, b'<') {
                        self.start_fn(&n, Kind::Fn);
                        return at + 1;
                    }
                }
            }
            b"const" | b"let" | b"var" => {
                if let Some(n) = name {
                    if is_punct(&self.toks, i + 2, b'=') && !self.in_type() {
                        let text = name_of(self.src, &n);
                        if self.is_function_value(i + 3) {
                            self.start_fn(&n, Kind::Fn);
                            return i + 3;
                        }
                        if word == b"const" && text.chars().any(char::is_alphabetic) && text.chars().all(|c| c.is_uppercase() || c.is_ascii_digit() || c == '_') {
                            let parent = self.owner();
                            let def = self.record(&n, Kind::Const, parent);
                            self.pending = Some(BPending { def, kind: Bk::Other, paren: self.paren, line: n.line });
                            return i + 3;
                        }
                    }
                }
            }
            _ => {
                // A method of a class: `name(...) {`, with the modifiers before it.
                if self.in_type() && is_punct(&self.toks, i + 1, b'(') && !NOT_A_METHOD.iter().any(|k| k.as_bytes() == word) {
                    let before_ok = i == 0
                        || matches!(self.toks[i - 1].t, T::Punct if matches!(self.toks[i - 1].c, b'{' | b';' | b'}' | b'*' | b'>' | b'@'))
                        || matches!(
                            self.word(i - 1),
                            b"static" | b"async" | b"get" | b"set" | b"public" | b"private" | b"protected" | b"readonly" | b"override" | b"abstract" | b"declare"
                        );
                    if before_ok {
                        let tok = self.toks[i];
                        self.start_fn(&tok, Kind::Method);
                        return i + 1;
                    }
                }
            }
        }
        i + 1
    }

    /// Whether the value after `=` is a function: `function`, `async x =>`, `(a) =>`.
    fn is_function_value(&self, at: usize) -> bool {
        let mut j = at;
        if self.word(j) == b"async" {
            j += 1;
        }
        if self.word(j) == b"function" {
            return true;
        }
        if is_ident(&self.toks, j) {
            return adjacent_punct(&self.toks, j + 1, b'=', b'>');
        }
        if is_punct(&self.toks, j, b'(') {
            let Some(close) = matching_close(&self.toks, j) else {
                return false;
            };
            let mut k = close + 1;
            // A return type: `(a): number =>`
            if is_punct(&self.toks, k, b':') {
                while k < self.toks.len() && k < close + 12 && !adjacent_punct(&self.toks, k, b'=', b'>') && !is_punct(&self.toks, k, b';') {
                    k += 1;
                }
            }
            return adjacent_punct(&self.toks, k, b'=', b'>');
        }
        false
    }

    fn go(&mut self, i: usize, word: &[u8]) -> usize {
        if self.fn_depth > 0 {
            return i + 1;
        }
        match word {
            b"func" => {
                let mut at = i + 1;
                let mut parent = None;
                if is_punct(&self.toks, at, b'(') {
                    // The receiver: the last name before the closing parenthesis (`r *Reader` is Reader).
                    let Some(close) = matching_close(&self.toks, at) else {
                        return self.toks.len();
                    };
                    parent = (at..close).rev().find(|&k| is_ident(&self.toks, k)).map(|k| name_of(self.src, &self.toks[k]));
                    at = close + 1;
                }
                if let Some(n) = self.toks.get(at).copied().filter(|t| t.t == T::Ident) {
                    let def = self.record(&n, if parent.is_some() { Kind::Method } else { Kind::Fn }, parent);
                    self.pending = Some(BPending { def, kind: Bk::Fn, paren: self.paren, line: n.line });
                    return at + 1;
                }
            }
            b"type" => {
                if let Some(n) = self.toks.get(i + 1).copied().filter(|t| t.t == T::Ident) {
                    let kind = match self.word(i + 2) {
                        b"struct" => Kind::Struct,
                        b"interface" => Kind::Interface,
                        _ => Kind::Type,
                    };
                    self.start_type(&n, kind, Bk::Other);
                    return i + 2;
                }
            }
            b"const" | b"var" => {
                let kind = if word == b"const" { Kind::Const } else { Kind::Static };
                if is_punct(&self.toks, i + 1, b'(') {
                    self.group = Some((kind, 1));
                    return i + 1;
                }
                if let Some(n) = self.toks.get(i + 1).copied().filter(|t| t.t == T::Ident) {
                    let def = self.record(&n, kind, None);
                    self.pending = Some(BPending { def, kind: Bk::Other, paren: self.paren, line: n.line });
                    return i + 2;
                }
            }
            _ => {}
        }
        i + 1
    }

    fn java_like(&mut self, i: usize, word: &[u8]) -> usize {
        let csharp = self.family == Family::CSharp;
        let name = self.toks.get(i + 1).copied().filter(|t| t.t == T::Ident);
        match word {
            b"class" | b"interface" | b"enum" | b"record" | b"struct" if word != b"struct" || csharp => {
                if let Some(n) = name {
                    // `class` after a dot (`Foo.class`) is no declaration.
                    if i > 0 && is_punct(&self.toks, i - 1, b'.') {
                        return i + 1;
                    }
                    let kind = match word {
                        b"interface" => Kind::Interface,
                        b"enum" => Kind::Enum,
                        b"struct" => Kind::Struct,
                        _ => Kind::Class,
                    };
                    let text = name_of(self.src, &n);
                    self.start_type(&n, kind, Bk::Type(text));
                    return i + 2;
                }
            }
            b"namespace" if csharp => {
                let mut last = name;
                let mut j = i + 1;
                while is_ident(&self.toks, j) {
                    last = Some(self.toks[j]);
                    if is_punct(&self.toks, j + 1, b'.') {
                        j += 2;
                    } else {
                        break;
                    }
                }
                if let Some(n) = last {
                    let text = name_of(self.src, &n);
                    self.start_type(&n, Kind::Module, Bk::Module(text));
                    return j + 1;
                }
            }
            _ => {
                if self.in_type() && is_punct(&self.toks, i + 1, b'(') && !NOT_A_METHOD.iter().any(|k| k.as_bytes() == word) {
                    let tok = self.toks[i];
                    let own = matches!(self.scopes.last().map(|s| &s.kind), Some(Bk::Type(t)) if t.as_bytes() == word);
                    let typed_before = i > 0
                        && (matches!(self.toks[i - 1].t, T::Ident)
                            || (matches!(self.toks[i - 1].t, T::Punct) && matches!(self.toks[i - 1].c, b'>' | b']')))
                        && !matches!(self.word(i - 1), b"new" | b"return" | b"throw" | b"else" | b"await");
                    if own || typed_before {
                        self.start_fn(&tok, Kind::Method);
                        return i + 1;
                    }
                }
            }
        }
        i + 1
    }

    fn kotlin(&mut self, i: usize, word: &[u8]) -> usize {
        if self.fn_depth > 0 {
            return i + 1;
        }
        let name = self.toks.get(i + 1).copied().filter(|t| t.t == T::Ident);
        match word {
            b"class" | b"interface" | b"object" => {
                if let Some(n) = name {
                    let kind = if word == b"interface" { Kind::Interface } else { Kind::Class };
                    let text = name_of(self.src, &n);
                    self.start_type(&n, kind, Bk::Type(text));
                    return i + 2;
                }
            }
            b"typealias" => {
                if let Some(n) = name {
                    self.start_type(&n, Kind::Type, Bk::Other);
                    return i + 2;
                }
            }
            b"fun" => {
                // `fun <T> Recv.name(`: the last name before the parenthesis.
                let mut j = i + 1;
                let mut last: Option<Tok> = None;
                while j < self.toks.len() && j < i + 40 && !is_punct(&self.toks, j, b'(') {
                    if is_ident(&self.toks, j) {
                        last = Some(self.toks[j]);
                    } else if is_punct(&self.toks, j, b'{') || is_punct(&self.toks, j, b'=') {
                        last = None;
                        break;
                    }
                    j += 1;
                }
                if let Some(n) = last {
                    let kind = if self.in_type() { Kind::Method } else { Kind::Fn };
                    self.start_fn(&n, kind);
                    return j;
                }
            }
            b"val" if i > 0 && self.word(i - 1) == b"const" => {
                if let Some(n) = name {
                    let parent = self.owner();
                    let def = self.record(&n, Kind::Const, parent);
                    self.pending = Some(BPending { def, kind: Bk::Other, paren: self.paren, line: n.line });
                    return i + 2;
                }
            }
            _ => {}
        }
        i + 1
    }

    fn swift(&mut self, i: usize, word: &[u8]) -> usize {
        if self.fn_depth > 0 {
            return i + 1;
        }
        let name = self.toks.get(i + 1).copied().filter(|t| t.t == T::Ident);
        match word {
            b"class" | b"struct" | b"enum" | b"protocol" | b"actor" => {
                if let Some(n) = name {
                    let kind = match word {
                        b"struct" => Kind::Struct,
                        b"enum" => Kind::Enum,
                        b"protocol" => Kind::Interface,
                        _ => Kind::Class,
                    };
                    let text = name_of(self.src, &n);
                    self.start_type(&n, kind, Bk::Type(text));
                    return i + 2;
                }
            }
            b"extension" => {
                if let Some(n) = name {
                    let text = name_of(self.src, &n);
                    self.pending = Some(BPending { def: None, kind: Bk::Type(text), paren: self.paren, line: n.line });
                    return i + 2;
                }
            }
            b"typealias" => {
                if let Some(n) = name {
                    self.start_type(&n, Kind::Type, Bk::Other);
                    return i + 2;
                }
            }
            b"func" => {
                if let Some(n) = name {
                    let kind = if self.in_type() { Kind::Method } else { Kind::Fn };
                    self.start_fn(&n, kind);
                    return i + 2;
                }
            }
            _ => {}
        }
        i + 1
    }
}

// ---------------------------------------------------------------------------------------------
// Shell
// ---------------------------------------------------------------------------------------------

/// The braces of a line of shell that are not inside a quote.
fn shell_braces(line: &str) -> i32 {
    let mut depth = 0i32;
    let mut quote = 0u8;
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if quote != 0 {
            if c == b'\\' && quote == b'"' {
                i += 1;
            } else if c == quote {
                quote = 0;
            }
        } else {
            match c {
                b'"' | b'\'' => quote = c,
                b'#' if i == 0 || b[i - 1] == b' ' || b[i - 1] == b'\t' => break,
                b'{' => depth += 1,
                b'}' => depth -= 1,
                _ => {}
            }
        }
        i += 1;
    }
    depth
}

fn scan_shell(src: &[u8], caps: &ScanCaps) -> (Vec<RawDef>, Vec<String>) {
    let text = String::from_utf8_lossy(src);
    let mut out: Vec<RawDef> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut open: Option<(usize, i32)> = None;
    for (n, line) in text.lines().enumerate() {
        if out.len() >= caps.max_defs {
            break;
        }
        let line_no = u32::try_from(n + 1).unwrap_or(u32::MAX);
        let t = line.trim_start();
        if t.starts_with('#') {
            continue;
        }
        if let Some((def, depth)) = open.as_mut() {
            *depth += shell_braces(line);
            if *depth <= 0 {
                out[*def].lines = line_no.saturating_sub(out[*def].line) + 1;
                open = None;
            }
            continue;
        }
        if let Some(rest) = t.strip_prefix("source ").or_else(|| t.strip_prefix(". ")) {
            let path = rest.split_whitespace().next().unwrap_or("").trim_matches(|c| c == '"' || c == '\'');
            if !path.is_empty() && path.len() <= MAX_IMPORT_CHARS && imports.len() < caps.max_imports {
                imports.push(path.to_string());
            }
            continue;
        }
        let (had_keyword, rest) = t.strip_prefix("function ").map_or((false, t), |r| (true, r.trim_start()));
        let end = rest.find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':'))).unwrap_or(rest.len());
        let (name, after) = rest.split_at(end);
        let after = after.trim_start();
        let defined = !name.is_empty() && (had_keyword || after.starts_with("()")) && (after.is_empty() || after.starts_with('(') || after.starts_with('{'));
        if !defined {
            continue;
        }
        out.push(RawDef { name: name.to_string(), kind: Kind::Fn, vis: Vis::Public, test: false, line: line_no, lines: 1, parent: None });
        let depth = shell_braces(after);
        if depth > 0 {
            open = Some((out.len() - 1, depth));
        }
    }
    if let Some((def, _)) = open {
        let total = u32::try_from(text.lines().count()).unwrap_or(u32::MAX);
        out[def].lines = total.saturating_sub(out[def].line) + 1;
    }
    (out, imports)
}

// ---------------------------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------------------------

struct OpenDef {
    idx: usize,
    indent: usize,
    is_class: bool,
    name: String,
}

/// The identifier at the start of `s` (letters, digits, `_`, non-ASCII), and the rest.
fn ident_prefix(s: &str) -> (&str, &str) {
    let end = s.find(|c: char| !(c.is_alphanumeric() || c == '_')).unwrap_or(s.len());
    s.split_at(end)
}

fn is_python_constant(name: &str) -> bool {
    name.len() >= 2
        && name.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && name.chars().any(|c| c.is_ascii_uppercase())
}

fn scan_python(src: &[u8], caps: &ScanCaps) -> (Vec<RawDef>, Vec<String>) {
    let text = String::from_utf8_lossy(src);
    let mut out: Vec<RawDef> = Vec::new();
    let mut imports: Vec<String> = Vec::new();
    let mut stack: Vec<OpenDef> = Vec::new();
    let mut triple: Option<(u8, usize)> = None;
    let mut depth: i32 = 0;
    let mut continued = false;
    let mut last_code = 0u32;
    let push_import = |imports: &mut Vec<String>, p: String| {
        if imports.len() < caps.max_imports && !p.is_empty() && p.len() <= MAX_IMPORT_CHARS {
            imports.push(p);
        }
    };
    for (n, raw_line) in text.split('\n').enumerate() {
        let line_no = u32::try_from(n + 1).unwrap_or(u32::MAX);
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if out.len() >= caps.max_defs {
            break;
        }
        // Inside a triple-quoted string: look for its end.
        let mut code_from = 0usize;
        if let Some((q, _)) = triple {
            let pat = [q, q, q];
            match find_triple(line.as_bytes(), 0, &pat) {
                Some(end) => {
                    triple = None;
                    code_from = end;
                    last_code = line_no;
                    if line[code_from..].trim().is_empty() {
                        continue;
                    }
                }
                None => {
                    last_code = line_no;
                    continue;
                }
            }
        }
        let indent = if code_from == 0 { indent_of(line) } else { usize::MAX };
        let trimmed = line[code_from..].trim_start();
        let statement_start = depth == 0 && !continued && code_from == 0;
        if trimmed.is_empty() || (trimmed.starts_with('#') && code_from == 0) {
            continue;
        }
        if statement_start {
            while stack.last().is_some_and(|o| indent <= o.indent) {
                if let Some(o) = stack.pop() {
                    out[o.idx].lines = last_code.saturating_sub(out[o.idx].line) + 1;
                }
            }
            let owner_is_def = stack.last().is_some_and(|o| !o.is_class);
            let after_async = trimmed.strip_prefix("async ").map_or(trimmed, str::trim_start);
            if !owner_is_def {
                let parent_class = stack.last().map(|o| o.name.clone());
                if let Some(rest) = after_async.strip_prefix("def ") {
                    let (name, _) = ident_prefix(rest.trim_start());
                    if !name.is_empty() {
                        let kind = if parent_class.is_some() { Kind::Method } else { Kind::Fn };
                        out.push(RawDef {
                            name: name.to_string(),
                            kind,
                            vis: if name.starts_with('_') && !name.starts_with("__") { Vis::Private } else { Vis::Public },
                            test: false,
                            line: line_no,
                            lines: 1,
                            parent: parent_class,
                        });
                        stack.push(OpenDef { idx: out.len() - 1, indent, is_class: false, name: name.to_string() });
                    }
                } else if let Some(rest) = trimmed.strip_prefix("class ") {
                    let (name, _) = ident_prefix(rest.trim_start());
                    if !name.is_empty() {
                        out.push(RawDef {
                            name: name.to_string(),
                            kind: Kind::Class,
                            vis: if name.starts_with('_') { Vis::Private } else { Vis::Public },
                            test: false,
                            line: line_no,
                            lines: 1,
                            parent: parent_class,
                        });
                        stack.push(OpenDef { idx: out.len() - 1, indent, is_class: true, name: name.to_string() });
                    }
                } else if indent == 0 {
                    let (name, rest) = ident_prefix(trimmed);
                    let rest = rest.trim_start();
                    if is_python_constant(name) && ((rest.starts_with('=') && !rest.starts_with("==")) || rest.starts_with(':')) {
                        out.push(RawDef {
                            name: name.to_string(),
                            kind: Kind::Const,
                            vis: Vis::Public,
                            test: false,
                            line: line_no,
                            lines: 1,
                            parent: None,
                        });
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("import ") {
                for part in rest.split(',') {
                    let name = part.split(" as ").next().unwrap_or("").trim();
                    push_import(&mut imports, name.to_string());
                }
            } else if let Some(rest) = trimmed.strip_prefix("from ") {
                if let Some((module, names)) = rest.split_once(" import ") {
                    let module = module.trim();
                    let names = names.trim().trim_start_matches('(').trim_end_matches(')');
                    for part in names.split(',') {
                        let name = part.split(" as ").next().unwrap_or("").trim();
                        if name.is_empty() {
                            continue;
                        }
                        if name == "*" {
                            push_import(&mut imports, module.to_string());
                        } else if module.ends_with('.') {
                            push_import(&mut imports, format!("{module}{name}"));
                        } else {
                            push_import(&mut imports, format!("{module}.{name}"));
                        }
                    }
                }
            }
        }
        last_code = line_no;
        // The brackets, strings and continuation of this line.
        let bytes = &line.as_bytes()[code_from..];
        let mut k = 0usize;
        continued = false;
        while k < bytes.len() {
            match bytes[k] {
                b'#' => break,
                q @ (b'"' | b'\'') => {
                    if bytes.get(k + 1) == Some(&q) && bytes.get(k + 2) == Some(&q) {
                        let pat = [q, q, q];
                        match find_triple(bytes, k + 3, &pat) {
                            Some(end) => k = end,
                            None => {
                                triple = Some((q, 0));
                                break;
                            }
                        }
                        continue;
                    }
                    // A one-line string.
                    k += 1;
                    while k < bytes.len() && bytes[k] != q {
                        if bytes[k] == b'\\' {
                            k += 1;
                        }
                        k += 1;
                    }
                }
                b'(' | b'[' | b'{' => depth += 1,
                b')' | b']' | b'}' => depth = (depth - 1).max(0),
                b'\\' if k + 1 >= bytes.len() => continued = true,
                _ => {}
            }
            k += 1;
        }
    }
    let total = u32::try_from(text.split('\n').count()).unwrap_or(u32::MAX);
    for o in stack.drain(..) {
        out[o.idx].lines = last_code.min(total).saturating_sub(out[o.idx].line) + 1;
    }
    (out, imports)
}

fn indent_of(line: &str) -> usize {
    let mut n = 0;
    for c in line.chars() {
        match c {
            ' ' => n += 1,
            '\t' => n += 8,
            _ => break,
        }
    }
    n
}

/// The index just past the first `pat` (three quotes) in `bytes` at or after `from`.
fn find_triple(bytes: &[u8], from: usize, pat: &[u8; 3]) -> Option<usize> {
    let mut k = from;
    while k + 3 <= bytes.len() {
        if bytes[k] == b'\\' {
            k += 2;
            continue;
        }
        if bytes[k..k + 3] == pat[..] {
            return Some(k + 3);
        }
        k += 1;
    }
    None
}

#[cfg(test)]
#[path = "symbols_scan_test.rs"]
mod symbols_scan_test;

#[cfg(test)]
#[path = "symbols_scan_more_test.rs"]
mod symbols_scan_more_test;
