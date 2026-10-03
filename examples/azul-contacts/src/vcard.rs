//! vCard 3.0 (RFC 2426) and 4.0 (RFC 6350): cards of content lines.
//!
//! A card is a list of properties `[group.]NAME[;PARAM=value[,value]]*:value`
//! between `BEGIN:VCARD` and `END:VCARD`. The line format - folding at 75
//! octets, TEXT escaping, parameters in every spelling, groups, structured
//! and list values - is the one iCalendar has too and lives in
//! `azul_pim::content_line` (scripts/DEDUP_EDITORS_2026_10_02.md, B15); a
//! [`Property`] is its `ContentLine`. This module reads and writes the cards.
//!
//! What a property MEANS (a phone, a birthday) is `contact.rs`'s business.

pub use azul_pim::content_line::{
    escape_text, fold, parse_line, unfold, ContentLine as Property,
};

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

/// One card.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Card {
    pub version: Version,
    /// In file order, without BEGIN, VERSION and END.
    pub properties: Vec<Property>,
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
