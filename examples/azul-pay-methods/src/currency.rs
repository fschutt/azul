//! Prices in other currencies: a converter with fixed rates, and the price points each
//! currency's custom asks for.
//!
//! A rate says how many units of a currency one euro buys, in integer millionths (1 EUR =
//! 4.37 PLN is 4_370_000), so every reader computes the same minor units and no float ever
//! touches money. A conversion goes through the euro in one rounding, half away from zero, into
//! the target's minor unit (ISO 4217, [`crate::minor_digits`]).
//!
//! The default rates are the ECB's euro reference rates of [`ECB_DATE`] and the currencies of
//! the methods registry the ECB does not publish, each with its source and date
//! ([`OTHER_RATES`]). They are constants on purpose: prices frozen from them stay what they were
//! until an operator passes newer ones ([`CurrencyConverter::with_rate`], or a day of an ECB file
//! through [`parse_ecb`] and [`CurrencyConverter::with_ecb`]). Nothing here reads the network.
//! A converted price is then rounded to a price point ([`price_point`]) by its currency's rule
//! ([`price_style`]).

use std::collections::BTreeMap;
use std::fmt;

use crate::{currency_ok, minor_digits, Amount};

/// One unit in millionths: the euro's rate against itself.
pub const MICRO: i64 = 1_000_000;

/// The day of the ECB rates the default converter carries.
pub const ECB_DATE: &str = "2026-10-16";

/// The source every ECB rate names.
pub const ECB_SOURCE: &str = "ECB euro foreign exchange reference rates";

/// The ECB's euro reference rates of [`ECB_DATE`] (eurofxref-daily.xml), in millionths of each
/// currency per euro.
pub const ECB_RATES: [(&str, i64); 29] = [
    ("USD", 1_125_300),
    ("JPY", 177_820_000),
    ("CZK", 24_323_000),
    ("DKK", 7_474_800),
    ("GBP", 843_850),
    ("HUF", 363_080_000),
    ("PLN", 4_370_000),
    ("RON", 5_331_000),
    ("SEK", 11_145_000),
    ("CHF", 927_200),
    ("ISK", 136_200_000),
    ("NOK", 10_624_000),
    ("TRY", 55_732_800),
    ("AUD", 1_617_100),
    ("BRL", 5_719_700),
    ("CAD", 1_593_500),
    ("CNY", 7_524_000),
    ("HKD", 8_831_600),
    ("IDR", 20_290_280_000),
    ("ILS", 3_482_200),
    ("INR", 106_864_000),
    ("KRW", 1_499_010_000),
    ("MXN", 20_687_100),
    ("MYR", 4_592_900),
    ("NZD", 1_986_800),
    ("PHP", 70_858_000),
    ("SGD", 1_439_500),
    ("THB", 37_602_000),
    ("ZAR", 18_493_000),
];

/// A default rate the ECB does not publish.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OtherRate {
    pub code: &'static str,
    /// Millionths of the currency per euro.
    pub per_eur_micro: i64,
    /// The day of the source's rate.
    pub date: &'static str,
    /// Who published it, and how it became a euro rate.
    pub source: &'static str,
}

