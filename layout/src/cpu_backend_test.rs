//! Tests of the CPU frame (`CpuBackend::render_frame`): what a frame reports about the pixels it
//! painted.

use azul_core::{geom::LogicalSize, resources::RendererResources, styled_dom::StyledDom};
use rust_fontconfig::FcFontCache;

use super::*;
use crate::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};

/// The page of the E2E scenario `op_resize_shrink_stays_full`: a bar as wide as the window.
fn page() -> StyledDom {
    page_with("blue")
}

/// [`page`] with a box of another colour.
fn page_with(box_color: &str) -> StyledDom {
    parse(&format!(
        "<html><head><style>\
         body {{ font-size: 20px; color: black; }} \
         #root {{ display: flex; flex-direction: column; }} \
         .bar {{ width: 100%; height: 30px; background: green; }} \
         .box {{ width: 60px; height: 40px; background: {box_color}; }}\
         </style></head>\
         <body><div id=\"root\"><div class=\"bar\"/><div class=\"box\"/></div></body></html>"
    ))
}

fn parse(xml: &str) -> StyledDom {
    crate::xml::parse_xml_to_styled_dom(xml).expect("the page parses")
}

fn lay_out(lw: &mut LayoutWindow, width: f32, height: f32) {
    lay_out_page(lw, page(), width, height);
}

