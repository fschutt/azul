//! Buying a drive: the tiers on sale (`GET /v1/tiers`), where a checkout stands
//! (`GET /v1/checkout/{id}`), its abandon (`POST /v1/checkout/{id}/abandon`) and how the token
//! server refuses an order. Every enum the apps read has an `Unknown` for a newer server's
//! word: the apps take it neither as a success nor as an end, and ask for an update.

use std::{collections::BTreeMap, fmt};

use serde::{Deserialize, Serialize};

use crate::{CheckoutId, Currency, InvalidValue, Months, Rfc3339, StoredBytes, TierId};

/// The periods a tier is sold for at once, shortest first, each once. Never empty: a tier
/// that can be bought for no period is not on sale.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "Vec<Months>", into = "Vec<Months>")]
pub struct Prepay(Vec<Months>);

impl Prepay {
    /// `months` sorted, each once.
    ///
    /// # Errors
    ///
    /// No period at all.
    pub fn new(mut months: Vec<Months>) -> Result<Prepay, InvalidValue> {
        months.sort();
        months.dedup();
        if months.is_empty() {
            return Err(InvalidValue::new("list of prepaid periods", "[]"));
        }
        Ok(Prepay(months))
    }

    /// The periods, shortest first.
    #[must_use]
    pub fn months(&self) -> &[Months] {
        &self.0
    }

    #[must_use]
    pub fn contains(&self, months: Months) -> bool {
        self.0.contains(&months)
    }

    /// The shortest period it is sold for.
    #[must_use]
    pub fn shortest(&self) -> Months {
        self.0[0]
    }
}

impl TryFrom<Vec<Months>> for Prepay {
    type Error = InvalidValue;
    fn try_from(months: Vec<Months>) -> Result<Prepay, InvalidValue> {
        Prepay::new(months)
    }
}

impl From<Prepay> for Vec<Months> {
    fn from(prepay: Prepay) -> Vec<Months> {
        prepay.0
    }
}

/// A tier's price in one currency, in its smallest unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TierPriceWire {
    /// A month paid monthly; 0 for a tier sold only by the year.
    pub month_cents: u64,
    /// A year paid at once.
    pub year_cents: u64,
}

/// Why a tier is not sold right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TierClosed {
    /// Its full quota does not fit what is left of the capacity.
    CapacityFull,
    /// The capacity is not known: nothing is sold blind.
    CapacityUnknown,
    /// The price ladder has no such tier.
    NotSold,
    /// Its period tokens cannot be issued yet.
    NotReady,
    #[serde(other)]
    Unknown,
}

/// One tier of `GET /v1/tiers`, priced for the buyer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierOffer {
    pub id: TierId,
    /// What a drive of the tier may keep.
    #[serde(rename = "quota_bytes")]
    pub quota: StoredBytes,
    /// A month paid monthly; `None` for a tier sold only by the year, or without a price.
    pub price_cents_month: Option<u64>,
    /// A year paid at once.
    pub price_cents_year: Option<u64>,
    /// The currency the two prices are in.
    pub currency: Currency,
    /// Its price in every currency the price ladder has for it.
    #[serde(default)]
    pub prices: BTreeMap<Currency, TierPriceWire>,
    /// Sold for 12 or 24 months only: a month at a time is refused.
    #[serde(default)]
    pub yearly_only: bool,
    /// The payment methods it is sold with (`sepa`, `card`, `cash`, ...).
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub first_month_free: bool,
    /// The periods a checkout of it may pay at once.
    #[serde(rename = "prepay_months")]
    pub prepay: Prepay,
    /// A tier of the answer's `unavailable` list: why it is closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<TierClosed>,
    /// A closed tier takes names for a list of who to tell when it opens again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub waitlist: bool,
    /// A closed tier's reason as a word (`temporarily_unavailable`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl TierOffer {
    /// Whether a checkout may pay it with `method` (a tier naming no methods takes every one).
    #[must_use]
    pub fn takes(&self, method: &str) -> bool {
        self.methods.is_empty() || self.methods.iter().any(|m| m.trim() == method.trim())
    }
}

/// A retention add-on: deleted files kept longer, for a price per TB and month.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionAddon {
    pub id: String,
    pub days: u32,
    pub cents_per_tb_month: u64,
}

