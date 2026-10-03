//! The sample register `--sample` writes into an empty data tree (the ERP
//! README: "a seed script with realistic data for one company ... 10
//! assets"): Mustermann GmbH's categories, locations, assets acquired over
//! the last years (both methods, a low-value asset, a disposed van), a
//! maintenance log and check-outs, one of them overdue. Every day is
//! relative to `today`, so the sample looks the same whenever it is made.

use chrono::NaiveDate;

use crate::store::Book;

/// The sample register on `today`; `new_id` mints the record ids.
#[must_use]
pub fn book(today: NaiveDate, new_id: &mut dyn FnMut() -> String) -> Book {
    let _ = (today, new_id);
    Book::default()
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
