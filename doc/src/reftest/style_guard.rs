//! Proof that an edit changed only the style of a file, never its logic.
//!
//! The rewrite pass (`autoreview --rewrite-source`) lets a low-cost model
//! rewrite the comments of one source file, add blank lines and give
//! too-short local variables readable names. Its edits are kept only if this
//! module finds the code otherwise untouched: both versions are lexed, the
//! comments are set aside, and the remaining tokens must match one for one,
//! including whether two symbols touch (`->` is not `- >`). The one allowed
//! difference is a renamed local variable, and only when the rename is a
//! clean substitution inside its function (see `check_renames`).
//!
//! A few more rules catch edits that keep the tokens but still break the
//! build or the tests: no doc comment where there was none, doctest code
//! blocks unchanged, `+spec` annotations and TODO markers kept, and the file
//! must still parse.
//!
//! The lexer knows just enough Rust to tell comments apart from strings, chars
//! and lifetimes. `cross_check` compares it with proc-macro2's lexer before an
//! agent may touch a file, so a gap in this lexer shows up as a skipped file
//! instead of a code change that slips through.

use std::collections::{BTreeMap, HashMap, HashSet};

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use syn::{spanned::Spanned, visit::Visit};

/// What a lexed token is. Comments stay in the token list so the checks can
/// see where doc comments sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokKind {
    Ident,
    Lifetime,
    Literal,
    Punct,
    Comment(CommentKind),
}

/// Plain comments may change freely; doc comments turn into attributes, so
/// where they sit matters to the compiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentKind {
    Plain,
    /// `///` or `/** */`: documents the item that follows.
    OuterDoc,
    /// `//!` or `/*! */`: documents the enclosing module or crate.
    InnerDoc,
}

#[derive(Debug, Clone, Copy)]
pub struct Tok<'a> {
    /// What the token is.
    pub kind: TokKind,
    /// The token exactly as written.
    pub text: &'a str,
    /// Byte offset of the token in the source, used to rebuild the file
    /// without its comments.
    pub start: usize,
    /// 1-based line of the token's first character, for error messages and
    /// for moving finding line numbers to the edited file.
    pub line: usize,
    /// For punctuation: the next character is punctuation too, with nothing in
    /// between, so `->` and `- >` compare different. False for other tokens.
    pub joint: bool,
}

impl Tok<'_> {
    fn is_code(&self) -> bool {
        !matches!(self.kind, TokKind::Comment(_))
    }
}

/// proc-macro2's punctuation set. A punctuation token is joint when the next
/// character is in this set too.
const PUNCT_CHARS: &str = "~!@#$%^&*-=+|;:,<.>/?'";

fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

fn char_at(src: &str, i: usize) -> Option<char> {
    src.get(i..)?.chars().next()
}

/// Splits Rust source into tokens, comments included.
pub fn lex(src: &str) -> Result<Vec<Tok<'_>>, String> {
    let mut toks = Vec::new();
    let mut i = if src.starts_with('\u{feff}') { '\u{feff}'.len_utf8() } else { 0 };
    let mut line = 1usize;
    while let Some(c) = char_at(src, i) {
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }
        let rest = &src[i..];
        let unterminated = |what: &str| format!("line {line}: unterminated {what}");
        let (kind, end) = if rest.starts_with("//") {
            let end = rest.find('\n').map_or(src.len(), |n| i + n);
            (TokKind::Comment(line_comment_kind(&src[i..end])), end)
        } else if rest.starts_with("/*") {
            let end = block_comment_end(src, i).ok_or_else(|| unterminated("block comment"))?;
            (TokKind::Comment(block_comment_kind(&src[i..end])), end)
        } else if c == '"' {
            let end = quoted_end(src, i + 1).ok_or_else(|| unterminated("string"))?;
            (TokKind::Literal, literal_suffix_end(src, end))
        } else if c == '\'' {
            quote_token(src, i).ok_or_else(|| unterminated("character literal"))?
        } else if c.is_ascii_digit() {
            (TokKind::Literal, number_end(src, i))
        } else if is_ident_start(c) {
            word_token(src, i).ok_or_else(|| unterminated("literal"))?
        } else {
            (TokKind::Punct, i + c.len_utf8())
        };
        let text = &src[i..end];
        let joint = kind == TokKind::Punct && PUNCT_CHARS.contains(c) && starts_with_punct(&src[end..]);
        toks.push(Tok {
            kind,
            text,
            start: i,
            line,
            joint,
        });
        line += text.matches('\n').count();
        i = end;
    }
    Ok(toks)
}

/// The text starts with punctuation that does not open a comment.
fn starts_with_punct(rest: &str) -> bool {
    !rest.starts_with("//")
        && !rest.starts_with("/*")
        && rest.chars().next().is_some_and(|c| PUNCT_CHARS.contains(c))
}

fn line_comment_kind(text: &str) -> CommentKind {
    if text.starts_with("///") && !text.starts_with("////") {
        CommentKind::OuterDoc
    } else if text.starts_with("//!") {
        CommentKind::InnerDoc
    } else {
        CommentKind::Plain
    }
}

fn block_comment_kind(text: &str) -> CommentKind {
    if text.starts_with("/*!") {
        CommentKind::InnerDoc
    } else if text.starts_with("/**") && !text.starts_with("/***") && text != "/**/" {
        CommentKind::OuterDoc
    } else {
        CommentKind::Plain
    }
}

