//! The reports: what the register is worth by category and location, what
//! it will lose year by year, which assets are due for a service, which
//! check-outs are overdue. Plain sums over the [`Book`]; the Reports screen
//! shows them as tables and charts.

use chrono::{Months, NaiveDate};

use crate::{depreciation, model::Asset, store::Book};

/// The assets of one category or location.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTotal {
    /// The category's / location's name; "(none)" for the assets without one.
    pub name: String,
    pub count: usize,
    pub cost: i64,
    pub book_value: i64,
}

/// The name of the group of assets without a category or location.
pub const NO_GROUP: &str = "(none)";

/// The whole register on a day.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    /// Assets on the books (not disposed of by the day).
    pub count: usize,
    pub cost: i64,
    pub book_value: i64,
    /// The depreciation of the day's year.
    pub depreciation_this_year: i64,
    /// Assets disposed of by the day.
    pub disposed: usize,
    /// Assets checked out now.
    pub checked_out: usize,
}

/// An asset due for a service.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Due {
    pub asset: String,
    pub number: String,
    pub name: String,
    pub due: NaiveDate,
    /// Due before the day asked about.
    pub overdue: bool,
}

/// An open check-out past its due date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Overdue {
    pub asset: String,
    pub number: String,
    pub name: String,
    pub custodian: String,
    pub due: NaiveDate,
    pub days_late: i64,
}

/// Whether the asset is on the books on `day` (acquired and not disposed of).
#[must_use]
pub fn on_books(asset: &Asset, day: NaiveDate) -> bool {
    asset.acquired <= day && asset.disposed.map_or(true, |d| day < d)
}

/// The register on `day`.
#[must_use]
pub fn totals(book: &Book, day: NaiveDate) -> Totals {
    let _ = (book, day, depreciation::book_value_on);
    todo!("GREEN")
}

/// The assets on the books on `day`, by category, by name.
#[must_use]
pub fn by_category(book: &Book, day: NaiveDate) -> Vec<GroupTotal> {
    let _ = (book, day);
    todo!("GREEN")
}

/// The assets on the books on `day`, by location, by name.
#[must_use]
pub fn by_location(book: &Book, day: NaiveDate) -> Vec<GroupTotal> {
    let _ = (book, day);
    todo!("GREEN")
}

/// Every asset's depreciation in each year from `from` to `to` (both
/// included): the forecast (and history) of the register.
#[must_use]
pub fn depreciation_by_year(book: &Book, from: i32, to: i32) -> Vec<(i32, i64)> {
    let _ = (book, from, to);
    todo!("GREEN")
}

/// The asset's next service: its maintenance interval after its last
/// maintenance entry (or after it was acquired); `None` without an
/// interval or once it is disposed of.
#[must_use]
pub fn next_service(book: &Book, asset: &Asset) -> Option<NaiveDate> {
    let _ = (book, asset, Months::new(0));
    todo!("GREEN")
}

/// The assets whose next service is at most `within_days` after `today`
/// (or past), the earliest first.
#[must_use]
pub fn maintenance_due(book: &Book, today: NaiveDate, within_days: i64) -> Vec<Due> {
    let _ = (book, today, within_days);
    todo!("GREEN")
}

