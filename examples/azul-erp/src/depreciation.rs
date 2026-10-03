//! Depreciation schedules: what an asset loses each (calendar) year, and
//! its book value on a day.
//!
//! - Monthly pro rata ("monatsgenau"): depreciation starts in the month the
//!   asset was acquired and, for a disposed asset, ends in the month it was
//!   disposed of. A 3-year life acquired in July is depreciated for 6 months
//!   of the first year, 12 of the next two and 6 of the fourth.
//! - Straight-line: each year's share of the depreciable amount (cost minus
//!   residual value) is `months / life months`, rounded to the cent; the
//!   final year takes what is left, so the schedule always ends EXACTLY at
//!   the residual value (1,596.64 over 3 years: 532.21, 532.21, 532.22).
//! - Declining balance: the rate (basis points, [`Asset::rate_bp`]) of the
//!   book value, pro rata, switching to straight-line over the remaining
//!   months as soon as that is more (the switch the German "degressive AfA"
//!   allows), never below the residual value; the final year takes the rest.
//! - A useful life of 0 years is a low-value asset: written off in full in
//!   the year it was acquired.
//!
//! Money is integer minor units, every division rounds half away from zero
//! ([`crate::money::div_round`]).

use chrono::{Datelike, NaiveDate};

use crate::{
    model::{Asset, Method, FULL_RATE_BP},
    money::div_round,
};

/// One calendar year of a schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScheduleRow {
    pub year: i32,
    /// The first month of the year the asset was depreciated in (1 to 12).
    pub first_month: u32,
    /// The months of the year it was depreciated for (1 to 12).
    pub months: u32,
    /// The book value at the start of the year.
    pub opening: i64,
    pub depreciation: i64,
    /// Every year's depreciation so far, this one included.
    pub accumulated: i64,
    /// The book value at the end of the year.
    pub closing: i64,
}

/// What a schedule is computed from (an asset's figures).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Plan {
    pub cost: i64,
    pub residual: i64,
    pub life_years: u32,
    pub method: Method,
    /// The declining-balance rate in basis points.
    pub rate_bp: u32,
    /// Depreciation starts in this day's month.
    pub start: NaiveDate,
    /// Depreciation ends in this day's month (the disposal).
    pub end: Option<NaiveDate>,
}

impl Plan {
    /// The plan of an asset.
    #[must_use]
    pub fn of(asset: &Asset) -> Plan {
        Plan {
            cost: asset.cost,
            residual: asset.residual,
            life_years: asset.life_years,
            method: asset.method,
            rate_bp: asset.rate_bp(),
            start: asset.acquired,
            end: asset.disposed,
        }
    }
}

/// The asset's schedule, year by year.
#[must_use]
pub fn schedule(asset: &Asset) -> Vec<ScheduleRow> {
    schedule_of(&Plan::of(asset))
}

/// The schedule of a plan, year by year (empty when there is nothing to
/// depreciate: the residual value is the cost).
#[must_use]
pub fn schedule_of(plan: &Plan) -> Vec<ScheduleRow> {
    let _ = (plan, FULL_RATE_BP);
    todo!("GREEN")
}

/// The asset's book value at the end of `day`: its cost before it was
/// acquired, accrued month by month within a year (the day's month counts),
/// its residual value after its life, 0 from the day it was disposed of.
#[must_use]
pub fn book_value_on(asset: &Asset, day: NaiveDate) -> i64 {
    let _ = (asset, day);
    todo!("GREEN")
}

/// The asset's depreciation in `year` (0 outside its schedule).
#[must_use]
pub fn depreciation_in_year(asset: &Asset, year: i32) -> i64 {
    let _ = (asset, year);
    todo!("GREEN")
}

