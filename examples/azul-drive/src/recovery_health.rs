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

    /// A new recovery code (the setup, a rotation): made `now`, not checked yet, its recovery
    /// key `recovery_key`. The drills count from it again, and the shares trusted contacts hold
    /// of the old code no longer open anything.
    pub fn code_made(&mut self, now: u64, recovery_key: Option<String>) {
        self.code_made = Some(now);
        self.code_checked = None;
        self.drills_done = 0;
        self.postponed_until = None;
        self.recovery_key = recovery_key;
        self.contacts_set = None;
        self.contacts.clear();
        self.drills_off = false;
    }

    /// The setup's check passed: the code was typed back right `now`.
    pub fn setup_verified(&mut self, now: u64) {
        self.code_checked = Some(now);
    }

    /// A drill passed `now` (the code typed back right): the next one comes later.
    pub fn drill_passed(&mut self, now: u64) {
        self.code_checked = Some(now);
        self.drills_done = self.drills_done.saturating_add(1);
        self.postponed_until = None;
    }

    /// When the next drill is due: a week after the code was made, three months after it, then
    /// a year after the last check - or later when it was postponed. `None` for a code never
    /// checked (it needs a new code, not a drill).
    #[must_use]
    pub fn next_drill(&self) -> Option<u64> {
        let made = self.code_made?;
        let checked = self.code_checked?;
        let due = match self.drills_done {
            0 => made.saturating_add(FIRST_DRILL),
            1 => made.saturating_add(SECOND_DRILL),
            _ => checked.saturating_add(YEARLY),
        };
        Some(self.postponed_until.map_or(due, |later| due.max(later)))
    }

    /// Whether a drill is due `now`.
    #[must_use]
    pub fn drill_due(&self, now: u64) -> bool {
        if self.drills_off && self.may_stop_drills() {
            return false;
        }
        self.next_drill().is_some_and(|due| now >= due)
    }

    /// "Later": the drill asks again a week after `now`.
    pub fn postpone(&mut self, now: u64) {
        self.postponed_until = Some(now.saturating_add(POSTPONE));
    }

    /// Shares handed over (`kind`: of that kind only).
    fn shares_handed(&self, kind: Option<ShareKind>) -> usize {
        self.contacts
            .iter()
            .filter(|c| c.handed.is_some() && kind.is_none_or(|k| c.kind == k))
            .count()
    }

    /// Methods that need no device and no account: the checked code, and enough printed
    /// shares to open it.
    #[must_use]
    pub fn offline_methods(&self) -> usize {
        usize::from(self.code_checked.is_some())
            + usize::from(self.shares_handed(Some(ShareKind::Printed)) >= SHARES_NEEDED)
    }

    /// Whether the drills may stop: not while the code is the only offline method.
    #[must_use]
    pub fn may_stop_drills(&self) -> bool {
        self.offline_methods() >= 2
    }

    /// Stops the drills when they may stop (whether they did).
    pub fn stop_drills(&mut self) -> bool {
        self.drills_off = self.may_stop_drills();
        self.drills_off
    }

    /// The drive's methods: the code once checked, trusted contacts once enough shares to
    /// open it were handed over, other devices with the key.
    #[must_use]
    pub fn methods(&self) -> Vec<Method> {
        let mut methods = Vec::new();
        if self.code_checked.is_some() {
            methods.push(Method::Code);
        }
        if self.shares_handed(None) >= SHARES_NEEDED {
            methods.push(Method::Contacts);
        }
        if self.other_devices > 0 {
            methods.push(Method::OtherDevice);
        }
        methods
    }

    /// The traffic light `now` (see the module documentation).
    #[must_use]
    pub fn health(&self, now: u64) -> Health {
        let methods = self.methods();
        if methods.is_empty() {
            return Health::Red;
        }
        let fresh = self
            .code_checked
            .is_some_and(|at| now.saturating_sub(at) <= YEARLY);
        if methods.len() >= 2 && fresh {
            Health::Green
        } else {
            Health::Yellow
        }
    }
}

