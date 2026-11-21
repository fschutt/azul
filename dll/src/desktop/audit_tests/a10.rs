//! A10, the present path and DPI.
//!
//! Source-text guards for the shells a test binary on one host cannot run
//! (as `window_teardown_invariants` does for teardown): the shapes the CPU
//! present of each shell depends on. The arithmetic itself is tested where
//! it lives (`azul_layout::window::present_geometry`, and the macOS shell's
//! own `audit_a10_tests`); the findings are in `audit/A10.md`.

const MACOS_RS: &str = include_str!("../shell2/macos/mod.rs");
const MACOS_EVENTS_RS: &str = include_str!("../shell2/macos/events.rs");
const WINDOWS_RS: &str = include_str!("../shell2/windows/mod.rs");
const X11_RS: &str = include_str!("../shell2/linux/x11/mod.rs");
const WAYLAND_RS: &str = include_str!("../shell2/linux/wayland/mod.rs");
const WAYLAND_EVENTS_RS: &str = include_str!("../shell2/linux/wayland/events.rs");
const ANDROID_RS: &str = include_str!("../shell2/android/mod.rs");

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

/// Every place `needle` occurs in `haystack`.
fn all(haystack: &str, needle: &str) -> Vec<usize> {
    haystack.match_indices(needle).map(|(i, _)| i).collect()
}

/// Whether `marker` occurs within `reach` bytes before or after `at`.
fn near(haystack: &str, at: usize, marker: &str, reach: usize) -> bool {
    let mut from = at.saturating_sub(reach);
    while !haystack.is_char_boundary(from) {
        from -= 1;
    }
    let mut to = (at + reach).min(haystack.len());
    while !haystack.is_char_boundary(to) {
        to += 1;
    }
    haystack[from..to].contains(marker)
}

/// A10-08. A CPU frame at another DPI factor shares no pixel with the frame
/// before it, and the display list (logical px) is the same at both: the
/// frame-to-frame diff finds nothing and the old scale's pixels stay. So
/// every write of `size.dpi` a shell makes while it runs raises
/// `CpuBackend::force_full_repaint`, next to the write.
///
/// (`CpuBackend::render_frame` remembering the factor itself would make these
/// calls redundant; until it does, this is what keeps a window dragged to a
/// display of another scale from showing a quarter-size or a doubled frame.)
///
/// Mutation: removing the call at any of the eight writes fails here.
#[test]
fn every_dpi_write_of_a_shell_forces_a_full_cpu_frame() {
    // (shell, source, the write, the flag, how many writes, how near)
    let writes: [(&str, &str, &str, &str, usize, usize); 7] = [
        (
            "macOS handle_dpi_change",
            MACOS_RS,
            "ws.size.dpi = (new_hidpi.inner.get() * BASE_DPI) as u32",
            "self.cpu_frame_scale_changed();",
            1,
            600,
        ),
        (
            "macOS sync_window_size_from_content_view",
            MACOS_RS,
            "|ws| ws.size.dpi = new_dpi);",
            "self.cpu_frame_scale_changed();",
            1,
            400,
        ),
        (
            "macOS handle_resize",
            MACOS_EVENTS_RS,
            "|ws| ws.size.dpi = new_dpi,",
            "self.cpu_frame_scale_changed();",
            1,
            300,
        ),
        (
            "X11 monitor change",
            X11_RS,
            "ws.size.dpi = new_dpi;",
            "self.cpu_backend.force_full_repaint = true;",
            1,
            1800,
        ),
        (
            "Wayland apply_os_dpi_change",
            WAYLAND_EVENTS_RS,
            "ws.size.dpi = new_dpi;",
            "window.cpu_backend.force_full_repaint = true;",
            1,
            600,
        ),
        (
            "Win32 WM_DPICHANGED",
            WINDOWS_RS,
            "|ws| ws.size.dpi = new_dpi,",
            "window.cpu_backend.force_full_repaint = true;",
            1,
            700,
        ),
        (
            "Android density",
            ANDROID_RS,
            "ws.size.dpi = dpi",
            "window.cpu_backend.force_full_repaint = true;",
            2,
            700,
        ),
    ];
    for (shell, source, write, flag, count, reach) in writes {
        let at = all(source, write);
        assert_eq!(
            at.len(),
            count,
            "{shell}: `{write}` occurs {} times, expected {count} - a new write of the DPI has to \
             raise the flag too, and this list has to name it",
            at.len()
        );
        for i in at {
            assert!(
                near(source, i, flag, reach),
                "{shell}: the write of `size.dpi` is not followed by `{flag}`"
            );
        }
    }
    // The one door of the macOS shell raises the backend's flag.
    assert!(
        body(MACOS_RS, "fn cpu_frame_scale_changed(&mut self)")
            .contains("self.cpu_backend.force_full_repaint = true;"),
        "macOS: cpu_frame_scale_changed no longer forces a full repaint"
    );
}

