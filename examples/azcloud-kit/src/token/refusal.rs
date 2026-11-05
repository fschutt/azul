//! What a refusal of the token server says, read in ONE place (ARCH28 RF51 / AR5b-6).
//!
//! [`TokenError::Refused`] and [`TokenError::Busy`] keep the wire facts - the HTTP status, the
//! `error` word and the sentence - so a log shows what the server said. Whoever DECIDES on a
//! refusal asks [`TokenError::refusal`] for the contract's typed value and matches that: no
//! other file of the money path compares a status number or a code word (`money_gate` reads the
//! sources and fails when one does).
//!
//! A refusal this build cannot place - a word the contract does not name, a status with no word
//! at all (a proxy's 404, an edge page) - is [`Refusal::Unclassified`]: it means "ask again
//! later, change nothing". It is never "the checkout is gone", "not sold here" or "the token is
//! spent".
//!
//! A word two families of the contract share (`bad_request` of a redemption and of a proof)
//! reads as the family listed first in [`Refusal::decode`]; a decision that needs the family a
//! route has already got it typed from the route's own answer ([`super::IssueAnswer`], the
//! redeem and archive calls).

use azlin_contract::{ArchiveRefusal, AzlinError, CheckoutRefusal, ClaimRefusal, RedeemRefusal};
use serde::de::DeserializeOwned;

use super::TokenError;

/// A refusal of the token server, as the contract types it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// An order, the tier list, the payment options, a surface, an abandon or the poll of a
    /// checkout.
    Checkout(CheckoutRefusal),
    /// A redemption of a period token.
    Redeem(RedeemRefusal),
    /// A locked drive's proof.
    Archive(ArchiveRefusal),
    /// A code the nodes and the token server share (a ban, a lockdown, the last recovery key).
    Service(AzlinError),
    /// The pick-up of a paid drive by its claim ticket.
    Claim(ClaimRefusal),
    /// A word the contract does not name, or none: `status` and the word as the server wrote
    /// it (empty when its answer carried none).
    Unclassified { status: u16, code: String },
}

/// `word` as the family `T` of the contract reads it (`{"error": word}`), `None` for a word that
/// is no variant with that shape, or the family's catch-all.
fn family<T: DeserializeOwned + PartialEq>(word: &str, catch_all: &T) -> Option<T> {
    if word.is_empty() {
        return None;
    }
    let read: T = serde_json::from_value(serde_json::json!({ "error": word })).ok()?;
    (read != *catch_all).then_some(read)
}

impl Refusal {
    /// The refusal the server's `status` and `code` word say. Families in this order: the
    /// pick-up's, the codes nodes and token server share, a checkout's, a redemption's, a
    /// proof's; anything else is [`Refusal::Unclassified`].
    #[must_use]
    pub fn decode(status: u16, code: &str) -> Refusal {
        let word = code.trim();
        if let Some(claim) = ClaimRefusal::parse(word) {
            return Refusal::Claim(claim);
        }
        if let Some(shared) = AzlinError::parse(word) {
            return Refusal::Service(shared);
        }
        if let Some(checkout) = CheckoutRefusal::parse(word) {
            return Refusal::Checkout(checkout);
        }
        if let Some(redeem) = family(word, &RedeemRefusal::Unknown) {
            return Refusal::Redeem(redeem);
        }
        if let Some(archive) = family(word, &ArchiveRefusal::Unknown) {
            return Refusal::Archive(archive);
        }
        Refusal::Unclassified {
            status,
            code: word.to_string(),
        }
    }

    /// The word as the server wrote it (empty for an answer that carried none).
    #[must_use]
    pub fn word(&self) -> &str {
        match self {
            Refusal::Checkout(r) => r.as_str(),
            Refusal::Redeem(r) => r.as_str(),
            Refusal::Archive(r) => r.as_str(),
            Refusal::Service(r) => r.as_str(),
            Refusal::Claim(r) => r.as_str(),
            Refusal::Unclassified { code, .. } => code,
        }
    }

    /// The token server has no such checkout, in the contract's word: the only refusal after
    /// which a checkout is gone. A bare 404, a 404 with another word and an HTML page are not.
    #[must_use]
    pub fn is_no_such_checkout(&self) -> bool {
        matches!(self, Refusal::Checkout(CheckoutRefusal::NoSuchCheckout))
    }

    /// The checkout was paid: it cannot be abandoned, its drive comes.
    #[must_use]
    pub fn is_checkout_paid(&self) -> bool {
        matches!(self, Refusal::Checkout(CheckoutRefusal::CheckoutPaid))
    }

    /// Azlin Storage is not sold to the buyer's country, in the contract's word: the only
    /// refusal that says so (a 403 of an edge or a firewall does not).
    #[must_use]
    pub fn is_blocked_country(&self) -> bool {
        matches!(self, Refusal::Checkout(CheckoutRefusal::BlockedCountry))
    }

    /// A renewal's price lock was refused (used, not this checkout's, another tier, none, not
    /// paid): the order goes on without the lock, at today's price.
    #[must_use]
    pub fn is_lock_refused(&self) -> bool {
        matches!(
            self,
            Refusal::Checkout(
                CheckoutRefusal::LockUsed
                    | CheckoutRefusal::LockKeyWrong
                    | CheckoutRefusal::LockTier
                    | CheckoutRefusal::NoLock
                    | CheckoutRefusal::NotPaid
            )
        )
    }

    /// A redemption's refusal of a token that can never buy a month
    /// ([`RedeemRefusal::is_useless`]).
    #[must_use]
    pub fn is_useless_token(&self) -> bool {
        matches!(self, Refusal::Redeem(r) if r.is_useless())
    }

