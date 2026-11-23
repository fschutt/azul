use super::*;

#[test]
fn a_program_is_found_in_a_directory_of_path_and_nowhere_else() {
    let tag = std::process::id();
    let dir = std::env::temp_dir().join(format!("azul-which-{tag}"));
    let other = std::env::temp_dir().join(format!("azul-which-other-{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("azul-tool"), b"").unwrap();

    let path = std::env::join_paths([other.as_path(), dir.as_path()]).unwrap();
    assert!(which_in(Some(&path), "azul-tool"), "the second directory has it");
    assert!(!which_in(Some(&path), "azul-missing"));
    assert!(!which_in(None, "azul-tool"), "no PATH at all");
    assert!(
        !which_in(Some(std::ffi::OsStr::new("")), "azul-tool"),
        "an empty PATH holds nothing"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(target_os = "linux")]
fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_string()).collect()
}

#[cfg(target_os = "linux")]
#[test]
fn the_session_of_the_current_desktop_wins() {
    let sessions = names(&["xfce", "plasmax11", "gnome-xorg"]);
    assert_eq!(
        pick_x11_session(sessions.clone(), Some("XFCE")),
        Some("xfce".to_string())
    );
    assert_eq!(
        pick_x11_session(sessions, Some("gnome-xorg:GNOME")),
        Some("gnome-xorg".to_string()),
        "the session name inside the desktop list"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn without_a_match_the_known_desktops_come_first_then_the_alphabet() {
    let sessions = names(&["xfce", "plasmax11", "gnome-xorg"]);
    assert_eq!(
        pick_x11_session(sessions.clone(), Some("KDE")),
        Some("plasmax11".to_string()),
        "no session is named like the desktop: the preferred list decides"
    );
    assert_eq!(
        pick_x11_session(sessions, None),
        Some("plasmax11".to_string())
    );
    assert_eq!(
        pick_x11_session(names(&["zeta", "alpha"]), None),
        Some("alpha".to_string()),
        "unknown sessions only: the first by name"
    );
    assert_eq!(pick_x11_session(Vec::new(), Some("KDE")), None);
}
