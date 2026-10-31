//! The Options' Cancel puts back what the pages changed, never what was done from them: a
//! recovery finished (or noted pending, a drill passed, a share made) happened at the token
//! server and on this computer, and forgetting it would make AzDrive show a drive it holds as
//! still waiting to be recovered.

use crate::{model::Settings, recovery_health::PendingRecovery, settings_after_cancel};

#[test]
fn cancel_puts_back_the_pages_settings_and_keeps_a_recovery_done_meanwhile() {
    let mut found = Settings::default();
    found.recovery.pending.push(PendingRecovery {
        drive_id: String::from("d_1"),
        token_url: String::from("http://127.0.0.1:1"),
        until: Some(100),
        started: 1,
    });
    // While the Options showed: a page's setting changed, and the recovery finished.
    let mut now = found.clone();
    now.show_hidden = !found.show_hidden;
    now.recovery.pending.clear();

    let back = settings_after_cancel(found.clone(), &now);
    assert_eq!(back.show_hidden, found.show_hidden, "the page's change is put back");
    assert!(back.recovery.pending.is_empty(), "the finished recovery stays finished");
}