/// The legal texts the order screen shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Legal {
    /// The sentence a buyer agrees to: the service starts at once, and with it the right of
    /// withdrawal ends.
    #[serde(default)]
    pub withdrawal_consent: Option<String>,
    /// What a direct debit's buyer is told before the first debit.
    #[serde(default)]
    pub sepa_prenotification: Option<String>,
    /// Small amounts get a simplified invoice.
    #[serde(default)]
    pub small_amount_invoice: bool,
}

/// An announced price change: the tiers it moves, to which prices (`None`: the tier's price
/// ends), and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledChange {
    pub id: u64,
    pub from_version: u64,
    pub to_version: u64,
    pub announced_at: Rfc3339,
    pub effective_at: Rfc3339,
    pub prices: BTreeMap<TierId, Option<BTreeMap<Currency, TierPriceWire>>>,
    #[serde(default)]
    pub note: String,
}

/// The price ladder the tiers come from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderRef {
    /// The signed version a checkout records (its renewals keep its price).
    pub version: u64,
    pub currency: Currency,
    pub active_from: Option<Rfc3339>,
}

/// `GET /v1/tiers?country=`: what is on sale for a buyer of that country.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TiersAnswer {
    /// The tiers on sale, in the order they are shown.
    pub tiers: Vec<TierOffer>,
    /// The tiers closed for now.
    #[serde(default)]
    pub unavailable: Vec<TierOffer>,
    /// Why nothing at all is on sale (`no_price_ladder`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<String>,
    /// The payment methods the token server takes at all (`cash` only where it takes cash by
    /// post).
    #[serde(default)]
    pub methods: Vec<String>,
    #[serde(default)]
    pub retention_addons: Vec<RetentionAddon>,
    #[serde(default)]
    pub legal: Legal,
    #[serde(default)]
    pub scheduled_changes: Vec<ScheduledChange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ladder: Option<LadderRef>,
}

/// Why the last attempt to pay a checkout failed, in the words every provider shares.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeclineWire {
    /// The shared word (`insufficient_funds`).
    pub reason: Option<String>,
    /// The provider's own code.
    pub code: Option<String>,
    /// The provider's sentence.
    pub text: Option<String>,
}

/// Where cash by post goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailTo {
    pub name: String,
    #[serde(default)]
    pub lines: Vec<String>,
}

/// Where a checkout stands: its `status` and what goes with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CheckoutStatusWire {
    /// Not paid yet; the payer may still try (why the last try failed, if one did).
    Pending {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decline: Option<DeclineWire>,
    },
    /// A cash checkout waiting for its letter: its code and the address again, to print anew.
    AwaitingCash {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        activation_code: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mail_to: Option<MailTo>,
    },
    /// Paid. `sealed_signup` is the sign-up sealed to the checkout's claim key, kept 30 days
    /// after the payment; `None` once that hand-over is gone (or for a checkout from before the
    /// claim keys): the app cannot get that drive any more.
    Approved {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sealed_signup: Option<String>,
    },
    /// The payment was declined: no money, no drive.
    Declined {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decline: Option<DeclineWire>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Paid, then the money went back (a returned debit, a chargeback).
    Reversed {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// The operator did not take a letter's cash (the amount, the slip).
    Rejected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Nobody can pay it any more (abandoned, or past its end).
    Expired {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// A status this build does not know: asked again, the app says to update it.
    #[serde(other)]
    Unknown,
}

/// How fast a provider checkout's payment settles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Settles {
    Instant,
    /// A direct debit: days.
    Days,
    #[serde(other)]
    Unknown,
}

/// The receipt of a booked sale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptWire {
    pub issue_day: String,
    pub service: String,
    pub gross_cents: i64,
    pub currency: Currency,
    pub rate_permille: i64,
    pub country: String,
    pub scheme: String,
    pub checkout_id: CheckoutId,
    /// `small_amount_invoice` or `payment_receipt`.
    pub kind: String,
}

