//! The validation of a `watch` subscription: what a device may ask to be told about (planning/
//! architecture.md section 5.13 "`watch`, device to node": "The device says what it wants, per space ...
//! `live`, `sync`, `notify`, none ... The home node checks each subscribed space against the registry,
//! so events never tell a device more than `head_get` would", section 2.5 limits "spaces in one `watch`
//! subscription | 20,000 | `bad_request`"; build unit S16, lane W1-S16a).
//!
//! Only the VALIDATION is here. The delivery (the standing connection to the home node, the pacing of
//! `sync`, the coalescing of queued events, the security events that reach every device whatever it
//! asked for) is S33's. A subscription is a list of (space, level) pairs: a space that is not listed
//! is level "none" and nothing is sent for it.
//!
//! WHAT VALIDATION DOES: it refuses a subscription of more than [`MAX_SPACES`] entries (`TooMany`, a
//! `bad_request`: counted as sent, before anything is looked at, so that the work is bounded by the
//! limit and not by what a client sends); it collapses a space that is listed twice to the HIGHEST of
//! its levels; and it drops every space the device may not read the head of (`SpaceAccess::may_head_get`)
//! without a word: a space that does not exist and a space the device has no access to are the same
//! answer from the registry and are dropped the same way, so the result never tells a device more than
//! `head_get` would.

use std::collections::BTreeMap;

use azlin_seams::{mutant, DeviceId, SpaceId};

use crate::access::SpaceAccess;

/// How closely a device follows a space: `Notify` (only notices), `Sync` (the space is synced on this
/// device: at most one event per space per 5 s, est.), `Live` (the space is open on screen: every
/// change at once). A space the device does not follow is not in the subscription at all (level
/// "none"). The levels are ordered: `Live` is the highest, and a space listed twice keeps the higher
/// level.
///
/// Implements planning/architecture.md section 5.13 (`watch`: the levels `live`, `sync`, `notify`).
///
/// Caller: `validate_subscription`; S33 delivers each level at its pace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// A member, but the space is not synced: only notices.
    Notify,
    /// The space is synced on this device: at most one event per space per 5 s (an estimate).
    Sync,
    /// The space is open on screen: every change at once.
    Live,
}

impl Level {
    /// Every level, lowest first: the only three there are ("none" is the absence of an entry).
    pub const ALL: [Level; 3] = [Level::Notify, Level::Sync, Level::Live];

    /// The word of the level on the wire and in logs.
    pub const fn as_str(self) -> &'static str {
        match self {
            Level::Notify => "notify",
            Level::Sync => "sync",
            Level::Live => "live",
        }
    }

    /// The level of a word: exactly one of the three; `none` and every other word are refused (a
    /// device that wants nothing leaves the space out).
    pub fn from_word(word: &str) -> Option<Level> {
        match word {
            "notify" => Some(Level::Notify),
            "sync" => Some(Level::Sync),
            "live" => Some(Level::Live),
            _ => None,
        }
    }
}

/// The most spaces in one subscription: 20,000 (the limits table of planning/architecture.md section
/// 2.5).
///
/// Implements planning/architecture.md section 2.5 (limits: spaces in one `watch` subscription) and
/// section 1.4 rule 4.
///
/// Caller: `validate_subscription`.
pub const MAX_SPACES: usize = 20_000;

/// A subscription that passed validation: each space the device may read, at the highest level it
/// asked for. It holds at most [`MAX_SPACES`] spaces.
///
/// Implements planning/architecture.md section 5.13 (the home node checks each subscribed space against
/// the registry).
///
/// Caller: `validate_subscription`; S33 (the watcher's subscription state).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Accepted {
    spaces: BTreeMap<SpaceId, Level>,
}

impl Accepted {
    /// The spaces and levels, in the order of the space ids.
    pub fn iter(&self) -> impl Iterator<Item = (SpaceId, Level)> + '_ {
        self.spaces.iter().map(|(space, level)| (*space, *level))
    }

    /// The level of one space; `None` when it is not subscribed (or was dropped).
    pub fn level(&self, space: SpaceId) -> Option<Level> {
        self.spaces.get(&space).copied()
    }

    /// How many spaces are subscribed.
    pub fn len(&self) -> usize {
        self.spaces.len()
    }

    /// Whether nothing is subscribed.
    pub fn is_empty(&self) -> bool {
        self.spaces.is_empty()
    }
}

/// Why a subscription is refused.
///
/// Implements planning/architecture.md section 2.5 (limits: `bad_request` at 20,000 spaces) and section
/// 5.13.
///
/// Caller: `validate_subscription`; SYNC-APPLY / S33 map it to a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WatchRefusal {
    /// The subscription lists more spaces than a subscription may (status `bad_request`).
    #[error("the subscription lists {got} spaces, at most {max} are allowed")]
    TooMany {
        /// How many entries the subscription had.
        got: usize,
        /// The limit.
        max: usize,
    },
}

/// Validates a subscription of `device`: refuses more than [`MAX_SPACES`] entries, collapses a space
/// listed twice to its highest level, and drops the spaces `access` does not let the device read the
/// head of. See the module for why dropping is silent and identical for every reason.
///
/// Implements planning/architecture.md section 5.13 (`watch`: the device says what it wants; the home
/// node checks each space against the registry) and section 2.5 (the limit).
///
/// Caller: SYNC-APPLY / S33 (the home node, when a device opens or updates its `watch`).
pub fn validate_subscription(
    entries: &[(SpaceId, Level)],
    who: DeviceId,
    access: &dyn SpaceAccess,
) -> Result<Accepted, WatchRefusal> {
    if entries.len() > MAX_SPACES && !mutant!("sync_watch_limit_unchecked") {
        return Err(WatchRefusal::TooMany {
            got: entries.len(),
            max: MAX_SPACES,
        });
    }
    let mut spaces: BTreeMap<SpaceId, Level> = BTreeMap::new();
    for (space, level) in entries {
        match spaces.get_mut(space) {
            Some(held) => {
                let keep_lower = mutant!("sync_watch_keeps_lowest_level");
                if (*level > *held && !keep_lower) || (*level < *held && keep_lower) {
                    *held = *level;
                }
            }
            None => {
                spaces.insert(*space, *level);
            }
        }
    }
    // One registry question per distinct space; the answer for an unknown space and for a space the
    // device has no access to is the same `false`.
    if !mutant!("sync_watch_keeps_inaccessible") {
        spaces.retain(|space, _| access.may_head_get(who, *space));
    }
    Ok(Accepted { spaces })
}
