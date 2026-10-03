//! The dates feeds write, read leniently.
//!
//! RSS says RFC 822 (`Mon, 12 Oct 2009 17:50:30 +0200`), Atom and JSON Feed say RFC 3339
//! (`2009-10-12T17:50:30+02:00`). Real feeds write a weekday that is wrong, a full month name
//! (`September`, `Sept`), one-digit hours, two-digit years, named zones (`CEST`, `PDT`), no zone
//! at all (read as UTC), `+02:00` in an RFC 822 date, a space for the `T`, no seconds, only the
//! date, or `September 30, 2026 8:42 PM`. chrono's own RFC 2822 parser refuses a wrong weekday
//! and a missing zone, so the parts are read here and chrono only adds them up.

/// Seconds since 1970-01-01 UTC of a date as a feed writes it; `None` when it is no date.
#[must_use]
pub fn parse_date(_text: &str) -> Option<i64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc_822_dates_with_numeric_and_named_zones() {
        assert_eq!(parse_date("Mon, 12 Oct 2009 17:50:30 +0200"), Some(1_255_362_630));
        assert_eq!(parse_date("Mon, 12 Oct 2009 17:50:30 GMT"), Some(1_255_369_830));
        assert_eq!(parse_date("Mon, 12 Oct 2009 17:50:30 UT"), Some(1_255_369_830));
        assert_eq!(parse_date("Wed, 30 Sep 2026 10:00:00 CEST"), Some(1_790_755_200));
        assert_eq!(parse_date("Wed, 30 Sep 2026 20:15 PDT"), Some(1_790_824_500), "no seconds");
        assert_eq!(parse_date("Tue, 1 Sep 2026 9:05:00 -0500"), Some(1_788_271_500), "one-digit day and hour");
        assert_eq!(parse_date("Wed, 30 Sep 2026 08:42:00 +02:00"), Some(1_790_750_520), "a colon in the zone");
        assert_eq!(parse_date("Wed, 30 Sep 2026 08:42:00 -04:30"), Some(1_790_773_920));
    }

    #[test]
    fn what_real_rss_feeds_get_wrong_is_read_anyway() {
        // 30 September 2026 is a Wednesday: the weekday is ignored.
        assert_eq!(parse_date("Fri, 30 Sep 2026 08:42:00 GMT"), Some(1_790_757_720), "a wrong weekday");
        assert_eq!(parse_date("Wednesday, 30 September 2026 08:42:00 GMT"), Some(1_790_757_720), "full names");
        assert_eq!(parse_date("Wed, 30 Sept 2026 08:42:00 GMT"), Some(1_790_757_720), "Sept");
        assert_eq!(parse_date("Wed, 30 Sep 2026 08:42:00"), Some(1_790_757_720), "no zone is UTC");
        assert_eq!(parse_date("wed, 30 sep 2026 08:42:00 gmt"), Some(1_790_757_720), "lower case");
        assert_eq!(parse_date("  Wed,30 Sep 2026 08:42:00 Z "), Some(1_790_757_720), "no space after the comma");
        assert_eq!(parse_date("Mon, 07 Mar 05 14:00:00 +0000"), Some(1_110_204_000), "a two-digit year");
        assert_eq!(parse_date("30 Sep 2026"), Some(1_790_726_400), "only the date");
        assert_eq!(parse_date("Wed, 30 Sep 2026 08:42:00 GMT+0200"), Some(1_790_750_520), "GMT+0200");
    }

    #[test]
    fn rfc_3339_and_iso_8601_dates() {
        assert_eq!(parse_date("2026-09-30T08:42:00Z"), Some(1_790_757_720));
        assert_eq!(parse_date("2026-09-30T08:42:00.123Z"), Some(1_790_757_720), "fractions");
        assert_eq!(parse_date("2026-09-30T08:42:00+02:00"), Some(1_790_750_520));
        assert_eq!(parse_date("2026-09-30T08:42:00+0200"), Some(1_790_750_520));
        assert_eq!(parse_date("2026-09-30T08:42:00 +02:00"), Some(1_790_750_520), "a space before the zone");
        assert_eq!(parse_date("2026-09-30 08:42:00"), Some(1_790_757_720), "a space for the T, no zone");
        assert_eq!(parse_date("2026-09-30t08:42z"), Some(1_790_757_720), "no seconds, lower case");
        assert_eq!(parse_date("2026-09-30"), Some(1_790_726_400), "only the date");
    }

    #[test]
    fn us_style_dates_with_am_and_pm() {
        assert_eq!(parse_date("September 30, 2026 8:42 PM"), Some(1_790_800_920));
        assert_eq!(parse_date("Sep 30, 2026"), Some(1_790_726_400));
        assert_eq!(parse_date("September 30th, 2026 12:00 AM UTC"), Some(1_790_726_400));
    }

    #[test]
    fn what_is_no_date_gives_none() {
        for text in ["", "yesterday", "2026-13-01T00:00:00Z", "31 Feb 2026", "Wed, 30 Sep 08:42:00 GMT", "12:00", "2026"] {
            assert_eq!(parse_date(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_date_is_never_read_from_a_bogus_hour() {
        assert_eq!(parse_date("Wed, 30 Sep 2026 25:00:00 GMT"), None);
        assert_eq!(parse_date("2026-09-30T08:61:00Z"), None);
    }
}
