//! A contact, and how it is written as a vCard 3.0 or 4.0.
//!
//! The fields people edit: the name parts, company, department and job
//! title, several phones, emails, addresses and web pages each with a label
//! (`mobile`, `work`, `home`, `other` or the user's own), a birthday with or
//! without a year, notes, a photo (a `data:` URI or a link), the groups
//! (`CATEGORIES`), a favourite flag and custom fields. Properties the model
//! does not know (`IMPP`, `X-SOCIALPROFILE`, ...) are kept and written back
//! unchanged, so a round trip through AzContacts loses nothing.
//!
//! Labels: the TYPE parameter (`CELL` / `cell` is `mobile`; `voice`, `pref`,
//! `internet` say nothing about the label), or Apple's grouped `X-ABLabel`
//! (`item1.TEL` + `item1.X-ABLabel:_$!<Mobile>!$_`). A label vCard has no
//! type for is written as `x-<label>`.

use crate::vcard::{self, escape_text, Card, Property, Version};

/// A value with its label: a phone, an email, a web page, a custom field.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Labeled {
    pub label: String,
    pub value: String,
}

impl Labeled {
    #[must_use]
    pub fn new(label: &str, value: &str) -> Labeled {
        Labeled {
            label: label.to_string(),
            value: value.to_string(),
        }
    }
}

/// A postal address (vCard's seven ADR components).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Address {
    pub label: String,
    pub po_box: String,
    pub extended: String,
    pub street: String,
    pub locality: String,
    pub region: String,
    pub postcode: String,
    pub country: String,
}

impl Address {
    /// The lines on an envelope: street, `postcode locality`, region, country.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        todo!("RED: lines")
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lines().is_empty()
    }
}

/// A birthday, the year optional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Birthday {
    pub year: Option<i32>,
    pub month: u32,
    pub day: u32,
}

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
    "November", "December",
];

impl Birthday {
    /// `1987-03-14`, `19870314`, `--0314`, `--03-14`, `1987-03-14T00:00:00Z`;
    /// the day-first `14.03.1987` / `14.03.` the edit form takes too.
    #[must_use]
    pub fn parse(text: &str) -> Option<Birthday> {
        todo!("RED: parse")
    }

    /// As vCard writes it: 4.0 `19870314` / `--0314`, 3.0 `1987-03-14` / `--03-14`.
    #[must_use]
    pub fn to_vcard(&self, version: Version) -> String {
        todo!("RED: to_vcard")
    }

    /// `14 March 1987`, `14 March`.
    #[must_use]
    pub fn describe(&self) -> String {
        todo!("RED: describe")
    }

    /// The edit form's text: `14.03.1987` or `14.03.`.
    #[must_use]
    pub fn to_form(&self) -> String {
        match self.year {
            Some(y) => format!("{:02}.{:02}.{y:04}", self.day, self.month),
            None => format!("{:02}.{:02}.", self.day, self.month),
        }
    }
}

trait YearCheck {
    fn is_none_or_valid(&self) -> bool;
}

impl YearCheck for Option<i32> {
    fn is_none_or_valid(&self) -> bool {
        self.map_or(true, |y| (1..=9999).contains(&y))
    }
}

/// One contact.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Contact {
    /// The file name and vCard UID (a UUID).
    pub uid: String,
    pub prefix: String,
    pub given: String,
    pub additional: String,
    pub family: String,
    pub suffix: String,
    /// The vCard FN when it is not just the name parts (a company card, a
    /// name written only as FN).
    pub formatted: String,
    pub nickname: String,
    pub org: String,
    pub department: String,
    pub title: String,
    pub phones: Vec<Labeled>,
    pub emails: Vec<Labeled>,
    pub addresses: Vec<Address>,
    pub urls: Vec<Labeled>,
    pub birthday: Option<Birthday>,
    pub notes: String,
    /// A `data:` URI or a link.
    pub photo: String,
    pub groups: Vec<String>,
    pub favorite: bool,
    pub custom: Vec<Labeled>,
    /// What the model does not know, written back as read.
    pub extra: Vec<Property>,
}

/// The label of a TYPE list, or `None` if it names none.
fn label_of_types(types: &[String]) -> Option<String> {
        todo!("RED: label_of_types")
    }

/// Apple's label text without its `_$!<...>!$_` wrapper, lower-case.
fn clean_ab_label(text: &str) -> String {
    let t = text.trim();
    let t = t
        .strip_prefix("_$!<")
        .and_then(|r| r.strip_suffix(">!$_"))
        .unwrap_or(t);
    t.to_lowercase()
}