/// The registry's currencies the ECB does not publish. A rate against the US dollar is crossed
/// with the ECB's dollar rate of the same day (1.1253), the US Treasury's with its own euro rate.
pub const OTHER_RATES: [OtherRate; 12] = [
    OtherRate {
        code: "AED",
        per_eur_micro: 4_132_664,
        date: "2026-10-16",
        source: "the dirham's peg of 3.6725 per US dollar (Central Bank of the UAE) x the ECB's dollar rate",
    },
    OtherRate {
        code: "SAR",
        per_eur_micro: 4_219_875,
        date: "2026-10-16",
        source: "the riyal's peg of 3.75 per US dollar (Saudi Central Bank) x the ECB's dollar rate",
    },
    OtherRate {
        code: "ARS",
        per_eur_micro: 1_704_258_600,
        date: "2026-10-16",
        source: "BCRA exchange rates (Comunicacion A 3500 table), the euro's quote",
    },
    OtherRate {
        code: "CLP",
        per_eur_micro: 1_107_396_477,
        date: "2026-10-16",
        source: "IMF representative rate reported by the Banco Central de Chile, 984.09 per US dollar, x the ECB's dollar rate",
    },
    OtherRate {
        code: "COP",
        per_eur_micro: 3_567_718_638,
        date: "2026-10-16",
        source: "TRM of the Superintendencia Financiera de Colombia, 3170.46 per US dollar, x the ECB's dollar rate",
    },
    OtherRate {
        code: "PEN",
        per_eur_micro: 3_855_278,
        date: "2026-10-16",
        source: "IMF representative rate reported by the Banco Central de Reserva del Peru, 3.426 per US dollar, x the ECB's dollar rate",
    },
    OtherRate {
        code: "UYU",
        per_eur_micro: 45_121_154,
        date: "2026-10-16",
        source: "IMF representative rate reported by the Banco Central del Uruguay, 40.097 per US dollar, x the ECB's dollar rate",
    },
    OtherRate {
        code: "TWD",
        per_eur_micro: 36_389_951,
        date: "2026-10-16",
        source: "Central Bank of the Republic of China (Taiwan) closing rate, 32.338 per US dollar, x the ECB's dollar rate",
    },
    OtherRate {
        code: "NGN",
        per_eur_micro: 1_490_637_000,
        date: "2026-10-16",
        source: "Central Bank of Nigeria, the euro's central rate",
    },
    OtherRate {
        code: "KES",
        per_eur_micro: 147_026_107,
        date: "2026-09-30",
        source: "US Treasury reporting rate of exchange, 129.53 per US dollar, / its euro rate of 0.881",
    },
    OtherRate {
        code: "EGP",
        per_eur_micro: 58_898_978,
        date: "2026-09-30",
        source: "US Treasury reporting rate of exchange, 51.89 per US dollar, / its euro rate of 0.881",
    },
    OtherRate {
        code: "VND",
        per_eur_micro: 29_465_380_250,
        date: "2026-09-30",
        source: "US Treasury reporting rate of exchange, 25959 per US dollar, / its euro rate of 0.881",
    },
];

/// One currency's rate against the euro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rate {
    /// Units of the currency one euro buys, in millionths.
    pub per_eur_micro: i64,
    /// The day it is from (`2026-10-16`): a frozen price says which rates made it.
    pub date: String,
    /// Who published it, and how it was derived where it was.
    pub source: String,
}

/// Why a conversion or a rate was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrencyError {
    /// The converter has no rate for this currency: it is never guessed.
    UnknownCurrency(String),
    /// Not three capital letters.
    BadCode(String),
    /// Not a positive rate, or a rate for the euro (the unit every rate is measured in).
    BadRate(String),
    /// Not a day written `YYYY-MM-DD`.
    BadDate(String),
    /// The result does not fit an amount.
    Overflow,
}

impl fmt::Display for CurrencyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CurrencyError::UnknownCurrency(c) => write!(f, "no exchange rate for {c}"),
            CurrencyError::BadCode(c) => write!(f, "`{c}` is not a currency code like EUR"),
            CurrencyError::BadRate(r) => write!(f, "not a usable rate: {r}"),
            CurrencyError::BadDate(d) => write!(f, "`{d}` is not a day like 2026-10-16"),
            CurrencyError::Overflow => write!(f, "the amount is too large to convert"),
        }
    }
}

impl std::error::Error for CurrencyError {}

/// Fixed exchange rates against the euro, and conversions with them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrencyConverter {
    /// Every known currency's rate by its code; the euro is always there at exactly [`MICRO`].
    rates: BTreeMap<String, Rate>,
}

impl Default for CurrencyConverter {
    /// The ECB's rates of [`ECB_DATE`] and [`OTHER_RATES`].
    fn default() -> Self {
        let mut c = CurrencyConverter::euro_only();
        for (code, per_eur_micro) in ECB_RATES {
            c.rates.insert(code.to_string(), rate(per_eur_micro, ECB_DATE, ECB_SOURCE));
        }
        for o in OTHER_RATES.iter() {
            c.rates.insert(o.code.to_string(), rate(o.per_eur_micro, o.date, o.source));
        }
        c
    }
}