/// `GET /v1/checkout/{id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutAnswer {
    pub checkout_id: CheckoutId,
    #[serde(flatten)]
    pub status: CheckoutStatusWire,
    /// The tier bought (empty for a checkout that names none yet).
    #[serde(default)]
    pub tier: String,
    /// The months it pays for (a monthly plan's grow as they are paid).
    #[serde(default)]
    pub months: u32,
    #[serde(default)]
    pub amount_cents: u64,
    /// A cash checkout's end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Rfc3339>,
    /// A provider checkout: the provider's id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settles: Option<Settles>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub months_paid: Option<u32>,
    /// A monthly plan that is still running.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recurring: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub receipt: Option<ReceiptWire>,
}

/// What `POST /v1/checkout/{id}/abandon` found. A checkout that was paid meanwhile is no
/// answer but the refusal [`CheckoutRefusal::CheckoutPaid`]: an app must never read it as
/// abandoned, or it throws away the key its drive is sealed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
#[must_use]
pub enum AbandonAnswer {
    /// Its payment session expired: nobody can pay it any more.
    Expired,
    /// It waits for money the token server cannot stop (a letter of cash in the post).
    #[serde(rename = "pending")]
    StillPending,
    /// Over already without a drive.
    Declined,
    Reversed,
    Rejected,
    #[serde(other)]
    Unknown,
}

impl AbandonAnswer {
    /// Whether the checkout is over: nothing can bring its drive any more, so its claim key
    /// may go.
    #[must_use]
    pub fn is_over(self) -> bool {
        matches!(
            self,
            AbandonAnswer::Expired
                | AbandonAnswer::Declined
                | AbandonAnswer::Reversed
                | AbandonAnswer::Rejected
        )
    }
}

/// The answer of an abandon, with the checkout it is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbandonWire {
    pub checkout_id: CheckoutId,
    #[serde(flatten)]
    pub answer: AbandonAnswer,
}

/// How the token server refuses an order, a checkout's surface or its abandon: the `error` of
/// its answer (a `message` in English goes with it; the apps show their own text per variant).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum CheckoutRefusal {
    /// Azlin Storage is not sold to the buyer's country.
    BlockedCountry,
    /// The tier is not sold for that period: `allowed` are the periods it is.
    BadMonths {
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        allowed: Vec<Months>,
    },
    /// Not with that payment method.
    BadMethod,
    BadTier,
    BadCurrency,
    /// The tier is closed for now.
    TierUnavailable {
        #[serde(default)]
        waitlist: bool,
    },
    /// The method needs details of the payer the order did not carry.
    PayerRequired,
    PayerUnexpected,
    PayerInvalid,
    ConsentRequired,
    BadProvider,
    SurfaceUnavailable,
    /// The payment provider did not answer: nothing changed, ask again.
    ProviderUnavailable,
    /// Nothing is for sale right now.
    NoPriceLadder,
    /// The tier's period tokens cannot be issued yet.
    IssuerKeyMissing,
    CashUnavailable,
    ClaimKeyRequired,
    /// A renewal's price lock: used, or not this checkout's.
    LockUsed,
    LockKeyWrong,
    LockTier,
    NoLock,
    NotPaid,
    TierEnded,
    CurrencyEnded,
    /// The checkout is over: no other surface for it.
    CheckoutClosed,
    /// The checkout was paid: its drive comes, it cannot be abandoned.
    CheckoutPaid,
    NoSuchCheckout,
    /// A refusal this build does not know.
    #[serde(other)]
    Unknown,
}

impl CheckoutRefusal {
    /// Every variant without its details, in the order above.
    pub const ALL: [CheckoutRefusal; 28] = [
        CheckoutRefusal::BlockedCountry,
        CheckoutRefusal::BadMonths {
            allowed: Vec::new(),
        },
        CheckoutRefusal::BadMethod,
        CheckoutRefusal::BadTier,
        CheckoutRefusal::BadCurrency,
        CheckoutRefusal::TierUnavailable { waitlist: false },
        CheckoutRefusal::PayerRequired,
        CheckoutRefusal::PayerUnexpected,
        CheckoutRefusal::PayerInvalid,
        CheckoutRefusal::ConsentRequired,
        CheckoutRefusal::BadProvider,
        CheckoutRefusal::SurfaceUnavailable,
        CheckoutRefusal::ProviderUnavailable,
        CheckoutRefusal::NoPriceLadder,
        CheckoutRefusal::IssuerKeyMissing,
        CheckoutRefusal::CashUnavailable,
        CheckoutRefusal::ClaimKeyRequired,
        CheckoutRefusal::LockUsed,
        CheckoutRefusal::LockKeyWrong,
        CheckoutRefusal::LockTier,
        CheckoutRefusal::NoLock,
        CheckoutRefusal::NotPaid,
        CheckoutRefusal::TierEnded,
        CheckoutRefusal::CurrencyEnded,
        CheckoutRefusal::CheckoutClosed,
        CheckoutRefusal::CheckoutPaid,
        CheckoutRefusal::NoSuchCheckout,
        CheckoutRefusal::Unknown,
    ];

