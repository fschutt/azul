//! The currency converter: exact conversions through the euro in integer millionths, the
//! default rates and where they come from, the operator's own rates, the ECB's files, and the
//! price points each currency's custom asks for.

use crate::currency::*;
use crate::{Amount, Registry};

fn amt(currency: &str, minor: i64) -> Amount {
    Amount::new(currency, minor)
}

fn conv() -> CurrencyConverter {
    CurrencyConverter::default()
}

fn point(currency: &str, minor: i64) -> i64 {
    price_point(&amt(currency, minor)).minor
}

#[test]
fn the_default_rates_are_the_ecb_reference_rates_of_their_day() {
    let c = conv();
    let pln = c.rate("PLN").expect("PLN");
    assert_eq!(pln.per_eur_micro, 4_370_000);
    assert_eq!(pln.date, ECB_DATE);
    assert!(pln.source.contains("ECB"), "{}", pln.source);
    assert_eq!(c.rate("JPY").map(|r| r.per_eur_micro), Some(177_820_000));
    assert_eq!(c.rate("GBP").map(|r| r.per_eur_micro), Some(843_850));
    assert_eq!(c.rate("EUR").map(|r| r.per_eur_micro), Some(MICRO));
}

#[test]
fn every_currency_the_registry_names_has_a_default_rate_with_its_source_and_date() {
    let r = Registry::builtin().expect("registry");
    let mut named: Vec<&str> = r.countries.iter().map(|c| c.currency.as_str()).collect();
    for route in &r.routes {
        named.extend(route.currencies.iter().map(String::as_str));
    }
    let c = conv();
    for code in named.into_iter().filter(|c| !c.is_empty() && *c != "*") {
        let rate = c.rate(code).unwrap_or_else(|| panic!("no default rate for {code}"));
        assert!(rate.per_eur_micro > 0, "{code}");
        assert_eq!(rate.date.len(), 10, "{code}: {}", rate.date);
        assert!(!rate.source.is_empty(), "{code}");
    }
}

#[test]
fn a_currency_the_ecb_does_not_publish_names_its_own_source() {
    let c = conv();
    for code in ["AED", "SAR", "ARS", "CLP", "COP", "PEN", "UYU", "TWD", "NGN", "KES", "EGP", "VND"] {
        let rate = c.rate(code).unwrap_or_else(|| panic!("{code}"));
        assert!(!rate.source.starts_with("ECB"), "{code}: {}", rate.source);
    }
    // the dirham is pegged to the dollar: 3.6725 x the ECB's 1.1253
    assert_eq!(c.rate("AED").map(|r| r.per_eur_micro), Some(4_132_664));
    // the Treasury's quarter-end rates are older than the ECB's day, and say so
    assert_eq!(c.rate("KES").map(|r| r.date.as_str()), Some("2026-09-30"));
}

#[test]
fn converting_euros_to_zloty_is_exact_in_grosze() {
    let c = conv();
    // 4.99 x 4.37 = 21.8063 and 0.99 x 4.37 = 4.3263
    assert_eq!(c.convert(&amt("EUR", 499), "PLN"), Ok(amt("PLN", 2181)));
    assert_eq!(c.convert(&amt("EUR", 99), "PLN"), Ok(amt("PLN", 433)));
    assert_eq!(c.convert(&amt("EUR", 499), "EUR"), Ok(amt("EUR", 499)));
}

#[test]
fn a_conversion_rounds_half_away_from_zero() {
    let c = CurrencyConverter::euro_only()
        .with_rate("XTS", 1_500_000, "2026-10-16", "a test rate")
        .expect("rate");
    // 0.01 x 1.5 = 0.015
    assert_eq!(c.convert(&amt("EUR", 1), "XTS"), Ok(amt("XTS", 2)));
    assert_eq!(c.convert(&amt("EUR", -1), "XTS"), Ok(amt("XTS", -2)));
}

#[test]
fn a_currency_without_decimals_gets_whole_units() {
    let c = conv();
    // 4.99 x 177.82 = 887.3218 yen; 4.99 x 1499.01 = 7480.06 won
    assert_eq!(c.convert(&amt("EUR", 499), "JPY"), Ok(amt("JPY", 887)));
    assert_eq!(c.convert(&amt("EUR", 499), "KRW"), Ok(amt("KRW", 7480)));
}