fn lay_out_page(lw: &mut LayoutWindow, dom: StyledDom, width: f32, height: f32) {
    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = state.clone();
    lw.layout_and_generate_display_list(
        dom,
        &state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// The damage a frame reports names the pixels of the frame, and only those: after a window
/// shrinks, the bar that was 784 px wide is 484 px wide, and the 300 px it lost lie past the new
/// edge. Reported whole, that strip was PAINT damage outside the frame - and outside every rect
/// the frame presented, which is the rasteriser's clamped paint (`present ⊇ paint`).
#[test]
fn a_shrunk_frames_paint_damage_lies_inside_the_frame() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let resources = RendererResources::default();
    let mut backend = CpuBackend::new();

    lay_out(&mut lw, 800.0, 900.0);
    backend.render_frame(&lw, &resources, 800.0, 900.0, 1.0);

    lay_out(&mut lw, 500.0, 600.0);
    backend.render_frame(&lw, &resources, 500.0, 600.0, 1.0);

    let FrameDamage::Rects(paint) = &backend.last_frame_damage else {
        panic!("a shrink repaints incrementally: {:?}", backend.last_frame_damage);
    };
    assert!(!paint.is_empty(), "the bar that got narrower is damaged");
    for rect in paint {
        let (right, bottom) = (rect.origin.x + rect.size.width, rect.origin.y + rect.size.height);
        assert!(
            rect.origin.x >= 0.0 && rect.origin.y >= 0.0 && right <= 500.0 && bottom <= 600.0,
            "paint damage {rect:?} reaches past the 500 x 600 frame"
        );
    }
    // The strip the bar lost is in the frame up to its edge, and the frame says so.
    assert!(
        paint.iter().any(|r| r.origin.x + r.size.width >= 500.0),
        "the lost strip is damaged up to the new edge: {paint:?}"
    );
    // Every paint rect is presented: its centre lies in a rect of the present damage.
    let FrameDamage::Rects(present) = &backend.last_present_damage else {
        panic!("a shrink presents rects: {:?}", backend.last_present_damage);
    };
    for rect in paint {
        let (cx, cy) = (
            rect.origin.x + rect.size.width / 2.0,
            rect.origin.y + rect.size.height / 2.0,
        );
        assert!(
            present.iter().any(|p| {
                cx >= p.origin.x
                    && cx < p.origin.x + p.size.width
                    && cy >= p.origin.y
                    && cy < p.origin.y + p.size.height
            }),
            "paint damage {rect:?} is in no present rect of {present:?}"
        );
    }
}

/// What the window's settings ask of the frame reaches it: where the dumps go, the bound of the
/// kept surfaces and the thread budget. A backend reads them from the window at every frame, so
/// a window that was given other settings changes the next frame, and a window that was given
/// none dumps nothing.
///
/// Mutation: a backend that takes the dump directories from anywhere but the window's settings
/// writes no file here (or writes one for the window without settings).
#[test]
fn a_frame_follows_the_settings_of_its_window() {
    use azul_css::env::MapEnv;

    use crate::settings::LayoutSettings;

    let dir = std::env::temp_dir().join(format!("azul-cpu-frame-settings-{}", std::process::id()));
    drop(std::fs::remove_dir_all(dir.as_path()));
    let resources = RendererResources::default();

    // A window nobody gave settings: nothing is dumped, the defaults hold.
    let mut plain = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut backend = CpuBackend::new();
    lay_out(&mut plain, 300.0, 200.0);
    backend.render_frame(&plain, &resources, 300.0, 200.0, 1.0);
    assert!(!dir.exists(), "no setting, no dump");
    assert_eq!(backend.retained_frames.bytes(), 0);

    // A window that asks for dumps, a bound and one thread.
    let text = dir.to_str().expect("a temp dir of text");
    let env = MapEnv::new()
        .with("AZ_DUMP_FRAME_DIR", text)
        .with("AZ_DUMP_DL_DIR", text)
        .with("AZ_CPU_RETAINED_MB", "7")
        .with("AZ_CPU_THREADS", "1");
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_settings(LayoutSettings::from_lookup(&env));
    let mut backend = CpuBackend::new();
    lay_out(&mut lw, 300.0, 200.0);
    backend.render_frame(&lw, &resources, 300.0, 200.0, 1.0);
    assert!(dir.join("frame_000_full.png").is_file(), "the first frame is dumped");
    assert!(dir.join("dl_000.json").is_file(), "the first display list is dumped");
    assert!(backend.retained_frames.can_keep(7 << 20), "the bound of the settings holds");
    assert!(!backend.retained_frames.can_keep((7 << 20) + 1), "and no more than it");
    assert_eq!(backend.glyph_cache.scratch.threads, 1, "one thread paints everything");
    drop(std::fs::remove_dir_all(dir.as_path()));
}

// ---- the frame tap ----

use azul_core::refany::RefAny;

use crate::frame_tap::{FrameTap, FrameTapCallbackType, FrameTapInfo};

/// What a test's frame tap keeps: the picture, built the way a recorder builds it (the whole frame,
/// or only the rows of the damaged rectangles laid over the picture it had), and what it was told of
/// each frame.
struct Tapped {
    picture: Vec<u8>,
    /// Width, height, stride, scale, whole, rectangles - one entry for each call.
    told: Vec<(u32, u32, usize, f32, bool, usize)>,
    /// The pixels the rectangles of each call name.
    named: Vec<usize>,
    wrong: Vec<String>,
}

extern "C" fn tapped(mut data: RefAny, info: FrameTapInfo) {
    let mut seen = data.downcast_mut::<Tapped>().expect("the record of the test");
    let (stride, height) = (info.stride, info.height as usize);
    let pixels = info.pixels.as_slice();
    if pixels.len() != stride * height {
        seen.wrong.push(format!("{} bytes for {stride} x {height}", pixels.len()));
        return;
    }
    let rects = info.damage.as_slice();
    let rects: Vec<(usize, usize, usize, usize)> = rects
        .chunks_exact(4)
        .map(|r| (r[0] as usize, r[1] as usize, r[2] as usize, r[3] as usize))
        .collect();
    if info.full || seen.picture.len() != pixels.len() {
        if !info.full {
            seen.wrong.push(String::from("a frame of another size that is not whole"));
        }
        seen.picture = pixels.to_vec();
    } else {
        for &(x, y, w, h) in &rects {
            for row in y..y + h {
                let from = row * stride + x * 4;
                seen.picture[from..from + w * 4].copy_from_slice(&pixels[from..from + w * 4]);
            }
        }
    }
    seen.told.push((info.width, info.height, info.stride, info.scale, info.full, rects.len()));
    seen.named.push(rects.iter().map(|&(_, _, w, h)| w * h).sum());
}

fn tapped_record() -> RefAny {
    RefAny::new(Tapped { picture: Vec::new(), told: Vec::new(), named: Vec::new(), wrong: Vec::new() })
}

fn look_at<R>(record: &RefAny, f: impl FnOnce(&Tapped) -> R) -> R {
    let mut record = record.clone();
    let seen = record.downcast_ref::<Tapped>().expect("the record of the test");
    f(&seen)
}

/// The window's own picture of the last frame.
fn shown(backend: &CpuBackend) -> Vec<u8> {
    backend.last_frame.as_ref().expect("a frame").data().to_vec()
}

/// The frame tap hears the CPU frame as the window shows it: every frame that changed, at the size and
/// the scale it was painted at, whole when the whole window changed and as rectangles when a part did;
/// none for a frame that changed nothing; and the picture a tap builds from what it is told (the whole
/// frame, then only the rows of the rectangles) is the window's picture, byte for byte. That last is
/// what a recorder stands on: a rectangle that is a row short or at the wrong scale leaves a stale
/// stripe in the video.
///
/// Mutation: no call from `render_frame` (nothing heard); a call for the idle frame (the second
/// entry); the damage in logical pixels (the rectangles name a quarter of the pixels and the
/// pictures part); the call before the pixels are painted (the picture is the frame before); no frame
/// made for a tap that has heard nothing (the late tap is told nothing at rest).
#[test]
fn a_frame_tap_hears_every_frame_that_changed_and_builds_the_windows_picture() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let resources = RendererResources::default();
    let mut backend = CpuBackend::new();
    let record = tapped_record();
    lw.frame_tap = Some(FrameTap::new(record.clone(), tapped as FrameTapCallbackType));

    // The first frame is the whole window, at twice the logical size.
    lay_out(&mut lw, 300.0, 200.0);
    backend.render_frame(&lw, &resources, 300.0, 200.0, 2.0);
    look_at(&record, |t| {
        assert_eq!(t.told, vec![(600, 400, 2400, 2.0, true, 0)]);
        assert_eq!(t.picture, shown(&backend), "the whole frame");
    });

    // Nothing changed: no frame is painted and the tap is not called.
    backend.render_frame(&lw, &resources, 300.0, 200.0, 2.0);
    assert_eq!(backend.last_present_damage, FrameDamage::None, "an idle frame");
    look_at(&record, |t| assert_eq!(t.told.len(), 1, "an idle frame is not told"));

    // The box changes colour: a part of the window, in device pixels, and the picture still equals it.
    lay_out_page(&mut lw, page_with("red"), 300.0, 200.0);
    backend.render_frame(&lw, &resources, 300.0, 200.0, 2.0);
    assert!(matches!(backend.last_present_damage, FrameDamage::Rects(_)), "{:?}", backend.last_present_damage);
    look_at(&record, |t| {
        assert_eq!(t.told.len(), 2);
        let (w, h, stride, scale, whole, rects) = t.told[1];
        assert_eq!((w, h, stride, scale, whole), (600, 400, 2400, 2.0, false));
        assert!(rects >= 1, "what changed is told");
        assert!(t.named[1] < 600 * 400 / 4, "a part, not the window: {} px of {}", t.named[1], 600 * 400);
        assert!(t.named[1] >= 120 * 80, "the box of 60 x 40 logical px is 120 x 80 device px: {}", t.named[1]);
        assert_eq!(t.picture, shown(&backend), "the rows of the rectangles are the whole of the change");
        assert!(t.picture.chunks_exact(4).any(|p| p == [255, 0, 0, 255]), "the new colour is in it");
        assert!(t.wrong.is_empty(), "{:?}", t.wrong);
    });

    // A resize, and then another scale: each is a whole frame at its own size.
    lay_out(&mut lw, 400.0, 300.0);
    backend.render_frame(&lw, &resources, 400.0, 300.0, 2.0);
    backend.render_frame(&lw, &resources, 400.0, 300.0, 1.0);
    look_at(&record, |t| {
        assert_eq!(t.told.len(), 4);
        assert_eq!(t.told[2], (800, 600, 3200, 2.0, true, 0));
        assert_eq!(t.told[3], (400, 300, 1600, 1.0, true, 0));
        assert_eq!(t.picture, shown(&backend));
        assert!(t.wrong.is_empty(), "{:?}", t.wrong);
    });

    // A window whose tap was taken away is told nothing more.
    lw.frame_tap = None;
    lay_out_page(&mut lw, page_with("green"), 400.0, 300.0);
    backend.render_frame(&lw, &resources, 400.0, 300.0, 1.0);
    look_at(&record, |t| assert_eq!(t.told.len(), 4, "no tap, no call"));

    // A tap registered on a window at rest is told the whole window at the next frame - the frame is
    // made for it, though nothing changed - and then the window is at rest again.
    let late = tapped_record();
    lw.frame_tap = Some(FrameTap::new(late.clone(), tapped as FrameTapCallbackType));
    backend.render_frame(&lw, &resources, 400.0, 300.0, 1.0);
    look_at(&late, |t| {
        assert_eq!(t.told, vec![(400, 300, 1600, 1.0, true, 0)]);
        assert_eq!(t.picture, shown(&backend));
    });
    backend.render_frame(&lw, &resources, 400.0, 300.0, 1.0);
    look_at(&late, |t| assert_eq!(t.told.len(), 1, "at rest again"));
}

