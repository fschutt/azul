use crate::time::{amz_date, iso8601, parse_http_date, parse_iso8601};

#[test]
fn amz_date_formats_unix_seconds_in_utc() {
    assert_eq!(amz_date(1_440_938_160), "20150830T123600Z");
    assert_eq!(amz_date(1_369_353_600), "20130524T000000Z");
    assert_eq!(amz_date(0), "19700101T000000Z");
    assert_eq!(amz_date(951_868_799), "20000229T235959Z");
}

#[test]
fn s3_list_timestamps_parse_with_and_without_milliseconds() {
    assert_eq!(
        parse_iso8601("2009-10-12T17:50:30.000Z"),
        Some(1_255_369_830)
    );
    assert_eq!(parse_iso8601("2009-10-12T17:50:30Z"), Some(1_255_369_830));
    assert_eq!(parse_iso8601("2000-02-29T23:59:59.999Z"), Some(951_868_799));
}

#[test]
fn http_dates_parse_to_unix_seconds() {
    assert_eq!(
        parse_http_date("Mon, 12 Oct 2009 17:50:30 GMT"),
        Some(1_255_369_830)
    );
    assert_eq!(
        parse_http_date("Tue, 29 Feb 2000 23:59:59 GMT"),
        Some(951_868_799)
    );
}

#[test]
fn garbage_dates_do_not_parse() {
    assert_eq!(parse_iso8601(""), None);
    assert_eq!(parse_iso8601("yesterday"), None);
    assert_eq!(parse_iso8601("2009-13-12T17:50:30Z"), None);
    assert_eq!(parse_http_date("Mon, 12 Foo 2009 17:50:30 GMT"), None);
    assert_eq!(parse_http_date("12 Oct 2009"), None);
}

#[test]
fn iso8601_formats_unix_seconds_and_reads_back() {
    assert_eq!(iso8601(1_440_938_160), "2015-08-30T12:36:00Z");
    assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
    assert_eq!(iso8601(951_868_799), "2000-02-29T23:59:59Z");
    for secs in [0, 951_868_799, 1_440_938_160, 1_790_086_400] {
        assert_eq!(parse_iso8601(&iso8601(secs)), Some(secs));
    }
}
