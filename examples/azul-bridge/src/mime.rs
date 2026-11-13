//! A message's MIME structure as IMAP needs it: byte ranges of every part's header and body
//! (FETCH BODY[1.2.MIME], BODY[TEXT]), the raw header fields (ENVELOPE, HEADER.FIELDS), the
//! content type with its parameters, and address lists (ENVELOPE's from / to / cc ...).
//!
//! Everything is read from the exact bytes and nothing is decoded: IMAP hands out the raw
//! octets and the raw header values (encoded words stay encoded). The parser is strict about
//! what it trusts and lenient about what mail programs write: CRLF or bare LF line ends, a
//! missing final boundary, a part without a header. It never recurses deeper than
//! [`MAX_DEPTH`] nor makes more than [`MAX_PARTS`] parts, so a hostile message cannot exhaust
//! the stack or memory.

use std::ops::Range;

/// Multiparts and messages nest at most this deep; deeper ones are leaves.
pub const MAX_DEPTH: usize = 32;
/// One message makes at most this many parts; the rest of a multipart is left out.
pub const MAX_PARTS: usize = 2_000;

/// One part: its header (with the blank line after it) and its body, as ranges of the
/// message's bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub header: Range<usize>,
    pub body: Range<usize>,
    pub kind: Kind,
}

/// What a part holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Content of its own (text, a picture, an attachment).
    Leaf,
    /// `multipart/*`: its parts.
    Multipart(Vec<Node>),
    /// `message/rfc822` (or `message/global`): the message inside.
    Message(Box<Node>),
}

/// Where the header ends: `(end of the header with its blank line, start of the body)`. A
/// message without a blank line is all header.
#[must_use]
pub fn split(bytes: &[u8]) -> (usize, usize) {
    // A part that starts with its blank line has an empty header.
    if bytes.starts_with(b"\r\n") {
        return (2, 2);
    }
    if bytes.starts_with(b"\n") {
        return (1, 1);
    }
    let mut i = 0;
    while let Some(at) = bytes[i..].iter().position(|b| *b == b'\n') {
        let line_end = i + at + 1;
        // The next line is empty: the header ends after it.
        if bytes[line_end..].starts_with(b"\r\n") {
            return (line_end + 2, line_end + 2);
        }
        if bytes[line_end..].starts_with(b"\n") {
            return (line_end + 1, line_end + 1);
        }
        i = line_end;
    }
    (bytes.len(), bytes.len())
}

/// One header field: its name and its raw value (folded lines included, without the final
/// line end), and the whole field's bytes (name, colon, value, line ends).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field<'a> {
    pub name: &'a str,
    pub value: &'a [u8],
    pub raw: &'a [u8],
}

impl Field<'_> {
    /// The value unfolded (line ends before blanks removed) and trimmed.
    #[must_use]
    pub fn text(&self) -> String {
        unfold(self.value)
    }
}

/// The value with its folding undone, trimmed, as text (bytes that are not UTF-8 are
/// replaced: only what IMAP renders raw uses the bytes).
#[must_use]
pub fn unfold(value: &[u8]) -> String {
    let mut out = Vec::with_capacity(value.len());
    for &b in value {
        if b != b'\r' && b != b'\n' {
            out.push(b);
        }
    }
    String::from_utf8_lossy(&out).trim().to_string()
}

/// The fields of a header block.
#[must_use]
pub fn fields(header: &[u8]) -> Vec<Field<'_>> {
    let mut out: Vec<Field<'_>> = Vec::new();
    let mut start = 0;
    let mut lines: Vec<(usize, usize)> = Vec::new();
    while start < header.len() {
        let end = header[start..]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(header.len(), |at| start + at + 1);
        lines.push((start, end));
        start = end;
    }
    let mut i = 0;
    while i < lines.len() {
        let (first_start, first_end) = lines[i];
        let line = &header[first_start..first_end];
        if line == b"\r\n" || line == b"\n" {
            break;
        }
        let mut field_end = first_end;
        let mut j = i + 1;
        while j < lines.len() && matches!(header[lines[j].0], b' ' | b'\t') {
            field_end = lines[j].1;
            j += 1;
        }
        let raw = &header[first_start..field_end];
        if let Some(colon) = line.iter().position(|b| *b == b':') {
            if let Ok(name) = std::str::from_utf8(&line[..colon]) {
                let name_trimmed = name.trim_end();
                if !name_trimmed.is_empty() && !name_trimmed.contains(' ') {
                    let mut value = &header[first_start + colon + 1..field_end];
                    while value.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                        value = &value[..value.len() - 1];
                    }
                    out.push(Field {
                        name: name_trimmed,
                        value,
                        raw,
                    });
                }
            }
        }
        i = j;
    }
    out
}

