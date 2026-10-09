//! SEARCH: a key against one message - its flags, numbers, size and dates, and when the key
//! asks for them its header or its whole bytes (read once per message by the session).

use crate::{dates, mime};

use super::{
    parse::{SearchKey, SequenceSet},
    Flags,
};

/// What of a message a key needs read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Need {
    Nothing,
    Header,
    Whole,
}

/// What `key` needs of each message.
#[must_use]
pub fn need(key: &SearchKey) -> Need {
    match key {
        SearchKey::Bcc(_)
        | SearchKey::Cc(_)
        | SearchKey::From(_)
        | SearchKey::To(_)
        | SearchKey::Subject(_)
        | SearchKey::Header(_, _)
        | SearchKey::SentBefore(_)
        | SearchKey::SentOn(_)
        | SearchKey::SentSince(_) => Need::Header,
        SearchKey::Body(_) | SearchKey::Text(_) => Need::Whole,
        SearchKey::Not(inner) => need(inner),
        SearchKey::Or(a, b) => need(a).max(need(b)),
        SearchKey::And(keys) => keys.iter().map(need).max().unwrap_or(Need::Nothing),
        _ => Need::Nothing,
    }
}

/// One message as SEARCH sees it.
#[derive(Debug, Clone, Copy)]
pub struct Candidate<'a> {
    pub seq: u32,
    pub uid: u32,
    pub size: u64,
    /// INTERNALDATE, seconds since 1970.
    pub arrived: i64,
    pub flags: &'a Flags,
    /// The header, or the whole message, as [`need`] asked; `None` when it could not be read
    /// (a key on it does not match then).
    pub bytes: Option<&'a [u8]>,
}

/// The numbers `*` stands for.
#[derive(Debug, Clone, Copy)]
pub struct Largest {
    pub seq: u32,
    pub uid: u32,
}

fn contains_text(haystack: &[u8], needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    String::from_utf8_lossy(haystack)
        .to_lowercase()
        .contains(&needle.to_lowercase())
}

fn header_of<'a>(candidate: &Candidate<'a>) -> Option<&'a [u8]> {
    let bytes = candidate.bytes?;
    let (end, _) = mime::split(bytes);
    Some(&bytes[..end])
}

/// Whether a header field `name` of the candidate contains `needle` (any case).
fn field_contains(candidate: &Candidate<'_>, name: &str, needle: &str) -> bool {
    let Some(header) = header_of(candidate) else {
        return false;
    };
    mime::fields(header)
        .iter()
        .filter(|f| f.name.eq_ignore_ascii_case(name))
        .any(|f| contains_text(f.text().as_bytes(), needle))
}

fn sent_day(candidate: &Candidate<'_>) -> Option<i64> {
    let header = header_of(candidate)?;
    let date = mime::field(header, "Date")?;
    dates::parse_rfc5322_date(&date).map(dates::day_of)
}

fn in_set(set: &SequenceSet, n: u32, max: u32) -> bool {
    max > 0 && set.contains(n, max)
}

