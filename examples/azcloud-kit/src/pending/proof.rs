//! The proofs that take a checkout off the keyring's list (ARCH28 RF51 / AR5b-6).
//!
//! A checkout's claim secret is the only key to the drive it pays for, and its issue key the only
//! key to the months that drive is still owed. A checkout therefore leaves the list only through
//! [`super::settle`], with a [`Settled`] - and a [`Settled`] is made here and nowhere else, from
//! a piece of EVIDENCE that has no public constructor either:
//!
//! - [`NoSuchCheckout`]: the token server answered the contract's `no_such_checkout` - not a bare
//!   404, not an HTML page, not a 404 of another word;
//! - [`ClaimsEnded`]: it refused the pick-up of the drive for good (`claims_used`,
//!   `claim_expired`);
//! - [`Strikes`]: the sign-up did not OPEN with the claim key [`super::UNOPENABLE_AFTER`] times -
//!   an AEAD failure; a sign-up that opens and does not read, or an answer that is not a sealed
//!   text, is never a strike (the sealed text is intact for 30 days: update the app, not erase
//!   its key);
//! - [`GrantKept`]: the period grant is in two places ([`crate::vault::Kept::is_redundant`]) -
//!   the drive's own vault has it, or a second copy was written here;
//! - [`GrantKeptEarlier`]: the same, said by the entry itself ([`ReceiptDue::GrantKept`], written
//!   on a [`GrantKept`] only) - what a checkout that waited for its receipt only is ended with;
//! - [`NeverIssued`]: the token server will never issue the rest (a final refusal, or a grant no
//!   server issues);
//! - [`NotMade`]: an order whose entry was made before its `POST /v1/checkout` (state `Posting`)
//!   and that made no checkout anyone was shown - the token server refused it, or a day is gone
//!   without an answer;
//! - [`HandoverOver`]: a checkout that was paid and then TAKEN BACK stayed unclaimed for as long
//!   as the token server keeps the sealed sign-up of a paid checkout - a payment that is put
//!   back after that brings no drive anyone's claim key could open (X-DELTA-MONEY5-1);
//! - a poll's answer that ends the checkout ([`Settled::ended_by`]): declined, expired, gone, the
//!   hand-over purged - NOT a payment taken back, which the token server puts back when its
//!   refund failed or a dispute was won;
//! - an abandon's answer that ended it (`Settled::abandoned`, called by [`super::abandon`] only).
//!
//! `money_gate` reads the sources: `Settled(` and `SettledBy::` stand in this file only.

use azlin_contract::AbandonAnswer;

use super::{Abandoned, Final, PendingCheckout, ReceiptDue, POSTING_LIFE_SECS, UNOPENABLE_AFTER};
use crate::{
    period::Issued,
    token::{CheckoutStatus, TokenError},
    vault::Kept,
};

/// What the token server said that ends a checkout: the only thing that takes one off the list
/// ([`super::settle`]). Made from its answers in this file, never from a guess.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settled(SettledBy);

#[derive(Debug, Clone, PartialEq, Eq)]
enum SettledBy {
    /// Its drive is in the drives file and nothing more is owed for it.
    Claimed,
    /// Its period grant is in two places.
    Finished,
    /// Its period grant was in two places before, and its receipt is kept - or the wait for it is
    /// over.
    Receipted,
    /// The token server will never issue the rest of its grant.
    NeverIssued,
    /// The payment did not go through (declined, the cash refused).
    Declined,
    /// The token server no longer has it, or its drive's hand-over is gone.
    Gone,
    /// It was taken back and not put back while the token server kept its hand-over.
    HandoverOver,
    /// The token server refused the drive's pick-up for good.
    ClaimsEnded,
    /// Its abandon ended it at the token server.
    Abandoned(AbandonAnswer),
    /// Its sealed sign-up did not open with its claim key, [`UNOPENABLE_AFTER`] times.
    Unopenable,
    /// The order made no checkout anyone was shown.
    NotMade,
}

