//! The lenient HTML tokenizer (`azul_core::xml::html`): the HTML Living Standard's
//! tokenization (13.2.5) as a state machine, one method per state. It never fails: what the
//! standard calls a parse error is read the way a browser reads it.
//!
//! What it simplifies: a state that only reports a parse error is folded into its
//! neighbour; script data's escape states (`<!--` inside a `<script>`) are not modelled (a
//! script is raw text); character references are decoded per text run / attribute value
//! ([`super::decode_character_references`], the same algorithm); NUL and the other C0
//! controls but tab, line feed and form feed are dropped; CR LF and a lone CR are a line
//! feed (13.2.3.5).

use alloc::{
    borrow::Cow,
    string::{String, ToString},
    vec::Vec,
};

use super::{clean_text, is_html_space, CharRefMode};

/// How the tokenizer reads what follows a start tag: the TREE CONSTRUCTION decides it
/// (13.2.6.2, the generic raw text and RCDATA element parsing algorithms - an SVG
/// `<style>` is markup, an HTML one is not) and hands it to
/// [`HtmlTokenizer::set_text_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextMode {
    /// Markup (13.2.5.1 "data state").
    Data,
    /// Text with its character references decoded, up to `</name` (13.2.5.2 "RCDATA
    /// state": `title`, `textarea`).
    RcData(&'static str),
    /// Text as written, up to `</name` (13.2.5.3 "RAWTEXT state", 13.2.5.4 "script data
    /// state": `style`, `script`, `xmp`, `iframe`, `noembed`, `noframes`).
    RawText(&'static str),
    /// Text as written, to the end of the input (13.2.5.5 "PLAINTEXT state").
    PlainText,
}

/// A `<!DOCTYPE ...>` (13.2.5.53 - 13.2.5.68): what decides quirks mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Doctype {
    /// The name, lower-cased (`html`); empty when there is none.
    pub name: String,
    /// The public identifier (`-//W3C//DTD XHTML 1.0 Transitional//EN`), if written.
    pub public_id: Option<String>,
    /// The system identifier (`http://www.w3.org/TR/xhtml1/...`), if written.
    pub system_id: Option<String>,
    /// The doctype was malformed: the document is in quirks mode.
    pub force_quirks: bool,
}

