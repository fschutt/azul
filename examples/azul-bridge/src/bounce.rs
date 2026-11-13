//! A delivery report (RFC 3464, `multipart/report; report-type=delivery-status`) for a mail the
//! submission port took but some recipients will never get: the mail program had its 250
//! (the others got the mail, or it was queued), so the sender learns of the failures the way
//! every mail server tells: a message in the Inbox. It lists each failed recipient with the
//! reply, carries the machine-readable delivery status and the original message's header.
//!
//! The report is filed into the drive's Inbox by the bridge itself and never sent anywhere.

use crate::{dates, mime};

/// One recipient that will never get the mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// `local@domain`.
    pub address: String,
    /// The reply code, when a server answered.
    pub code: Option<u16>,
    /// The reply or the problem, for people.
    pub reason: String,
}

/// The enhanced status (`5.1.1`) a reason names, else the class of its reply code.
#[must_use]
pub fn status_of(failure: &Failure) -> String {
    let found = failure.reason.split(|c: char| c.is_whitespace() || c == '(' || c == ')').find(|word| {
        let parts: Vec<&str> = word.split('.').collect();
        parts.len() == 3
            && matches!(parts[0], "2" | "4" | "5")
            && parts[1..]
                .iter()
                .all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
    });
    match (found, failure.code) {
        (Some(status), _) => status.to_string(),
        (None, Some(code)) if (400..500).contains(&code) => String::from("4.0.0"),
        _ => String::from("5.0.0"),
    }
}

/// Text on one line without control characters (a header value of the report).
fn header_safe(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .trim()
        .to_string()
}

