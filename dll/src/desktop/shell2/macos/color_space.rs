//! The colour space of the pixels azul hands to AppKit.
//!
//! Both renderers write CSS colours as they are, so every byte in a frame is
//! an sRGB value. A Mac converts what a window shows into the colour space
//! of the display it is on, and for that it has to know what space the
//! pixels are in. The same bytes read as the display's own space look too
//! saturated on a wide-gamut (Display P3) screen, and read as the generic
//! RGB space (gamma 1.8) they look washed out.
//!
//! So the window says its pixels are sRGB, the GL surface's layer says so
//! too where it takes a colour space, and the CPU view's bitmap is tagged
//! sRGB. AppKit converts from sRGB to the display, as a browser does, and a
//! colour looks the same on both renderers and in Chrome.

use objc2::{msg_send, rc::Retained, runtime::NSObjectProtocol, sel, AnyThread};
use objc2_app_kit::{NSBitmapFormat, NSBitmapImageRep, NSColorSpace, NSView, NSWindow};
use objc2_foundation::ns_string;

use super::super::common::debug_server::LogCategory;
use crate::log_warn;

/// Tells AppKit that everything shown in `window` is sRGB, and reads the
/// tags back as one line for the log.
///
/// The window's colour space covers the CPU view's drawing and the GL
/// surface, whose pixels carry no tag of their own and are taken to be in
/// the window's space. A GL surface drawn through a `CAOpenGLLayer` is not
/// converted at all while that layer has no colour space, so the layer of
/// `render_view` gets one too when it takes one.
pub(super) fn tag_window_srgb(window: &NSWindow, render_view: &NSView) -> String {
    let srgb = NSColorSpace::sRGBColorSpace();
    window.setColorSpace(Some(&srgb));
    let layer = match render_view.layer() {
        None => "none".to_string(),
        Some(layer) => {
            let class = layer.class().name().to_string_lossy().into_owned();
            match srgb.CGColorSpace() {
                Some(cg) if layer.respondsToSelector(sel!(setColorspace:)) => {
                    unsafe {
                        let _: () = msg_send![&*layer, setColorspace: &*cg];
                    }
                    format!("{class}, tagged sRGB")
                }
                _ => format!("{class}, in the window's colour space"),
            }
        }
    };
    let window_space = window
        .colorSpace()
        .and_then(|space| space.localizedName())
        .map_or_else(|| "none".to_string(), |name| name.to_string());
    format!("colour space of the window: {window_space}; render layer: {layer}")
}

/// The bitmap the CPU view copies its framebuffer into before drawing it:
/// `width` x `height` pixels of 8-bit RGBA, premultiplied, top row first,
/// tagged sRGB. Its rows may be padded, so writers read `bytesPerRow` back.
pub(super) fn cpu_frame_bitmap(width: usize, height: usize) -> Option<Retained<NSBitmapImageRep>> {
    // A new bitmap is given its colour space by name, and none of the names
    // is sRGB, so it is made in the generic space and then retagged: the
    // pixels stay as they are, only the tag changes.
    let generic = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            width as isize,
            height as isize,
            8,
            4,
            true,
            false,
            ns_string!("NSCalibratedRGBColorSpace"),
            (width * 4) as isize,
            32,
        )
    }?;
    let Some(srgb) =
        generic.bitmapImageRepByRetaggingWithColorSpace(&NSColorSpace::sRGBColorSpace())
    else {
        log_warn!(
            LogCategory::Rendering,
            "[color_space] the CPU frame could not be tagged sRGB; its colours will look washed out"
        );
        return Some(generic);
    };
    // The retagged bitmap may keep its pixels as an image until its bytes
    // are asked for. Asking now gives it the buffer the view writes into, in
    // the layout AppKit chose. Any layout but the renderer's RGBA would swap
    // or skew channels, and that is worse than washed-out colours.
    if srgb.bitmapData().is_null() || !holds_premultiplied_rgba8(&srgb) {
        log_warn!(
            LogCategory::Rendering,
            "[color_space] the sRGB CPU frame came back as {:?}, {} bits per pixel; \
             drawing it in the generic colour space instead",
            srgb.bitmapFormat(),
            srgb.bitsPerPixel()
        );
        return Some(generic);
    }
    Some(srgb)
}

