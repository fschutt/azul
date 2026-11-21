//! `AppConfig::app_id`: the app names itself - and where the platform has
//! already named it, the platform wins.
//!
//! `desktop::app_identity::current()` (azul-dll) reads the platform's
//! declaration (a bundle's `CFBundleIdentifier`, the Android package,
//! `FLATPAK_ID`) and hands it, with the app's `app_id` and the executable,
//! to ONE pure rule, `wire::AppIdentity::resolve`. That rule is what these
//! tests pin, without an OS:
//!
//! | OS | wins | the app's `app_id` |
//! |---|---|---|
//! | macOS / iOS | the bundle's `CFBundleIdentifier` | a different one is a warning |
//! | Android | the manifest package | a different one is a warning |
//! | Linux in Flatpak | `FLATPAK_ID` | a different one is a warning |
//! | Linux, Windows | `app_id` | names the AUMID, `desktop-entry`, Wayland `app_id`, `WM_CLASS` |
//!
//! An empty `app_id` is no declaration: the identity is what it was before
//! the field existed.

use azul_layout::managers::notification::wire::{self, AppIdSource, AppIdentity, PlatformAppId};

const EDITOR: &str = "org.example.Editor";

#[test]
fn a_declared_app_id_names_the_windows_toast_sender() {
    // An unpackaged exe: Windows declares nothing, so the app's id is the
    // AUMID the toast is attributed to (Settings > Notifications lists it).
    let exe = "C:\\Program Files\\Editor\\Editor.exe";
    let resolved = AppIdentity::resolve(EDITOR, None, exe);
    assert_eq!(resolved.warning, None, "nothing overrode the app's id");
    let app = resolved.identity;
    assert_eq!(app.source, AppIdSource::Declared);
    assert_eq!(
        app.windows_aumid(),
        EDITOR,
        "the toast AUMID is the app's own id, not com.azul.editor"
    );
    assert_eq!(app.display_name(), "Editor");

    let values = wire::toast_registry_values(&app.windows_aumid(), &app.display_name(), exe);
    assert_eq!(
        values[0].key,
        wire::aumid_registry_key(EDITOR),
        "the HKCU registration is under the app's id"
    );
    assert_eq!(
        values[1].data,
        wire::guid_string(wire::toast_activator_clsid(EDITOR)),
        "the COM activator's class follows the AUMID"
    );
}

#[test]
fn a_declared_app_id_names_the_desktop_entry_outside_flatpak() {
    // A Linux app outside a sandbox: the app's id is the `.desktop` file a
    // notification server looks up, the Wayland `app_id` and the X11
    // `WM_CLASS` instance - one string, so they all find the same file.
    let resolved = AppIdentity::resolve(EDITOR, None, "/usr/bin/editor");
    assert_eq!(resolved.warning, None);
    assert_eq!(resolved.identity.desktop_entry(), EDITOR);
    assert_eq!(resolved.identity.display_name(), "editor");
}

#[test]
fn the_bundle_id_wins_over_a_declared_app_id_on_macos() {
    // UN, TCC and LaunchServices key on the bundle's id, and the app cannot
    // change it at run time - so the app runs under it, and is told.
    let bundle = PlatformAppId::AppleBundle("com.acme.editor".to_string());
    let resolved = AppIdentity::resolve(
        EDITOR,
        Some(&bundle),
        "/Applications/Editor.app/Contents/MacOS/Editor",
    );
    assert_eq!(resolved.identity.id, "com.acme.editor");
    assert_eq!(resolved.identity.source, AppIdSource::Declared);
    let warning = resolved
        .warning
        .expect("an app_id the bundle overrode is reported");
    assert!(
        warning.contains(EDITOR) && warning.contains("com.acme.editor"),
        "the warning names both ids: {warning}"
    );
}