#[test]
fn two_foreign_currencies_convert_through_the_euro_in_one_rounding() {
    let c = conv();
    // 21.81 / 4.37 x 0.9272 = 4.6275...
    assert_eq!(c.convert(&amt("PLN", 2181), "CHF"), Ok(amt("CHF", 463)));
}

#[test]
fn a_round_trip_through_any_default_currency_moves_a_price_by_at_most_one_cent() {
    let c = conv();
    let codes: Vec<String> = c.rates().map(|(code, _)| code.to_string()).collect();
    for code in &codes {
        for cents in [1, 49, 99, 249, 499, 899, 1999, 3499, 4990, 34_990, 99_999] {
            let there = c.convert(&amt("EUR", cents), code).expect("there");
            let back = c.convert(&there, "EUR").expect("back");
            assert!((back.minor - cents).abs() <= 1, "{code}: {cents} -> {} -> {}", there.minor, back.minor);
        }
    }
}

#[test]
fn an_unknown_currency_is_refused_not_guessed() {
    let c = conv();
    assert_eq!(
        c.convert(&amt("EUR", 100), "XYZ"),
        Err(CurrencyError::UnknownCurrency("XYZ".into()))
    );
    assert_eq!(
        c.convert(&amt("XYZ", 100), "EUR"),
        Err(CurrencyError::UnknownCurrency("XYZ".into()))
    );
}

#[test]
fn a_rate_passed_in_replaces_the_default_with_its_own_date_and_source() {
    let c = conv().with_rate("PLN", 4_500_000, "2026-11-02", "the operator").expect("rate");
    assert_eq!(c.convert(&amt("EUR", 100), "PLN"), Ok(amt("PLN", 450)));
    let r = c.rate("PLN").expect("PLN");
    assert_eq!((r.date.as_str(), r.source.as_str()), ("2026-11-02", "the operator"));
    // the others stay
    assert_eq!(c.rate("CHF").map(|r| r.per_eur_micro), Some(927_200));
}

#[test]
fn a_rate_that_cannot_be_right_is_refused() {
    let c = conv();
    assert!(matches!(c.clone().with_rate("PLN", 0, "2026-11-02", "x"), Err(CurrencyError::BadRate(_))));
    assert!(matches!(c.clone().with_rate("PLN", -5, "2026-11-02", "x"), Err(CurrencyError::BadRate(_))));
    assert!(matches!(c.clone().with_rate("pln", 4_000_000, "2026-11-02", "x"), Err(CurrencyError::BadCode(_))));
    assert!(matches!(c.clone().with_rate("PLN", 4_000_000, "2.11.2026", "x"), Err(CurrencyError::BadDate(_))));
    // the euro is the unit everything is measured in
    assert!(matches!(c.with_rate("EUR", 1_100_000, "2026-11-02", "x"), Err(CurrencyError::BadRate(_))));
}

#[test]
fn each_currency_names_its_price_rule() {
    assert_eq!(price_style("EUR"), PriceStyle::Cents99);
    assert_eq!(price_style("USD"), PriceStyle::Cents99);
    assert_eq!(price_style("PLN"), PriceStyle::Cents99);
    assert_eq!(price_style("CHF"), PriceStyle::Cents90);
    assert_eq!(price_style("BRL"), PriceStyle::Cents90);
    for code in ["SEK", "NOK", "DKK", "CZK", "HUF", "ISK", "CLP", "INR", "MXN"] {
        assert_eq!(price_style(code), PriceStyle::Whole9, "{code}");
    }
    for code in ["JPY", "KRW", "IDR", "VND", "COP"] {
        assert_eq!(price_style(code), PriceStyle::WholeRound, "{code}");
    }
    // a currency without a rule of its own: by its minor unit
    assert_eq!(price_style("XYZ"), PriceStyle::Cents99);
    assert_eq!(price_style("XAF"), PriceStyle::WholeRound);
}

