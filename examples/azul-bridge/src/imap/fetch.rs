//! FETCH's message data: IMAP strings, ENVELOPE, BODY / BODYSTRUCTURE and body sections, all
//! from the raw bytes through [`crate::mime`].

use crate::mime::{self, Address, Kind, Node};

use super::parse::{Section, SectionText};

/// Writes `bytes` as an IMAP string: quoted when it is short printable ASCII, a literal
/// otherwise (line ends, 8-bit text, NUL, long values).
pub fn string(out: &mut Vec<u8>, bytes: &[u8]) {
    let quotable = bytes.len() <= 1024
        && bytes
            .iter()
            .all(|b| (0x20..0x7f).contains(b));
    if quotable {
        out.push(b'"');
        for &b in bytes {
            if b == b'"' || b == b'\\' {
                out.push(b'\\');
            }
            out.push(b);
        }
        out.push(b'"');
    } else {
        literal(out, bytes);
    }
}

/// Writes `bytes` as a literal: `{n}` CRLF and the bytes.
pub fn literal(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(format!("{{{}}}\r\n", bytes.len()).as_bytes());
    out.extend_from_slice(bytes);
}

/// `NIL` or the string.
pub fn nstring(out: &mut Vec<u8>, value: Option<&[u8]>) {
    match value {
        Some(bytes) => string(out, bytes),
        None => out.extend_from_slice(b"NIL"),
    }
}

/// The header field `name`'s raw value (unfolded, trimmed) as bytes, if present and not empty.
fn header_value(header: &[u8], name: &str) -> Option<Vec<u8>> {
    let field = mime::fields(header)
        .into_iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))?;
    let mut value: Vec<u8> = field
        .value
        .iter()
        .copied()
        .filter(|b| *b != b'\r' && *b != b'\n')
        .collect();
    while value.first().is_some_and(|b| *b == b' ' || *b == b'\t') {
        value.remove(0);
    }
    while value.last().is_some_and(|b| *b == b' ' || *b == b'\t') {
        value.pop();
    }
    (!value.is_empty()).then_some(value)
}

/// An address list header as ENVELOPE's list, `None` when the header is missing or empty.
fn address_list(out: &mut Vec<u8>, header: &[u8], name: &str) -> bool {
    let Some(value) = header_value(header, name) else {
        return false;
    };
    let addresses = mime::parse_addresses(&String::from_utf8_lossy(&value));
    if addresses.is_empty() {
        return false;
    }
    out.push(b'(');
    for address in &addresses {
        out.push(b'(');
        match address {
            Address::Mailbox {
                name,
                mailbox,
                host,
            } => {
                nstring(out, name.as_deref().map(str::as_bytes));
                out.extend_from_slice(b" NIL ");
                string(out, mailbox.as_bytes());
                out.push(b' ');
                nstring(out, host.as_deref().map(str::as_bytes));
            }
            Address::GroupStart(group) => {
                out.extend_from_slice(b"NIL NIL ");
                string(out, group.as_bytes());
                out.extend_from_slice(b" NIL");
            }
            Address::GroupEnd => out.extend_from_slice(b"NIL NIL NIL NIL"),
        }
        out.push(b')');
    }
    out.push(b')');
    true
}

/// ENVELOPE of a message's header block.
pub fn envelope(out: &mut Vec<u8>, header: &[u8]) {
    out.push(b'(');
    nstring(out, header_value(header, "Date").as_deref());
    out.push(b' ');
    nstring(out, header_value(header, "Subject").as_deref());
    out.push(b' ');
    // From, then Sender and Reply-To (each From's when it is missing or empty).
    let mut from = Vec::new();
    let has_from = address_list(&mut from, header, "From");
    for name in ["From", "Sender", "Reply-To"] {
        let mut list = Vec::new();
        if name != "From" && address_list(&mut list, header, name) {
            out.extend_from_slice(&list);
        } else if has_from {
            out.extend_from_slice(&from);
        } else {
            out.extend_from_slice(b"NIL");
        }
        out.push(b' ');
    }
    for name in ["To", "Cc", "Bcc"] {
        let mut list = Vec::new();
        if address_list(&mut list, header, name) {
            out.extend_from_slice(&list);
        } else {
            out.extend_from_slice(b"NIL");
        }
        out.push(b' ');
    }
    nstring(out, header_value(header, "In-Reply-To").as_deref());
    out.push(b' ');
    nstring(out, header_value(header, "Message-ID").as_deref());
    out.push(b')');
}