/// End of the block comment that starts at `start`; block comments nest.
fn block_comment_end(src: &str, start: usize) -> Option<usize> {
    let b = src.as_bytes();
    let mut depth = 0usize;
    let mut j = start;
    while j + 1 < b.len() {
        if b[j] == b'/' && b[j + 1] == b'*' {
            depth += 1;
            j += 2;
        } else if b[j] == b'*' && b[j + 1] == b'/' {
            depth -= 1;
            j += 2;
            if depth == 0 {
                return Some(j);
            }
        } else {
            j += 1;
        }
    }
    None
}

/// End (just past the closing quote) of a `"`-quoted string whose body starts
/// at `j`. A backslash escapes the next byte; multi-byte characters never
/// contain `"` or `\` bytes, so a byte scan is safe.
fn quoted_end(src: &str, mut j: usize) -> Option<usize> {
    let b = src.as_bytes();
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => return Some(j + 1),
            _ => j += 1,
        }
    }
    None
}

/// End of a raw string whose hashes (or opening quote) start at `j`, right
/// after the `r`: `r#"a "quoted" b"#` ends at the first `"#`.
fn raw_string_end(src: &str, j: usize) -> Option<usize> {
    let hashes = src[j..].bytes().take_while(|&c| c == b'#').count();
    if src.as_bytes().get(j + hashes) != Some(&b'"') {
        return None;
    }
    let closing = format!("\"{}", "#".repeat(hashes));
    let body = j + hashes + 1;
    src[body..].find(&closing).map(|n| body + n + closing.len())
}

fn ident_end(src: &str, mut j: usize) -> usize {
    while let Some(c) = char_at(src, j) {
        if !is_ident_continue(c) {
            break;
        }
        j += c.len_utf8();
    }
    j
}

/// A literal may carry a suffix (`"x"suffix`, `1u8`); proc-macro2 keeps it in
/// the literal, so this lexer does too.
fn literal_suffix_end(src: &str, j: usize) -> usize {
    if char_at(src, j).is_some_and(is_ident_start) {
        ident_end(src, j)
    } else {
        j
    }
}

/// A token starting with `'`: a char literal (`'a'`, `'\n'`, `'\u{1F600}'`),
/// a lifetime or label (`'a`, `'static`), or a lone quote.
fn quote_token(src: &str, i: usize) -> Option<(TokKind, usize)> {
    let after = i + 1;
    let Some(first) = char_at(src, after) else {
        return Some((TokKind::Punct, after));
    };
    if first == '\\' {
        let escaped = char_at(src, after + 1)?;
        let j = after + 1 + escaped.len_utf8();
        let close = src[j..].find('\'')?;
        return Some((TokKind::Literal, j + close + 1));
    }
    let after_first = after + first.len_utf8();
    if char_at(src, after_first) == Some('\'') {
        return Some((TokKind::Literal, after_first + 1));
    }
    if is_ident_start(first) {
        return Some((TokKind::Lifetime, ident_end(src, after)));
    }
    Some((TokKind::Punct, after))
}

/// A token starting with a letter: an identifier, a raw identifier
/// (`r#type`), or a literal with a letter prefix (`r"…"`, `r#"…"#`, `b"…"`,
/// `br"…"`, `b'x'`, `c"…"`, `cr"…"`).
fn word_token(src: &str, i: usize) -> Option<(TokKind, usize)> {
    let rest = &src[i..];
    let (prefix, raw) = if rest.starts_with("br") || rest.starts_with("cr") {
        (2, true)
    } else if rest.starts_with('r') {
        (1, true)
    } else if rest.starts_with('b') || rest.starts_with('c') {
        (1, false)
    } else {
        (0, false)
    };
    if prefix > 0 {
        let j = i + prefix;
        if raw {
            let hashes = src[j..].bytes().take_while(|&c| c == b'#').count();
            if src.as_bytes().get(j + hashes) == Some(&b'"') {
                return raw_string_end(src, j).map(|e| (TokKind::Literal, literal_suffix_end(src, e)));
            }
            if prefix == 1 && hashes == 1 && char_at(src, j + 1).is_some_and(is_ident_start) {
                return Some((TokKind::Ident, ident_end(src, j + 1)));
            }
        } else if char_at(src, j) == Some('"') {
            return quoted_end(src, j + 1).map(|e| (TokKind::Literal, literal_suffix_end(src, e)));
        } else if rest.starts_with("b'") {
            return match quote_token(src, j)? {
                (TokKind::Literal, e) => Some((TokKind::Literal, e)),
                _ => None,
            };
        }
    }
    Some((TokKind::Ident, ident_end(src, i)))
}

/// End of a number literal: digits, a radix prefix, a fraction, an exponent
/// (`1e-5`) and a suffix (`2.0f32`). `1..2` and `1.max(2)` stop before the dot,
/// and `t.0.1` lexes the `0.1` as one literal, all as proc-macro2 does.
fn number_end(src: &str, i: usize) -> usize {
    let b = src.as_bytes();
    let radix = b.get(i) == Some(&b'0') && matches!(b.get(i + 1), Some(b'x' | b'o' | b'b'));
    let mut j = alnum_run(src, if radix { i + 2 } else { i }, radix);
    if !radix && b.get(j) == Some(&b'.') {
        let after_dot = char_at(src, j + 1);
        if !after_dot.is_some_and(|c| c == '.' || is_ident_start(c)) {
            j = alnum_run(src, j + 1, false);
        }
    }
    j
}

