//! What does one FRAME cost — layout *and* rendering together?
//!
//! Run: `cargo test --release -p azul-layout --features probe --test all --
//! frame_perf:: --nocapture`
//!
//! `pagination_perf.rs` measures the layout half. The budget that matters to
//! an interactive editor is the whole frame: relayout the document, then put
//! pixels on the screen. This harness does both against one persistent set of
//! caches — a `LayoutWindow`, a `GlyphCache` and a retained pixmap — because
//! that is what a running window holds, and reports the steady-state cost of
//! the three frame shapes an editor actually produces:
//!
//!   * **idle**   — nothing changed; the frame should be nearly free.
//!   * **edit**   — one character typed into one paragraph.
//!   * **resize** — the viewport width changed, so everything reflows.
//!
//! MEASURE IN RELEASE. A debug build is 10-13x slower and not uniformly so,
//! which points a profile at the wrong function; the banner below says so.

use std::collections::BTreeMap;

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
    xml::DomXmlExt,
};

const W: f32 = 820.0;
const H: f32 = 1000.0;

/// The miniword sample shape: headings, a list, and `paragraphs` paragraphs.
/// `nth_body` lets a single paragraph differ so an "edit" frame can be
/// simulated without rebuilding the harness.
fn sample_html(paragraphs: usize, edited: Option<usize>) -> String {
    let mut s = String::from(
        r#"<html><head><style>
        body { font-family: 'Liberation Sans', sans-serif; font-size: 15px;
               color: #1a1a1a; line-height: 1.35; }
        p  { margin-bottom: 11px; }
        h1 { font-size: 28px; color: #2e74b5; margin-bottom: 12px; }
        ul { margin-bottom: 11px; margin-left: 36px; }
    </style></head><body>
    <h1>Project Report</h1>
    <ul><li>alpha item</li><li>beta item</li><li>gamma item</li></ul>
"#,
    );
    for i in 0..paragraphs {
        let suffix = if edited == Some(i) { "X" } else { "" };
        s.push_str(&format!(
            "<p>Paragraph number {i}: lorem ipsum dolor sit amet, consectetur \
             adipiscing elit, sed do eiusmod tempor incididunt ut labore et \
             dolore magna aliqua nostrud exercitation.{suffix}</p>\n"
        ));
    }
    s.push_str("</body></html>");
    s
}

struct FrameHarness {
    window: LayoutWindow,
    glyph_cache: GlyphCache,
    resources: RendererResources,
    callbacks: ExternalSystemCallbacks,
    width: f32,
}

impl FrameHarness {
    fn new() -> Self {
        let fc = azul_layout::font::loading::build_font_cache();
        Self {
            window: LayoutWindow::new(fc).expect("LayoutWindow"),
            glyph_cache: GlyphCache::new(),
            resources: RendererResources::default(),
            callbacks: ExternalSystemCallbacks::rust_internal(),
            width: W,
        }
    }

    /// One whole frame: parse + style + layout + display list + rasterize.
    /// Returns the pixmap so the caller can keep it alive (and so the
    /// rasterization cannot be optimised away).
    fn frame(&mut self, html: &str) -> AzulPixmap {
        let styled_dom = {
            let _p = azul_layout::probe::Probe::span("frame_parse_and_cascade");
            // `from_xml_string` runs the parse AND the cascade, returning a
            // StyledDom — the `<style>` block in the fixture is what styles it.
            Dom::from_xml_string(html)
        };

        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(self.width, H);

        {
            let _p = azul_layout::probe::Probe::span("frame_layout");
            let mut dbg = None;
            self.window
                .layout_and_generate_display_list(
                    styled_dom,
                    &ws,
                    &self.resources,
                    &self.callbacks,
                    &mut dbg,
                )
                .expect("layout");
        }

        let _p = azul_layout::probe::Probe::span("frame_render");
        let dl = &self
            .window
            .layout_results
            .get(&DomId { inner: 0 })
            .expect("layout result")
            .display_list;
        let opts = RenderOptions {
            width: self.width,
            height: H,
            dpi_factor: 1.0,
        };
        cpurender::render_with_font_manager(
            dl,
            &self.resources,
            &self.window.font_manager,
            opts,
            &mut self.glyph_cache,
        )
        .expect("render")
    }
}

impl FrameHarness {
    /// Relayout, then repaint ONLY `damage` — the path a real shell takes
    /// for an edit. `pixmap` is retained across calls, as a window's is.
    fn damaged_frame(
        &mut self,
        html: &str,
        pixmap: &mut AzulPixmap,
        damage: &[azul_core::geom::LogicalRect],
    ) {
        let styled_dom = {
            let _p = azul_layout::probe::Probe::span("frame_parse_and_cascade");
            Dom::from_xml_string(html)
        };
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(self.width, H);
        {
            let _p = azul_layout::probe::Probe::span("frame_layout");
            let mut dbg = None;
            self.window
                .layout_and_generate_display_list(
                    styled_dom,
                    &ws,
                    &self.resources,
                    &self.callbacks,
                    &mut dbg,
                )
                .expect("layout");
        }
        let _p = azul_layout::probe::Probe::span("frame_render_damaged");
        let dl = &self
            .window
            .layout_results
            .get(&DomId { inner: 0 })
            .expect("layout result")
            .display_list;
        let state = cpurender::CpuRenderState::new(Default::default());
        cpurender::render_display_list_damaged(
            dl,
            pixmap,
            1.0,
            &self.resources,
            &self.window.font_manager,
            &mut self.glyph_cache,
            &state,
            damage,
        )
        .expect("damaged render");
    }
}

/// Group drained probe events by name and report SELF time.
///
/// Spans arrive post-order carrying (duration, depth), so a span's immediate
/// children are the not-yet-consumed spans at depth+1 that precede it.
/// Subtracting them turns "this subtree cost X" (which double-counts) into
/// "this phase itself cost X" — the only number that names a hot spot.
fn report(label: &str, frames: u32) {
    let events = azul_layout::probe::Probe::drain();
    if events.is_empty() {
        eprintln!("[frame] {label}: no probe events — rerun with `--features probe`");
        return;
    }
    let mut totals: BTreeMap<&'static str, (u64, u64, u32)> = BTreeMap::new();
    let mut pending: Vec<(u16, u64)> = Vec::new();
    for e in &events {
        let azul_layout::probe::EventKind::Span { dur_ns } = e.kind else {
            continue;
        };
        let mut children_ns = 0u64;
        while let Some(&(d, ns)) = pending.last() {
            if d > e.depth {
                if d == e.depth + 1 {
                    children_ns += ns;
                }
                pending.pop();
            } else {
                break;
            }
        }
        let slot = totals.entry(e.name).or_insert((0, 0, 0));
        slot.0 += dur_ns.saturating_sub(children_ns);
        slot.1 += dur_ns;
        slot.2 += 1;
        pending.push((e.depth, dur_ns));
    }
    let mut rows: Vec<_> = totals.into_iter().collect();
    rows.sort_by_key(|(_, (self_ns, _, _))| std::cmp::Reverse(*self_ns));
    eprintln!("[frame] {label} — per-frame SELF time over {frames} frames:");
    for (name, (self_ns, cum_ns, count)) in rows.iter().take(24) {
        let per_frame_ms = *self_ns as f64 / 1_000_000.0 / f64::from(frames);
        if per_frame_ms < 0.005 {
            continue;
        }
        eprintln!(
            "[frame]   {:<28} {:>7.2} ms/frame  ({:>7.2} ms cum, {count} calls)",
            name,
            per_frame_ms,
            *cum_ns as f64 / 1_000_000.0,
        );
    }
}

#[test]
fn frame_cost_idle_edit_and_resize() {
    // `Probe`'s recording flag is a process-global atomic (the buffer it gates
    // is thread-local). This file shares a binary with `probe_gate`, which
    // deliberately flips that flag on and off; without this lock the `report`
    // calls below would attribute a truncated or a phantom profile depending
    // on the interleaving. See `crate::PROBE_LOCK`.
    let _serialised = crate::probe_lock();

    if cfg!(debug_assertions) {
        eprintln!(
            "[frame] *** DEBUG BUILD — these numbers are NOT the shipped cost. \
             Re-run with --release before drawing any conclusion. ***"
        );
    }

    const N: u32 = 10;
    let mut h = FrameHarness::new();
    let html = sample_html(30, None);

    // Cold frame: fonts load, glyphs get hinted, caches fill.
    let t = std::time::Instant::now();
    let _ = h.frame(&html);
    eprintln!("[frame] cold   = {:?}", t.elapsed());
    let _ = azul_layout::probe::Probe::drain();

    // IDLE: the same document over and over. Nothing changed, so this is the
    // floor — whatever it costs is work the engine does unconditionally.
    let t = std::time::Instant::now();
    for _ in 0..N {
        let _ = h.frame(&html);
    }
    eprintln!("[frame] idle   = {:?} per frame", t.elapsed() / N);
    report("idle", N);

    // EDIT: one character changes in one paragraph, as when typing.
    let t = std::time::Instant::now();
    for i in 0..N {
        let edited = sample_html(30, Some((i as usize) % 30));
        let _ = h.frame(&edited);
    }
    eprintln!("[frame] edit   = {:?} per frame", t.elapsed() / N);
    report("edit", N);

    // RESIZE: the width changes every frame, so everything reflows and every
    // glyph lands at a new position.
    let t = std::time::Instant::now();
    for i in 0..N {
        h.width = W - (i % 5) as f32 * 20.0;
        let _ = h.frame(&html);
    }
    eprintln!("[frame] resize = {:?} per frame", t.elapsed() / N);
    report("resize", N);

    // DAMAGED EDIT — what typing actually costs in a real window.
    //
    // The three phases above deliberately repaint the WHOLE frame: that is
    // the first-paint / full-expose case. A shell with damage tracking
    // repaints only the changed paragraph, and `render_display_list_damaged`
    // is the function it calls. One paragraph of a 30-paragraph page is the
    // realistic edit, and this — not the full-frame number — is what the
    // interactive budget is spent against.
    h.width = W;
    let mut retained = h.frame(&html);
    let _ = azul_layout::probe::Probe::drain();
    let para = azul_core::geom::LogicalRect {
        origin: azul_core::geom::LogicalPosition { x: 0.0, y: 300.0 },
        size: LogicalSize::new(W, 40.0),
    };
    let t = std::time::Instant::now();
    for i in 0..N {
        let edited = sample_html(30, Some((i as usize) % 30));
        h.damaged_frame(&edited, &mut retained, core::slice::from_ref(&para));
    }
    eprintln!(
        "[frame] edit (DAMAGED, one paragraph) = {:?} per frame",
        t.elapsed() / N
    );
    report("edit-damaged", N);
}
