//! The data tree: one JSON file per record, keyed as the user's S3 bucket
//! will be (the S3 split):
//!
//! ```text
//! erp/assets/<uuid>.json        a fixed asset
//! erp/categories/<uuid>.json    a category
//! erp/locations/<uuid>.json     a location
//! erp/maintenance/<uuid>.json   an entry of an asset's maintenance log
//! erp/checkouts/<uuid>.json     a check-out (open until checked in)
//! erp/exports/<name>.csv        the CSV exports (they go INTO the tree)
//! erp/settings.json             azul-appkit's settings file
//! ```
//!
//! The app reads every record at start (one azul-appkit `GetAll` of
//! `erp/` on a Thread, [`load`]) and keeps them in a [`Book`]; a change puts
//! the one file it changed ([`write_of`]) through the write queue. Nothing
//! here touches a disk: it is plain data, tested without a window.

use crate::model::{self, Asset, Category, Checkout, Location, MaintenanceEntry, Record};

/// The app's folder in the data tree (azul-appkit's `app_folder`).
pub const APP_FOLDER: &str = "erp";

/// The exports' folder under [`APP_FOLDER`].
pub const EXPORTS_FOLDER: &str = "exports";

/// The record kinds, by their folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Asset,
    Category,
    Location,
    Maintenance,
    Checkout,
}

impl Kind {
    pub const ALL: [Kind; 5] = [
        Kind::Asset,
        Kind::Category,
        Kind::Location,
        Kind::Maintenance,
        Kind::Checkout,
    ];

    /// Its folder under `erp/`.
    #[must_use]
    pub fn folder(self) -> &'static str {
        match self {
            Kind::Asset => Asset::FOLDER,
            Kind::Category => Category::FOLDER,
            Kind::Location => Location::FOLDER,
            Kind::Maintenance => MaintenanceEntry::FOLDER,
            Kind::Checkout => Checkout::FOLDER,
        }
    }
}

/// The listing prefix of every record: `erp/`.
#[must_use]
pub fn prefix() -> String {
    format!("{APP_FOLDER}/")
}

/// The key of a record of `folder` with id `id`: `erp/assets/<id>.json`.
#[must_use]
pub fn key_of(folder: &str, id: &str) -> String {
    format!("{APP_FOLDER}/{folder}/{id}.json")
}

/// The key of `record`.
#[must_use]
pub fn key<R: Record>(record: &R) -> String {
    key_of(R::FOLDER, record.id())
}

/// The key of an export named `name` (`assets-2026-10-03.csv`).
#[must_use]
pub fn export_key(name: &str) -> String {
    format!("{APP_FOLDER}/{EXPORTS_FOLDER}/{name}")
}

/// What `key` names: a record's kind and id, or `None` for any other file
/// (the settings, an export, a file in a folder this version does not know,
/// an id with characters an id never has).
#[must_use]
pub fn parse_key(key: &str) -> Option<(Kind, String)> {
    let _ = key;
    todo!("GREEN")
}

/// The record's write: its key and its file.
#[must_use]
pub fn write_of<R: Record>(record: &R) -> (String, Vec<u8>) {
    (key(record), model::to_json(record).into_bytes())
}

/// A file that could not be read, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub key: String,
    pub reason: String,
}

/// Every record of the data tree, in memory.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub assets: Vec<Asset>,
    pub categories: Vec<Category>,
    pub locations: Vec<Location>,
    pub maintenance: Vec<MaintenanceEntry>,
    pub checkouts: Vec<Checkout>,
}

/// A record kind the [`Book`] holds a list of.
pub trait Stored: Record {
    const KIND: Kind;
    fn list(book: &Book) -> &Vec<Self>;
    fn list_mut(book: &mut Book) -> &mut Vec<Self>;
}

impl Stored for Asset {
    const KIND: Kind = Kind::Asset;
    fn list(book: &Book) -> &Vec<Self> {
        &book.assets
    }
    fn list_mut(book: &mut Book) -> &mut Vec<Self> {
        &mut book.assets
    }
}

impl Stored for Category {
    const KIND: Kind = Kind::Category;
    fn list(book: &Book) -> &Vec<Self> {
        &book.categories
    }
    fn list_mut(book: &mut Book) -> &mut Vec<Self> {
        &mut book.categories
    }
}

