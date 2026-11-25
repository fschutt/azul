//! The desktop state a script says and a headless run reports. No desktop.

use super::*;

fn bundle(s: &DesktopState) -> Option<&str> {
    match &s.front_bundle {
        OptionString::Some(b) => Some(b.as_str()),
        OptionString::None => None,
    }
}

#[test]
fn a_script_becomes_the_state_it_says() {
    let s = DesktopState::from_script("front=com.apple.Safari|Safari\npointer=100.5,200\nlocked=1\nasleep=1\n");
    assert_eq!(bundle(&s), Some("com.apple.Safari"));
    assert!(matches!(&s.front_name, OptionString::Some(n) if n.as_str() == "Safari"));
    assert_eq!(s.pointer, OptionScreenPosition::Some(ScreenPosition::new(100.5, 200.0)));
    assert!(s.locked && s.display_asleep);
}

#[test]
fn what_nobody_says_is_not_known_and_the_screen_is_not_locked() {
    assert_eq!(DesktopState::from_script(""), DesktopState::UNKNOWN);
    assert_eq!(DesktopState::headless(), DesktopState::HEADLESS);
    assert_eq!(DesktopState::HEADLESS, DesktopState::UNKNOWN);
    assert!(!DesktopState::UNKNOWN.locked && !DesktopState::UNKNOWN.display_asleep);
}

#[test]
fn a_file_that_is_not_there_is_the_headless_state_and_one_that_is_there_is_read_each_time() {
    assert_eq!(DesktopState::of_file(None), DesktopState::HEADLESS);
    let dir = std::env::temp_dir().join(format!("azul-desktop-state-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("desktop.txt");
    assert_eq!(DesktopState::of_file(Some(&file)), DesktopState::HEADLESS, "no such file");
    std::fs::write(&file, "front=org.example.One\n").unwrap();
    assert_eq!(bundle(&DesktopState::of_file(Some(&file))), Some("org.example.One"));
    std::fs::write(&file, "front=org.example.Two\nlocked=on\n").unwrap();
    let again = DesktopState::of_file(Some(&file));
    assert_eq!(bundle(&again), Some("org.example.Two"), "a test switches the app in front by writing the file");
    assert!(again.locked);
    let _ = std::fs::remove_dir_all(&dir);
}

/// On a Mac the real reading answers with what the desktop has: some app is in front, the pointer is
/// somewhere on the desktop, and a person who runs the test is neither locked out nor asleep.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "X-EDGE-C-2: reads the real desktop of the machine it runs on, run by hand: cargo test -p azul-dll --lib the_real_desktop_of_a_mac -- --ignored"]
fn the_real_desktop_of_a_mac_has_an_app_in_front_and_a_pointer() {
    let s = platform::read();
    assert!(bundle(&s).is_some_and(|b| b.contains('.')), "{s:?}");
    assert!(matches!(s.pointer, OptionScreenPosition::Some(p) if p.x.is_finite() && p.y.is_finite()), "{s:?}");
}
