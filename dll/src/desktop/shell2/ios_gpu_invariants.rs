//! The iOS GPU path's rules, pinned in the source of the shell a test binary on a Linux or macOS host cannot run.
//!
//! The lifecycle machine (`common/gpu_lifecycle.rs`) is tested against a fake platform; what this file holds is
//! that the iOS shell asks it: that every UIKit event the machine needs reaches it, that the CPU path is left as it
//! was, that nothing links or closes `OpenGLES.framework`, and that what is given up in the background is given up
//! without a GL command. A rule that is only in a comment is not a rule: these read the code.

const GL: &str = include_str!("ios/gl.rs");
const GPU: &str = include_str!("ios/gpu.rs");
const SHELL: &str = include_str!("ios/mod.rs");

/// The text of the function that begins with `signature`: from there to the brace that closes it, comments and
/// string literals skipped.
fn function_body<'a>(source: &'a str, signature: &str) -> &'a str {
    let start = source
        .find(signature)
        .unwrap_or_else(|| panic!("`{signature}` is not in the source"));
    let bytes = source.as_bytes();
    let (mut i, mut depth, mut opened) = (start + signature.len(), 0_i32, false);
    // the signature's own text has no brace; a `{` after it opens the body
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'"' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'"' {
                    if bytes[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            b'{' => {
                depth += 1;
                opened = true;
            }
            b'}' => {
                depth -= 1;
                if opened && depth == 0 {
                    return &source[start..=i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    &source[start..]
}

/// The part of `source` between two markers.
fn between<'a>(source: &'a str, from: &str, to: &str) -> &'a str {
    let start = source
        .find(from)
        .unwrap_or_else(|| panic!("`{from}` is not in the source"));
    let rest = &source[start..];
    let end = rest[from.len()..]
        .find(to)
        .map_or(rest.len(), |at| at + from.len());
    &rest[..end]
}

/// `OpenGLES.framework` is opened by its path when a window asks for the GPU: it is not linked (a link flag
/// would make every app depend on a deprecated framework, and the build scripts would have to name it) and it is
/// not closed (the GL table points into it, and a GL library is not safe to unload).
///
/// Mutation: a `#[link(name = "OpenGLES", kind = "framework")]`; a library that is dropped.
#[test]
fn opengles_is_opened_by_path_and_neither_linked_nor_closed() {
    for (name, text) in [("gl.rs", GL), ("gpu.rs", GPU), ("mod.rs", SHELL)] {
        assert!(
            !text.contains("name = \"OpenGLES\""),
            "{name} links OpenGLES.framework"
        );
        assert!(!text.contains("dlclose"), "{name} closes a library");
    }
    assert!(
        GL.contains("libloading::Library::new(OPENGLES_PATH)"),
        "the framework is opened by its path"
    );
    assert!(
        GL.contains("ManuallyDrop<libloading::Library>"),
        "the opened framework is never dropped (dlclose)"
    );
    assert!(GL.contains("/System/Library/Frameworks/OpenGLES.framework/OpenGLES"));
}

/// The context is OpenGL ES 3.0 only: no other API is asked for, so a device without it falls back instead of
/// handing WebRender a context it cannot run on.
#[test]
fn the_context_is_opengl_es_3_only() {
    assert!(GL.contains("const EAGL_RENDERING_API_OPENGLES3: usize = 3;"));
    assert_eq!(
        GL.matches("msg_send![allocated, initWithAPI:").count(),
        1,
        "one way to make a context, ES 3.0, and no fallback to ES 2.0"
    );
}

/// Every UIKit event the lifecycle machine needs reaches it, from the handler UIKit calls.
///
/// Mutation: a handler that forgets its event (the app is paused and backgrounded without the machine knowing:
/// a GL command in the background ends the process).
#[test]
fn the_uikit_lifecycle_reaches_the_machine() {
    for (handler, event) in [
        (
            "extern \"C\" fn app_will_resign_active",
            "gpu_event(gpu_lifecycle::Event::Paused)",
        ),
        (
            "extern \"C\" fn app_did_enter_background",
            "gpu_event(gpu_lifecycle::Event::Backgrounded)",
        ),
        (
            "extern \"C\" fn app_did_become_active",
            "gpu_event(gpu_lifecycle::Event::Resumed)",
        ),
        (
            "extern \"C\" fn app_did_receive_memory_warning",
            "gpu_memory_warning()",
        ),
        (
            "extern \"C\" fn app_will_terminate",
            "gpu_event(gpu_lifecycle::Event::Shutdown)",
        ),
        ("pub fn close", "gpu_event(gpu_lifecycle::Event::Shutdown)"),
        ("extern \"C\" fn layout_subviews", "gpu_resized()"),
    ] {
        assert!(
            function_body(SHELL, handler).contains(event),
            "{handler} must tell the GPU path: {event}"
        );
    }
}

/// The CPU path is what it was for a CPU window: `displayLayer:` does nothing for a GPU window (its layer has no
/// content to set and the frame is the display tick's), `present()` does not ask Core Animation for a display
/// pass for it, and the CPU raster is for the CPU backend only.
///
/// Mutation: a GPU window whose `displayLayer:` still rasters on the CPU (two renderers drawing one window).
#[test]
fn a_gpu_window_is_not_drawn_by_the_cpu_path() {
    let display_layer = function_body(SHELL, "extern \"C\" fn display_layer");
    let guard = display_layer
        .find("window.backend == RenderBackend::Gpu")
        .expect("display_layer names the GPU backend");
    let first_work = display_layer
        .find("pump_webviews_if_any")
        .expect("display_layer pumps the web views");
    assert!(guard < first_work, "the GPU window leaves displayLayer: before any work");

    let present = function_body(SHELL, "pub fn present");
    let guard = present
        .find("RenderBackend::Gpu")
        .expect("present names the GPU backend");
    let request = present
        .find("setNeedsDisplay")
        .expect("present asks for a display pass");
    assert!(guard < request, "a GPU window asks for no display pass");

    let layout = function_body(SHELL, "pub fn regenerate_layout_inner");
    assert!(layout.contains("self.gpu_send_frame()"), "a GPU window sends its frame");
    assert!(
        layout.contains("if self.backend == RenderBackend::Cpu"),
        "the CPU raster is for the CPU backend"
    );

    let tick = function_body(SHELL, "extern \"C\" fn display_tick");
    assert!(
        tick.contains("window.backend == RenderBackend::Gpu") && tick.contains("window.gpu_tick()"),
        "the display tick draws a GPU window"
    );
}

/// What is given up in the background (a lost context, a window that closes) is abandoned: leaked without a GL
/// command, a context call or a release that flushes. Nothing that deletes or deinits may be in that arm.
///
/// Mutation: an `AbandonGl` that deletes (a GL command in the background ends the process).
#[test]
fn abandoning_gl_issues_no_gl_command() {
    let abandon = between(GPU, "Action::AbandonGl => {", "Action::FallBackToCpu");
    for forbidden in [
        ".finish()",
        ".destroy()",
        "deinit_renderer",
        "make_current",
        ".bind(",
        "notify_memory_pressure",
    ] {
        assert!(
            !abandon.contains(forbidden),
            "the AbandonGl arm must not call `{forbidden}`"
        );
    }
    assert!(abandon.contains("std::mem::forget(renderer)"), "the renderer is leaked");
    assert!(abandon.contains(".abandon()"), "the drawable is abandoned");
    assert!(abandon.contains(".leak()"), "the context is leaked");
}

/// Stopping the frames pauses the shell's display link, starting them runs it: the machine's pause is the
/// shell's pause, not a second clock.
#[test]
fn the_machines_frames_are_the_shells_display_link() {
    let stop = between(GPU, "Action::StopFrames => {", "Action::StartFrames");
    assert!(stop.contains("set_display_link_paused(true)"));
    let start = between(GPU, "Action::StartFrames => {", "Action::Finish");
    assert!(start.contains("set_display_link_paused(false)"));
}

/// A GPU frame is drawn only when the machine says so (`Event::Frame` answered by `Present`), never from a
/// handler of UIKit's: the tick asks, and every `Present` goes through the executor.
#[test]
fn a_gpu_frame_is_asked_of_the_machine() {
    let tick = function_body(GPU, "pub(super) fn gpu_tick");
    assert!(tick.contains("self.gpu_event(Event::Frame)"));
    assert!(tick.contains("Phase::Ready"), "nothing is drawn unless the machine is ready");
    // the only caller of the frame is the executor's `Present` arm
    assert_eq!(GPU.matches("self.gpu_draw_frame()").count(), 1);
}

/// Both renderers note their presents, so the QA compares the frame times of a scroll measured by the same
/// clock in the same place: where the frame is handed to the system.
///
/// Mutation: a renderer that presents without noting it (its frame times are missing from the comparison).
#[test]
fn both_renderers_note_their_presents() {
    assert!(function_body(SHELL, "extern \"C\" fn display_layer").contains("present_log.note(None)"));
    assert!(function_body(GPU, "fn gpu_draw_frame").contains("present_log.note(Some(render_us))"));
}

/// The one line the QA harness reads is written by the decision, not composed by hand in the shell.
#[test]
fn the_render_line_comes_from_the_decision() {
    assert!(GPU.contains("decision.render_line()"));
    assert!(GPU.contains("decision.fallback_line()"));
    assert!(
        !GPU.contains("\"[render] backend="),
        "the line's words live in gpu_select::Decision::render_line"
    );
}
