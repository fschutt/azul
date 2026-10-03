//! What AzMail reads out of a message (with `mail-parser`): the index line, the message view,
//! quoted lines, and dates.

use mail_parser::{Address, Message, MessageParser, MimeHeaders};

use crate::store::IndexEntry;

/// `secs` since 1970 as RFC 3339 in UTC (`2026-09-30T08:42:00Z`); empty when out of range.
pub fn rfc3339_utc(secs: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_default()
}

/// The year and month of `secs` since 1970, in UTC; `(0, 0)` when out of range.
pub fn year_month(secs: i64) -> (i32, u32) {
    use chrono::Datelike;

    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, 0)
        .map_or((0, 0), |d| (d.year(), d.month()))
}

/// An RFC 3339 date as `2026-09-30 10:42` in `tz`; the text as it is when it is not a date.
pub fn short_date_in<Tz: chrono::TimeZone>(rfc3339: &str, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    match chrono::DateTime::parse_from_rfc3339(rfc3339) {
        Ok(date) => date.with_timezone(tz).format("%Y-%m-%d %H:%M").to_string(),
        Err(_) => rfc3339.to_string(),
    }
}

/// The message's index line. `internal_date` is when the server received it (seconds since
/// 1970), the fallback for the date when the message has no Date header.
pub fn index_entry(
    uid: u32,
    bytes: &[u8],
    flags: &[String],
    internal_date: Option<i64>,
    path: &str,
) -> IndexEntry {
    let parsed = MessageParser::default().parse(bytes);
    let message = parsed.as_ref();
    let date = message
        .and_then(header_date)
        .or(internal_date)
        .map(rfc3339_utc)
        .unwrap_or_default();
    IndexEntry {
        uid,
        message_id: message
            .and_then(|m| m.message_id())
            .unwrap_or_default()
            .to_string(),
        date,
        from: message.map(|m| addresses(m.from())).unwrap_or_default(),
        to: message.map(|m| addresses(m.to())).unwrap_or_default(),
        subject: message
            .and_then(|m| m.subject())
            .unwrap_or_default()
            .to_string(),
        flags: flags.to_vec(),
        size: bytes.len() as u64,
        path: path.to_string(),
    }
}

/// The Date header, in seconds since 1970, when it is a date.
fn header_date(message: &Message<'_>) -> Option<i64> {
    message
        .date()
        .filter(|d| d.is_valid())
        .map(|d| d.to_timestamp())
}

/// An address header as `Name <address>, address, ...`.
fn addresses(address: Option<&Address<'_>>) -> String {
    let Some(address) = address else {
        return String::new();
    };
    address
        .iter()
        .map(|a| match (a.name().map(str::trim), a.address()) {
            (Some(name), Some(addr)) if !name.is_empty() => format!("{name} <{addr}>"),
            (_, Some(addr)) => addr.to_string(),
            (Some(name), None) => name.to_string(),
            (None, None) => String::new(),
        })
        .filter(|a| !a.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

/// An attachment's name and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub size: usize,
}

/// An attachment with its bytes, as a forward or a reopened draft carries it on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentPart {
    pub name: String,
    /// `type/subtype`, lower case; `application/octet-stream` when the part names none.
    pub mime_type: String,
    /// Decoded (no base64 / quoted-printable left).
    pub bytes: Vec<u8>,
}

/// Every attachment of a message's bytes with its contents, in the order of
/// [`MessageView::attachments`]; none when the bytes are not a message.
pub fn attachment_parts(bytes: &[u8]) -> Vec<AttachmentPart> {
    let Some(message) = MessageParser::default().parse(bytes) else {
        return Vec::new();
    };
    message
        .attachments()
        .map(|part| AttachmentPart {
            name: part.attachment_name().unwrap_or("attachment").to_string(),
            mime_type: part
                .content_type()
                .map(|ct| match ct.subtype() {
                    Some(sub) => format!("{}/{}", ct.ctype(), sub),
                    None => ct.ctype().to_string(),
                })
                .map(|t| t.to_ascii_lowercase())
                .unwrap_or_else(|| String::from("application/octet-stream")),
            bytes: part.contents().to_vec(),
        })
        .collect()
}

/// What the message view shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MessageView {
    pub subject: String,
    pub from: String,
    pub to: String,
    pub cc: String,
    /// RFC 3339, UTC; empty without a Date header.
    pub date: String,
    /// The plain text: the text part, else the HTML part turned into text.
    pub text: String,
    /// The HTML part, if the message has one (a plain-text message has none).
    pub html: Option<String>,
    pub attachments: Vec<Attachment>,
    /// The Message-ID, bare (no angle brackets); empty without one.
    pub message_id: String,
    /// The Reply-To header as `Name <address>, ...`; empty without one.
    pub reply_to: String,
    /// The References header's ids, bare, oldest first.
    pub references: Vec<String>,
    /// The Bcc header as `Name <address>, ...` (only a draft or a sent copy has one).
    pub bcc: String,
    /// The In-Reply-To header's id, bare; empty without one.
    pub in_reply_to: String,
}

