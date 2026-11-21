//! The layer-backed present of the CPU view (A10-06; `AZ_CPU_PRESENT=layer`, default off).
//!
//! With the switch on, the CPU backend leaves the picture of ONE reference frame (a map
//! view's) out of the window's frame and reports it as a layer
//! (`azul_layout::cpurender::present_layer::PresentLayer`): a surface, where it lands, what
//! clips it. The view shows it with two Core Animation layers of its own, as sublayers of
//! the view's layer, which goes on showing the window's frame as before (`drawRect:`):
//!
//! ```text
//! the view's layer            the window's frame, drawn by drawRect: (unchanged)
//!   +- clip layer             masksToBounds; its frame is the layer's clip
//!        +- picture layer     contents: an sRGB CGImage over a COPY of the surface;
//!                             its frame is where the surface lands (contents stretched)
//! ```
//!
//! A pan or a zoom step of the picture is then ONE property: the picture layer's frame. The
//! window server moves or scales the pixels; the view presents nothing.
//!
//! # Spaces
//!
//! The renderer's rects are device px of the frame, origin top left, y down. A layer's frame
//! is in its superlayer's coordinates, which for the sublayers of a view that is not flipped
//! (the CPU view is not) are points with the origin at the BOTTOM left, y up; the same holds
//! inside a plain `CALayer`. So every rect is flipped once, against the height of what it
//! is in (the frame for the clip, the clip for the picture), and divided by the backing
//! scale: [`layer_geometry`]. No layer here is geometry-flipped and none has a transform.
//!
//! # Who owns the pixels handed to Core Animation
//!
//! The picture layer's contents are a `CGImage` whose data provider owns a `Vec<u8>`: a copy
//! of the surface made while the renderer's surface is borrowed
//! (`CpuBackend::with_present_layer_surface`). Core Animation reads an image's bytes when
//! the transaction commits and may read them again later; they are the provider's own until
//! Core Graphics releases the provider, which frees them ([`release_layer_pixels`]). Nothing
//! points into the renderer's surface (which the renderer repaints and replaces between
//! frames), nor into the view's framebuffer (which a resize moves), after the call returns.
//! The image is tagged sRGB when it is made, as the window and the framebuffer's image are.
//!
//! # What is tested and what is not
//!
//! The geometry ([`layer_geometry`]) and what one frame changes ([`layer_transaction`]) are
//! pure and tested (`audit_tests/a10/layer.rs`). [`LayerHost`]'s methods call AppKit and
//! Core Animation and cannot run without a window: each is a few lines, one call per line,
//! every selector checked against `objc2-quartz-core` 0.3.2's bindings.

use objc2::{msg_send, rc::Retained, runtime::AnyObject};
use objc2_app_kit::NSView;
use objc2_foundation::{NSPoint, NSRect, NSSize};

/// A rect in points, origin at the bottom left of what it is in, y up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PtRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The layer a frame reports, as the shell needs it (`PresentLayer` of the renderer, without
/// its types): device px of the frame, origin top left, y down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayerInput {
    /// Which picture the surface holds.
    pub surface_id: u64,
    /// The stamp of the surface's pixels.
    pub generation: u64,
    /// The surface's size, px.
    pub surface_size: (u32, u32),
    /// The surface's pixel `(x, y)` lands at `(scale.0 * x + offset.0, scale.1 * y + offset.1)`.
    pub scale: (f64, f64),
    pub offset: (f64, f64),
    /// What the layer shows in, `(x0, y0, x1, y1)`, half-open.
    pub clip: (i32, i32, i32, i32),
}

/// Where the two layers go.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayerGeometry {
    /// The clip layer's frame, in the view.
    pub clip: PtRect,
    /// The picture layer's frame, in the clip layer.
    pub picture: PtRect,
}