/// `("name" "value" ...)` or `NIL`.
fn params(out: &mut Vec<u8>, params: &[(String, String)]) {
    if params.is_empty() {
        out.extend_from_slice(b"NIL");
        return;
    }
    out.push(b'(');
    for (i, (name, value)) in params.iter().enumerate() {
        if i > 0 {
            out.push(b' ');
        }
        string(out, name.to_ascii_uppercase().as_bytes());
        out.push(b' ');
        string(out, value.as_bytes());
    }
    out.push(b')');
}

/// The extension data a BODYSTRUCTURE part ends with: disposition, language, location.
fn disposition_language_location(out: &mut Vec<u8>, header: &[u8]) {
    out.push(b' ');
    match mime::field(header, "Content-Disposition").filter(|v| !v.is_empty()) {
        Some(value) => {
            let (kind, ps) = mime::parse_params(&value);
            out.push(b'(');
            string(out, kind.to_ascii_uppercase().as_bytes());
            out.push(b' ');
            params(out, &ps);
            out.push(b')');
        }
        None => out.extend_from_slice(b"NIL"),
    }
    out.push(b' ');
    match mime::field(header, "Content-Language").filter(|v| !v.is_empty()) {
        Some(value) => {
            let tags: Vec<&str> = value
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .collect();
            if tags.len() == 1 {
                string(out, tags[0].as_bytes());
            } else {
                out.push(b'(');
                for (i, tag) in tags.iter().enumerate() {
                    if i > 0 {
                        out.push(b' ');
                    }
                    string(out, tag.as_bytes());
                }
                out.push(b')');
            }
        }
        None => out.extend_from_slice(b"NIL"),
    }
    out.push(b' ');
    nstring(
        out,
        mime::field(header, "Content-Location")
            .filter(|v| !v.is_empty())
            .as_deref()
            .map(str::as_bytes),
    );
}

/// BODY (`extensible: false`) or BODYSTRUCTURE of the part `node` of `bytes`.
pub fn body_structure(out: &mut Vec<u8>, bytes: &[u8], node: &Node, extensible: bool) {
    let header = &bytes[node.header.clone()];
    let ct = mime::content_type(header);
    out.push(b'(');
    if let Kind::Multipart(children) = &node.kind {
        if children.is_empty() {
            // A multipart with no parts is described as an empty text part.
            out.extend_from_slice(b"\"TEXT\" \"PLAIN\" NIL NIL NIL \"7BIT\" 0 0)");
            return;
        }
        for child in children {
            body_structure(out, bytes, child, extensible);
        }
        out.push(b' ');
        string(out, ct.subtype.to_ascii_uppercase().as_bytes());
        if extensible {
            out.push(b' ');
            params(out, &ct.params);
            disposition_language_location(out, header);
        }
        out.push(b')');
        return;
    }
    string(out, ct.kind.to_ascii_uppercase().as_bytes());
    out.push(b' ');
    string(out, ct.subtype.to_ascii_uppercase().as_bytes());
    out.push(b' ');
    params(out, &ct.params);
    out.push(b' ');
    nstring(out, header_value(header, "Content-ID").as_deref());
    out.push(b' ');
    nstring(out, header_value(header, "Content-Description").as_deref());
    out.push(b' ');
    let encoding = mime::field(header, "Content-Transfer-Encoding")
        .map(|e| e.to_ascii_uppercase())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| String::from("7BIT"));
    string(out, encoding.as_bytes());
    let body = &bytes[node.body.clone()];
    out.extend_from_slice(format!(" {}", body.len()).as_bytes());
    match &node.kind {
        Kind::Message(inner) => {
            out.push(b' ');
            envelope(out, &bytes[inner.header.clone()]);
            out.push(b' ');
            body_structure(out, bytes, inner, extensible);
            out.extend_from_slice(format!(" {}", mime::line_count(body)).as_bytes());
        }
        _ if ct.kind == "text" => {
            out.extend_from_slice(format!(" {}", mime::line_count(body)).as_bytes());
        }
        _ => {}
    }
    if extensible {
        // MD5: never computed.
        out.extend_from_slice(b" NIL");
        disposition_language_location(out, header);
    }
    out.push(b')');
}

/// The header fields of `header` whose names are (`keep`) or are not in `names`, with the
/// blank line after them.
fn header_fields(header: &[u8], names: &[String], keep: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for field in mime::fields(header) {
        let listed = names.iter().any(|n| n.eq_ignore_ascii_case(field.name));
        if listed == keep {
            out.extend_from_slice(field.raw);
            if !field.raw.ends_with(b"\n") {
                out.extend_from_slice(b"\r\n");
            }
        }
    }
    out.extend_from_slice(b"\r\n");
    out
}

