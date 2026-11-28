//! The pretend user of the stand-in keyring and biometric sensor.
//!
//! A test or headless run never reaches the system keyring or a real sensor: the stand-in keyring
//! (`dll::desktop::extra::keyring`'s memory and file backends) and the stand-in biometric gate
//! answer from a [`StandInUser`] instead. The user is SCRIPTED: what is enrolled, whether a
//! passcode is set, how often the enrolled set has changed, and what the user does at each
//! sheet (a match, a mismatch, a cancel, a lockout, the passcode). The e2e scenarios script it
//! (`biometric_enroll`, `biometric_script`, `biometric_enrollment_changed`), the unit tests of
//! every backend script it, and the flows of the protected keyring items run the same on every
//! host: a desktop, an emulator, a phone.
//!
//! Owned by the app (`BiometricChannels::user`, one per `App`) and handed to the stand-in with
//! each request: nothing is looked up by name, two apps in a process or two tests each have a
//! user of their own. Cloning gives another handle on the SAME user.

use alloc::{collections::VecDeque, sync::Arc, vec::Vec};
use std::sync::{Mutex, MutexGuard, PoisonError};

use azul_core::{
    biometric::{BiometricKind, BiometricResult},
    keyring::KeyringProtection,
};

/// What the pretend user does at the next user-check sheet. At most [`MAX_SCRIPT`] wait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScriptedOutcome {
    /// The right finger / face (or the passcode on a sheet that takes it): the check passes.
    Match,
    /// A finger that does not match and the tries are used up: `Failed`.
    Mismatch,
    /// The user dismisses the sheet: `Cancelled`.
    Cancel,
    /// Too many failures: `LockedOut`.
    Lockout,
    /// The user takes the passcode path: passes a `UserPresence` sheet; a `Biometric` sheet has
    /// no such path, the user can only cancel.
    Passcode,
    /// The device turns out to have nothing enrolled: `NotEnrolled`.
    NotEnrolled,
}

impl ScriptedOutcome {
    /// Every outcome, in the order the e2e scenarios name them.
    pub const ALL: [Self; 6] = [
        Self::Match,
        Self::Mismatch,
        Self::Cancel,
        Self::Lockout,
        Self::Passcode,
        Self::NotEnrolled,
    ];

    /// The outcome a scenario names: `match`, `mismatch`, `cancel`, `lockout`, `passcode`,
    /// `not_enrolled` (any case, a `-` or a space for the `_`).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Self::ALL.into_iter().find(|outcome| outcome.name() == name)
    }

    /// The name a scenario uses for the outcome.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Match => "match",
            Self::Mismatch => "mismatch",
            Self::Cancel => "cancel",
            Self::Lockout => "lockout",
            Self::Passcode => "passcode",
            Self::NotEnrolled => "not_enrolled",
        }
    }
}

/// What the pretend sheet decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetOutcome {
    /// The check passed.
    Passed,
    /// The user dismissed the sheet.
    Cancelled,
    /// The biometric did not match.
    Failed,
    /// Biometrics are locked out.
    LockedOut,
    /// Nothing is enrolled that could pass the check.
    NotEnrolled,
}

/// The most outcomes a script holds: a longer one is cut (a scenario names a handful).
pub const MAX_SCRIPT: usize = 64;

#[derive(Debug)]
struct State {
    enrolled: bool,
    passcode_set: bool,
    epoch: u32,
    outcomes: VecDeque<ScriptedOutcome>,
}

/// The scripted user of the stand-ins. See the module.
#[derive(Debug, Clone)]
pub struct StandInUser {
    state: Arc<Mutex<State>>,
}

impl Default for StandInUser {
    fn default() -> Self {
        Self::new()
    }
}