/// The frames of the clip layer and of the picture layer for `layer`, in a frame `frame_h`
/// device px high at `scale` backing px per point.
///
/// The clip `(x0, y0) .. (x1, y1)` (y down) in the view (y up, points):
/// `(x0 / s, (H - y1) / s, (x1 - x0) / s, (y1 - y0) / s)`.
/// The surface lands on the device rect with the left edge `tx`, the top edge `ty`, the
/// width `sw kx` and the height `sh ky`. In the clip layer (origin at the clip's bottom
/// left, the device point `(x0, y1)`, y up): `((tx - x0) / s, (y1 - (ty + sh ky)) / s,
/// sw kx / s, sh ky / s)`.
///
/// The handover's example, scale 2, frame 2940 x 1912 px: clip (100, 76) .. (2940, 1876),
/// a 3964 x 2936 surface scaled by 1.5 and landing at (-300, -200). Clip layer:
/// (50, (1912 - 1876) / 2, 1420, 900) = (50, 18, 1420, 900). Picture: 3964 x 1.5 = 5946 px
/// wide and 2936 x 1.5 = 4404 px high, so its bottom edge is at -200 + 4404 = 4204 px:
/// ((-300 - 100) / 2, (1876 - 4204) / 2, 5946 / 2, 4404 / 2) = (-200, -1164, 2973, 2202).
/// (Its top edge is 2202 - 1164 = 1038 pt above the clip's bottom, 138 pt above the clip's
/// top: (-200 - 76) / 2 = -138 in the frame's y-down terms.)
/// At scale 1 the same window is 1470 x 956 px, the clip (50, 38) .. (1470, 938), the
/// surface 1982 x 1468 landing at (-150, -100): the same two rects in points.
pub(crate) fn layer_geometry(layer: &LayerInput, frame_h: u32, scale: f64) -> LayerGeometry {
    let s = if scale > 0.0 { scale } else { 1.0 };
    let (x0, y0, x1, y1) = (
        f64::from(layer.clip.0),
        f64::from(layer.clip.1),
        f64::from(layer.clip.2),
        f64::from(layer.clip.3),
    );
    let h = f64::from(frame_h);
    let width = f64::from(layer.surface_size.0) * layer.scale.0;
    let height = f64::from(layer.surface_size.1) * layer.scale.1;
    LayerGeometry {
        clip: PtRect {
            x: x0 / s,
            y: (h - y1) / s,
            w: (x1 - x0) / s,
            h: (y1 - y0) / s,
        },
        picture: PtRect {
            x: (layer.offset.0 - x0) / s,
            y: (y1 - (layer.offset.1 + height)) / s,
            w: width / s,
            h: height / s,
        },
    }
}

/// What the view's two layers show now.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Shown {
    /// The layers exist and are in the view's layer.
    pub attached: bool,
    /// They are not hidden.
    pub visible: bool,
    /// The picture layer has contents, of this surface and stamp.
    pub contents: Option<(u64, u64)>,
    pub clip: Option<PtRect>,
    pub picture: Option<PtRect>,
}

/// One change of a frame's transaction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum LayerOp {
    /// Make the two layers and put them into the view's layer.
    Attach,
    /// The clip layer's frame.
    SetClip(PtRect),
    /// The picture layer's frame: a pan or a zoom step is this alone.
    SetPicture(PtRect),
    /// New contents for the picture layer: a copy of the whole surface, this many bytes.
    SetContents { bytes: usize },
    /// Unhide the clip layer.
    Show,
    /// Hide the clip layer and let go of the picture's contents (the copy of the surface).
    Hide,
}

