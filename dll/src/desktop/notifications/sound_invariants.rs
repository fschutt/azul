//! What a notification SOUNDS like, pinned in the source of the platform code a host cannot run
//! (the Java in the APK, the Objective-C calls of the iOS backend).
//!
//! The sound of a notification is the notification's own: it is decided by the channel on Android
//! and by the content on iOS, not by the app's `play_system_sound`. The facts below are the ones
//! the system-sounds work checked on 2026-11-28; each pins the shape whose absence would be a
//! notification without its sound, or an app that cannot be silenced.

const AZUL_NOTIFICATIONS_JAVA: &str =
    include_str!("../../../../scripts/android/AzulNotifications.java");
const APPLE_RS: &str = include_str!("apple.rs");

/// The text of `source` from `start` to the next member of the class (a javadoc block).
fn java_member<'a>(source: &'a str, start: &str) -> &'a str {
    let from = source
        .find(start)
        .unwrap_or_else(|| panic!("no `{start}` in the Java source"));
    let rest = &source[from..];
    let end = rest[start.len()..]
        .find("\n    /**")
        .map_or(rest.len(), |e| e + start.len());
    &rest[..end]
}

/// The default channel is `IMPORTANCE_DEFAULT` (sound, status bar, no heads-up) and takes the sound
/// the user chose for notifications, which is what a new channel carries when no sound is set; the
/// silent one is `IMPORTANCE_LOW`. A channel's sound cannot change after creation, so "silent" is a
/// channel of its own and the default channel is never given a sound of the app's.
#[test]
fn the_android_default_channel_has_the_users_sound_and_the_silent_one_is_low_importance() {
    let channel = java_member(
        AZUL_NOTIFICATIONS_JAVA,
        "private static String ensureChannel(",
    );
    assert!(
        channel.contains("NotificationManager.IMPORTANCE_DEFAULT"),
        "the default channel must be IMPORTANCE_DEFAULT: {channel}"
    );
    assert!(
        channel.contains("NotificationManager.IMPORTANCE_LOW"),
        "the silent channel must be IMPORTANCE_LOW: {channel}"
    );
    assert!(
        channel.contains("silent ? CHANNEL_SILENT : CHANNEL_DEFAULT"),
        "silent is a channel of its own: {channel}"
    );
    assert!(
        !channel.contains("setSound("),
        "a channel that sets a sound of its own can never be changed back to the user's: {channel}"
    );
}

/// From API 26 the channel decides the sound; `setDefaults(DEFAULT_SOUND ...)` is for the phones
/// before it and must not be the only thing that gives a notification its sound.
#[test]
fn the_android_sound_flag_of_a_notification_is_for_the_phones_before_channels() {
    let post = java_member(AZUL_NOTIFICATIONS_JAVA, "public static String post(");
    let channels = post
        .find("Build.VERSION.SDK_INT >= 26")
        .expect("post() branches on the API level of the channels");
    let defaults = post
        .find("setDefaults(")
        .expect("post() gives a pre-26 notification its default sound");
    let else_branch = post[channels..]
        .find("} else {")
        .map(|e| e + channels)
        .expect("the pre-26 branch");
    assert!(
        defaults > else_branch,
        "setDefaults belongs to the pre-26 branch: the channel decides from API 26"
    );
    assert!(
        post.contains("silent ? 0"),
        "a silent notification gets no default sound: {post}"
    );
}

/// iOS: a notification has `UNNotificationSound.defaultSound` unless it is silent, the app asks the
/// user for the sound authorization with the alert one, and a notification that arrives while the
/// app is in front is presented with its sound too.
#[test]
fn the_ios_notification_has_the_default_sound_unless_it_is_silent() {
    assert!(
        APPLE_RS.contains("(NotificationSound::Silent, _) | (_, None) => core::ptr::null_mut()"),
        "a silent notification has no sound object"
    );
    assert!(
        APPLE_RS
            .contains("(NotificationSound::Default, Some(cls)) => msg_send![cls, defaultSound]"),
        "the default notification has UNNotificationSound.defaultSound"
    );
    assert!(
        APPLE_RS.contains("const AUTH_SOUND_ALERT: usize = (1 << 1) | (1 << 2);"),
        "the authorization asks for the sound (UNAuthorizationOptionSound is bit 1) and the alert"
    );
    assert!(
        APPLE_RS.contains(
            "const PRESENT_SOUND_ALERT_LIST_BANNER: usize = (1 << 1) | (1 << 2) | (1 << 3) | (1 << 4);"
        ),
        "a notification that arrives with the app in front keeps its sound"
    );
}
