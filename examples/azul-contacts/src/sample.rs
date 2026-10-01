//! The sample address book (`--sample` on an empty data folder), as the
//! plan describes it: 300 contacts - names with diacritics (Krüger,
//! Łukasz), a Chinese and an Arabic name, a single name, a company-only
//! card; up to four phones, three emails and two addresses (German, US,
//! Japanese formats); birthdays with and without a year; 12 favourites;
//! 8 groups; three pairs of duplicates with differing fields. All emails
//! are @example.org / @example.net, all phone numbers have `0000` blocks:
//! nobody real. Deterministic: the same 300 every time (fixed UIDs).

use crate::contact::{Address, Birthday, Contact, Labeled};

/// How many contacts the sample has.
pub const SAMPLE_SIZE: usize = 300;

/// The sample's groups.
pub const GROUPS: [&str; 8] = [
    "Family", "Work", "Book club", "Football", "Neighbours", "School", "Choir", "Suppliers",
];

const GIVEN: [&str; 40] = [
    "Anna", "Ben", "Clara", "David", "Elif", "Finn", "Greta", "Hannes", "Ida", "Jonas", "Katrin", "Lena",
    "Moritz", "Nora", "Oskar", "Paula", "Quentin", "Rosa", "Sven", "Tara", "Ulrich", "Vera", "Wim", "Xenia",
    "Yusuf", "Zoe", "Amelie", "Bruno", "Carla", "Dario", "Emil", "Frieda", "Gustav", "Helena", "Ilias", "Jule",
    "Karl", "Luisa", "Milan", "Nele",
];

const FAMILY: [&str; 40] = [
    "Becker", "Fischer", "Hoffmann", "Keller", "Lange", "Meyer", "Neumann", "Peters", "Richter", "Schmidt",
    "Schneider", "Wagner", "Weber", "Wolf", "Zimmermann", "Braun", "Hartmann", "Krause", "Lehmann", "Möller",
    "Schäfer", "Schulz", "Vogel", "Walter", "Yilmaz", "Novak", "Rossi", "García", "Dubois", "Jansen",
    "Andersson", "Kowalski", "Horvath", "Costa", "Murphy", "Okafor", "Tanaka", "Haddad", "Silva", "Brandt",
];

const COMPANIES: [&str; 8] = [
    "Northwind", "Contoso", "Fabrikam", "Tailspin", "Wide World Importers", "Adventure Works", "Litware",
    "Proseware",
];

const TITLES: [&str; 8] = [
    "Product lead", "Engineer", "Accountant", "Designer", "Teacher", "Nurse", "Sales", "Architect",
];

const CITIES_DE: [(&str, &str); 4] = [("Berlin", "10115"), ("Hamburg", "20095"), ("München", "80331"), ("Köln", "50667")];

/// A small deterministic generator (a 64-bit LCG).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
}

/// The fixed UID of sample contact `i`.
#[must_use]
pub fn sample_uid(i: usize) -> String {
        todo!("RED: sample_uid")
    }

fn phone(n: usize, kind: usize) -> String {
    match kind {
        0 => format!("+49 151 0000 {:04}", n % 10_000),
        1 => format!("+49 30 0000 {:04}", n % 10_000),
        2 => format!("+1 555 0000 {:04}", n % 10_000),
        _ => format!("+81 3 0000 {:04}", n % 10_000),
    }
}

fn address(rng: &mut Rng, label: &str, n: usize) -> Address {
    match rng.below(3) {
        0 => {
            let (city, code) = CITIES_DE[rng.below(CITIES_DE.len())];
            Address {
                label: label.to_string(),
                street: format!("Musterweg {}", n % 90 + 1),
                locality: city.to_string(),
                postcode: code.to_string(),
                country: "Germany".to_string(),
                ..Address::default()
            }
        }
        1 => Address {
            label: label.to_string(),
            street: format!("{} Example Street", 100 + n % 800),
            locality: "Springfield".to_string(),
            region: "IL".to_string(),
            postcode: "62701".to_string(),
            country: "USA".to_string(),
            ..Address::default()
        },
        _ => Address {
            label: label.to_string(),
            street: format!("1-{}-{} Chiyoda", n % 9 + 1, n % 30 + 1),
            locality: "Tokyo".to_string(),
            postcode: "100-0001".to_string(),
            country: "Japan".to_string(),
            ..Address::default()
        },
    }
}

/// The hand-written cards the plan names (Robin Weber's card, the special
/// names, the duplicates).
fn special() -> Vec<Contact> {
        todo!("RED: special")
    }

/// The sample address book: the special cards and generated people up to
/// [`SAMPLE_SIZE`], every one with a fixed UID.
#[must_use]
pub fn sample_book() -> Vec<Contact> {
        todo!("RED: sample_book")
    }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dupes::{find_duplicates, THRESHOLD};

    #[test]
    fn three_hundred_contacts_with_fixed_unique_uids() {
        let book = sample_book();
        assert_eq!(book.len(), SAMPLE_SIZE);
        assert_eq!(book, sample_book(), "deterministic");
        let mut uids: Vec<&str> = book.iter().map(|c| c.uid.as_str()).collect();
        assert!(uids.iter().all(|u| azul_appkit::data::is_uuid(u)));
        uids.sort_unstable();
        uids.dedup();
        assert_eq!(uids.len(), SAMPLE_SIZE);
    }

    #[test]
    fn the_special_cards_of_the_plan_are_there() {
        let book = sample_book();
        let names: Vec<String> = book.iter().map(Contact::display_name).collect();
        for want in ["Robin Weber", "Anna Berg", "Ben Krüger", "\u{141}ukasz Nowak", "\u{738b}\u{82b3}", "Sanjay", "Northwind GmbH"] {
            assert!(names.iter().any(|n| n == want), "{want} is missing");
        }
        assert!(book.iter().any(|c| c.is_company()));
        assert!(book.iter().any(|c| c.birthday.is_some_and(|b| b.year.is_none())));
        assert!(book.iter().any(|c| c.birthday.is_some_and(|b| b.year.is_some())));
        assert!(book.iter().any(|c| c.phones.len() == 4));
        assert!(book.iter().any(|c| c.addresses.len() == 2));
        assert!(book.iter().all(|c| c.emails.iter().all(|e| e.value.ends_with("@example.org") || e.value.ends_with("@example.net"))));
        assert!(book.iter().all(|c| c.phones.iter().all(|p| p.value.contains("0000"))));
    }

    #[test]
    fn twelve_favourites_eight_groups_and_three_duplicate_pairs() {
        let book = sample_book();
        assert_eq!(book.iter().filter(|c| c.favorite).count(), 12);
        let groups = crate::book::group_counts(&book);
        assert_eq!(groups.len(), 8, "{groups:?}");
        let pairs = find_duplicates(&book[..16], THRESHOLD, &[]);
        assert_eq!(pairs.len(), 3, "{pairs:?}");
    }

    #[test]
    fn every_sample_card_survives_its_file() {
        let book = sample_book();
        let files: Vec<(String, Vec<u8>)> = book.iter().map(crate::store::file_of).collect();
        let (back, problems) = crate::store::load(&files);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(back, book);
    }
}
