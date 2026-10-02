//! Content lines: the line format iCalendar (RFC 5545, `.ics`) and vCard (RFC 6350 / 2426,
//! `.vcf`) share. AzCalendar's `ics.rs` and AzContacts' `vcard.rs` each had a copy
//! (scripts/DEDUP_EDITORS_2026_10_02.md, B15); AzMail needs it next (meeting invites as
//! `text/calendar` parts, contact cards as `.vcf` attachments).
//!
//! A line is `[group.]NAME[;PARAM=value[,value]]*:value`:
//! - folding: a line longer than 75 octets continues on lines that start with a blank (a space or
//!   a tab); [`unfold`] joins them, [`fold`] splits at 75 octets without cutting a UTF-8
//!   character, with CRLF and a space;
//! - escaping: in TEXT values `\\`, `\,`, `\;` and `\n` (or `\N`) stand for a backslash, a comma,
//!   a semicolon and a line break ([`escape_text`], [`unescape_text`]);
//! - parameters: `TYPE=work,voice` and `TYPE=work;TYPE=voice` are the same, values may be quoted
//!   (`CN="Doe, Ana: PM"`), a vCard 2.1 bare parameter (`TEL;CELL:`) is a TYPE;
//! - several values: structured values split at unescaped `;` ([`ContentLine::components`]),
//!   lists at unescaped `,` ([`ContentLine::list`]);
//! - a group (`item1.TEL`, Apple's way of labelling one vCard property) is kept apart from the
//!   name.
//!
//! What a line MEANS (an event's start, a phone) is the app's business.

/// The longest a written line is, in octets, before it is folded (RFC 5545 3.1, RFC 6350 3.2).
pub const FOLD_OCTETS: usize = 75;

/// One content line, unfolded.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ContentLine {
    /// `item1` of `item1.TEL`.
    pub group: Option<String>,
    /// Upper case: `TEL`, `DTSTART`, `X-ABLABEL`.
    pub name: String,
    /// Upper-case names, values as given (unquoted, trimmed), in order.
    pub params: Vec<(String, Vec<String>)>,
    /// The value as written (still escaped).
    pub value: String,
}

impl ContentLine {
    /// A line with a raw (already escaped) value.
    #[must_use]
    pub fn new(name: &str, raw_value: &str) -> ContentLine {
        ContentLine {
            group: None,
            name: name.to_ascii_uppercase(),
            params: Vec::new(),
            value: raw_value.to_string(),
        }
    }

    /// A TEXT line: the value escaped.
    #[must_use]
    pub fn text_value(name: &str, text: &str) -> ContentLine {
        ContentLine::new(name, &escape_text(text))
    }

    /// The line with one more parameter.
    #[must_use]
    pub fn with_param(mut self, name: &str, values: &[&str]) -> ContentLine {
        self.params.push((
            name.to_ascii_uppercase(),
            values.iter().map(|v| (*v).to_string()).collect(),
        ));
        self
    }

    /// The values of the first parameter called `name` (any case).
    #[must_use]
    pub fn param(&self, name: &str) -> Option<&[String]> {
        self.params
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_slice())
    }

    /// The first value of the first parameter called `name` (any case): `TZID`, `VALUE`, `CN`.
    #[must_use]
    pub fn param_value(&self, name: &str) -> Option<&str> {
        self.param(name)
            .and_then(|values| values.first())
            .map(String::as_str)
    }

    /// Every TYPE value, lower case, from every TYPE parameter.
    #[must_use]
    pub fn types(&self) -> Vec<String> {
        self.params
            .iter()
            .filter(|(n, _)| n == "TYPE")
            .flat_map(|(_, v)| v.iter())
            .flat_map(|v| v.split(','))
            .map(|v| v.trim().to_ascii_lowercase())
            .filter(|v| !v.is_empty())
            .collect()
    }

    /// The value as text (unescaped).
    #[must_use]
    pub fn text(&self) -> String {
        unescape_text(&self.value)
    }

    /// A structured value's components (`N`, `ADR`, `REQUEST-STATUS`), each unescaped.
    #[must_use]
    pub fn components(&self) -> Vec<String> {
        split_unescaped(&self.value, ';')
            .iter()
            .map(|c| unescape_text(c))
            .collect()
    }

    /// A list value's items (`CATEGORIES`, `NICKNAME`), each unescaped, blanks left out.
    #[must_use]
    pub fn list(&self) -> Vec<String> {
        split_unescaped(&self.value, ',')
            .iter()
            .map(|c| unescape_text(c).trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    }

    /// The line as written, unfolded: a parameter value holding `,` `;` or `:` is quoted.
    #[must_use]
    pub fn to_line(&self) -> String {
        let mut out = String::new();
        if let Some(g) = &self.group {
            out.push_str(g);
            out.push('.');
        }
        out.push_str(&self.name);
        for (name, values) in &self.params {
            out.push(';');
            out.push_str(name);
            out.push('=');
            let joined: Vec<String> = values.iter().map(|v| quote_param(v)).collect();
            out.push_str(&joined.join(","));
        }
        out.push(':');
        out.push_str(&self.value);
        out
    }
}

