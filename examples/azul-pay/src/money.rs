//! The smallest unit of a currency (ISO 4217): how many decimals an amount in `Cents` has. The
//! pay path writes money through the app's formatter ([`azlin_contract::Cents`] is the amount and
//! its currency in one value); this table is for the one place that cannot use it - a slip's
//! amount in English words - and for the tests that hold the other copies of the table to it.

/// Currencies whose smallest unit is the whole unit (the yen, the won).
const NO_DECIMALS: [&str; 17] = [
    "BIF", "CLP", "DJF", "GNF", "ISK", "JPY", "KMF", "KRW", "MGA", "PYG", "RWF", "UGX", "VND",
    "VUV", "XAF", "XOF", "XPF",
];
/// Currencies whose smallest unit is a thousandth (the Gulf dinars).
const THREE_DECIMALS: [&str; 7] = ["BHD", "IQD", "JOD", "KWD", "LYD", "OMR", "TND"];

/// How many decimals `currency` (an ISO 4217 code in capitals) has: none for the yen or the won,
/// three for the Gulf dinars, two for the rest.
#[must_use]
pub fn minor_digits(currency: &str) -> u32 {
    if NO_DECIMALS.contains(&currency) {
        0
    } else if THREE_DECIMALS.contains(&currency) {
        3
    } else {
        2
    }
}