/// The token server answered the contract's `no_such_checkout`.
#[derive(Debug)]
pub struct NoSuchCheckout(());

impl NoSuchCheckout {
    /// The word of `e`, when it is that one.
    pub(super) fn said_by(e: &TokenError) -> Option<NoSuchCheckout> {
        e.is_no_such_checkout().then_some(NoSuchCheckout(()))
    }
}

/// The token server refused the pick-up of a drive for good.
#[derive(Debug)]
pub struct ClaimsEnded(());

impl ClaimsEnded {
    /// The refusal of `e`, when it is one that cannot change.
    pub(super) fn said_by(e: &TokenError) -> Option<ClaimsEnded> {
        e.claim_refusal()
            .is_some_and(|refusal| refusal.is_final())
            .then_some(ClaimsEnded(()))
    }
}

/// A checkout's sealed sign-up did not open with its claim key often enough to give it up.
#[derive(Debug)]
pub struct Strikes(());

impl Strikes {
    /// What the list counted of `checkout`: [`UNOPENABLE_AFTER`] failures to open, no fewer.
    pub(super) fn of(checkout: &PendingCheckout) -> Option<Strikes> {
        (checkout.open_failures >= UNOPENABLE_AFTER).then_some(Strikes(()))
    }
}

/// A period grant is in two places: the drive's vault and this computer's, or this computer's
/// two copies.
#[derive(Debug)]
pub struct GrantKept(());

impl GrantKept {
    /// Where a join landed, when that is two places.
    pub(super) fn of(kept: &Kept) -> Option<GrantKept> {
        kept.is_redundant().then_some(GrantKept(()))
    }
}

/// The period grant of a claimed checkout was kept in two places by an earlier [`super::finish`],
/// and its entry has waited for its receipt only since ([`ReceiptDue::GrantKept`]).
#[derive(Debug)]
pub struct GrantKeptEarlier(());

impl GrantKeptEarlier {
    /// What the entry of `checkout` says: a claimed one, whose grant is kept.
    pub(super) fn of(checkout: &PendingCheckout) -> Option<GrantKeptEarlier> {
        (checkout.receipt_due == Some(ReceiptDue::GrantKept) && checkout.period.is_some())
            .then_some(GrantKeptEarlier(()))
    }
}

/// An unclaimed checkout whose payment was taken back stayed taken back for the whole life of the
/// hand-over the token server keeps for it.
#[derive(Debug)]
pub struct HandoverOver(());

impl HandoverOver {
    /// What the entry's own count says at `now` (seconds since 1970, this computer's clock):
    /// `checkout` is not claimed - an entry with a period is [`super::finish`]'s, whose claim
    /// secret is dead and whose issue key is a paid month's - and its wait since the first look
    /// that found the payment taken back, counted in the time this computer saw pass
    /// ([`super::TakenBack`]), has reached [`PendingCheckout::handover_life_secs`]. That first
    /// look is not earlier than the payment, so it is not earlier than the hand-over's own start
    /// either: the token server has purged the hand-over by then (X-DELTA-MONEY5-1).
    pub(super) fn of(checkout: &PendingCheckout, now: i64) -> Option<HandoverOver> {
        let taken_back = checkout.taken_back.as_ref()?;
        (checkout.period.is_none()
            && now.saturating_sub(taken_back.since) >= checkout.handover_life_secs())
        .then_some(HandoverOver(()))
    }
}

/// The token server will never issue the rest of a grant.
#[derive(Debug)]
pub struct NeverIssued(());

impl NeverIssued {
    /// A grant no token server issues: no month, more months than one issue takes, or no issue
    /// key (`most`: the most months one issue takes).
    pub(super) fn unusable_grant(months: u32, issue_key: &str, most: usize) -> Option<NeverIssued> {
        let months_ok = usize::try_from(months).is_ok_and(|m| (1..=most).contains(&m));
        (!months_ok || issue_key.trim().is_empty()).then_some(NeverIssued(()))
    }