/// A parameter value, quoted when it holds `,` `;` or `:` (a quote inside becomes `'`: a
/// parameter value cannot hold one).
fn quote_param(v: &str) -> String {
    if v.contains([',', ';', ':']) {
        format!("\"{}\"", v.replace('"', "'"))
    } else {
        v.to_string()
    }
}

/// The logical lines of a text: CRLF or LF line ends; a line starting with a space or a tab
/// continues the line before it (the break and that one blank go). Empty lines are left out.
#[must_use]
pub fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        match raw.strip_prefix([' ', '\t']) {
            Some(rest) if !lines.is_empty() => {
                if let Some(last) = lines.last_mut() {
                    last.push_str(rest);
                }
            }
            _ => {
                if !raw.is_empty() {
                    lines.push(raw.to_string());
                }
            }
        }
    }
    lines
}

/// `line` folded at 75 octets: the first piece up to 75 bytes, every other a space and up to
/// 74 bytes, joined by CRLF, never inside a character. No CRLF at the end.
#[must_use]
pub fn fold(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + line.len() / FOLD_OCTETS * 3);
    let mut used = 0;
    // The first physical line holds 75 octets; each one after it 74 (its space is one).
    let mut room = FOLD_OCTETS;
    for c in line.chars() {
        let len = c.len_utf8();
        if used + len > room {
            out.push_str("\r\n ");
            used = 0;
            room = FOLD_OCTETS - 1;
        }
        out.push(c);
        used += len;
    }
    out
}

/// `text` as a TEXT value: `\`, `;` and `,` escaped, line breaks (CRLF, LF or CR) as `\n`.
#[must_use]
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' | '\r' => out.push_str("\\n"),
            c => out.push(c),
        }
    }
    out
}

/// A TEXT value read back: `\n` / `\N` a line break; `\,` `\;` `\\` `\:` (and any other escaped
/// character) the character; a lone backslash at the end stays.
#[must_use]
pub fn unescape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// `value` split at every `sep` that is not escaped; the pieces keep their escapes.
#[must_use]
pub fn split_unescaped(value: &str, sep: char) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c == sep {
            out.push(String::new());
            continue;
        }
        let Some(last) = out.last_mut() else {
            break;
        };
        last.push(c);
        if c == '\\' {
            // The escaped character stays with its backslash, a separator too.
            if let Some(next) = chars.next() {
                last.push(next);
            }
        }
    }
    out
}

/// Reads one unfolded line. `Err` says why it is not one: no `:` outside quotes, no name, a
/// name that is not letters, digits and `-`.
pub fn parse_line(line: &str) -> Result<ContentLine, String> {
    // The value starts at the first ':' outside a quoted parameter value.
    let mut in_quotes = false;
    let mut colon = None;
    for (i, c) in line.char_indices() {
        match c {
            '"' => in_quotes = !in_quotes,
            ':' if !in_quotes => {
                colon = Some(i);
                break;
            }
            _ => {}
        }
    }
    let colon = colon.ok_or_else(|| format!("no ':' in {line:?}"))?;
    let (head, value) = (&line[..colon], &line[colon + 1..]);
    // The head: name and parameters, split at ';' outside quotes.
    let mut parts: Vec<String> = vec![String::new()];
    let mut in_quotes = false;
    for c in head.chars() {
        match c {
            ';' if !in_quotes => parts.push(String::new()),
            c => {
                if c == '"' {
                    in_quotes = !in_quotes;
                }
                if let Some(last) = parts.last_mut() {
                    last.push(c);
                }
            }
        }
    }
    let full_name = parts[0].trim();
    if full_name.is_empty() {
        return Err(format!("no property name in {line:?}"));
    }
    let (group, name) = match full_name.rsplit_once('.') {
        Some((g, n)) => (Some(g.to_string()), n),
        None => (None, full_name),
    };
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(format!("{name:?} is not a property name"));
    }
    let mut params = Vec::new();
    for part in &parts[1..] {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (pname, pvalue) = match part.split_once('=') {
            Some((n, v)) => (n.trim().to_ascii_uppercase(), v),
            // vCard 2.1: `TEL;CELL;VOICE:` - bare types.
            None => ("TYPE".to_string(), part),
        };
        params.push((pname, split_param_values(pvalue)));
    }
    Ok(ContentLine {
        group,
        name: name.to_ascii_uppercase(),
        params,
        value: value.to_string(),
    })
}

