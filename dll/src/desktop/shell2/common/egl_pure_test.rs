//! Tests of `egl_pure`: the tables the EGL binding of the GPU path is made of.

use super::*;

#[test]
fn error_codes_have_their_names() {
    assert_eq!(egl_error_name(EGL_SUCCESS), "EGL_SUCCESS");
    assert_eq!(egl_error_name(0x300B), "EGL_BAD_NATIVE_WINDOW");
    assert_eq!(egl_error_name(0x300E), "EGL_CONTEXT_LOST");
    assert_eq!(egl_error_name(0x3009), "EGL_BAD_MATCH");
    assert_eq!(egl_error_name(0x1234), "an unknown EGL error");
}

/// The constants are the ones of the EGL headers (`EGL/egl.h`, `EGL/eglext.h`): a wrong digit here is a context that
/// is asked for the wrong thing on a device nobody can attach a debugger to.
#[test]
fn the_constants_are_the_ones_of_the_headers() {
    assert_eq!(EGL_NONE, 0x3038);
    assert_eq!(EGL_RENDERABLE_TYPE, 0x3040);
    assert_eq!(EGL_OPENGL_ES3_BIT, 0x40);
    assert_eq!(EGL_OPENGL_ES_API, 0x30A0);
    assert_eq!(EGL_CONTEXT_CLIENT_VERSION, 0x3098);
    assert_eq!(EGL_NATIVE_VISUAL_ID, 0x302E);
    assert_eq!(EGL_BUFFER_AGE_EXT, 0x313D);
    assert_eq!(EGL_WINDOW_BIT, 4);
    assert_eq!(EGL_PBUFFER_BIT, 1);
    assert_eq!(EGL_EXTENSIONS, 0x3055);
}

#[test]
fn a_display_says_what_it_can_do_in_its_extension_string() {
    let phone = "EGL_ANDROID_blob_cache EGL_KHR_surfaceless_context EGL_EXT_buffer_age \
                 EGL_KHR_swap_buffers_with_damage EGL_KHR_partial_update";
    let caps = EglCaps::from_extensions(phone);
    assert!(caps.surfaceless_context && caps.buffer_age && caps.swap_with_damage_khr);
    assert!(!caps.swap_with_damage_ext);
    assert_eq!(caps.swap_with_damage_symbol(), Some("eglSwapBuffersWithDamageKHR"));

    let ext_only = EglCaps::from_extensions("EGL_EXT_swap_buffers_with_damage");
    assert_eq!(ext_only.swap_with_damage_symbol(), Some("eglSwapBuffersWithDamageEXT"));

    // Both: the KHR one wins.
    let both = EglCaps::from_extensions(
        "EGL_EXT_swap_buffers_with_damage EGL_KHR_swap_buffers_with_damage",
    );
    assert_eq!(both.swap_with_damage_symbol(), Some("eglSwapBuffersWithDamageKHR"));

    // Nothing, or an extension whose name only CONTAINS one of ours, gives nothing.
    assert_eq!(EglCaps::from_extensions(""), EglCaps::default());
    assert_eq!(
        EglCaps::from_extensions("EGL_KHR_surfaceless_context_not EGL_EXT_buffer_age_x"),
        EglCaps::default()
    );
    assert_eq!(EglCaps::default().swap_with_damage_symbol(), None);
}

#[test]
fn the_config_request_is_rgba8_es3_and_asks_the_surface_type_given() {
    let list = CONFIG_WISHES[0].attribs(EGL_WINDOW_BIT | EGL_PBUFFER_BIT);
    assert_eq!(list.last(), Some(&EGL_NONE));
    assert_eq!(list.len() % 2, 1, "key/value pairs and the terminator");
    let pairs: Vec<(EglInt, EglInt)> = list[..list.len() - 1]
        .chunks_exact(2)
        .map(|c| (c[0], c[1]))
        .collect();
    let value = |key| pairs.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
    for channel in [EGL_RED_SIZE, EGL_GREEN_SIZE, EGL_BLUE_SIZE, EGL_ALPHA_SIZE] {
        assert_eq!(value(channel), Some(8));
    }
    assert_eq!(value(EGL_RENDERABLE_TYPE), Some(EGL_OPENGL_ES3_BIT));
    assert_eq!(value(EGL_SURFACE_TYPE), Some(5));
    assert_eq!(value(EGL_DEPTH_SIZE), Some(0));
    assert_eq!(value(EGL_STENCIL_SIZE), Some(0));

    let pbuffer_only = CONFIG_WISHES[2].attribs(EGL_PBUFFER_BIT);
    let pairs: Vec<(EglInt, EglInt)> = pbuffer_only[..pbuffer_only.len() - 1]
        .chunks_exact(2)
        .map(|c| (c[0], c[1]))
        .collect();
    assert!(pairs.contains(&(EGL_SURFACE_TYPE, EGL_PBUFFER_BIT)));
    assert!(pairs.contains(&(EGL_DEPTH_SIZE, 24)) && pairs.contains(&(EGL_STENCIL_SIZE, 8)));
}

