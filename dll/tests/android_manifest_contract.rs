//! What the Android manifest and the APK build say, pinned in source (an APK cannot be installed where the tests run;
//! `aapt2 link` of the resources is run on the build). Ledger rows AN-19 and E46 of the 2026-11-28 audit.
//!
//! * the activity has a THEME whose window is the colour of the engine's default window per night mode: with none, the
//!   platform's dark default window sat for a second between the system's starting window and the first frame (E46);
//! * the hardware a permission goes with is declared NOT required, and a fine location comes with the coarse one (AN-19).
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_manifest_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The XML without its comments (a comment may mention an attribute the file does not set).
fn xml_code(rel: &str) -> String {
    let mut text = read(rel);
    while let Some(start) = text.find("<!--") {
        let end = text[start..]
            .find("-->")
            .map(|e| start + e + 3)
            .expect("an unterminated comment");
        text.replace_range(start..end, "");
    }
    text
}

/// Java source without its comments.
fn java_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
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

/// The activity's window before the first frame is the engine's own default colour per night mode: the starting
/// window (the icon on the colour) is followed by the same colour, not by the platform's dark default. The Android 12+
/// starting window gets the same background through its own attribute, which an older platform does not have.
///
/// Mutation: `android:theme` removed from `<application>` (the state of 6ce827b284: 1.03 s of flat dark at a cold start).
#[test]
fn the_activity_window_is_the_engines_colour_in_both_modes() {
    let manifest = xml_code("scripts/android/AndroidManifest.xml");
    let application = &manifest[manifest.find("<application").expect("the application element")..];
    let application = &application[..application.find('>').unwrap()];
    assert!(
        application.contains("android:theme=\"@style/AzulTheme\""),
        "the application has the azul theme: {application}"
    );

    for (file, colour, parent) in [
        ("scripts/android/res/values/azul_theme.xml", "#FFFFFF", "android:Theme.Material.Light.NoActionBar"),
        ("scripts/android/res/values-night/azul_theme.xml", "#000000", "android:Theme.Material.NoActionBar"),
    ] {
        let xml = xml_code(file);
        assert!(
            xml.contains(&format!("<color name=\"azul_window_background\">{colour}</color>")),
            "{file}: the window colour"
        );
        assert!(
            xml.contains(&format!("<style name=\"AzulTheme\" parent=\"{parent}\">")),
            "{file}: the theme"
        );
        assert!(
            xml.contains("<item name=\"android:windowBackground\">@color/azul_window_background</item>"),
            "{file}"
        );
    }
    for (file, parent) in [
        ("scripts/android/res/values-v31/azul_theme.xml", "android:Theme.Material.Light.NoActionBar"),
        ("scripts/android/res/values-night-v31/azul_theme.xml", "android:Theme.Material.NoActionBar"),
    ] {
        let xml = xml_code(file);
        assert!(
            xml.contains(&format!("<style name=\"AzulTheme\" parent=\"{parent}\">")),
            "{file}"
        );
        assert!(
            xml.contains("<item name=\"android:windowSplashScreenBackground\">@color/azul_window_background</item>"),
            "{file}: the Android 12+ starting window's background"
        );
    }
    // the colours are the engine's: the same values as css/src/system.rs
    let system = read("css/src/system.rs");
    let light = &system[system.find("pub fn android_material_light()").unwrap()..];
    assert!(
        light[..600].contains("background: OptionColorU::Some(ColorU::new_rgb(255, 255, 255))"),
        "light is white"
    );
    let dark = &system[system.find("pub fn android_holo_dark()").unwrap()..];
    assert!(
        dark[..600].contains("background: OptionColorU::Some(ColorU::new_rgb(0, 0, 0))"),
        "dark is black"
    );
}

/// The resources go into EVERY APK (not only into one with a generated launcher icon): the theme is compiled and
/// linked whatever the icon script does.
///
/// Mutation: `aapt2 compile` back inside the `if` of the icon (an APK without the icon script had no theme resource and
/// `aapt2 link` failed on `@style/AzulTheme`).
#[test]
fn the_theme_is_compiled_into_every_apk() {
    let script = read("scripts/build-android.sh");
    let copy = script
        .find("cp -R \"$ASSET_ROOT/scripts/android/res/.\" \"$BUILD_DIR/res/\"")
        .expect("the theme resources are copied into the build's res directory");
    let icon = script
        .find("if [[ -f \"$ASSET_ROOT/scripts/demo_icon.py\"")
        .expect("the icon block");
    let icon_end = icon + script[icon..].find("\nfi\n").expect("the icon block ends");
    let compile = script
        .find("\"$BT/aapt2\" compile --dir \"$BUILD_DIR/res\"")
        .expect("the resources are compiled");
    assert!(copy < icon, "the theme is copied before the icon is added to the same directory");
    assert!(compile > icon_end, "the compile is outside the icon block: it runs for every APK");
    assert!(
        script.contains("$( [[ -f compiled-res.zip ]] && echo compiled-res.zip )"),
        "and the link takes the compiled resources"
    );
}

