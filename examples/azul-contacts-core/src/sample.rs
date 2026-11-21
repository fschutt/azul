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
    let i = i as u64;
    azul_appkit::data::uuid_from_words(0xa71c_0000_0000_0000 | i, 0x5a3b_1e00_0000_0000 ^ (i * 0x9e37_79b9))
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
    let robin = Contact {
        given: "Robin".into(),
        family: "Weber".into(),
        org: "Northwind".into(),
        title: "Product lead".into(),
        phones: vec![Labeled::new("mobile", "+49 151 0000 0001"), Labeled::new("work", "+49 30 0000 0002")],
        emails: vec![Labeled::new("work", "robin@example.org")],
        addresses: vec![Address {
            label: "home".into(),
            street: "Musterweg 1".into(),
            locality: "Berlin".into(),
            postcode: "10115".into(),
            country: "Germany".into(),
            ..Address::default()
        }],
        birthday: Some(Birthday { year: None, month: 3, day: 14 }),
        notes: "Prefers Signal.".into(),
        groups: vec!["Work".into(), "Book club".into()],
        favorite: true,
        ..Contact::default()
    };
    let person = |given: &str, family: &str| Contact {
        given: given.into(),
        family: family.into(),
        ..Contact::default()
    };
    let mut anna = person("Anna", "Berg");
    anna.org = "Northwind".into();
    anna.department = "Finance".into();
    anna.title = "Accountant".into();
    anna.emails = vec![Labeled::new("work", "anna@example.org")];
    anna.phones = vec![Labeled::new("mobile", "+49 151 0000 0004")];
    anna.groups = vec!["Work".into()];
    anna.favorite = true;
    anna.photo = "data:image/svg+xml;base64,PHN2ZyB4bWxucz0naHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmcnIHZpZXdCb3g9JzAgMCAxIDEnPjxyZWN0IHdpZHRoPScxJyBoZWlnaHQ9JzEnIGZpbGw9JyM0YTdjNTknLz48L3N2Zz4=".into();
    let mut anna2 = person("Anna", "Berg (imported)");
    anna2.emails = vec![Labeled::new("home", "anna.berg@example.net")];
    anna2.phones = vec![Labeled::new("mobile", "+49 151 0000 0014")];
    let mut jonas = person("Jonas", "Wolf");
    jonas.phones = vec![Labeled::new("mobile", "+49 160 0000 0005")];
    jonas.emails = vec![Labeled::new("home", "jonas.wolf@example.org")];
    let mut jonas2 = person("J.", "Wolf");
    jonas2.emails = vec![Labeled::new("home", "jonas.wolf@example.org"), Labeled::new("work", "jwolf@example.net")];
    jonas2.org = "Fabrikam".into();
    let mut mara = person("Mara", "Schulz");
    mara.emails = vec![Labeled::new("home", "mara@example.org")];
    mara.phones = vec![Labeled::new("mobile", "+49 170 0000 0003")];
    mara.birthday = Some(Birthday { year: Some(1990), month: 11, day: 2 });
    let mut mara2 = person("Mara", "Schulz-Lang");
    mara2.phones = vec![Labeled::new("home", "0170 00000003")];
    mara2.groups = vec!["Choir".into()];
    let mut ben = person("Ben", "Krüger");
    ben.groups = vec!["Football".into()];
    ben.favorite = true;
    let mut lukasz = person("\u{141}ukasz", "Nowak");
    lukasz.emails = vec![Labeled::new("work", "lukasz@example.org")];
    lukasz.favorite = true;
    let wang = Contact {
        formatted: "\u{738b}\u{82b3}".into(),
        emails: vec![Labeled::new("home", "wang.fang@example.org")],
        phones: vec![Labeled::new("mobile", "+81 3 0000 0006")],
        favorite: true,
        ..Contact::default()
    };
    let layla = Contact {
        formatted: "\u{644}\u{64a}\u{644}\u{649} \u{62d}\u{62f}\u{627}\u{62f}".into(),
        emails: vec![Labeled::new("home", "layla@example.net")],
        ..Contact::default()
    };
    let mononym = Contact {
        given: "Sanjay".into(),
        notes: "Goes by one name only.".into(),
        ..Contact::default()
    };
    let company = Contact {
        org: "Northwind GmbH".into(),
        department: "Reception".into(),
        phones: vec![Labeled::new("main", "+49 30 0000 0100")],
        emails: vec![Labeled::new("work", "info@example.org")],
        urls: vec![Labeled::new("work", "https://example.org")],
        groups: vec!["Suppliers".into()],
        ..Contact::default()
    };
    let mut kai = person("Kai", "Neumann");
    kai.favorite = true;
    kai.custom = vec![Labeled::new("Shoe size", "44")];
    let mut lena = person("Lena", "Hoffmann");
    lena.favorite = true;
    lena.groups = vec!["Family".into()];
    lena.birthday = Some(Birthday { year: Some(1985), month: 7, day: 21 });
    vec![robin, anna, anna2, jonas, jonas2, mara, mara2, ben, lukasz, wang, layla, mononym, company, kai, lena, person("Annika", "Roth")]
}

