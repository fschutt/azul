//! vCard 3.0 (RFC 2426) and 4.0 (RFC 6350) at the content-line level.
//!
//! A card is a list of properties `[group.]NAME[;PARAM=value[,value]]*:value`
//! between `BEGIN:VCARD` and `END:VCARD`. This module reads and writes them:
//! - folding: lines longer than 75 octets continue on lines that start with a
//!   blank (a space or a tab); [`unfold`] joins them, [`fold`] splits at 75
//!   octets without cutting a UTF-8 character;
//! - escaping: in text values `\\`, `\,`, `\;` and `\n` stand for a
//!   backslash, a comma, a semicolon and a line break ([`escape_text`],
//!   [`unescape`]);
//! - parameters: `TYPE=work,voice` and `TYPE=work;TYPE=voice` are the same,
//!   values may be quoted (`LABEL="Musterweg 1, Berlin"`), a vCard 2.1 bare
//!   parameter (`TEL;CELL:`) is a TYPE;
//! - multiple values: structured values split at unescaped `;`
//!   ([`Property::components`]), lists at unescaped `,` ([`Property::list`]).
//!
//! What a property MEANS (a phone, a birthday) is `contact.rs`'s business.

/// The vCard version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Version {
    #[default]
    V3,
    V4,
}

impl Version {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Version::V3 => "3.0",
            Version::V4 => "4.0",
        }
    }
}

/// One content line.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Property {
    /// `item1` of `item1.TEL` (Apple's way of labelling one property).
    pub group: Option<String>,
    /// Upper-case: `TEL`, `X-ABLABEL`.
    pub name: String,
    /// Upper-case names, values as given (unquoted), in order.
    pub params: Vec<(String, Vec<String>)>,
    /// The value as written (still escaped).
    pub value: String,
}

/// One card.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Card {
    pub version: Version,
    /// In file order, without BEGIN, VERSION and END.
    pub properties: Vec<Property>,
}

impl Property {
    /// A property with a raw (already escaped) value.
    #[must_use]
    pub fn new(name: &str, raw_value: &str) -> Property {
        Property {
            group: None,
            name: name.to_ascii_uppercase(),
            params: Vec::new(),
            value: raw_value.to_string(),
        }
    }

    /// A text property: the value escaped.
    #[must_use]
    pub fn text_value(name: &str, text: &str) -> Property {
        Property::new(name, &escape_text(text))
    }

    /// Adds a parameter.
    #[must_use]
    pub fn with_param(mut self, name: &str, values: &[&str]) -> Property {
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

    /// Every TYPE value, lower-case, from every TYPE parameter.
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
        unescape(&self.value)
    }

    /// A structured value's components (`N`, `ADR`), each unescaped.
    #[must_use]
    pub fn components(&self) -> Vec<String> {
        split_unescaped(&self.value, ';').iter().map(|c| unescape(c)).collect()
    }

    /// A list value's items (`CATEGORIES`, `NICKNAME`), each unescaped, blanks dropped.
    #[must_use]
    pub fn list(&self) -> Vec<String> {
        split_unescaped(&self.value, ',')
            .iter()
            .map(|c| unescape(c).trim().to_string())
            .filter(|c| !c.is_empty())
            .collect()
    }

    /// The content line, unfolded.
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

impl Card {
    /// The first property called `name` (any case).
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name.eq_ignore_ascii_case(name))
    }

    /// Every property called `name` (any case).
    #[must_use]
    pub fn all(&self, name: &str) -> Vec<&Property> {
        self.properties
            .iter()
            .filter(|p| p.name.eq_ignore_ascii_case(name))
            .collect()
    }
}

/// A parameter value, quoted when it holds `,` `;` or `:`.
fn quote_param(v: &str) -> String {
    if v.contains([',', ';', ':']) {
        format!("\"{}\"", v.replace('"', "'"))
    } else {
        v.to_string()
    }
}

/// The logical lines of a text: CRLF or LF line ends, a line starting with a
/// space or a tab continues the line before it (the blank is dropped).
#[must_use]
pub fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix([' ', '\t']) {
            if let Some(last) = out.last_mut() {
                last.push_str(rest);
                continue;
            }
        }
        out.push(line.to_string());
    }
    while out.last().is_some_and(String::is_empty) {
        out.pop();
    }
    out
}

