//! Source-text invariants for how a window goes away on the backends a test
//! binary on one host cannot run (macOS, Win32, X11, Wayland): the shapes the
//! teardown depends on, pinned as `loop_wakeup_invariants` pins the run
//! loops' wake-up.
//!
//! One lifecycle on every backend: a pass only REQUESTS a close
//! (`flags.close_requested`); the loop carries it out at its safe point,
//! outside every callback of the window; the GPU side goes first, in the
//! window's own GL context, and the thread gets back the context it had
//! before; nothing native calls into a window once its struct is freed.

const MACOS_RS: &str = include_str!("macos/mod.rs");
const WINDOWS_RS: &str = include_str!("windows/mod.rs");
const WINDOWS_DLOPEN_RS: &str = include_str!("windows/dlopen.rs");
const X11_RS: &str = include_str!("linux/x11/mod.rs");
const X11_GL_RS: &str = include_str!("linux/x11/gl.rs");
const X11_DLOPEN_RS: &str = include_str!("linux/x11/dlopen.rs");
const WAYLAND_RS: &str = include_str!("linux/wayland/mod.rs");
const RUN_RS: &str = include_str!("run.rs");
const EVENT_RS: &str = include_str!("common/event.rs");

/// The body of the first fn declared by `decl`, from its opening brace to the
/// matching closing one.
fn body<'a>(source: &'a str, decl: &str) -> &'a str {
    let start = source
        .find(decl)
        .unwrap_or_else(|| panic!("`{decl}` not found - was it renamed?"));
    let open = start + source[start..].find('{').expect("a fn body");
    let mut depth = 0_usize;
    for (i, c) in source[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &source[open..=open + i];
                }
            }
            _ => {}
        }
    }
    &source[open..]
}

/// Where `needle` first occurs in `haystack`; a panic saying what is missing.
fn at(haystack: &str, needle: &str, what: &str) -> usize {
    haystack
        .find(needle)
        .unwrap_or_else(|| panic!("{what}: `{needle}` is missing"))
}

/// `first` occurs, and before `then`.
fn in_order(haystack: &str, first: &str, then: &str, what: &str) {
    assert!(
        at(haystack, first, what) < at(haystack, then, what),
        "{what}: `{first}` must come before `{then}`"
    );
}

/// AppKit keeps a closed window and its views alive past the `MacOSWindow`
/// that owns them (the autorelease pool of the turn that closed it, a
/// display-link tick that retained it, a layer display it had queued). A
/// `drawRect:` that still finds the view's back-pointer then reads the GL
/// functions out of the freed window: a segfault on a popover's close. Every
/// way out lets go of every pointer AppKit could call through, before the box
/// can be freed.
#[test]
fn a_closed_macos_window_takes_back_every_pointer_appkit_could_call_it_through() {
    let release = body(MACOS_RS, "fn release_native(&mut self)");
    for needle in [
        "self.retire_display_link()",
        "self.window_delegate.forget_window()",
        "self.window.setDelegate(None)",
        "timer.invalidate()",
        "self.release_gpu_resources()",
    ] {
        at(release, needle, "release_native");
    }
    assert!(
        release.matches("view.forget_window()").count() >= 2,
        "release_native: the GL view and the CPU view both let go of the window"
    );
    let forget = body(MACOS_RS, "fn forget_window(view: &NSView");
    at(forget, "window_ptr.set(None)", "forget_window");
    at(forget, "cancelPreviousPerformRequestsWithTarget", "forget_window");

    in_order(
        body(MACOS_RS, "fn close_window(&mut self)"),
        "self.release_native()",
        "self.window.close()",
        "close_window",
    );
    let by_appkit = body(MACOS_RS, "fn closed_by_appkit(&mut self)");
    at(by_appkit, "self.release_native()", "closed_by_appkit");
    at(by_appkit, "self.unregister_and_park()", "closed_by_appkit");
    let will_close = body(MACOS_RS, "fn window_will_close(&self");
    at(will_close, "closed_by_appkit()", "windowWillClose:");
    assert!(
        !will_close.contains("Box::from_raw"),
        "windowWillClose: runs inside the window's own delegate: it parks, never frees"
    );
    at(
        body(MACOS_RS, "impl Drop for MacOSWindow"),
        "self.release_native()",
        "Drop for MacOSWindow",
    );

    // A teardown inside an AppKit callback clears the pointer the callback
    // read: a RefCell's guard would turn that into a "already borrowed" panic.
    assert_eq!(
        MACOS_RS
            .matches("window_ptr: Cell<Option<*mut std::ffi::c_void>>,")
            .count(),
        3,
        "the GL view, the CPU view and the delegate hold the back-pointer in a Cell"
    );
    assert!(!MACOS_RS.contains("window_ptr: RefCell<"));
}