/// A frame that the shell lets the renderer paint into a buffer of its own (a Wayland slot, a Core
/// Animation surface) is told as well, and with the buffer's own rows: the renderer lets that buffer go
/// at the end of the frame (`last_frame` stays empty), so the tap is called before that, and a buffer
/// whose rows are padded is told with its stride and the frame's width.
///
/// Mutation: the call placed after the buffer is let go (the tap is never called for such a frame); the
/// frame's width taken from the buffer's (304 instead of 300); the stride taken as `width * 4`.
#[test]
fn a_frame_painted_into_the_platforms_buffer_is_told_with_the_buffers_rows() {
    use crate::cpurender::AzulPixmap;

    let (w, h, pad) = (300_u32, 200_u32, 4_u32);
    let pitch = (w + pad) as usize * 4;
    let mut buffer = vec![0_u8; pitch * h as usize];
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let resources = RendererResources::default();
    let mut backend = CpuBackend::new();
    let record = tapped_record();
    lw.frame_tap = Some(FrameTap::new(record.clone(), tapped as FrameTapCallbackType));

    // SAFETY: `buffer` is `(w + pad) * h * 4` bytes, outlives the frame and is only read after the
    // frame, once the backend let its pixmap go.
    backend.native_target = unsafe { AzulPixmap::from_external(buffer.as_mut_ptr(), w + pad, h) };
    backend.native_target_row_padding_px = pad;
    backend.native_target_holds_previous_frame = false;
    lay_out(&mut lw, 300.0, 200.0);
    backend.render_frame(&lw, &resources, 300.0, 200.0, 1.0);
    assert!(backend.rendered_native && backend.last_frame.is_none(), "painted in the platform's buffer");

    look_at(&record, |t| {
        assert_eq!(t.told, vec![(300, 200, pitch, 1.0, true, 0)]);
        assert!(t.wrong.is_empty(), "{:?}", t.wrong);
        // The frame's pixels of every row are the buffer's.
        for row in 0..h as usize {
            let (from, to) = (row * pitch, row * pitch + w as usize * 4);
            assert_eq!(t.picture[from..to], buffer[from..to], "row {row}");
        }
        assert!(t.picture.chunks_exact(4).any(|p| p == [0, 0, 255, 255]), "the blue box is in it");
    });
}
