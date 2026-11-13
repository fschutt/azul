//! A request refused because this computer's clock is off. A body says so
//! (RequestTimeTooSkewed); a HEAD's answer has no body, but its `Date` is the node's clock: an
//! answer 400 / 403 without a code, dated far from this clock, is the same refusal.

use crate::{time::now_unix, DriveError, HttpReply, S3Drive};

/// `secs` since 1970 as an HTTP date (the weekday is not read).
fn http_date(secs: u64) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Days since 1970 to a civil date (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "Xxx, {day:02} {} {year} {:02}:{:02}:{:02} GMT",
        MONTHS[(month - 1) as usize],
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

fn code_of(reply: &HttpReply) -> String {
    match S3Drive::failure_of(reply, None) {
        DriveError::Service(service) => service.code,
        other => panic!("not a service error: {other:?}"),
    }
}

#[test]
fn a_bare_refusal_dated_far_from_this_clock_is_a_skewed_clock() {
    let ahead = HttpReply {
        status: 403,
        headers: vec![(String::from("date"), http_date(now_unix() + 20 * 60))],
        body: Vec::new(),
    };
    assert_eq!(code_of(&ahead), "RequestTimeTooSkewed");
    let behind = HttpReply {
        status: 400,
        headers: vec![(String::from("date"), http_date(now_unix() - 3 * 3600))],
        body: Vec::new(),
    };
    assert_eq!(code_of(&behind), "RequestTimeTooSkewed");
    // A bare refusal from a node whose clock agrees is something else.
    let agreeing = HttpReply {
        status: 403,
        headers: vec![(String::from("date"), http_date(now_unix() + 30))],
        body: Vec::new(),
    };
    assert_eq!(code_of(&agreeing), "Forbidden");
    // A refusal that names its code keeps it, whatever its date.
    let named = HttpReply {
        status: 403,
        headers: vec![(String::from("date"), http_date(now_unix() + 20 * 60))],
        body: b"<Error><Code>AccessDenied</Code><Message>no</Message></Error>".to_vec(),
    };
    assert_eq!(code_of(&named), "AccessDenied");
}