/// A permission that goes with hardware does not make the hardware REQUIRED (a store filters an app out of every
/// device without a required feature), and a FINE location is declared with the COARSE one (the request is made for
/// both: a permission that is not declared is refused unasked).
///
/// Mutation: the `uses-feature` loop removed (the state of 6ce827b284: AzMeet was offered to no device without an
/// autofocus camera).
#[test]
fn hardware_is_optional_and_fine_location_comes_with_coarse() {
    let script = read("scripts/build-android.sh");
    for needle in [
        "android.hardware.camera android.hardware.camera.autofocus",
        "android.hardware.microphone",
        "android.hardware.location android.hardware.location.gps",
        "<uses-feature android:name=\\\"$f\\\" android:required=\\\"false\\\" />",
        "android.permission.ACCESS_COARSE_LOCATION\\\" />",
    ] {
        assert!(script.contains(needle), "build-android.sh: `{needle}`");
    }
    let coarse = script
        .find("!= *android.permission.ACCESS_COARSE_LOCATION*")
        .expect("FINE without COARSE adds it");
    let fine = script[..coarse]
        .rfind("== *android.permission.ACCESS_FINE_LOCATION*")
        .expect("the FINE test");
    assert!(fine < coarse);

    let manifest = xml_code("scripts/android/AndroidManifest.xml");
    assert!(
        manifest.contains("<uses-feature android:name=\"android.hardware.wifi\" android:required=\"false\" />"),
        "CHANGE_WIFI_MULTICAST_STATE implies required Wi-Fi hardware: the template says it is optional"
    );
}

/// On Android 12 and later the system's starting window (the icon on the window colour) leaves when the activity's window has
/// drawn, which for a NativeActivity is BEFORE its surface has a buffer: a second of the platform's empty window followed on the CPU
/// renderer (QA's flicker audit, ledger E46; the theme's window colour covers only what the view hierarchy draws). The starting
/// window is held with an exit listener and removed when the engine says its first picture was posted, from the CPU path and from the
/// GPU path, or after two seconds: it must never trap the app behind it. The Android 12 API sits in a class of its own that an older
/// system never loads.
///
/// Mutation: the `postDelayed` removed (an engine that never posts a picture left the app behind its starting window for ever).
#[test]
fn the_starting_window_is_held_until_the_first_picture_and_never_trapped() {
    let java = java_code("scripts/android/AzulActivity.java");
    let create = body(&java, "protected void onCreate(Bundle savedInstanceState)");
    assert!(create.contains("holdStartingWindow();"), "onCreate installs the hold");
    let hold = body(&java, "private void holdStartingWindow()");
    assert!(
        hold.contains("android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.S") && hold.contains("Splash31.hold(this);"),
        "Android 12 and later only, through the class that older systems never load"
    );
    let splash = body(&java, "private static final class Splash31");
    assert!(splash.contains("setOnExitAnimationListener"), "the system hands the starting window over");
    assert!(splash.contains("postDelayed(remove, SPLASH_HOLD_MS)"), "and takes it back after a time, whatever the engine does");
    assert!(splash.contains("view.remove()"), "the window is removed, once");
    assert!(java.contains("private static final long SPLASH_HOLD_MS = 2000;"));
    let first = body(&java, "public static void firstFramePresented(final android.app.Activity activity)");
    assert!(first.contains("activity.runOnUiThread") && first.contains("firstFrameShown = true;") && first.contains("remove.run();"));

    let shell = rust_code("dll/src/desktop/shell2/android/mod.rs");
    let cpu = shell.find(".mark_first_present(").expect("the CPU path marks its first present");
    assert!(shell[cpu..].contains("window.first_frame_posted();"), "the CPU path tells the activity after its first post");
    let signal = body(&shell, "fnsignal_first_frame(){");
    assert!(signal.contains("\"firstFramePresented\"") && signal.contains("\"com/azul/app/AzulActivity\""));
    let once = body(&shell, "pub(super)fnfirst_frame_posted(&mutself){");
    assert!(once.contains("ifself.first_frame_signalled{return;}"), "once per window");
    let gpu = rust_code("dll/src/desktop/shell2/android/gpu.rs");
    assert!(
        gpu.contains("ifself.backend==RenderBackend::Gpu{self.first_frame_posted();}"),
        "the GPU path tells it too, while it is still the GPU path"
    );
}