fn rate(per_eur_micro: i64, date: &str, source: &str) -> Rate {
    Rate {
        per_eur_micro,
        date: date.to_string(),
        source: source.to_string(),
    }
}

impl CurrencyConverter {
    /// A converter that knows the euro only; rates are added with [`CurrencyConverter::with_rate`].
    pub fn euro_only() -> CurrencyConverter {
        let mut rates = BTreeMap::new();
        rates.insert("EUR".to_string(), rate(MICRO, ECB_DATE, "the euro, the unit of every rate"));
        CurrencyConverter { rates }
    }

    /// The converter with `currency` at `per_eur_micro` millionths per euro, published on `date`
    /// by `source`; a rate it had is replaced.
    pub fn with_rate(
        mut self,
        currency: &str,
        per_eur_micro: i64,
        date: &str,
        source: &str,
    ) -> Result<CurrencyConverter, CurrencyError> {
        if !currency_ok(currency) {
            return Err(CurrencyError::BadCode(currency.to_string()));
        }
        if currency == "EUR" || per_eur_micro <= 0 {
            return Err(CurrencyError::BadRate(format!("{currency} at {per_eur_micro} millionths per euro")));
        }
        if !date_ok(date) {
            return Err(CurrencyError::BadDate(date.to_string()));
        }
        self.rates.insert(currency.to_string(), rate(per_eur_micro, date, source));
        Ok(self)
    }

    /// The converter with every rate of one ECB day; the currencies the ECB does not publish keep
    /// theirs.
    pub fn with_ecb(self, day: &EcbDay) -> Result<CurrencyConverter, CurrencyError> {
        let mut c = self;
        for (code, q) in &day.rates {
            c = c.with_rate(code, q.per_eur_micro, &day.date, ECB_SOURCE)?;
        }
        Ok(c)
    }

    pub fn rate(&self, currency: &str) -> Option<&Rate> {
        self.rates.get(currency)
    }

    /// Every rate, by code.
    pub fn rates(&self) -> impl Iterator<Item = (&str, &Rate)> {
        self.rates.iter().map(|(code, r)| (code.as_str(), r))
    }

    /// `amount` in `to`, exact to `to`'s minor unit (rounded half away from zero).
    pub fn convert(&self, amount: &Amount, to: &str) -> Result<Amount, CurrencyError> {
        let from = self
            .rate(&amount.currency)
            .ok_or_else(|| CurrencyError::UnknownCurrency(amount.currency.clone()))?;
        let into = self
            .rate(to)
            .ok_or_else(|| CurrencyError::UnknownCurrency(to.to_string()))?;
        let minor = convert_minor(amount.minor, &amount.currency, from.per_eur_micro, to, into.per_eur_micro)
            .ok_or(CurrencyError::Overflow)?;
        Ok(Amount::new(to, minor))
    }

    /// `amount` in `to`, rounded to `to`'s price point ([`price_point`]).
    pub fn price_point(&self, amount: &Amount, to: &str) -> Result<Amount, CurrencyError> {
        self.convert(amount, to).map(|a| price_point(&a))
    }
}

/// `minor` units of `from` (whose rate is `from_per_eur_micro` millionths per euro) in the minor
/// unit of `to`, rounded half away from zero; none when a rate is not positive or the result does
/// not fit. The one conversion every caller uses, the books' as much as the price list's.
pub fn convert_minor(minor: i64, from: &str, from_per_eur_micro: i64, to: &str, to_per_eur_micro: i64) -> Option<i64> {
    if from_per_eur_micro <= 0 || to_per_eur_micro <= 0 {
        return None;
    }
    let scale_from = 10_i128.checked_pow(minor_digits(from))?;
    let scale_to = 10_i128.checked_pow(minor_digits(to))?;
    // minor / scale_from units of `from` are that / from_rate euros, which are that x to_rate
    // units of `to`
    let num = i128::from(minor)
        .checked_mul(scale_to)?
        .checked_mul(i128::from(to_per_eur_micro))?;
    let den = scale_from.checked_mul(i128::from(from_per_eur_micro))?;
    i64::try_from(div_half_away(num, den)?).ok()
}