/// A10-07. The macOS view's framebuffer is the render target of the next
/// frame and the base of every partial present only while it holds the frame
/// before, pixel for pixel:
/// - a frame of another size is rescaled into it, and that clears the flag;
/// - `drawRect:` keeps old pixels across a resize only at the same scale;
/// - a framebuffer made anew is marked blank, and the present of an owned
///   frame whose damage is `None` copies the whole frame then.
///
/// Mutations: `fb_content_valid.set(true)` after the rescale, dropping
/// `&& !scale_changed`, or dropping the `shows_blank` arm each fail here; on
/// a real window each shows a stretched or blank view after a zoom.
#[test]
fn the_macos_view_never_takes_a_stretched_or_blank_buffer_for_the_frame_before() {
    let update = body(MACOS_RS, "pub fn update_framebuffer(");
    let rescale = update
        .find("} else if pixmap_w > 0 && pixmap_h > 0 {")
        .expect("the rescaling branch of update_framebuffer");
    assert!(
        update[rescale..].contains("fb_content_valid.set(false);"),
        "a rescaled frame must not validate the framebuffer"
    );
    assert!(
        !update[rescale..].contains("fb_content_valid.set(true);"),
        "a rescaled frame must not validate the framebuffer"
    );
    let same_size = update
        .find("if pixmap_w == view_w && pixmap_h == view_h {\n            let len")
        .expect("the 1:1 branch of update_framebuffer");
    assert!(
        update[same_size..rescale].contains("fb_content_valid.set(true);")
            && update[same_size..rescale].contains("fb_blank.set(false);"),
        "a 1:1 copy of the whole frame validates the framebuffer and ends its blank state"
    );

    let draw = body(MACOS_RS, "fn draw_rect(&self, dirty_rect: NSRect)");
    assert!(
        draw.contains("&& !scale_changed;"),
        "drawRect: must not keep pixels across a change of the backing scale"
    );
    assert!(
        draw.contains("if !kept {") && draw.contains("ivars.fb_blank.set(true);"),
        "drawRect: must mark a framebuffer it made anew as blank"
    );

    let native = body(MACOS_RS, "pub fn native_target_ptr(");
    assert!(
        native.contains("if !ivars.fb_content_valid.get() {\n            return None;"),
        "an invalid framebuffer must not become the render target"
    );

    let present = body(MACOS_RS, "pub fn render_and_present_in_draw_rect(");
    let blank_arm = present
        .find("if !cpu_view.shows_blank() => {}")
        .expect("the present of an owned frame must ask whether the view shows the blank fill");
    let copy = present[blank_arm..]
        .find("cpu_view.update_framebuffer(")
        .expect("a blank view gets the whole frame");
    let rects_arm = present[blank_arm..]
        .find("damage => {")
        .expect("the arm of a frame with damage");
    assert!(copy < rects_arm, "the full copy of a blank view comes before the damage arm");
}

/// A10-01, A10-02. Every conversion of the macOS present goes through the
/// tested functions: no arithmetic of its own is left in the Objective-C
/// methods.
#[test]
fn the_macos_present_uses_the_shared_conversions() {
    let draw = body(MACOS_RS, "fn draw_rect(&self, dirty_rect: NSRect)");
    for call in [
        "cpu_view_framebuffer_size(",
        "dirty_rows_to_copy(",
        "bitmap_source_rect(",
        "draw_framebuffer_span(",
    ] {
        assert!(draw.contains(call), "drawRect: no longer calls `{call}`");
    }
    assert!(
        !draw.contains("backing.size.width as usize"),
        "drawRect: sizes the framebuffer by truncation again (A10-02)"
    );
    let invalidate = body(MACOS_RS, "fn invalidate_px_rects(");
    assert!(
        invalidate.contains("present_rect_in_view_points("),
        "the invalidation no longer goes through present_rect_in_view_points"
    );
    assert!(
        body(MACOS_RS, "fn dirty_rows_to_copy(").contains("rows_of_bottom_left_rect("),
        "dirty_rows_to_copy no longer is the shared row arithmetic"
    );
    assert!(
        body(MACOS_RS, "pub fn render_and_present_in_draw_rect(").contains("frame_size_px(width, height, dpi)"),
        "the size of the macOS frame is no longer the shared formula"
    );
}

