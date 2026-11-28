//! What the Android cold start shows, the engine's half of it, pinned in source (a phone cannot run where the tests run; the lab that measures it is
//! `scripts/ci/android_startup_lab.sh`, its numbers are in the engine ledger rows E46 / E47 / E60): no black between the system's
//! starting window and the first picture, no picture that is the wrong colour or in the wrong place and moves a moment later.
//!
//! The mechanism behind all of it: a NativeActivity's window is a surface the engine draws on itself (`Window.takeSurface`), the view
//! system never draws it, so the system takes its starting window away as soon as the window has "drawn" (at once), before the
//! surface has a single buffer. What the engine knows that late (the insets, the night mode) is known late; what it posts first is
//! posted late.
//!
//! The Java half is `dll/tests/android_startup_contract.rs`.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_startup_engine_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Rust source without its line comments and without whitespace (rustfmt breaks the calls over lines).
fn rust_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<String>()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// The body of a method or function of `src` starting at `signature` (up to its matching brace).
fn body<'a>(src: &'a str, signature: &str) -> &'a str {
    let start = src.find(signature).unwrap_or_else(|| panic!("no `{signature}` in the source"));
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


/// The icon colour of the system bars is told BEFORE the activity hears that the first picture is posted: the window draws (and the starting
/// window leaves) after that, and the bars are already the colour this picture needs. Told after it, the clock and the battery were white
/// on a white page for the frames between.
///
/// Mutation: the two calls swapped (the state of 98d415cb4e).
#[test]
fn the_bars_are_told_before_the_window_may_draw() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let render = body(&shell, "fnrender_frame(window:&mutAndroidWindow)->Result<(),WindowError>{");
    let bars = render.find("set_system_bars_appearance(status_light,navigation_light);").expect("the bars are told after a present");
    let first = render.find("window.first_frame_posted();").expect("the first picture is announced");
    assert!(bars < first, "the bars first, then the first picture");
}

/// The engine asks Java for the window's first insets when its surface is created and applies them before the first layout: the surface comes
/// after the window's first dispatch, so the answer is there.
///
/// Mutation: the snapshot applied after the layout is requested; the engine not asking at the surface.
#[test]
fn the_windows_first_insets_are_applied_before_the_first_layout() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let arm = shell.find("MainEvent::InitWindow{..}=>{").expect("the surface-created arm") + "MainEvent::InitWindow{..}".len();
    let init = body(&shell[arm..], "=>");
    let size = init.find("apply_surface_size(window,(logical_w,logical_h));").expect("the surface's size is applied");
    let insets = init.find("apply_early_insets(window);").expect("the first insets are applied at the surface");
    let layout = init.find("window.common.request_regeneration(reason)").expect("the layout is requested");
    assert!(size < insets && insets < layout, "the insets are applied after the size and before the layout is requested");
    let asks = body(&shell, "fnearly_insets()->Option<Vec<i32>>{");
    assert!(asks.contains("\"com/azul/text/NativeTextBridge\"") && asks.contains("\"insetsSnapshot\""), "the engine asks Java through JNI");
}

/// The activity's NDK configuration says at once whether the phone is in its dark mode; the style `App::create` made says light for every
/// phone, and the Java side tells the mode only from the first window focus on, after the first picture: a phone in its dark mode showed a
/// white first picture, then the dark one. The window is made in the phone's mode.
///
/// Mutation: the mode read after the window is made (the state of 98d415cb4e: the first picture is the light one).
#[test]
fn the_phones_night_mode_is_the_windows_from_its_first_picture() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let main = body(&shell, "pubfnandroid_main(app:AndroidApp){");
    let night = main.find("ifletSome(mode)=night_mode_of(&app){config.system_style.mode=mode;}").expect("the phone's mode goes into the style");
    let window = main.find("AndroidWindow::new(").expect("the window is made");
    assert!(night < window, "the style carries the mode before the window takes it");
    let of = body(&shell, "fnnight_mode_of(app:&AndroidApp)->Option<azul_core::window::DarkLightMode>{");
    assert!(of.contains("app.config().ui_mode_night()"), "from the activity's configuration");
    assert!(of.contains("UiModeNight::Yes=>Some(azul_core::window::DarkLightMode::Dark)"));
    assert!(of.contains("UiModeNight::No=>Some(azul_core::window::DarkLightMode::Light)"));
}

/// A screen reader reads the CACHED tree: the Java bridge answers every question from the snapshot the layout published, and nothing tells the screen
/// reader that a tree changed. The first tree is therefore published by the layout pass that makes the first picture, so that it is there when the
/// window first draws (a frame or two after the picture is posted) and the screen reader asks. A first tree built after the picture (30-45 ms later,
/// E33's `a11y_tree_first_built`) races that first draw: a screen reader that asks in between is told the window is empty and is not told otherwise.
/// What the hold gains is a picture without black, not those milliseconds.
///
/// Mutation: the first layout owing its tree to a later iteration of the loop (an `a11y_first_owed` that the layout sets until a picture is out).
#[test]
fn the_first_accessibility_tree_is_published_by_the_layout_that_makes_the_first_picture() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    assert!(!shell.contains("a11y_first_owed"), "no tree is owed to a later iteration of the loop");
    let layout = body(&shell, "pubfnregenerate_layout_inner(");
    let publish = layout.find("#[cfg(feature=\"a11y\")]self.refresh_accessibility_tree();").expect("the layout pass publishes its tree");
    assert!(
        !layout[publish..].starts_with("#[cfg(feature=\"a11y\")]{") && !layout.contains("first_frame_signalled"),
        "whether or not a picture is out"
    );
    assert!(publish < layout.find("self.common.clear_regeneration_unless_reraised").expect("the pass retires its request"));
}