impl Health {
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Health::Green => "Green",
            Health::Yellow => "Yellow",
            Health::Red => "Red",
        }
    }
}

impl Method {
    /// The method as the methods list names it.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Method::Code => "Recovery code",
            Method::Contacts => "Trusted contacts",
            Method::OtherDevice => "Another device",
            Method::Passkey => "Passkey",
        }
    }
}

/// The recovery of `drive_id`, if this computer keeps one.
#[must_use]
pub fn state_of<'a>(states: &'a [RecoveryState], drive_id: &str) -> Option<&'a RecoveryState> {
    states.iter().find(|state| state.drive_id == drive_id)
}

/// The recovery of `drive_id`, made when there is none.
pub fn state_mut<'a>(states: &'a mut Vec<RecoveryState>, drive_id: &str) -> &'a mut RecoveryState {
    let index = match states.iter().position(|state| state.drive_id == drive_id) {
        Some(index) => index,
        None => {
            states.push(RecoveryState::new(drive_id));
            states.len() - 1
        }
    };
    &mut states[index]
}

/// A day as the info panel writes it (UTC): `2026-10-10`.
fn day(at: u64) -> String {
    let text = azul_storage::time::iso8601(at);
    text.get(..10).unwrap_or(&text).to_string()
}

/// The info panel's line: `Green: 2 methods, the code checked on 2026-10-10`; `None` for a
/// drive this computer keeps no recovery of.
#[must_use]
pub fn health_line(states: &[RecoveryState], drive_id: &str, now: u64) -> Option<String> {
    let state = state_of(states, drive_id)?;
    let health = state.health(now);
    let methods = state.methods().len();
    let checked = state.code_checked.map_or_else(
        || String::from("the code never checked"),
        |at| format!("the code checked on {}", day(at)),
    );
    let advice = match health {
        Health::Green => String::new(),
        Health::Yellow if methods < 2 => String::from(" - add a second method"),
        Health::Yellow => String::from(" - check the code (Options > Drives > Test)"),
        Health::Red => String::from(" - make a new recovery code"),
    };
    let counted = if methods == 1 {
        String::from("1 method")
    } else {
        format!("{methods} methods")
    };
    Some(format!("{}: {counted}, {checked}{advice}", health.word()))
}

/// What a method's row in the methods list offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodAction {
    /// Check that it still works (a drill; two contacts' shares; ...).
    Test,
    /// Set it up.
    Add,
    /// Stop counting it.
    Remove,
    /// Count again (the other devices).
    CountAgain,
}

/// A method's row in Options > Drives: whether the drive has it, what it says, what it offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRow {
    pub method: Method,
    pub present: bool,
    pub status: String,
    pub actions: Vec<MethodAction>,
}

