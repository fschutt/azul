//! What the sidebar's status slot and quick-access buttons say and do, as far as it needs no
//! window.

use azul::widgets::{PieTone, StatusBarSyncKind};

use super::*;

#[test]
fn a_running_send_receive_is_a_busy_pie_as_full_as_it_is_far() {
    assert_eq!(sync_pie(StatusBarSyncKind::Syncing, Some(40.0)), Some((0.4, PieTone::Busy)));
    assert_eq!(sync_pie(StatusBarSyncKind::Syncing, Some(150.0)), Some((1.0, PieTone::Busy)), "held to full");
    assert_eq!(sync_pie(StatusBarSyncKind::Syncing, Some(-3.0)), Some((0.0, PieTone::Busy)));
}

#[test]
fn a_send_receive_that_does_not_say_how_far_is_half_a_busy_pie() {
    assert_eq!(sync_pie(StatusBarSyncKind::Syncing, None), Some((0.5, PieTone::Busy)));
    assert_eq!(sync_pie(StatusBarSyncKind::Syncing, Some(f32::NAN)), Some((0.5, PieTone::Busy)));
    assert_eq!(sync_pie(StatusBarSyncKind::Syncing, Some(f32::INFINITY)), Some((0.5, PieTone::Busy)));
}

#[test]
fn a_finished_send_receive_is_a_full_pie_green_or_red_and_no_account_is_none() {
    assert_eq!(sync_pie(StatusBarSyncKind::Connected, None), Some((1.0, PieTone::Good)));
    assert_eq!(sync_pie(StatusBarSyncKind::Error, None), Some((1.0, PieTone::Danger)));
    assert_eq!(sync_pie(StatusBarSyncKind::Offline, None), None, "nothing is connected: nothing to say");
}

#[test]
fn every_quick_button_runs_its_own_command_and_compose_is_new_mail() {
    assert_eq!(quick_command(Quick::SendReceive), Action::SendReceive);
    assert_eq!(quick_command(Quick::ToggleFlag), Action::ToggleFlag);
    assert_eq!(quick_command(Quick::ToggleRead), Action::ToggleRead);
    assert_eq!(quick_command(Quick::Compose), Action::NewMail);
    // and every button of the plan has one
    for quick in nav::QUICK {
        let _ = quick_command(quick);
    }
}

#[test]
fn the_text_beside_the_pie_is_short_and_empty_with_no_pie() {
    crate::l10n::in_english();
    assert_eq!(sync_text(StatusBarSyncKind::Syncing, Some(41.6)), "42 %");
    assert_eq!(sync_text(StatusBarSyncKind::Syncing, Some(250.0)), "100 %");
    assert_eq!(sync_text(StatusBarSyncKind::Syncing, None), "");
    assert_eq!(sync_text(StatusBarSyncKind::Connected, None), "Up to date");
    assert_eq!(sync_text(StatusBarSyncKind::Error, None), "Failed");
    assert_eq!(sync_text(StatusBarSyncKind::Offline, None), "");
}