/// What one frame changes of the layers, and what they show afterwards: `layer` is the
/// frame's layer (`None`: nothing is promoted), `frame_h` the frame's height in device px,
/// `scale` the backing scale.
///
/// - No layer: the layers are hidden if they are shown (the window's frame holds the whole
///   picture then: the renderer repainted the clip in the same frame).
/// - A layer: the layers are made once; each of the two frames is set when it is another;
///   the contents are set when the picture has none of this surface and stamp; the layers
///   are shown if they are hidden.
///
/// So a pan or a zoom step of a picture whose pixels did not change is exactly
/// `[SetPicture]`: no byte is handed to AppKit.
pub(crate) fn layer_transaction(
    shown: &Shown,
    layer: Option<&LayerInput>,
    frame_h: u32,
    scale: f64,
) -> (Vec<LayerOp>, Shown) {
    let mut ops = Vec::new();
    let mut next = *shown;
    let Some(layer) = layer else {
        if shown.visible {
            ops.push(LayerOp::Hide);
            next.visible = false;
            next.contents = None;
        }
        return (ops, next);
    };
    if !shown.attached {
        ops.push(LayerOp::Attach);
        next.attached = true;
        // New layers: no frame, no contents, hidden until shown.
        next.visible = false;
        next.contents = None;
        next.clip = None;
        next.picture = None;
    }
    let geometry = layer_geometry(layer, frame_h, scale);
    if next.clip != Some(geometry.clip) {
        ops.push(LayerOp::SetClip(geometry.clip));
        next.clip = Some(geometry.clip);
    }
    if next.picture != Some(geometry.picture) {
        ops.push(LayerOp::SetPicture(geometry.picture));
        next.picture = Some(geometry.picture);
    }
    let contents = (layer.surface_id, layer.generation);
    if next.contents != Some(contents) {
        let (w, h) = layer.surface_size;
        ops.push(LayerOp::SetContents {
            bytes: w as usize * h as usize * 4,
        });
        next.contents = Some(contents);
    }
    if !next.visible {
        ops.push(LayerOp::Show);
        next.visible = true;
    }
    (ops, next)
}

/// How many bytes a transaction hands to AppKit (the copies of the surface it makes).
pub(crate) fn contents_bytes(ops: &[LayerOp]) -> usize {
    ops.iter()
        .map(|op| match op {
            LayerOp::SetContents { bytes } => *bytes,
            _ => 0,
        })
        .sum()
}

/// Whether the layer present may be used for a frame of `frame` px by a view whose
/// framebuffer is `view` px (`(0, 0)` before its first `drawRect:`): only while the two
/// agree. A frame of another size than the view is shown rescaled (a resize or a change of
/// scale in flight), and a layer placed by the frame's pixels would not lie on it.
pub(crate) fn layer_present_fits(frame: (usize, usize), view: (usize, usize)) -> bool {
    frame.0 > 0 && frame.1 > 0 && (view == (0, 0) || view == frame)
}

/// The two layers of a view and what they show. Lives in the view's ivars; main thread.
#[derive(Default)]
pub(crate) struct LayerHost {
    pub(crate) shown: Shown,
    clip: Option<Retained<AnyObject>>,
    picture: Option<Retained<AnyObject>>,
    /// The layers could not be made or attached: the layer present is off for this view.
    pub(crate) failed: bool,
}

fn ns_rect(r: PtRect) -> NSRect {
    NSRect::new(NSPoint::new(r.x, r.y), NSSize::new(r.w, r.h))
}

/// The release callback of a layer image's data provider: frees the copy of the surface the
/// provider was made over. `info` is the `Box<Vec<u8>>` leaked in [`surface_image`]; Core
/// Graphics calls this once, on any thread, when the last reference to the provider goes.
extern "C" fn release_layer_pixels(
    info: *mut std::ffi::c_void,
    _data: *const std::ffi::c_void,
    _size: usize,
) {
    if !info.is_null() {
        // SAFETY: `info` came from `Box::into_raw` in `surface_image`, and this callback is
        // the only place that takes it back.
        drop(unsafe { Box::from_raw(info.cast::<Vec<u8>>()) });
        #[cfg(test)]
        tests::PIXELS_FREED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }
}

