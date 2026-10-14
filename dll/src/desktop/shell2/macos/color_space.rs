//! The colour space of the pixels azul hands to AppKit.
//!
//! Both renderers write CSS colours as they are, so every byte in a frame is
//! an sRGB value. A Mac converts what a window shows into the colour space
//! of the display it is on, and for that it has to know what space the
//! pixels are in. The same bytes read as the display's own space look too
//! saturated on a wide-gamut (Display P3) screen, and read as the generic
//! RGB space (gamma 1.8) they look washed out.

use objc2::{rc::Retained, AnyThread};
use objc2_app_kit::NSBitmapImageRep;
use objc2_foundation::ns_string;

/// The bitmap the CPU view copies its framebuffer into before drawing it:
/// `width` x `height` pixels of 8-bit RGBA, premultiplied, top row first.
/// Its rows may be padded, so writers read `bytesPerRow` back.
pub(super) fn cpu_frame_bitmap(width: usize, height: usize) -> Option<Retained<NSBitmapImageRep>> {
    unsafe {
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
    }
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

    /// The pixel at column `x` of the first row, as sRGB bytes.
    fn srgb_bytes_at(bitmap: &NSBitmapImageRep, x: usize) -> [u8; 3] {
        let colour = bitmap
            .colorAtX_y(x as isize, 0)
            .expect("a pixel inside the bitmap");
        let srgb = colour
            .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
            .expect("an RGB colour converts to sRGB");
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        [
            byte(srgb.redComponent()),
            byte(srgb.greenComponent()),
            byte(srgb.blueComponent()),
        ]
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
