//! A drive's recovery as this computer keeps it (D51): when its recovery code was made and when
//! it was last typed back right (the setup's check, a drill), the drills' schedule, the trusted
//! contacts holding shares of the code, the other devices with the key - and the RECOVERY
//! HEALTH the drive's info panel shows from it:
//!
//! - green: two methods or more, the code checked within a year;
//! - yellow: one method, or the code last checked over a year ago;
//! - red: no checked method (a setup whose code was never typed back).
//!
//! The methods: the recovery code (once typed back right), trusted contacts (when at least two
//! of the three shares were handed over: two open the code), another device with the key, and
//! a passkey (later). DRILLS ask for the code again a week after it was made, three months
//! after, then every year; "Later" moves one a week, and they stop only when another offline
//! method exists (two printed shares) - never while the code is the only one.
//!
//! Plain data with the settings (`Settings::recovery`, `drive/view.json`), so it is tested
//! without the encryption feature; the flows are `recovery.rs`'s.

use serde::{Deserialize, Serialize};

/// A day in seconds.
pub const DAY: u64 = 86_400;
/// The drills: a week after the code was made, three months after it, then every year.
pub const FIRST_DRILL: u64 = 7 * DAY;
pub const SECOND_DRILL: u64 = 91 * DAY;
pub const YEARLY: u64 = 365 * DAY;
/// How far "Later" moves a drill.
pub const POSTPONE: u64 = 7 * DAY;
/// Shares of a code that open it (of the three trusted contacts hold).
pub const SHARES_NEEDED: usize = 2;

/// A way back into a drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Method {
    /// The recovery code (the emergency kit).
    Code,
    /// Trusted contacts, two of three shares of the code.
    Contacts,
    /// Another device that has the drive's key.
    OtherDevice,
    /// A passkey whose PRF output wraps the drive key (designed, not built yet).
    Passkey,
}

/// The info panel's traffic light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Green,
    Yellow,
    Red,
}

/// How a trusted contact holds a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareKind {
    /// Sealed to the contact's key, in their AzDrive.
    #[default]
    App,
    /// On paper (a QR code and its text), for someone without AzDrive.
    Printed,
}

/// A trusted contact of a drive.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TrustedContact {
    /// What the owner calls them (on this computer only).
    pub name: String,
    pub kind: ShareKind,
    /// Which of the shares (1 to 3).
    pub index: u8,
    /// When it was handed over (sent, printed), seconds since 1970.
    pub handed: Option<u64>,
}

/// A share of someone else's recovery code this computer holds for them (a trusted contact's
/// side): sealed to a key of this computer's keyring.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HeldShare {
    /// The id of the key it is sealed to (the keyring entry's).
    pub key_id: String,
    /// Whose it is, in the owner's words (from inside the sealed share once it came).
    pub label: String,
    /// The sealed share's text; empty until the owner sent it.
    pub sealed: String,
    /// When the key was made, and when the share came (seconds since 1970).
    pub made: u64,
    pub received: Option<u64>,
}

/// One drive's recovery.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoveryState {
    pub drive_id: String,
    /// When its recovery code was made (the setup, a rotation), seconds since 1970.
    pub code_made: Option<u64>,
    /// When the code was last typed back right (the setup's check, a drill); `None`: never.
    pub code_checked: Option<u64>,
    /// Drills passed since the code was made.
    pub drills_done: u32,
    /// "Later": the next drill waits until then.
    pub postponed_until: Option<u64>,
    /// The code's public recovery key (standard base64, what the token server holds too): a
    /// drill checks the code typed against it, offline.
    pub recovery_key: Option<String>,
    /// The id of the split the trusted contacts hold shares of (hex).
    pub contacts_set: Option<String>,
    pub contacts: Vec<TrustedContact>,
    /// Other devices with the drive's key, as last counted.
    pub other_devices: u32,
    /// The drills stopped (only with another offline method).
    pub drills_off: bool,
}

/// The settings' part: each drive's recovery, and the shares held for others.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RecoverySettings {
    pub drives: Vec<RecoveryState>,
    pub held: Vec<HeldShare>,
}

