//! What the platform's answer about notifications means for the alarms. No window.

use super::os_alarms;

#[test]
fn a_platform_that_rings_by_itself_gets_the_alarms() {
    // Android, macOS, Windows: the system shows a notification at its time, closed or not.
    let (can_notify, (rings, how)) = os_alarms(true, "NotificationManager", "", false);
    assert!(can_notify);
    assert!(rings);
    assert_eq!(how, "NotificationManager", "the line says how");
}

#[test]
fn a_desktop_that_only_shows_notifications_does_not_ring_while_closed() {
    let (can_notify, (rings, _)) = os_alarms(true, "org.freedesktop.Notifications", "", true);
    assert!(can_notify, "it can post one");
    assert!(!rings, "but the notification server does not wake a closed app");
}

#[test]
fn an_unavailable_platform_says_why_and_posts_nothing() {
    let (can_notify, (rings, why)) = os_alarms(false, "NotificationManager", "no activity yet", false);
    assert!(!can_notify);
    assert!(!rings);
    assert_eq!(why, "NotificationManager: no activity yet");
}

#[test]
fn a_headless_run_still_posts_so_the_scripts_see_the_plan() {
    let (can_notify, (rings, _)) = os_alarms(false, "headless-test", "no display", false);
    assert!(can_notify);
    assert!(!rings);
}
