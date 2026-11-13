//! The smallest unit of a currency (ISO 4217): how many decimals an amount in `Cents` has. The
//! pay path writes money through the app's formatter ([`azlin_contract::Cents`] is the amount and
//! its currency in one value); this is for the one place that cannot use it - a slip's amount in
//! English words. The table is the contract's: one, read by every crate (a copy here and another
//! in the methods registry had answers of their own).

/// How many decimals `currency` (an ISO 4217 code in capitals) has: none for the yen or the won,
/// three for the Gulf dinars, two for the rest - the contract's table, the one the Azlin apps and
/// services read ([`azlin_contract::minor_digits`]).
pub use azlin_contract::minor_digits;