    /// An issue that ended the grant at the token server (a reversed payment, a wrong issue
    /// key, every month issued to another request), with why.
    pub(super) fn ended(issued: &Issued) -> Option<(NeverIssued, String)> {
        match issued {
            Issued::Ended(why) => Some((NeverIssued(()), why.clone())),
            Issued::Tokens(_) | Issued::NotDue { .. } => None,
        }
    }
}

/// An order that made no checkout anyone was shown: nothing can be paid for it.
#[derive(Debug)]
pub struct NotMade(());

impl NotMade {
    /// The token server REFUSED the order (`e` is an answer with a refusal, not a lost one): it
    /// made no checkout. A call that got no answer, a busy server and a 5xx are not this: the
    /// checkout may exist, its entry stays as the record ([`super::PendingCheckout::is_posting`]).
    pub(super) fn refused(e: &TokenError) -> Option<NotMade> {
        matches!(e, TokenError::Refused { .. }).then_some(NotMade(()))
    }

    /// The checkout the order made was ended before anything of it was shown (its abandon
    /// answered over, or another window had ended it): the order's record goes with it.
    pub(super) fn unshown(abandoned: &Abandoned) -> Option<NotMade> {
        match abandoned {
            Abandoned::Ended | Abandoned::Settled => Some(NotMade(())),
            Abandoned::Kept { .. } => None,
        }
    }

    /// An entry of an order whose answer never came ([`super::PendingCheckout::is_posting`]),
    /// made [`POSTING_LIFE_SECS`] or more before `now` (seconds since 1970): its payment page
    /// was never shown, so nobody can pay for what it may have made.
    pub(super) fn stale(entry: &PendingCheckout, now: i64) -> Option<NotMade> {
        (entry.is_posting() && now.saturating_sub(entry.started_at) >= POSTING_LIFE_SECS)
            .then_some(NotMade(()))
    }
}

impl Settled {
    /// What it ends the checkout for, for an error's context.
    pub(super) fn what(&self) -> String {
        let why = match &self.0 {
            SettledBy::Claimed => "its drive is saved",
            SettledBy::Finished => "its period grant is kept with its drive",
            SettledBy::Receipted => "its receipt is kept or no longer awaited",
            SettledBy::NeverIssued => "the token server will issue no more of its grant",
            SettledBy::Declined => "its payment did not go through",
            SettledBy::Gone => "the token server no longer has it",
            SettledBy::HandoverOver => {
                "its payment stayed taken back until its hand-over was purged"
            }
            SettledBy::ClaimsEnded => "the drive can be picked up here no more",
            SettledBy::Abandoned(answer) => {
                return format!("ending the checkout: its abandon answered {answer:?}")
            }
            SettledBy::Unopenable => "its sign-up does not open",
            SettledBy::NotMade => "the order made no checkout",
        };
        format!("ending the checkout: {why}")
    }

    /// Its drive is saved in the drives file and it has no period grant: nothing more is owed.
    /// (A development sign-up, a checkout approved before period tokens.)
    pub(super) fn claimed() -> Settled {
        Settled(SettledBy::Claimed)
    }

    /// The token server has no such checkout.
    pub(super) fn gone(_: &NoSuchCheckout) -> Settled {
        Settled(SettledBy::Gone)
    }

    /// The drive can be picked up here no more.
    pub(super) fn claims_ended(_: &ClaimsEnded) -> Settled {
        Settled(SettledBy::ClaimsEnded)
    }

    /// Its payment was taken back for as long as the token server kept its hand-over.
    pub(super) fn handover_over(_: &HandoverOver) -> Settled {
        Settled(SettledBy::HandoverOver)
    }

    /// Its sign-up did not open, often enough.
    pub(super) fn unopenable(_: &Strikes) -> Settled {
        Settled(SettledBy::Unopenable)
    }

    /// The order made no checkout anyone was shown.
    pub(super) fn not_made(_: &NotMade) -> Settled {
        Settled(SettledBy::NotMade)
    }

    /// Its period grant is in two places.
    pub(super) fn finished(_: &GrantKept) -> Settled {
        Settled(SettledBy::Finished)
    }