/// The report for the mail `original` the account's `sender` (`local@domain`) submitted:
/// `failures` will never get it; `others_got_it`: some recipients did. `now` dates it; `token`
/// makes its Message-ID and boundary unique.
#[must_use]
pub fn report(
    sender: &str,
    original: &[u8],
    failures: &[Failure],
    others_got_it: bool,
    now: i64,
    token: &str,
) -> Vec<u8> {
    let (header_end, _) = mime::split(original);
    let original_header = &original[..header_end];
    let subject = mime::field(original_header, "Subject").unwrap_or_default();
    let domain = sender.rsplit_once('@').map_or("localhost", |(_, d)| d);
    let date = dates::http_date(now).replace(" GMT", " +0000");
    let boundary = format!("azlin-bridge-report-{token}");
    let mut text = String::from("Your message could not be delivered to:\r\n\r\n");
    for failure in failures {
        text.push_str(&format!(
            "  {}: {}\r\n",
            header_safe(&failure.address),
            header_safe(&failure.reason)
        ));
    }
    text.push_str("\r\n");
    text.push_str(if others_got_it {
        "The other recipients got it.\r\n"
    } else {
        "Nobody got it.\r\n"
    });
    text.push_str("\r\n(Reported by the Azlin Bridge on this computer.)\r\n");
    let mut status = format!("Reporting-MTA: dns; azlin-bridge.localhost\r\nArrival-Date: {date}\r\n");
    for failure in failures {
        status.push_str("\r\n");
        status.push_str(&format!("Final-Recipient: rfc822; {}\r\n", header_safe(&failure.address)));
        status.push_str("Action: failed\r\n");
        status.push_str(&format!("Status: {}\r\n", status_of(failure)));
        status.push_str(&format!("Diagnostic-Code: smtp; {}\r\n", header_safe(&failure.reason)));
    }
    let mut out = String::new();
    out.push_str(&format!("From: Mail Delivery <mailer-daemon@{domain}>\r\n"));
    out.push_str(&format!("To: <{}>\r\n", header_safe(sender)));
    out.push_str(&format!(
        "Subject: Undelivered mail: {}\r\n",
        header_safe(&subject)
    ));
    out.push_str(&format!("Date: {date}\r\n"));
    out.push_str(&format!("Message-ID: <report-{token}@azlin-bridge.localhost>\r\n"));
    out.push_str("Auto-Submitted: auto-replied\r\n");
    out.push_str("MIME-Version: 1.0\r\n");
    out.push_str(&format!(
        "Content-Type: multipart/report; report-type=delivery-status; boundary=\"{boundary}\"\r\n\r\n"
    ));
    out.push_str(&format!("--{boundary}\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{text}"));
    out.push_str(&format!(
        "--{boundary}\r\nContent-Type: message/delivery-status\r\n\r\n{status}"
    ));
    let mut bytes = out.into_bytes();
    bytes.extend_from_slice(
        format!("--{boundary}\r\nContent-Type: text/rfc822-headers\r\n\r\n").as_bytes(),
    );
    bytes.extend_from_slice(original_header);
    bytes.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mime::Kind;

    const ORIGINAL: &[u8] = b"From: Ada <ada@example.org>\r\nTo: ben@example.net, cy@example.com\r\n\
        Subject: Lunch\r\nMessage-ID: <m1@example.org>\r\n\r\nSee you.\r\n";

    fn failure(address: &str, code: Option<u16>, reason: &str) -> Failure {
        Failure {
            address: address.to_string(),
            code,
            reason: reason.to_string(),
        }
    }

    #[test]
    fn the_status_is_the_enhanced_code_of_the_reply_else_its_class() {
        assert_eq!(status_of(&failure("a@b", Some(550), "mx.b: 550 5.1.1 no such user")), "5.1.1");
        assert_eq!(status_of(&failure("a@b", Some(452), "452 too many")), "4.0.0");
        assert_eq!(status_of(&failure("a@b", None, "given up after 5 days (no connection)")), "5.0.0");
        assert_eq!(status_of(&failure("a@b", Some(554), "554 (5.7.1) relay denied")), "5.7.1");
    }

    #[test]
    fn a_report_lists_the_failures_and_parses_as_a_delivery_report() {
        let bytes = report(
            "ada@example.org",
            ORIGINAL,
            &[failure("ben@example.net", Some(550), "550 5.1.1 no such user")],
            true,
            1_790_843_400,
            "t1",
        );
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.starts_with("From: Mail Delivery <mailer-daemon@example.org>\r\n"), "{text}");
        assert!(text.contains("Subject: Undelivered mail: Lunch\r\n"), "{text}");
        assert!(text.contains("Date: Thu, 01 Oct 2026 08:30:00 +0000\r\n"), "{text}");
        assert!(text.contains("  ben@example.net: 550 5.1.1 no such user\r\n"), "{text}");
        assert!(text.contains("The other recipients got it."), "{text}");
        assert!(text.contains("Final-Recipient: rfc822; ben@example.net\r\nAction: failed\r\nStatus: 5.1.1\r\n"), "{text}");
        let root = mime::parse(&bytes);
        let Kind::Multipart(parts) = &root.kind else {
            panic!("not multipart: {text}");
        };
        assert_eq!(parts.len(), 3);
        let ct = mime::content_type(&bytes[root.header.clone()]);
        assert_eq!((ct.kind.as_str(), ct.subtype.as_str()), ("multipart", "report"));
        assert_eq!(ct.param("report-type"), Some("delivery-status"));
        assert!(String::from_utf8_lossy(&bytes[parts[2].body.clone()]).contains("Message-ID: <m1@example.org>"));
    }

    #[test]
    fn a_reason_cannot_break_out_of_its_line() {
        let bytes = report(
            "ada@example.org",
            ORIGINAL,
            &[failure("ben@example.net", None, "bad\r\nX-Injected: yes")],
            false,
            0,
            "t2",
        );
        let text = String::from_utf8(bytes).unwrap();
        assert!(!text.contains("\r\nX-Injected"), "{text}");
        assert!(text.contains("Nobody got it."), "{text}");
    }
}