/// An sRGB `CGImage` (+1, the caller releases it) over `pixels`, which it owns from now on:
/// `width` x `height` px of RGBA8, premultiplied, top row first, tight rows. Null when the
/// buffer is not that size or Core Graphics makes no image (the pixels are freed then, too).
///
/// # Safety
/// Calls Core Graphics; any thread.
unsafe fn surface_image(pixels: Vec<u8>, width: usize, height: usize) -> *mut std::ffi::c_void {
    use std::ffi::c_void;

    use super::cg_present;

    let stride = width * 4;
    if width == 0 || height == 0 || pixels.len() != stride * height {
        return std::ptr::null_mut();
    }
    let space = cg_present::CGColorSpaceCreateWithName(cg_present::kCGColorSpaceSRGB);
    if space.is_null() {
        return std::ptr::null_mut();
    }
    let len = pixels.len();
    let owned = Box::into_raw(Box::new(pixels));
    // The Vec's buffer does not move when the Vec itself is boxed and leaked.
    let data = (*owned).as_ptr();
    let provider = cg_present::CGDataProviderCreateWithData(
        owned.cast::<c_void>(),
        data.cast::<c_void>(),
        len,
        Some(release_layer_pixels),
    );
    if provider.is_null() {
        drop(Box::from_raw(owned));
        cg_present::CGColorSpaceRelease(space);
        return std::ptr::null_mut();
    }
    let image = cg_present::CGImageCreate(
        width,
        height,
        8,
        32,
        stride,
        space,
        cg_present::RGBA8_PREMULTIPLIED,
        provider,
        std::ptr::null(),
        true,
        cg_present::INTENT_DEFAULT,
    );
    // The image holds the provider and the colour space; the provider frees the pixels when
    // it goes (now, if no image was made).
    cg_present::CGDataProviderRelease(provider);
    cg_present::CGColorSpaceRelease(space);
    image
}

impl LayerHost {
    /// Make the clip layer and the picture layer and put them into `view`'s layer, under the
    /// layers of its subviews and over its own drawing. `false` when the view has no layer
    /// or a layer could not be made.
    ///
    /// # Safety
    /// Main thread.
    unsafe fn attach(&mut self, view: &NSView) -> bool {
        let host = match view.layer() {
            Some(layer) => layer,
            None => {
                view.setWantsLayer(true);
                match view.layer() {
                    Some(layer) => layer,
                    None => return false,
                }
            }
        };
        let clip: Option<Retained<AnyObject>> = msg_send![objc2::class!(CALayer), new];
        let picture: Option<Retained<AnyObject>> = msg_send![objc2::class!(CALayer), new];
        let (Some(clip), Some(picture)) = (clip, picture) else {
            return false;
        };
        let _: () = msg_send![&*clip, setMasksToBounds: true];
        let _: () = msg_send![&*clip, setHidden: true];
        let _: () = msg_send![&*clip, addSublayer: &*picture];
        let _: () = msg_send![&*host, insertSublayer: &*clip, atIndex: 0u32];
        self.clip = Some(clip);
        self.picture = Some(picture);
        true
    }

    /// Take the layers out of the view's layer and forget them.
    ///
    /// # Safety
    /// Main thread.
    pub(crate) unsafe fn detach(&mut self) {
        if let Some(clip) = self.clip.take() {
            let _: () = msg_send![&*clip, removeFromSuperlayer];
        }
        self.picture = None;
        self.shown = Shown::default();
    }

    /// The picture layer's contents: an image over `pixels` (see the module: who owns them).
    ///
    /// # Safety
    /// Main thread.
    unsafe fn set_contents(&self, pixels: Vec<u8>, width: usize, height: usize) -> bool {
        let Some(picture) = self.picture.as_ref() else {
            return false;
        };
        let _span = azul_layout::probe::Probe::span("layer_present_contents");
        let image = surface_image(pixels, width, height);
        if image.is_null() {
            return false;
        }
        let _: () = msg_send![&**picture, setContents: image.cast_const().cast::<AnyObject>()];
        // The layer holds the image now.
        super::cg_present::CGImageRelease(image);
        true
    }

    /// One layer's frame.
    ///
    /// # Safety
    /// Main thread.
    unsafe fn set_frame(layer: Option<&Retained<AnyObject>>, frame: PtRect) {
        if let Some(layer) = layer {
            let _: () = msg_send![&**layer, setFrame: ns_rect(frame)];
        }
    }

