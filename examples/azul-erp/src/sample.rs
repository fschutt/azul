//! The sample register `--sample` writes into an empty data tree (the ERP
//! README: "a seed script with realistic data for one company ... 10
//! assets"): Mustermann GmbH's categories, locations, assets acquired over
//! the last years (both methods, a low-value asset, a disposed van), a
//! maintenance log and check-outs, one of them overdue. Every day is
//! relative to `today`, so the sample looks the same whenever it is made.

use chrono::{Datelike, NaiveDate, TimeDelta};

use crate::{
    model::{
        Asset, Category, Checkout, Location, MaintenanceEntry, MaintenanceKind, Method, Status,
    },
    store::Book,
};

/// `(name, life years, method)` of the sample's categories.
const CATEGORIES: [(&str, u32, Method); 5] = [
    ("IT equipment", 3, Method::StraightLine),
    ("Office furniture", 13, Method::StraightLine),
    ("Vehicles", 6, Method::DecliningBalance),
    ("Machinery", 10, Method::DecliningBalance),
    ("Low-value assets", 0, Method::StraightLine),
];

/// `(name, address)` of the sample's locations.
const LOCATIONS: [(&str, &str); 3] = [
    ("Head office, Munich", "Leopoldstr. 1, 80802 Munich"),
    ("Warehouse, Augsburg", "Industriestr. 12, 86167 Augsburg"),
    ("Branch office, Berlin", "Friedrichstr. 100, 10117 Berlin"),
];

/// One sample asset: number, name, category and location (indices), years
/// before today's year, month, day, cost in cents, life, method, the
/// declining rate (bp), serial, service interval (months).
struct Seed {
    number: &'static str,
    name: &'static str,
    category: usize,
    location: usize,
    years_ago: i32,
    month: u32,
    day: u32,
    cost: i64,
    life: u32,
    method: Method,
    rate_bp: u32,
    serial: &'static str,
    service_months: u32,
}

const SEEDS: [Seed; 12] = [
    Seed {
        number: "A-0001",
        name: "ThinkPad X1 Carbon",
        category: 0,
        location: 0,
        years_ago: 1,
        month: 1,
        day: 15,
        cost: 159_664,
        life: 3,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "PF-2XK91",
        service_months: 0,
    },
    Seed {
        number: "A-0002",
        name: "Dell UltraSharp 27 monitor",
        category: 0,
        location: 0,
        years_ago: 1,
        month: 2,
        day: 1,
        cost: 34_900,
        life: 3,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "CN-0F8K2",
        service_months: 0,
    },
    Seed {
        number: "A-0003",
        name: "HP LaserJet M507",
        category: 0,
        location: 0,
        years_ago: 2,
        month: 5,
        day: 12,
        cost: 89_900,
        life: 3,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "VNB3K21907",
        service_months: 12,
    },
    Seed {
        number: "A-0004",
        name: "Synology NAS DS923+",
        category: 0,
        location: 0,
        years_ago: 2,
        month: 9,
        day: 1,
        cost: 119_000,
        life: 5,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "2260PDN8",
        service_months: 0,
    },
    Seed {
        number: "A-0005",
        name: "Standing desk, oak",
        category: 1,
        location: 0,
        years_ago: 3,
        month: 3,
        day: 1,
        cost: 129_000,
        life: 13,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "",
        service_months: 0,
    },
    Seed {
        number: "A-0006",
        name: "Conference table",
        category: 1,
        location: 2,
        years_ago: 4,
        month: 6,
        day: 15,
        cost: 245_000,
        life: 13,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "",
        service_months: 0,
    },
    Seed {
        number: "A-0007",
        name: "VW Crafter van",
        category: 2,
        location: 1,
        years_ago: 2,
        month: 4,
        day: 1,
        cost: 4_290_000,
        life: 6,
        method: Method::DecliningBalance,
        rate_bp: 2500,
        serial: "WV1ZZZSYZN9012345",
        service_months: 12,
    },
    Seed {
        number: "A-0008",
        name: "Forklift Linde E20",
        category: 3,
        location: 1,
        years_ago: 3,
        month: 7,
        day: 1,
        cost: 2_850_000,
        life: 10,
        method: Method::DecliningBalance,
        rate_bp: 0,
        serial: "H2X386R01234",
        service_months: 6,
    },
    Seed {
        number: "A-0009",
        name: "Pallet racking",
        category: 3,
        location: 1,
        years_ago: 5,
        month: 1,
        day: 10,
        cost: 1_240_000,
        life: 10,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "",
        service_months: 0,
    },
    Seed {
        number: "A-0010",
        name: "iPhone 15",
        category: 4,
        location: 2,
        years_ago: 0,
        month: 1,
        day: 1,
        cost: 79_900,
        life: 0,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "F2LXK0Q1PQ",
        service_months: 0,
    },
    Seed {
        number: "A-0011",
        name: "Office chair",
        category: 4,
        location: 0,
        years_ago: 1,
        month: 11,
        day: 20,
        cost: 64_900,
        life: 0,
        method: Method::StraightLine,
        rate_bp: 0,
        serial: "",
        service_months: 0,
    },
    Seed {
        number: "A-0012",
        name: "VW Caddy van",
        category: 2,
        location: 1,
        years_ago: 7,
        month: 3,
        day: 1,
        cost: 2_390_000,
        life: 6,
        method: Method::DecliningBalance,
        rate_bp: 2500,
        serial: "WV2ZZZ2KZHX054321",
        service_months: 12,
    },
];

