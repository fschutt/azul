//! The Java half of `WindowFlags::system_bars` (the status and navigation bar modes, the icon colour, drawing into the cutout area)
//! and the shell's call into it, pinned in source (ledger: the TABS agent's contract, item 5).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_system_bars_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn java_code(rel: &str) -> String {
    read(rel).lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
}

/// Rust source without its comments and without whitespace.
fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}`"));
    let open = start + src[start..].find('{').unwrap();
    let mut depth = 0;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..open + i + 1];
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces after `{signature}`");
}

/// A hidden bar is shown again for a moment by a swipe (transient) or until the app hides it (sticky), the window draws into the cutout
/// area with `ALWAYS` from Android 11 and `SHORT_EDGES` before, edge to edge, only when asked; fixed icon colours stand the shell's own
/// pick down.
///
/// Mutation: `layoutInDisplayCutoutMode` set whatever `drawIntoCutout` says (the status of `extend_into_safe_area` alone changed the cutout).
#[test]
fn the_activity_applies_the_bars_and_the_cutout_the_window_asks_for() {
    let java = java_code("scripts/android/AzulActivity.java");
    let apply = body(&java, "public static void applySystemBars(");
    assert!(apply.contains("if (drawIntoCutout && android.os.Build.VERSION.SDK_INT >= 28)"), "the cutout mode only when asked, from Android 9");
    assert!(apply.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_ALWAYS") && apply.contains("LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES"));
    assert!(apply.contains("window.setDecorFitsSystemWindows(false)"), "edge to edge");
    assert!(apply.contains("BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE") && apply.contains("BEHAVIOR_DEFAULT"), "transient and sticky");
    assert!(apply.contains("controller.hide(type)") || java.contains("controller.hide(type);"), "a hidden bar is hidden by the controller");
    assert!(apply.contains("if (icons != 0)"), "Auto leaves the colour of the icons to the shell");
    assert!(apply.contains("runOnUiThread"));
}

/// The shell applies the window's options when they are new or changed (the Android `sync_window_state` did nothing), not on every pass,
/// and a fixed icon colour stops the shell's own pick.
#[test]
fn the_shell_applies_the_options_on_change_and_lets_fixed_icons_stand() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let sync = body(&shell, "fnsync_window_state(&mutself){");
    assert!(sync.contains("apply_system_bars_if_changed(self);"));
    let arm = shell.find("MainEvent::InitWindow{..}=>{").expect("the surface-created arm") + "MainEvent::InitWindow{..}".len();
    assert!(
        body(&shell[arm..], "=>").contains("apply_system_bars_if_changed(window);"),
        "and the window's options are applied when it is created, before the app's create callback"
    );
    let change = body(&shell, "fnapply_system_bars_if_changed(window:&mutAndroidWindow){");
    assert!(change.contains("ifwindow.applied_system_bars==Some(wanted){return;}"), "only a change is applied");
    assert!(change.contains("window.bars_icons_fixed=wanted.icons!=azul_core::display_features::SystemBarIcons::Auto;"));
    assert!(shell.contains("bars_to_tell.filter(|_|!window.bars_icons_fixed)"), "fixed icons stand the shell's pick down");
    let call = body(&shell, "fnapply_system_bars(status:i32,navigation:i32,icons:i32,draw_into_cutout:bool){");
    assert!(call.contains("\"applySystemBars\"") && call.contains("\"(Landroid/app/Activity;IIIZ)V\""));
}