/// `minor` units of `currency` as the decimal a provider's API takes (`34290` BRL is `342.90`,
/// `1100` JPY is `1100`): exact, never through a float.
pub fn decimal_text(minor: i64, currency: &str) -> String {
    let places = minor_digits(currency);
    if places == 0 {
        return minor.to_string();
    }
    let scale = 10_i64.checked_pow(places).unwrap_or(100);
    let sign = if minor < 0 { "-" } else { "" };
    let a = minor.unsigned_abs();
    let s = scale.unsigned_abs();
    format!("{sign}{}.{:0width$}", a / s, a % s, width = places as usize)
}

/// A provider's decimal amount (`342.9`, `"996.2143853357243"`) in the minor unit of `currency`,
/// rounded half away from zero past its decimals; none for anything but a plain decimal.
pub fn parse_decimal(text: &str, currency: &str) -> Option<i64> {
    let t = text.trim();
    let (negative, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, t),
    };
    let (whole, fraction) = t.split_once('.').unwrap_or((t, ""));
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || fraction.len() > 18 {
        return None;
    }
    let places = minor_digits(currency) as usize;
    let scale = 10_i128.checked_pow(u32::try_from(places).ok()?)?;
    let mut minor = whole.parse::<i128>().ok()?.checked_mul(scale)?;
    let kept: String = fraction.chars().take(places).collect();
    let kept = format!("{kept:0<places$}");
    if places > 0 {
        minor = minor.checked_add(kept.parse::<i128>().ok()?)?;
    }
    // the first digit past the currency's decimals rounds
    if fraction.chars().nth(places).is_some_and(|d| d >= '5') {
        minor = minor.checked_add(1)?;
    }
    let minor = i64::try_from(minor).ok()?;
    Some(if negative { -minor } else { minor })
}

/// `num / den` rounded half away from zero; `den` must be positive.
fn div_half_away(num: i128, den: i128) -> Option<i128> {
    if den <= 0 {
        return None;
    }
    let (q, r) = (num / den, num % den);
    if r.checked_abs()?.checked_mul(2)? >= den {
        Some(if num < 0 { q - 1 } else { q + 1 })
    } else {
        Some(q)
    }
}

/// How prices in a currency end: what people there are used to seeing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceStyle {
    /// Cents ending in 9: X.49 and X.99 from five units up, X.09 ... X.99 below (half a unit
    /// would move a price under five by a tenth or more). The euro's, the dollar's, the
    /// pound's, the zloty's custom, and every two-decimal currency without a rule of its own.
    Cents99,
    /// X.90 from ten units up, round tenths below: Swiss prices move in five-centime steps and
    /// end in .90 or .50, Brazilian, Peruvian, Chinese and Hong Kong prices in ,90 / .9.
    Cents90,
    /// Whole units ending in 9 at a step of a tenth of the price (99, 219, 1 099), and plain whole
    /// units under a hundred, where a 9-ending would move the price by a tenth or more: the
    /// Nordic crowns, the koruna, the forint, the krona, the Chilean peso, the rupee, the baht,
    /// the peso of Mexico, the rand and the other currencies priced in whole units.
    Whole9,
    /// Whole units at two significant digits (180, 1 500, 20 000): the yen, the won, the
    /// rupiah, the dong, the Colombian peso and the naira, whose units are small and whose prices
    /// are round.
    WholeRound,
}

