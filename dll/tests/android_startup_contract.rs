//! What the Android cold start shows, pinned in source (a phone cannot run where the tests run; the lab that measures it is
//! `scripts/ci/android_startup_lab.sh`, its numbers are in the engine ledger rows E46 / E47 / E60): no black between the system's
//! starting window and the first picture, no picture that is the wrong colour or in the wrong place and moves a moment later.
//!
//! The mechanism behind all of it: a NativeActivity's window is a surface the engine draws on itself (`Window.takeSurface`), the view
//! system never draws it, so the system takes its starting window away as soon as the window has "drawn" (at once), before the
//! surface has a single buffer. What the engine knows that late (the insets, the night mode) is known late; what it posts first is
//! posted late.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_startup_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Java source without its comments (line comments and `/* */` blocks: a doc comment names what the code does not use).
fn java_code(rel: &str) -> String {
    let mut text = read(rel);
    while let Some(start) = text.find("/*") {
        let end = text[start..].find("*/").map(|e| start + e + 2).expect("an unterminated block comment");
        text.replace_range(start..end, "");
    }
    text.lines().map(|l| l.split("//").next().unwrap_or("")).collect::<Vec<_>>().join("\n")
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

/// The surface of a NativeActivity has no buffer until the engine's first layout is done, and the system's starting window (the icon on
/// the window colour) is taken away as soon as the window has drawn, which for such a window is at once: the screen showed the empty
/// window, black, for as long as the layout took (QA's flicker audit, ledger E46: 0.4 s for the smallest app, 1.1 s for the widget
/// gallery, and no shorter with the hold that stood here). That hold ended the starting window with `SplashScreen.setOnExitAnimationListener`,
/// which hands the app a COPY of the window, a view in a window nobody draws: it held nothing. The window's FIRST DRAW is held instead,
/// the way the AndroidX splash screen library holds it: a pre-draw listener that cancels the draw until the engine says its first picture is
/// posted; the system keeps the starting window for as long as the window has not drawn. With a limit: it must never trap the app behind it.
///
/// Mutation: the listener answering `true` (no hold: the black is back); the deadline removed (an engine that never posts a picture left the
/// app behind its starting window for ever); `setOnExitAnimationListener` back (a hold that holds a view nobody draws).
#[test]
fn the_windows_first_draw_is_held_until_the_first_picture_and_never_trapped() {
    let java = java_code("scripts/android/AzulActivity.java");
    let create = body(&java, "protected void onCreate(Bundle savedInstanceState)");
    let sup = create.find("super.onCreate(savedInstanceState);").expect("NativeActivity makes the content view and takes the surface");
    let hold_call = create.find("holdFirstDraw();").expect("onCreate installs the hold");
    assert!(sup < hold_call, "the hold is installed after the window's content exists, before its first traversal");

    let hold = body(&java, "private void holdFirstDraw()");
    assert!(hold.contains("getViewTreeObserver().addOnPreDrawListener("), "the first draw is held by a pre-draw listener");
    let listener = body(hold, "public boolean onPreDraw()");
    assert!(
        listener.contains("if (!firstFrameShown && SystemClock.uptimeMillis() < deadline) {") && listener.contains("return false;"),
        "the draw is cancelled until the engine's first picture or the deadline"
    );
    assert!(listener.contains("removeOnPreDrawListener(this)") && listener.contains("return true;"), "and let through, once");
    assert!(java.contains("private static final long FIRST_DRAW_HOLD_MS = 4000;"), "the limit: the window must never stay away for good");
    assert!(hold.contains("if (firstFrameShown) {"), "a process that has shown its picture holds nothing");

    let first = body(&java, "public static void firstFramePresented(final android.app.Activity activity)");
    assert!(first.contains("activity.runOnUiThread(() -> firstFrameShown = true)"), "the engine's word reaches the UI thread's flag");

    for gone in ["setOnExitAnimationListener", "getSplashScreen", "Splash31", "SPLASH_HOLD_MS"] {
        assert!(!java.contains(gone), "AzulActivity still has `{gone}`: the hold that held a copy of the starting window nobody draws");
    }

    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let signal = body(&shell, "fnsignal_first_frame(){");
    assert!(signal.contains("\"firstFramePresented\"") && signal.contains("\"com/azul/app/AzulActivity\""));
    let once = body(&shell, "pub(super)fnfirst_frame_posted(&mutself){");
    assert!(once.contains("ifself.first_frame_signalled{return;}"), "once per window");
    let cpu = shell.find(".mark_first_present(").expect("the CPU path marks its first present");
    assert!(shell[cpu..].contains("window.first_frame_posted();"), "the CPU path tells the activity after its first post");
    let gpu = rust_code("dll/src/desktop/shell2/android/gpu.rs");
    assert!(
        gpu.contains("ifself.backend==RenderBackend::Gpu{self.first_frame_posted();}"),
        "the GPU path tells it too, while it is still the GPU path"
    );
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

/// The window's insets (the status bar, the navigation bar, the cutout) are dispatched in its FIRST traversal, before its surface exists. A
/// listener installed at the first window focus, where it used to be, missed that dispatch: the engine laid its first picture out under the
/// status bar, got the insets after the picture, laid everything out AGAIN (the "Resize" pass of the startup timeline, 0.3 s on the widget
/// gallery) and moved the page down by the height of the bar. The listener is installed in onCreate and keeps what it sees; the engine asks
/// for it when its surface is created and applies it before the first layout.
///
/// Mutation: the early listener not installed (the state of 98d415cb4e); the snapshot applied after the layout is requested.
#[test]
fn the_windows_first_insets_are_known_before_the_first_layout() {
    let java = java_code("scripts/android/AzulActivity.java");
    let create = body(&java, "protected void onCreate(Bundle savedInstanceState)");
    assert!(create.contains("NativeTextBridge.installEarlyInsetsListener(this);"), "onCreate installs the listener that sees the first dispatch");
    assert!(
        create.find("super.onCreate(savedInstanceState);").unwrap() < create.find("NativeTextBridge.installEarlyInsetsListener(this);").unwrap(),
        "after the window's decor exists"
    );
    let bridge = java_code("scripts/android/NativeTextBridge.java");
    let early = body(&bridge, "public static void installEarlyInsetsListener(final Activity activity)");
    assert!(early.contains("root.setOnApplyWindowInsetsListener((v, insets) -> {") && early.contains("rememberInsets(activity, v, insets);"));
    let remember = body(&bridge, "private static void rememberInsets(");
    assert!(remember.contains("lastInsets = values;") && remember.contains("lastFeatures = features;"), "what the first dispatch gave is kept");
    let snapshot = body(&bridge, "public static int[] insetsSnapshot()");
    assert!(snapshot.contains("if (insets == null) {") && snapshot.contains("return null;"), "no dispatch yet: no snapshot, not zeros");
    // a late attach hands the engine what the window has been given so far
    let late = body(&bridge, "public static void installInsetsListener(final Activity activity, final long nativePtr)");
    assert!(late.contains("insetsTarget = nativePtr;") && late.contains("nativeOnWindowInsets(nativePtr, insets[0]"), "a late attach forwards what was kept");

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

/// The surface of a rotation (or of any size the retained frame does not fit) has no buffer until the layout and the render at its size are
/// done, and shows black meanwhile: the window's canvas colour goes on it at once, before the layout is requested. One colour, every byte of the
/// buffer (`android_core::present::fill_flat`), the geometry told, and the first real present after it is whole (`PresentState::flat_posted`,
/// tested on every host). NOT on the window's first surface: there the starting window is held until the first picture, and a flat frame would
/// put the allocation of a second buffer and a fill in front of the first layout (the lab: the flat frame alone leaves 1.5 black frames median,
/// the hold leaves none). A window that draws on the GPU takes no CPU frame.
///
/// Mutation: the flat frame removed from `InitWindow`; posted after the layout is requested; posted on the first surface too.
#[test]
fn a_surface_with_no_frame_of_ours_gets_the_canvas_colour_before_the_layout() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let arm = shell.find("MainEvent::InitWindow{..}=>{").expect("the surface-created arm") + "MainEvent::InitWindow{..}".len();
    let init = body(&shell[arm..], "=>");
    let retained = init
        .find("window.draws_on_cpu()&&crate::desktop::shell2::android_core::present::retained_frame_fits(retained,surface_px)")
        .expect("the retained frame is the first choice");
    let flat = init[retained..]
        .find("elseifwindow.draws_on_cpu()&&window.present.has_frame(){")
        .expect("otherwise a CPU window that has shown a frame before (a rotation) gets the flat frame")
        + retained;
    let post = init[flat..].find("post_flat_frame(window,surface_px);").expect("it is posted") + flat;
    let layout = init.find("window.common.request_regeneration(reason)").expect("the layout is requested");
    assert!(retained < flat && flat < post && post < layout, "the flat frame is posted before the layout is requested");

    let flat_fn = body(&shell, "fnpost_flat_frame(window:&mutAndroidWindow,(width,height):(u32,u32)){");
    assert!(flat_fn.contains("window.common.clear_color()"), "the colour is THE canvas colour the renderer clears to");
    assert!(flat_fn.contains("nw.set_buffers_geometry(w,h,Some(Hbf::R8G8B8A8_UNORM))"), "the geometry the first real frame needs is told here");
    assert!(flat_fn.contains("android_core::present::fill_flat(bytes,pixel);"), "every byte of the buffer is written");
    assert!(flat_fn.contains("window.present.flat_posted((width,height));"), "the state knows the surface has no frame of ours");
}

/// The accessibility tree of the first layout is 30-45 ms of the time to the first picture (E33: the startup timeline's `a11y_tree_first_built`,
/// built inside the layout pass) and a screen reader reads a window that is on the screen: the first tree is built after the first picture is
/// posted. Every later layout builds its own, as before.
///
/// Mutation: the tree built inside the first layout again (the state of 98d415cb4e).
#[test]
fn the_first_accessibility_tree_is_built_after_the_first_picture() {
    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let layout = body(&shell, "pubfnregenerate_layout_inner(");
    let gate = layout
        .find("ifself.first_frame_signalled{self.refresh_accessibility_tree();}else{self.a11y_first_owed=true;}")
        .expect("the layout builds the tree only once the first picture is out, else it owes it");
    assert!(gate < layout.find("self.common.clear_regeneration_unless_reraised").expect("the pass retires its request"));
    let main = body(&shell, "pubfnandroid_main(app:AndroidApp){");
    let present = main.find("window.gpu_present();").expect("the loop presents");
    let owed = main
        .find("ifwindow.a11y_first_owed&&window.first_frame_signalled{window.a11y_first_owed=false;window.refresh_accessibility_tree();}")
        .expect("the loop builds the owed tree once the first picture is out");
    assert!(present < owed, "after the frame of this iteration is posted");
}