#[test]
fn prices_like_the_euros_end_in_49_or_99() {
    assert_eq!(point("PLN", 2181), 2199);
    assert_eq!(point("PLN", 1307), 1299);
    assert_eq!(point("EUR", 2550), 2549);
    assert_eq!(point("USD", 22_317), 22_299);
    // under five units half a unit is too coarse: the tenth ends in 9
    assert_eq!(point("PLN", 433), 429);
    assert_eq!(point("USD", 111), 109);
    // a tie goes up
    assert_eq!(point("GBP", 84), 89);
}

#[test]
fn swiss_and_brazilian_prices_end_in_90() {
    assert_eq!(point("CHF", 1854), 1890);
    assert_eq!(point("BRL", 2854), 2890);
    assert_eq!(point("CHF", 3244), 3290);
    // under ten units the tenth is round
    assert_eq!(point("CHF", 463), 460);
    assert_eq!(point("CHF", 92), 90);
}

#[test]
fn nordic_and_other_whole_unit_prices_end_in_9() {
    assert_eq!(point("SEK", 110_335), 109_900);
    assert_eq!(point("SEK", 22_280), 21_900);
    assert_eq!(point("HUF", 35_945), 35_900);
    assert_eq!(point("HUF", 179_500), 179_900);
    assert_eq!(point("INR", 10_580), 10_900);
    // ISK and CLP have no minor unit
    assert_eq!(point("ISK", 135), 139);
    assert_eq!(point("CLP", 1096), 1099);
    // under a hundred units a 9-ending would move the price by a tenth or more: whole units
    assert_eq!(point("SEK", 5561), 5600);
    assert_eq!(point("SEK", 1103), 1100);
}

#[test]
fn yen_won_rupiah_and_dong_keep_two_significant_digits() {
    assert_eq!(point("JPY", 176), 180);
    assert_eq!(point("JPY", 887), 890);
    assert_eq!(point("KRW", 7480), 7500);
    assert_eq!(point("KRW", 1484), 1500);
    // 101 249.50 rupiah: two significant digits
    assert_eq!(point("IDR", 10_124_950), 10_000_000);
    assert_eq!(point("VND", 29_171), 29_000);
}

#[test]
fn a_price_rounded_across_its_range_s_edge_takes_the_rule_of_the_range_it_lands_in() {
    // 99.54 crowns round to 100 as whole units, but from a hundred up prices end in 9
    assert_eq!(point("SEK", 9954), 9900);
    // 9.96 francs round to 10.00 as tenths, but from ten up prices end in .90
    assert_eq!(point("CHF", 996), 990);
    assert_eq!(point("BRL", 997), 990);
    // the yen's two digits: 996 is 1 000 either way
    assert_eq!(point("JPY", 996), 1000);
}

#[test]
fn a_price_point_is_its_own_price_point() {
    let codes = ["EUR", "GBP", "CHF", "BRL", "SEK", "HUF", "ISK", "JPY", "KRW", "IDR", "KWD"];
    for code in codes {
        for minor in (1..20_000).step_by(37).chain((20_000..5_000_000).step_by(9_973)) {
            let once = price_point(&amt(code, minor));
            assert!(once.minor > 0, "{code} {minor}");
            assert_eq!(price_point(&once), once, "{code} {minor}");
        }
    }
}

#[test]
fn the_price_point_of_a_converted_price_is_a_price_point_in_the_target() {
    let c = conv();
    // 4.99 EUR: 21.81 PLN -> 21.99; 887 JPY -> 890; 55.61 SEK -> 56
    assert_eq!(c.price_point(&amt("EUR", 499), "PLN"), Ok(amt("PLN", 2199)));
    assert_eq!(c.price_point(&amt("EUR", 499), "JPY"), Ok(amt("JPY", 890)));
    assert_eq!(c.price_point(&amt("EUR", 499), "SEK"), Ok(amt("SEK", 5600)));
}

const DAILY: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<gesmes:Envelope xmlns:gesmes=\"http://www.gesmes.org/xml/2002-08-01\" xmlns=\"http://www.ecb.int/vocabulary/2002-08-01/eurofxref\">
	<gesmes:subject>Reference rates</gesmes:subject>
	<Cube>
		<Cube time='2026-10-16'>
			<Cube currency='USD' rate='1.1253'/>
			<Cube currency='GBP' rate='0.84385'/>
			<Cube currency='PLN' rate='4.3700'/>
			<Cube currency='IDR' rate='20290.28'/>
		</Cube>
	</Cube>
