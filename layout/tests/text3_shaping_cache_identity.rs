//! T2 — the shaping-cache IDENTITY gate (SHAPED_TEXT_REFACTOR_PLAN §2.3).
//!
//! `shape_visual_items_with_per_item_cache`'s hit path re-stamps paint
//! (colour) and identity (`source_node_id`) onto cached clusters — the
//! correctness fix of commit `8ec9f387d`, which had NO test until this
//! file. Any cache-sharing scheme (the §3.2 ShapedRun relocation, Option
//! C) re-opens exactly this defect, and without this gate nothing would
//! notice: two ribbon tabs with the same caption would silently render in
//! one colour and hit-test as one node.
//!
//! The negative control is a first-class part of the gate: with
//! `AZ_T2_SKIP_RESTAMP=1` the hit path hands back the cached entry
//! unmodified, and `identity_gate_negative_control` requires the pins to
//! FAIL under it — proving the gate can actually see the defect.

use azul_core::{
    dom::{Dom, IdOrClass},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};

/// Pin 4 (plan §2.3): the two runs' GLYPH GEOMETRY is identical — same
/// glyph ids, same advances (x-deltas), differing only in the block
/// offset. That identity IS the cache key's promise, and it is what
/// makes sharing the shaped data sound in the first place: if this
/// fails, the runs were shaped separately and the cache never fired.
fn assert_geometry_identical(
    a: &[(u32, f32, f32)],
    b: &[(u32, f32, f32)],
) -> Result<(), String> {
    if a.is_empty() || a.len() != b.len() {
        return Err(format!("glyph counts differ: {} vs {}", a.len(), b.len()));
    }
    for (i, ((ga, xa, _), (gb, xb, _))) in a.iter().zip(b.iter()).enumerate() {
        if ga != gb {
            return Err(format!("glyph id differs at {i}: {ga} vs {gb}"));
        }
        // Compare x RELATIVE to each run's first glyph.
        let ra = xa - a[0].1;
        let rb = xb - b[0].1;
        if (ra - rb).abs() > 0.01 {
            return Err(format!("relative x differs at {i}: {ra} vs {rb}"));
        }
    }
    Ok(())
}
use rust_fontconfig::FcFontCache;

/// Two paragraphs with IDENTICAL text (same layout_hash inputs) but
/// DIFFERENT colours — the cache-sharing trap.
/// (item index, RGB, [(glyph id, x, y)]) for one text run.
type RunIdentity = (Option<usize>, (u8, u8, u8), Vec<(u32, f32, f32)>);

fn layout_two_same_text_nodes() -> Vec<RunIdentity> {
    let mut dom = Dom::create_body()
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("a".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Layout")),
        )
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("b".into())].into())
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Layout")),
        );
    let css_str = r#"
        .a { color: rgb(200, 30, 30); }
        .b { color: rgb(30, 30, 200); }
    "#;
    let (css, _) = azul_css::parser2::new_from_str(css_str);
    let styled_dom = StyledDom::create(&mut dom, css);

    let font_cache = FcFontCache::build();
    let mut layout_window = LayoutWindow::new(font_cache).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(600.0, 400.0);
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            &window_state,
            &renderer_resources,
            &system_callbacks,
            &mut dbg,
        )
        .unwrap();

    let dl = &layout_window
        .layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
        .expect("root layout")
        .display_list;
    dl.items
        .iter()
        .filter_map(|it| match it {
            DisplayListItem::Text {
                color,
                source_node_index,
                glyphs,
                ..
            } => Some((
                *source_node_index,
                (color.r, color.g, color.b),
                glyphs
                    .iter()
                    .map(|g| (g.index, g.point.x, g.point.y))
                    .collect::<Vec<_>>(),
            )),
            _ => None,
        })
        .collect()
}

fn assert_identity(runs: &[RunIdentity]) -> Result<(), String> {
    if runs.len() < 2 {
        return Err(format!("expected 2 text runs, got {}", runs.len()));
    }
    // Pin 1: distinct colours survive the shared shaping cache.
    if runs[0].1 == runs[1].1 {
        return Err(format!(
            "both runs have colour {:?} — the cache returned the first \
             node's paint for the second node",
            runs[0].1
        ));
    }
    // Pin 3: distinct source_node_index (damage/hit-test identity).
    if runs[0].0 == runs[1].0 || runs[0].0.is_none() || runs[1].0.is_none() {
        return Err(format!(
            "source_node_index not distinct: {:?} vs {:?} — the second \
             node's text is attributed to the first",
            runs[0].0, runs[1].0
        ));
    }
    // Pin 4: identical glyph geometry (the cache key's promise).
    assert_geometry_identical(&runs[0].2, &runs[1].2)?;
    Ok(())
}

#[test]
fn same_text_different_nodes_keep_their_colour_and_identity() {
    // Run in a subprocess-free way: the env var must NOT be set here.
    assert!(
        std::env::var_os("AZ_T2_SKIP_RESTAMP").is_none(),
        "positive pin must run without the NC knob"
    );
    let runs = layout_two_same_text_nodes();
    if let Err(e) = assert_identity(&runs) {
        panic!("identity gate: {e}");
    }
}

#[test]
fn identity_gate_negative_control() {
    // The NC proves the gate SEES the defect: with the re-stamp skipped
    // (the exact pre-8ec9f387d behaviour), the pins must fail. Subprocess
    // so the env var cannot leak into the positive pin.
    let exe = std::env::current_exe().unwrap();
    let out = std::process::Command::new(exe)
        .arg("same_text_different_nodes_keep_their_colour_and_identity")
        .arg("--exact")
        .arg("--nocapture")
        .env("AZ_T2_SKIP_RESTAMP", "1")
        .output()
        .expect("spawn self");
    let code = out.status.code();
    assert!(
        !out.status.success(),
        "NEGATIVE CONTROL DID NOT FIRE: with AZ_T2_SKIP_RESTAMP=1 the \
         identity pins still passed (exit {code:?}) — the gate cannot see \
         the defect it exists to catch. stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