/// What the disposal brought beyond the book value then: a gain (positive)
/// or a loss (negative); `None` for an asset not disposed of.
#[must_use]
pub fn disposal_result(asset: &Asset) -> Option<i64> {
    let _ = asset;
    todo!("GREEN")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn asset(cost: i64, life: u32, acquired: NaiveDate) -> Asset {
        Asset::new("a", "A-1", "Asset", acquired, cost, life)
    }

    fn deps(rows: &[ScheduleRow]) -> Vec<i64> {
        rows.iter().map(|r| r.depreciation).collect()
    }

    #[test]
    fn straight_line_spreads_the_cost_evenly_and_the_last_year_takes_the_rounding() {
        // The asset-management plan's sample: 1,596.64 over 3 years.
        let rows = schedule(&asset(159_664, 3, day(2026, 1, 15)));
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows.iter().map(|r| r.year).collect::<Vec<_>>(),
            [2026, 2027, 2028]
        );
        assert_eq!(deps(&rows), [53_221, 53_221, 53_222]);
        assert_eq!(
            rows.iter().map(|r| r.accumulated).collect::<Vec<_>>(),
            [53_221, 106_442, 159_664]
        );
        assert_eq!(
            rows.iter().map(|r| r.opening).collect::<Vec<_>>(),
            [159_664, 106_443, 53_222]
        );
        assert_eq!(
            rows.iter().map(|r| r.closing).collect::<Vec<_>>(),
            [106_443, 53_222, 0]
        );
        assert!(rows.iter().all(|r| r.months == 12 && r.first_month == 1));
    }

    #[test]
    fn the_first_and_last_years_are_pro_rata_by_month() {
        let rows = schedule(&asset(159_664, 3, day(2026, 7, 15)));
        assert_eq!(
            rows.iter().map(|r| r.year).collect::<Vec<_>>(),
            [2026, 2027, 2028, 2029]
        );
        assert_eq!(
            rows.iter().map(|r| r.months).collect::<Vec<_>>(),
            [6, 12, 12, 6]
        );
        assert_eq!(rows[0].first_month, 7);
        assert_eq!(rows[1].first_month, 1);
        assert_eq!(deps(&rows), [26_611, 53_221, 53_221, 26_611]);
        assert_eq!(rows[3].closing, 0);
    }

    #[test]
    fn declining_balance_switches_to_straight_line_when_that_is_more() {
        let mut a = asset(1_000_000, 5, day(2026, 1, 1));
        a.method = Method::DecliningBalance;
        assert_eq!(a.rate_bp(), 4000);
        let rows = schedule(&a);
        // 40 % of the book value for three years, then the remaining
        // 2,160.00 straight-line over the last two (1,080.00 > 864.00).
        assert_eq!(deps(&rows), [400_000, 240_000, 144_000, 108_000, 108_000]);
        assert_eq!(rows[4].closing, 0);
    }

    #[test]
    fn declining_balance_with_its_own_rate_stops_at_the_residual_value() {
        let mut a = asset(1_000_000, 5, day(2026, 4, 1));
        a.method = Method::DecliningBalance;
        a.declining_rate_bp = 2500;
        a.residual = 100_000;
        let rows = schedule(&a);
        assert_eq!(rows.len(), 6, "April: 9 months, four years, 3 months");
        // 25 % of 10,000.00 for 9 months = 1,875.00 (straight-line: 1,350.00).
        assert_eq!(rows[0].depreciation, 187_500);
        assert!(rows.iter().all(|r| r.closing >= 100_000), "{rows:?}");
        assert_eq!(rows.last().unwrap().closing, 100_000);
        assert_eq!(rows.last().unwrap().accumulated, 900_000);
    }

    #[test]
    fn a_low_value_asset_is_written_off_in_the_year_it_was_acquired() {
        let rows = schedule(&asset(79_900, 0, day(2026, 3, 10)));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].year, 2026);
        assert_eq!(rows[0].first_month, 3);
        assert_eq!(rows[0].months, 10);
        assert_eq!(rows[0].depreciation, 79_900);
        assert_eq!(rows[0].closing, 0);
    }

    #[test]
    fn depreciation_stops_in_the_month_of_disposal() {
        let mut a = asset(159_664, 3, day(2026, 1, 15));
        a.disposed = Some(day(2027, 6, 30));
        a.disposal_amount = 50_000;
        let rows = schedule(&a);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].months, 6);
        assert_eq!(deps(&rows), [53_221, 26_611]);
        assert_eq!(rows[1].closing, 79_832);
        assert_eq!(disposal_result(&a), Some(50_000 - 79_832), "a loss");
        assert_eq!(disposal_result(&asset(1, 1, day(2026, 1, 1))), None);
        let mut early = a.clone();
        early.disposed = Some(day(2025, 12, 31));
        assert!(
            schedule(&early).is_empty(),
            "disposed before it was acquired"
        );
    }

    #[test]
    fn nothing_to_depreciate_gives_no_schedule() {
        let mut a = asset(100_000, 5, day(2026, 1, 1));
        a.residual = 100_000;
        assert!(schedule(&a).is_empty());
        assert_eq!(book_value_on(&a, day(2030, 1, 1)), 100_000);
        assert!(schedule(&asset(0, 5, day(2026, 1, 1))).is_empty());
    }

    #[test]
    fn the_book_value_on_a_day_accrues_month_by_month() {
        let a = asset(159_664, 3, day(2026, 1, 15));
        assert_eq!(
            book_value_on(&a, day(2025, 12, 31)),
            159_664,
            "not acquired yet"
        );
        // Six of the first year's twelve months: 532.21 / 2 = 266.105 -> 266.11.
        assert_eq!(book_value_on(&a, day(2026, 6, 30)), 159_664 - 26_611);
        assert_eq!(book_value_on(&a, day(2026, 12, 31)), 106_443);
        assert_eq!(
            book_value_on(&a, day(2027, 1, 1)),
            106_443 - div_round(53_221, 12)
        );
        assert_eq!(book_value_on(&a, day(2028, 12, 31)), 0);
        assert_eq!(book_value_on(&a, day(2040, 1, 1)), 0);
        let mut sold = a.clone();
        sold.disposed = Some(day(2027, 6, 30));
        assert_eq!(book_value_on(&sold, day(2027, 6, 29)), 79_832);
        assert_eq!(
            book_value_on(&sold, day(2027, 6, 30)),
            0,
            "off the books from the disposal day"
        );
        assert_eq!(depreciation_in_year(&a, 2027), 53_221);
        assert_eq!(depreciation_in_year(&a, 2031), 0);
    }

    #[test]
    fn every_schedule_ends_exactly_at_the_residual_value() {
        for cost in [1_i64, 99, 100_003, 159_664, 12_345_678] {
            for life in [1_u32, 3, 5, 7, 10] {
                for month in [1_u32, 6, 12] {
                    for method in Method::ALL {
                        for residual in [0, cost / 10] {
                            let mut a = asset(cost, life, day(2026, month, 1));
                            a.method = method;
                            a.residual = residual;
                            let rows = schedule(&a);
                            let what = format!(
                                "{cost} {life}y from {month} {method:?} residual {residual}"
                            );
                            if cost == residual {
                                assert!(rows.is_empty(), "{what}");
                                continue;
                            }
                            let years = if month == 1 { life } else { life + 1 };
                            assert_eq!(rows.len() as u32, years, "{what}");
                            assert_eq!(
                                rows.iter().map(|r| r.months).sum::<u32>(),
                                life * 12,
                                "{what}"
                            );
                            assert_eq!(deps(&rows).iter().sum::<i64>(), cost - residual, "{what}");
                            assert_eq!(rows.last().unwrap().closing, residual, "{what}");
                            assert!(rows.iter().all(|r| r.depreciation >= 0), "{what}: {rows:?}");
                            assert!(
                                rows.windows(2).all(|w| w[1].opening == w[0].closing),
                                "{what}"
                            );
                        }
                    }
                }
            }
        }
    }
}