/// The first field `name` (any case), unfolded.
#[must_use]
pub fn field(header: &[u8], name: &str) -> Option<String> {
    fields(header)
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))
        .map(|f| f.text())
}

/// A content type: `type/subtype` (lower case) and its parameters as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentType {
    pub kind: String,
    pub subtype: String,
    pub params: Vec<(String, String)>,
}

impl ContentType {
    /// RFC 2045's default: `text/plain; charset=us-ascii`.
    #[must_use]
    pub fn default_text() -> ContentType {
        ContentType {
            kind: String::from("text"),
            subtype: String::from("plain"),
            params: vec![(String::from("charset"), String::from("us-ascii"))],
        }
    }

    /// The parameter `name` (any case).
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    #[must_use]
    pub fn is_multipart(&self) -> bool {
        self.kind == "multipart"
    }

    /// `message/rfc822` or `message/global`: a whole message inside.
    #[must_use]
    pub fn is_message(&self) -> bool {
        self.kind == "message" && matches!(self.subtype.as_str(), "rfc822" | "global")
    }
}

/// Removes `(comments)` (nested, escapes kept out of them) outside quoted strings.
fn strip_comments(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    for c in value.chars() {
        if escaped {
            if depth == 0 {
                out.push(c);
            }
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted || depth > 0 => {
                escaped = true;
                if depth == 0 {
                    out.push(c);
                }
            }
            '"' if depth == 0 => {
                quoted = !quoted;
                out.push(c);
            }
            '(' if !quoted => depth += 1,
            ')' if !quoted && depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// `value; a=b; c="d e"` as the value and its parameters (names lower case, quoted values
/// unquoted).
#[must_use]
pub fn parse_params(text: &str) -> (String, Vec<(String, String)>) {
    let text = strip_comments(text);
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => {
                current.push(c);
                escaped = true;
            }
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            ';' if !quoted => parts.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    parts.push(current);
    let mut parts = parts.into_iter();
    let value = parts.next().unwrap_or_default().trim().to_string();
    let params = parts
        .filter_map(|part| {
            let (name, value) = part.split_once('=')?;
            let name = name.trim().to_ascii_lowercase();
            if name.is_empty() {
                return None;
            }
            Some((name, unquote(value.trim())))
        })
        .collect();
    (value, params)
}

/// A quoted string's content (escapes undone); anything else as it is.
#[must_use]
pub fn unquote(text: &str) -> String {
    let Some(inner) = text.strip_prefix('"').and_then(|t| t.strip_suffix('"')) else {
        return text.to_string();
    };
    let mut out = String::with_capacity(inner.len());
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else {
            out.push(c);
        }
    }
    out
}

/// The content type of a header block (the default when it has none or a broken one).
#[must_use]
pub fn content_type(header: &[u8]) -> ContentType {
    let Some(value) = field(header, "Content-Type") else {
        return ContentType::default_text();
    };
    let (kind, params) = parse_params(&value);
    let Some((main, sub)) = kind.split_once('/') else {
        return ContentType::default_text();
    };
    let (main, sub) = (main.trim().to_ascii_lowercase(), sub.trim().to_ascii_lowercase());
    if main.is_empty() || sub.is_empty() {
        return ContentType::default_text();
    }
    ContentType {
        kind: main,
        subtype: sub,
        params,
    }
}

/// Parses the message `bytes` into its parts.
#[must_use]
pub fn parse(bytes: &[u8]) -> Node {
    let mut budget = MAX_PARTS;
    parse_part(bytes, 0..bytes.len(), 0, &mut budget, false)
}

/// The part in `range`; `digest`: a part of `multipart/digest`, whose default type is
/// `message/rfc822`.
fn parse_part(
    bytes: &[u8],
    range: Range<usize>,
    depth: usize,
    budget: &mut usize,
    digest: bool,
) -> Node {
    let part = &bytes[range.clone()];
    let (header_end, body_start) = split(part);
    let header = range.start..range.start + header_end;
    let body = range.start + body_start..range.end;
    let has_type = field(&bytes[header.clone()], "Content-Type").is_some();
    let ct = if !has_type && digest {
        ContentType {
            kind: String::from("message"),
            subtype: String::from("rfc822"),
            params: Vec::new(),
        }
    } else {
        content_type(&bytes[header.clone()])
    };
    *budget = budget.saturating_sub(1);
    if depth >= MAX_DEPTH || *budget == 0 {
        return Node {
            header,
            body,
            kind: Kind::Leaf,
        };
    }
    let kind = if ct.is_multipart() {
        match ct.param("boundary").filter(|b| !b.is_empty()) {
            Some(boundary) => {
                let digest = ct.subtype == "digest";
                let mut children = Vec::new();
                for child in split_multipart(bytes, body.clone(), boundary.as_bytes()) {
                    if *budget == 0 {
                        break;
                    }
                    children.push(parse_part(bytes, child, depth + 1, budget, digest));
                }
                Kind::Multipart(children)
            }
            None => Kind::Leaf,
        }
    } else if ct.is_message() {
        Kind::Message(Box::new(parse_part(
            bytes,
            body.clone(),
            depth + 1,
            budget,
            false,
        )))
    } else {
        Kind::Leaf
    };
    Node { header, body, kind }
}

/// The ranges of a multipart body's parts: what lies between `--boundary` lines, without the
/// line end before each delimiter (RFC 2046 counts it to the delimiter). The preamble and the
/// epilogue are no parts; a missing close delimiter ends the last part at the body's end.
fn split_multipart(bytes: &[u8], body: Range<usize>, boundary: &[u8]) -> Vec<Range<usize>> {
    let mut delimiter = Vec::with_capacity(boundary.len() + 2);
    delimiter.extend_from_slice(b"--");
    delimiter.extend_from_slice(boundary);
    let mut parts = Vec::new();
    let mut part_start: Option<usize> = None;
    let mut line_start = body.start;
    while line_start < body.end {
        let line_end = bytes[line_start..body.end]
            .iter()
            .position(|b| *b == b'\n')
            .map_or(body.end, |at| line_start + at + 1);
        let line = &bytes[line_start..line_end];
        if line.starts_with(&delimiter) {
            let rest = &line[delimiter.len()..];
            let close = rest.starts_with(b"--");
            let only_padding = rest
                .iter()
                .all(|b| matches!(b, b' ' | b'\t' | b'\r' | b'\n'));
            if close || only_padding {
                if let Some(start) = part_start.take() {
                    // The line end before the delimiter belongs to it.
                    let mut end = line_start;
                    if end > start && bytes[end - 1] == b'\n' {
                        end -= 1;
                        if end > start && bytes[end - 1] == b'\r' {
                            end -= 1;
                        }
                    }
                    parts.push(start..end.max(start));
                }
                if close {
                    return parts;
                }
                part_start = Some(line_end);
            }
        }
        line_start = line_end;
    }
    if let Some(start) = part_start {
        parts.push(start..body.end);
    }
    parts
}

/// One entry of an address list, as IMAP's ENVELOPE lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Address {
    /// `name <mailbox@host>`: the display name as written (encoded words stay), the local
    /// part, the domain.
    Mailbox {
        name: Option<String>,
        mailbox: String,
        host: Option<String>,
    },
    /// `group:` starts a group...
    GroupStart(String),
    /// ...and `;` ends it.
    GroupEnd,
}

/// The pieces of one address while it is read.
#[derive(Default)]
struct Pending {
    phrase: String,
    spec: String,
    angle: Option<String>,
    comment: Option<String>,
}

impl Pending {
    fn finish(&mut self, out: &mut Vec<Address>) {
        let pending = std::mem::take(self);
        let (address, name) = match pending.angle {
            Some(angle) => {
                let phrase = pending.phrase.split_whitespace().collect::<Vec<_>>().join(" ");
                let name = if phrase.is_empty() {
                    pending.comment
                } else {
                    Some(phrase)
                };
                // `<@relay:user@host>`: an old source route goes.
                let address = angle.rsplit(':').next().unwrap_or_default().trim().to_string();
                (address, name)
            }
            None => (
                pending.spec.split_whitespace().collect::<Vec<_>>().join(""),
                pending.comment,
            ),
        };
        if address.is_empty() && name.is_none() {
            return;
        }
        let (mailbox, host) = match address.rsplit_once('@') {
            Some((local, domain)) => (local.to_string(), Some(domain.to_string())),
            None => (address, None),
        };
        out.push(Address::Mailbox {
            name: name.filter(|n| !n.is_empty()),
            mailbox,
            host,
        });
    }
}

/// An address list header's value (`From`, `To`, `Cc`, ...): mailboxes and groups, read
/// leniently (a missing `<>`, comments as names, a group without its `;`).
#[must_use]
pub fn parse_addresses(value: &str) -> Vec<Address> {
    let chars: Vec<char> = value.chars().collect();
    let mut out = Vec::new();
    let mut pending = Pending::default();
    let mut in_group = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    text.push(chars[i]);
                    i += 1;
                }
                if !pending.phrase.is_empty() {
                    pending.phrase.push(' ');
                }
                pending.phrase.push_str(&text);
                pending.spec.push('"');
                pending.spec.push_str(&text);
                pending.spec.push('"');
            }
            '(' => {
                let mut depth = 1;
                let mut text = String::new();
                i += 1;
                while i < chars.len() && depth > 0 {
                    match chars[i] {
                        '\\' if i + 1 < chars.len() => {
                            i += 1;
                            text.push(chars[i]);
                        }
                        '(' => {
                            depth += 1;
                            text.push('(');
                        }
                        ')' => {
                            depth -= 1;
                            if depth > 0 {
                                text.push(')');
                            }
                        }
                        other => text.push(other),
                    }
                    i += 1;
                }
                i -= 1;
                let text = text.trim().to_string();
                if pending.comment.is_none() && !text.is_empty() {
                    pending.comment = Some(text);
                }
            }
            '<' => {
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '>' {
                    text.push(chars[i]);
                    i += 1;
                }
                pending.angle = Some(text);
            }
            ',' => pending.finish(&mut out),
            ':' if pending.angle.is_none() && !in_group => {
                let name = pending.phrase.split_whitespace().collect::<Vec<_>>().join(" ");
                pending = Pending::default();
                out.push(Address::GroupStart(name));
                in_group = true;
            }
            ';' if in_group => {
                pending.finish(&mut out);
                out.push(Address::GroupEnd);
                in_group = false;
            }
            _ => {
                pending.phrase.push(c);
                pending.spec.push(c);
            }
        }
        i += 1;
    }
    pending.finish(&mut out);
    if in_group {
        out.push(Address::GroupEnd);
    }
    out
}