/// A10-04 (checked: fine). Every shell turns the renderer's damage into
/// device rects with the ONE conversion (`FrameDamage::to_present_rects_physical`,
/// outward rounding, clamped), none with arithmetic of its own.
#[test]
fn every_shell_presents_through_the_one_damage_conversion() {
    for (shell, source) in [
        ("macOS", MACOS_RS),
        ("Win32", WINDOWS_RS),
        ("X11", X11_RS),
        ("Wayland", WAYLAND_RS),
    ] {
        assert!(
            source.contains(".to_present_rects_physical("),
            "{shell}: the CPU present no longer uses FrameDamage::to_present_rects_physical"
        );
    }
}

/// A10-05. X11 and Win32 learn their size in device pixels and store
/// `px / dpi` as the logical size; the frame is `ceil(logical * dpi)` of what
/// the shell hands `render_frame`. They hand it
/// `present_geometry::logical_len_for_px(px, dpi)` (tested exhaustively in
/// azul-layout), so the frame is the window's size and not one pixel more
/// (1003 px at 175 %: 1004 without it).
///
/// Mutation: passing `ws.size.dimensions.width` to `render_frame` again
/// fails here.
#[test]
fn x11_and_win32_render_a_frame_of_the_windows_pixel_size() {
    for (shell, source, decl) in [
        ("X11", X11_RS, "if let RenderMode::Cpu(gc) = &self.render_mode {"),
        ("Win32", WINDOWS_RS, "pub fn render_and_present(&mut self, layout_was_regenerated: bool)"),
    ] {
        let arm = body(source, decl);
        let render = arm
            .find("self.cpu_backend.render_frame(")
            .unwrap_or_else(|| panic!("{shell}: the CPU arm no longer calls render_frame"));
        let before = &arm[..render];
        assert!(
            before.matches("present_geometry::surface_len_px(").count() == 2
                && before.matches("present_geometry::logical_len_for_px(").count() == 2,
            "{shell}: width and height of the CPU frame must come from the window's pixel size"
        );
        assert!(
            !before.contains("let width = ws.size.dimensions.width;"),
            "{shell}: the CPU frame is sized from the stored logical size again"
        );
        // A10-17: and that pixel size is round(logical * THE factor), never
        // `get_physical_size()` / `get_hidpi_factor()` of the three decimals.
        assert!(
            !before.contains("get_physical_size()") && !before.contains("get_hidpi_factor()"),
            "{shell}: the CPU frame is sized with the three-decimal factor again"
        );
    }
}

const IOS_RS: &str = include_str!("../shell2/ios/mod.rs");
const CORE_WINDOW_RS: &str = include_str!("../../../../core/src/window.rs");

