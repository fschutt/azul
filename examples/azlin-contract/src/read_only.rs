//! Why a bucket takes no writes, and what a node answers for each reason; the cluster map's
//! epoch the requests name.

use serde::{Deserialize, Serialize};

use crate::AzlinError;

/// Why a drive's bucket takes no writes. The bucket row keeps it beside its `read_only` flag;
/// a row from before the reason existed reads as [`ReadOnlyWhy::Unpaid`], the only reason
/// there was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadOnlyWhy {
    /// The paid period ended, and its grace too.
    #[default]
    Unpaid,
    /// Someone used the recovery code: the drive is frozen until the lockdown's notice ends.
    LockdownPending,
    /// The operator froze it (a move between blocks, `azctl bucket set`).
    Operator,
    /// A reason this build does not know: a newer service wrote it. Never written.
    #[serde(other)]
    Unknown,
}

impl ReadOnlyWhy {
    /// Every reason a service writes ([`ReadOnlyWhy::Unknown`] is not one).
    pub const ALL: [ReadOnlyWhy; 3] = [
        ReadOnlyWhy::Unpaid,
        ReadOnlyWhy::LockdownPending,
        ReadOnlyWhy::Operator,
    ];

    /// The reason as the wire and the bucket row write it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReadOnlyWhy::Unpaid => "unpaid",
            ReadOnlyWhy::LockdownPending => "lockdown_pending",
            ReadOnlyWhy::Operator => "operator",
            ReadOnlyWhy::Unknown => "unknown",
        }
    }

    /// The reason `text` names; `None` for one this build does not know.
    #[must_use]
    pub fn parse(text: &str) -> Option<ReadOnlyWhy> {
        let text = text.trim();
        ReadOnlyWhy::ALL.into_iter().find(|why| why.as_str() == text)
    }

    /// The code a node refuses a write with: each reason its own words for the user. A reason
    /// this build does not know is "read-only for now", never a payment.
    #[must_use]
    pub fn code(self) -> AzlinError {
        match self {
            ReadOnlyWhy::Unpaid => AzlinError::ReadOnlyUnpaid,
            ReadOnlyWhy::LockdownPending => AzlinError::ReadOnlyLockdown,
            ReadOnlyWhy::Operator | ReadOnlyWhy::Unknown => AzlinError::ReadOnlyOperator,
        }
    }
}

/// The header a client names the epoch of the cluster map it knows in (`x-azlin-map: 7`), and
/// a node with a newer map answers its own epoch and hash in (`x-azlin-map: 8 <hex>`, with a
/// 409 `map_stale`).
pub const MAP_HEADER: &str = "x-azlin-map";

/// The epoch an `x-azlin-map` value names: its first word.
#[must_use]
pub fn map_epoch(header: &str) -> Option<u64> {
    header.split_whitespace().next()?.parse().ok()
}