    /// The `error` word (`blocked_country`).
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            CheckoutRefusal::BlockedCountry => "blocked_country",
            CheckoutRefusal::BadMonths { .. } => "bad_months",
            CheckoutRefusal::BadMethod => "bad_method",
            CheckoutRefusal::BadTier => "bad_tier",
            CheckoutRefusal::BadCurrency => "bad_currency",
            CheckoutRefusal::TierUnavailable { .. } => "tier_unavailable",
            CheckoutRefusal::PayerRequired => "payer_required",
            CheckoutRefusal::PayerUnexpected => "payer_unexpected",
            CheckoutRefusal::PayerInvalid => "payer_invalid",
            CheckoutRefusal::ConsentRequired => "consent_required",
            CheckoutRefusal::BadProvider => "bad_provider",
            CheckoutRefusal::SurfaceUnavailable => "surface_unavailable",
            CheckoutRefusal::ProviderUnavailable => "provider_unavailable",
            CheckoutRefusal::NoPriceLadder => "no_price_ladder",
            CheckoutRefusal::IssuerKeyMissing => "issuer_key_missing",
            CheckoutRefusal::CashUnavailable => "cash_unavailable",
            CheckoutRefusal::ClaimKeyRequired => "claim_key_required",
            CheckoutRefusal::LockUsed => "lock_used",
            CheckoutRefusal::LockKeyWrong => "lock_key_wrong",
            CheckoutRefusal::LockTier => "lock_tier",
            CheckoutRefusal::NoLock => "no_lock",
            CheckoutRefusal::NotPaid => "not_paid",
            CheckoutRefusal::TierEnded => "tier_ended",
            CheckoutRefusal::CurrencyEnded => "currency_ended",
            CheckoutRefusal::CheckoutClosed => "checkout_closed",
            CheckoutRefusal::CheckoutPaid => "checkout_paid",
            CheckoutRefusal::NoSuchCheckout => "no_such_checkout",
            CheckoutRefusal::Unknown => "unknown",
        }
    }

    /// The refusal the word `code` names, without its details; `None` for one this build does
    /// not know.
    #[must_use]
    pub fn parse(code: &str) -> Option<CheckoutRefusal> {
        let code = code.trim();
        CheckoutRefusal::ALL
            .into_iter()
            .find(|r| *r != CheckoutRefusal::Unknown && r.as_str() == code)
    }

    /// The HTTP status the token server answers it with.
    #[must_use]
    pub fn status(&self) -> u16 {
        match self {
            CheckoutRefusal::BlockedCountry | CheckoutRefusal::LockKeyWrong => 403,
            CheckoutRefusal::NoSuchCheckout => 404,
            CheckoutRefusal::TierUnavailable { .. }
            | CheckoutRefusal::LockUsed
            | CheckoutRefusal::NoLock
            | CheckoutRefusal::NotPaid
            | CheckoutRefusal::TierEnded
            | CheckoutRefusal::CurrencyEnded
            | CheckoutRefusal::CheckoutClosed
            | CheckoutRefusal::CheckoutPaid => 409,
            CheckoutRefusal::ProviderUnavailable => 502,
            CheckoutRefusal::NoPriceLadder | CheckoutRefusal::IssuerKeyMissing => 503,
            _ => 400,
        }
    }

    /// Whether the same order may work when asked again unchanged (the provider or the price
    /// ladder was missing for a moment).
    #[must_use]
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            CheckoutRefusal::ProviderUnavailable
                | CheckoutRefusal::NoPriceLadder
                | CheckoutRefusal::IssuerKeyMissing
                | CheckoutRefusal::TierUnavailable { .. }
        )
    }
}

impl fmt::Display for CheckoutRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
