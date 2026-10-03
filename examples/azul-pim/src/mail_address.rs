//! E-mail address lines and attendees: one reading for AzMail's To / Cc / Bcc, AzCalendar's
//! attendees, AzContacts' e-mail fields and the `ATTENDEE`s of an .ics file
//! (scripts/DEDUP_EDITORS_2026_10_02.md, B14).
//!
//! An address line is entries separated by commas, semicolons or line breaks; a separator inside
//! a quoted name (`"Lovelace, Ada" <ada@example.org>`) or inside angle brackets is part of the
//! entry. An entry is `name <address>`, `"quoted, name" <address>` or the address alone.
//!
//! Two address checks, not RFC 5322, what a person types or a server reports:
//! - [`is_email`]: one `@` with something on both sides, a dot in the domain, no blanks, no
//!   control characters, none of `< > , ; "`, no empty domain label. For what people type: an
//!   attendee, a contact's address, a To line.
//! - [`is_email_any_host`]: the same without the dot, so `postmaster@localhost` passes: an
//!   account on a test server.

/// One entry of an address line, read apart.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Mailbox {
    /// The display name, its quotes and escapes taken off: `Lovelace, Ada`; empty without one.
    pub name: String,
    /// What stands between the angle brackets, or the whole entry without them, trimmed; not
    /// checked (see [`is_email`]).
    pub address: String,
}

/// The entries of an address line: split at commas, semicolons and line breaks that are not
/// inside quotes or angle brackets, each trimmed, empty ones left out.
#[must_use]
pub fn split_address_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    let mut angle = 0usize;
    let flush = |current: &mut String, out: &mut Vec<String>| {
        let entry = current.trim();
        if !entry.is_empty() {
            out.push(entry.to_string());
        }
        current.clear();
    };
    for c in line.chars() {
        if escaped {
            escaped = false;
            current.push(c);
            continue;
        }
        match c {
            '\\' if quoted => {
                escaped = true;
                current.push(c);
            }
            '"' => {
                quoted = !quoted;
                current.push(c);
            }
            '<' if !quoted => {
                angle += 1;
                current.push(c);
            }
            '>' if !quoted => {
                angle = angle.saturating_sub(1);
                current.push(c);
            }
            ',' | ';' | '\n' | '\r' if !quoted && angle == 0 => flush(&mut current, &mut out),
            _ => current.push(c),
        }
    }
    flush(&mut current, &mut out);
    out
}

/// One entry read apart: `"Lovelace, Ada" <ada@example.org>` is the name `Lovelace, Ada` and
/// the address `ada@example.org`; `ada@example.org` alone has no name. A `mailto:` before the
/// address (an .ics `ATTENDEE`) is taken off.
#[must_use]
pub fn parse_mailbox(entry: &str) -> Mailbox {
    let entry = entry.trim();
    let (name, address) = match (entry.rfind('<'), entry.rfind('>')) {
        (Some(open), Some(close)) if open < close => {
            (unquote(entry[..open].trim()), entry[open + 1..close].trim())
        }
        _ => (String::new(), entry),
    };
    let address = address
        .get(..7)
        .filter(|p| p.eq_ignore_ascii_case("mailto:"))
        .map_or(address, |_| &address[7..])
        .trim();
    Mailbox {
        name,
        address: address.to_string(),
    }
}

/// A display name without its surrounding quotes and with `\"` / `\\` read back.
fn unquote(name: &str) -> String {
    let Some(inner) = name.strip_prefix('"').and_then(|n| n.strip_suffix('"')) else {
        return name.to_string();
    };
    let mut out = String::with_capacity(inner.len());
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// The address of an entry if it holds one ([`is_email_any_host`]): `ada@example.org` from
/// `Ada <ada@example.org>`, `"L, Ada" <ada@example.org>` or `ada@example.org`.
#[must_use]
pub fn bare_address(entry: &str) -> Option<String> {
    let address = parse_mailbox(entry).address;
    is_email_any_host(&address).then_some(address)
}

/// Whether two entries name the same mailbox: their addresses, ignoring case.
#[must_use]
pub fn same_address(a: &str, b: &str) -> bool {
    match (bare_address(a), bare_address(b)) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(&b),
        _ => false,
    }
}

/// The addresses of a line people typed (attendees, a To line), each once (ignoring case), in
/// order; every entry must hold an address by [`is_email`]. `Err` is the first entry that does
/// not, as it was typed.
pub fn address_list(line: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    for entry in split_address_line(line) {
        let address = parse_mailbox(&entry).address;
        if !is_email(&address) {
            return Err(entry);
        }
        if !out.iter().any(|a| a.eq_ignore_ascii_case(&address)) {
            out.push(address);
        }
    }
    Ok(out)
}

/// Whether `text` (trimmed) reads as an e-mail address a person typed: see the module's rules.
#[must_use]
pub fn is_email(text: &str) -> bool {
    check(text, true)
}

/// [`is_email`] without the dot in the domain: `postmaster@localhost` passes.
#[must_use]
pub fn is_email_any_host(text: &str) -> bool {
    check(text, false)
}

/// The domain of an address, in lower case: `example.org` of `Ada@Example.Org`.
#[must_use]
pub fn email_domain(text: &str) -> Option<String> {
    is_email_any_host(text).then(|| {
        let text = text.trim();
        text[text.rfind('@').map_or(0, |at| at + 1)..].to_ascii_lowercase()
    })
}