impl Stored for Location {
    const KIND: Kind = Kind::Location;
    fn list(book: &Book) -> &Vec<Self> {
        &book.locations
    }
    fn list_mut(book: &mut Book) -> &mut Vec<Self> {
        &mut book.locations
    }
}

impl Stored for MaintenanceEntry {
    const KIND: Kind = Kind::Maintenance;
    fn list(book: &Book) -> &Vec<Self> {
        &book.maintenance
    }
    fn list_mut(book: &mut Book) -> &mut Vec<Self> {
        &mut book.maintenance
    }
}

impl Stored for Checkout {
    const KIND: Kind = Kind::Checkout;
    fn list(book: &Book) -> &Vec<Self> {
        &book.checkouts
    }
    fn list_mut(book: &mut Book) -> &mut Vec<Self> {
        &mut book.checkouts
    }
}

impl Book {
    /// The record of kind `R` with id `id`.
    #[must_use]
    pub fn get<R: Stored>(&self, id: &str) -> Option<&R> {
        R::list(self).iter().find(|r| r.id() == id)
    }

    /// The record of kind `R` with id `id`, to change.
    pub fn get_mut<R: Stored>(&mut self, id: &str) -> Option<&mut R> {
        R::list_mut(self).iter_mut().find(|r| r.id() == id)
    }

    /// Adds `record`, or replaces the one with its id.
    pub fn put<R: Stored>(&mut self, record: R) {
        let list = R::list_mut(self);
        match list.iter_mut().find(|r| r.id() == record.id()) {
            Some(slot) => *slot = record,
            None => list.push(record),
        }
    }

    /// Removes the record of kind `R` with id `id`.
    pub fn remove<R: Stored>(&mut self, id: &str) -> Option<R> {
        let list = R::list_mut(self);
        let at = list.iter().position(|r| r.id() == id)?;
        Some(list.remove(at))
    }

    /// Removes an asset with its maintenance log and its check-outs; the
    /// keys of every file to delete.
    pub fn remove_asset(&mut self, id: &str) -> Vec<String> {
        let _ = id;
        todo!("GREEN")
    }

    /// A category's name ("" for none or an unknown id).
    #[must_use]
    pub fn category_name(&self, id: &str) -> &str {
        self.get::<Category>(id).map_or("", |c| c.name.as_str())
    }

    /// A location's name ("" for none or an unknown id).
    #[must_use]
    pub fn location_name(&self, id: &str) -> &str {
        self.get::<Location>(id).map_or("", |l| l.name.as_str())
    }

    /// The category named `name` (any case, spaces trimmed).
    #[must_use]
    pub fn category_by_name(&self, name: &str) -> Option<&Category> {
        let name = name.trim();
        self.categories
            .iter()
            .find(|c| c.name.trim().eq_ignore_ascii_case(name))
    }

    /// The location named `name` (any case, spaces trimmed).
    #[must_use]
    pub fn location_by_name(&self, name: &str) -> Option<&Location> {
        let name = name.trim();
        self.locations
            .iter()
            .find(|l| l.name.trim().eq_ignore_ascii_case(name))
    }

    /// The asset with the asset number `number` (any case, spaces trimmed).
    #[must_use]
    pub fn asset_by_number(&self, number: &str) -> Option<&Asset> {
        let number = number.trim();
        self.assets
            .iter()
            .find(|a| a.number.trim().eq_ignore_ascii_case(number))
    }

    /// The asset's maintenance log, the newest entry first.
    #[must_use]
    pub fn maintenance_of(&self, asset: &str) -> Vec<&MaintenanceEntry> {
        let _ = asset;
        todo!("GREEN")
    }

    /// The asset's check-outs, the newest first.
    #[must_use]
    pub fn checkouts_of(&self, asset: &str) -> Vec<&Checkout> {
        let _ = asset;
        todo!("GREEN")
    }

    /// The asset's open check-out.
    #[must_use]
    pub fn open_checkout(&self, asset: &str) -> Option<&Checkout> {
        let _ = asset;
        todo!("GREEN")
    }

    /// The next free asset number: `A-` and one more than the highest
    /// `A-<digits>` there is, four digits at least (`A-0043`).
    #[must_use]
    pub fn next_number(&self) -> String {
        todo!("GREEN")
    }