/// The day `n` days before `today`.
fn days_before(today: NaiveDate, n: i64) -> NaiveDate {
    today
        .checked_sub_signed(TimeDelta::days(n))
        .unwrap_or(today)
}

/// The day `n` days after `today`.
fn days_after(today: NaiveDate, n: i64) -> NaiveDate {
    today
        .checked_add_signed(TimeDelta::days(n))
        .unwrap_or(today)
}

/// The sample register on `today`; `new_id` mints the record ids.
#[must_use]
pub fn book(today: NaiveDate, new_id: &mut dyn FnMut() -> String) -> Book {
    let mut b = Book::default();
    let categories: Vec<String> = CATEGORIES
        .iter()
        .map(|(name, life, method)| {
            let id = new_id();
            b.put(Category {
                id: id.clone(),
                name: (*name).to_string(),
                life_years: *life,
                method: *method,
                notes: String::new(),
            });
            id
        })
        .collect();
    let locations: Vec<String> = LOCATIONS
        .iter()
        .map(|(name, address)| {
            let id = new_id();
            b.put(Location {
                id: id.clone(),
                name: (*name).to_string(),
                address: (*address).to_string(),
                notes: String::new(),
            });
            id
        })
        .collect();
    let mut assets: Vec<String> = Vec::new();
    for s in &SEEDS {
        let acquired =
            NaiveDate::from_ymd_opt(today.year() - s.years_ago, s.month, s.day).unwrap_or(today);
        let mut a = Asset::new(&new_id(), s.number, s.name, acquired, s.cost, s.life);
        a.category = categories[s.category].clone();
        a.location = locations[s.location].clone();
        a.method = s.method;
        a.declining_rate_bp = s.rate_bp;
        a.serial = s.serial.to_string();
        a.maintenance_months = s.service_months;
        assets.push(a.id.clone());
        b.put(a);
    }
    // The old van was sold last summer.
    if let Some(van) = b.get_mut::<Asset>(&assets[11]) {
        van.disposed = NaiveDate::from_ymd_opt(today.year() - 1, 6, 30);
        van.disposal_amount = 450_000;
        van.status = Status::Disposed;
        van.notes = "Sold to a dealer; replaced by the Crafter.".to_string();
    }
    let checkouts = [
        // asset, custodian, out (days ago), due (days from today), returned (days ago)
        (0, "Ada Lovelace", 40, Some(-10), None),
        (1, "Grace Hopper", 5, Some(25), None),
        (2, "Alan Turing", 120, Some(-90), Some(95)),
    ];
    for (asset, custodian, out, due, returned) in checkouts {
        let k = Checkout {
            id: new_id(),
            asset: assets[asset].clone(),
            custodian: custodian.to_string(),
            out: days_before(today, out),
            due: due.map(|d: i64| days_after(today, d)),
            returned: returned.map(|r: i64| days_before(today, r)),
            note: String::new(),
        };
        if k.is_open() {
            if let Some(a) = b.get_mut::<Asset>(&assets[asset]) {
                a.status = Status::CheckedOut;
                a.custodian = custodian.to_string();
            }
        }
        b.put(k);
    }
    let log = [
        (
            2,
            200,
            MaintenanceKind::Service,
            "Toner and fuser replaced",
            18_900,
            "Service desk",
        ),
        (
            6,
            300,
            MaintenanceKind::Inspection,
            "Main inspection (TUV)",
            14_900,
            "DEKRA",
        ),
        (
            7,
            100,
            MaintenanceKind::Service,
            "Battery and brakes checked",
            32_000,
            "Linde Service",
        ),
        (
            7,
            30,
            MaintenanceKind::Repair,
            "Hydraulic hose replaced",
            54_050,
            "Linde Service",
        ),
        (
            8,
            400,
            MaintenanceKind::Inspection,
            "Racking inspection (DIN EN 15635)",
            0,
            "Safety officer",
        ),
    ];
    for (asset, ago, kind, description, cost, by) in log {
        b.put(MaintenanceEntry {
            id: new_id(),
            asset: assets[asset].clone(),
            date: days_before(today, ago),
            kind,
            description: description.to_string(),
            cost,
            by: by.to_string(),
        });
    }
    b.sort();
    b
}