/// The bytes of `section` of the message `bytes` (parsed as `root`); `None` for a part that
/// is not there (the response gives an empty string then).
#[must_use]
pub fn section_bytes(bytes: &[u8], root: &Node, section: &Section) -> Option<Vec<u8>> {
    let node = mime::resolve(root, &section.path)?;
    // HEADER, TEXT and the field lists apply to a message: the whole one, or one inside.
    let message: Option<&Node> = if section.path.is_empty() {
        Some(root)
    } else if let Kind::Message(inner) = &node.kind {
        Some(&**inner)
    } else {
        None
    };
    Some(match &section.text {
        None if section.path.is_empty() => bytes.to_vec(),
        None => bytes[node.body.clone()].to_vec(),
        Some(SectionText::Mime) => bytes[node.header.clone()].to_vec(),
        Some(SectionText::Header) => bytes[message?.header.clone()].to_vec(),
        Some(SectionText::Text) => bytes[message?.body.clone()].to_vec(),
        Some(SectionText::HeaderFields(names)) => {
            header_fields(&bytes[message?.header.clone()], names, true)
        }
        Some(SectionText::HeaderFieldsNot(names)) => {
            header_fields(&bytes[message?.header.clone()], names, false)
        }
    })
}

/// Whether a section needs only the header block (a header fetch reads only the start of a
/// big message).
#[must_use]
pub fn header_only(section: &Section) -> bool {
    section.path.is_empty()
        && matches!(
            section.text,
            Some(SectionText::Header | SectionText::HeaderFields(_) | SectionText::HeaderFieldsNot(_))
        )
}