/// Each window has a GL context of its own, and the current one is whichever
/// window drew last: WebRender's deinit, the document's GL textures and the
/// GL context pointer's shader programs, deleted in that one, free another
/// window's objects with the same numbers. On macOS a GL call with NO
/// current context crashes outright, so the teardown also gives the thread
/// back the context it found.
#[test]
fn a_macos_window_releases_its_gpu_side_in_its_own_context_and_gives_the_previous_one_back() {
    let release = body(MACOS_RS, "fn release_gpu_resources(&mut self)");
    in_order(release, "with_own_gl_context", "deinit_renderer()", "release_gpu_resources");
    at(release, "remove_document_textures", "release_gpu_resources");

    let own = body(MACOS_RS, "fn with_own_gl_context(");
    in_order(own, "currentContext", "own.makeCurrentContext()", "with_own_gl_context");
    in_order(own, "own.makeCurrentContext()", "f(self)", "with_own_gl_context");
    in_order(own, "f(self)", "previous.makeCurrentContext()", "with_own_gl_context");
    at(own, "clearCurrentContext", "with_own_gl_context");

    let drop = body(MACOS_RS, "impl Drop for MacOSWindow");
    in_order(drop, "with_own_gl_context", "layout_window.take()", "Drop for MacOSWindow");
    at(drop, "gl_context_ptr = OptionGlContextPtr::None", "Drop for MacOSWindow");

    assert!(
        !body(MACOS_RS, "pub fn close(&mut self)").contains("deinit_renderer"),
        "close() must not deinit the renderer in whatever context is current"
    );
}

/// A pass only raises `close_requested`: a window torn down inside the pass
/// that raised it (a `drawRect:`, its own resign-key notification) would run
/// the rest of that pass without its renderer, and a close its protocol
/// confirmed is not asked again. The loop carries a close out, once, and a
/// close raised while the loop is parked wakes it.
#[test]
fn a_macos_close_is_carried_out_by_the_loop_never_inside_the_pass_that_raised_it() {
    let sync = body(
        MACOS_RS,
        "fn sync_window_state(&mut self) {\n        // The native chrome",
    );
    assert!(
        !sync.contains("close_window()"),
        "sync_window_state runs inside the pass: it must not close the window"
    );
    let drain = body(MACOS_RS, "pub(crate) fn drain_loop_work(&mut self)");
    in_order(drain, "confirm_app_close", "self.close_window()", "drain_loop_work");
    assert!(
        !drain.contains("process_close_event") && !MACOS_RS.contains("fn handle_close_request"),
        "a close the protocol confirmed is not asked a second time"
    );
    at(
        body(MACOS_RS, "fn close_owed(&mut self)"),
        "loop_waker::wake()",
        "close_owed",
    );
    at(
        body(MACOS_RS, "pub(crate) fn drain_closed_windows()"),
        "inside_nested_run_loop()",
        "drain_closed_windows",
    );
    let menu = body(MACOS_RS, "pub(crate) fn present_pending_context_menu(");
    in_order(menu, "MENUS_TRACKING", "popUpMenuPositioningItem", "present_pending_context_menu");
    at(
        body(MACOS_RS, "pub fn poll_event(&mut self) -> Option<MacOSEvent>"),
        "if !self.is_open",
        "poll_event",
    );
    assert!(
        RUN_RS.matches("if !window.is_open() {").count() >= 2,
        "both macOS loops skip a window their loop work just closed"
    );
}

