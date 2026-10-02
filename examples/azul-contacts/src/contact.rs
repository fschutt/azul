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
        let mut out = Vec::new();
        for part in [&self.po_box, &self.extended, &self.street] {
            if !part.trim().is_empty() {
                out.push(part.trim().to_string());
            }
        }
        let city = [self.postcode.trim(), self.locality.trim()]
            .iter()
            .filter(|s| !s.is_empty())
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        if !city.is_empty() {
            out.push(city);
        }
        for part in [&self.region, &self.country] {
            if !part.trim().is_empty() {
                out.push(part.trim().to_string());
            }
        }
        out
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
        let t = text.trim();
        let t = t.split(['T', 't']).next().unwrap_or(t);
        let valid = |year: Option<i32>, month: u32, day: u32| {
            let max = match month {
                1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
                4 | 6 | 9 | 11 => 30,
                2 => 29,
                _ => 0,
            };
            (day >= 1 && day <= max && year.is_none_or_valid()).then_some(Birthday { year, month, day })
        };
        if t.contains('.') {
            let parts: Vec<&str> = t.split('.').map(str::trim).collect();
            let day = parts.first()?.parse().ok()?;
            let month = parts.get(1)?.parse().ok()?;
            let year = match parts.get(2).filter(|y| !y.is_empty()) {
                Some(y) => Some(y.parse().ok()?),
                None => None,
            };
            return valid(year, month, day);
        }
        if let Some(rest) = t.strip_prefix("--") {
            let digits: String = rest.chars().filter(char::is_ascii_digit).collect();
            if digits.len() != 4 {
                return None;
            }
            return valid(None, digits[..2].parse().ok()?, digits[2..].parse().ok()?);
        }
        let digits: String = t.chars().filter(char::is_ascii_digit).collect();
        if digits.len() != 8 {
            return None;
        }
        valid(
            Some(digits[..4].parse().ok()?),
            digits[4..6].parse().ok()?,
            digits[6..].parse().ok()?,
        )
    }

    /// As vCard writes it: 4.0 `19870314` / `--0314`, 3.0 `1987-03-14` / `--03-14`.
    #[must_use]
    pub fn to_vcard(&self, version: Version) -> String {
        match (version, self.year) {
            (Version::V4, Some(y)) => format!("{y:04}{:02}{:02}", self.month, self.day),
            (Version::V4, None) => format!("--{:02}{:02}", self.month, self.day),
            (Version::V3, Some(y)) => format!("{y:04}-{:02}-{:02}", self.month, self.day),
            (Version::V3, None) => format!("--{:02}-{:02}", self.month, self.day),
        }
    }

    /// `14 March 1987`, `14 March`.
    #[must_use]
    pub fn describe(&self) -> String {
        let month = MONTHS[(self.month.clamp(1, 12) - 1) as usize];
        match self.year {
            Some(y) => format!("{} {month} {y}", self.day),
            None => format!("{} {month}", self.day),
        }
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
    const GENERIC: [&str; 8] = ["voice", "pref", "internet", "x400", "msg", "text", "uri", "postal"];
    for t in types {
        let t = t.as_str();
        if GENERIC.contains(&t) || t.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        return Some(match t {
            "cell" | "mobile" | "iphone" => "mobile".to_string(),
            other => other.strip_prefix("x-").unwrap_or(other).replace('-', " "),
        });
    }
    None
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
    let l = label.trim().to_lowercase();
    let t = match l.as_str() {
        "mobile" | "cell" => "cell".to_string(),
        "work" | "home" | "fax" | "pager" => l.clone(),
        "main" if version == Version::V3 && phone => "x-main".to_string(),
        "main" => "main".to_string(),
        "" | "other" => "other".to_string(),
        other => format!("x-{}", other.replace(' ', "-")),
    };
    if version == Version::V3 {
        t.to_uppercase()
    } else {
        t
    }
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
        let composed = self.composed_name();
        if !composed.is_empty() {
            return composed;
        }
        for candidate in [&self.formatted, &self.org] {
            if let Some(s) = nonempty(candidate) {
                return s.to_string();
            }
        }
        if let Some(e) = self.emails.first() {
            return e.value.clone();
        }
        if let Some(p) = self.phones.first() {
            return p.value.clone();
        }
        "(no name)".to_string()
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

    /// What the edit form refuses: no name at all, an email that is not one
    /// (`azul_pim::mail_address::is_email`), a birthday that is not a date.
    #[must_use]
    pub fn problems(&self, birthday_text: Option<&str>) -> Vec<String> {
        let mut out = Vec::new();
        if self.composed_name().is_empty() && nonempty(&self.formatted).is_none() && nonempty(&self.org).is_none() {
            out.push("A contact needs a name or a company.".to_string());
        }
        for e in &self.emails {
            let v = e.value.trim();
            if v.is_empty() {
                continue;
            }
            if !azul_pim::mail_address::is_email(v) {
                out.push(format!("\"{v}\" is not an email address."));
            }
        }
        if let Some(text) = birthday_text {
            if !text.trim().is_empty() && Birthday::parse(text).is_none() {
                out.push(format!("\"{}\" is not a date (DD.MM.YYYY, or DD.MM. without a year).", text.trim()));
            }
        }
        out
    }

    /// Reads one card.
    #[must_use]
    pub fn from_card(card: &Card) -> Contact {
        let mut c = Contact::default();
        // Apple's grouped labels: item1.X-ABLabel names item1.TEL.
        let ab_labels: Vec<(String, String)> = card
            .properties
            .iter()
            .filter(|p| p.name == "X-ABLABEL")
            .filter_map(|p| p.group.clone().map(|g| (g, clean_ab_label(&p.text()))))
            .collect();
        let label_for = |p: &Property| -> String {
            if let Some(g) = &p.group {
                if let Some((_, l)) = ab_labels.iter().find(|(lg, _)| lg == g) {
                    return if l == "cell" || l == "iphone" { "mobile".to_string() } else { l.clone() };
                }
            }
            label_of_types(&p.types()).unwrap_or_else(|| "other".to_string())
        };
        let mut used_groups: Vec<String> = Vec::new();
        for p in &card.properties {
            let mut used = true;
            match p.name.as_str() {
                "FN" => c.formatted = p.text(),
                "N" => {
                    let parts = p.components();
                    let part = |i: usize| parts.get(i).cloned().unwrap_or_default();
                    c.family = part(0);
                    c.given = part(1);
                    c.additional = part(2);
                    c.prefix = part(3);
                    c.suffix = part(4);
                }
                "NICKNAME" => c.nickname = p.list().join(", "),
                "ORG" => {
                    let parts = p.components();
                    c.org = parts.first().cloned().unwrap_or_default();
                    c.department = parts.get(1..).map(|r| r.join(", ")).unwrap_or_default();
                }
                "TITLE" => c.title = p.text(),
                "TEL" => {
                    let raw = p.text();
                    let value = raw.strip_prefix("tel:").unwrap_or(&raw).to_string();
                    c.phones.push(Labeled { label: label_for(p), value });
                }
                "EMAIL" => {
                    let raw = p.text();
                    let value = raw.strip_prefix("mailto:").unwrap_or(&raw).to_string();
                    c.emails.push(Labeled { label: label_for(p), value });
                }
                "ADR" => {
                    let parts = p.components();
                    let part = |i: usize| parts.get(i).cloned().unwrap_or_default();
                    c.addresses.push(Address {
                        label: label_for(p),
                        po_box: part(0),
                        extended: part(1),
                        street: part(2),
                        locality: part(3),
                        region: part(4),
                        postcode: part(5),
                        country: part(6),
                    });
                }
                "URL" => c.urls.push(Labeled {
                    label: label_for(p),
                    value: p.text(),
                }),
                "BDAY" => {
                    c.birthday = Birthday::parse(&p.value).map(|mut b| {
                        // Apple stores a birthday without a year as 1604.
                        if p.param("X-APPLE-OMIT-YEAR").is_some() {
                            b.year = None;
                        }
                        b
                    });
                    if c.birthday.is_none() {
                        used = false;
                    }
                }
                "NOTE" => {
                    if !c.notes.is_empty() {
                        c.notes.push('\n');
                    }
                    c.notes.push_str(&p.text());
                }
                "PHOTO" => {
                    let encoded = p
                        .param("ENCODING")
                        .is_some_and(|v| v.iter().any(|e| e.eq_ignore_ascii_case("b") || e.eq_ignore_ascii_case("base64")));
                    c.photo = if encoded {
                        let subtype = p
                            .types()
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "jpeg".to_string())
                            .trim_start_matches("image/")
                            .to_string();
                        format!("data:image/{subtype};base64,{}", p.value.trim())
                    } else {
                        p.value.trim().to_string()
                    };
                }
                "CATEGORIES" => {
                    for g in p.list() {
                        if !c.groups.contains(&g) {
                            c.groups.push(g);
                        }
                    }
                }
                "UID" => {
                    let v = p.text();
                    c.uid = v.strip_prefix("urn:uuid:").unwrap_or(&v).to_string();
                }
                "X-AZLIN-FAVORITE" => c.favorite = p.text().trim().eq_ignore_ascii_case("true"),
                "X-AZLIN-FIELD" => c.custom.push(Labeled {
                    label: p.param("X-LABEL").and_then(|v| v.first().cloned()).unwrap_or_default(),
                    value: p.text(),
                }),
                // Written fresh every time.
                "PRODID" | "REV" | "KIND" => {}
                "X-ABLABEL" => used = false,
                _ => used = false,
            }
            if used {
                if let Some(g) = &p.group {
                    used_groups.push(g.clone());
                }
            }
        }
        // Everything else is kept, except the labels that were applied.
        for p in &card.properties {
            let known = matches!(
                p.name.as_str(),
                "FN" | "N" | "NICKNAME" | "ORG" | "TITLE" | "TEL" | "EMAIL" | "ADR" | "URL" | "NOTE" | "PHOTO"
                    | "CATEGORIES" | "UID" | "X-AZLIN-FAVORITE" | "X-AZLIN-FIELD" | "PRODID" | "REV" | "KIND"
            ) || (p.name == "BDAY" && c.birthday.is_some());
            let applied_label = p.name == "X-ABLABEL"
                && p.group.as_ref().is_some_and(|g| used_groups.contains(g));
            if !known && !applied_label {
                c.extra.push(p.clone());
            }
        }
        // An FN that only repeats what the name parts (or the company) say is not kept apart.
        let fn_text = std::mem::take(&mut c.formatted);
        if c.display_name() != fn_text.trim() {
            c.formatted = fn_text;
        }
        c
    }

    /// The card of this contact in `version`.
    #[must_use]
    pub fn to_card(&self, version: Version) -> Card {
        let mut props = Vec::new();
        let fn_text = if nonempty(&self.formatted).is_some() && self.composed_name().is_empty() {
            self.formatted.trim().to_string()
        } else {
            self.display_name()
        };
        if version == Version::V4 && self.is_company() {
            props.push(Property::new("KIND", "org"));
        }
        props.push(Property::text_value("FN", &fn_text));
        props.push(Property::new(
            "N",
            &[&self.family, &self.given, &self.additional, &self.prefix, &self.suffix]
                .iter()
                .map(|s| escape_text(s))
                .collect::<Vec<_>>()
                .join(";"),
        ));
        if let Some(n) = nonempty(&self.nickname) {
            props.push(Property::text_value("NICKNAME", n));
        }
        if nonempty(&self.org).is_some() || nonempty(&self.department).is_some() {
            let mut value = escape_text(self.org.trim());
            if let Some(d) = nonempty(&self.department) {
                value.push(';');
                value.push_str(&escape_text(d));
            }
            props.push(Property::new("ORG", &value));
        }
        if let Some(t) = nonempty(&self.title) {
            props.push(Property::text_value("TITLE", t));
        }
        for p in self.phones.iter().filter(|p| nonempty(&p.value).is_some()) {
            let t = type_of_label(&p.label, version, true);
            let types: Vec<&str> = if version == Version::V3 { vec![t.as_str(), "VOICE"] } else { vec![t.as_str()] };
            let types: Vec<&str> = if t.eq_ignore_ascii_case("fax") || t.eq_ignore_ascii_case("pager") {
                vec![t.as_str()]
            } else {
                types
            };
            props.push(Property::text_value("TEL", p.value.trim()).with_param("TYPE", &types));
        }
        for e in self.emails.iter().filter(|e| nonempty(&e.value).is_some()) {
            let t = type_of_label(&e.label, version, false);
            let types: Vec<&str> = if version == Version::V3 { vec!["INTERNET", t.as_str()] } else { vec![t.as_str()] };
            props.push(Property::text_value("EMAIL", e.value.trim()).with_param("TYPE", &types));
        }
        for a in self.addresses.iter().filter(|a| !a.is_empty()) {
            let value = [&a.po_box, &a.extended, &a.street, &a.locality, &a.region, &a.postcode, &a.country]
                .iter()
                .map(|s| escape_text(s.trim()))
                .collect::<Vec<_>>()
                .join(";");
            let t = type_of_label(&a.label, version, false);
            props.push(Property::new("ADR", &value).with_param("TYPE", &[t.as_str()]));
        }
        for u in self.urls.iter().filter(|u| nonempty(&u.value).is_some()) {
            let t = type_of_label(&u.label, version, false);
            props.push(Property::text_value("URL", u.value.trim()).with_param("TYPE", &[t.as_str()]));
        }
        if let Some(b) = &self.birthday {
            props.push(Property::new("BDAY", &b.to_vcard(version)));
        }
        if let Some(n) = nonempty(&self.notes) {
            props.push(Property::text_value("NOTE", n));
        }
        if let Some(photo) = nonempty(&self.photo) {
            props.push(photo_property(photo, version));
        }
        if !self.groups.is_empty() {
            let value = self.groups.iter().map(|g| escape_text(g)).collect::<Vec<_>>().join(",");
            props.push(Property::new("CATEGORIES", &value));
        }
        if self.favorite {
            props.push(Property::new("X-AZLIN-FAVORITE", "true"));
        }
        for f in self.custom.iter().filter(|f| nonempty(&f.value).is_some()) {
            props.push(Property::text_value("X-AZLIN-FIELD", f.value.trim()).with_param("X-LABEL", &[f.label.trim()]));
        }
        props.extend(self.extra.iter().cloned());
        if !self.uid.is_empty() {
            let uid = if version == Version::V4 && azul_appkit::data::is_uuid(&self.uid) {
                format!("urn:uuid:{}", self.uid)
            } else {
                self.uid.clone()
            };
            props.push(Property::text_value("UID", &uid));
        }
        props.push(Property::text_value("PRODID", "-//Azlin//AzContacts//EN"));
        Card {
            version,
            properties: props,
        }
    }

    /// The contact as `.vcf` text.
    #[must_use]
    pub fn to_vcf(&self, version: Version) -> String {
        vcard::write(&self.to_card(version))
    }
}

