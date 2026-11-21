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

/// A10-11. The iOS present hands Core Graphics a copy of the frame. It takes
/// the copy's buffer from the pool the release callback fills (two buffers
/// alternate: `a_run_of_whole_frame_presents_allocates_two_buffers` in
/// azul-layout), and it hands nothing over while the layer still shows the
/// frame (`frame_already_shown`: no frame since, or one that changed nothing).
///
/// Mutations: `Box::from(pixmap.data())` back in `display_layer`, the release
/// callback dropping the box, or the skip removed, fail here.
#[test]
fn the_ios_present_reuses_its_frame_copies_and_skips_an_unchanged_frame() {
    let present = body(IOS_RS, "extern \"C\" fn display_layer(");
    assert!(
        present.contains("pool.take(len)") && !present.contains("Box::from(pixmap.data())"),
        "the frame copy must come from the pool"
    );
    let skip = present
        .find("present_geometry::frame_already_shown(")
        .expect("the present must ask whether the layer already shows the frame");
    let copy = present.find("pool.take(len)").expect("the copy");
    assert!(skip < copy, "the skip comes before the copy");
    assert!(
        present.contains("let contents: *mut Object = msg_send![layer, contents];"),
        "a layer without contents is never skipped"
    );
    let release = body(IOS_RS, "extern \"C\" fn release_frame_pixels(");
    assert!(
        release.contains("pool.give_back(buffer);"),
        "the release callback must give the buffer back to the pool"
    );
}

/// A10-13, A10-14, A10-06: the wiring of the macOS CPU view's `drawRect:`
/// (the arithmetic is tested in `macos/mod.rs`, `audit_a10_tests`).
/// - the rects being drawn are recorded before the frame is rendered, and
///   the invalidation skips what they contain (one present per frame);
/// - a resize moves the framebuffer's rows in place and reuses a bitmap that
///   is large enough;
/// - with `AZ_CPU_PRESENT=cgimage` the dirty span is drawn straight from the
///   framebuffer, and a view whose image Core Graphics held goes back to the
///   bitmap for good.
///
/// Mutations: any of the calls below removed from `drawRect:`, or the filter
/// removed from `invalidate_px_rects`, fails here.
#[test]
fn the_macos_draw_rect_presents_once_resizes_in_place_and_can_draw_from_the_framebuffer() {
    let draw = body(MACOS_RS, "fn draw_rect(&self, dirty_rect: NSRect)");
    let record = draw
        .find("*ivars.drawing_spans.borrow_mut() = self.spans_being_drawn(dirty_rect, width, height);")
        .expect("drawRect: records what it draws");
    let render = draw
        .find("macos_window.render_and_present_in_draw_rect();")
        .expect("drawRect: renders");
    let clear = draw
        .find("ivars.drawing_spans.borrow_mut().clear();")
        .expect("drawRect: forgets what it drew");
    assert!(record < render && render < clear, "recorded before the render, cleared after it");
    for call in [
        "resize_framebuffer(",
        "bitmap_size_to_allocate(width, height, have)",
        "bitmap_source_rect(span, bitmap_h)",
        "draw_framebuffer_span_cg(&framebuffer, width, height, span, to_pt)",
        "ivars.cg_tripped.set(true);",
    ] {
        assert!(draw.contains(call), "drawRect: no longer has `{call}`");
    }
    let invalidate = body(MACOS_RS, "fn invalidate_px_rects(");
    assert!(
        invalidate.contains("rects_left_to_invalidate(rects, &self.ivars().drawing_spans.borrow())"),
        "the invalidation must skip what the running drawRect: draws"
    );
    let spans = body(MACOS_RS, "fn spans_being_drawn(");
    assert!(
        spans.contains("getRectsBeingDrawn_count(") && spans.contains("rows_inside_backing_rect("),
        "what is being drawn comes from AppKit's own list, cut inward"
    );
}

const MACOS_A11Y_RS: &str = include_str!("../shell2/macos/accessibility.rs");