/// Digits, letters and underscores, plus the sign of a decimal exponent
/// (`e-5` right after a digit).
fn alnum_run(src: &str, mut j: usize, radix: bool) -> usize {
    let b = src.as_bytes();
    while j < b.len() {
        let c = b[j];
        let exponent_sign = !radix
            && (c == b'+' || c == b'-')
            && j >= 2
            && matches!(b[j - 1], b'e' | b'E')
            && (b[j - 2].is_ascii_digit() || b[j - 2] == b'_' || b[j - 2] == b'.')
            && b.get(j + 1).is_some_and(u8::is_ascii_digit);
        if c.is_ascii_alphanumeric() || c == b'_' || exponent_sign {
            j += 1;
        } else {
            break;
        }
    }
    j
}

/// For each line of the original file, the line of the edited file that holds
/// the same code. Findings are written against the original, then moved.
#[derive(Debug)]
pub struct LineMap {
    /// (old line, new line) of every code token, in order.
    pairs: Vec<(usize, usize)>,
}

impl LineMap {
    /// No edit, no movement.
    pub fn identity() -> Self {
        LineMap { pairs: Vec::new() }
    }

    /// The new line for `old`. A line of code maps to where that code is now;
    /// a comment or blank line keeps its distance to the code below it, but
    /// never lands above the code before it.
    pub fn map(&self, old: usize) -> usize {
        if self.pairs.is_empty() {
            return old;
        }
        let idx = self.pairs.partition_point(|&(o, _)| o < old);
        match self.pairs.get(idx) {
            Some(&(o, n)) if o == old => n,
            Some(&(o, n)) => {
                let floor = idx.checked_sub(1).map_or(1, |p| self.pairs[p].1 + 1);
                n.saturating_sub(o - old).max(floor).min(n)
            }
            None => {
                let (o, n) = self.pairs[self.pairs.len() - 1];
                n + (old - o)
            }
        }
    }
}

/// Checks that `after` differs from `before` only in style (comments,
/// whitespace, renamed local variables) under the rules in the module docs,
/// and returns how line numbers moved.
pub fn check_style_only(before: &str, after: &str, renames: bool) -> Result<LineMap, String> {
    let old = lex(before).map_err(|e| format!("the original does not lex: {e}"))?;
    let new = lex(after).map_err(|e| format!("the edited file does not lex: {e}"))?;
    let pairs = compare_code(before, &old, &new, renames)?;
    check_doc_placement(&old, &new)?;
    if doc_code_blocks(&old) != doc_code_blocks(&new) {
        return Err("a code block inside a doc comment (a doctest) changed".into());
    }
    if spec_annotations(&old) != spec_annotations(&new) {
        return Err("a `+spec` annotation changed".into());
    }
    let (todo_before, todo_after) = (marker_count(&old), marker_count(&new));
    if todo_before != todo_after {
        return Err(format!(
            "TODO/FIXME/XXX/HACK markers went from {todo_before} to {todo_after}; they are flagged, not removed"
        ));
    }
    still_parses(before, after)?;
    Ok(LineMap { pairs })
}

/// Checks rustfmt's output for a file whose edit was already accepted: only
/// the layout may change, plus the trailing commas rustfmt adds to vertical
/// lists and drops from horizontal ones. Returns how line numbers moved.
pub fn check_layout_only(before: &str, after: &str) -> Result<LineMap, String> {
    let old = lex(before).map_err(|e| format!("the original does not lex: {e}"))?;
    let new = lex(after).map_err(|e| format!("the formatted file does not lex: {e}"))?;
    let old = without_trailing_commas(&old);
    let new = without_trailing_commas(&new);
    let pairs = compare_code(before, &old, &new, false)?;
    still_parses(before, after)?;
    Ok(LineMap { pairs })
}

/// The code tokens without commas that close a list (`,` before `)`, `]`,
/// `}`, or the `{` after a where clause).
fn without_trailing_commas<'a>(toks: &[Tok<'a>]) -> Vec<Tok<'a>> {
    let code: Vec<Tok<'a>> = toks.iter().filter(|t| t.is_code()).copied().collect();
    code.iter()
        .enumerate()
        .filter(|(k, t)| {
            let closes = code
                .get(k + 1)
                .is_some_and(|n| matches!(n.text, ")" | "]" | "}" | "{"));
            !(t.text == "," && closes)
        })
        .map(|(_, t)| *t)
        .collect()
}