/// The methods list of a drive: the code, trusted contacts, other devices, a passkey (later).
#[must_use]
pub fn methods_list(state: &RecoveryState, _now: u64) -> Vec<MethodRow> {
    let code = match state.code_checked {
        Some(at) => {
            let next = if state.drills_off && state.may_stop_drills() {
                String::from(", no more checks")
            } else {
                state
                    .next_drill()
                    .map_or_else(String::new, |due| format!(", next check on {}", day(due)))
            };
            MethodRow {
                method: Method::Code,
                present: true,
                status: format!("Checked on {}{next}", day(at)),
                actions: vec![MethodAction::Test],
            }
        }
        None => MethodRow {
            method: Method::Code,
            present: false,
            status: String::from("Never typed back: make a new recovery code"),
            actions: vec![MethodAction::Test],
        },
    };
    let handed = state.shares_handed(None);
    let total = state.contacts.len();
    let contacts = if total == 0 {
        MethodRow {
            method: Method::Contacts,
            present: false,
            status: String::from("None"),
            actions: vec![MethodAction::Add],
        }
    } else {
        let names: Vec<&str> = state.contacts.iter().map(|c| c.name.as_str()).collect();
        let enough = handed >= SHARES_NEEDED;
        MethodRow {
            method: Method::Contacts,
            present: enough,
            status: if enough {
                format!(
                    "{handed} of {total} shares handed over ({})",
                    names.join(", ")
                )
            } else {
                format!(
                    "{handed} of {total} shares handed over ({}): two open the code",
                    names.join(", ")
                )
            },
            actions: if enough {
                vec![MethodAction::Test, MethodAction::Remove]
            } else {
                vec![MethodAction::Remove]
            },
        }
    };
    let devices = MethodRow {
        method: Method::OtherDevice,
        present: state.other_devices > 0,
        status: match state.other_devices {
            0 => String::from("None counted"),
            1 => String::from("1 other device has the key"),
            n => format!("{n} other devices have the key"),
        },
        actions: vec![MethodAction::CountAgain, MethodAction::Add],
    };
    let passkey = MethodRow {
        method: Method::Passkey,
        present: false,
        status: String::from("Not yet: a passkey comes with a later AzDrive"),
        actions: Vec::new(),
    };
    vec![code, contacts, devices, passkey]
}

/// The warning over the list when the drive has fewer than two methods.
#[must_use]
pub fn methods_warning(state: &RecoveryState) -> Option<&'static str> {
    (state.methods().len() < 2).then_some(
        "Fewer than two ways back in: with one, losing it locks you out of the drive. Add \
         trusted contacts or another device.",
    )
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
    fn the_methods_list_shows_each_method_what_it_offers_and_warns_below_two() {
        let mut state = checked();
        let rows = methods_list(&state, NOW);
        assert_eq!(
            rows.iter().map(|row| row.method).collect::<Vec<_>>(),
            [
                Method::Code,
                Method::Contacts,
                Method::OtherDevice,
                Method::Passkey
            ]
        );
        assert!(rows[0].present, "{:?}", rows[0]);
        assert!(rows[0].status.contains("Checked on"), "{}", rows[0].status);
        assert!(rows[0].status.contains("next check"), "{}", rows[0].status);
        assert_eq!(rows[0].actions, [MethodAction::Test]);
        assert!(!rows[1].present);
        assert_eq!(rows[1].actions, [MethodAction::Add]);
        assert!(!rows[2].present);
        assert_eq!(
            rows[2].actions,
            [MethodAction::CountAgain, MethodAction::Add]
        );
        assert!(!rows[3].present && rows[3].actions.is_empty());
        assert!(rows[3].status.contains("later"), "{}", rows[3].status);
        assert!(methods_warning(&state).is_some(), "one method");

        state.contacts = vec![
            contact("Ada", 1, ShareKind::App),
            contact("Grace", 2, ShareKind::Printed),
            TrustedContact {
                handed: None,
                ..contact("Linus", 3, ShareKind::App)
            },
        ];
        let rows = methods_list(&state, NOW);
        assert!(rows[1].present);
        assert!(rows[1].status.contains("2 of 3"), "{}", rows[1].status);
        assert_eq!(rows[1].actions, [MethodAction::Test, MethodAction::Remove]);
        assert!(methods_warning(&state).is_none(), "two methods");

        state.contacts.truncate(1);
        let rows = methods_list(&state, NOW);
        assert!(!rows[1].present, "one share opens nothing");
        assert_eq!(rows[1].actions, [MethodAction::Remove]);

        state.other_devices = 2;
        let rows = methods_list(&state, NOW);
        assert!(
            rows[2].present && rows[2].status.contains('2'),
            "{}",
            rows[2].status
        );

        let mut unchecked = RecoveryState::new("d_1");
        unchecked.code_made(NOW, None);
        let rows = methods_list(&unchecked, NOW);
        assert!(!rows[0].present);
        assert!(
            rows[0].status.contains("new recovery code"),
            "{}",
            rows[0].status
        );
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