/// Lines of a body (what BODYSTRUCTURE gives a text part): its line ends, and one more for
/// a last line without one.
#[must_use]
pub fn line_count(body: &[u8]) -> usize {
    let ends = body.iter().filter(|b| **b == b'\n').count();
    if body.last().is_some_and(|b| *b != b'\n') {
        ends + 1
    } else {
        ends
    }
}

/// The part at IMAP's part number `path` (`[2, 1]` is `2.1`), and whether it is a message
/// (its header is a message's: `HEADER` / `TEXT` apply) - the whole message for `[]`.
#[must_use]
pub fn resolve<'a>(root: &'a Node, path: &[u32]) -> Option<&'a Node> {
    let mut current = root;
    let mut is_message = true;
    for &n in path {
        if n == 0 {
            return None;
        }
        let container: &Node = if is_message {
            current
        } else if let Kind::Message(inner) = &current.kind {
            &**inner
        } else {
            current
        };
        let container_is_message = is_message || matches!(current.kind, Kind::Message(_));
        match &container.kind {
            Kind::Multipart(children) => {
                current = children.get(n as usize - 1)?;
            }
            _ if container_is_message && n == 1 => {
                current = container;
            }
            _ => return None,
        }
        is_message = false;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE: &[u8] = b"From: Ada <ada@example.org>\r\n\
        Subject: Hi\r\n\
        \r\n\
        line one\r\n\
        line two\r\n";

    fn nested() -> Vec<u8> {
        b"From: ada@example.org\r\n\
          Content-Type: multipart/mixed; boundary=\"outer\"\r\n\
          \r\n\
          preamble\r\n\
          --outer\r\n\
          Content-Type: text/plain; charset=utf-8\r\n\
          \r\n\
          Hello.\r\n\
          --outer\r\n\
          Content-Type: multipart/alternative; boundary=inner\r\n\
          \r\n\
          --inner\r\n\
          \r\n\
          plain\r\n\
          --inner\r\n\
          Content-Type: text/html\r\n\
          \r\n\
          <p>html</p>\r\n\
          --inner--\r\n\
          --outer\r\n\
          Content-Type: message/rfc822\r\n\
          \r\n\
          Subject: inside\r\n\
          \r\n\
          inner body\r\n\
          --outer--\r\n\
          epilogue\r\n"
            .to_vec()
    }

    fn text<'a>(bytes: &'a [u8], range: &Range<usize>) -> &'a str {
        std::str::from_utf8(&bytes[range.clone()]).unwrap()
    }

    #[test]
    fn the_header_ends_at_the_first_empty_line_with_crlf_or_lf() {
        assert_eq!(split(SIMPLE), (44, 44));
        assert_eq!(&SIMPLE[..44], b"From: Ada <ada@example.org>\r\nSubject: Hi\r\n\r\n");
        assert_eq!(split(b"A: b\n\nbody"), (6, 6));
        assert_eq!(split(b"\r\nbody"), (2, 2));
        assert_eq!(split(b"A: b\r\n"), (6, 6));
    }

    #[test]
    fn folded_fields_keep_their_raw_bytes_and_unfold_for_reading() {
        let header = b"Subject: one\r\n two\r\nX-Empty:\r\nbroken line\r\nTo: a@b\r\n\r\n";
        let got = fields(header);
        let names: Vec<&str> = got.iter().map(|f| f.name).collect();
        assert_eq!(names, vec!["Subject", "X-Empty", "To"]);
        assert_eq!(got[0].raw, b"Subject: one\r\n two\r\n");
        assert_eq!(got[0].text(), "one two");
        assert_eq!(got[1].text(), "");
        assert_eq!(field(header, "subject").as_deref(), Some("one two"));
        assert_eq!(field(header, "cc"), None);
    }

    #[test]
    fn content_types_read_parameters_quotes_and_comments() {
        let ct = content_type(
            b"Content-Type: Text/HTML (a comment); charset=\"utf-8\"; name=\"a \\\"b\\\".html\"\r\n\r\n",
        );
        assert_eq!((ct.kind.as_str(), ct.subtype.as_str()), ("text", "html"));
        assert_eq!(ct.param("CHARSET"), Some("utf-8"));
        assert_eq!(ct.param("name"), Some("a \"b\".html"));
        assert_eq!(content_type(b"Subject: x\r\n\r\n"), ContentType::default_text());
        assert_eq!(content_type(b"Content-Type: garbage\r\n\r\n"), ContentType::default_text());
    }

    #[test]
    fn a_nested_message_splits_into_numbered_parts() {
        let bytes = nested();
        let root = parse(&bytes);
        let Kind::Multipart(parts) = &root.kind else {
            panic!("{root:?}");
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(text(&bytes, &parts[0].body), "Hello.");
        let Kind::Multipart(alternatives) = &parts[1].kind else {
            panic!("{:?}", parts[1]);
        };
        assert_eq!(text(&bytes, &alternatives[0].header), "\r\n");
        assert_eq!(text(&bytes, &alternatives[0].body), "plain");
        assert_eq!(text(&bytes, &alternatives[1].body), "<p>html</p>");
        let Kind::Message(inner) = &parts[2].kind else {
            panic!("{:?}", parts[2]);
        };
        assert_eq!(text(&bytes, &inner.header), "Subject: inside\r\n\r\n");
        assert_eq!(text(&bytes, &inner.body), "inner body");
        // IMAP's part numbers.
        assert_eq!(resolve(&root, &[]), Some(&root));
        assert_eq!(resolve(&root, &[1]).map(|n| text(&bytes, &n.body)), Some("Hello."));
        assert_eq!(resolve(&root, &[2, 2]).map(|n| text(&bytes, &n.body)), Some("<p>html</p>"));
        assert_eq!(resolve(&root, &[3]), Some(&parts[2]));
        assert_eq!(resolve(&root, &[3, 1]).map(|n| text(&bytes, &n.body)), Some("inner body"));
        assert_eq!(resolve(&root, &[4]), None);
        assert_eq!(resolve(&root, &[1, 1]), None);
        assert_eq!(resolve(&root, &[0]), None);
        // A message that is not multipart has a part 1: its body.
        let simple = parse(SIMPLE);
        assert_eq!(resolve(&simple, &[1]).map(|n| text(SIMPLE, &n.body)), Some("line one\r\nline two\r\n"));
        assert_eq!(resolve(&simple, &[2]), None);
    }

    #[test]
    fn a_missing_close_delimiter_ends_the_last_part_at_the_end() {
        let bytes = b"Content-Type: multipart/mixed; boundary=b\n\n--b\n\none\n--b\n\ntwo\n".to_vec();
        let root = parse(&bytes);
        let Kind::Multipart(parts) = &root.kind else {
            panic!("{root:?}");
        };
        assert_eq!(parts.len(), 2);
        assert_eq!(text(&bytes, &parts[0].body), "one");
        assert_eq!(text(&bytes, &parts[1].body), "two\n");
    }

    #[test]
    fn hostile_nesting_stops_at_the_depth_limit() {
        let mut bytes = Vec::new();
        for _ in 0..200 {
            bytes.extend_from_slice(b"Content-Type: message/rfc822\r\n\r\n");
        }
        bytes.extend_from_slice(b"Subject: deep\r\n\r\nbottom\r\n");
        let mut node = parse(&bytes);
        let mut depth = 0;
        while let Kind::Message(inner) = node.kind {
            node = *inner;
            depth += 1;
        }
        assert_eq!(depth, MAX_DEPTH);
        let mut wide = b"Content-Type: multipart/mixed; boundary=x\r\n\r\n".to_vec();
        for _ in 0..(MAX_PARTS + 100) {
            wide.extend_from_slice(b"--x\r\n\r\np\r\n");
        }
        let root = parse(&wide);
        let Kind::Multipart(parts) = &root.kind else {
            panic!("not multipart");
        };
        assert!(parts.len() < MAX_PARTS, "{}", parts.len());
    }

    #[test]
    fn address_lists_give_names_mailboxes_hosts_and_groups() {
        let mailbox = |name: Option<&str>, mailbox: &str, host: Option<&str>| Address::Mailbox {
            name: name.map(str::to_string),
            mailbox: mailbox.to_string(),
            host: host.map(str::to_string),
        };
        assert_eq!(
            parse_addresses("Ada Lovelace <ada@example.org>, ben@example.net"),
            vec![
                mailbox(Some("Ada Lovelace"), "ada", Some("example.org")),
                mailbox(None, "ben", Some("example.net")),
            ]
        );
        assert_eq!(
            parse_addresses("\"Lovelace, Ada\" <ada@example.org>"),
            vec![mailbox(Some("Lovelace, Ada"), "ada", Some("example.org"))]
        );
        assert_eq!(
            parse_addresses("cy@example.com (Cy Young)"),
            vec![mailbox(Some("Cy Young"), "cy", Some("example.com"))]
        );
        assert_eq!(
            parse_addresses("=?utf-8?q?J=C3=B6rg?= <joerg@example.de>"),
            vec![mailbox(Some("=?utf-8?q?J=C3=B6rg?="), "joerg", Some("example.de"))]
        );
        assert_eq!(
            parse_addresses("Team: a@x.org, b@y.org;, c@z.org"),
            vec![
                Address::GroupStart("Team".to_string()),
                mailbox(None, "a", Some("x.org")),
                mailbox(None, "b", Some("y.org")),
                Address::GroupEnd,
                mailbox(None, "c", Some("z.org")),
            ]
        );
        assert_eq!(
            parse_addresses("undisclosed-recipients:;"),
            vec![
                Address::GroupStart("undisclosed-recipients".to_string()),
                Address::GroupEnd
            ]
        );
        assert_eq!(parse_addresses("<@relay.org:ada@example.org>"), vec![mailbox(None, "ada", Some("example.org"))]);
        assert_eq!(parse_addresses(""), vec![]);
    }

    #[test]
    fn text_lines_count_a_last_line_without_its_end() {
        assert_eq!(line_count(b""), 0);
        assert_eq!(line_count(b"a\r\nb\r\n"), 2);
        assert_eq!(line_count(b"a\r\nb"), 2);
    }
}
