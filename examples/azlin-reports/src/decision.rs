//! The operator's decision on a report and its validation.
//!
//! Implements planning/architecture.md section 3.9 ("Deciding (AzCtl's Reports page, `azctl account content`):
//! the operator opens the item through the reported link and chooses no action, block the link (`link_blocks`,
//! every node at once), lock the account (read only), ban, legal hold, or refer to the authorities (Art. 18;
//! CSAM to the BKA's channel, check). Each decision has a reason from a fixed list plus text").
//!
//! What a decision needs from its report (drift X-DELTA-W1S46A-18; the matrix pinned by the tests of check R16):
//! - `NoAction` needs nothing: it is the answer "looked, nothing to do" to any report.
//! - `BlockLink` needs a link: a `Link` subject, or a `File` that has one.
//! - `LockAccount`, `Ban` and `LegalHold` act on an account: every subject names one (a link, a chat record's
//!   author, a member) except a `File` with no link, which names nothing the registry can resolve.
//! - `ReferToAuthorities` (Art. 18) needs the operator's text.
//! - Any action but `NoAction` with the reason `Other` needs text (a free reason is explained).
//!
//! Caller: S46-INT-3 (AzCtl's Reports page validates a decision before it signs the operator row in `ops`).

use core::fmt;

use crate::body::{Field, Reason, ReportBody, Subject, Text};
use crate::ids::{Day, ReportId};

/// What the operator does about a report (planning/architecture.md section 3.9).
///
/// Caller: S46-INT-3 (the Reports page's buttons), `statement_of` and `NotifierNotice::for_decision`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    /// Nothing is done.
    NoAction,
    /// Block the link (`link_blocks`, every node at once).
    BlockLink,
    /// Lock the account: read only.
    LockAccount,
    /// Ban the account (`ban_until`).
    Ban,
    /// A legal hold: every purge of the account or space stops until it is released.
    LegalHold,
    /// Refer to the authorities (Art. 18 DSA; CSAM to the BKA's channel).
    ReferToAuthorities,
}

impl Action {
    /// Every action.
    pub const ALL: [Action; 6] = [
        Action::NoAction,
        Action::BlockLink,
        Action::LockAccount,
        Action::Ban,
        Action::LegalHold,
        Action::ReferToAuthorities,
    ];
}

/// A decision on one report.
///
/// Caller: S46-INT-3 (AzCtl records it in `ops`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// The report decided.
    pub report: ReportId,
    /// What is done.
    pub action: Action,
    /// The operator's reason, from the fixed list.
    pub reason: Reason,
    /// The operator's text (at most [`MAX_TEXT`](crate::body::MAX_TEXT) bytes); required for `Other` and for a referral.
    pub text: Option<Text>,
    /// The day of the decision.
    pub day: Day,
}

/// Why a decision does not fit its report. When several things are wrong the first of this list is reported:
/// the wrong report, the subject, the text.
///
/// Caller: S46-INT-3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecisionError {
    /// The decision names another report than the one it is validated against.
    WrongReport,
    /// `BlockLink` needs a subject with a link.
    NeedsLink,
    /// The action acts on an account and the subject names none the registry could resolve.
    NeedsAccount,
    /// The reason `Other` or the action `ReferToAuthorities` needs the operator's text.
    NeedsText,
    /// The text is longer than [`MAX_TEXT`](crate::body::MAX_TEXT) bytes.
    TextTooLong,
}

impl fmt::Display for DecisionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            DecisionError::WrongReport => "the decision is about another report",
            DecisionError::NeedsLink => "blocking a link needs a reported link",
            DecisionError::NeedsAccount => "the report names no account to act on",
            DecisionError::NeedsText => "the reason or the action needs the operator's text",
            DecisionError::TextTooLong => "the text is longer than the limit",
        })
    }
}

impl std::error::Error for DecisionError {}

impl Decision {
    /// Whether this decision fits `report` (see the module documentation for the matrix).
    ///
    /// Caller: S46-INT-3.
    pub fn validate(&self, report: &ReportBody) -> Result<(), DecisionError> {
        if self.report != report.id {
            return Err(DecisionError::WrongReport);
        }
        if self.action == Action::NoAction {
            return Ok(());
        }
        let has_link = match &report.subject {
            Subject::Link { .. } => true,
            Subject::File { link, .. } => link.is_some(),
            Subject::ChatRecords { .. } | Subject::Member { .. } => false,
        };
        let has_account = !matches!(&report.subject, Subject::File { link: None, .. });
        match self.action {
            Action::BlockLink if !has_link => return Err(DecisionError::NeedsLink),
            Action::LockAccount | Action::Ban | Action::LegalHold if !has_account => {
                return Err(DecisionError::NeedsAccount);
            }
            _ => {}
        }
        let text_len = self.text.as_ref().map(Text::len);
        if text_len.is_some_and(|n| n > Field::Text.max()) {
            return Err(DecisionError::TextTooLong);
        }
        let has_text = text_len.is_some_and(|n| n > 0);
        if !has_text && (self.reason == Reason::Other || self.action == Action::ReferToAuthorities)
        {
            return Err(DecisionError::NeedsText);
        }
        Ok(())
    }
}