/// The open check-outs past their due date on `today`, the latest first.
#[must_use]
pub fn overdue_checkouts(book: &Book, today: NaiveDate) -> Vec<Overdue> {
    let _ = (book, today);
    todo!("GREEN")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Category, Checkout, Location, MaintenanceEntry, MaintenanceKind, Method, Status,
    };

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn register() -> Book {
        let mut b = Book::default();
        b.put(Category {
            id: "c-it".into(),
            name: "IT equipment".into(),
            life_years: 3,
            method: Method::StraightLine,
            notes: String::new(),
        });
        b.put(Category {
            id: "c-fur".into(),
            name: "Furniture".into(),
            life_years: 13,
            method: Method::StraightLine,
            notes: String::new(),
        });
        b.put(Location {
            id: "l-hq".into(),
            name: "Head office".into(),
            address: String::new(),
            notes: String::new(),
        });
        // 1,596.64 over 3 years from January 2026.
        let mut laptop = Asset::new("a1", "A-0001", "Laptop", day(2026, 1, 15), 159_664, 3);
        laptop.category = "c-it".into();
        laptop.location = "l-hq".into();
        laptop.maintenance_months = 12;
        b.put(laptop);
        // 1,200.00 over 3 years from January 2026.
        let mut phone = Asset::new("a2", "A-0002", "Phone", day(2026, 1, 1), 120_000, 3);
        phone.category = "c-it".into();
        b.put(phone);
        // 1,300.00 over 13 years from January 2025.
        let mut desk = Asset::new("a3", "A-0003", "Desk", day(2025, 1, 1), 130_000, 13);
        desk.category = "c-fur".into();
        desk.location = "l-hq".into();
        desk.maintenance_months = 6;
        b.put(desk);
        // Sold in 2025: off the books.
        let mut van = Asset::new("a4", "A-0004", "Van", day(2020, 1, 1), 3_000_000, 6);
        van.disposed = Some(day(2025, 6, 30));
        van.status = Status::Disposed;
        van.maintenance_months = 12;
        b.put(van);
        b.put(MaintenanceEntry {
            id: "m1".into(),
            asset: "a3".into(),
            date: day(2026, 2, 10),
            kind: MaintenanceKind::Inspection,
            description: String::new(),
            cost: 0,
            by: String::new(),
        });
        b.put(Checkout {
            id: "k1".into(),
            asset: "a2".into(),
            custodian: "Grace".into(),
            out: day(2026, 8, 1),
            due: Some(day(2026, 9, 1)),
            returned: None,
            note: String::new(),
        });
        b.put(Checkout {
            id: "k2".into(),
            asset: "a1".into(),
            custodian: "Ada".into(),
            out: day(2026, 8, 1),
            due: Some(day(2026, 9, 20)),
            returned: Some(day(2026, 9, 25)),
            note: String::new(),
        });
        b
    }

    #[test]
    fn the_totals_count_only_the_assets_on_the_books() {
        let t = totals(&register(), day(2026, 12, 31));
        assert_eq!(t.count, 3);
        assert_eq!(t.cost, 159_664 + 120_000 + 130_000);
        assert_eq!(t.book_value, 106_443 + 80_000 + 110_000);
        assert_eq!(t.depreciation_this_year, 53_221 + 40_000 + 10_000);
        assert_eq!(t.disposed, 1);
        assert_eq!(t.checked_out, 1, "the open check-out");
    }

    #[test]
    fn the_register_sums_by_category_and_location_with_a_group_for_none() {
        let b = register();
        let cats = by_category(&b, day(2026, 12, 31));
        assert_eq!(
            cats,
            [
                GroupTotal {
                    name: "Furniture".into(),
                    count: 1,
                    cost: 130_000,
                    book_value: 110_000
                },
                GroupTotal {
                    name: "IT equipment".into(),
                    count: 2,
                    cost: 279_664,
                    book_value: 186_443
                },
            ]
        );
        let places = by_location(&b, day(2026, 12, 31));
        let names: Vec<&str> = places.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["(none)", "Head office"]);
        assert_eq!(places[0].count, 1);
        assert_eq!(places[1].count, 2);
    }

    #[test]
    fn the_depreciation_forecast_sums_every_assets_year() {
        let years = depreciation_by_year(&register(), 2025, 2029);
        let van_2025 =
            depreciation::depreciation_in_year(register().get::<Asset>("a4").unwrap(), 2025);
        assert_eq!(
            years,
            [
                (2025, 10_000 + van_2025),
                (2026, 53_221 + 40_000 + 10_000),
                (2027, 53_221 + 40_000 + 10_000),
                (2028, 53_222 + 40_000 + 10_000),
                (2029, 10_000),
            ]
        );
    }

    #[test]
    fn a_service_is_due_an_interval_after_the_last_one_or_the_acquisition() {
        let b = register();
        let laptop = b.get::<Asset>("a1").unwrap();
        assert_eq!(next_service(&b, laptop), Some(day(2027, 1, 15)));
        let desk = b.get::<Asset>("a3").unwrap();
        assert_eq!(
            next_service(&b, desk),
            Some(day(2026, 8, 10)),
            "six months after the inspection"
        );
        assert_eq!(
            next_service(&b, b.get::<Asset>("a2").unwrap()),
            None,
            "no interval"
        );
        assert_eq!(
            next_service(&b, b.get::<Asset>("a4").unwrap()),
            None,
            "disposed of"
        );
        let due = maintenance_due(&b, day(2026, 10, 3), 90);
        let numbers: Vec<(&str, bool)> =
            due.iter().map(|d| (d.number.as_str(), d.overdue)).collect();
        assert_eq!(numbers, [("A-0003", true)], "the laptop is due in 104 days");
        let due = maintenance_due(&b, day(2026, 10, 3), 365);
        assert_eq!(due.len(), 2);
        assert_eq!(due[1].number, "A-0001");
        assert!(!due[1].overdue);
    }

    #[test]
    fn overdue_checkouts_are_the_open_ones_past_their_due_date() {
        let b = register();
        let late = overdue_checkouts(&b, day(2026, 10, 3));
        assert_eq!(
            late,
            [Overdue {
                asset: "a2".into(),
                number: "A-0002".into(),
                name: "Phone".into(),
                custodian: "Grace".into(),
                due: day(2026, 9, 1),
                days_late: 32,
            }]
        );
        assert!(
            overdue_checkouts(&b, day(2026, 9, 1)).is_empty(),
            "due today is not late"
        );
    }
}