/// The price rule of `currency`; a currency without its own rule follows its minor unit (whole
/// units round, cents end in 9).
pub fn price_style(currency: &str) -> PriceStyle {
    const CENTS_90: [&str; 5] = ["BRL", "CHF", "CNY", "HKD", "PEN"];
    const WHOLE_9: [&str; 18] = [
        "ARS", "CLP", "CZK", "DKK", "EGP", "HUF", "INR", "ISK", "KES", "MXN", "NOK", "PHP", "SEK", "THB", "TRY", "TWD",
        "UYU", "ZAR",
    ];
    const WHOLE_ROUND: [&str; 6] = ["COP", "IDR", "JPY", "KRW", "NGN", "VND"];
    if CENTS_90.contains(&currency) {
        PriceStyle::Cents90
    } else if WHOLE_9.contains(&currency) {
        PriceStyle::Whole9
    } else if WHOLE_ROUND.contains(&currency) || minor_digits(currency) == 0 {
        PriceStyle::WholeRound
    } else {
        PriceStyle::Cents99
    }
}

/// The price point nearest to `amount` under its currency's rule (a tie goes up); a price point
/// is its own price point. Nothing but positive amounts are prices: others come back unchanged.
pub fn price_point(amount: &Amount) -> Amount {
    let a = amount.minor;
    if a <= 0 {
        return amount.clone();
    }
    let unit = 10_i64.checked_pow(minor_digits(&amount.currency)).unwrap_or(100);
    let style = price_style(&amount.currency);
    // Each range of prices has its own grid (crowns: whole units under a hundred, 9-endings
    // above), and a price point is a grid point inside its own range: 100 crowns is none. The
    // nearest one may lie in the next range, so the grids of the ranges the neighbours fall in
    // are asked too (a range's edge is crossed at most once each way).
    let mut asked = vec![a];
    let mut next = vec![a];
    let mut found: Vec<i64> = Vec::new();
    while let Some(size) = next.pop() {
        let (below, above) = around(style, a, size, unit);
        for p in [below, above] {
            if p <= 0 {
                continue;
            }
            if around(style, p, p, unit).0 == p {
                found.push(p);
            } else if asked.len() < 4 && !asked.contains(&p) {
                asked.push(p);
                next.push(p);
            }
        }
    }
    // the nearest, a tie going up
    let minor = found
        .into_iter()
        .min_by_key(|p| ((p - a).abs(), std::cmp::Reverse(*p)))
        .unwrap_or(a);
    Amount::new(&amount.currency, minor)
}

/// The grid points of the range prices the size of `size` are in, at or below `a` and above
/// it: cents ending in 9 below five units and X.49 / X.99 above (Cents99), tenths below ten
/// units and X.90 above (Cents90), and so on.
fn around(style: PriceStyle, a: i64, size: i64, unit: i64) -> (i64, i64) {
    match style {
        PriceStyle::Cents99 => {
            let cent = (unit / 100).max(1);
            if size >= unit.saturating_mul(5) {
                grid(a, unit / 2, cent)
            } else {
                grid(a, unit / 10, cent)
            }
        }
        PriceStyle::Cents90 => {
            if size >= unit.saturating_mul(10) {
                grid(a, unit, unit / 10)
            } else {
                grid(a, (unit / 10).max(1), 0)
            }
        }
        PriceStyle::Whole9 => {
            let step = decade_step(size, unit);
            if step == unit {
                multiples(a, unit)
            } else {
                grid(a, step, unit)
            }
        }
        PriceStyle::WholeRound => multiples(a, decade_step(size, unit)),
    }
}

/// The values of the grid `k x step - end` (k = 1, 2, ...) at or below `a` and above it.
fn grid(a: i64, step: i64, end: i64) -> (i64, i64) {
    if step <= 0 {
        return (a, a);
    }
    let k = a.saturating_add(end) / step;
    let below = k.saturating_mul(step).saturating_sub(end);
    (below, below.saturating_add(step))
}

/// The multiples of `step` at or below `a` and above it.
fn multiples(a: i64, step: i64) -> (i64, i64) {
    if step <= 0 {
        return (a, a);
    }
    let down = a / step * step;
    (down, down.saturating_add(step))
}