/// `a,"b,c",d` -> `a`, `b,c`, `d` (quotes taken off, each trimmed).
fn split_param_values(raw: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut in_quotes = false;
    for c in raw.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => out.push(String::new()),
            other => {
                if let Some(last) = out.last_mut() {
                    last.push(other);
                }
            }
        }
    }
    out.into_iter().map(|v| v.trim().to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_lines_unfold_with_a_space_or_a_tab_and_empty_lines_go() {
        // AzContacts' vcard.rs and AzCalendar's ics.rs tests.
        let text = "BEGIN:VCARD\r\nNOTE:Prefers\r\n  Signal.\r\nFN:Ro\n\tbin\r\nEND:VCARD\r\n";
        assert_eq!(
            unfold(text),
            vec![
                "BEGIN:VCARD",
                "NOTE:Prefers Signal.",
                "FN:Robin",
                "END:VCARD"
            ]
        );
        let text = "SUMMARY:Team\r\n  sync\nDESCRIPTION:a\n\tb\r\n\r\nLOCATION:x\r\n";
        assert_eq!(
            unfold(text),
            vec!["SUMMARY:Team sync", "DESCRIPTION:ab", "LOCATION:x"]
        );
        assert_eq!(
            unfold(" lone:start"),
            vec![" lone:start"],
            "nothing to continue"
        );
    }

    #[test]
    fn a_long_line_folds_at_75_octets_and_unfolds_back() {
        let line = format!("NOTE:{}", "abcdefghij".repeat(20));
        let folded = fold(&line);
        for (i, piece) in folded.split("\r\n").enumerate() {
            assert!(piece.len() <= 75, "piece {i} has {} octets", piece.len());
            assert_eq!(piece.starts_with(' '), i > 0);
        }
        assert_eq!(unfold(&folded), vec![line]);
    }

    #[test]
    fn folding_never_cuts_a_character() {
        for line in [
            format!("FN:{}", "\u{738b}\u{82b3}\u{fc}".repeat(30)),
            format!("SUMMARY:{}", "\u{20ac}\u{e4}".repeat(40)),
        ] {
            let folded = fold(&line);
            for piece in folded.split("\r\n") {
                assert!(piece.len() <= 75, "{} octets", piece.len());
                assert!(std::str::from_utf8(piece.as_bytes()).is_ok());
            }
            assert_eq!(unfold(&folded), vec![line]);
        }
        assert_eq!(fold("FN:short"), "FN:short");
    }

    #[test]
    fn text_values_escape_and_unescape() {
        let text = "Musterweg 1, Hinterhaus; 2. OG\nBerlin \\ Mitte";
        let escaped = escape_text(text);
        assert_eq!(
            escaped,
            "Musterweg 1\\, Hinterhaus\\; 2. OG\\nBerlin \\\\ Mitte"
        );
        assert_eq!(unescape_text(&escaped), text);
        assert_eq!(unescape_text("a\\Nb\\:c\\x"), "a\nb:cx");
        assert_eq!(unescape_text("one\\Ntwo"), "one\ntwo");
        assert_eq!(unescape_text("end\\"), "end\\");
        assert_eq!(escape_text("a\r\nb"), "a\\nb");
        assert_eq!(escape_text("a\rb"), "a\\nb");
    }

    #[test]
    fn structured_and_list_values_split_at_unescaped_separators() {
        let n = parse_line("N:Weber;Robin;;Dr.;").unwrap();
        assert_eq!(n.components(), vec!["Weber", "Robin", "", "Dr.", ""]);
        let adr =
            parse_line("ADR;TYPE=home:;;Musterweg 1\\, Hinterhaus;Berlin;;10115;Germany").unwrap();
        assert_eq!(adr.components()[2], "Musterweg 1, Hinterhaus");
        assert_eq!(adr.components()[5], "10115");
        let cats = parse_line("CATEGORIES:Work,Book club,Rock\\, Paper").unwrap();
        assert_eq!(cats.list(), vec!["Work", "Book club", "Rock, Paper"]);
    }

    #[test]
    fn parameters_in_every_spelling() {
        let a = parse_line("TEL;TYPE=WORK,VOICE:+49 30 0000 0002").unwrap();
        let b = parse_line("TEL;TYPE=work;TYPE=voice:+49 30 0000 0002").unwrap();
        let c = parse_line("TEL;TYPE=\"work,voice\":+49 30 0000 0002").unwrap();
        let old = parse_line("TEL;WORK;VOICE:+49 30 0000 0002").unwrap();
        for p in [&a, &b, &c, &old] {
            assert_eq!(p.types(), vec!["work", "voice"], "{p:?}");
            assert_eq!(p.value, "+49 30 0000 0002");
        }
        let label =
            parse_line("ADR;LABEL=\"Musterweg 1, 10115 Berlin: Germany\";TYPE=home:;;x;;;;")
                .unwrap();
        assert_eq!(
            label.param("label").unwrap(),
            ["Musterweg 1, 10115 Berlin: Germany"]
        );
        assert_eq!(label.types(), vec!["home"]);
    }

    #[test]
    fn a_line_keeps_quoted_parameters_and_colons_in_its_value() {
        // AzCalendar's ics.rs test.
        let line =
            parse_line("ATTENDEE;CN=\"Doe, Ana: PM\";role=REQ-PARTICIPANT:mailto:ana@example.com")
                .unwrap();
        assert_eq!(line.name, "ATTENDEE");
        assert_eq!(line.param_value("CN"), Some("Doe, Ana: PM"));
        assert_eq!(line.param_value("ROLE"), Some("REQ-PARTICIPANT"));
        assert_eq!(line.param_value("cn"), Some("Doe, Ana: PM"), "any case");
        assert_eq!(line.param_value("TZID"), None);
        assert_eq!(line.value, "mailto:ana@example.com");
        let p = parse_line("URL:https://example.org:8080/a").unwrap();
        assert_eq!(p.value, "https://example.org:8080/a");
        let photo = parse_line("PHOTO:data:image/png;base64,iVBORw0KGgo=").unwrap();
        assert_eq!(photo.value, "data:image/png;base64,iVBORw0KGgo=");
        let zoned = parse_line("DTSTART;TZID=\"Europe/Berlin\":20261005T090000").unwrap();
        assert_eq!(zoned.param_value("TZID"), Some("Europe/Berlin"));
    }

    #[test]
    fn a_line_without_a_colon_or_a_name_is_refused() {
        assert!(parse_line("no colon here").is_err());
        assert!(parse_line(":value").is_err());
        assert!(parse_line("BAD NAME:value").is_err());
        assert!(parse_line("item1.:value").is_err());
    }

    #[test]
    fn a_group_prefix_is_kept_apart_from_the_name() {
        let p = parse_line("item1.TEL;type=pref:+49 151 0000 0001").unwrap();
        assert_eq!(p.group.as_deref(), Some("item1"));
        assert_eq!(p.name, "TEL");
        assert_eq!(p.types(), vec!["pref"]);
        let l = parse_line("item1.X-ABLabel:_$!<Mobile>!$_").unwrap();
        assert_eq!(l.name, "X-ABLABEL");
    }

    #[test]
    fn a_line_writes_back_what_it_read() {
        let line = ContentLine::new("TEL", "tel:+49-151-0000-0001")
            .with_param("TYPE", &["cell"])
            .with_param("VALUE", &["uri"]);
        assert_eq!(
            line.to_line(),
            "TEL;TYPE=cell;VALUE=uri:tel:+49-151-0000-0001"
        );
        let note = ContentLine::text_value("NOTE", "a, b").with_param("X-LABEL", &["x,y"]);
        assert_eq!(note.to_line(), "NOTE;X-LABEL=\"x,y\":a\\, b");
        assert_eq!(parse_line(&note.to_line()), Ok(note.clone()));
        assert_eq!(note.text(), "a, b");
        let mut grouped = parse_line("item2.EMAIL;TYPE=work:a@example.org").unwrap();
        assert_eq!(grouped.to_line(), "item2.EMAIL;TYPE=work:a@example.org");
        grouped.group = None;
        assert_eq!(grouped.to_line(), "EMAIL;TYPE=work:a@example.org");
    }
}
