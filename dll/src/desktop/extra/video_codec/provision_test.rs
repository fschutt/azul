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

/// A codec the OS ships but the build cannot create is NOT available: the probe answers from the
/// engine behind `VideoDecoder::open` / `VideoEncoder::open`, so `VideoStartupCheck` never says
/// ready for a handle that opens invalid (Android, before its MediaCodec backend: the arm said
/// available whatever the build had).
#[test]
fn a_codec_the_build_cannot_create_is_not_available_whatever_the_os_ships() {
    let none = probe_of_engine(
        "MediaCodec",
        Err(String::from("the MediaCodec backend is not implemented yet")),
    );
    assert!(!none.available);
    assert_eq!(none.backend, "MediaCodec");
    assert!(none.detail.contains("not implemented"), "{}", none.detail);
    assert!(!none.can_remediate, "no driver install adds a system codec");

    let some = probe_of_engine(
        "MediaCodec",
        Ok(String::from("MediaCodec (c2.android.avc.decoder, software)")),
    );
    assert!(some.available);
    assert!(some.detail.contains("c2.android.avc.decoder"), "{}", some.detail);
}

/// The Android arms of both hardware probes answer from the build's own engine check and never
/// with a literal yes: a source-level invariant (the arms only compile for Android), so the next
/// edit cannot bring back "available: true" for a service the build does not reach.
#[test]
fn the_android_arms_of_the_hardware_probes_ask_the_build_s_engine() {
    let source = include_str!("provision.rs");
    for (probe, engine) in [
        ("pub fn probe_hw_decode()", "super::decode_engine()"),
        ("fn probe_hw_encode()", "super::encode_engine()"),
    ] {
        let from = source.find(probe).unwrap_or_else(|| panic!("{probe} moved"));
        let body = &source[from..];
        let arm_start = body
            .find("#[cfg(target_os = \"android\")]")
            .unwrap_or_else(|| panic!("{probe} has no android arm"));
        let arm = &body[arm_start..];
        let arm_end = arm
            .find("#[cfg(any(target_os = \"linux\"")
            .unwrap_or_else(|| panic!("{probe}: the arm after android moved"));
        let arm = &arm[..arm_end];
        assert!(
            arm.contains(engine) && arm.contains("probe_of_engine"),
            "the android arm of {probe} must answer from {engine}:\n{arm}"
        );
        assert!(
            !arm.contains("available: true") && !arm.contains("            true,"),
            "the android arm of {probe} says yes without asking the engine:\n{arm}"
        );
    }
}