/// `<start.length>` applied: the octets from `start`, at most `length` of them.
#[must_use]
pub fn partial(bytes: &[u8], start: u64, length: u64) -> &[u8] {
    let start = usize::try_from(start).unwrap_or(usize::MAX).min(bytes.len());
    let end = start
        .saturating_add(usize::try_from(length).unwrap_or(usize::MAX))
        .min(bytes.len());
    &bytes[start..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(out: &[u8]) -> String {
        String::from_utf8_lossy(out).into_owned()
    }

    const PLAIN: &[u8] = b"Date: Thu, 01 Oct 2026 08:30:00 +0000\r\n\
        From: Ada Lovelace <ada@example.org>\r\n\
        To: ben@example.net, \"Cy, C.\" <cy@example.com>\r\n\
        Subject: =?utf-8?q?Gr=C3=BC=C3=9Fe?=\r\n\
        Message-ID: <m1@example.org>\r\n\
        \r\n\
        Hello\r\nBen\r\n";

    #[test]
    fn strings_are_quoted_when_they_can_be_and_literals_otherwise() {
        let mut out = Vec::new();
        string(&mut out, b"plain \"quoted\" back\\slash");
        assert_eq!(text(&out), "\"plain \\\"quoted\\\" back\\\\slash\"");
        let mut out = Vec::new();
        string(&mut out, "Grüße".as_bytes());
        assert_eq!(out, b"{7}\r\nGr\xc3\xbc\xc3\x9fe");
        let mut out = Vec::new();
        string(&mut out, b"two\r\nlines");
        assert_eq!(text(&out), "{10}\r\ntwo\r\nlines");
        let mut out = Vec::new();
        nstring(&mut out, None);
        assert_eq!(out, b"NIL");
    }

    #[test]
    fn the_envelope_lists_raw_values_and_falls_back_to_from() {
        let mut out = Vec::new();
        let (end, _) = mime::split(PLAIN);
        envelope(&mut out, &PLAIN[..end]);
        assert_eq!(
            text(&out),
            "(\"Thu, 01 Oct 2026 08:30:00 +0000\" \"=?utf-8?q?Gr=C3=BC=C3=9Fe?=\" \
             ((\"Ada Lovelace\" NIL \"ada\" \"example.org\")) \
             ((\"Ada Lovelace\" NIL \"ada\" \"example.org\")) \
             ((\"Ada Lovelace\" NIL \"ada\" \"example.org\")) \
             ((NIL NIL \"ben\" \"example.net\")(\"Cy, C.\" NIL \"cy\" \"example.com\")) \
             NIL NIL NIL \"<m1@example.org>\")"
        );
        let mut out = Vec::new();
        envelope(&mut out, b"\r\n");
        assert_eq!(text(&out), "(NIL NIL NIL NIL NIL NIL NIL NIL NIL NIL)");
    }

    #[test]
    fn a_plain_message_has_a_text_structure_with_size_and_lines() {
        let root = mime::parse(PLAIN);
        let mut out = Vec::new();
        body_structure(&mut out, PLAIN, &root, false);
        assert_eq!(
            text(&out),
            "(\"TEXT\" \"PLAIN\" (\"CHARSET\" \"us-ascii\") NIL NIL \"7BIT\" 12 2)"
        );
        let mut out = Vec::new();
        body_structure(&mut out, PLAIN, &root, true);
        assert_eq!(
            text(&out),
            "(\"TEXT\" \"PLAIN\" (\"CHARSET\" \"us-ascii\") NIL NIL \"7BIT\" 12 2 NIL NIL NIL NIL)"
        );
    }

    #[test]
    fn a_multipart_with_an_attachment_and_a_message_is_described_part_by_part() {
        let bytes: &[u8] = b"Content-Type: multipart/mixed; boundary=b\r\n\
            \r\n\
            --b\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            Content-Transfer-Encoding: quoted-printable\r\n\
            \r\n\
            Hi=\r\n\
            --b\r\n\
            Content-Type: application/pdf; name=\"plan.pdf\"\r\n\
            Content-Disposition: attachment; filename=\"plan.pdf\"\r\n\
            Content-Transfer-Encoding: base64\r\n\
            \r\n\
            JVBERi0=\r\n\
            --b\r\n\
            Content-Type: message/rfc822\r\n\
            \r\n\
            Subject: inner\r\n\
            \r\n\
            x\r\n\
            --b--\r\n";
        let root = mime::parse(bytes);
        let mut out = Vec::new();
        body_structure(&mut out, bytes, &root, true);
        assert_eq!(
            text(&out),
            "((\"TEXT\" \"PLAIN\" (\"CHARSET\" \"utf-8\") NIL NIL \"QUOTED-PRINTABLE\" 3 1 NIL NIL NIL NIL)\
             (\"APPLICATION\" \"PDF\" (\"NAME\" \"plan.pdf\") NIL NIL \"BASE64\" 8 NIL \
             (\"ATTACHMENT\" (\"FILENAME\" \"plan.pdf\")) NIL NIL)\
             (\"MESSAGE\" \"RFC822\" NIL NIL NIL \"7BIT\" 19 \
             (NIL \"inner\" NIL NIL NIL NIL NIL NIL NIL NIL) \
             (\"TEXT\" \"PLAIN\" (\"CHARSET\" \"us-ascii\") NIL NIL \"7BIT\" 1 1 NIL NIL NIL NIL) 3 NIL NIL NIL NIL) \
             \"MIXED\" (\"BOUNDARY\" \"b\") NIL NIL NIL)"
        );
        let mut out = Vec::new();
        body_structure(&mut out, bytes, &root, false);
        assert!(text(&out).ends_with(" \"MIXED\")"), "{}", text(&out));
    }

    #[test]
    fn sections_give_headers_texts_field_lists_and_parts() {
        let root = mime::parse(PLAIN);
        let section = |path: Vec<u32>, text: Option<SectionText>| Section { path, text };
        assert_eq!(section_bytes(PLAIN, &root, &section(vec![], None)).unwrap(), PLAIN);
        assert_eq!(
            section_bytes(PLAIN, &root, &section(vec![], Some(SectionText::Text))).unwrap(),
            b"Hello\r\nBen\r\n"
        );
        assert_eq!(
            section_bytes(PLAIN, &root, &section(vec![1], None)).unwrap(),
            b"Hello\r\nBen\r\n"
        );
        let fields = section_bytes(
            PLAIN,
            &root,
            &section(
                vec![],
                Some(SectionText::HeaderFields(vec!["SUBJECT".into(), "MESSAGE-ID".into()])),
            ),
        )
        .unwrap();
        assert_eq!(
            text(&fields),
            "Subject: =?utf-8?q?Gr=C3=BC=C3=9Fe?=\r\nMessage-ID: <m1@example.org>\r\n\r\n"
        );
        let not = section_bytes(
            PLAIN,
            &root,
            &section(vec![], Some(SectionText::HeaderFieldsNot(vec!["TO".into(), "FROM".into(), "DATE".into()]))),
        )
        .unwrap();
        assert!(!text(&not).contains("To:") && text(&not).starts_with("Subject:"));
        assert!(section_bytes(PLAIN, &root, &section(vec![2], None)).is_none());
        assert!(section_bytes(PLAIN, &root, &section(vec![1], Some(SectionText::Header))).is_none());
        assert_eq!(partial(b"0123456789", 2, 3), b"234");
        assert_eq!(partial(b"0123", 2, 99), b"23");
        assert_eq!(partial(b"0123", 9, 1), b"");
    }
}
