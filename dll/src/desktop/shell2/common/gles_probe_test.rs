//! Tests of `gles_probe`: the version strings and renderer strings real drivers answer, and the minimum WebRender asks.

use super::*;

/// The strings below are what the drivers of the phones and of the emulator answer for `GL_VERSION` (collected from the
/// logs of the WebRender / Firefox bug trackers and the emulator's own `adb logcat` line `GL_VERSION`).
#[test]
fn gles_versions_are_read_from_the_driver_strings() {
    let gles = |major, minor| Some(GlesVersion { major, minor });
    assert_eq!(parse_gles_version("OpenGL ES 3.2 V@0502.0 (GIT@4b55d5e, I5ebf6b7fb4)"), gles(3, 2));
    assert_eq!(parse_gles_version("OpenGL ES 3.0 Mesa 23.2.1-1ubuntu3.1~22.04.2"), gles(3, 0));
    assert_eq!(parse_gles_version("OpenGL ES 3.1 v1.r32p1-01eac0.f0e1c9b4d6e3c1b3"), gles(3, 1));
    assert_eq!(parse_gles_version("OpenGL ES 3.0 (ANGLE 2.1.0 git hash: abc)"), gles(3, 0));
    assert_eq!(parse_gles_version("OpenGL ES 2.0 Apple A7 GPU - 85.11"), gles(2, 0));
    assert_eq!(parse_gles_version("  OpenGL ES 3.0 \n"), gles(3, 0));
    // The ES 1 profiles name themselves with a suffix on the name.
    assert_eq!(parse_gles_version("OpenGL ES-CM 1.1"), gles(1, 1));
    assert_eq!(parse_gles_version("OpenGL ES-CL 1.1 Imagination"), gles(1, 1));
}

#[test]
fn a_desktop_or_empty_version_string_is_not_gles() {
    assert_eq!(parse_gles_version("4.6.0 NVIDIA 535.183.01"), None);
    assert_eq!(parse_gles_version("4.1 Metal - 88.1"), None);
    assert_eq!(parse_gles_version("3.3 (Core Profile) Mesa 23.2.1"), None);
    assert_eq!(parse_gles_version(""), None);
    // A truncated or damaged ES string is not a version.
    assert_eq!(parse_gles_version("OpenGL ES"), None);
    assert_eq!(parse_gles_version("OpenGL ES three"), None);
    assert_eq!(parse_gles_version("OpenGL ES 3"), None);
}

#[test]
fn the_renderers_of_phones_the_emulator_and_software_are_told_apart() {
    let hardware = RendererClass { software: false, emulated: false };
    assert_eq!(classify_renderer("Adreno (TM) 740"), hardware);
    assert_eq!(classify_renderer("Mali-G78"), hardware);
    assert_eq!(classify_renderer("PowerVR Rogue GE8320"), hardware);
    assert_eq!(classify_renderer("Apple A15 GPU"), hardware);
    assert_eq!(classify_renderer("NVIDIA Tegra"), hardware);

    // The emulator's `swiftshader_indirect`: the translator in front of SwiftShader on the host.
    assert_eq!(
        classify_renderer("Android Emulator OpenGL ES Translator (Google SwiftShader)"),
        RendererClass { software: true, emulated: true }
    );
    // The emulator's `host` mode: the translator in front of the host's GPU.
    assert_eq!(
        classify_renderer("Android Emulator OpenGL ES Translator (Apple M2 Pro)"),
        RendererClass { software: false, emulated: true }
    );
    // The emulator's `guest` mode: the guest's own software GL ES.
    assert_eq!(
        classify_renderer("Google SwiftShader"),
        RendererClass { software: true, emulated: false }
    );
    assert_eq!(
        classify_renderer("llvmpipe (LLVM 15.0.7, 256 bits)"),
        RendererClass { software: true, emulated: false }
    );
    assert_eq!(
        classify_renderer("softpipe"),
        RendererClass { software: true, emulated: false }
    );
    assert_eq!(
        classify_renderer("Mesa Software Rasterizer"),
        RendererClass { software: true, emulated: false }
    );
}

#[test]
fn webrenders_minimum_is_gles_3_and_a_2048_texture() {
    let ok = probe("OpenGL ES 3.2 V@0502.0", "Adreno (TM) 740", 16384).expect("a phone GPU");
    assert_eq!(ok.version, GlesVersion { major: 3, minor: 2 });
    assert!(!ok.class.software && !ok.class.emulated);

    assert!(probe("OpenGL ES 3.0 Mesa 23.2", "llvmpipe", 2048).is_ok(), "exactly 3.0 and 2048 is enough");
    let emulator = probe(
        "OpenGL ES 3.0 (ANGLE)",
        "Android Emulator OpenGL ES Translator (Google SwiftShader)",
        8192,
    )
    .expect("the emulator");
    assert!(emulator.class.software && emulator.class.emulated);
}

#[test]
fn each_unmet_minimum_says_which_one() {
    let old = probe("OpenGL ES 2.0 Adreno", "Adreno (TM) 200", 4096).unwrap_err();
    assert!(old.contains("OpenGL ES 2.0") && old.contains("3.0"), "{old}");

    let desktop = probe("4.6.0 NVIDIA 535.183.01", "NVIDIA GeForce", 16384).unwrap_err();
    assert!(desktop.contains("not OpenGL ES") && desktop.contains("NVIDIA 535"), "{desktop}");

    let small = probe("OpenGL ES 3.0", "Mali-400", 1024).unwrap_err();
    assert!(small.contains("MAX_TEXTURE_SIZE is 1024") && small.contains("2048"), "{small}");

    // A context that answers nothing (a broken driver) cannot drive WebRender either.
    assert!(probe("", "", 0).is_err());
}

#[test]
fn the_required_functions_are_the_es_3_core_ones_and_name_no_desktop_only_call() {
    assert!(REQUIRED_FUNCTIONS.contains(&"glDrawElementsInstanced"));
    assert!(REQUIRED_FUNCTIONS.contains(&"glClearBufferfv"));
    // OpenGL ES has glClearDepthf; the double-precision glClearDepth and glGetTexImage do not exist there.
    assert!(!REQUIRED_FUNCTIONS.contains(&"glClearDepth"));
    assert!(!REQUIRED_FUNCTIONS.contains(&"glGetTexImage"));
    assert!(!REQUIRED_FUNCTIONS.contains(&"glBufferStorage"), "an extension on OpenGL ES, optional");
    // No duplicates, so the diagnostic does not name a function twice.
    let mut sorted = REQUIRED_FUNCTIONS.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), REQUIRED_FUNCTIONS.len());
}