#[cfg(test)]
mod tests {
    use chrono::Datelike;

    use super::*;
    use crate::{
        depreciation,
        model::{Asset, Method, Status},
        reports,
    };

    fn ids() -> impl FnMut() -> String {
        let mut n = 0;
        move || {
            n += 1;
            format!("00000000-0000-4000-8000-{n:012}")
        }
    }

    #[test]
    fn the_sample_has_every_kind_of_record_and_every_asset_can_be_saved() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 3).unwrap();
        let mut mint = ids();
        let b = book(today, &mut mint);
        assert!(b.categories.len() >= 4);
        assert!(b.locations.len() >= 3);
        assert!(b.assets.len() >= 10);
        assert!(!b.maintenance.is_empty());
        assert!(!b.checkouts.is_empty());
        for a in &b.assets {
            assert!(a.problems().is_empty(), "{}: {:?}", a.number, a.problems());
            assert!(
                !b.category_name(&a.category).is_empty(),
                "{} has a category",
                a.number
            );
            assert!(a.acquired <= today, "{} was acquired by today", a.number);
        }
        assert!(b
            .assets
            .iter()
            .any(|a| a.method == Method::DecliningBalance));
        assert!(
            b.assets.iter().any(|a| a.life_years == 0),
            "a low-value asset"
        );
        assert!(b
            .assets
            .iter()
            .any(|a| a.status == Status::Disposed && a.disposed.is_some()));
        assert!(
            !reports::overdue_checkouts(&b, today).is_empty(),
            "one check-out is overdue"
        );
        for k in b.checkouts.iter().filter(|k| k.is_open()) {
            let a = b.get::<Asset>(&k.asset).unwrap();
            assert_eq!(a.status, Status::CheckedOut, "{} is out", a.number);
            assert_eq!(a.custodian, k.custodian);
        }
        let numbers: Vec<&str> = b.assets.iter().map(|a| a.number.as_str()).collect();
        let mut unique = numbers.clone();
        unique.dedup();
        assert_eq!(numbers, unique, "sorted, no number twice");
        assert!(depreciation::schedule(&b.assets[0]).len() > 1);
    }

    #[test]
    fn the_sample_is_the_same_relative_to_any_day() {
        let a = book(NaiveDate::from_ymd_opt(2026, 10, 3).unwrap(), &mut ids());
        let b = book(NaiveDate::from_ymd_opt(2030, 3, 31).unwrap(), &mut ids());
        assert_eq!(a.assets.len(), b.assets.len());
        let years: Vec<i32> = a.assets.iter().map(|x| 2026 - x.acquired.year()).collect();
        let later: Vec<i32> = b.assets.iter().map(|x| 2030 - x.acquired.year()).collect();
        assert_eq!(years, later);
    }
}