/// The TYPE a label is written with.
fn type_of_label(label: &str, version: Version, phone: bool) -> String {
        todo!("RED: type_of_label")
    }

fn nonempty(s: &str) -> Option<&str> {
    let t = s.trim();
    (!t.is_empty()).then_some(t)
}

impl Contact {
    /// The name parts in reading order: `Dr. Robin A. Weber Jr.`.
    #[must_use]
    pub fn composed_name(&self) -> String {
        [&self.prefix, &self.given, &self.additional, &self.family, &self.suffix]
            .iter()
            .filter_map(|s| nonempty(s))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The name the list shows: the name parts, else FN, else the company,
    /// else the first email or phone.
    #[must_use]
    pub fn display_name(&self) -> String {
        todo!("RED: display_name")
    }

    /// A company card: no personal name, a company.
    #[must_use]
    pub fn is_company(&self) -> bool {
        nonempty(&self.given).is_none() && nonempty(&self.family).is_none() && nonempty(&self.org).is_some()
    }

    /// The line under the name: `Product lead · Northwind`.
    #[must_use]
    pub fn subtitle(&self) -> String {
        if self.is_company() {
            return self.department.trim().to_string();
        }
        [self.title.trim(), self.org.trim()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" \u{b7} ")
    }

    /// What the edit form refuses: no name at all, an email without `@` and
    /// a dot after it, a birthday that is not a date.
    #[must_use]
    pub fn problems(&self, birthday_text: Option<&str>) -> Vec<String> {
        todo!("RED: problems")
    }

    /// Reads one card.
    #[must_use]
    pub fn from_card(card: &Card) -> Contact {
        todo!("RED: from_card")
    }

    /// The card of this contact in `version`.
    #[must_use]
    pub fn to_card(&self, version: Version) -> Card {
        todo!("RED: to_card")
    }

    /// The contact as `.vcf` text.
    #[must_use]
    pub fn to_vcf(&self, version: Version) -> String {
        vcard::write(&self.to_card(version))
    }
}

/// PHOTO: 4.0 a URI (`data:` or a link); 3.0 inline base64 with ENCODING=b, or VALUE=uri.
fn photo_property(photo: &str, version: Version) -> Property {
        todo!("RED: photo_property")
    }

/// Every contact of a `.vcf` text, and what could not be read.
#[must_use]
pub fn parse_vcf(text: &str) -> (Vec<Contact>, Vec<String>) {
    let (cards, problems) = vcard::parse(text);
    (cards.iter().map(Contact::from_card).collect(), problems)
}

/// Several contacts as one `.vcf` text (an export).
#[must_use]
pub fn write_vcf(contacts: &[Contact], version: Version) -> String {
    contacts.iter().map(|c| c.to_vcf(version)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plan's sample card.
    fn robin() -> Contact {
        Contact {
            uid: "0b8f8a4e-6f0e-4c39-9a51-6c0f1e2d3a4b".to_string(),
            given: "Robin".to_string(),
            family: "Weber".to_string(),
            org: "Northwind".to_string(),
            title: "Product lead".to_string(),
            phones: vec![Labeled::new("mobile", "+49 151 0000 0001"), Labeled::new("work", "+49 30 0000 0002")],
            emails: vec![Labeled::new("work", "robin@example.org")],
            addresses: vec![Address {
                label: "home".to_string(),
                street: "Musterweg 1".to_string(),
                locality: "Berlin".to_string(),
                postcode: "10115".to_string(),
                country: "Germany".to_string(),
                ..Address::default()
            }],
            birthday: Some(Birthday {
                year: Some(1987),
                month: 3,
                day: 14,
            }),
            notes: "Prefers Signal.\nAsk about the book club, first.".to_string(),
            groups: vec!["Work".to_string(), "Book club".to_string()],
            favorite: true,
            custom: vec![Labeled::new("Shoe size", "44")],
            ..Contact::default()
        }
    }

    #[test]
    fn the_sample_card_round_trips_in_both_versions() {
        for version in [Version::V3, Version::V4] {
            let text = robin().to_vcf(version);
            let (back, problems) = parse_vcf(&text);
            assert!(problems.is_empty(), "{problems:?}");
            assert_eq!(back, vec![robin()], "vCard {}:\n{text}", version.label());
        }
    }

    #[test]
    fn version_3_and_4_spell_types_and_birthdays_their_way() {
        let v3 = robin().to_vcf(Version::V3);
        assert!(v3.contains("VERSION:3.0\r\n"));
        assert!(v3.contains("TEL;TYPE=CELL,VOICE:+49 151 0000 0001\r\n"), "{v3}");
        assert!(v3.contains("EMAIL;TYPE=INTERNET,WORK:robin@example.org\r\n"));
        assert!(v3.contains("BDAY:1987-03-14\r\n"));
        assert!(v3.contains("UID:0b8f8a4e-"));
        let v4 = robin().to_vcf(Version::V4);
        assert!(v4.contains("VERSION:4.0\r\n"));
        assert!(v4.contains("TEL;TYPE=cell:+49 151 0000 0001\r\n"), "{v4}");
        assert!(v4.contains("BDAY:19870314\r\n"));
        assert!(v4.contains("UID:urn:uuid:0b8f8a4e-"));
        assert!(v4.contains("N:Weber;Robin;;;\r\n"));
        assert!(v4.contains("ADR;TYPE=home:;;Musterweg 1;Berlin;;10115;Germany\r\n"));
        assert!(v4.contains("NOTE:Prefers Signal.\\nAsk about the book club\\, first.\r\n"));
        assert!(v4.contains("CATEGORIES:Work,Book club\r\n"));
        assert!(v4.contains("X-AZLIN-FIELD;X-LABEL=Shoe size:44\r\n"));
    }

    #[test]
    fn labels_come_from_types_or_apples_grouped_labels() {
        let text = "BEGIN:VCARD\r\nVERSION:3.0\r\nFN:A B\r\nN:B;A;;;\r\n\
                    TEL;TYPE=CELL,VOICE:1\r\nTEL;TYPE=pref:2\r\nitem1.TEL:3\r\nitem1.X-ABLabel:_$!<Mobile>!$_\r\n\
                    item2.TEL:4\r\nitem2.X-ABLabel:Boat\r\nEMAIL;TYPE=INTERNET,WORK:a@example.org\r\n\
                    TEL;TYPE=x-assistant:5\r\nEND:VCARD\r\n";
        let (contacts, _) = parse_vcf(text);
        let labels: Vec<&str> = contacts[0].phones.iter().map(|p| p.label.as_str()).collect();
        assert_eq!(labels, vec!["mobile", "other", "mobile", "boat", "assistant"]);
        assert_eq!(contacts[0].emails[0].label, "work");
        assert!(contacts[0].extra.is_empty(), "applied labels are not kept as extras: {:?}", contacts[0].extra);
    }

    #[test]
    fn a_custom_label_is_written_as_an_x_type() {
        let c = Contact {
            given: "A".into(),
            phones: vec![Labeled::new("assistant", "5"), Labeled::new("main", "6")],
            ..Contact::default()
        };
        assert!(c.to_vcf(Version::V4).contains("TEL;TYPE=x-assistant:5"));
        assert!(c.to_vcf(Version::V3).contains("TEL;TYPE=X-MAIN,VOICE:6"));
        let (back, _) = parse_vcf(&c.to_vcf(Version::V3));
        assert_eq!(back[0].phones[0].label, "assistant");
        assert_eq!(back[0].phones[1].label, "main");
    }

    #[test]
    fn birthdays_with_and_without_a_year() {
        let b = |s: &str| Birthday::parse(s);
        let full = Some(Birthday { year: Some(1987), month: 3, day: 14 });
        let no_year = Some(Birthday { year: None, month: 3, day: 14 });
        assert_eq!(b("1987-03-14"), full);
        assert_eq!(b("19870314"), full);
        assert_eq!(b("1987-03-14T00:00:00Z"), full);
        assert_eq!(b("14.03.1987"), full);
        assert_eq!(b("--0314"), no_year);
        assert_eq!(b("--03-14"), no_year);
        assert_eq!(b("14.03."), no_year);
        assert_eq!(b("1987-02-30"), None);
        assert_eq!(b("soon"), None);
        assert_eq!(full.unwrap().describe(), "14 March 1987");
        assert_eq!(no_year.unwrap().describe(), "14 March");
        assert_eq!(no_year.unwrap().to_vcard(Version::V4), "--0314");
        assert_eq!(no_year.unwrap().to_form(), "14.03.");
        let apple = "BEGIN:VCARD\nVERSION:3.0\nFN:X\nBDAY;X-APPLE-OMIT-YEAR=1604:1604-03-14\nEND:VCARD\n";
        assert_eq!(parse_vcf(apple).0[0].birthday, no_year);
    }

    #[test]
    fn photos_move_between_inline_base64_and_data_uris() {
        let v3 = "BEGIN:VCARD\nVERSION:3.0\nFN:X\nPHOTO;ENCODING=b;TYPE=JPEG:/9j/4AAQ\nEND:VCARD\n";
        let c = &parse_vcf(v3).0[0];
        assert_eq!(c.photo, "data:image/jpeg;base64,/9j/4AAQ");
        assert!(c.to_vcf(Version::V3).contains("PHOTO;ENCODING=b;TYPE=JPEG:/9j/4AAQ\r\n"));
        assert!(c.to_vcf(Version::V4).contains("PHOTO:data:image/jpeg;base64,/9j/4AAQ\r\n"));
        let link = Contact {
            given: "Y".into(),
            photo: "https://example.org/y.png".into(),
            ..Contact::default()
        };
        assert!(link.to_vcf(Version::V3).contains("PHOTO;VALUE=uri:https://example.org/y.png"));
    }

    #[test]
    fn unknown_properties_survive_the_round_trip() {
        let text = "BEGIN:VCARD\r\nVERSION:4.0\r\nFN:Kai Neumann\r\nN:Neumann;Kai;;;\r\n\
                    IMPP;TYPE=work:xmpp:kai@example.org\r\nX-SOCIALPROFILE;TYPE=mastodon:https://example.social/@kai\r\n\
                    END:VCARD\r\n";
        let c = &parse_vcf(text).0[0];
        assert_eq!(c.extra.len(), 2);
        let again = c.to_vcf(Version::V4);
        assert!(again.contains("IMPP;TYPE=work:xmpp:kai@example.org\r\n"));
        assert!(again.contains("X-SOCIALPROFILE;TYPE=mastodon:https://example.social/@kai\r\n"));
        assert_eq!(&parse_vcf(&again).0[0], c);
    }

    #[test]
    fn names_companies_and_mononyms() {
        let company = Contact {
            org: "Northwind GmbH".into(),
            department: "Sales".into(),
            ..Contact::default()
        };
        assert!(company.is_company());
        assert_eq!(company.display_name(), "Northwind GmbH");
        let v4 = company.to_vcf(Version::V4);
        assert!(v4.contains("KIND:org\r\n") && v4.contains("FN:Northwind GmbH\r\n") && v4.contains("ORG:Northwind GmbH;Sales\r\n"));
        assert_eq!(parse_vcf(&v4).0[0], company);
        let mono = parse_vcf("BEGIN:VCARD\nVERSION:3.0\nFN:Madonna\nN:;Madonna;;;\nEND:VCARD\n").0.remove(0);
        assert_eq!(mono.display_name(), "Madonna");
        assert_eq!(mono.formatted, "", "an FN that repeats the name is not kept apart");
        let only_fn = parse_vcf("BEGIN:VCARD\nVERSION:3.0\nFN:Dr. Who\nEND:VCARD\n").0.remove(0);
        assert_eq!(only_fn.display_name(), "Dr. Who");
        assert_eq!(robin().subtitle(), "Product lead \u{b7} Northwind");
        assert_eq!(Contact::default().display_name(), "(no name)");
    }

    #[test]
    fn the_edit_form_refuses_a_nameless_contact_and_bad_emails() {
        let mut c = Contact::default();
        assert_eq!(c.problems(None), vec!["A contact needs a name or a company."]);
        c.given = "A".into();
        c.emails = vec![Labeled::new("work", "a@example.org"), Labeled::new("home", "not-an-email"), Labeled::new("home", "x@y")];
        let p = c.problems(Some("31.02.1990"));
        assert_eq!(p.len(), 3, "{p:?}");
        assert!(p[0].contains("not-an-email") && p[1].contains("x@y") && p[2].contains("31.02.1990"));
        assert!(c.problems(Some("")).len() == 2, "an empty birthday is fine");
    }

    #[test]
    fn addresses_read_as_envelope_lines() {
        let a = &robin().addresses[0];
        assert_eq!(a.lines(), vec!["Musterweg 1", "10115 Berlin", "Germany"]);
        assert!(Address::default().is_empty());
    }
}