/// A token of HTML markup ([`HtmlTokenizer`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HtmlToken<'a> {
    /// `<name a=1 b="2" c>` / `<name ... />`: the name and the attribute
    /// names lower-cased, the values decoded (an attribute written twice
    /// keeps its first value; a bare one is `""`).
    StartTag {
        name: String,
        attributes: Vec<(String, String)>,
        self_closing: bool,
    },
    /// `</name>`, the name lower-cased (its attributes are read and dropped).
    EndTag { name: String },
    /// Text: its character references decoded (the raw text of a `<style>`
    /// or a `<script>` as written), CR LF as LF, stray control characters
    /// dropped.
    Text(Cow<'a, str>),
    /// `<!-- .. -->`: the text between the markers; also a bogus comment (`<!x>`,
    /// `<?x>`, `</ x>`, Word's `<![if ...]>`, a CDATA section outside SVG / MathML): its
    /// text up to the `>`.
    Comment(&'a str),
    /// `<![CDATA[ .. ]]>` in SVG / MathML ([`HtmlTokenizer::set_cdata_allowed`]): its
    /// text.
    Cdata(&'a str),
    /// `<!DOCTYPE ...>`.
    Doctype(Doctype),
}

/// The states of 13.2.5 the tokenizer is in between two bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// 13.2.5.1
    Data,
    /// 13.2.5.2 (and its end tag states 13.2.5.9 - 13.2.5.11)
    RcData(&'static str),
    /// 13.2.5.3 / 13.2.5.4 (and their end tag states)
    RawText(&'static str),
    /// 13.2.5.5
    PlainText,
    /// 13.2.5.6
    TagOpen,
    /// 13.2.5.7
    EndTagOpen,
    /// 13.2.5.8
    TagName,
    /// 13.2.5.32
    BeforeAttributeName,
    /// 13.2.5.33
    AttributeName,
    /// 13.2.5.34
    AfterAttributeName,
    /// 13.2.5.35
    BeforeAttributeValue,
    /// 13.2.5.36 / 13.2.5.37 (the quote)
    AttributeValueQuoted(u8),
    /// 13.2.5.38
    AttributeValueUnquoted,
    /// 13.2.5.39
    AfterAttributeValueQuoted,
    /// 13.2.5.40
    SelfClosingStartTag,
    /// 13.2.5.41
    BogusComment,
    /// 13.2.5.42
    MarkupDeclarationOpen,
    /// 13.2.5.43 - 13.2.5.52 (comment start ... comment end bang)
    Comment,
    /// 13.2.5.53 - 13.2.5.68 (doctype ... bogus doctype)
    Doctype,
    /// 13.2.5.69
    CdataSection,
}

/// What a state's method did.
enum Next<'a> {
    /// A token is complete (the method has set the next state).
    Token(HtmlToken<'a>),
    /// The method moved to another state.
    Continue,
    /// The end of the input.
    End,
}

/// The tag being read.
#[derive(Debug, Clone, Default)]
struct Tag {
    end: bool,
    name: String,
    attributes: Vec<(String, String)>,
    self_closing: bool,
    /// The name of the attribute being read.
    attribute: String,
    /// Where the value being read starts.
    value_start: usize,
}

impl Tag {
    /// The attribute being read is complete with `value`; a second one of the same name is
    /// dropped (13.2.5.33: "the new attribute must be removed from the token").
    fn commit(&mut self, value: String) {
        let name = core::mem::take(&mut self.attribute);
        if !self.attributes.iter().any(|(k, _)| *k == name) {
            self.attributes.push((name, value));
        }
    }
}

/// `true` if `s` (starting with `<`) starts markup (13.2.5.6 "tag open state": an ASCII
/// letter, `!`, `/` or `?` follows; `</` needs one more byte). Any other `<` is text.
fn starts_markup(s: &str) -> bool {
    let b = s.as_bytes();
    b.first() == Some(&b'<')
        && match b.get(1) {
            Some(c) if c.is_ascii_alphabetic() => true,
            Some(b'!' | b'?') => true,
            Some(b'/') => b.get(2).is_some(),
            _ => false,
        }
}

/// How long the text run at the start of `rest` is: up to the next `<` that
/// starts markup (a `<` that does not is part of the text).
fn text_run_len(rest: &str) -> usize {
    let mut from = 0;
    while let Some(found) = rest.get(from..).and_then(|r| r.find('<')) {
        let lt = from + found;
        if lt > 0 && starts_markup(&rest[lt..]) {
            return lt;
        }
        from = lt + 1;
    }
    rest.len()
}

/// Where the raw text in `rest` ends: at the "appropriate end tag" (13.2.5.11): `</name`
/// (in any case) followed by white space, `/` or `>`; else at the end.
fn raw_text_end(rest: &str, name: &str) -> usize {
    let bytes = rest.as_bytes();
    let n = name.len();
    let mut from = 0;
    while let Some(found) = rest.get(from..).and_then(|r| r.find("</")) {
        let lt = from + found;
        let after = lt + 2;
        if bytes
            .get(after..after + n)
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(name.as_bytes()))
        {
            if let Some(&b) = bytes.get(after + n) {
                if is_html_space(b) || b == b'/' || b == b'>' {
                    return lt;
                }
            }
        }
        from = after;
    }
    rest.len()
}

/// The length of the name at the start of `s`: up to white space, `/`, `>` (and `=` for an
/// attribute's name after its first character).
fn name_len(s: &str, stop_at_equals: bool) -> usize {
    s.bytes()
        .enumerate()
        .take_while(|&(i, b)| {
            !(is_html_space(b) || b == b'/' || b == b'>' || (stop_at_equals && i > 0 && b == b'='))
        })
        .count()
}

/// `s` without its leading HTML white space.
fn skip_space(s: &str) -> &str {
    s.trim_start_matches(['\t', '\n', '\u{C}', '\r', ' '])
}

/// A doctype's identifier at the start of `s` (after its keyword and white space): the
/// quoted text and what follows its closing quote; `Err` with what was read when the quote
/// is missing (13.2.5.57 "missing-quote-before-doctype-public-identifier") or not closed
/// before the `>` (13.2.5.59 "abrupt-doctype-public-identifier") - quirks.
fn doctype_identifier(s: &str) -> Result<(String, &str), String> {
    let s = skip_space(s);
    let Some(&quote) = s.as_bytes().first().filter(|q| matches!(**q, b'"' | b'\'')) else {
        return Err(String::new());
    };
    let body = &s[1..];
    match body.find(char::from(quote)) {
        Some(end) => Ok((body[..end].to_string(), &body[end + 1..])),
        None => Err(body.to_string()),
    }
}

/// `text` (between `<!DOCTYPE` and `>`) as a [`Doctype`]; `eof`: the input ended in it.
fn parse_doctype(text: &str, eof: bool) -> Doctype {
    let mut doctype = Doctype {
        force_quirks: eof,
        ..Doctype::default()
    };
    let s = skip_space(text);
    let len = s.bytes().take_while(|&b| !is_html_space(b)).count();
    doctype.name = s[..len].to_ascii_lowercase();
    if doctype.name.is_empty() {
        // 13.2.5.54 "missing-doctype-name"
        doctype.force_quirks = true;
        return doctype;
    }
    let rest = skip_space(&s[len..]);
    if rest.is_empty() {
        return doctype;
    }
    let keyword = |k: &str| {
        rest.len() >= k.len() && rest.as_bytes()[..k.len()].eq_ignore_ascii_case(k.as_bytes())
    };
    if keyword("public") {
        match doctype_identifier(&rest[6..]) {
            Ok((public, after)) => {
                doctype.public_id = Some(public);
                let after = skip_space(after);
                if !after.is_empty() {
                    match doctype_identifier(after) {
                        Ok((system, _)) => doctype.system_id = Some(system),
                        Err(partial) => {
                            if !partial.is_empty() {
                                doctype.system_id = Some(partial);
                            }
                            doctype.force_quirks = true;
                        }
                    }
                }
            }
            Err(partial) => {
                if !partial.is_empty() {
                    doctype.public_id = Some(partial);
                }
                doctype.force_quirks = true;
            }
        }
    } else if keyword("system") {
        match doctype_identifier(&rest[6..]) {
            Ok((system, _)) => doctype.system_id = Some(system),
            Err(partial) => {
                if !partial.is_empty() {
                    doctype.system_id = Some(partial);
                }
                doctype.force_quirks = true;
            }
        }
    } else {
        // 13.2.5.56 "invalid-character-sequence-after-doctype-name": a bogus doctype
        doctype.force_quirks = true;
    }
    doctype
}

/// The lenient HTML tokenizer: an iterator of [`HtmlToken`]s that never fails.
///
/// The tree construction drives it, as in the standard: after a start tag it sets the
/// [`TextMode`] the element's content is read in ([`Self::set_text_mode`]; on its own the
/// tokenizer reads every element's content as markup), and allows CDATA sections inside
/// SVG / MathML ([`Self::set_cdata_allowed`]). A tag cut off by the end of the input is
/// dropped, as a browser drops it.
#[derive(Debug, Clone)]
pub struct HtmlTokenizer<'a> {
    src: &'a str,
    pos: usize,
    state: State,
    /// Where a bogus comment's text starts.
    start: usize,
    tag: Tag,
    cdata_allowed: bool,
}

impl<'a> HtmlTokenizer<'a> {
    /// A tokenizer over `src` (a leading byte order mark is skipped).
    #[must_use]
    pub fn new(src: &'a str) -> Self {
        Self {
            src: src.strip_prefix('\u{FEFF}').unwrap_or(src),
            pos: 0,
            state: State::Data,
            start: 0,
            tag: Tag::default(),
            cdata_allowed: false,
        }
    }

    /// How the content after the start tag just read is tokenized (the tree construction
    /// says: [`super::TreeBuilder::text_mode_for`]).
    pub fn set_text_mode(&mut self, mode: TextMode) {
        self.state = match mode {
            TextMode::Data => State::Data,
            TextMode::RcData(name) => State::RcData(name),
            TextMode::RawText(name) => State::RawText(name),
            TextMode::PlainText => State::PlainText,
        };
    }

    /// Whether `<![CDATA[` starts a CDATA section (the adjusted current node is an SVG /
    /// MathML element: [`super::TreeBuilder::in_foreign_content`]) or a bogus comment.
    pub fn set_cdata_allowed(&mut self, allowed: bool) {
        self.cdata_allowed = allowed;
    }

    fn byte(&self, at: usize) -> Option<u8> {
        self.src.as_bytes().get(at).copied()
    }

    /// The tag is complete: its token (13.2.5.8 "emit the current tag token"; an end tag's
    /// attributes and self-closing flag are dropped).
    fn emit_tag(&mut self) -> Next<'a> {
        self.state = State::Data;
        let tag = core::mem::take(&mut self.tag);
        Next::Token(if tag.end {
            HtmlToken::EndTag { name: tag.name }
        } else {
            HtmlToken::StartTag {
                name: tag.name,
                attributes: tag.attributes,
                self_closing: tag.self_closing,
            }
        })
    }

    // ---- the states ----

    /// 13.2.5.1: text up to the next `<` that starts markup.
    fn data(&mut self) -> Next<'a> {
        let src = self.src;
        let rest = &src[self.pos..];
        if rest.is_empty() {
            return Next::End;
        }
        if starts_markup(rest) {
            self.pos += 1;
            self.state = State::TagOpen;
            return Next::Continue;
        }
        let len = text_run_len(rest);
        self.pos += len;
        Next::Token(HtmlToken::Text(clean_text(
            &rest[..len],
            Some(CharRefMode::HtmlText),
        )))
    }

    /// 13.2.5.2 / 13.2.5.3 / 13.2.5.4: text up to the element's own end tag, which is then
    /// read as an end tag (13.2.5.11: white space, `/` or `>` after its name).
    fn raw_text(&mut self, name: &'static str, decoded: bool) -> Next<'a> {
        let src = self.src;
        let rest = &src[self.pos..];
        if rest.is_empty() {
            return Next::End;
        }
        let len = raw_text_end(rest, name);
        if len > 0 {
            self.pos += len;
            let mode = decoded.then_some(CharRefMode::HtmlText);
            return Next::Token(HtmlToken::Text(clean_text(&rest[..len], mode)));
        }
        // At `</name` and white space, `/` or `>`.
        self.tag = Tag {
            end: true,
            name: name.to_string(),
            ..Tag::default()
        };
        self.pos += 2 + name.len();
        match self.byte(self.pos) {
            Some(b'/') => {
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
                Next::Continue
            }
            Some(b'>') => {
                self.pos += 1;
                self.emit_tag()
            }
            _ => {
                self.pos += 1;
                self.state = State::BeforeAttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.5: the rest of the input is text.
    fn plain_text(&mut self) -> Next<'a> {
        let src = self.src;
        let rest = &src[self.pos..];
        if rest.is_empty() {
            return Next::End;
        }
        self.pos = src.len();
        Next::Token(HtmlToken::Text(clean_text(rest, None)))
    }

    /// 13.2.5.6: after `<` (which [`starts_markup`]).
    fn tag_open(&mut self) -> Next<'a> {
        match self.byte(self.pos) {
            Some(b'!') => {
                self.pos += 1;
                self.state = State::MarkupDeclarationOpen;
            }
            Some(b'/') => {
                self.pos += 1;
                self.state = State::EndTagOpen;
            }
            Some(b'?') => {
                // "unexpected-question-mark-instead-of-tag-name": a bogus comment, the `?`
                // its first character (`<?xml ...?>`, `<?php ...?>`).
                self.start = self.pos;
                self.state = State::BogusComment;
            }
            Some(c) if c.is_ascii_alphabetic() => {
                self.tag = Tag::default();
                self.state = State::TagName;
            }
            _ => {
                // "invalid-first-character-of-tag-name": the `<` is text.
                self.state = State::Data;
                return Next::Token(HtmlToken::Text(Cow::Borrowed("<")));
            }
        }
        Next::Continue
    }

    /// 13.2.5.7: after `</`.
    fn end_tag_open(&mut self) -> Next<'a> {
        match self.byte(self.pos) {
            Some(c) if c.is_ascii_alphabetic() => {
                self.tag = Tag {
                    end: true,
                    ..Tag::default()
                };
                self.state = State::TagName;
            }
            Some(b'>') => {
                // "missing-end-tag-name": `</>` is nothing.
                self.pos += 1;
                self.state = State::Data;
            }
            None => {
                self.state = State::Data;
                return Next::Token(HtmlToken::Text(Cow::Borrowed("</")));
            }
            Some(_) => {
                // "invalid-first-character-of-tag-name": `</ x>` is a bogus comment.
                self.start = self.pos;
                self.state = State::BogusComment;
            }
        }
        Next::Continue
    }

    /// 13.2.5.8: the tag's name, lower-cased.
    fn tag_name(&mut self) -> Next<'a> {
        let src = self.src;
        let rest = &src[self.pos..];
        let len = name_len(rest, false);
        self.tag.name.push_str(&rest[..len].to_ascii_lowercase());
        self.pos += len;
        match self.byte(self.pos) {
            None => Next::End,
            Some(b'/') => {
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
                Next::Continue
            }
            Some(b'>') => {
                self.pos += 1;
                self.emit_tag()
            }
            Some(_) => {
                self.pos += 1;
                self.state = State::BeforeAttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.32
    fn before_attribute_name(&mut self) -> Next<'a> {
        while self.byte(self.pos).is_some_and(is_html_space) {
            self.pos += 1;
        }
        match self.byte(self.pos) {
            None => Next::End,
            Some(b'/') => {
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
                Next::Continue
            }
            Some(b'>') => {
                self.pos += 1;
                self.emit_tag()
            }
            Some(_) => {
                // A first `=` is part of the name ("unexpected-equals-sign-before-
                // attribute-name").
                self.tag.attribute.clear();
                self.state = State::AttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.33: the attribute's name, lower-cased.
    fn attribute_name(&mut self) -> Next<'a> {
        let src = self.src;
        let rest = &src[self.pos..];
        let len = name_len(rest, true);
        self.tag
            .attribute
            .push_str(&rest[..len].to_ascii_lowercase());
        self.pos += len;
        match self.byte(self.pos) {
            None => Next::End,
            Some(b'=') => {
                self.pos += 1;
                self.state = State::BeforeAttributeValue;
                Next::Continue
            }
            Some(_) => {
                self.state = State::AfterAttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.34
    fn after_attribute_name(&mut self) -> Next<'a> {
        while self.byte(self.pos).is_some_and(is_html_space) {
            self.pos += 1;
        }
        match self.byte(self.pos) {
            None => Next::End,
            Some(b'=') => {
                self.pos += 1;
                self.state = State::BeforeAttributeValue;
                Next::Continue
            }
            Some(b'/') => {
                self.tag.commit(String::new());
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
                Next::Continue
            }
            Some(b'>') => {
                self.tag.commit(String::new());
                self.pos += 1;
                self.emit_tag()
            }
            Some(_) => {
                // A bare attribute, then the next one.
                self.tag.commit(String::new());
                self.state = State::AttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.35
    fn before_attribute_value(&mut self) -> Next<'a> {
        while self.byte(self.pos).is_some_and(is_html_space) {
            self.pos += 1;
        }
        match self.byte(self.pos) {
            None => Next::End,
            Some(quote @ (b'"' | b'\'')) => {
                self.pos += 1;
                self.tag.value_start = self.pos;
                self.state = State::AttributeValueQuoted(quote);
                Next::Continue
            }
            Some(b'>') => {
                // "missing-attribute-value": the attribute is empty.
                self.tag.commit(String::new());
                self.pos += 1;
                self.emit_tag()
            }
            Some(_) => {
                self.tag.value_start = self.pos;
                self.state = State::AttributeValueUnquoted;
                Next::Continue
            }
        }
    }

    /// 13.2.5.36 / 13.2.5.37: up to the closing quote (`<`, `>` and `&` that starts no
    /// reference are characters).
    fn attribute_value_quoted(&mut self, quote: u8) -> Next<'a> {
        let src = self.src;
        let start = self.tag.value_start;
        let Some(len) = src[start..].find(char::from(quote)) else {
            // "eof-in-tag": the tag is dropped.
            self.pos = src.len();
            return Next::End;
        };
        let value = clean_text(&src[start..start + len], Some(CharRefMode::HtmlAttribute));
        self.tag.commit(value.into_owned());
        self.pos = start + len + 1;
        self.state = State::AfterAttributeValueQuoted;
        Next::Continue
    }

    /// 13.2.5.38: up to white space or `>` (quotes, `<`, `=` and `` ` `` are characters).
    fn attribute_value_unquoted(&mut self) -> Next<'a> {
        let src = self.src;
        let start = self.tag.value_start;
        let len = src[start..]
            .bytes()
            .take_while(|&b| !(is_html_space(b) || b == b'>'))
            .count();
        self.pos = start + len;
        let Some(next) = self.byte(self.pos) else {
            return Next::End;
        };
        let value = clean_text(&src[start..start + len], Some(CharRefMode::HtmlAttribute));
        self.tag.commit(value.into_owned());
        self.pos += 1;
        if next == b'>' {
            return self.emit_tag();
        }
        self.state = State::BeforeAttributeName;
        Next::Continue
    }

    /// 13.2.5.39
    fn after_attribute_value_quoted(&mut self) -> Next<'a> {
        match self.byte(self.pos) {
            None => Next::End,
            Some(b) if is_html_space(b) => {
                self.pos += 1;
                self.state = State::BeforeAttributeName;
                Next::Continue
            }
            Some(b'/') => {
                self.pos += 1;
                self.state = State::SelfClosingStartTag;
                Next::Continue
            }
            Some(b'>') => {
                self.pos += 1;
                self.emit_tag()
            }
            Some(_) => {
                // "missing-whitespace-between-attributes": the next one starts here.
                self.state = State::BeforeAttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.40: after `/` in a tag.
    fn self_closing_start_tag(&mut self) -> Next<'a> {
        match self.byte(self.pos) {
            None => Next::End,
            Some(b'>') => {
                self.tag.self_closing = true;
                self.pos += 1;
                self.emit_tag()
            }
            Some(_) => {
                // "unexpected-solidus-in-tag": the `/` is nothing.
                self.state = State::BeforeAttributeName;
                Next::Continue
            }
        }
    }

    /// 13.2.5.41: up to the next `>` (or the end).
    fn bogus_comment(&mut self) -> Next<'a> {
        let src = self.src;
        let end = src[self.pos..]
            .find('>')
            .map_or(src.len(), |i| self.pos + i);
        let text = &src[self.start..end];
        self.pos = (end + 1).min(src.len());
        self.state = State::Data;
        Next::Token(HtmlToken::Comment(text))
    }

    /// 13.2.5.42: after `<!`.
    fn markup_declaration_open(&mut self) -> Next<'a> {
        let src = self.src;
        let rest = src[self.pos..].as_bytes();
        if rest.starts_with(b"--") {
            self.pos += 2;
            self.state = State::Comment;
        } else if rest.len() >= 7 && rest[..7].eq_ignore_ascii_case(b"doctype") {
            self.pos += 7;
            self.state = State::Doctype;
        } else if rest.starts_with(b"[CDATA[") && self.cdata_allowed {
            self.pos += 7;
            self.state = State::CdataSection;
        } else {
            // "incorrectly-opened-comment" / "cdata-in-html-content": a bogus comment
            // (Word's `<![if !supportLists]>`, a CDATA section in HTML).
            self.start = self.pos;
            self.state = State::BogusComment;
        }
        Next::Continue
    }

    /// 13.2.5.43 - 13.2.5.52: after `<!--`, up to `-->` or `--!>` (`<!-->` and `<!--->`
    /// are empty comments).
    fn comment(&mut self) -> Next<'a> {
        let src = self.src;
        let body = &src[self.pos..];
        self.state = State::Data;
        if body.starts_with('>') {
            self.pos += 1;
            return Next::Token(HtmlToken::Comment(""));
        }
        if body.starts_with("->") {
            self.pos += 2;
            return Next::Token(HtmlToken::Comment(""));
        }
        let end = [
            body.find("-->").map(|i| (i, 3)),
            body.find("--!>").map(|i| (i, 4)),
        ]
        .into_iter()
        .flatten()
        .min();
        let (len, close) = end.unwrap_or((body.len(), 0));
        self.pos += len + close;
        Next::Token(HtmlToken::Comment(&body[..len]))
    }

    /// 13.2.5.53 - 13.2.5.68: after `<!DOCTYPE`, up to the next `>`.
    fn doctype(&mut self) -> Next<'a> {
        let src = self.src;
        let rest = &src[self.pos..];
        let end = rest.find('>');
        let text = &rest[..end.unwrap_or(rest.len())];
        self.pos = end.map_or(src.len(), |e| self.pos + e + 1);
        self.state = State::Data;
        Next::Token(HtmlToken::Doctype(parse_doctype(text, end.is_none())))
    }

    /// 13.2.5.69: after `<![CDATA[` in SVG / MathML, up to `]]>`.
    fn cdata_section(&mut self) -> Next<'a> {
        let src = self.src;
        let body = &src[self.pos..];
        let (len, close) = body.find("]]>").map_or((body.len(), 0), |i| (i, 3));
        self.pos += len + close;
        self.state = State::Data;
        Next::Token(HtmlToken::Cdata(&body[..len]))
    }
}

impl<'a> Iterator for HtmlTokenizer<'a> {
    type Item = HtmlToken<'a>;

    fn next(&mut self) -> Option<HtmlToken<'a>> {
        loop {
            let next = match self.state {
                State::Data => self.data(),
                State::RcData(name) => self.raw_text(name, true),
                State::RawText(name) => self.raw_text(name, false),
                State::PlainText => self.plain_text(),
                State::TagOpen => self.tag_open(),
                State::EndTagOpen => self.end_tag_open(),
                State::TagName => self.tag_name(),
                State::BeforeAttributeName => self.before_attribute_name(),
                State::AttributeName => self.attribute_name(),
                State::AfterAttributeName => self.after_attribute_name(),
                State::BeforeAttributeValue => self.before_attribute_value(),
                State::AttributeValueQuoted(quote) => self.attribute_value_quoted(quote),
                State::AttributeValueUnquoted => self.attribute_value_unquoted(),
                State::AfterAttributeValueQuoted => self.after_attribute_value_quoted(),
                State::SelfClosingStartTag => self.self_closing_start_tag(),
                State::BogusComment => self.bogus_comment(),
                State::MarkupDeclarationOpen => self.markup_declaration_open(),
                State::Comment => self.comment(),
                State::Doctype => self.doctype(),
                State::CdataSection => self.cdata_section(),
            };
            match next {
                Next::Token(token) => return Some(token),
                Next::Continue => {}
                Next::End => {
                    self.pos = self.src.len();
                    self.state = State::Data;
                    return None;
                }
            }
        }
    }
}