#[test]
fn the_cheapest_config_is_tried_first() {
    // WebRender draws the default framebuffer without depth: colour only is the first wish, and the wishes get larger.
    assert_eq!(CONFIG_WISHES[0], ConfigWish { depth: 0, stencil: 0 });
    let sizes: Vec<EglInt> = CONFIG_WISHES.iter().map(|w| w.depth + w.stencil).collect();
    assert!(sizes.windows(2).all(|w| w[0] < w[1]), "{sizes:?}");
}

#[test]
fn the_context_asks_for_the_major_version_and_ends_the_list() {
    assert_eq!(context_attribs(3), vec![0x3098, 3, 0x3038]);
}

/// WebRender's rectangles have their origin at the top left, EGL's damage at the bottom left: a 100 px strip at the
/// top of a 1000 px high surface is the rectangle at y = 900.
#[test]
fn damage_is_flipped_to_the_bottom_left_origin() {
    assert_eq!(damage_rects_for_swap(&[(0, 0, 1080, 100)], 1000), vec![0, 900, 1080, 100]);
    assert_eq!(damage_rects_for_swap(&[(10, 900, 50, 100)], 1000), vec![10, 0, 50, 100]);
    assert_eq!(
        damage_rects_for_swap(&[(0, 0, 4, 4), (8, 8, 4, 4)], 100),
        vec![0, 96, 4, 4, 8, 88, 4, 4]
    );
    assert!(damage_rects_for_swap(&[], 100).is_empty());
}

/// A rectangle that lies below the surface does not wrap around: the arithmetic saturates.
#[test]
fn damage_below_the_surface_stays_on_the_edge() {
    assert_eq!(damage_rects_for_swap(&[(0, 990, 10, 50)], 1000), vec![0, 0, 10, 50]);
    assert_eq!(damage_rects_for_swap(&[(0, u32::MAX, 10, 10)], 1000), vec![0, 0, 10, 10]);
    assert_eq!(damage_rects_for_swap(&[(u32::MAX, 0, 1, 1)], 10), vec![i32::MAX, 9, 1, 1]);
}

#[test]
fn a_failed_swap_is_sorted_by_what_the_shell_must_do() {
    assert_eq!(classify_swap_error(EGL_CONTEXT_LOST), SwapError::ContextLost);
    assert_eq!(classify_swap_error(EGL_BAD_CONTEXT), SwapError::ContextLost);
    assert_eq!(classify_swap_error(EGL_BAD_SURFACE), SwapError::SurfaceLost);
    assert_eq!(classify_swap_error(EGL_BAD_NATIVE_WINDOW), SwapError::SurfaceLost);
    assert_eq!(classify_swap_error(EGL_BAD_CURRENT_SURFACE), SwapError::SurfaceLost);
    assert_eq!(classify_swap_error(EGL_BAD_ALLOC), SwapError::Failed(EGL_BAD_ALLOC));
    assert_eq!(classify_swap_error(0x7777), SwapError::Failed(0x7777));
}

#[test]
fn the_buffer_age_is_zero_unless_the_driver_gave_a_positive_one() {
    assert_eq!(usable_buffer_age(true, Some(2)), 2);
    assert_eq!(usable_buffer_age(true, Some(1)), 1);
    assert_eq!(usable_buffer_age(true, Some(0)), 0, "0 is 'undefined' in the extension");
    assert_eq!(usable_buffer_age(true, Some(-1)), 0);
    assert_eq!(usable_buffer_age(true, None), 0, "the query failed");
    assert_eq!(usable_buffer_age(false, Some(3)), 0, "no extension, no age");
}
