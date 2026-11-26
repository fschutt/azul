//! Tests of `egl_gles` that need no EGL library: what a missing one says, and how a failure reads. The context, the
//! surface and the swap are exercised by the GLES host test (`gles_host_test`) on a runner with Mesa, and on the
//! emulator by the phone QA.

use super::*;

/// A device without `libEGL.so` is a diagnosed fallback to the CPU renderer, not a process that does not start: the
/// failure names the stage, the call and every library that was tried.
#[test]
fn a_missing_egl_library_is_a_failure_that_names_what_was_tried() {
    let err = EglApi::load(&["libEGL-that-does-not-exist.so", "libEGL-neither.so.1"]).unwrap_err();
    assert_eq!(err.stage, "library");
    assert_eq!(err.call, "dlopen");
    assert!(err.detail.contains("libEGL-that-does-not-exist.so"), "{err}");
    assert!(err.detail.contains("libEGL-neither.so.1"), "{err}");
    assert!(err.to_string().starts_with("dlopen failed: "), "{err}");
}

#[test]
fn no_candidate_at_all_is_a_failure_too() {
    let err = EglApi::load(&[]).unwrap_err();
    assert_eq!((err.stage, err.call), ("library", "dlopen"));
}

/// A library that exists but is not an EGL (any shared object will do) fails at the first function it lacks, with the
/// name of that function: the stage is still `library`.
#[cfg(target_os = "linux")]
#[test]
fn a_library_without_the_egl_functions_names_the_first_missing_one() {
    let err = EglApi::load(&["libm.so.6", "libc.so.6"]).unwrap_err();
    assert_eq!(err.stage, "library");
    assert_eq!(err.call, "eglGetDisplay", "{err}");
}

#[test]
fn a_failure_reads_as_call_error_and_detail() {
    let failure = EglFailure {
        stage: "surface",
        call: "eglCreateWindowSurface",
        code: 0x300B,
        detail: "the window is gone".into(),
    };
    assert_eq!(
        failure.to_string(),
        "eglCreateWindowSurface failed (EGL_BAD_NATIVE_WINDOW 0x300b): the window is gone"
    );
    let bare = EglFailure {
        stage: "context",
        call: "eglInitialize",
        code: 0,
        detail: String::new(),
    };
    assert_eq!(bare.to_string(), "eglInitialize failed");
}