/// The largest `unit x 10^j` that is at most a tenth of `a`; at least `unit`.
fn decade_step(a: i64, unit: i64) -> i64 {
    let mut step = unit.max(1);
    while let Some(next) = step.checked_mul(10) {
        match next.checked_mul(10) {
            Some(tenfold) if tenfold <= a => step = next,
            _ => break,
        }
    }
    step
}

/// `YYYY-MM-DD`.
fn date_ok(d: &str) -> bool {
    d.len() == 10
        && d.bytes()
            .enumerate()
            .all(|(i, b)| if i == 4 || i == 7 { b == b'-' } else { b.is_ascii_digit() })
}

/// One ECB rate as published: its value in millionths and how many decimals the ECB gave it (a
/// monthly mean is rounded back to them).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EcbQuote {
    pub per_eur_micro: i64,
    pub decimals: u32,
}

/// The rates of one day of an ECB file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EcbDay {
    /// `2026-10-16`.
    pub date: String,
    pub rates: BTreeMap<String, EcbQuote>,
}

/// The days of an ECB reference rates file: the daily one (`eurofxref-daily.xml`, one day) or a
/// history (`eurofxref-hist-90d.xml`, newest first), with either quote style. A rate that is not a
/// plain decimal is an error, never a gap. The work is linear in the text, whose size the caller
/// bounds.
pub fn parse_ecb(xml: &str) -> Result<Vec<EcbDay>, String> {
    let mut days: Vec<EcbDay> = Vec::new();
    for tag in xml.split("<Cube").skip(1) {
        let tag = tag.split('>').next().unwrap_or("");
        if let Some(date) = attr(tag, "time") {
            if !date_ok(date) {
                return Err(format!("the ECB file names the day `{date}`"));
            }
            days.push(EcbDay {
                date: date.to_string(),
                rates: BTreeMap::new(),
            });
            continue;
        }
        let (Some(code), Some(value)) = (attr(tag, "currency"), attr(tag, "rate")) else {
            continue;
        };
        if !currency_ok(code) {
            return Err(format!("the ECB file names the currency `{code}`"));
        }
        let quote = parse_rate(value).ok_or_else(|| format!("the ECB's {code} rate `{value}` is not a plain number"))?;
        let day = days
            .last_mut()
            .ok_or_else(|| format!("the ECB file gives {code} before any day"))?;
        day.rates.insert(code.to_string(), quote);
    }
    if days.iter().all(|d| d.rates.is_empty()) {
        return Err("no rates: this is not the ECB's reference rates file".to_string());
    }
    Ok(days)
}

/// The value of the attribute `name` of a tag's text, in single or double quotes.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=");
    for (at, _) in tag.match_indices(&pattern) {
        let before = tag.get(..at).and_then(|b| b.chars().last());
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = tag.get(at + pattern.len()..)?;
        let quote = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
        return rest.get(1..)?.split(quote).next();
    }
    None
}

/// A rate as people and the ECB write it: `4.3700` is 4_370_000 millionths with 4 decimals.
/// Nothing but digits and one dot, at most six decimals, above zero; anything else is none, so a
/// typo never becomes another rate.
pub fn parse_rate(s: &str) -> Option<EcbQuote> {
    let (whole, fraction) = s.split_once('.').unwrap_or((s, ""));
    let digits = |t: &str| t.bytes().all(|b| b.is_ascii_digit());
    if whole.is_empty() || !digits(whole) || !digits(fraction) || fraction.len() > 6 {
        return None;
    }
    if s.contains('.') && fraction.is_empty() {
        return None;
    }
    let decimals = u32::try_from(fraction.len()).ok()?;
    let whole: i64 = whole.parse().ok()?;
    let fraction: i64 = if fraction.is_empty() { 0 } else { fraction.parse().ok()? };
    let scale = 10_i64.checked_pow(6 - decimals)?;
    let micro = whole.checked_mul(MICRO)?.checked_add(fraction.checked_mul(scale)?)?;
    (micro > 0).then_some(EcbQuote {
        per_eur_micro: micro,
        decimals,
    })
}