/// The code tokens must match one for one; with `renames`, a local variable
/// may carry a new name (see `check_renames`). Returns the (old, new) line of
/// every code token.
fn compare_code(
    before: &str,
    old: &[Tok],
    new: &[Tok],
    renames: bool,
) -> Result<Vec<(usize, usize)>, String> {
    let old_code: Vec<&Tok> = old.iter().filter(|t| t.is_code()).collect();
    let new_code: Vec<&Tok> = new.iter().filter(|t| t.is_code()).collect();
    if old_code.len() != new_code.len() {
        // Report the first place where the two diverge, not the end of the file.
        let first = old_code
            .iter()
            .zip(&new_code)
            .position(|(a, b)| a.text != b.text)
            .unwrap_or(old_code.len().min(new_code.len()));
        let tok = old_code.get(first).or(new_code.get(first)).copied();
        let (line, text) = tok.map_or((0, ""), |t| (t.line, t.text));
        let what = if old_code.len() > new_code.len() { "removed" } else { "added" };
        return Err(format!("code {what} near line {line} (`{}`)", short(text)));
    }
    let mut changed: Vec<usize> = Vec::new();
    for (k, (a, b)) in old_code.iter().zip(&new_code).enumerate() {
        if a.joint != b.joint {
            return Err(format!(
                "the spacing after `{}` at line {} changed, which changes the symbol",
                a.text, a.line
            ));
        }
        if a.kind == b.kind && a.text == b.text {
            continue;
        }
        if renames && a.kind == b.kind && matches!(a.kind, TokKind::Ident | TokKind::Literal) {
            changed.push(k);
            continue;
        }
        return Err(format!(
            "code changed at line {} (line {} after the edit): `{}` became `{}`",
            a.line,
            b.line,
            short(a.text),
            short(b.text)
        ));
    }
    if !changed.is_empty() {
        check_renames(before, &old_code, &new_code, &changed)?;
    }
    Ok(old_code.iter().zip(&new_code).map(|(a, b)| (a.line, b.line)).collect())
}

/// One function of the file and where a local variable may be renamed in it.
#[derive(Default)]
struct FnScope {
    /// Byte range of the whole function, signature included.
    start: usize,
    end: usize,
    /// Names bound by `let`, parameters, closures and patterns in this function.
    bindings: HashSet<String>,
    /// Offsets of identifiers that are a binding or a plain use of a name
    /// (`x`, not `a.x`, `x::y`, `x!` or a struct field).
    plain: HashSet<usize>,
    /// Offsets of shorthand fields (`Point { x }`): renaming one would rename
    /// the field too.
    shorthand: HashSet<usize>,
    /// Byte ranges of macro arguments, which syn leaves unparsed.
    macros: Vec<(usize, usize)>,
}

fn offset(span: proc_macro2::Span) -> usize {
    span.byte_range().start
}

/// Every function in the file (free, inherent, trait default, nested), with
/// the facts `check_renames` needs.
fn fn_scopes(src: &str) -> Result<Vec<FnScope>, String> {
    struct Finder {
        scopes: Vec<FnScope>,
    }
    impl Finder {
        fn add(&mut self, span: proc_macro2::Span, sig: &syn::Signature, block: &syn::Block) {
            let range = span.byte_range();
            let mut scope = FnScope {
                start: range.start,
                end: range.end,
                ..FnScope::default()
            };
            let mut locals = Locals { scope: &mut scope };
            locals.visit_signature(sig);
            locals.visit_block(block);
            self.scopes.push(scope);
        }
    }
    impl<'ast> Visit<'ast> for Finder {
        fn visit_item_fn(&mut self, f: &'ast syn::ItemFn) {
            self.add(f.span(), &f.sig, &f.block);
            syn::visit::visit_item_fn(self, f);
        }
        fn visit_impl_item_fn(&mut self, f: &'ast syn::ImplItemFn) {
            self.add(f.span(), &f.sig, &f.block);
            syn::visit::visit_impl_item_fn(self, f);
        }
        fn visit_trait_item_fn(&mut self, f: &'ast syn::TraitItemFn) {
            if let Some(block) = &f.default {
                self.add(f.span(), &f.sig, block);
            }
            syn::visit::visit_trait_item_fn(self, f);
        }
    }
    /// Collects one function's locals; nested items are scopes of their own.
    struct Locals<'s> {
        scope: &'s mut FnScope,
    }
    impl<'ast> Visit<'ast> for Locals<'_> {
        fn visit_item(&mut self, _: &'ast syn::Item) {}
        fn visit_pat_ident(&mut self, p: &'ast syn::PatIdent) {
            self.scope.bindings.insert(p.ident.to_string());
            self.scope.plain.insert(offset(p.ident.span()));
            syn::visit::visit_pat_ident(self, p);
        }
        fn visit_field_pat(&mut self, p: &'ast syn::FieldPat) {
            if let (None, syn::Member::Named(id)) = (&p.colon_token, &p.member) {
                self.scope.shorthand.insert(offset(id.span()));
            }
            syn::visit::visit_field_pat(self, p);
        }
        fn visit_field_value(&mut self, v: &'ast syn::FieldValue) {
            if let (None, syn::Member::Named(id)) = (&v.colon_token, &v.member) {
                self.scope.shorthand.insert(offset(id.span()));
            }
            syn::visit::visit_field_value(self, v);
        }
        fn visit_expr_path(&mut self, e: &'ast syn::ExprPath) {
            let path = &e.path;
            if e.qself.is_none() && path.leading_colon.is_none() && path.segments.len() == 1 {
                let seg = &path.segments[0];
                if seg.arguments.is_none() {
                    self.scope.plain.insert(offset(seg.ident.span()));
                }
            }
            syn::visit::visit_expr_path(self, e);
        }
        fn visit_macro(&mut self, m: &'ast syn::Macro) {
            let range = m.delimiter.span().join().byte_range();
            self.scope.macros.push((range.start, range.end));
        }
    }

    let file = syn::parse_file(src).map_err(|e| format!("renames need a file that parses: {e}"))?;
    let mut finder = Finder { scopes: Vec::new() };
    finder.visit_file(&file);
    drop(file);
    proc_macro2::extra::invalidate_current_thread_spans();
    Ok(finder.scopes)
}

