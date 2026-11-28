//! What the Android cold start shows, the Java half of it, pinned in source (a phone cannot run where the tests run; the lab that measures it is
//! `scripts/ci/android_startup_lab.sh`, its numbers are in the engine ledger rows E46 / E47 / E60): no black between the system's
//! starting window and the first picture, no picture that is the wrong colour or in the wrong place and moves a moment later.
//!
//! The mechanism behind all of it: a NativeActivity's window is a surface the engine draws on itself (`Window.takeSurface`), the view
//! system never draws it, so the system takes its starting window away as soon as the window has "drawn" (at once), before the
//! surface has a single buffer. What the engine knows that late (the insets, the night mode) is known late; what it posts first is
//! posted late.
//!
//! The engine's half is `dll/tests/android_startup_engine_contract.rs`.
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
        listener.contains("if (!engineSaidSo && SystemClock.uptimeMillis() < deadline) {") && listener.contains("return false;"),
        "the draw is cancelled until the engine's first picture or the deadline"
    );
    assert!(listener.contains("removeOnPreDrawListener(this)") && listener.contains("return true;"), "and let through, once");
    assert!(listener.contains("the engine never said so"), "a draw let through at the limit says so in the log");
    assert!(java.contains("private static final long FIRST_DRAW_HOLD_MS = 4000;"), "the limit: the window must never stay away for good");

    // The flag is the ACTIVITY's, not a process static: no activity inherits what an earlier one saw.
    assert!(java.contains("private boolean firstFrameShown;"), "the flag is a field of the activity");
    assert!(!java.contains("static boolean firstFrameShown"), "not a field of the process: a second activity of a process would not be held");
    assert!(!hold.contains("if (firstFrameShown) {"), "no activity is let through because an earlier one showed its picture");
    let first = body(&java, "public static void firstFramePresented(final android.app.Activity activity)");
    assert!(first.contains("activity instanceof AzulActivity"), "the activity the engine names is the one whose flag is set");
    assert!(first.contains("azul.runOnUiThread(() -> azul.firstFrameShown = true)"), "the engine's word reaches that activity's flag on the UI thread");

    for gone in ["SPLASH_HOLD_MS", "removeSplash", "holdStartingWindow"] {
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

/// When the window has drawn, the system takes the starting window away with an exit animation of its own: the icon dims and the window fades out
/// over the page. In the night mode the starting window is black and the page nearly so, and the fade passes through a window with no icon and no
/// page: 3 and 2.5 frames median, 4 and 6 worst, of the lab's control at 10 starts per row on Android 14 and 15 (none in the light mode). With an exit
/// listener that removes the copy of the window the system hands over AT ONCE, the page replaces the starting window in one frame: 0 of 20 night
/// starts and 0 of 20 light ones show black, a launcher, a light picture or a page that moved. The listener must hold NOTHING: the first draw is
/// what is held (`holdFirstDraw`); a listener that keeps the copy for later is the E60 hold again, which held a view in a window nobody draws.
///
/// Mutation: the listener keeping the view (the state of 98d415cb4e); no listener (the system's fade through black in the night mode).
#[test]
fn the_starting_window_goes_in_one_frame_when_the_window_draws() {
    let java = java_code("scripts/android/AzulActivity.java");
    let create = body(&java, "protected void onCreate(Bundle savedInstanceState)");
    let hold = create.find("holdFirstDraw();").expect("the first draw is held");
    let cut = create
        .find("if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S) {Splash31.cut(this);}")
        .or_else(|| create.find("Splash31.cut(this);"))
        .expect("onCreate sets the exit listener, on Android 12 and later");
    assert!(hold < cut, "after the hold is in place");
    assert!(create[..cut].contains("VERSION_CODES.S"), "only where the API exists");
    let listener = body(&java, "private static final class Splash31");
    assert!(
        listener.contains("activity.getSplashScreen().setOnExitAnimationListener(view -> view.remove());"),
        "the copy of the starting window is removed the moment the system hands it over: {listener}"
    );
    assert!(!listener.contains("postDelayed") && !listener.contains("Handler") && !listener.contains("static Runnable"), "no timer, nothing kept for later");
    assert!(listener.contains("catch (RuntimeException"), "a system that refuses leaves its own exit, not a crash");
}