#[test]
fn a_bundle_id_that_is_the_app_ids_apple_form_warns_nothing() {
    let exe = "/Applications/Editor.app/Contents/MacOS/Editor";
    let same = PlatformAppId::AppleBundle(EDITOR.to_string());
    assert_eq!(AppIdentity::resolve(EDITOR, Some(&same), exe).warning, None);
    // A CFBundleIdentifier has no underscore: the bundle carries the app
    // id's Apple form, which is the same name.
    let apple_form = PlatformAppId::AppleBundle("org.example.my-app".to_string());
    let resolved = AppIdentity::resolve("org.example.my_app", Some(&apple_form), exe);
    assert_eq!(resolved.identity.id, "org.example.my-app");
    assert_eq!(resolved.warning, None);
}

#[test]
fn the_manifest_package_wins_over_a_declared_app_id_on_android() {
    let package = PlatformAppId::AndroidPackage("com.acme.editor".to_string());
    let resolved = AppIdentity::resolve(EDITOR, Some(&package), "/system/bin/app_process64");
    assert_eq!(resolved.identity.id, "com.acme.editor");
    let warning = resolved
        .warning
        .expect("an app_id the package overrode is reported");
    assert!(
        warning.contains(EDITOR) && warning.contains("com.acme.editor"),
        "{warning}"
    );

    let same = PlatformAppId::AndroidPackage(EDITOR.to_string());
    assert_eq!(
        AppIdentity::resolve(EDITOR, Some(&same), "/system/bin/app_process64").warning,
        None
    );
}

#[test]
fn flatpak_id_wins_inside_the_sandbox() {
    // The sandbox is authoritative: the portal attributes notifications to
    // FLATPAK_ID and the exported `.desktop` file is named after it.
    let sandbox = PlatformAppId::Flatpak("rs.azul.Widgets".to_string());
    let resolved = AppIdentity::resolve(EDITOR, Some(&sandbox), "/app/bin/widgets");
    assert_eq!(resolved.identity.id, "rs.azul.Widgets");
    assert_eq!(
        resolved.identity.desktop_entry(),
        "rs.azul.Widgets",
        "the Wayland app_id and the desktop-entry hint name the sandbox's .desktop file"
    );
    let warning = resolved.warning.expect("a different app_id is reported");
    assert!(
        warning.contains(EDITOR) && warning.contains("rs.azul.Widgets"),
        "{warning}"
    );
}

#[test]
fn an_empty_app_id_keeps_todays_identity() {
    // No platform declaration: the id derived from the executable.
    for exe in ["/usr/bin/AzWidgets", "C:\\x\\AzWidgets.exe", ""] {
        let resolved = AppIdentity::resolve("", None, exe);
        assert_eq!(
            resolved.identity,
            AppIdentity::from_executable(exe),
            "{exe:?}"
        );
        assert_eq!(resolved.warning, None);
        assert_eq!(
            AppIdentity::resolve("   ", None, exe).identity,
            AppIdentity::from_executable(exe),
            "a blank app_id is no declaration"
        );
    }
    // A platform declaration: exactly the identity it gave before.
    let exe = "/Applications/Az.app/Contents/MacOS/AzWidgets";
    for platform in [
        PlatformAppId::AppleBundle("com.azul.azwidgets".to_string()),
        PlatformAppId::AndroidPackage("com.azul.azwidgets".to_string()),
        PlatformAppId::Flatpak("rs.azul.Widgets".to_string()),
    ] {
        let resolved = AppIdentity::resolve("", Some(&platform), exe);
        assert_eq!(
            resolved.identity,
            AppIdentity::declared(platform.id(), exe),
            "{platform:?}"
        );
        assert_eq!(resolved.warning, None, "no app_id, nothing overridden");
    }
}

#[test]
fn an_empty_platform_declaration_leaves_the_app_id_in_charge() {
    // An unbundled macOS binary, a process whose package could not be read:
    // the platform named nothing, so the app's own id stands.
    let nothing = PlatformAppId::AppleBundle(String::new());
    let resolved = AppIdentity::resolve(EDITOR, Some(&nothing), "/target/release/editor");
    assert_eq!(resolved.identity.id, EDITOR);
    assert_eq!(resolved.warning, None);
}