</gesmes:Envelope>";

const HISTORY: &str = "<Cube><Cube time=\"2026-10-16\"><Cube currency=\"PLN\" rate=\"4.37\"/></Cube>\
<Cube time=\"2026-10-15\"><Cube currency=\"PLN\" rate=\"4.3651\"/><Cube currency=\"USD\" rate=\"1.1208\"/></Cube></Cube>";

#[test]
fn the_ecbs_daily_file_parses_into_exact_millionths_and_their_decimals() {
    let days = parse_ecb(DAILY).expect("parse");
    assert_eq!(days.len(), 1);
    let d = &days[0];
    assert_eq!(d.date, "2026-10-16");
    assert_eq!(d.rates.get("PLN"), Some(&EcbQuote { per_eur_micro: 4_370_000, decimals: 4 }));
    assert_eq!(d.rates.get("GBP"), Some(&EcbQuote { per_eur_micro: 843_850, decimals: 5 }));
    assert_eq!(d.rates.get("IDR"), Some(&EcbQuote { per_eur_micro: 20_290_280_000, decimals: 2 }));
}

#[test]
fn the_ecbs_history_file_gives_every_day_in_either_quote_style() {
    let days = parse_ecb(HISTORY).expect("parse");
    let dates: Vec<&str> = days.iter().map(|d| d.date.as_str()).collect();
    assert_eq!(dates, ["2026-10-16", "2026-10-15"]);
    assert_eq!(days[1].rates.get("USD").map(|q| q.per_eur_micro), Some(1_120_800));
    assert_eq!(days[1].rates.get("PLN").map(|q| q.per_eur_micro), Some(4_365_100));
}

#[test]
fn a_rate_that_is_not_a_plain_number_is_an_error_not_a_gap() {
    for bad in ["1,1253", "1.1253e0", "-1.1", "", "1.1234567"] {
        let xml = format!("<Cube time='2026-10-16'><Cube currency='USD' rate='{bad}'/></Cube>");
        assert!(parse_ecb(&xml).is_err(), "{bad:?}");
    }
    assert!(parse_ecb("<html>not the ECB</html>").is_err());
}

#[test]
fn an_ecb_day_replaces_the_published_rates_and_keeps_the_others() {
    let days = parse_ecb(HISTORY).expect("parse");
    let c = conv().with_ecb(&days[1]).expect("rates");
    let pln = c.rate("PLN").expect("PLN");
    assert_eq!((pln.per_eur_micro, pln.date.as_str()), (4_365_100, "2026-10-15"));
    assert!(pln.source.starts_with("ECB"));
    // not in that day's file: unchanged
    assert_eq!(c.rate("AED").map(|r| r.per_eur_micro), Some(4_132_664));
    assert_eq!(c.rate("JPY").map(|r| r.date.as_str()), Some(ECB_DATE));
}

#[test]
fn no_float_touches_money() {
    let source = include_str!("currency.rs");
    for word in ["f64", "f32", " as f"] {
        assert!(!source.contains(word), "currency.rs uses {word}");
    }
}

#[test]
fn an_amount_goes_to_a_provider_as_its_exact_decimal_and_comes_back_in_minor_units() {
    assert_eq!(decimal_text(34_290, "BRL"), "342.90");
    assert_eq!(decimal_text(5, "EUR"), "0.05");
    assert_eq!(decimal_text(-1250, "PLN"), "-12.50");
    assert_eq!(decimal_text(1100, "JPY"), "1100");
    assert_eq!(decimal_text(1234, "KWD"), "1.234");
    assert_eq!(parse_decimal("342.9", "BRL"), Some(34_290));
    assert_eq!(parse_decimal("342.90", "BRL"), Some(34_290));
    assert_eq!(parse_decimal("1100", "JPY"), Some(1100));
    assert_eq!(parse_decimal("996.2143853357243", "USD"), Some(99_621), "rounded past the cents");
    assert_eq!(parse_decimal("0.005", "EUR"), Some(1), "half away from zero");
    assert_eq!(parse_decimal("-1.255", "EUR"), Some(-126));
    for bad in ["", "1,50", "1e3", "abc", "1.2.3", "."] {
        assert_eq!(parse_decimal(bad, "EUR"), None, "{bad:?}");
    }
}