/// The innermost function containing `at`.
fn scope_at(scopes: &[FnScope], at: usize) -> Option<usize> {
    scopes
        .iter()
        .enumerate()
        .filter(|(_, s)| s.start <= at && at < s.end)
        .min_by_key(|(_, s)| s.end - s.start)
        .map(|(k, _)| k)
}

/// A rename is a clean substitution when, inside one function: the old name
/// is a local of that function (a `let`, parameter, closure or pattern
/// binding) and starts lower-case; every binding and plain use of it gets the
/// same new name (in macro arguments too), and nothing else does (fields,
/// methods, paths and shorthand fields keep theirs); the new name does not
/// already appear in the function, so it captures nothing; and format strings
/// that capture the old name (`"{old}"`) are updated to match.
fn check_renames(before: &str, old: &[&Tok], new: &[&Tok], changed: &[usize]) -> Result<(), String> {
    let scopes = fn_scopes(before)?;
    let mut maps: HashMap<usize, BTreeMap<&str, &str>> = HashMap::new();
    for &k in changed.iter().filter(|&&k| old[k].kind == TokKind::Ident) {
        let (a, b) = (old[k], new[k]);
        let s = scope_at(&scopes, a.start).ok_or_else(|| {
            format!("`{}` at line {} was renamed outside a function", a.text, a.line)
        })?;
        let scope = &scopes[s];
        if !scope.bindings.contains(a.text) || !a.text.starts_with(|c: char| c.is_lowercase() || c == '_') {
            return Err(format!(
                "`{}` at line {} is not a local variable; only locals may be renamed",
                a.text, a.line
            ));
        }
        if scope.shorthand.contains(&a.start) {
            return Err(format!(
                "`{}` at line {} is a shorthand field (`{{ {} }}`); renaming it renames the field",
                a.text, a.line, a.text
            ));
        }
        if !renamable(scope, old, k) {
            return Err(format!(
                "`{}` at line {} is a field, method or path here, not the local",
                a.text, a.line
            ));
        }
        let map = maps.entry(s).or_default();
        match map.get(a.text) {
            Some(&n) if n != b.text => {
                return Err(format!(
                    "`{}` got two new names in one function: `{n}` and `{}` (line {})",
                    a.text, b.text, a.line
                ))
            }
            _ => {
                map.insert(a.text, b.text);
            }
        }
    }

    for (&s, map) in &maps {
        let scope = &scopes[s];
        let mut targets: HashSet<&str> = HashSet::new();
        for (from, to) in map {
            if !targets.insert(*to) {
                return Err(format!("two locals were renamed to `{to}`"));
            }
            if map.contains_key(to) {
                return Err(format!("`{from}` was renamed to `{to}`, which is renamed too"));
            }
        }
        for k in (0..old.len()).filter(|&k| scope_at(&scopes, old[k].start) == Some(s)) {
            let (a, b) = (old[k], new[k]);
            if a.kind == TokKind::Ident && targets.contains(a.text) {
                return Err(format!(
                    "`{}` already appears at line {} of this function; the new name would clash",
                    a.text, a.line
                ));
            }
            if a.kind == TokKind::Ident && map.contains_key(a.text) && renamable(scope, old, k) && a.text == b.text {
                return Err(format!(
                    "`{}` was renamed in its function but not at line {}",
                    a.text, a.line
                ));
            }
            if a.kind == TokKind::Literal {
                // Only a string handed to a macro can be a format string.
                let in_macro = scope.macros.iter().any(|&(s, e)| s < a.start && a.start < e);
                let expected = if in_macro && a.text.contains('"') {
                    rename_captures(a.text, map)
                } else {
                    a.text.to_string()
                };
                if expected != b.text {
                    return Err(format!(
                        "the literal at line {} may only change where it captures a renamed local",
                        a.line
                    ));
                }
            }
        }
    }

    // A changed literal outside any renaming function is a plain code change.
    for &k in changed.iter().filter(|&&k| old[k].kind == TokKind::Literal) {
        let in_map = scope_at(&scopes, old[k].start).is_some_and(|s| maps.contains_key(&s));
        if !in_map {
            return Err(format!("the literal at line {} changed", old[k].line));
        }
    }
    Ok(())
}

/// The identifier at `k` is the local itself: a binding or plain use syn
/// saw, or, inside macro arguments, a bare name (no `.` before it, no `::`,
/// `!` or `:` after it).
fn renamable(scope: &FnScope, toks: &[&Tok], k: usize) -> bool {
    let at = toks[k].start;
    if scope.shorthand.contains(&at) {
        return false;
    }
    if scope.plain.contains(&at) {
        return true;
    }
    let in_macro = scope.macros.iter().any(|&(s, e)| s < at && at < e);
    let prev = k.checked_sub(1).map_or("", |p| toks[p].text);
    let next = toks.get(k + 1).map_or("", |t| t.text);
    in_macro && prev != "." && !matches!(next, ":" | "!")
}