/// Whether `candidate` matches `key`.
#[must_use]
pub fn matches(key: &SearchKey, candidate: &Candidate<'_>, largest: Largest) -> bool {
    let flags = candidate.flags;
    let day = dates::day_of(candidate.arrived);
    match key {
        SearchKey::All | SearchKey::Old => true,
        SearchKey::New | SearchKey::Recent => false,
        SearchKey::Answered => flags.answered,
        SearchKey::Unanswered => !flags.answered,
        SearchKey::Deleted => flags.deleted,
        SearchKey::Undeleted => !flags.deleted,
        SearchKey::Draft => flags.draft,
        SearchKey::Undraft => !flags.draft,
        SearchKey::Flagged => flags.flagged,
        SearchKey::Unflagged => !flags.flagged,
        SearchKey::Seen => flags.seen,
        SearchKey::Unseen => !flags.seen,
        SearchKey::Keyword(word) => flags.has(word),
        SearchKey::Unkeyword(word) => !flags.has(word),
        SearchKey::Bcc(text) => field_contains(candidate, "Bcc", text),
        SearchKey::Cc(text) => field_contains(candidate, "Cc", text),
        SearchKey::From(text) => field_contains(candidate, "From", text),
        SearchKey::To(text) => field_contains(candidate, "To", text),
        SearchKey::Subject(text) => field_contains(candidate, "Subject", text),
        SearchKey::Header(name, text) => field_contains(candidate, name, text),
        SearchKey::Body(text) => candidate.bytes.is_some_and(|bytes| {
            let (_, body) = mime::split(bytes);
            contains_text(&bytes[body..], text)
        }),
        SearchKey::Text(text) => candidate.bytes.is_some_and(|bytes| contains_text(bytes, text)),
        SearchKey::Before(d) => day < *d,
        SearchKey::On(d) => day == *d,
        SearchKey::Since(d) => day >= *d,
        SearchKey::SentBefore(d) => sent_day(candidate).is_some_and(|s| s < *d),
        SearchKey::SentOn(d) => sent_day(candidate) == Some(*d),
        SearchKey::SentSince(d) => sent_day(candidate).is_some_and(|s| s >= *d),
        SearchKey::Larger(n) => candidate.size > *n,
        SearchKey::Smaller(n) => candidate.size < *n,
        SearchKey::Uid(set) => in_set(set, candidate.uid, largest.uid),
        SearchKey::Seq(set) => in_set(set, candidate.seq, largest.seq),
        SearchKey::Not(inner) => !matches(inner, candidate, largest),
        SearchKey::Or(a, b) => matches(a, candidate, largest) || matches(b, candidate, largest),
        SearchKey::And(keys) => keys.iter().all(|k| matches(k, candidate, largest)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imap::parse::Bound;

    /// 2026-10-01T08:30:00Z
    const OCT_1: i64 = 1_790_843_400;

    const MAIL: &[u8] = b"From: Ada Lovelace <ada@example.org>\r\n\
        To: ben@example.net\r\n\
        Subject: Lunch on Thursday\r\n\
        Date: Wed, 30 Sep 2026 18:00:00 +0000\r\n\
        X-Priority: 1\r\n\
        \r\n\
        See you at NOON.\r\n";

    fn candidate<'a>(flags: &'a Flags, bytes: Option<&'a [u8]>) -> Candidate<'a> {
        Candidate {
            seq: 2,
            uid: 7,
            size: 500,
            arrived: OCT_1,
            flags,
            bytes,
        }
    }

    const LARGEST: Largest = Largest { seq: 3, uid: 9 };

    fn day(y: i64, m: u32, d: u32) -> i64 {
        dates::days_from_civil(y, m, d)
    }

    #[test]
    fn keys_say_what_they_need_read() {
        assert_eq!(need(&SearchKey::Unseen), Need::Nothing);
        assert_eq!(need(&SearchKey::From("a".into())), Need::Header);
        assert_eq!(
            need(&SearchKey::Or(
                Box::new(SearchKey::Subject("a".into())),
                Box::new(SearchKey::Not(Box::new(SearchKey::Body("b".into()))))
            )),
            Need::Whole
        );
    }

    #[test]
    fn flags_numbers_sizes_and_dates_match_without_reading_the_message() {
        let flags = Flags {
            seen: true,
            deleted: true,
            ..Flags::default()
        };
        let c = candidate(&flags, None);
        assert!(matches(&SearchKey::Seen, &c, LARGEST));
        assert!(!matches(&SearchKey::Unseen, &c, LARGEST));
        assert!(matches(&SearchKey::Deleted, &c, LARGEST));
        assert!(matches(&SearchKey::Unflagged, &c, LARGEST));
        assert!(matches(&SearchKey::Larger(499), &c, LARGEST));
        assert!(!matches(&SearchKey::Smaller(500), &c, LARGEST));
        assert!(matches(&SearchKey::On(day(2026, 10, 1)), &c, LARGEST));
        assert!(matches(&SearchKey::Since(day(2026, 10, 1)), &c, LARGEST));
        assert!(!matches(&SearchKey::Before(day(2026, 10, 1)), &c, LARGEST));
        let uids = |a, b| SequenceSet(vec![(a, b)]);
        assert!(matches(&SearchKey::Uid(uids(Bound::Num(5), Bound::Star)), &c, LARGEST));
        assert!(!matches(&SearchKey::Uid(uids(Bound::Num(8), Bound::Star)), &c, LARGEST));
        assert!(matches(&SearchKey::Seq(uids(Bound::Num(2), Bound::Num(2))), &c, LARGEST));
        assert!(matches(&SearchKey::Old, &c, LARGEST) && !matches(&SearchKey::New, &c, LARGEST));
    }

    #[test]
    fn header_and_text_keys_match_any_case_and_dates_come_from_the_date_header() {
        let flags = Flags::default();
        let c = candidate(&flags, Some(MAIL));
        assert!(matches(&SearchKey::From("LOVELACE".into()), &c, LARGEST));
        assert!(matches(&SearchKey::To("ben@".into()), &c, LARGEST));
        assert!(!matches(&SearchKey::Cc("ben".into()), &c, LARGEST));
        assert!(matches(&SearchKey::Subject("thursday".into()), &c, LARGEST));
        assert!(matches(&SearchKey::Header("x-priority".into(), String::new()), &c, LARGEST));
        assert!(!matches(&SearchKey::Header("X-Spam".into(), String::new()), &c, LARGEST));
        assert!(matches(&SearchKey::Body("noon".into()), &c, LARGEST));
        assert!(!matches(&SearchKey::Body("lunch".into()), &c, LARGEST));
        assert!(matches(&SearchKey::Text("lunch".into()), &c, LARGEST));
        assert!(matches(&SearchKey::SentOn(day(2026, 9, 30)), &c, LARGEST));
        assert!(matches(&SearchKey::SentBefore(day(2026, 10, 1)), &c, LARGEST));
        assert!(!matches(&SearchKey::SentSince(day(2026, 10, 1)), &c, LARGEST));
        let unread = candidate(&flags, None);
        assert!(!matches(&SearchKey::From("ada".into()), &unread, LARGEST));
        assert!(matches(
            &SearchKey::Not(Box::new(SearchKey::From("ada".into()))),
            &unread,
            LARGEST
        ));
    }
}