/// A10-17. Every shell renders its frame at THE factor of the window
/// (`LayoutWindow::frame_scale()`, or `present_geometry::frame_dpi_factor` of
/// the state's DPI where no layout window is at hand): `dpi / 96`, not
/// `WindowSize::get_hidpi_factor()`, which is cut to three decimals (1.333 for
/// 4/3) and makes a frame, a buffer or a logical size that is a pixel row or
/// column off the other one. X11 derives its logical sizes with the same
/// factor (`hidpi()`), and `WindowSize::get_physical_size()` uses it too.
///
/// Mutations: `get_hidpi_factor()` back in `X11Window::hidpi` or in
/// `WindowSize::get_physical_size`, or `ws.size.dpi as f32 / 96.0` back in an
/// arm, fail here.
#[test]
fn every_shell_renders_at_the_windows_frame_factor() {
    // (shell, source, where its CPU frame starts, the factor expression, how many frames)
    let arms: [(&str, &str, &str, &str, usize); 6] = [
        (
            "macOS",
            MACOS_RS,
            "pub fn render_and_present_in_draw_rect(",
            "let dpi = layout_window.frame_scale();",
            1,
        ),
        (
            "X11",
            X11_RS,
            "if let RenderMode::Cpu(gc) = &self.render_mode {",
            "let dpi = layout_window.frame_scale();",
            1,
        ),
        (
            "Win32",
            WINDOWS_RS,
            "pub fn render_and_present(&mut self, layout_was_regenerated: bool)",
            "let dpi = layout_window.frame_scale();",
            1,
        ),
        (
            "Wayland",
            WAYLAND_RS,
            "RenderMode::Cpu(Some(cpu_state)) => {",
            "let dpi = layout_window.frame_scale();",
            1,
        ),
        (
            "iOS",
            IOS_RS,
            "impl IOSWindow {",
            "let dpi = azul_layout::window::present_geometry::frame_dpi_factor(ws.size.dpi);",
            2,
        ),
        (
            "Android",
            ANDROID_RS,
            "impl AndroidWindow {",
            "let dpi = azul_layout::window::present_geometry::frame_dpi_factor(ws.size.dpi);",
            1,
        ),
    ];
    for (shell, source, start, factor, frames) in arms {
        let from = source
            .find(start)
            .unwrap_or_else(|| panic!("{shell}: `{start}` not found - was it renamed?"));
        assert_eq!(
            source[from..].matches(factor).count(),
            frames,
            "{shell}: its CPU frame(s) must be rendered at `{factor}`"
        );
        assert!(
            !source.contains("let dpi = ws.size.dpi as f32 / 96.0;")
                && !source.contains("let dpi = ws.size.dpi as f32 / BASE_DPI;"),
            "{shell}: a frame is rendered at a factor of its own"
        );
    }
    let hidpi = body(X11_RS, "pub(crate) fn hidpi(&self) -> f32");
    assert!(
        hidpi.contains("present_geometry::frame_dpi_factor(") && !hidpi.contains(".get_hidpi_factor()"),
        "X11: logical sizes must be derived with the frame's factor"
    );
    let physical = body(CORE_WINDOW_RS, "pub fn get_physical_size(&self) -> PhysicalSize<u32>");
    assert!(
        physical.contains("dpi as f32 / 96.0") && !physical.contains(".get_hidpi_factor()"),
        "WindowSize::get_physical_size must use dpi / 96, not the three-decimal factor"
    );
}

/// A10-09. At a fractional scale a Wayland toplevel is `round(logical *
/// scale)` device pixels (wp-fractional-scale-v1), and both the shm buffers
/// and the CPU frame are that size: the buffers through `cpu_buffer_spec`,
/// the frame because the CPU arm hands `render_frame` the logical size that
/// renders to it. `ceil` on either side is one pixel more for one size in
/// four at 125 % and 175 %, and the compositor resamples the whole window.
///
/// Mutation: `.ceil()` in `cpu_buffer_spec`, or the stored logical size
/// handed to `render_frame` at a fractional scale, fails here.
#[test]
fn wayland_buffers_and_frame_are_the_surfaces_size_at_a_fractional_scale() {
    let spec = body(WAYLAND_RS, "fn cpu_buffer_spec(&self, logical_w: i32, logical_h: i32)");
    assert!(
        spec.matches("present_geometry::surface_len_px(").count() == 2 && !spec.contains(".ceil()"),
        "cpu_buffer_spec must size fractional buffers by round-half-away, not ceil"
    );
    let arm_start = WAYLAND_RS
        .find("RenderMode::Cpu(Some(cpu_state)) => {")
        .expect("the CPU arm of the Wayland present");
    let arm = &WAYLAND_RS[arm_start..];
    let render = arm
        .find("self.cpu_backend.render_frame(")
        .expect("the CPU arm calls render_frame");
    let before = &arm[..render];
    assert!(
        before.contains("let (width, height) = if fractional")
            && before.contains("logical_len_for_px(surface_len_px(logical_w, d), d)")
            && before.contains("logical_len_for_px(surface_len_px(logical_h, d), d)"),
        "at a fractional scale the frame must be sized from the surface's device pixels"
    );
}

