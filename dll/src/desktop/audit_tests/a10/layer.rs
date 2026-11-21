//! A10-06, the layer-backed present of the macOS CPU view (`AZ_CPU_PRESENT=layer`).
//!
//! What a test on one host can hold of code that needs a window: the shapes in the source.
//! The arithmetic (where the layers go, what a frame changes) is tested where it lives
//! (`shell2/macos/layer_present.rs`, on macOS), the renderer's side in `azul-layout`
//! (`cpurender/present_layer.rs`, `audit_tests/a10/layer.rs`).

use super::{all, body, near, MACOS_RS};

const LAYER_RS: &str = include_str!("../../shell2/macos/layer_present.rs");

/// The switch. The renderer promotes a frame only when the shell says so, the shell says so
/// only for the word `layer`, and the default has no such word: with the switch off the
/// renderer is not asked, no layer is made, and `drawRect:` is the whole present.
/// Mutation: `layer: true` in the default, or `promote_for_present` set from anything else.
#[test]
fn the_layer_present_is_off_unless_its_word_is_given() {
    let default = body(MACOS_RS, "impl Default for CpuPresentOptions");
    assert!(default.contains("layer: false"), "the default is the present from the framebuffer alone");
    let parse = body(MACOS_RS, "pub(crate) fn parse_cpu_present_options");
    assert!(parse.contains("\"layer\" => options.layer = true"));
    let sets = all(MACOS_RS, "promote_for_present = ");
    assert_eq!(sets.len(), 1, "set in one place: before each frame of the CPU arm");
    assert!(
        MACOS_RS[sets[0]..].starts_with("promote_for_present = cpu_present_options().layer"),
        "the renderer is asked only under the switch"
    );
    // Every use of the layers in the shell is under the switch.
    for call in ["cpu_view.present_layer(", "cpu_view.hide_present_layer("] {
        for at in all(MACOS_RS, call) {
            assert!(
                near(MACOS_RS, at, "if cpu_present_options().layer {", 2000),
                "`{call}` runs only with AZ_CPU_PRESENT=layer"
            );
        }
    }
}

/// One transaction a frame, with implicit animations off. A layer property set outside one
/// is animated (a quarter of a second of a map sliding after the pointer), and a move
/// committed apart from the frame's own redraw tears against it. So every setter is private
/// to `LayerHost` and reached only through `apply`, which opens the transaction before the
/// first and commits it after the last.
/// Mutation: a setter made `pub(crate)` and called from the view; `commit` before the ops.
#[test]
fn every_layer_change_is_made_in_one_transaction_without_actions() {
    let apply = body(LAYER_RS, "pub(crate) unsafe fn apply");
    let begin = apply.find("CATransaction), begin]").expect("the transaction is opened");
    let quiet = apply.find("CATransaction), setDisableActions: true]").expect("actions are off");
    let commit = apply.find("CATransaction), commit]").expect("the transaction is committed");
    let first_op = apply.find("for op in ops").expect("the ops");
    assert!(begin < quiet && quiet < first_op && first_op < commit);
    assert_eq!(all(LAYER_RS, "CATransaction), begin]").len(), 1, "no other transaction");
    for setter in ["unsafe fn attach", "unsafe fn set_contents", "unsafe fn set_frame", "unsafe fn set_hidden"] {
        let at = LAYER_RS.find(setter).unwrap_or_else(|| panic!("`{setter}` not found - was it renamed?"));
        let line_start = LAYER_RS[..at].rfind('\n').map_or(0, |i| i + 1);
        assert_eq!(LAYER_RS[line_start..at].trim(), "", "`{setter}` is private to the host");
    }
    // The only caller of `apply` is the view's `present_layer`.
    assert_eq!(all(MACOS_RS, "host.apply(").len(), 1);
}

/// Who owns the pixels handed to Core Animation: the image's data provider, over a COPY made
/// while the renderer's surface is borrowed. The renderer repaints and replaces its surface
/// between frames and Core Animation reads an image when the transaction commits, after the
/// call has returned: an image over the renderer's memory would be read while (or after) it
/// changes. The image is tagged sRGB when it is made, as the framebuffer's is.
/// Mutation: `surface.data().as_ptr()` handed to the provider; a provider without the
/// release callback (the copy leaks, 46.6 MB a time).
#[test]
fn the_layers_image_owns_a_copy_of_the_surface() {
    let at = MACOS_RS.find("layer_present_failed = !cpu_view.present_layer(").expect("the present");
    assert!(
        near(MACOS_RS, at, "with_present_layer_surface(", 600),
        "the surface is borrowed for the copy only"
    );
    assert!(near(MACOS_RS, at, "surface.data().to_vec()", 600), "a copy, not the renderer's memory");
    let image = body(LAYER_RS, "unsafe fn surface_image");
    assert!(image.contains("Box::into_raw(Box::new(pixels))"), "the provider owns the copy");
    assert!(image.contains("Some(release_layer_pixels)"), "and frees it");
    assert!(image.contains("kCGColorSpaceSRGB"), "tagged sRGB at creation");
    assert!(image.contains("RGBA8_PREMULTIPLIED"));
    let release = body(LAYER_RS, "extern \"C\" fn release_layer_pixels");
    assert!(release.contains("Box::from_raw(info.cast::<Vec<u8>>())"));
    assert!(!LAYER_RS.contains("framebuffer.as_ptr()"), "nothing of the view's framebuffer is handed out here");
}

/// The fallbacks. A view asks for a layer only over a frame of its own size (a resize or a
/// change of scale in flight is presented through the frame alone), and a window that gives
/// its regenerable memory back hides the layers, lets go of their pixels and owes a frame.
#[test]
fn a_resize_and_a_memory_release_fall_back_to_the_frame_alone() {
    let usable = body(MACOS_RS, "pub(crate) fn layer_present_usable");
    assert!(usable.contains("layer_present_fits((w, h), (ivars.width.get(), ivars.height.get()))"));
    assert!(usable.contains("failed"), "a view whose layers failed never asks again");
    let release = body(MACOS_RS, "pub(crate) fn release_regenerable_memory");
    let hidden = release.find("cpu_view.hide_present_layer();").expect("the layers are hidden");
    let dropped = release.find("self.cpu_backend.release_caches();").expect("the surfaces are released");
    assert!(dropped < hidden);
    assert!(release[hidden..].contains("self.redraw_requested = true;"), "a hidden window owes the frame");
    assert!(release[hidden..].contains("self.request_redraw();"), "a shown one gets it now");
}
