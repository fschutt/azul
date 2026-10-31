//! The documented test IBANs both bank-debit providers take here, and what each one does
//! (Stripe's list, https://docs.stripe.com/testing, read 2026-10-17; only the entries whose
//! check digits are right, plus the two older German ones Stripe's earlier list named). Any
//! other IBAN, even a real and valid one, is refused before anything happens.

/// What a debit from a test IBAN comes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Debit {
    /// Collected.
    Ok,
    /// Collected after a longer wait.
    Slow,
    /// Returned by the bank, with Stripe's decline code.
    Fail(&'static str),
    /// Collected, then disputed by the payer.
    Dispute,
}

pub const IBANS: &[(&str, Debit)] = &[
    ("DE89370400440532013000", Debit::Ok),
    ("DE91100000000123456789", Debit::Fail("generic_decline")),
    ("DE62370400440532013001", Debit::Fail("generic_decline")),
    ("DE35370400440532013002", Debit::Dispute),
    ("AT611904300234573201", Debit::Ok),
    ("AT321904300235473204", Debit::Slow),
    ("AT861904300235473202", Debit::Fail("generic_decline")),
    ("AT051904300235473205", Debit::Fail("generic_decline")),
    ("AT201904300000454545", Debit::Fail("debit_not_authorized")),
    ("AT591904300235473203", Debit::Dispute),
    ("AT981904300002222227", Debit::Fail("insufficient_funds")),
];

/// The message for an IBAN that is not one of [`IBANS`].
pub const TEST_IBANS_ONLY: &str = "This sandbox only accepts test IBANs, for example DE89 3704 0044 0532 0130 00.";

/// What `iban` (compact) does; none for an IBAN that is not a test IBAN.
pub fn debit_of(iban: &str) -> Option<Debit> {
    IBANS.iter().find(|(i, _)| *i == iban).map(|(_, d)| *d)
}

/// Why an IBAN the payer typed cannot be used, in the words a payment page shows; none for a
/// test IBAN. The checksum comes first, as a real form checks it while typing.
pub fn iban_problem(compact: &str) -> Option<&'static str> {
    if compact.len() < 15 {
        return Some("Your IBAN is incomplete.");
    }
    if !crate::util::iban_checksum_ok(compact) {
        return Some("Your IBAN is invalid.");
    }
    if debit_of(compact).is_none() {
        return Some(TEST_IBANS_ONLY);
    }
    None
}
