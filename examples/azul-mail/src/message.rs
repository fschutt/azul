//! What AzMail reads out of a message (with `mail-parser`): the index line, the message view,
//! quoted lines, and dates.

use crate::store::IndexEntry;

/// `secs` since 1970 as RFC 3339 in UTC (`2026-09-30T08:42:00Z`); empty when out of range.
pub fn rfc3339_utc(secs: i64) -> String {
    todo!()
}

/// The year and month of `secs` since 1970, in UTC; `(0, 0)` when out of range.
pub fn year_month(secs: i64) -> (i32, u32) {
    todo!()
}

/// An RFC 3339 date as `2026-09-30 10:42` in `tz`; the text as it is when it is not a date.
pub fn short_date_in<Tz: chrono::TimeZone>(rfc3339: &str, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    todo!()
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
    todo!()
}

/// An attachment's name and size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attachment {
    pub name: String,
    pub size: usize,
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
}

/// The message view of a message's bytes; `None` when they are not a message.
pub fn parse_view(bytes: &[u8]) -> Option<MessageView> {
    todo!()
}

/// A line of plain text and how deeply it is quoted (`> ` marks, spaces between them allowed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuotedLine {
    pub level: usize,
    pub text: String,
}

/// The text's lines, each with its quote level and without its quote marks.
pub fn quote_lines(text: &str) -> Vec<QuotedLine> {
    todo!()
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
