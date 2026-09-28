//! What the Wayland portal backend does at the end of one reconcile batch -
//! as a pure function, so it is testable on every host (the portal itself
//! only exists on a Linux desktop).
//!
//! # Why batches and stable ids
//!
//! The xdg-desktop-portal `GlobalShortcuts` interface is bind-once per
//! SESSION ("an application can only attempt to bind shortcuts of a session
//! once"), may show the user an approval dialog per bind, and remembers what
//! an app bound under its shortcut IDS (`ListShortcuts`). So:
//!
//! - every accelerator the manager asks for in one batch is bound in ONE new session - one
//!   dialog for N hotkeys, not N dialogs;
//! - the shortcut id is the canonical accelerator (`portal_trigger`: `CTRL+ALT+k`), so it is the
//!   same on every launch whatever order the app declares in, and the desktop recognises the
//!   shortcuts it already approved instead of listing stale ones and asking again;
//! - a shortcut cannot be unbound from a multi-shortcut session: releasing one leaves a TOMBSTONE
//!   (its activations are dropped). A session whose shortcuts are all released is closed; a
//!   partly released one is folded into the next batch's new session (its survivors are bound
//!   again there, and it is closed once that session answered).

#![cfg_attr(not(target_os = "linux"), allow(dead_code))]

use azul_core::global_hotkey::GlobalHotkey;

/// One shortcut the backend binds (or has bound) in a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedShortcut {
    /// The manager's OS id: what presses and answers are reported under.
    pub os_id: u32,
    /// The portal shortcut id - [`shortcut_id`] of the accelerator.
    pub shortcut_id: String,
    /// The `preferred_trigger` (XDG shortcuts format).
    pub trigger: String,
    /// What the desktop shows the user.
    pub description: String,
}

/// What the backend knows about one of its sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionView {
    /// The backend's own key for the session.
    pub key: u64,
    /// Its shortcuts still wanted.
    pub live: Vec<PlannedShortcut>,
    /// How many of its shortcuts were released (tombstones).
    pub released: usize,
}

/// What `commit` must do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CommitPlan {
    /// Sessions with nothing left in them: close now.
    pub close_now: Vec<u64>,
    /// Partly released sessions whose survivors are re-bound in the new
    /// session: close them once it answered (so their shortcuts keep firing
    /// while the desktop asks).
    pub close_after_bind: Vec<u64>,
    /// Everything to bind in ONE new session (empty = no new session).
    pub bind: Vec<PlannedShortcut>,
}

/// The portal shortcut id of `hotkey`: its canonical accelerator. Stable
/// across launches and declaration orders; `None` for a key the portal
/// cannot name.
pub(crate) fn shortcut_id(hotkey: &GlobalHotkey) -> Option<String> {
    azul_core::global_hotkey::portal_trigger(hotkey)
}

/// Plan the end of one batch: `sessions` as they stand, `pending` the
/// shortcuts registered since the last commit.
pub(crate) fn plan_commit(sessions: &[SessionView], pending: &[PlannedShortcut]) -> CommitPlan {
    let mut plan = CommitPlan::default();
    for session in sessions {
        if session.live.is_empty() {
            // Nothing in it is wanted any more.
            plan.close_now.push(session.key);
        } else if session.released > 0 && !pending.is_empty() {
            // A new session is made anyway: take the survivors over, so the
            // tombstones do not live on for the rest of the run.
            plan.close_after_bind.push(session.key);
            plan.bind.extend(session.live.iter().cloned());
        }
    }
    plan.bind.extend(pending.iter().cloned());
    plan
}

#[cfg(test)]
mod tests {
    use azul_core::{
        global_hotkey::{GlobalHotkey, HotkeyModifiers},
        window::VirtualKeyCode as K,
    };

    use super::*;

    fn ctrl_alt(key: K) -> GlobalHotkey {
        GlobalHotkey {
            modifiers: HotkeyModifiers {
                ctrl: true,
                alt: true,
                shift: false,
                meta: false,
            },
            key,
        }
    }

    fn shortcut(os_id: u32, key: K) -> PlannedShortcut {
        let id = shortcut_id(&ctrl_alt(key)).unwrap_or_default();
        PlannedShortcut {
            os_id,
            shortcut_id: id.clone(),
            trigger: id,
            description: String::from("test"),
        }
    }

    /// The id the desktop remembers an approval under must not depend on
    /// the order the app happened to declare in (the old `azul-hotkey-{n}`
    /// did: every launch looked like new shortcuts - fresh dialogs, stale
    /// entries in the desktop's settings).
    #[test]
    fn a_shortcut_id_is_the_accelerator_and_stable_across_launches() {
        assert_eq!(
            shortcut_id(&ctrl_alt(K::K)).as_deref(),
            Some("CTRL+ALT+k")
        );
        let respelled = GlobalHotkey::parse_for("alt + CTRL + k", false).unwrap();
        assert_eq!(shortcut_id(&respelled), shortcut_id(&ctrl_alt(K::K)));
        assert_ne!(shortcut_id(&ctrl_alt(K::J)), shortcut_id(&ctrl_alt(K::K)));
    }

    /// N new hotkeys cost ONE session - one approval dialog - not N.
    #[test]
    fn one_batch_binds_every_new_shortcut_in_one_session() {
        let pending = vec![shortcut(1, K::K), shortcut(2, K::J)];
        let plan = plan_commit(&[], &pending);
        assert_eq!(plan.bind, pending);
        assert!(plan.close_now.is_empty());
        assert!(plan.close_after_bind.is_empty());
    }

    #[test]
    fn a_session_whose_shortcuts_are_all_released_is_closed() {
        let sessions = vec![SessionView {
            key: 7,
            live: Vec::new(),
            released: 2,
        }];
        let plan = plan_commit(&sessions, &[]);
        assert_eq!(plan.close_now, vec![7]);
        assert!(plan.bind.is_empty(), "nothing to bind: no new session");
    }

    /// A shortcut cannot be unbound from a live multi-shortcut session, so
    /// the next new session takes the survivors over.
    #[test]
    fn a_partly_released_session_is_folded_into_the_next_batch() {
        let sessions = vec![SessionView {
            key: 7,
            live: vec![shortcut(1, K::K)],
            released: 1,
        }];
        let pending = vec![shortcut(3, K::J)];
        let plan = plan_commit(&sessions, &pending);
        assert_eq!(plan.bind, vec![shortcut(1, K::K), shortcut(3, K::J)]);
        assert_eq!(plan.close_after_bind, vec![7]);
        assert!(plan.close_now.is_empty());
    }

    /// Without a new batch, a tombstone costs nothing: no dialog is shown
    /// just to compact.
    #[test]
    fn a_partly_released_session_stays_while_nothing_new_is_bound() {
        let sessions = vec![SessionView {
            key: 7,
            live: vec![shortcut(1, K::K)],
            released: 1,
        }];
        assert_eq!(plan_commit(&sessions, &[]), CommitPlan::default());
    }

    /// An intact session is never touched by a later batch.
    #[test]
    fn an_intact_session_is_left_alone() {
        let sessions = vec![SessionView {
            key: 7,
            live: vec![shortcut(1, K::K)],
            released: 0,
        }];
        let pending = vec![shortcut(3, K::J)];
        let plan = plan_commit(&sessions, &pending);
        assert_eq!(plan.bind, pending);
        assert!(plan.close_now.is_empty());
        assert!(plan.close_after_bind.is_empty());
    }
}