impl RecoveryState {
    #[must_use]
    pub fn new(drive_id: &str) -> RecoveryState {
        RecoveryState {
            drive_id: drive_id.to_string(),
            ..RecoveryState::default()
        }
    }

    pub fn code_made(&mut self, _now: u64, _recovery_key: Option<String>) {}

    pub fn setup_verified(&mut self, _now: u64) {}

    pub fn drill_passed(&mut self, _now: u64) {}

    #[must_use]
    pub fn next_drill(&self) -> Option<u64> {
        None
    }

    #[must_use]
    pub fn drill_due(&self, _now: u64) -> bool {
        false
    }

    pub fn postpone(&mut self, _now: u64) {}

    #[must_use]
    pub fn may_stop_drills(&self) -> bool {
        true
    }

    pub fn stop_drills(&mut self) -> bool {
        true
    }

    #[must_use]
    pub fn methods(&self) -> Vec<Method> {
        Vec::new()
    }

    #[must_use]
    pub fn health(&self, _now: u64) -> Health {
        Health::Green
    }
}

/// The recovery of `drive_id`, if this computer keeps one.
#[must_use]
pub fn state_of<'a>(_states: &'a [RecoveryState], _drive_id: &str) -> Option<&'a RecoveryState> {
    None
}

/// The recovery of `drive_id`, made when there is none.
pub fn state_mut<'a>(states: &'a mut Vec<RecoveryState>, drive_id: &str) -> &'a mut RecoveryState {
    states.push(RecoveryState::new(drive_id));
    states.last_mut().expect("just pushed")
}