/// The Java side hands the engine the insets it has kept again when its bridges attach (`NativeTextBridge.installInsetsListener`), and a dispatch
/// the system repeats (the keyboard's animation, a focus change) says the same numbers: insets the layout already has are no pass. Without the early
/// return the first insets of the surface, applied before the first layout, were applied once more a moment after the first picture and the engine
/// laid everything out a second time for the same numbers (0.3 s on the widget gallery).
///
/// Mutation: `apply_insets` asking for a regeneration whatever the numbers (the state of 98d415cb4e).
#[test]
fn insets_the_layout_already_has_are_no_layout_pass() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let apply = body(&shell, "fnapply_insets(window:&mutAndroidWindow,px:[i32;5]){");
    let same = apply
        .find("ifwindow.inset_px==px&&window.common.layout_window.as_ref().is_some_and(|lw|lw.safe_area_insets==insets){return;}")
        .expect("insets that equal the ones the layout has return before anything is asked for");
    let asks = apply.find("request_regeneration(RelayoutReason::RefreshDom)").expect("a change of the insets is a pass");
    assert!(same < asks, "the equality is decided before the regeneration is requested");
    let early = body(&shell, "fnapply_early_insets(window:&mutAndroidWindow){");
    assert!(
        early.contains("request_regeneration(RelayoutReason::Initial)"),
        "the first layout keeps the reason `Initial` the app's layout callback reads: a change of the insets or of the display features raises its own reason over it"
    );
}

/// AzWriter on a phone hides the status bar and draws into the cutout area, and its strip takes the bar's place only while the bar is hidden
/// (`ribbon::status_band`): the insets of the window's first dispatch are the window BEFORE that request, so a first layout made for them is another
/// page (an ordinary row under the bar) that the request's answer then replaces. An app that asks the system to hide a bar or to draw into the cutout
/// area keeps the old path for its first layout (no snapshot: the insets come after the request, as before); an app that asks for nothing gets them.
///
/// Mutation: the snapshot applied for every app (AzWriter's first picture is the wrong page and moves).
#[test]
fn an_app_that_asks_for_the_bars_to_change_does_not_get_the_snapshot_that_precedes_the_request() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let arm = shell.find("MainEvent::InitWindow{..}=>{").expect("the surface-created arm") + "MainEvent::InitWindow{..}".len();
    let init = body(&shell[arm..], "=>");
    let gate = init.find("ifearly_insets_fit_the_request(window){apply_early_insets(window);}").expect("the snapshot is applied only when it fits the request");
    let layout = init.find("window.common.request_regeneration(reason)").expect("the layout is requested");
    let request = init[layout..].find("apply_system_bars_if_changed(window);").expect("the request goes to the activity") + layout;
    assert!(gate < layout && layout < request, "the gate comes before the layout, the request after it as it always did");

    let fit = body(&shell, "fnearly_insets_fit_the_request(window:&AndroidWindow)->bool{");
    assert!(
        fit.contains("bars_request_moves_insets(wanted.status asi32!=0,wanted.navigation asi32!=0,wanted.draw_into_cutout)".replace(' ', "").as_str()),
        "a hidden bar or the cutout area moves the insets"
    );
    assert!(fit.trim_end().ends_with("!moves}"), "and then the snapshot does not fit");
}

/// The system reports "resized" at every relayout of the window that touches its surface, most of them with the size the window already has (the
/// first draw going ahead after the held first draw, a system bar's appearance, the keyboard): the engine laid everything out again for each, 0.3 to
/// 1.2 s of its time right after the first picture, for nothing (the startup timeline's `reason=Resize` passes after `first_present`, with the same
/// 1080x2400 on the record). Only another size is a layout.
///
/// Mutation: the layout requested whatever the size (the state of 98d415cb4e).
#[test]
fn a_resize_that_names_the_size_the_window_has_asks_for_no_layout() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let arm = shell.find("MainEvent::WindowResized{..}=>{").expect("the resized arm") + "MainEvent::WindowResized{..}".len();
    let resized = body(&shell[arm..], "=>");
    let differs = resized.find("letdiffers=surface_size_differs(window,logical);").expect("the size is compared with the window's own");
    let owed = resized.find("ifowed{").expect("the pass is owed only for another size");
    let layout = resized.find("window.common.request_regeneration(reason);").expect("the layout is requested");
    assert!(differs < owed && owed < layout, "the comparison, then the gate, then the layout");
    assert!(resized.contains("None=>true,"), "no surface to compare with: as before");
    let same = body(&shell, "fnsurface_size_differs(window:&AndroidWindow,(logical_w,logical_h):(f32,f32))->bool{");
    assert!(same.contains("now.width.to_bits()!=logical_w.to_bits()||now.height.to_bits()!=logical_h.to_bits()"), "bit for bit");
    let apply = body(&shell, "fnapply_surface_size(window:&mutAndroidWindow,logical:(f32,f32)){");
    assert!(apply.contains("ifsurface_size_differs(window,logical){window.cpu_backend.force_full_repaint=true;}"), "one comparison for both");
}