    /// Nothing more will be issued for it.
    pub(super) fn never_issued(_: &NeverIssued) -> Settled {
        Settled(SettledBy::NeverIssued)
    }

    /// Its grant was kept before, and all that waited was its receipt.
    pub(super) fn receipted(_: &GrantKeptEarlier) -> Settled {
        Settled(SettledBy::Receipted)
    }

    /// The abandon's answer, when it ended the checkout ([`AbandonAnswer::is_over`]); `None`
    /// when the checkout may still be paid. Made by [`super::abandon`] from the answer the token
    /// server gave it and nowhere else: the answer is a public enum, so a caller that could call
    /// this would end any checkout with one it made up (X-DELTA-AZDRIVE-MONEY-11).
    #[must_use]
    pub(super) fn abandoned(answer: AbandonAnswer) -> Option<Settled> {
        answer
            .is_over()
            .then_some(Settled(SettledBy::Abandoned(answer)))
    }

    /// A proof for the tests of the list itself: a checkout whose abandon the token server
    /// answered "expired".
    #[cfg(test)]
    pub(crate) fn expired_for_tests() -> Settled {
        Settled(SettledBy::Abandoned(AbandonAnswer::Expired))
    }

    /// What a poll's answer `status` ends - the payment declined (or its cash refused), the
    /// checkout gone (expired, or the token server's own `no_such_checkout`), its drive's
    /// hand-over purged - with what is said of it, once; `None` for an answer that ends nothing.
    /// A payment TAKEN BACK ([`CheckoutStatus::Reversed`]) ends nothing: the token server puts
    /// it back when its refund failed or a dispute was won, and an app that threw its claim secret
    /// away on the word could never open the drive it then approves again (X-DELTA-MONEY5-1).
    ///
    /// [`CheckoutStatus::Declined`], [`CheckoutStatus::Gone`] and [`CheckoutStatus::HandoverGone`]
    /// are made by [`crate::token::TokenServer::checkout_status`] from those words of the token
    /// server and from nothing else.
    pub(super) fn ended_by(status: &CheckoutStatus, checkout_id: &str) -> Option<(Settled, Final)> {
        match status {
            CheckoutStatus::Declined(why) => Some((
                Settled(SettledBy::Declined),
                Final::Declined(format!("the payment did not go through: {why}")),
            )),
            CheckoutStatus::Gone(why) => Some((
                Settled(SettledBy::Gone),
                Final::Gone(format!("the token server no longer has it: {why}")),
            )),
            CheckoutStatus::HandoverGone => Some((
                Settled(SettledBy::Gone),
                Final::Gone(format!(
                    "it was paid, but the token server keeps its drive's hand-over only 30 days \
                     after the payment and has it no more: support can help with the checkout \
                     {checkout_id}"
                )),
            )),
            CheckoutStatus::Pending
            | CheckoutStatus::Approved(..)
            | CheckoutStatus::Reversed(_)
            | CheckoutStatus::Unopenable(_)
            | CheckoutStatus::Unreadable(_)
            | CheckoutStatus::Renewal(..)
            | CheckoutStatus::Unknown(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use azlin_contract::AbandonAnswer;

    use super::Settled;

    /// An abandon's answer ends a checkout only when the token server says it is over; a pending
    /// one (a letter of cash in the post) or a word this build does not know never does. (Only
    /// `abandon` calls `Settled::abandoned`: the answer is a public enum.)
    #[test]
    fn an_abandon_ends_a_checkout_only_with_an_answer_that_says_it_is_over() {
        for over in [
            AbandonAnswer::Expired,
            AbandonAnswer::Declined,
            AbandonAnswer::Reversed,
            AbandonAnswer::Rejected,
        ] {
            assert!(Settled::abandoned(over).is_some(), "{over:?}");
        }
        for open in [AbandonAnswer::StillPending, AbandonAnswer::Unknown] {
            assert!(Settled::abandoned(open).is_none(), "{open:?}");
        }
    }
}