/// Two users compare EQUAL whatever they hold: a user is the connection of a stand-in to the
/// script that drives it, not part of the manager's state (as with `SharedQueue`).
impl PartialEq for StandInUser {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for StandInUser {}

impl StandInUser {
    /// A device with nothing set up: no biometric enrolled, no passcode, nothing scripted. A
    /// stand-in gate answers `Unavailable` and no protection level can be met.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                enrolled: false,
                passcode_set: false,
                epoch: 0,
                outcomes: VecDeque::new(),
            })),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Enrol (or remove) the biometric, and set (or clear) the passcode. Enrolling a biometric
    /// that was not enrolled before is a change of the enrolled set, as on a device.
    pub fn enroll(&self, enrolled: bool, passcode_set: bool) {
        let mut state = self.lock();
        if state.enrolled != enrolled {
            state.epoch = state.epoch.wrapping_add(1);
        }
        state.enrolled = enrolled;
        state.passcode_set = passcode_set;
    }

    /// Whether a biometric is enrolled.
    #[must_use]
    pub fn enrolled(&self) -> bool {
        self.lock().enrolled
    }

    /// Whether a passcode is set.
    #[must_use]
    pub fn passcode_set(&self) -> bool {
        self.lock().passcode_set
    }

    /// A finger or face was added or removed: items bound to the old set (`Biometric`) are
    /// invalidated, `UserPresence` ones survive (the passcode still works).
    pub fn enrollment_changed(&self) {
        let mut state = self.lock();
        state.epoch = state.epoch.wrapping_add(1);
    }

    /// How often the enrolled set changed: an item keeps the number it was stored under.
    #[must_use]
    pub fn epoch(&self) -> u32 {
        self.lock().epoch
    }

    /// Replace the script: what the user does at the next sheets, in order. More than
    /// [`MAX_SCRIPT`] are cut. Once it runs out the user matches.
    pub fn script(&self, outcomes: impl IntoIterator<Item = ScriptedOutcome>) {
        let mut state = self.lock();
        state.outcomes = outcomes.into_iter().take(MAX_SCRIPT).collect();
    }

    /// How many scripted outcomes have not been used yet.
    #[must_use]
    pub fn pending_outcomes(&self) -> usize {
        self.lock().outcomes.len()
    }

    /// The sensor kind the stand-in reports: a fingerprint reader when a biometric is enrolled.
    #[must_use]
    pub fn kind(&self) -> BiometricKind {
        if self.lock().enrolled {
            BiometricKind::Fingerprint
        } else {
            BiometricKind::NotAvailable
        }
    }

    /// Whether this device can meet `protection`: a biometric must be enrolled for `Biometric`,
    /// a biometric or a passcode for `UserPresence`; `DeviceUnlocked` always.
    #[must_use]
    pub fn meets(&self, protection: KeyringProtection) -> bool {
        let state = self.lock();
        match protection {
            KeyringProtection::DeviceUnlocked => true,
            KeyringProtection::UserPresence => state.enrolled || state.passcode_set,
            KeyringProtection::Biometric => state.enrolled,
        }
    }

    /// The pretend sheet of an item of `protection`: it needs what the level needs enrolled
    /// (else `NotEnrolled`, nothing scripted is used up), then the user does the next scripted
    /// thing (a match when the script has run out).
    #[must_use]
    pub fn check(&self, protection: KeyringProtection) -> SheetOutcome {
        if !self.meets(protection) {
            return SheetOutcome::NotEnrolled;
        }
        let next = self.lock().outcomes.pop_front().unwrap_or(ScriptedOutcome::Match);
        match next {
            ScriptedOutcome::Match => SheetOutcome::Passed,
            ScriptedOutcome::Mismatch => SheetOutcome::Failed,
            ScriptedOutcome::Cancel => SheetOutcome::Cancelled,
            ScriptedOutcome::Lockout => SheetOutcome::LockedOut,
            ScriptedOutcome::NotEnrolled => SheetOutcome::NotEnrolled,
            // The passcode path exists on a `UserPresence` sheet only.
            ScriptedOutcome::Passcode => {
                if protection == KeyringProtection::UserPresence {
                    SheetOutcome::Passed
                } else {
                    SheetOutcome::Cancelled
                }
            }
        }
    }

    /// The pretend standalone biometric gate (`request_biometric_auth`). A device with no
    /// biometric enrolled (and no passcode where the prompt allows it) answers `Unavailable`, as
    /// the stand-in always did; else the user does the next scripted thing.
    #[must_use]
    pub fn gate(&self, allow_device_credential: bool) -> BiometricResult {
        let (enrolled, passcode_set) = {
            let state = self.lock();
            (state.enrolled, state.passcode_set)
        };
        if !enrolled && !(allow_device_credential && passcode_set) {
            return BiometricResult::Unavailable;
        }
        let next = self.lock().outcomes.pop_front().unwrap_or(ScriptedOutcome::Match);
        match next {
            ScriptedOutcome::Match if enrolled => BiometricResult::Authenticated,
            ScriptedOutcome::Match => BiometricResult::FellBackToPasscode,
            ScriptedOutcome::Mismatch => BiometricResult::Failed,
            ScriptedOutcome::Cancel => BiometricResult::Cancelled,
            ScriptedOutcome::Lockout => BiometricResult::LockedOut,
            ScriptedOutcome::NotEnrolled => BiometricResult::NotEnrolled,
            ScriptedOutcome::Passcode => {
                if allow_device_credential {
                    BiometricResult::FellBackToPasscode
                } else {
                    BiometricResult::Cancelled
                }
            }
        }
    }

    /// The outcomes still scripted, oldest first (a test looks at what is left).
    #[must_use]
    pub fn remaining_script(&self) -> Vec<ScriptedOutcome> {
        self.lock().outcomes.iter().copied().collect()
    }
}

#[cfg(test)]
#[path = "stand_in_user_test.rs"]
mod stand_in_user_test;