/// A10-19 (the shell side of A3-core-10). On macOS the accessibility tree is
/// built on demand: the layout side only notes that a pass is due, and the
/// shell hands the adapter a closure that builds the update. accesskit runs
/// that closure only while an assistive technology is active, so with none:
/// no tree is built, and the "last tree sent" does not advance.
/// - the phase of the feed decides `full`: the first update after an
///   activation (and after a missed one) is the whole tree;
/// - `build(full)` is called INSIDE `update_if_active` and nowhere else;
/// - the on-demand delivery never takes a parked update (`take_pending`):
///   nothing is parked on that path;
/// - an activation schedules a delivery, so the tree arrives without a layout.
///
/// Mutations: `build(full)` hoisted out of the closure, `wants_full_tree`
/// true only while awaiting, or the forced activation left in the on-demand
/// branch of `init_accessibility`, each fail here.
#[test]
fn the_macos_accessibility_tree_is_built_inside_the_adapters_closure() {
    let on_demand = body(MACOS_A11Y_RS, "pub fn update_tree_on_demand(");
    let closure = on_demand
        .find("self.adapter.update_if_active(|| {")
        .expect("the update is handed over through update_if_active");
    let build = on_demand.find("build(full)").expect("the update is built by the caller's closure");
    assert!(closure < build, "the tree must be built inside update_if_active's closure");
    assert_eq!(on_demand.matches("build(full)").count(), 1, "and nowhere else");
    assert!(
        on_demand.contains("let full = self.feed.wants_full_tree();")
            && on_demand.contains("self.feed.built_and_taken();")
            && on_demand.contains("self.feed.missed();"),
        "the feed's phase decides whether the whole tree is asked for"
    );

    let handler = body(MACOS_A11Y_RS, "fn request_initial_tree(&mut self) -> Option<TreeUpdate>");
    assert!(
        handler.contains("self.feed.placeholder_requested();")
            && handler.contains("after_turn::a11y_delivery().request(self.window);"),
        "an activation must schedule the delivery of the first tree"
    );

    let deliver = body(MACOS_RS, "fn deliver_a11y_on_demand(&mut self, note_pass: bool)");
    assert!(
        deliver.contains("if lw.a11y_update_pending() || adapter.awaiting_tree() {")
            && deliver.contains("adapter.update_tree_on_demand(|full| lw.a11y_update_on_demand(full));")
            && !deliver.contains("take_pending"),
        "the on-demand delivery builds inside the adapter's closure and parks nothing"
    );

    let init = body(MACOS_RS, "fn init_accessibility(&mut self)");
    let lazy = init.find("if on_demand {").expect("the on-demand branch of init_accessibility");
    let forced = init
        .find("accessibilityChildren]")
        .expect("the eager path still activates the adapter itself");
    let lazy_branch = &init[lazy..forced];
    assert!(
        lazy_branch.contains("lw.a11y_manager.on_demand = true;") && lazy_branch.contains("return;"),
        "the on-demand branch must leave before the forced activation"
    );
}

/// A10-19. The phases an adapter that builds on demand goes through: no tree
/// (the whole one is wanted), activated and waiting, active (patches), and
/// back to "the whole one" after an update it did not take.
#[cfg(feature = "a11y")]
#[test]
fn an_adapter_built_on_demand_gets_the_whole_tree_first_and_after_a_miss() {
    use crate::desktop::shell2::common::accessibility::A11yTreeFeed;
    let feed = A11yTreeFeed::new();
    assert!(feed.wants_full_tree() && !feed.awaiting_tree(), "inactive: nothing is owed yet");
    feed.placeholder_requested();
    assert!(feed.wants_full_tree() && feed.awaiting_tree(), "activated: the whole tree is owed");
    feed.built_and_taken();
    assert!(!feed.wants_full_tree() && !feed.awaiting_tree(), "active: patches from here on");
    feed.missed();
    assert!(feed.wants_full_tree() && feed.awaiting_tree(), "a missed update: the whole tree again");
    feed.built_and_taken();
    assert!(!feed.wants_full_tree());
}

/// A10-26. The macOS shell gives regenerable memory back at three moments,
/// through one function (`MacOSWindow::release_regenerable_memory`): the
/// window goes to the Dock (A9-09), the app is hidden (Cmd-H), the system
/// reports memory pressure (a dispatch source for the warning and the
/// critical level, on the main queue).
///
/// Mutations: `applicationDidHide:` removed, the source made for another
/// mask or never resumed, or a release that no longer drops the compositor,
/// each fail here.
#[test]
fn the_macos_shell_gives_memory_back_when_hidden_and_under_pressure() {
    assert!(
        MACOS_RS.contains("#[unsafe(method(applicationDidHide:))]")
            && body(MACOS_RS, "fn application_did_hide(")
                .contains("release_memory_of_every_window(MemoryRelease::NotShown);"),
        "a hidden app gives back what a minimised window gives back"
    );
    let install = body(MACOS_RS, "pub fn install() {");
    for call in [
        "core::ptr::addr_of!(_dispatch_source_type_memorypressure)",
        "WARN | CRITICAL",
        "core::ptr::addr_of!(_dispatch_main_q)",
        "dispatch_source_set_event_handler_f(source, Some(on_memory_pressure));",
        "dispatch_resume(source);",
    ] {
        assert!(install.contains(call), "the memory-pressure source: `{call}` is missing");
    }
    assert!(
        MACOS_RS.matches("memory_pressure::install();").count() >= 2,
        "the source is installed at launch and with the first window"
    );
    let release = body(MACOS_RS, "pub(crate) fn release_regenerable_memory(");
    for call in [
        "release_plan(reason, keep_caches())",
        // The compositor goes with everything else the CPU backend keeps (A1-29).
        "self.cpu_backend.release_caches();",
        "self.cpu_backend.glyph_cache.gc();",
        "cpu_view.release_staging_bitmap();",
        "api.notify_memory_pressure();",
        "azul_layout::probe::hint_purge_allocator();",
    ] {
        assert!(release.contains(call), "the release: `{call}` is missing");
    }
    assert!(
        body(MACOS_RS, "extern \"C\" fn on_memory_pressure(")
            .contains("super::release_memory_of_every_window(reason);"),
        "a pressure event releases through the same function"
    );
    assert!(
        body(MACOS_RS, "fn window_did_miniaturize(")
            .contains("macos_window.release_regenerable_memory(MemoryRelease::NotShown);"),
        "and so does a window that goes to the Dock"
    );
}