    /// A useless-token refusal that may be the SERVER's fault, not the token's: it lost an
    /// issuer key (`unknown_issuer`) or verifies with another one (`bad_token`), and then says
    /// so of every token it is asked about.
    #[must_use]
    pub fn is_doubtful_token(&self) -> bool {
        matches!(
            self,
            Refusal::Redeem(RedeemRefusal::UnknownIssuer | RedeemRefusal::BadToken)
        )
    }

    /// The drive is banned, in the contract's word `drive_banned` (403): past the ban's end the
    /// token server refuses the drive's token for everything, a redemption included.
    #[must_use]
    pub fn is_drive_banned(&self) -> bool {
        matches!(self, Refusal::Service(AzlinError::DriveBanned))
    }

    /// A recovery-key lockdown of the drive is pending, in the contract's word
    /// `lockdown_pending` (403): the drive's tokens wait for its end or its cancel.
    #[must_use]
    pub fn is_lockdown_pending(&self) -> bool {
        matches!(self, Refusal::Service(AzlinError::LockdownPending))
    }

    /// The pick-up's refusal, when it is one.
    #[must_use]
    pub fn claim(&self) -> Option<ClaimRefusal> {
        match self {
            Refusal::Claim(claim) => Some(*claim),
            _ => None,
        }
    }

    /// The answer of a route the server does not have: a 404 without a word of the contract.
    #[must_use]
    pub fn is_route_missing(&self) -> bool {
        matches!(self, Refusal::Unclassified { status: 404, .. })
    }

    /// The server does not know the credential the call sent and says no word of the contract
    /// (401): the pick-up of a drive from before the claim tickets answers so.
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, Refusal::Unclassified { status: 401, .. })
    }
}

impl TokenError {
    /// What the server refused, typed. `Some` for the two errors that carry its answer
    /// ([`TokenError::Refused`] and [`TokenError::Busy`]); `None` for no answer, a sign-in that
    /// is needed, a protocol fault and a missing configuration.
    #[must_use]
    pub fn refusal(&self) -> Option<Refusal> {
        match self {
            TokenError::Refused { status, code, .. } | TokenError::Busy { status, code, .. } => {
                Some(Refusal::decode(*status, code))
            }
            TokenError::Connect(_)
            | TokenError::SignIn(_)
            | TokenError::Protocol(_)
            | TokenError::Config(_) => None,
        }
    }

    /// The token server has no such checkout, in the contract's word ([`Refusal::is_no_such_checkout`]).
    #[must_use]
    pub fn is_no_such_checkout(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_no_such_checkout())
    }

    /// Azlin Storage is not sold to the buyer's country ([`Refusal::is_blocked_country`]).
    #[must_use]
    pub fn is_blocked_country(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_blocked_country())
    }

    /// A renewal's price lock was refused ([`Refusal::is_lock_refused`]).
    #[must_use]
    pub fn is_lock_refused(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_lock_refused())
    }

    /// The checkout was paid meanwhile ([`Refusal::is_checkout_paid`]).
    #[must_use]
    pub fn is_checkout_paid(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_checkout_paid())
    }

    /// A redemption's refusal of a token that can never buy a month ([`Refusal::is_useless_token`]).
    #[must_use]
    pub fn is_useless_token(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_useless_token())
    }

    /// A useless-token refusal that may be the server's fault ([`Refusal::is_doubtful_token`]).
    #[must_use]
    pub fn is_doubtful_token(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_doubtful_token())
    }

    /// The drive is banned ([`Refusal::is_drive_banned`]).
    #[must_use]
    pub fn is_drive_banned(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_drive_banned())
    }

    /// A recovery-key lockdown is pending ([`Refusal::is_lockdown_pending`]).
    #[must_use]
    pub fn is_lockdown_pending(&self) -> bool {
        self.refusal().is_some_and(|r| r.is_lockdown_pending())
    }

    /// The pick-up's refusal, when this is one.
    #[must_use]
    pub fn claim_refusal(&self) -> Option<ClaimRefusal> {
        self.refusal().and_then(|r| r.claim())
    }

    /// A credential the call sent is not known to the server ([`Refusal::is_unauthorized`]).
    #[must_use]
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, TokenError::Refused { .. })
            && self.refusal().is_some_and(|r| r.is_unauthorized())
    }

    /// What the server said, for a sentence: its own sentence, else its word, else the error as
    /// it prints.
    #[must_use]
    pub fn said(&self) -> String {
        match self {
            TokenError::Refused { code, message, .. } | TokenError::Busy { code, message, .. } => {
                let sentence = message.trim();
                if !sentence.is_empty() {
                    sentence.to_string()
                } else if !code.trim().is_empty() {
                    code.trim().to_string()
                } else {
                    self.to_string()
                }
            }
            other => other.to_string(),
        }
    }

    /// The token server does not have the route: it answered 404 without a word of the
    /// contract (a bare `not_found`, a proxy's page). A checkout it does not know is
    /// [`TokenError::is_no_such_checkout`], not this.
    #[must_use]
    pub fn is_route_missing(&self) -> bool {
        matches!(self, TokenError::Refused { .. })
            && self.refusal().is_some_and(|r| r.is_route_missing())
    }

    /// The token server sells drives through a checkout only: it has no development sign-up
    /// (`POST /v1/drives` is a route it does not have).
    #[must_use]
    pub fn is_checkout_only(&self) -> bool {
        self.is_route_missing()
    }
}