/// The sample address book: the special cards and generated people up to
/// [`SAMPLE_SIZE`], every one with a fixed UID.
#[must_use]
pub fn sample_book() -> Vec<Contact> {
    let mut book = special();
    let mut favorites = book.iter().filter(|c| c.favorite).count();
    // Every generated name is new, so the only duplicates are the three planned pairs.
    let mut used: Vec<(String, String)> = book.iter().map(|c| (c.given.clone(), c.family.clone())).collect();
    let mut rng = Rng(0x00a2_1c0f_fee1_5eed);
    let mut n = 100;
    while book.len() < SAMPLE_SIZE {
        let given = GIVEN[rng.below(GIVEN.len())];
        let family = FAMILY[rng.below(FAMILY.len())];
        if used.iter().any(|(g, f)| g == given && f == family) {
            continue;
        }
        used.push((given.to_string(), family.to_string()));
        n += 1;
        let mut c = Contact {
            given: given.to_string(),
            family: family.to_string(),
            ..Contact::default()
        };
        // Up to four phones, three emails, two addresses.
        let phones = rng.below(5);
        let labels = ["mobile", "work", "home", "other"];
        for k in 0..phones {
            c.phones.push(Labeled::new(labels[k], &phone(n * 4 + k, rng.below(4))));
        }
        let mail_user = crate::book::fold(&format!("{given}.{family}")).replace(' ', "");
        for k in 0..rng.below(4) {
            let domain = if k % 2 == 0 { "example.org" } else { "example.net" };
            let user = if k == 0 { mail_user.clone() } else { format!("{mail_user}{k}") };
            c.emails.push(Labeled::new(["home", "work", "other"][k], &format!("{user}@{domain}")));
        }
        for k in 0..rng.below(3) {
            let a = address(&mut rng, ["home", "work"][k], n);
            c.addresses.push(a);
        }
        if rng.chance(40) {
            c.org = COMPANIES[rng.below(COMPANIES.len())].to_string();
            c.title = TITLES[rng.below(TITLES.len())].to_string();
        }
        if rng.chance(60) {
            let year = if rng.chance(70) { Some(1950 + rng.below(55) as i32) } else { None };
            c.birthday = Some(Birthday { year, month: 1 + rng.below(12) as u32, day: 1 + rng.below(28) as u32 });
        }
        for g in GROUPS {
            if rng.chance(8) {
                c.groups.push(g.to_string());
            }
        }
        if favorites < 12 && rng.chance(5) {
            c.favorite = true;
            favorites += 1;
        }
        if rng.chance(10) {
            c.notes = "Met at a sample event.".to_string();
        }
        book.push(c);
    }
    // The plan's counts, whatever the generator drew: 12 favourites, every group used.
    for c in book.iter_mut().skip(16) {
        if favorites >= 12 {
            break;
        }
        if !c.favorite {
            c.favorite = true;
            favorites += 1;
        }
    }
    for (k, g) in GROUPS.iter().enumerate() {
        if !book.iter().any(|c| c.groups.iter().any(|x| x == g)) {
            let c = &mut book[16 + k];
            c.groups.push((*g).to_string());
        }
    }
    for (i, c) in book.iter_mut().enumerate() {
        c.uid = sample_uid(i);
    }
    book
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