/// `text` (a string literal) with every format capture of a renamed name
/// (`{old}` or `{old:..}`) given its new name; `{{` stays literal.
fn rename_captures(text: &str, map: &BTreeMap<&str, &str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find('{') {
        out.push_str(&rest[..=pos]);
        rest = &rest[pos + 1..];
        if let Some(after) = rest.strip_prefix('{') {
            out.push('{');
            rest = after;
            continue;
        }
        let name_len = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(rest.len());
        let (name, tail) = rest.split_at(name_len);
        match map.get(name) {
            Some(new) if tail.starts_with('}') || tail.starts_with(':') => out.push_str(new),
            _ => out.push_str(name),
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

fn short(text: &str) -> String {
    let one_line = text.lines().next().unwrap_or("");
    if one_line.chars().count() > 60 {
        format!("{}...", one_line.chars().take(60).collect::<String>())
    } else {
        one_line.to_string()
    }
}

/// Which doc comment kinds (outer, inner) sit in each gap between code
/// tokens. Gap `k` is the space before the `k`-th code token.
fn doc_gaps(toks: &[Tok]) -> BTreeMap<usize, (bool, bool)> {
    let mut gaps: BTreeMap<usize, (bool, bool)> = BTreeMap::new();
    let mut code = 0usize;
    for t in toks {
        match t.kind {
            TokKind::Comment(CommentKind::OuterDoc) => gaps.entry(code).or_default().0 = true,
            TokKind::Comment(CommentKind::InnerDoc) => gaps.entry(code).or_default().1 = true,
            TokKind::Comment(CommentKind::Plain) => {}
            _ => code += 1,
        }
    }
    gaps
}

/// A doc comment may be reworded or deleted, but not added where there was
/// none: on a statement it is a lint warning, before nothing it is an error.
fn check_doc_placement(old: &[Tok], new: &[Tok]) -> Result<(), String> {
    let before = doc_gaps(old);
    for (gap, (outer, inner)) in doc_gaps(new) {
        let had = before.get(&gap).copied().unwrap_or_default();
        if (outer && !had.0) || (inner && !had.1) {
            let line = new.iter().filter(|t| t.is_code()).nth(gap).map_or(0, |t| t.line);
            return Err(format!(
                "a doc comment was added where there was none (before line {line} of the edited file)"
            ));
        }
    }
    Ok(())
}

/// The text lines of one doc comment, without the comment markers.
fn doc_lines(text: &str) -> Vec<&str> {
    if let Some(rest) = text.strip_prefix("///").or_else(|| text.strip_prefix("//!")) {
        return vec![rest.strip_suffix('\r').unwrap_or(rest)];
    }
    let inner = text.get(3..text.len().saturating_sub(2)).unwrap_or("");
    inner
        .lines()
        .map(|l| {
            let t = l.trim_start();
            t.strip_prefix('*').unwrap_or(t)
        })
        .collect()
}

/// The fenced code blocks inside doc comments, fence lines included, in
/// order. Rustdoc compiles them as tests.
fn doc_code_blocks(toks: &[Tok]) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut open: Option<String> = None;
    let mut code_seen = 0usize;
    let mut run_gap = usize::MAX;
    for t in toks {
        let TokKind::Comment(kind) = t.kind else {
            code_seen += 1;
            continue;
        };
        if kind == CommentKind::Plain {
            continue;
        }
        // A run of doc comments ends at the next code token; an unclosed fence ends with it.
        if code_seen != run_gap {
            blocks.extend(open.take());
            run_gap = code_seen;
        }
        for line in doc_lines(t.text) {
            let trimmed = line.trim_start();
            let fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
            if let Some(mut block) = open.take() {
                block.push_str(line.trim_end());
                if fence {
                    blocks.push(block);
                } else {
                    block.push('\n');
                    open = Some(block);
                }
            } else if fence {
                open = Some(format!("{}\n", line.trim_end()));
            }
        }
    }
    blocks.extend(open);
    blocks
}

/// Every comment line that carries a `+spec` annotation, in order.
fn spec_annotations(toks: &[Tok]) -> Vec<String> {
    comment_lines(toks).filter(|l| l.contains("+spec")).map(|l| l.trim().to_string()).collect()
}

/// How many TODO, FIXME, XXX and HACK markers the comments hold.
fn marker_count(toks: &[Tok]) -> usize {
    comment_lines(toks)
        .flat_map(|l| l.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|w| matches!(*w, "TODO" | "FIXME" | "XXX" | "HACK"))
        .count()
}

fn comment_lines<'a>(toks: &'a [Tok<'a>]) -> impl Iterator<Item = &'a str> + 'a {
    toks.iter()
        .filter(|t| !t.is_code())
        .flat_map(|t| t.text.lines())
}

/// If the original parses as a Rust file, the edited one must too: a doc
/// comment left with no item after it is a hard error that tokens cannot
/// show. Files that never parsed alone (`include!` fragments) skip this.
pub fn still_parses(before: &str, after: &str) -> Result<(), String> {
    let result = if syn::parse_file(before).is_ok() {
        syn::parse_file(after).map(|_| ()).map_err(|e| {
            format!(
                "the edited file no longer parses as Rust (line {}): {e}",
                e.span().start().line
            )
        })
    } else {
        Ok(())
    };
    // Each parse adds the file to proc-macro2's per-thread source map; free it.
    proc_macro2::extra::invalidate_current_thread_spans();
    result
}

/// Compares this lexer's code tokens with proc-macro2's on one file. Any
/// difference means the guard cannot be trusted there, so the file is left
/// alone.
pub fn cross_check(src: &str) -> Result<(), String> {
    let ours: Vec<(String, usize)> = lex(src)?
        .iter()
        .filter(|t| t.is_code())
        .map(|t| (t.text.to_string(), t.line))
        .collect();
    let theirs = {
        let stream: Result<TokenStream, _> = src.parse();
        let flat = stream.map(|s| {
            let mut out = Vec::new();
            flatten(s, src, &mut out);
            out
        });
        proc_macro2::extra::invalidate_current_thread_spans();
        flat.map_err(|e| format!("proc-macro2 cannot lex the file: {e}"))?
    };
    for (k, (text, line)) in ours.iter().enumerate() {
        match theirs.get(k) {
            Some(t) if t == text => {}
            Some(t) => {
                return Err(format!(
                    "the comment guard and proc-macro2 disagree at line {line}: `{}` vs `{}`",
                    short(text),
                    short(t)
                ))
            }
            None => return Err(format!("proc-macro2 ends early, at line {line}")),
        }
    }
    if theirs.len() > ours.len() {
        return Err(format!(
            "proc-macro2 sees {} more tokens at the end",
            theirs.len() - ours.len()
        ));
    }
    Ok(())
}

/// proc-macro2's tokens as plain text in source order: groups become their
/// delimiters, a lifetime is one token as in `lex`, and the `#[doc = ".."]`
/// attributes that doc comments turn into are dropped (written-out
/// `#[doc = ..]` attributes stay).
fn flatten(stream: TokenStream, src: &str, out: &mut Vec<String>) {
    let mut iter = stream.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Punct(p) if p.as_char() == '#' && from_comment(p.span(), src) => {
                if matches!(iter.peek(), Some(TokenTree::Punct(q)) if q.as_char() == '!') {
                    iter.next();
                }
                iter.next();
            }
            TokenTree::Punct(p) if p.as_char() == '\'' && p.spacing() == Spacing::Joint => {
                match iter.peek() {
                    Some(TokenTree::Ident(id)) => {
                        out.push(format!("'{id}"));
                        iter.next();
                    }
                    _ => out.push("'".to_string()),
                }
            }
            TokenTree::Punct(p) => out.push(p.as_char().to_string()),
            TokenTree::Ident(id) => out.push(id.to_string()),
            TokenTree::Literal(l) => out.push(l.to_string()),
            TokenTree::Group(g) => {
                let (open, close) = match g.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                if !open.is_empty() {
                    out.push(open.to_string());
                }
                flatten(g.stream(), src, out);
                if !close.is_empty() {
                    out.push(close.to_string());
                }
            }
        }
    }
}

