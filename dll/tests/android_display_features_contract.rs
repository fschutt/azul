//! The Java half of the display features (cutouts, rounded corners, system bars, fingerprint sensor), pinned in source: the
//! array `NativeTextBridge.dispatchDisplayFeatures` builds is the one `android_core/display.rs` reads (that module's tests read
//! an array of the same layout), and the platform's public API only is used.
//!
//!     CARGO_MANIFEST_DIR=$PWD/dll rustc --edition 2021 --test dll/tests/android_display_features_contract.rs -o /tmp/t && /tmp/t

use std::{fs, path::Path};

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn java_code(rel: &str) -> String {
    read(rel)
        .lines()
        .map(|l| l.split("//").next().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
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

/// The array is the layout `android_core/display.rs` documents (version 1): the version and the cutout count, the cutouts' rects,
/// the four corner radii in the order top left, top right, bottom right, bottom left, the status bar, the navigation bar, the
/// fingerprint sensor; its length is the sum of those parts.
///
/// Mutation: the corners in another order (a Pixel's rounded top right drawn on the bottom left).
#[test]
fn the_array_of_the_java_bridge_is_the_layout_the_rust_decoder_reads() {
    let java = java_code("scripts/android/NativeTextBridge.java");
    let dispatch = body(&java, "private static void dispatchDisplayFeatures(");
    assert!(dispatch.contains("new int[2 + 4 * count + 4 + 6 + 6 + 1]"), "version + count, 4 per cutout, 4 corners, two bars of 6, the sensor");
    let order = [
        "flat[at++] = 1;",
        "flat[at++] = count;",
        "RoundedCorner.POSITION_TOP_LEFT",
        "RoundedCorner.POSITION_TOP_RIGHT",
        "RoundedCorner.POSITION_BOTTOM_RIGHT",
        "RoundedCorner.POSITION_BOTTOM_LEFT",
        "at = putBar(flat, at, statusHeight > 0,",
        "at = putBar(flat, at, true, 0, height - navBottom,",
        "flat[at] = fingerprint ? 0 : -1;",
        "nativeOnDisplayFeatures(nativePtr, flat);",
    ];
    let mut from = 0;
    for needle in order {
        let at = dispatch[from..].find(needle).unwrap_or_else(|| panic!("`{needle}` is missing or out of order"));
        from += at + needle.len();
    }
    let put = body(&java, "private static int putBar(");
    // present, left, top, right, bottom, visible: the six of a bar in `display.rs`
    let fields = ["present ? 1 : 0", "= left;", "= top;", "= right;", "= bottom;", "visible ? 1 : 0"];
    let mut from = 0;
    for needle in fields {
        from += put[from..].find(needle).unwrap_or_else(|| panic!("putBar: `{needle}`")) + needle.len();
    }
    assert!(java.contains("private static native void nativeOnDisplayFeatures(long nativePtr, int[] flat);"));
    assert!(
        body(&java, "root.setOnApplyWindowInsetsListener((v, insets) -> {").contains("dispatchDisplayFeatures(nativePtr, v, insets);"),
        "the features come with every insets dispatch (a rotation moves the cutout to another edge)"
    );
}

/// The fingerprint sensor's kind and place are hidden API: no reflection and no `FingerprintSensorProperties` here, only whether
/// the device has the feature; a platform that refuses one call leaves the app with its insets.
///
/// Mutation: `getDeclaredMethod` to read the sensor's location (a hack the engine's rule forbids).
#[test]
fn the_fingerprint_sensor_is_told_by_the_public_api_only() {
    let java = java_code("scripts/android/NativeTextBridge.java");
    for forbidden in ["getDeclaredMethod", "Class.forName", "FingerprintSensorProperties", "getSensorLocation"] {
        assert!(!java.contains(forbidden), "NativeTextBridge uses `{forbidden}`: hidden API, no reflection");
    }
    let dispatch = body(&java, "private static void dispatchDisplayFeatures(");
    assert!(dispatch.contains("PackageManager.FEATURE_FINGERPRINT"));
    assert!(dispatch.contains("catch (RuntimeException e)"), "a refused call must not end the insets dispatch");
}