/// The message view of a message's bytes; `None` when they are not a message.
pub fn parse_view(bytes: &[u8]) -> Option<MessageView> {
    let message = MessageParser::default().parse(bytes)?;
    // A plain-text message lists its text part as its HTML body too (converted); only a real
    // text/html part is HTML.
    let has_html = message.html_bodies().any(|part| part.is_text_html());
    let text = message
        .body_text(0)
        .map(|t| t.into_owned())
        .unwrap_or_default();
    let html = if has_html {
        message.body_html(0).map(|h| h.into_owned())
    } else {
        None
    };
    let attachments = message
        .attachments()
        .map(|part| Attachment {
            name: part.attachment_name().unwrap_or("attachment").to_string(),
            size: part.len(),
        })
        .collect();
    Some(MessageView {
        subject: message.subject().unwrap_or_default().to_string(),
        from: addresses(message.from()),
        to: addresses(message.to()),
        cc: addresses(message.cc()),
        date: header_date(&message).map(rfc3339_utc).unwrap_or_default(),
        text,
        html,
        attachments,
        message_id: message.message_id().unwrap_or_default().to_string(),
        reply_to: addresses(message.reply_to()),
        references: message
            .references()
            .as_text_list()
            .map(|ids| ids.iter().map(|id| id.trim().to_string()).collect())
            .unwrap_or_default(),
        bcc: addresses(message.bcc()),
        in_reply_to: message
            .in_reply_to()
            .as_text()
            .map(|id| id.trim().to_string())
            .unwrap_or_default(),
    })
}

/// A line of plain text and how deeply it is quoted (`> ` marks, spaces between them allowed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedLine {
    pub level: usize,
    pub text: String,
}