/// PHOTO: 4.0 a URI (`data:` or a link); 3.0 inline base64 with ENCODING=b, or VALUE=uri.
fn photo_property(photo: &str, version: Version) -> Property {
    if version == Version::V4 {
        return Property::new("PHOTO", photo);
    }
    if let Some(rest) = photo.strip_prefix("data:") {
        if let Some((meta, data)) = rest.split_once(',') {
            if meta.ends_with(";base64") {
                let subtype = meta
                    .trim_end_matches(";base64")
                    .trim_start_matches("image/")
                    .to_uppercase();
                return Property::new("PHOTO", data).with_param("ENCODING", &["b"]).with_param("TYPE", &[subtype.as_str()]);
            }
        }
    }
    Property::new("PHOTO", photo).with_param("VALUE", &["uri"])
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
    fn an_email_with_a_second_at_sign_or_brackets_is_refused() {
        // DEDUP_EDITORS B14: the form split at the first `@`, so `a@b@example.org` passed.
        let mut c = Contact::default();
        c.given = "A".into();
        c.emails = vec![
            Labeled::new("work", "a@b@example.org"),
            Labeled::new("home", "<a@example.org>"),
            Labeled::new("other", "a.b+c@mail.example.org"),
        ];
        let p = c.problems(None);
        assert_eq!(p.len(), 2, "{p:?}");
        assert!(p[0].contains("a@b@example.org") && p[1].contains("<a@example.org>"), "{p:?}");
    }

    #[test]
    fn addresses_read_as_envelope_lines() {
        let a = &robin().addresses[0];
        assert_eq!(a.lines(), vec!["Musterweg 1", "10115 Berlin", "Germany"]);
        assert!(Address::default().is_empty());
    }
}