/// Whether `bitmap` lays its pixels out as the CPU renderer writes them: one
/// plane of 8-bit red, green, blue and premultiplied alpha, in that order.
fn holds_premultiplied_rgba8(bitmap: &NSBitmapImageRep) -> bool {
    !bitmap.isPlanar()
        && bitmap.samplesPerPixel() == 4
        && bitmap.bitsPerPixel() == 32
        && bitmap.bitsPerSample() == 8
        && !bitmap.bitmapFormat().intersects(
            NSBitmapFormat::AlphaFirst
                | NSBitmapFormat::AlphaNonpremultiplied
                | NSBitmapFormat::FloatingPointSamples
                | NSBitmapFormat::ThirtyTwoBitLittleEndian,
        )
}

#[cfg(test)]
mod tests {
    //! What colour a CSS colour arrives as once AppKit has the CPU view's
    //! frame, and that both renderers write the same bytes for it.

    use azul_core::dom::Dom;
    use objc2::{rc::Retained, AnyThread};
    use objc2_app_kit::{
        NSBitmapImageRep, NSColorSpace, NSCompositingOperation, NSGraphicsContext,
    };
    use objc2_foundation::{ns_string, NSPoint, NSRect, NSSize};

    use super::cpu_frame_bitmap;
    use crate::desktop::shell2::macos::headless_gpu::{draw_both, keep};

    /// Colours from the widgets' palettes, and a mid grey, where a wrong
    /// gamma shows the most.
    const SWATCHES: [[u8; 3]; 6] = [
        [0x0d, 0x6e, 0xfd],
        [0x19, 0x87, 0x54],
        [0xdc, 0x35, 0x45],
        [0x6c, 0x75, 0x7d],
        [0x80, 0x80, 0x80],
        [0xff, 0xc1, 0x07],
    ];