/// A line folded at 75 octets: the first piece up to 75 bytes, every other
/// a space and up to 74 bytes, joined by CRLF, never inside a character.
#[must_use]
pub fn fold(line: &str) -> String {
    const LIMIT: usize = 75;
    let mut out = String::with_capacity(line.len() + line.len() / LIMIT * 3);
    let mut current = 0usize;
    let mut first = true;
    for c in line.chars() {
        let width = c.len_utf8();
        let limit = if first { LIMIT } else { LIMIT - 1 };
        if current + width > limit {
            out.push_str("\r\n ");
            current = 0;
            first = false;
        }
        out.push(c);
        current += width;
    }
    out
}

/// A text value escaped: backslash, comma, semicolon, line break.
#[must_use]
pub fn escape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            ',' => out.push_str("\\,"),
            ';' => out.push_str("\\;"),
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str("\\n");
            }
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out
}

/// The text of an escaped value: `\n` / `\N` a line break, `\\` `\,` `\;`
/// `\:` the character; an unknown escape keeps the character after the backslash.
#[must_use]
pub fn unescape(value: &str) -> String {
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
        if c == '\\' {
            let last = out.last_mut().expect("never empty");
            last.push(c);
            if let Some(next) = chars.next() {
                last.push(next);
            }
        } else if c == sep {
            out.push(String::new());
        } else {
            out.last_mut().expect("never empty").push(c);
        }
    }
    out
}

/// One unfolded content line.
pub fn parse_line(line: &str) -> Result<Property, String> {
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
            '"' => {
                in_quotes = !in_quotes;
                parts.last_mut().expect("never empty").push(c);
            }
            ';' if !in_quotes => parts.push(String::new()),
            other => parts.last_mut().expect("never empty").push(other),
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
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
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
        let values = split_param_values(pvalue);
        params.push((pname, values));
    }
    Ok(Property {
        group,
        name: name.to_ascii_uppercase(),
        params,
        value: value.to_string(),
    })
}

/// `a,"b,c",d` -> `a`, `b,c`, `d` (quotes removed).
fn split_param_values(raw: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut in_quotes = false;
    for c in raw.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => out.push(String::new()),
            other => out.last_mut().expect("never empty").push(other),
        }
    }
    out.into_iter().map(|v| v.trim().to_string()).collect()
}

/// Every card of a text (a `.vcf` file may hold many), and what could not be
/// read (a line that is not a property, a card without END).
#[must_use]
pub fn parse(text: &str) -> (Vec<Card>, Vec<String>) {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut cards = Vec::new();
    let mut problems = Vec::new();
    let mut current: Option<Card> = None;
    for (n, line) in unfold(text).into_iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let upper = line.trim().to_ascii_uppercase();
        if upper == "BEGIN:VCARD" {
            if current.is_some() {
                problems.push(format!("line {}: a card without END:VCARD", n + 1));
            }
            current = Some(Card::default());
            continue;
        }
        if upper == "END:VCARD" {
            match current.take() {
                Some(card) => cards.push(card),
                None => problems.push(format!("line {}: END:VCARD without BEGIN", n + 1)),
            }
            continue;
        }
        let Some(card) = current.as_mut() else {
            continue; // text between cards
        };
        match parse_line(&line) {
            Ok(p) if p.name == "VERSION" => {
                card.version = match p.value.trim() {
                    "4.0" => Version::V4,
                    "3.0" => Version::V3,
                    other => {
                        problems.push(format!("line {}: vCard {other} read as 3.0", n + 1));
                        Version::V3
                    }
                };
            }
            Ok(p) => card.properties.push(p),
            Err(e) => problems.push(format!("line {}: {e}", n + 1)),
        }
    }
    if current.is_some() {
        problems.push("the last card has no END:VCARD".to_string());
    }
    (cards, problems)
}