/// A10-02. The macOS window state follows the view's bounds not only when
/// they differ by more than half a point, but whenever the two would make CPU
/// frames of different pixel sizes: the frame is sized from the state, the
/// view's framebuffer from the bounds, and a frame of another size than the
/// view is rescaled into it on every present.
///
/// 800.2 pt in the state, 800 pt in the view, 2x: `ceil(1600.4)` = 1601 px
/// against 1600 px, inside the 0.5 pt tolerance.
///
/// Mutation: dropping `|| frame_differs` fails here.
#[test]
fn the_macos_state_follows_the_view_when_their_frames_differ() {
    let sync = body(MACOS_RS, "fn sync_window_size_from_content_view(&mut self)");
    assert!(
        sync.contains("let frame_differs = azul_layout::window::present_geometry::frame_size_px(")
            && sync.contains("|| frame_differs"),
        "the size sync no longer looks at the frame sizes"
    );
    use azul_layout::window::present_geometry::frame_size_px;
    assert_ne!(frame_size_px(800.2, 600.0, 2.0), frame_size_px(800.0, 600.0, 2.0));
    assert_eq!(frame_size_px(800.2, 600.0, 2.0), (1601, 1200));
    assert_eq!(frame_size_px(800.0, 600.0, 2.0), (1600, 1200));
}

/// A10-10. On the fractional path a Wayland window sizes its shm buffers and
/// renders its frame with the compositor's own scale (`preferred_scale /
/// 120`), not with `size.dpi / 96`, its rounding to a whole DPI: 110 % is DPI
/// 106 = 1.104, and 1000 surface units became a 1104 px buffer for an 1100 px
/// surface (the arithmetic: `a_wayland_frame_is_sized_with_the_compositors_scale_not_with_the_rounded_dpi`
/// in azul-layout). The layout side is told the factor
/// (`LayoutWindow::set_frame_scale`), and a scale that changes without
/// changing the whole DPI still remakes the buffers.
///
/// Mutations: `size.dpi as f32 / 96.0` back in `cpu_buffer_spec`, the
/// `set_frame_scale` line dropped from the arm, or the early `return` of the
/// handler without `apply_frame_scale_change`, each fail here.
#[test]
fn wayland_sizes_and_renders_with_the_compositors_preferred_scale() {
    let factor = body(WAYLAND_RS, "fn frame_factor(&self) -> f32");
    assert!(
        factor.contains("present_geometry::wayland_frame_scale(")
            && factor.contains("self.preferred_scale_120")
            && factor.contains("self.viewport.is_some()"),
        "frame_factor must be the preferred scale on the fractional path"
    );
    let spec = body(WAYLAND_RS, "fn cpu_buffer_spec(&self, logical_w: i32, logical_h: i32)");
    assert!(
        spec.contains("let d = self.frame_factor().max(0.01);") && !spec.contains("size.dpi as f32"),
        "the shm buffers must be sized with frame_factor()"
    );
    let arm_start = WAYLAND_RS
        .find("RenderMode::Cpu(Some(cpu_state)) => {")
        .expect("the CPU arm of the Wayland present");
    let arm = &WAYLAND_RS[arm_start..];
    let set = arm
        .find("layout_window.set_frame_scale(fractional.then_some(frame_factor));")
        .expect("the arm must tell the layout window the frame's factor");
    let read = arm
        .find("let dpi = layout_window.frame_scale();")
        .expect("the arm renders at the window's frame factor");
    assert!(set < read, "the factor is said before it is read");

    let handler = body(
        WAYLAND_EVENTS_RS,
        "pub(super) extern \"C\" fn wp_fractional_scale_preferred_scale_handler(",
    );
    let same_dpi = handler
        .find("if new_dpi == old_dpi {")
        .expect("the handler's same-DPI branch");
    assert!(
        handler[same_dpi..].contains("if window.frame_factor() != factor_before {")
            && handler[same_dpi..].contains("apply_frame_scale_change(window);"),
        "a scale that changes within one whole DPI must still remake the buffers"
    );
    let apply = body(WAYLAND_EVENTS_RS, "fn apply_frame_scale_change(window: &mut WaylandWindow)");
    assert!(
        apply.contains("window.cpu_backend.force_full_repaint = true;")
            && apply.contains("window.publish_frame_factor();")
            && apply.contains("window.resize_surface(w, h);")
            && apply.contains("window.request_redraw();"),
        "a scale change remakes the buffers and repaints the frame in full"
    );
}