/// The info panel's line: `Green: 2 methods, the code checked on 2026-10-10`.
#[must_use]
pub fn health_line(_states: &[RecoveryState], _drive_id: &str, _now: u64) -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_790_000_000;

    /// A drive whose code was made at `NOW` and typed back at once (the setup's check).
    fn checked() -> RecoveryState {
        let mut state = RecoveryState::new("d_1");
        state.code_made(NOW, Some(String::from("key")));
        state.setup_verified(NOW);
        state
    }

    fn contact(name: &str, index: u8, kind: ShareKind) -> TrustedContact {
        TrustedContact {
            name: name.to_string(),
            kind,
            index,
            handed: Some(NOW),
        }
    }

    #[test]
    fn drills_come_a_week_then_three_months_then_every_year_after_the_code_is_made() {
        let mut state = checked();
        assert_eq!(state.next_drill(), Some(NOW + 7 * DAY));
        assert!(!state.drill_due(NOW + 7 * DAY - 1));
        assert!(state.drill_due(NOW + 7 * DAY));
        state.drill_passed(NOW + 8 * DAY);
        assert_eq!(
            state.next_drill(),
            Some(NOW + 91 * DAY),
            "three months after the code"
        );
        state.drill_passed(NOW + 92 * DAY);
        assert_eq!(
            state.next_drill(),
            Some(NOW + 92 * DAY + 365 * DAY),
            "then a year"
        );
        state.drill_passed(NOW + 500 * DAY);
        assert_eq!(state.next_drill(), Some(NOW + 500 * DAY + 365 * DAY));
        assert_eq!(state.code_checked, Some(NOW + 500 * DAY));
    }

    #[test]
    fn a_drill_is_postponed_but_not_stopped_while_the_code_is_the_only_offline_method() {
        let mut state = checked();
        let due = NOW + 7 * DAY;
        state.postpone(due);
        assert!(!state.drill_due(due), "later");
        assert!(
            state.drill_due(due + POSTPONE),
            "a week later it asks again"
        );
        assert!(
            !state.may_stop_drills(),
            "the code is the only offline method"
        );
        assert!(!state.stop_drills());
        assert!(state.drill_due(due + POSTPONE));
        state.contacts = vec![
            contact("Ada", 1, ShareKind::App),
            contact("Grace", 2, ShareKind::App),
        ];
        assert!(!state.may_stop_drills(), "shares in apps are not offline");
        state.contacts = vec![
            contact("Ada", 1, ShareKind::Printed),
            contact("Grace", 2, ShareKind::Printed),
        ];
        assert!(
            state.may_stop_drills(),
            "two printed shares are an offline method too"
        );
        assert!(state.stop_drills());
        assert!(!state.drill_due(due + 400 * DAY));
    }

    #[test]
    fn recovery_health_is_green_with_two_methods_and_the_code_checked_within_a_year() {
        let mut state = checked();
        state.other_devices = 1;
        assert_eq!(state.methods(), vec![Method::Code, Method::OtherDevice]);
        assert_eq!(state.health(NOW + 365 * DAY), Health::Green);
        let mut contacts = checked();
        contacts.contacts = vec![
            contact("Ada", 1, ShareKind::App),
            contact("Grace", 2, ShareKind::Printed),
            contact("Linus", 3, ShareKind::App),
        ];
        assert_eq!(contacts.methods(), vec![Method::Code, Method::Contacts]);
        assert_eq!(contacts.health(NOW), Health::Green);
    }

    #[test]
    fn recovery_health_is_yellow_with_one_method_or_an_old_check() {
        let state = checked();
        assert_eq!(state.methods(), vec![Method::Code]);
        assert_eq!(state.health(NOW), Health::Yellow, "one method");
        let mut old = checked();
        old.other_devices = 2;
        assert_eq!(
            old.health(NOW + 366 * DAY),
            Health::Yellow,
            "checked over a year ago"
        );
        let mut one_share = checked();
        one_share.other_devices = 1;
        one_share.contacts = vec![contact("Ada", 1, ShareKind::App)];
        assert_eq!(
            one_share.methods(),
            vec![Method::Code, Method::OtherDevice],
            "one share opens nothing"
        );
    }

    #[test]
    fn recovery_health_is_red_without_a_checked_method_and_an_unchecked_code_has_no_drills() {
        let mut state = RecoveryState::new("d_1");
        state.code_made(NOW, None);
        assert!(state.methods().is_empty(), "the setup did not finish");
        assert_eq!(state.health(NOW), Health::Red);
        assert_eq!(state.next_drill(), None, "it needs a new code, not a drill");
        assert!(
            health_line(&[], "d_1", NOW).is_none(),
            "a drive without a state"
        );
        let line = health_line(&[state], "d_1", NOW).unwrap();
        assert!(line.starts_with("Red"), "{line}");
    }

    #[test]
    fn a_new_code_starts_the_drills_again_and_its_old_shares_no_longer_count() {
        let mut state = checked();
        state.contacts = vec![
            contact("Ada", 1, ShareKind::App),
            contact("Grace", 2, ShareKind::App),
        ];
        state.contacts_set = Some(String::from("0a0b0c0d"));
        state.drill_passed(NOW + 8 * DAY);
        state.code_made(NOW + 9 * DAY, Some(String::from("new key")));
        assert_eq!(state.code_checked, None);
        assert_eq!(state.drills_done, 0);
        assert!(state.contacts.is_empty() && state.contacts_set.is_none());
        assert_eq!(state.recovery_key.as_deref(), Some("new key"));
        state.setup_verified(NOW + 9 * DAY);
        assert_eq!(state.next_drill(), Some(NOW + 16 * DAY));
    }

    #[test]
    fn each_drives_recovery_lives_in_the_settings_file() {
        let mut settings = crate::model::Settings::default();
        let state = state_mut(&mut settings.recovery.drives, "d_1");
        state.code_made(NOW, Some(String::from("key")));
        state.setup_verified(NOW);
        state_mut(&mut settings.recovery.drives, "d_2").other_devices = 3;
        assert_eq!(settings.recovery.drives.len(), 2);
        assert_eq!(
            state_mut(&mut settings.recovery.drives, "d_1").code_checked,
            Some(NOW)
        );
        let back = crate::model::Settings::from_json(&settings.to_json());
        assert_eq!(back.recovery, settings.recovery);
        assert_eq!(
            state_of(&back.recovery.drives, "d_2").map(|s| s.other_devices),
            Some(3)
        );
        let old = crate::model::Settings::from_json(r#"{"show_hidden": true}"#);
        assert!(
            old.show_hidden && old.recovery.drives.is_empty(),
            "a file from before"
        );
    }
}