/// A card as text: BEGIN, VERSION, the properties, END; folded, CRLF line ends.
#[must_use]
pub fn write(card: &Card) -> String {
    let mut out = String::new();
    let mut push = |line: &str| {
        out.push_str(&fold(line));
        out.push_str("\r\n");
    };
    push("BEGIN:VCARD");
    push(&format!("VERSION:{}", card.version.label()));
    for p in card.properties.iter().filter(|p| p.name != "VERSION") {
        push(&p.to_line());
    }
    push("END:VCARD");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folded_lines_unfold_with_a_space_or_a_tab() {
        let text = "BEGIN:VCARD\r\nNOTE:Prefers\r\n  Signal.\r\nFN:Ro\n\tbin\r\nEND:VCARD\r\n";
        assert_eq!(
            unfold(text),
            vec!["BEGIN:VCARD", "NOTE:Prefers Signal.", "FN:Robin", "END:VCARD"]
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
        let line = format!("FN:{}", "\u{738b}\u{82b3}\u{fc}".repeat(30));
        let folded = fold(&line);
        for piece in folded.split("\r\n") {
            assert!(piece.len() <= 75);
            assert!(std::str::from_utf8(piece.as_bytes()).is_ok());
        }
        assert_eq!(unfold(&folded), vec![line]);
        assert_eq!(fold("FN:short"), "FN:short");
    }

    #[test]
    fn text_values_escape_and_unescape() {
        let text = "Musterweg 1, Hinterhaus; 2. OG\nBerlin \\ Mitte";
        let escaped = escape_text(text);
        assert_eq!(escaped, "Musterweg 1\\, Hinterhaus\\; 2. OG\\nBerlin \\\\ Mitte");
        assert_eq!(unescape(&escaped), text);
        assert_eq!(unescape("a\\Nb\\:c\\x"), "a\nb:cx");
        assert_eq!(escape_text("a\r\nb"), "a\\nb");
    }

    #[test]
    fn structured_and_list_values_split_at_unescaped_separators() {
        let n = parse_line("N:Weber;Robin;;Dr.;").unwrap();
        assert_eq!(n.components(), vec!["Weber", "Robin", "", "Dr.", ""]);
        let adr = parse_line("ADR;TYPE=home:;;Musterweg 1\\, Hinterhaus;Berlin;;10115;Germany").unwrap();
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
        let label = parse_line("ADR;LABEL=\"Musterweg 1, 10115 Berlin: Germany\";TYPE=home:;;x;;;;").unwrap();
        assert_eq!(label.param("label").unwrap(), ["Musterweg 1, 10115 Berlin: Germany"]);
        assert_eq!(label.types(), vec!["home"]);
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
    fn a_value_may_contain_colons() {
        let p = parse_line("URL:https://example.org:8080/a").unwrap();
        assert_eq!(p.value, "https://example.org:8080/a");
        let photo = parse_line("PHOTO:data:image/png;base64,iVBORw0KGgo=").unwrap();
        assert_eq!(photo.value, "data:image/png;base64,iVBORw0KGgo=");
        assert!(parse_line("no colon here").is_err());
        assert!(parse_line(":value").is_err());
    }

    #[test]
    fn several_cards_in_one_file_and_their_versions() {
        let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:A\r\nEND:VCARD\r\n\r\nBEGIN:VCARD\r\nVERSION:4.0\r\nFN:B\r\nEND:VCARD\r\n";
        let (cards, problems) = parse(text);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].version, Version::V3);
        assert_eq!(cards[1].version, Version::V4);
        assert_eq!(cards[1].get("fn").unwrap().text(), "B");
    }

    #[test]
    fn broken_input_is_reported_not_fatal() {
        let (cards, problems) = parse("BEGIN:VCARD\nVERSION:2.1\nFN:A\nthis is not a property\nEND:VCARD\nBEGIN:VCARD\nFN:B\n");
        assert_eq!(cards.len(), 1);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(problems[0].contains("2.1"));
        assert!(problems[2].contains("no END"));
    }

    #[test]
    fn writing_puts_begin_and_version_first_with_crlf_ends() {
        let card = Card {
            version: Version::V4,
            properties: vec![
                Property::text_value("FN", "Robin Weber"),
                Property::new("TEL", "tel:+49-151-0000-0001").with_param("TYPE", &["cell"]).with_param("VALUE", &["uri"]),
                Property::new("NOTE", &escape_text("a, b")).with_param("X-LABEL", &["x,y"]),
            ],
        };
        let text = write(&card);
        let lines: Vec<&str> = text.split("\r\n").collect();
        assert_eq!(lines[0], "BEGIN:VCARD");
        assert_eq!(lines[1], "VERSION:4.0");
        assert_eq!(lines[3], "TEL;TYPE=cell;VALUE=uri:tel:+49-151-0000-0001");
        assert_eq!(lines[4], "NOTE;X-LABEL=\"x,y\":a\\, b");
        assert_eq!(lines[5], "END:VCARD");
        assert!(text.ends_with("END:VCARD\r\n"));
        let (back, problems) = parse(&text);
        assert!(problems.is_empty());
        assert_eq!(back, vec![card]);
    }
}