    /// Hide or unhide the clip layer; hiding lets go of the picture's contents.
    ///
    /// # Safety
    /// Main thread.
    unsafe fn set_hidden(&self, hidden: bool) {
        if let Some(clip) = self.clip.as_ref() {
            let _: () = msg_send![&**clip, setHidden: hidden];
        }
        if hidden {
            if let Some(picture) = self.picture.as_ref() {
                let _: () = msg_send![&**picture, setContents: std::ptr::null::<AnyObject>()];
            }
        }
    }

    /// Apply one frame's changes in ONE transaction with implicit animations off (a layer
    /// property set outside one would be animated over a quarter of a second), in the run
    /// loop turn in which the view's dirty rects are drawn: Core Animation commits both
    /// together. `surface` yields the pixels for a `SetContents` (a copy, and its size).
    ///
    /// Returns what the layers show afterwards; on a failure (no layer, no image) the layers
    /// are taken out, [`Self::failed`] is set, and the caller presents without them.
    ///
    /// # Safety
    /// Main thread.
    pub(crate) unsafe fn apply(
        &mut self,
        view: &NSView,
        ops: &[LayerOp],
        next: Shown,
        surface: impl FnOnce() -> Option<(Vec<u8>, usize, usize)>,
    ) -> bool {
        if ops.is_empty() {
            return true;
        }
        let _span = azul_layout::probe::Probe::span("layer_present_transaction");
        let mut surface = Some(surface);
        let mut ok = true;
        let _: () = msg_send![objc2::class!(CATransaction), begin];
        let _: () = msg_send![objc2::class!(CATransaction), setDisableActions: true];
        for op in ops {
            match *op {
                LayerOp::Attach => ok = self.attach(view),
                LayerOp::SetClip(frame) => Self::set_frame(self.clip.as_ref(), frame),
                LayerOp::SetPicture(frame) => Self::set_frame(self.picture.as_ref(), frame),
                LayerOp::SetContents { .. } => {
                    ok = match surface.take().and_then(|read| read()) {
                        Some((pixels, width, height)) => self.set_contents(pixels, width, height),
                        None => false,
                    };
                }
                LayerOp::Show => self.set_hidden(false),
                LayerOp::Hide => self.set_hidden(true),
            }
            if !ok {
                break;
            }
        }
        if ok {
            self.shown = next;
        } else {
            self.detach();
            self.failed = true;
        }
        let _: () = msg_send![objc2::class!(CATransaction), commit];
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many copies of a surface their image's provider has freed.
    pub(super) static PIXELS_FREED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// The layer of the user's window (2940 x 1912 px at a scale of 2): a 3964 x 2936 surface
    /// (the view and 256 pt of overscan on each side) under the clip (100, 76) .. (2940, 1876).
    fn layer(scale: f64, offset: (f64, f64), generation: u64) -> LayerInput {
        LayerInput {
            surface_id: 7,
            generation,
            surface_size: (3964, 2936),
            scale: (scale, scale),
            offset,
            clip: (100, 76, 2940, 1876),
        }
    }

    fn pt(x: f64, y: f64, w: f64, h: f64) -> PtRect {
        PtRect { x, y, w, h }
    }

    /// A10-06, where the two layers go. The example of [`layer_geometry`]'s doc, at a scale
    /// of 2 and of 1: the same rects in points. And the picture at rest: scale 1, the
    /// surface's (512, 512) at the clip's corner, so it lands at (100 - 512, 76 - 512) =
    /// (-412, -436) px: ((-412 - 100) / 2, (1876 - (-436 + 2936)) / 2, 3964 / 2, 2936 / 2) =
    /// (-256, -312, 1982, 1468): 256 pt of overscan left of the clip, and 1468 - 312 - 900 =
    /// 256 pt above it.
    /// Mutations: `h - y1` -> `y0` in the clip's y puts the clip 38 pt from the BOTTOM
    /// (the map shifted down by 20 pt against the UI); `y1 - (ty + height)` -> `ty - y0`
    /// puts the picture's TOP where its bottom belongs (the map upside down in place: its
    /// overscan below instead of above); a missing `/ s` doubles everything at Retina.
    #[test]
    fn the_layers_are_placed_flipped_and_in_points() {
        let zoomed = layer_geometry(&layer(1.5, (-300.0, -200.0), 1), 1912, 2.0);
        assert_eq!(zoomed.clip, pt(50.0, 18.0, 1420.0, 900.0));
        assert_eq!(zoomed.picture, pt(-200.0, -1164.0, 2973.0, 2202.0));
        let at_one = LayerInput {
            surface_id: 7,
            generation: 1,
            surface_size: (1982, 1468),
            scale: (1.5, 1.5),
            offset: (-150.0, -100.0),
            clip: (50, 38, 1470, 938),
        };
        assert_eq!(layer_geometry(&at_one, 956, 1.0), zoomed, "the same points at a scale of 1");
        let rest = layer_geometry(&layer(1.0, (-412.0, -436.0), 1), 1912, 2.0);
        assert_eq!(rest.picture, pt(-256.0, -312.0, 1982.0, 1468.0));
        // The picture's top edge, from the clip's top, in y-down terms: (ty - y0) / s.
        assert_eq!(rest.clip.h - (rest.picture.y + rest.picture.h), (-436.0 - 76.0) / 2.0);
    }

    /// A10-06, the count: a pan and a zoom step of a picture whose pixels did not change are
    /// ONE layer property, and no byte is handed to AppKit. (Without the layer the view
    /// presents the clip: 2840 x 1800 px = 20,448,000 bytes through AppKit per pan frame.)
    /// The first frame makes the layers and hands over the whole surface once, 3964 x 2936 x
    /// 4 = 46,553,216 bytes; so does a frame that painted a strip of the surface.
    /// Mutations: contents set on every frame hands 46.6 MB to AppKit per pan frame; a
    /// `SetClip` on every frame is harmless and shows here; contents keyed by the surface's
    /// id alone miss the strip (the map keeps its old edge after a pan).
    #[test]
    fn a_pan_or_a_zoom_step_is_one_layer_property_and_no_byte() {
        let rest = layer(1.0, (-412.0, -436.0), 3);
        let (first, shown) = layer_transaction(&Shown::default(), Some(&rest), 1912, 2.0);
        assert_eq!(
            first,
            vec![
                LayerOp::Attach,
                LayerOp::SetClip(pt(50.0, 18.0, 1420.0, 900.0)),
                LayerOp::SetPicture(pt(-256.0, -312.0, 1982.0, 1468.0)),
                LayerOp::SetContents { bytes: 46_553_216 },
                LayerOp::Show,
            ]
        );
        assert_eq!(contents_bytes(&first), 3964 * 2936 * 4);
        assert!(shown.attached && shown.visible && shown.contents == Some((7, 3)));

        // The same frame again: nothing.
        let (again, same) = layer_transaction(&shown, Some(&rest), 1912, 2.0);
        assert!(again.is_empty() && same == shown);

        // A pan by 12 pt to the left: 24 px.
        let (pan, shown) = layer_transaction(&shown, Some(&layer(1.0, (-436.0, -436.0), 3)), 1912, 2.0);
        assert_eq!(pan, vec![LayerOp::SetPicture(pt(-268.0, -312.0, 1982.0, 1468.0))]);
        assert_eq!(contents_bytes(&pan), 0);

        // A zoom step.
        let (zoom, shown) = layer_transaction(&shown, Some(&layer(1.5, (-300.0, -200.0), 3)), 1912, 2.0);
        assert_eq!(zoom, vec![LayerOp::SetPicture(pt(-200.0, -1164.0, 2973.0, 2202.0))]);
        assert_eq!(contents_bytes(&zoom), 0);

        // The renderer painted a strip of the surface (another stamp), the layer stays put.
        let (strip, shown) = layer_transaction(&shown, Some(&layer(1.5, (-300.0, -200.0), 4)), 1912, 2.0);
        assert_eq!(strip, vec![LayerOp::SetContents { bytes: 46_553_216 }]);
        assert_eq!(shown.contents, Some((7, 4)));

        // The view's box changed (a panel opened beside the map): both frames, no contents.
        let narrower = LayerInput { clip: (100, 76, 2540, 1876), ..layer(1.5, (-300.0, -200.0), 4) };
        let (resized, _) = layer_transaction(&shown, Some(&narrower), 1912, 2.0);
        assert_eq!(resized, vec![LayerOp::SetClip(pt(50.0, 18.0, 1220.0, 900.0))]);
    }

    /// A10-06, the fallback: a frame that reports no layer hides the layers and lets go of
    /// the copy of the surface; the next layer is shown with its pixels handed over again;
    /// hidden layers stay as they are.
    /// Mutation: a `Hide` that keeps `contents` shows the pixels of before the resize when
    /// the layer comes back with another surface of the same id and stamp... which cannot
    /// be; but it would keep 46.6 MB alive under a hidden layer.
    #[test]
    fn a_frame_without_a_layer_hides_the_layers_and_lets_go_of_the_pixels() {
        let rest = layer(1.0, (-412.0, -436.0), 3);
        let (_, shown) = layer_transaction(&Shown::default(), Some(&rest), 1912, 2.0);
        let (hide, hidden) = layer_transaction(&shown, None, 1912, 2.0);
        assert_eq!(hide, vec![LayerOp::Hide]);
        assert!(hidden.attached && !hidden.visible && hidden.contents.is_none());
        let (nothing, still) = layer_transaction(&hidden, None, 1912, 2.0);
        assert!(nothing.is_empty() && still == hidden);
        // Never shown: nothing to hide, and no layer is made for it.
        assert!(layer_transaction(&Shown::default(), None, 0, 0.0).0.is_empty());
        // The same layer again: its frames are still right, its pixels are handed over.
        let (back, _) = layer_transaction(&hidden, Some(&rest), 1912, 2.0);
        assert_eq!(back, vec![LayerOp::SetContents { bytes: 46_553_216 }, LayerOp::Show]);
    }

    /// A10-06: a layer is used only over a frame of the view's own size. A frame of another
    /// size is shown rescaled (a resize or a change of scale in flight), and the layer,
    /// placed by the frame's pixels, would not lie on it.
    #[test]
    fn a_layer_is_only_used_over_a_frame_of_the_views_size() {
        assert!(layer_present_fits((2940, 1912), (2940, 1912)));
        assert!(layer_present_fits((2940, 1912), (0, 0)), "before the view's first drawRect:");
        assert!(!layer_present_fits((2940, 1912), (1470, 956)), "the view is still at a scale of 1");
        assert!(!layer_present_fits((2940, 1912), (2940, 1910)));
        assert!(!layer_present_fits((0, 0), (0, 0)));
    }

    /// A10-06, who owns the pixels handed to Core Animation: the image's data provider, which
    /// frees them when the image goes. Core Graphics alone (no window, no layer): an image is
    /// made over a copy and released, and the copy is freed exactly then; a buffer of the
    /// wrong size makes no image and is freed at once.
    /// Mutation: a provider made over the caller's buffer without the release callback
    /// leaves the image pointing at freed memory (and frees nothing here).
    #[test]
    fn the_pixels_of_a_layer_image_are_freed_with_the_image() {
        use std::sync::atomic::Ordering;
        let before = PIXELS_FREED.load(Ordering::SeqCst);
        // 4 x 3 px of opaque red.
        let pixels: Vec<u8> = [255u8, 0, 0, 255].repeat(12);
        let image = unsafe { surface_image(pixels, 4, 3) };
        assert!(!image.is_null(), "Core Graphics makes an image over 4 x 3 px of RGBA8");
        assert_eq!(PIXELS_FREED.load(Ordering::SeqCst), before, "the image holds its pixels");
        unsafe { super::super::cg_present::CGImageRelease(image) };
        assert_eq!(PIXELS_FREED.load(Ordering::SeqCst), before + 1, "freed with the image");
        // The wrong size: no image, and the buffer is dropped by the function itself.
        assert!(unsafe { surface_image(vec![0; 10], 4, 3) }.is_null());
    }
}