/// proc-macro2 gives the tokens it makes from a doc comment the comment's span.
fn from_comment(span: proc_macro2::Span, src: &str) -> bool {
    src.get(span.byte_range().start..)
        .is_some_and(|s| s.starts_with("//") || s.starts_with("/*"))
}

/// The file with every comment removed: a block comment becomes one space so
/// the tokens around it stay apart.
pub fn strip_comments(src: &str) -> Result<String, String> {
    let toks = lex(src)?;
    let mut out = String::with_capacity(src.len());
    let mut pos = 0usize;
    for t in toks.iter().filter(|t| !t.is_code()) {
        out.push_str(&src[pos..t.start]);
        if t.text.starts_with("/*") {
            out.push(' ');
        }
        pos = t.start + t.text.len();
    }
    out.push_str(&src[pos..]);
    Ok(out)
}

/// The dry-run test of the guard on one real file: the lexer agrees with
/// proc-macro2, and deleting every comment keeps the code tokens and the
/// ability to parse. If either fails, the guard has a gap on this file.
pub fn self_test(src: &str) -> Result<(), String> {
    cross_check(src)?;
    let stripped = strip_comments(src)?;
    let old = lex(src)?;
    let new = lex(&stripped)?;
    compare_code(src, &old, &new, false).map_err(|e| format!("removing the comments was seen as a code change: {e}"))?;
    still_parses(src, &stripped).map_err(|e| format!("removing the comments broke parsing: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn code_texts(src: &str) -> Vec<String> {
        lex(src).unwrap().iter().filter(|t| t.is_code()).map(|t| t.text.to_string()).collect()
    }

    #[test]
    fn comment_markers_inside_strings_chars_and_raw_strings_are_code() {
        let src = r####"let a = "// not a comment"; let b = '/'; let c = r#"/* "x" */"#; // real
let d = b'"'; let e = '\''; let f = 'x; /* real */ let g = 1;"####;
        let texts = code_texts(src);
        assert!(texts.contains(&r#""// not a comment""#.to_string()));
        assert!(texts.contains(&"'/'".to_string()));
        assert!(texts.contains(&r###"r#"/* "x" */"#"###.to_string()));
        assert!(texts.contains(&r#"b'"'"#.to_string()));
        assert!(texts.contains(&r"'\''".to_string()));
        assert!(texts.contains(&"'x".to_string()));
        assert!(!texts.iter().any(|t| t.contains("real")));
    }

    #[test]
    fn numbers_lex_like_proc_macro2() {
        assert_eq!(code_texts("0..10"), ["0", ".", ".", "10"]);
        assert_eq!(code_texts("1.5e-3f64"), ["1.5e-3f64"]);
        assert_eq!(code_texts("2.max(1)"), ["2", ".", "max", "(", "1", ")"]);
        assert_eq!(code_texts("t.0.1"), ["t", ".", "0.1"]);
        assert_eq!(code_texts("0x1e-5"), ["0x1e", "-", "5"]);
    }

    #[test]
    fn a_comment_only_rewrite_passes_and_moves_line_numbers() {
        let before = "// one\n// two\n// three\nfn a() {}\n\n/// Docs.\nfn b() {}\n";
        let after = "// shorter\nfn a() {}\n\n/// Better docs.\nfn b() {}\n";
        let map = check_style_only(before, after, true).unwrap();
        assert_eq!(map.map(4), 2);
        assert_eq!(map.map(7), 5);
    }

    #[test]
    fn a_changed_code_token_is_rejected() {
        let err = check_style_only("fn a() { 1 }\n", "fn a() { 2 }\n", true).unwrap_err();
        assert!(err.contains("changed"), "{err}");
        let err = check_style_only("fn a() { b() }\n", "fn a() { c() }\n", true).unwrap_err();
        assert!(err.contains("not a local"), "{err}");
    }

    #[test]
    fn a_local_renamed_everywhere_in_its_function_passes() {
        let before = "fn area(w: f32, h: f32) -> f32 {\n    let tr = w * h;\n    println!(\"{tr}\");\n    tr\n}\n";
        let after = "fn area(w: f32, h: f32) -> f32 {\n    let total_area = w * h;\n    println!(\"{total_area}\");\n    total_area\n}\n";
        check_style_only(before, after, true).unwrap();
        // A comments-only run takes no renames at all.
        assert!(check_style_only(before, after, false).is_err());
    }

    #[test]
    fn renames_that_would_change_meaning_are_rejected() {
        // A field access keeps its name even where a local shares it.
        let field = "struct P { x: f32 }\nfn f(p: P) -> f32 { let x = 1.0; p.x + x }\n";
        let renamed_field = "struct P { x: f32 }\nfn f(p: P) -> f32 { let offset = 1.0; p.offset + offset }\n";
        assert!(check_style_only(field, renamed_field, true).is_err());
        // A shorthand field would be renamed with the local.
        let shorthand = "struct P { x: f32 }\nfn f() -> P { let x = 1.0; P { x } }\n";
        let renamed_shorthand = "struct P { x: f32 }\nfn f() -> P { let offset = 1.0; P { offset } }\n";
        assert!(check_style_only(shorthand, renamed_shorthand, true).is_err());
        // Every use must follow, or the old uses would name something else.
        let partial = "fn f() -> i32 { let a = 1; let b = a + a; b }\n";
        let renamed_partly = "fn f() -> i32 { let total = 1; let b = total + a; b }\n";
        assert!(check_style_only(partial, renamed_partly, true).is_err());
        // The new name must not already be in use.
        let clash = "fn f() -> i32 { let a = 1; let b = 2; a + b }\n";
        let renamed_into_clash = "fn f() -> i32 { let b = 1; let b = 2; b + b }\n";
        assert!(check_style_only(clash, renamed_into_clash, true).is_err());
    }

    #[test]
    fn rustfmt_may_move_trailing_commas() {
        let before = "fn f() { g(a, b) }\n";
        let after = "fn f() {\n    g(\n        a,\n        b,\n    )\n}\n";
        let map = check_layout_only(before, after).unwrap();
        assert_eq!(map.map(1), 1);
        assert!(check_layout_only("fn f() { g(a, b) }\n", "fn f() { g(b, a) }\n").is_err());
    }

    #[test]
    fn joining_two_symbols_is_rejected() {
        let err = check_style_only("let f = a - /* c */ > b;\n", "let f = a ->b;\n", true).unwrap_err();
        assert!(err.contains("spacing"), "{err}");
    }

    #[test]
    fn a_new_doc_comment_is_rejected_but_rewording_one_is_not() {
        assert!(check_style_only("// note\nfn a() {}\n", "/// note\nfn a() {}\n", true).is_err());
        assert!(check_style_only("/// note\nfn a() {}\n", "/// A better note.\nfn a() {}\n", true).is_ok());
    }

    #[test]
    fn doctests_spec_annotations_and_todos_must_survive() {
        let doc = "/// Adds.\n///\n/// ```\n/// assert_eq!(1 + 1, 2);\n/// ```\nfn add() {}\n";
        let changed = "/// Adds.\n///\n/// ```\n/// assert_eq!(2, 2);\n/// ```\nfn add() {}\n";
        assert!(check_style_only(doc, changed, true).is_err());
        assert!(check_style_only("// +spec:box:1a - x\nfn a() {}\n", "fn a() {}\n", true).is_err());
        assert!(check_style_only("// TODO: later\nfn a() {}\n", "fn a() {}\n", true).is_err());
    }

    #[test]
    fn a_comment_turned_into_a_doc_comment_with_no_item_after_it_is_rejected() {
        let before = "fn a() {\n    let x = 1;\n    // end\n}\n";
        let after = "fn a() {\n    let x = 1;\n    /// end\n}\n";
        assert!(check_style_only(before, after, true).is_err());
    }

    #[test]
    fn a_finding_on_a_deleted_comment_line_moves_to_the_code_below() {
        let before = "fn a() {}\n// gone\n// gone\nfn b() {}\n";
        let after = "fn a() {}\nfn b() {}\n";
        let map = check_style_only(before, after, true).unwrap();
        assert_eq!(map.map(1), 1);
        assert_eq!(map.map(3), 2);
        assert_eq!(map.map(4), 2);
    }

    #[test]
    fn the_guard_agrees_with_proc_macro2_and_survives_stripping() {
        let src = "//! Module.\n#![allow(dead_code)]\n/// Doc.\n#[doc = \"explicit\"]\npub struct S<'a> { x: &'a str }\nfn f() -> u8 { let c = '\\u{41}'; let s = r#\"/*\"#; 1u8 }\n";
        self_test(src).unwrap();
    }
}