/// A popup its parent closes, or that dismisses itself, raises a confirmed
/// `close_requested` that no WM_CLOSE carries: the Win32 loop finishes it.
/// The teardown releases everything GL in the window's own context (the GL
/// context pointer's programs too, which the free would otherwise delete in
/// whatever context is current then), deletes the HGLRC, and gives the
/// thread back the context it had.
#[test]
fn win32_finishes_a_confirmed_close_and_releases_its_gpu_side_in_its_own_context() {
    let release = body(WINDOWS_RS, "pub(crate) fn release_gpu_resources(&mut self)");
    in_order(release, "wglGetCurrentContext", "(wgl.wglMakeCurrent)(hdc, hglrc)", "release_gpu_resources");
    in_order(release, "(wgl.wglMakeCurrent)(hdc, hglrc)", "deinit_renderer()", "release_gpu_resources");
    in_order(
        release,
        "gl_context_ptr = OptionGlContextPtr::None",
        "wglDeleteContext",
        "release_gpu_resources",
    );
    in_order(release, "wglDeleteContext", "previous_context", "release_gpu_resources");
    at(
        WINDOWS_DLOPEN_RS,
        "wglGetCurrentDC: dll.get_symbol(\"wglGetCurrentDC\")",
        "opengl32",
    );

    let finish = body(WINDOWS_RS, "pub(crate) fn finish_confirmed_close(&mut self)");
    at(finish, "self.wm_close_posted", "finish_confirmed_close");
    at(finish, "close_unconfirmed()", "finish_confirmed_close");
    in_order(finish, "self.release_gpu_resources()", "DestroyWindow", "finish_confirmed_close");
    at(RUN_RS, "window.finish_confirmed_close()", "the Win32 loop");
}

/// The run loop drops an X11 window after `close` destroyed it and, for the
/// last window, closed the display the GC and the EGL display point into: the
/// graphics go in `close`, before either.
#[test]
fn x11_releases_its_graphics_before_the_window_and_the_display_go() {
    in_order(
        body(X11_RS, "pub fn close(&mut self)"),
        "self.release_graphics()",
        "XDestroyWindow",
        "X11Window::close",
    );
    let release = body(X11_RS, "fn release_graphics(&mut self)");
    for (first, then) in [
        ("EglBinding::current", "gl_context.make_current()"),
        ("gl_context.make_current()", "deinit_renderer()"),
        ("deinit_renderer()", "gl_context_ptr = OptionGlContextPtr::None"),
        ("gl_context_ptr = OptionGlContextPtr::None", "self.render_mode = RenderMode::None"),
        ("self.render_mode = RenderMode::None", "restore_unless"),
    ] {
        in_order(release, first, then, "release_graphics");
    }
    at(release, "XFreeGC", "release_graphics");
    in_order(
        body(X11_RS, "impl Drop for X11Window"),
        "self.release_graphics()",
        "self.close()",
        "Drop for X11Window",
    );
    at(X11_GL_RS, "pub fn restore_unless(", "EglBinding");
    at(X11_DLOPEN_RS, "load_symbol!(lib, _, \"eglGetCurrentContext\")", "Egl");
}

/// Wayland's `close` releases the GPU side in the window's own context, and
/// gives the thread back the binding it had.
#[test]
fn wayland_releases_its_gpu_side_in_its_own_context() {
    let release = body(WAYLAND_RS, "fn release_gpu_in_own_context(&mut self)");
    in_order(release, "gl_context.make_current()", "deinit_renderer()", "release_gpu_in_own_context");
    at(release, "gl_context_ptr = OptionGlContextPtr::None", "release_gpu_in_own_context");
    // The first `close` in the file is the toplevel's (the popup's comes later).
    let close = body(WAYLAND_RS, "pub fn close(&mut self)");
    in_order(close, "self.egl_binding()", "self.release_gpu_in_own_context()", "WaylandWindow::close");
    in_order(close, "self.release_gpu_in_own_context()", "restore_unless", "WaylandWindow::close");
    let drop = body(WAYLAND_RS, "impl Drop for WaylandWindow");
    in_order(drop, "RenderMode::Cpu(None)", "restore_unless", "Drop for WaylandWindow");
}

/// The shared half: a close request tells the backend (`close_owed`), and a
/// popup that is closing already is not closed a second time when its
/// parent's close reaches it too.
#[test]
fn a_close_request_tells_the_backend_and_a_closing_popup_is_not_closed_twice() {
    at(EVENT_RS, "fn close_owed(&mut self) {}", "PlatformWindow");
    at(
        body(EVENT_RS, "fn request_window_close(&mut self, site: &str)"),
        "self.close_owed()",
        "request_window_close",
    );
    at(
        body(EVENT_RS, "fn request_close_by_app(&mut self)"),
        "self.close_owed()",
        "request_close_by_app",
    );
    at(
        body(EVENT_RS, "fn poll_transient_mailbox(&mut self)"),
        "PopupAction::Close if self.get_current_window_state().flags.close_requested",
        "poll_transient_mailbox",
    );
}