    /// Sorts every list the way the screens show it: assets by number,
    /// categories and locations by name, the logs by day.
    pub fn sort(&mut self) {
        todo!("GREEN")
    }
}

/// The records of the files a `GetAll` of [`prefix`] read (`(key, bytes)`),
/// and the files that could not be read. Other files (the settings, the
/// exports) are not records and are passed over.
#[must_use]
pub fn load(files: &[(String, Vec<u8>)]) -> (Book, Vec<Skipped>) {
    let _ = files;
    todo!("GREEN")
}

/// Reads one record file of kind `R` whose file name says `id`.
fn read_one<R: Stored>(book: &mut Book, id: &str, text: &str) -> Result<(), String> {
    match model::from_json::<R>(text) {
        Ok(record) if record.id() == id => {
            book.put(record);
            Ok(())
        }
        Ok(_) => Err("its id is not its file's name".to_string()),
        Err(e) => Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use super::*;
    use crate::model::MaintenanceKind;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    const A1: &str = "0b6f6a3e-1d2c-4b5a-8e9f-000000000001";
    const A2: &str = "0b6f6a3e-1d2c-4b5a-8e9f-000000000002";

    fn book() -> Book {
        let mut b = Book::default();
        b.put(Asset::new(
            A1,
            "A-0042",
            "ThinkPad",
            day(2026, 1, 15),
            159_664,
            3,
        ));
        b.put(Asset::new(
            A2,
            "A-0007",
            "Desk",
            day(2024, 5, 2),
            45_000,
            13,
        ));
        b.put(Category {
            id: "c1".into(),
            name: "IT equipment".into(),
            life_years: 3,
            method: model::Method::StraightLine,
            notes: String::new(),
        });
        b.put(Location {
            id: "l1".into(),
            name: "Head office".into(),
            address: String::new(),
            notes: String::new(),
        });
        for (id, d) in [
            ("m1", day(2026, 3, 1)),
            ("m2", day(2026, 9, 1)),
            ("m3", day(2026, 6, 1)),
        ] {
            b.put(MaintenanceEntry {
                id: id.into(),
                asset: A1.into(),
                date: d,
                kind: MaintenanceKind::Service,
                description: String::new(),
                cost: 1000,
                by: String::new(),
            });
        }
        b.put(Checkout {
            id: "k1".into(),
            asset: A1.into(),
            custodian: "Ada".into(),
            out: day(2026, 2, 1),
            due: None,
            returned: Some(day(2026, 2, 10)),
            note: String::new(),
        });
        b.put(Checkout {
            id: "k2".into(),
            asset: A1.into(),
            custodian: "Grace".into(),
            out: day(2026, 9, 1),
            due: Some(day(2026, 9, 30)),
            returned: None,
            note: String::new(),
        });
        b
    }

    #[test]
    fn a_records_key_names_its_folder_and_id_and_parses_back() {
        let b = book();
        let a = b.get::<Asset>(A1).unwrap();
        assert_eq!(key(a), format!("erp/assets/{A1}.json"));
        assert_eq!(parse_key(&key(a)), Some((Kind::Asset, A1.to_string())));
        for kind in Kind::ALL {
            let k = key_of(kind.folder(), "x-1");
            assert_eq!(parse_key(&k), Some((kind, "x-1".to_string())), "{k}");
        }
        assert_eq!(export_key("assets.csv"), "erp/exports/assets.csv");
    }

    #[test]
    fn other_files_of_the_tree_are_not_records() {
        for other in [
            "erp/settings.json",
            "erp/exports/assets.csv",
            "erp/exports/x.json",
            "erp/assets/x.txt",
            "erp/assets/sub/x.json",
            "erp/assets/.json",
            "erp/assets/a b.json",
            "erp/assets/../x.json",
            "erp/invoices/x.json",
            "notes/assets/x.json",
            "erp/assets/",
        ] {
            assert_eq!(parse_key(other), None, "{other}");
        }
    }

    #[test]
    fn loading_reads_every_kind_and_names_the_files_it_could_not_read() {
        let b = book();
        let mut files: Vec<(String, Vec<u8>)> = Vec::new();
        for a in &b.assets {
            files.push(write_of(a));
        }
        files.push(write_of(&b.categories[0]));
        files.push(write_of(&b.locations[0]));
        for m in &b.maintenance {
            files.push(write_of(m));
        }
        for k in &b.checkouts {
            files.push(write_of(k));
        }
        files.push(("erp/settings.json".into(), b"{\"theme\":\"flat\"}".to_vec()));
        files.push(("erp/exports/assets.csv".into(), b"a,b\n".to_vec()));
        files.push(("erp/assets/broken.json".into(), b"{ nope".to_vec()));
        let (_, misnamed) = write_of(&b.assets[0]);
        files.push(("erp/assets/someone-else.json".into(), misnamed));
        let newer = String::from_utf8(write_of(&b.assets[1]).1)
            .unwrap()
            .replace("\"version\": 1", "\"version\": 9");
        files.push((format!("erp/assets/{A2}.json"), newer.into_bytes()));

        let (mut loaded, skipped) = load(&files);
        let mut expected = b.clone();
        expected.sort();
        loaded.sort();
        // The newer file of A2 was read after the good one: A2 stays as it was.
        assert_eq!(loaded, expected);
        let keys: Vec<&str> = skipped.iter().map(|s| s.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "erp/assets/broken.json",
                "erp/assets/someone-else.json",
                &format!("erp/assets/{A2}.json")[..]
            ]
        );
        assert!(skipped[1].reason.contains("id"), "{:?}", skipped[1]);
        assert!(skipped[2].reason.contains("newer"), "{:?}", skipped[2]);
    }

    #[test]
    fn an_assets_logs_are_newest_first_and_its_open_checkout_is_found() {
        let b = book();
        let days: Vec<NaiveDate> = b.maintenance_of(A1).iter().map(|m| m.date).collect();
        assert_eq!(days, [day(2026, 9, 1), day(2026, 6, 1), day(2026, 3, 1)]);
        assert!(b.maintenance_of(A2).is_empty());
        let outs: Vec<&str> = b.checkouts_of(A1).iter().map(|k| k.id.as_str()).collect();
        assert_eq!(outs, ["k2", "k1"]);
        assert_eq!(
            b.open_checkout(A1).map(|k| k.custodian.as_str()),
            Some("Grace")
        );
        assert_eq!(b.open_checkout(A2), None);
    }

    #[test]
    fn the_next_asset_number_follows_the_highest() {
        assert_eq!(Book::default().next_number(), "A-0001");
        let mut b = book();
        assert_eq!(b.next_number(), "A-0043");
        let mut big = Asset::new("x", "a-12345", "Crane", day(2020, 1, 1), 1, 1);
        b.put(big.clone());
        assert_eq!(b.next_number(), "A-12346");
        big.number = "B-99999".into();
        b.put(big);
        assert_eq!(b.next_number(), "A-0043", "other prefixes do not count");
    }

    #[test]
    fn removing_an_asset_removes_its_logs_and_names_every_file() {
        let mut b = book();
        let mut keys = b.remove_asset(A1);
        keys.sort();
        let mut expected = vec![
            format!("erp/assets/{A1}.json"),
            "erp/maintenance/m1.json".to_string(),
            "erp/maintenance/m2.json".to_string(),
            "erp/maintenance/m3.json".to_string(),
            "erp/checkouts/k1.json".to_string(),
            "erp/checkouts/k2.json".to_string(),
        ];
        expected.sort();
        assert_eq!(keys, expected);
        assert!(b.get::<Asset>(A1).is_none());
        assert!(b.maintenance.is_empty());
        assert!(b.checkouts.is_empty());
        assert_eq!(b.assets.len(), 1);
        assert!(b.remove_asset("nobody").is_empty());
    }

    #[test]
    fn names_resolve_by_id_and_records_by_name_in_any_case() {
        let b = book();
        assert_eq!(b.category_name("c1"), "IT equipment");
        assert_eq!(b.category_name(""), "");
        assert_eq!(b.location_name("l1"), "Head office");
        assert_eq!(
            b.category_by_name(" it EQUIPMENT ").map(|c| c.id.as_str()),
            Some("c1")
        );
        assert_eq!(
            b.location_by_name("head office").map(|l| l.id.as_str()),
            Some("l1")
        );
        assert_eq!(b.asset_by_number("a-0007").map(|a| a.id.as_str()), Some(A2));
        assert!(b.asset_by_number("A-9").is_none());
    }
}
