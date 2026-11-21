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