    fn hex([r, g, b]: [u8; 3]) -> String {
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// A surface AppKit treats as sRGB, like the backing store of a window
    /// whose colour space is sRGB.
    fn srgb_surface(width: usize) -> Retained<NSBitmapImageRep> {
        let plain = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                std::ptr::null_mut(),
                width as isize,
                1,
                8,
                4,
                true,
                false,
                ns_string!("NSDeviceRGBColorSpace"),
                (width * 4) as isize,
                32,
            )
        }
        .expect("a one-row bitmap");
        plain
            .bitmapImageRepByRetaggingWithColorSpace(&NSColorSpace::sRGBColorSpace())
            .expect("an RGB bitmap takes the sRGB colour space")
    }

    /// The pixel at column `x` of the first row of `bitmap`, an sRGB surface
    /// from [`srgb_surface`]: its own bytes, which are sRGB values because
    /// the surface is tagged sRGB. `colorAtX:y:` cannot read them: it builds
    /// the colour from the bitmap's colour space NAME, which a retagged
    /// bitmap keeps from before (the generic space), so every pixel came
    /// back converted from gamma 1.8 (#0d6efd as #0187fe).
    fn srgb_bytes_at(bitmap: &NSBitmapImageRep, x: usize) -> [u8; 3] {
        let row = bitmap.bitmapData();
        assert!(!row.is_null(), "the surface has a pixel buffer");
        assert!(x < bitmap.pixelsWide() as usize, "a pixel inside the surface");
        let mut pixel = [0_u8; 4];
        // SAFETY: the surface is one row of `pixelsWide` 4-byte RGBA pixels
        // (`srgb_surface`), and `x` is inside it.
        unsafe { std::ptr::copy_nonoverlapping(row.add(x * 4), pixel.as_mut_ptr(), 4) };
        [pixel[0], pixel[1], pixel[2]]
    }

    #[test]
    fn the_cpu_views_frame_bitmap_reads_back_as_srgb() {
        let bitmap = cpu_frame_bitmap(8, 8).expect("an 8x8 bitmap");
        let name = |space: &NSColorSpace| space.localizedName().map(|name| name.to_string());
        assert_eq!(
            name(&*bitmap.colorSpace()),
            name(&*NSColorSpace::sRGBColorSpace()),
            "the CPU frame is in another colour space than sRGB"
        );
    }

    /// The frame is written the way `drawRect:` writes it (straight into
    /// `bitmapData`) and drawn with the same call, into an sRGB surface. A
    /// frame tagged with another colour space is converted on the way and
    /// arrives as other bytes: a grey of 128 as 147 under gamma 1.8.
    #[test]
    fn a_css_colour_drawn_from_the_cpu_frame_into_an_srgb_surface_keeps_its_bytes() {
        let width = SWATCHES.len();
        let frame = cpu_frame_bitmap(width, 1).expect("a one-row bitmap");
        let pixels = frame.bitmapData();
        assert!(!pixels.is_null(), "the CPU frame has a pixel buffer");
        for (x, [r, g, b]) in SWATCHES.iter().enumerate() {
            unsafe {
                std::ptr::copy_nonoverlapping([*r, *g, *b, 255].as_ptr(), pixels.add(x * 4), 4);
            }
        }

        let surface = srgb_surface(width);
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&surface)
            .expect("a graphics context on the surface");
        let rect = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(width as f64, 1.0));
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        let drawn = unsafe {
            frame.drawInRect_fromRect_operation_fraction_respectFlipped_hints(
                rect,
                rect,
                NSCompositingOperation::Copy,
                1.0,
                false,
                None,
            )
        };
        context.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();
        assert!(drawn, "AppKit drew the CPU frame");

        for (x, colour) in SWATCHES.iter().enumerate() {
            let arrived = srgb_bytes_at(&surface, x);
            assert!(
                arrived.iter().zip(colour).all(|(a, b)| a.abs_diff(*b) <= 2),
                "{} drawn from the CPU frame arrived as {}",
                hex(*colour),
                hex(arrived)
            );
        }
    }

    /// Solid CSS colours leave both renderers as the CSS bytes, so one sRGB
    /// tag on the window is right for the GPU and the CPU frame alike.
    #[test]
    fn solid_css_colours_are_their_css_bytes_on_the_gpu_and_on_the_cpu() {
        const DPI: f32 = 2.0;
        let mut row = Dom::create_div().with_css("display: flex; width: 600px; height: 100px;");
        for colour in SWATCHES {
            let style = format!("width: 100px; height: 100px; background: {};", hex(colour));
            row = row.with_child(Dom::create_div().with_css(style.as_str()));
        }
        let dom = Dom::create_body().with_css("margin: 0px;").with_child(row);
        let Some(frames) = draw_both(dom, 600.0, 100.0, DPI) else {
            return;
        };
        keep("solid-css-colours", &frames);
        for (i, colour) in SWATCHES.iter().enumerate() {
            let (x, y) = (i as u32 * 200 + 100, 100);
            for (backend, pixmap) in [("GPU", &frames.gpu), ("CPU", &frames.cpu)] {
                let at = ((y * pixmap.width + x) * 4) as usize;
                let drawn = [
                    pixmap.data()[at],
                    pixmap.data()[at + 1],
                    pixmap.data()[at + 2],
                ];
                assert!(
                    drawn.iter().zip(colour).all(|(a, b)| a.abs_diff(*b) <= 1),
                    "the {backend} drew {} as {}",
                    hex(*colour),
                    hex(drawn)
                );
            }
        }
    }
}
