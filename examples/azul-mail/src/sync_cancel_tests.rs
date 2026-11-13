//! Cancel on Send / Receive: the window is free at once, the status bar says it stopped, and the
//! stopped sync's late reports (it writes what it has, then says so) never touch the Send /
//! Receive that runs after it.

use azul::task::ThreadId;

use crate::SyncState;

fn running(id: usize) -> SyncState {
    SyncState::Running {
        thread: ThreadId { id },
        account: String::from("ada"),
        status: String::new(),
        percent: 0.0,
    }
}

#[test]
fn cancel_frees_send_receive_at_once() {
    let mut state = running(7);
    let stopped = state.cancel();
    assert!(stopped.is_some_and(|t| t.id == 7), "the running thread is the one to stop");
    assert!(
        !matches!(state, SyncState::Running { .. }),
        "Send / Receive must not wait for the stopped thread's last word"
    );
    assert!(state.cancel().is_none(), "nothing runs any more");
}

#[test]
fn a_stopped_syncs_last_report_does_not_touch_the_next_one() {
    let mut state = running(7);
    state.cancel();
    assert!(!state.runs_on(ThreadId { id: 7 }), "the stopped sync is no longer the window's");
    let next = running(8);
    assert!(!next.runs_on(ThreadId { id: 7 }), "the stopped sync's late report");
    assert!(next.runs_on(ThreadId { id: 8 }));
}

#[test]
fn a_stopped_send_receive_says_so_in_the_status_bar() {
    crate::l10n::in_english();
    let mut state = running(7);
    state.cancel();
    let (line, _) = crate::ui_main::sync_line(&state, false, || {
        String::from("All folders are up to date.")
    });
    assert_eq!(
        line,
        azul_appkit::l10n::t("azmail-send-receive-stopped"),
        "a stopped Send / Receive is no up-to-date one: the folders hold only what arrived"
    );
}

#[test]
fn a_send_receive_waiting_for_the_keyring_says_what_it_waits_for() {
    crate::l10n::in_english();
    let waiting = SyncState::Waiting(azul_appkit::l10n::t("azmail-reading-password"));
    let (line, _) = crate::ui_main::sync_line(&waiting, false, || {
        String::from("All folders are up to date.")
    });
    assert_eq!(line, azul_appkit::l10n::t("azmail-reading-password"));
}
