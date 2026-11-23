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
    parse(
        "<html><head><style>\
         body { font-size: 20px; color: black; } \
         #root { display: flex; flex-direction: column; } \
         .bar { width: 100%; height: 30px; background: green; } \
         .box { width: 60px; height: 40px; background: blue; }\
         </style></head>\
         <body><div id=\"root\"><div class=\"bar\"/><div class=\"box\"/></div></body></html>",
    )
}

fn parse(xml: &str) -> StyledDom {
    crate::xml::parse_xml_to_styled_dom(xml).expect("the page parses")
}

fn lay_out(lw: &mut LayoutWindow, width: f32, height: f32) {
    let mut state = FullWindowState::default();
    state.size.dimensions = LogicalSize::new(width, height);
    lw.current_window_state = state.clone();
    lw.layout_and_generate_display_list(
        page(),
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
