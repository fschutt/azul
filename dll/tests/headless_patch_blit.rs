//! ROUND-3 GOLDEN GATE: a frame presented via the patch BLIT (retained
//! pixmap shifted by the dominant layout delta, exceptions + strips
//! repainted) must be PIXEL-IDENTICAL to the same frame fully repainted.
//! Zero tolerance — the blit is a presentation optimization, never a
//! rendering change. Runs the same cold+hinted-resize sequence twice:
//! once with DL patching on (blit path) and once with it off (control).

use azul_core::dom::{Dom, IdOrClass, NodeType};
use azul_core::geom::LogicalSize;
use azul_core::resources::RendererResources;
use azul_core::styled_dom::StyledDom;
use azul_layout::callbacks::ExternalSystemCallbacks;
use azul_layout::solver3::display_list::set_dl_patching_enabled;
use azul_layout::window::LayoutWindow;
use azul_layout::window_state::FullWindowState;
use rust_fontconfig::FcFontCache;

use azul::desktop::shell2::headless::CpuBackend;

fn page_dom() -> StyledDom {
    let dom = Dom::create_node(NodeType::Div)
        .with_ids_and_classes(vec![IdOrClass::Class("root".into())].into())
        .with_child(
            Dom::create_node(NodeType::Div)
                .with_ids_and_classes(vec![IdOrClass::Class("page".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "blit golden paragraph one",
                ))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "second line of golden text",
                )),
        );
    let css = r#"
        * { margin: 0px; padding: 0px; }
        .root { width: 100%; height: 100%; background: #888;
                display: flex; justify-content: center; }
        .page { width: 300px; height: 200px; background: #fff; }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css);
    let mut dom = dom;
    StyledDom::create(&mut dom, css)
}

/// Cold layout @640x480, render; hinted resize @680x480, render.
/// Returns (second frame's pixels, whether the blit was applied).
fn run(patching: bool) -> (Vec<u8>, bool) {
    // Honest (refined) damage is REQUIRED here: with full-strength damage
    // the whole frame repaints and the blit is unfalsifiable.
    azul_layout::cpurender::set_dl_diff_refinements(true);
    set_dl_patching_enabled(patching);
    let font_cache = FcFontCache::build();
    let mut lw = LayoutWindow::new(font_cache).unwrap();
    let rr = RendererResources::default();
    let cb = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;

    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(640.0, 480.0);
    lw.layout_and_generate_display_list(page_dom(), &ws, &rr, &cb, &mut dbg)
        .unwrap();

    let mut backend = CpuBackend::new();
    backend.render_frame(&lw, &rr, 640.0, 480.0, 1.0);

    lw.layout_cache.resize_only_hint = true;
    ws.size.dimensions = LogicalSize::new(680.0, 480.0);
    lw.layout_and_generate_display_list(page_dom(), &ws, &rr, &cb, &mut dbg)
        .unwrap();
    assert!(
        lw.layout_cache.last_reconcile_was_skipped,
        "harness: the resize must take the skip branch"
    );
    if patching {
        assert!(
            lw.layout_cache.last_patch_move.is_some(),
            "harness: the patched pass must export a move summary \
             (centered page delta = +20 logical px, integral)"
        );
    }

    backend.render_frame(&lw, &rr, 680.0, 480.0, 1.0);
    let blit_applied = backend.last_patch_shift_dl != 0;
    let frame = backend
        .last_frame
        .as_ref()
        .expect("render_frame must retain a pixmap")
        .data()
        .to_vec();
    // Restore the default for tests running after us in-process.
    set_dl_patching_enabled(true);
    (frame, blit_applied)
}

#[test]
fn blitted_resize_frame_is_pixel_identical_to_a_full_repaint() {
    let (blit_frame, blit_applied) = run(true);
    let (full_frame, control_applied) = run(false);
    assert!(
        blit_applied,
        "the blit path must actually fire on the patched sequence — a gate \
         that silently takes the slow path both times proves nothing \
         (a zero is not a measurement)"
    );
    assert!(!control_applied, "the control must NOT blit");
    assert_eq!(blit_frame.len(), full_frame.len());
    let diff = blit_frame
        .iter()
        .zip(full_frame.iter())
        .filter(|(a, b)| a != b)
        .count();
    if diff != 0 {
        // Locate the divergence for the log: first + last differing pixel.
        let first = blit_frame
            .iter()
            .zip(full_frame.iter())
            .position(|(a, b)| a != b)
            .unwrap();
        let last = blit_frame.len()
            - 1
            - blit_frame
                .iter()
                .rev()
                .zip(full_frame.iter().rev())
                .position(|(a, b)| a != b)
                .unwrap();
        let w = 680usize;
        for y in [4usize, 8, 12, 16] {
            for x in [167usize, 168, 169, 170, 171] {
                let i = (y * 680 + x) * 4;
                eprintln!(
                    "  px({x},{y}): blit={:?} full={:?}",
                    &blit_frame[i..i + 4],
                    &full_frame[i..i + 4]
                );
            }
        }
        eprintln!(
            "first diff at px ({}, {}), last at ({}, {}), {} bytes total",
            (first / 4) % w,
            (first / 4) / w,
            (last / 4) % w,
            (last / 4) / w,
            diff
        );
    }
    assert_eq!(
        diff, 0,
        "blit-presented frame diverges from the full repaint on {diff} bytes"
    );
}