fn check(text: &str, need_dot: bool) -> bool {
    let text = text.trim();
    let mut parts = text.split('@');
    let (Some(local), Some(domain), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    !local.is_empty()
        && !domain.is_empty()
        && !text.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '<' | '>' | ',' | ';' | '"')
        })
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && (!need_dot || domain.contains('.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn address_lines_split_outside_quotes_and_brackets() {
        // AzMail's compose.rs test.
        assert_eq!(
            split_address_line(
                r#"Ada <ada@example.org>, "Okafor, Ben" <ben@example.org>; cleo@example.org,,"#
            ),
            vec![
                String::from("Ada <ada@example.org>"),
                String::from(r#""Okafor, Ben" <ben@example.org>"#),
                String::from("cleo@example.org"),
            ]
        );
        assert!(split_address_line("  ").is_empty());
        assert_eq!(
            split_address_line("a@example.org\nb@example.org\r\n"),
            vec![String::from("a@example.org"), String::from("b@example.org")],
            "a pasted column of addresses"
        );
        assert_eq!(
            split_address_line(r#""Say \"hi\", Ada" <ada@example.org>, b@example.org"#),
            vec![
                String::from(r#""Say \"hi\", Ada" <ada@example.org>"#),
                String::from("b@example.org")
            ],
            "an escaped quote does not end the name"
        );
    }

    #[test]
    fn an_entry_reads_apart_into_its_name_and_address() {
        assert_eq!(
            parse_mailbox(r#" "Lovelace, Ada" <ada@example.org> "#),
            Mailbox {
                name: String::from("Lovelace, Ada"),
                address: String::from("ada@example.org"),
            }
        );
        assert_eq!(parse_mailbox("Ada <ada@example.org>").name, "Ada");
        assert_eq!(
            parse_mailbox(r#""Say \"hi\"" <a@b.org>"#).name,
            r#"Say "hi""#
        );
        assert_eq!(
            parse_mailbox("ada@example.org"),
            Mailbox {
                name: String::new(),
                address: String::from("ada@example.org"),
            }
        );
        assert_eq!(
            parse_mailbox("mailto:ana@example.com").address,
            "ana@example.com"
        );
        assert_eq!(
            parse_mailbox("MAILTO:ana@example.com").address,
            "ana@example.com"
        );
    }

    #[test]
    fn the_bare_address_of_an_entry_and_the_same_mailbox() {
        assert_eq!(
            bare_address("Ada <ada@example.org>").as_deref(),
            Some("ada@example.org")
        );
        assert_eq!(
            bare_address(" ada@example.org ").as_deref(),
            Some("ada@example.org")
        );
        assert_eq!(
            bare_address(r#""L, Ada" <ada@example.org>"#).as_deref(),
            Some("ada@example.org")
        );
        assert_eq!(bare_address("Ada Lovelace"), None);
        assert_eq!(bare_address("<>"), None);
        assert_eq!(
            bare_address("Test <x@localhost>").as_deref(),
            Some("x@localhost")
        );
        assert!(same_address("ADA@Example.org", "Ada <ada@example.org>"));
        assert!(!same_address("ada@example.org", "ben@example.org"));
        assert!(!same_address("Ada", "Ada"), "no address, no mailbox");
    }

    #[test]
    fn an_address_list_keeps_each_address_once_and_names_a_bad_entry() {
        // AzCalendar's attendees line.
        assert_eq!(
            address_list("Ana <ana@example.com>, bo@example.org;\n ANA@example.com ; "),
            Ok(vec![
                String::from("ana@example.com"),
                String::from("bo@example.org")
            ])
        );
        assert_eq!(address_list("  "), Ok(Vec::new()));
        assert_eq!(
            address_list("ana@example.com, team"),
            Err(String::from("team"))
        );
        assert_eq!(
            address_list(r#""Lovelace, Ada" <ada@example.org>, bo@example.org"#),
            Ok(vec![
                String::from("ada@example.org"),
                String::from("bo@example.org")
            ])
        );
        assert_eq!(
            address_list("x@localhost"),
            Err(String::from("x@localhost")),
            "what people type needs a dotted domain"
        );
    }

    #[test]
    fn an_address_people_type_has_one_at_and_a_dotted_domain() {
        // AzCalendar's and AzContacts' rule.
        for good in [
            "ana@example.com",
            "a.b+c@mail.example.org",
            " ada@example.org ",
            "a.b+c@mail.example.co.uk",
        ] {
            assert!(is_email(good), "{good:?}");
        }
        for bad in [
            "",
            "ana",
            "ana@",
            "@example.com",
            "ana@example",
            "a na@example.com",
            "<a@b.c>",
            "a@b@c.de",
            "ada@ex ample.org",
            "ada@.example.org",
            "ada@example.org.",
            "ada@example..org",
            "a,b@example.org",
            "a\"b@example.org",
            "a\u{7}b@example.org",
        ] {
            assert!(!is_email(bad), "{bad:?}");
        }
    }

    #[test]
    fn an_account_address_may_name_a_host_without_a_dot() {
        // AzMail's rule.
        for good in [
            " ada@example.org ",
            "a.b+c@mail.example.co.uk",
            "x@localhost",
        ] {
            assert!(is_email_any_host(good), "{good:?}");
        }
        for bad in [
            "",
            "ada",
            "@example.org",
            "ada@",
            "a@b@c",
            "a d@example.org",
            "ada@ex ample.org",
        ] {
            assert!(!is_email_any_host(bad), "{bad:?}");
        }
        assert_eq!(
            email_domain("Ada@Example.Org").as_deref(),
            Some("example.org")
        );
        assert_eq!(email_domain("ada"), None);
    }
}