/// The text's lines, each with its quote level and without its quote marks.
pub fn quote_lines(text: &str) -> Vec<QuotedLine> {
    let body = text.strip_suffix('\n').unwrap_or(text);
    if body.is_empty() {
        return Vec::new();
    }
    body.split('\n')
        .map(|raw| {
            let line = raw.strip_suffix('\r').unwrap_or(raw);
            let mut level = 0;
            let mut rest = line;
            loop {
                if let Some(after) = rest.strip_prefix('>') {
                    level += 1;
                    rest = after;
                    continue;
                }
                // `> > text`: one space between marks.
                if level > 0 {
                    if let Some(after) = rest.strip_prefix(' ') {
                        if after.starts_with('>') {
                            rest = after;
                            continue;
                        }
                    }
                }
                break;
            }
            if level > 0 {
                rest = rest.strip_prefix(' ').unwrap_or(rest);
            }
            QuotedLine {
                level,
                text: rest.to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLAIN: &[u8] = b"Message-ID: <plain-1@example.org>\r\n\
Date: Wed, 30 Sep 2026 10:42:00 +0200\r\n\
From: Ada Lovelace <ada@example.org>\r\n\
To: Ben <ben@example.org>, cleo@example.org\r\n\
Cc: Dan <dan@example.org>\r\n\
Subject: =?ISO-8859-1?Q?Gr=FC=DFe_aus_Berlin?=\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Hi Ben,\r\n\
\r\n\
> Did you see it?\r\n\
> > Yes.\r\n\
Thanks\r\n";

    const ALTERNATIVE: &[u8] = b"Message-ID: <alt-1@example.org>\r\n\
Date: Wed, 30 Sep 2026 23:30:00 +0000\r\n\
From: news@example.org\r\n\
To: ada@example.org\r\n\
Subject: Newsletter\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"outer\"\r\n\
\r\n\
--outer\r\n\
Content-Type: multipart/alternative; boundary=\"inner\"\r\n\
\r\n\
--inner\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
The plain version.\r\n\
--inner\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<p>The <b>HTML</b> version.</p>\r\n\
--inner--\r\n\
--outer\r\n\
Content-Type: text/plain; name=\"notes.txt\"\r\n\
Content-Disposition: attachment; filename=\"notes.txt\"\r\n\
\r\n\
hello\r\n\
--outer--\r\n";

    const HTML_ONLY: &[u8] = b"From: shop@example.org\r\n\
Subject: Sale\r\n\
Content-Type: text/html; charset=utf-8\r\n\
\r\n\
<html><body><p>Everything must go</p></body></html>\r\n";

    #[test]
    fn dates_are_utc_rfc_3339_and_months() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(rfc3339_utc(1_790_757_720), "2026-09-30T08:42:00Z");
        assert_eq!(year_month(1_790_757_720), (2026, 9));
        assert_eq!(year_month(946_684_799), (1999, 12));
        assert_eq!(year_month(i64::MAX), (0, 0));
        assert_eq!(rfc3339_utc(i64::MAX), "");
        // The formatter is azul-storage's, the one the apps' files are dated with (DEDUP_EDITORS
        // B4): the same text for every date it can write; before 1970 there is none.
        for secs in [0_u64, 1_790_757_720, 951_782_400, 253_402_300_799] {
            assert_eq!(rfc3339_utc(secs as i64), azul_storage::time::iso8601(secs));
        }
        assert_eq!(rfc3339_utc(253_402_300_799), "9999-12-31T23:59:59Z");
        assert_eq!(rfc3339_utc(253_402_300_800), "", "past the year 9999");
        assert_eq!(rfc3339_utc(-1), "", "before 1970");
    }

    #[test]
    fn a_list_date_is_shown_in_the_viewers_zone() {
        let berlin = chrono::FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(
            short_date_in("2026-09-30T08:42:00Z", &berlin),
            "2026-09-30 10:42"
        );
        assert_eq!(
            short_date_in("2026-09-30T23:30:00Z", &berlin),
            "2026-10-01 01:30"
        );
        assert_eq!(short_date_in("", &berlin), "");
        assert_eq!(short_date_in("yesterday", &berlin), "yesterday");
    }

    #[test]
    fn the_index_line_comes_from_the_headers() {
        let flags = vec![String::from("\\Seen")];
        let e = index_entry(7, PLAIN, &flags, Some(0), "mail/inbox/2026/09/7.eml");
        assert_eq!(e.uid, 7);
        assert_eq!(e.message_id, "plain-1@example.org");
        assert_eq!(e.date, "2026-09-30T08:42:00Z", "the Date header, in UTC");
        assert_eq!(e.from, "Ada Lovelace <ada@example.org>");
        assert_eq!(e.to, "Ben <ben@example.org>, cleo@example.org");
        assert_eq!(e.subject, "Grüße aus Berlin");
        assert_eq!(e.flags, flags);
        assert_eq!(e.size, PLAIN.len() as u64);
        assert_eq!(e.path, "mail/inbox/2026/09/7.eml");
    }

    #[test]
    fn without_a_date_header_the_index_has_the_arrival_time() {
        let e = index_entry(1, HTML_ONLY, &[], Some(1_790_811_000), "p");
        assert_eq!(e.date, "2026-09-30T23:30:00Z");
        assert_eq!(e.from, "shop@example.org");
        assert_eq!(e.to, "");
        assert_eq!(index_entry(1, HTML_ONLY, &[], None, "p").date, "");
    }

    #[test]
    fn bytes_that_are_not_a_message_still_get_an_index_line() {
        let e = index_entry(3, b"", &[], Some(0), "mail/inbox/1970/01/3.eml");
        assert_eq!(e.uid, 3);
        assert_eq!(e.subject, "");
        assert_eq!(e.date, "1970-01-01T00:00:00Z");
        assert_eq!(e.size, 0);
    }

    #[test]
    fn the_view_has_the_text_part_the_html_part_and_the_attachments() {
        let v = parse_view(ALTERNATIVE).unwrap();
        assert_eq!(v.subject, "Newsletter");
        assert_eq!(v.from, "news@example.org");
        assert_eq!(v.to, "ada@example.org");
        assert_eq!(v.date, "2026-09-30T23:30:00Z");
        assert_eq!(v.text.trim(), "The plain version.");
        assert_eq!(
            v.html.as_deref().map(str::trim),
            Some("<p>The <b>HTML</b> version.</p>")
        );
        assert_eq!(
            v.attachments,
            vec![Attachment {
                name: String::from("notes.txt"),
                size: 5
            }]
        );
    }

    #[test]
    fn a_plain_message_has_no_html_and_an_html_one_has_text_too() {
        let plain = parse_view(PLAIN).unwrap();
        assert_eq!(plain.html, None);
        assert_eq!(plain.cc, "Dan <dan@example.org>");
        assert!(plain.text.contains("> Did you see it?"), "{}", plain.text);
        let html = parse_view(HTML_ONLY).unwrap();
        assert!(html
            .html
            .as_deref()
            .unwrap()
            .contains("<p>Everything must go</p>"));
        assert!(html.text.contains("Everything must go"), "{}", html.text);
        assert!(!html.text.contains("<p>"), "{}", html.text);
    }

    #[test]
    fn the_view_knows_the_thread_a_reply_continues() {
        const REPLY: &[u8] = b"Message-ID: <r-2@example.org>\r\n\
From: Ben <ben@example.org>\r\n\
Reply-To: Garden List <list@example.org>\r\n\
To: ada@example.org\r\n\
References: <root-0@example.org> <r-1@example.org>\r\n\
In-Reply-To: <r-1@example.org>\r\n\
Subject: Re: Garden\r\n\
\r\n\
Yes.\r\n";
        let v = parse_view(REPLY).unwrap();
        assert_eq!(v.message_id, "r-2@example.org");
        assert_eq!(v.reply_to, "Garden List <list@example.org>");
        assert_eq!(
            v.references,
            vec![String::from("root-0@example.org"), String::from("r-1@example.org")]
        );
        let plain = parse_view(PLAIN).unwrap();
        assert_eq!(plain.message_id, "plain-1@example.org");
        assert_eq!(plain.reply_to, "");
        assert!(plain.references.is_empty());
    }

    #[test]
    fn a_forward_gets_every_attachment_decoded_with_its_type() {
        const TWO: &[u8] = b"From: ben@example.org\r\n\
Subject: Photos\r\n\
MIME-Version: 1.0\r\n\
Content-Type: multipart/mixed; boundary=\"b\"\r\n\
\r\n\
--b\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Two files.\r\n\
--b\r\n\
Content-Type: Image/PNG; name=\"pic.png\"\r\n\
Content-Disposition: attachment; filename=\"pic.png\"\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
iVBORw==\r\n\
--b\r\n\
Content-Disposition: attachment; filename=\"data.bin\"\r\n\
Content-Type: application/octet-stream\r\n\
Content-Transfer-Encoding: base64\r\n\
\r\n\
AAEC\r\n\
--b--\r\n";
        assert_eq!(
            attachment_parts(TWO),
            vec![
                AttachmentPart {
                    name: String::from("pic.png"),
                    mime_type: String::from("image/png"),
                    bytes: vec![0x89, b'P', b'N', b'G'],
                },
                AttachmentPart {
                    name: String::from("data.bin"),
                    mime_type: String::from("application/octet-stream"),
                    bytes: vec![0, 1, 2],
                },
            ]
        );
        let names: Vec<String> = parse_view(TWO)
            .unwrap()
            .attachments
            .into_iter()
            .map(|a| a.name)
            .collect();
        assert_eq!(names, ["pic.png", "data.bin"], "the same order as the view's list");
        assert_eq!(
            attachment_parts(ALTERNATIVE),
            vec![AttachmentPart {
                name: String::from("notes.txt"),
                mime_type: String::from("text/plain"),
                bytes: b"hello".to_vec(),
            }]
        );
        assert!(attachment_parts(PLAIN).is_empty());
    }

    #[test]
    fn a_draft_read_back_keeps_its_bcc_and_the_mail_it_answers() {
        const DRAFT: &[u8] = b"Bcc: Eve <eve@example.org>, fay@example.org\r\n\
X-AzMail-Draft: 1\r\n\
Message-ID: <draft-1@example.org>\r\n\
Date: Wed, 30 Sep 2026 10:42:00 +0200\r\n\
From: Ada Lovelace <ada@example.org>\r\n\
To: ben@example.org\r\n\
Subject: Re: Garden plan\r\n\
In-Reply-To: <garden-1@example.org>\r\n\
References: <root-0@example.org> <garden-1@example.org>\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Not sure yet.\r\n";
        let v = parse_view(DRAFT).unwrap();
        assert_eq!(v.bcc, "Eve <eve@example.org>, fay@example.org");
        assert_eq!(v.in_reply_to, "garden-1@example.org");
        let plain = parse_view(PLAIN).unwrap();
        assert_eq!(plain.bcc, "");
        assert_eq!(plain.in_reply_to, "");
    }

    #[test]
    fn quoted_lines_know_their_level() {
        let got: Vec<(usize, String)> =
            quote_lines("Hi Ben,\r\n\r\n> Did you see it?\n> > Yes.\n>>> deep\n>\nThanks\na > b\n")
                .into_iter()
                .map(|l| (l.level, l.text))
                .collect();
        let want: Vec<(usize, String)> = [
            (0, "Hi Ben,"),
            (0, ""),
            (1, "Did you see it?"),
            (2, "Yes."),
            (3, "deep"),
            (1, ""),
            (0, "Thanks"),
            (0, "a > b"),
        ]
        .into_iter()
        .map(|(l, t)| (l, t.to_string()))
        .collect();
        assert_eq!(got, want);
        assert!(quote_lines("").is_empty());
        assert_eq!(quote_lines("x").len(), 1);
    }
}
