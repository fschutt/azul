//! A CPU frame painted incrementally - only its damage, over the frame before
//! it - is pixel for pixel the frame a full redraw paints, over many random
//! frames; a frame that changes nothing rasterises no pixel.
//!
//! Every CPU backend presents a frame by diffing the new display list with
//! the last one (`compute_display_list_damage`), adding what moved without a
//! list change (`gpu_value_damage`, `color_value_damage`), moving what a
//! scroll moved (`execute_scroll_shift`) and repainting only the damage
//! (`render_display_list_damaged`). A full redraw goes through the layered
//! compositor. Whatever the damage misses stays stale on screen, so the two
//! must agree on every pixel of every frame.
//!
//! The pages are real layouts of random scenes: boxes with colours (some
//! translucent), rounded corners, borders, shadows, opacity groups,
//! translated and turned boxes, boxes clipping a child, and a scroll box
//! with a scroll bar. Each frame mutates one thing - a move, a resize, a
//! recolour, an opacity, a transform, a box added or removed, a scroll - and
//! lays the page out again, as an app's rebuild does.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, AzulPixmap, CpuRenderState, ScrollOffsetMap},
    glyph_cache::GlyphCache,
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const W: f32 = 320.0;
const H: f32 = 240.0;

/// xorshift64*: a deterministic stream per seed, no dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn range(&mut self, lo: i32, hi: i32) -> i32 {
        lo + self.below((hi - lo + 1) as u64) as i32
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

#[derive(Clone, Debug)]
struct Shape {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    rgba: [u8; 4],
    radius: i32,
    border: bool,
    shadow: bool,
    opacity: Option<u8>,
    translate: Option<(i32, i32)>,
    rotate: bool,
    clips_child: bool,
}

impl Shape {
    fn random(r: &mut Rng) -> Self {
        Self {
            x: r.range(-20, 280),
            y: r.range(-20, 200),
            w: r.range(4, 120),
            h: r.range(4, 90),
            rgba: [
                r.below(256) as u8,
                r.below(256) as u8,
                r.below(256) as u8,
                if r.chance(25) { r.range(60, 230) as u8 } else { 255 },
            ],
            radius: if r.chance(30) { r.range(2, 20) } else { 0 },
            border: r.chance(30),
            shadow: r.chance(20),
            opacity: r.chance(20).then(|| r.range(20, 90) as u8),
            translate: r.chance(20).then(|| (r.range(-30, 30), r.range(-30, 30))),
            rotate: r.chance(10),
            clips_child: r.chance(25),
        }
    }

    fn css(&self) -> String {
        let [cr, cg, cb, ca] = self.rgba;
        let mut css = format!(
            "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; \
             background: rgba({cr}, {cg}, {cb}, {:.3});",
            self.x,
            self.y,
            self.w,
            self.h,
            f32::from(ca) / 255.0,
        );
        if self.radius > 0 {
            css.push_str(&format!(" border-radius: {}px;", self.radius));
        }
        if self.border {
            css.push_str(" border: 3px solid #224488;");
        }
        if self.shadow {
            css.push_str(" box-shadow: 4px 6px 8px rgba(0, 0, 0, 0.5);");
        }
        if let Some(o) = self.opacity {
            css.push_str(&format!(" opacity: {:.2};", f32::from(o) / 100.0));
        }
        match (self.translate, self.rotate) {
            (Some((dx, dy)), true) => {
                css.push_str(&format!(" transform: translate({dx}px, {dy}px) rotate(12deg);"));
            }
            (Some((dx, dy)), false) => {
                css.push_str(&format!(" transform: translate({dx}px, {dy}px);"));
            }
            (None, true) => css.push_str(" transform: rotate(12deg);"),
            (None, false) => {}
        }
        if self.clips_child {
            css.push_str(" overflow: hidden;");
        }
        css
    }

    fn dom(&self) -> Dom {
        let mut dom = Dom::create_div().with_css(&self.css());
        if self.clips_child {
            dom = dom.with_child(Dom::create_div().with_css(&format!(
                "position: absolute; left: {}px; top: {}px; width: {}px; height: {}px; \
                 background: #33cc66;",
                self.w / 2,
                self.h / 3,
                self.w,
                self.h,
            )));
        }
        dom
    }
}

#[derive(Clone, Debug)]
struct Scene {
    shapes: Vec<Shape>,
    scroll_y: f32,
    /// A second scroll box after the shapes (a panel that opens).
    panel: bool,
}

impl Scene {
    fn random(r: &mut Rng) -> Self {
        Self {
            shapes: (0..8).map(|_| Shape::random(r)).collect(),
            scroll_y: 0.0,
            panel: false,
        }
    }

    /// The page: the scroll box FIRST (its node id stays the same whatever
    /// shapes come and go), then the shapes in paint order.
    fn dom(&self) -> Dom {
        let mut scroller = Dom::create_div().with_css(
            "position: absolute; left: 196px; top: 12px; width: 110px; height: 120px; \
             overflow-y: scroll; background: #f0f0f0;",
        );
        for k in 0..16 {
            let color = if k % 2 == 0 { "#dd3333" } else { "#3366dd" };
            scroller = scroller.with_child(Dom::create_div().with_css(&format!(
                "height: 18px; margin: 2px; background: {color};"
            )));
        }
        let mut body = Dom::create_body()
            .with_css("margin: 0; padding: 0; width: 100%; height: 100%; background: #ffffff;")
            .with_child(scroller);
        for shape in &self.shapes {
            body = body.with_child(shape.dom());
        }
        if self.panel {
            let mut panel = Dom::create_div().with_css(
                "position: absolute; left: 20px; top: 160px; width: 140px; height: 60px; \
                 overflow-y: scroll; background: #fafae0;",
            );
            for k in 0..6 {
                let color = if k % 2 == 0 { "#88aa22" } else { "#aa8822" };
                panel = panel.with_child(Dom::create_div().with_css(&format!(
                    "height: 16px; margin: 2px; background: {color};"
                )));
            }
            body = body.with_child(panel);
        }
        Dom::create_html().with_child(body)
    }

    /// One random change. Returns what it did, for the failure message.
    fn mutate(&mut self, r: &mut Rng) -> String {
        let n = self.shapes.len();
        let i = r.below(n as u64) as usize;
        match r.below(12) {
            0 if n > 0 => {
                let s = &mut self.shapes[i];
                s.x += r.range(-25, 25);
                s.y += r.range(-25, 25);
                format!("moved shape {i}")
            }
            1 if n > 0 => {
                let s = &mut self.shapes[i];
                s.w = (s.w + r.range(-20, 20)).max(1);
                s.h = (s.h + r.range(-20, 20)).max(1);
                format!("resized shape {i}")
            }
            2 if n > 0 => {
                self.shapes[i].rgba = [r.below(256) as u8, r.below(256) as u8, r.below(256) as u8, 255];
                format!("recoloured shape {i}")
            }
            3 if n > 0 => {
                self.shapes[i].opacity = r.chance(70).then(|| r.range(10, 95) as u8);
                format!("changed the opacity of shape {i}")
            }
            4 if n > 0 => {
                self.shapes[i].translate = r.chance(80).then(|| (r.range(-40, 40), r.range(-40, 40)));
                format!("changed the transform of shape {i}")
            }
            5 if n < 12 => {
                let at = r.below(n as u64 + 1) as usize;
                self.shapes.insert(at, Shape::random(r));
                format!("added a shape at {at}")
            }
            6 if n > 1 => {
                self.shapes.remove(i);
                format!("removed shape {i}")
            }
            7 if n > 1 => {
                let j = r.below(n as u64) as usize;
                self.shapes.swap(i, j);
                format!("swapped shapes {i} and {j} (overlap order)")
            }
            8 if n > 0 => {
                let s = &mut self.shapes[i];
                s.radius = if s.radius > 0 { 0 } else { r.range(2, 24) };
                s.border = !s.border;
                format!("changed the corners and border of shape {i}")
            }
            9 if n > 0 => {
                let s = &mut self.shapes[i];
                s.clips_child = !s.clips_child;
                s.shadow = r.chance(50);
                format!("toggled the clip and shadow of shape {i}")
            }
            10 => {
                self.panel = !self.panel;
                format!("{} the panel", if self.panel { "opened" } else { "closed" })
            }
            _ => {
                // Whole device pixels at 1x and 2x: a scroll by a fraction of
                // a device pixel is held back by the backends' baseline, which
                // this harness does not model.
                self.scroll_y = (self.scroll_y + r.range(-3, 6) as f32 * 7.0).clamp(0.0, 300.0);
                format!("scrolled the scroll box to {}", self.scroll_y)
            }
        }
    }
}

fn window(dpi: u32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(W, H);
    ws.size.dpi = dpi;
    lw.current_window_state = ws;
    lw
}

/// Lay `scene` out as an app's rebuild does, and scroll its scroll box.
fn lay_out(lw: &mut LayoutWindow, scene: &Scene) {
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(scene.dom()),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let now = Instant::from(std::time::Instant::now());
    azul_layout::managers::scroll_registration::register_scroll_nodes(lw, &now);
    // html > body > scroller: the scroll box is node 2.
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        NodeId::new(2),
        LogicalPosition::new(0.0, scene.scroll_y),
        now,
    );
    let _ = lw.prepare_frame_cpu();
}

fn dpi_factor(lw: &LayoutWindow) -> f32 {
    lw.current_window_state.size.dpi as f32 / 96.0
}

fn pixel_size(lw: &LayoutWindow) -> (u32, u32) {
    let s = dpi_factor(lw);
    ((W * s).ceil() as u32, (H * s).ceil() as u32)
}

fn list(lw: &LayoutWindow) -> Arc<DisplayList> {
    lw.layout_results[&DomId::ROOT_ID].display_list.clone()
}

fn offsets(lw: &LayoutWindow) -> ScrollOffsetMap {
    let result = &lw.layout_results[&DomId::ROOT_ID];
    lw.scroll_manager
        .build_scroll_offset_map(DomId::ROOT_ID, &result.scroll_id_to_node_id)
}

fn state(lw: &LayoutWindow, offsets: &ScrollOffsetMap) -> CpuRenderState {
    let mut state = CpuRenderState::from_gpu_cache(
        lw.gpu_state_manager.get_cache(DomId::ROOT_ID),
        DomId::ROOT_ID,
        offsets,
    );
    // The views' documents, as a window composites them into its frame.
    state.virtual_view_display_lists = lw
        .layout_results
        .iter()
        .filter(|(id, _)| **id != DomId::ROOT_ID)
        .map(|(id, r)| (*id, r.display_list.clone()))
        .collect();
    state
}

/// The frame painted in full through the layered compositor, as a backend's
/// full repaint does.
fn full_redraw(lw: &LayoutWindow) -> AzulPixmap {
    let dl = list(lw);
    let (pw, ph) = pixel_size(lw);
    let s = dpi_factor(lw);
    let render_state = state(lw, &offsets(lw));
    let mut glyphs = GlyphCache::new();
    let mut compositor = cpurender::CompositorState::new(pw, ph);
    compositor.allocate_layers_from_display_list(
        &dl,
        s,
        &render_state.transforms,
        &render_state.opacities,
    );
    compositor
        .render_layers(
            &dl,
            s,
            &RendererResources::default(),
            &lw.font_manager,
            &mut glyphs,
            &render_state,
        )
        .expect("the layers render");
    let mut out = AzulPixmap::new(pw, ph).expect("a pixmap");
    out.fill(255, 255, 255, 255);
    compositor.composite_frame(&mut out, s);
    out
}

/// What the backends carry from one frame to the next.
struct Presented {
    pixels: AzulPixmap,
    list: Arc<DisplayList>,
    offsets: ScrollOffsetMap,
    transforms: std::collections::HashMap<usize, azul_core::transform::ComputedTransform3D>,
    opacities: std::collections::HashMap<usize, f32>,
    colors: cpurender::GpuColors,
    glyphs: GlyphCache,
    /// Frames presented with a move of pixels (`execute_translate_blit`).
    blits: usize,
}

impl Presented {
    fn first(lw: &LayoutWindow) -> Self {
        let cache = lw.gpu_state_manager.get_cache(DomId::ROOT_ID);
        let (transforms, opacities) = cpurender::extract_gpu_values(cache, DomId::ROOT_ID);
        Self {
            pixels: full_redraw(lw),
            list: list(lw),
            offsets: offsets(lw),
            transforms,
            opacities,
            colors: cpurender::extract_gpu_colors(cache),
            glyphs: GlyphCache::new(),
            blits: 0,
        }
    }
}

/// What one incremental frame did.
struct Frame {
    /// `None`: the frame fell back to a full repaint.
    painted_px: Option<f64>,
    /// The rects the frame repainted (empty for a full repaint).
    damage: Vec<azul_core::geom::LogicalRect>,
}

/// The next frame, presented the way the backends present it
/// (`CpuBackend::render_frame`): the damage of the list diff and of the
/// values that moved without one, the scroll moves, then only the damage
/// repainted over the frame before.
fn present(lw: &LayoutWindow, last: &mut Presented) -> Frame {
    let dl = list(lw);
    let new_offsets = offsets(lw);
    let s = dpi_factor(lw);
    let cache = lw.gpu_state_manager.get_cache(DomId::ROOT_ID);
    let (transforms, opacities) = cpurender::extract_gpu_values(cache, DomId::ROOT_ID);
    let colors = cpurender::extract_gpu_colors(cache);
    let mut gpu = cpurender::gpu_value_damage(
        &dl,
        &last.transforms,
        &last.opacities,
        &transforms,
        &opacities,
        &new_offsets,
    );
    let colour = cpurender::color_value_damage(&dl, &last.colors, &colors, &transforms, &new_offsets);
    gpu.rects.extend(colour.rects);
    gpu.needs_full |= colour.needs_full;
    // The layout pass's move hint: the movers it found, blitted where the
    // diff finds their items translated by the dominant delta.
    let hint = cpurender::translate_hint_for_patch(
        lw.layout_cache.last_patch_move.as_ref(),
        s,
        true,
        false,
        lw.scroll_manager.any_nonzero_offset(),
    );
    let mut moved_union = None;
    let diff = if Arc::ptr_eq(&dl, &last.list) {
        Some(Vec::new())
    } else {
        cpurender::compute_display_list_damage_translated(
            &last.list,
            &dl,
            &last.offsets,
            &new_offsets,
            hint.as_ref().map(|(h, _)| h),
        )
        .map(|(damage, moved)| {
            moved_union = moved;
            damage
        })
    };
    let frame = match diff {
        Some(mut damage) if !gpu.needs_full => {
            damage.extend(gpu.rects);
            if let (Some((h, exceptions)), Some(_)) = (hint.as_ref(), moved_union) {
                let movers = lw
                    .layout_cache
                    .last_patch_move
                    .as_ref()
                    .map(|m| m.mover_rects_old.clone())
                    .unwrap_or_default();
                let blit =
                    cpurender::execute_translate_blit(&mut last.pixels, h, exceptions, &movers, &dl, s);
                damage.extend(blit.damage);
                last.blits += 1;
            }
            for (scroll_id, clip, delta, offset) in
                cpurender::collect_scroll_shifts(&dl, &new_offsets, &last.offsets, s)
            {
                let out = cpurender::execute_scroll_shift(
                    &mut last.pixels,
                    &dl,
                    scroll_id,
                    &clip,
                    delta,
                    offset,
                    s,
                    &new_offsets,
                );
                damage.extend(out.damage);
            }
            cpurender::coalesce_damage_rects(&mut damage);
            let painted = cpurender::render_display_list_damaged(
                &dl,
                &mut last.pixels,
                s,
                &RendererResources::default(),
                &lw.font_manager,
                &mut last.glyphs,
                &state(lw, &new_offsets),
                &damage,
            )
            .expect("the damage repaints");
            let px: f64 = painted
                .iter()
                .map(|r| f64::from(r.size.width) * f64::from(r.size.height))
                .sum::<f64>()
                * f64::from(s)
                * f64::from(s);
            Frame {
                painted_px: Some(px),
                damage: painted,
            }
        }
        _ => {
            last.pixels = full_redraw(lw);
            Frame {
                painted_px: None,
                damage: Vec::new(),
            }
        }
    };
    last.list = dl;
    last.offsets = new_offsets;
    last.transforms = transforms;
    last.opacities = opacities;
    last.colors = colors;
    frame
}

/// The first pixel where `a` and `b` differ, with both values.
fn first_difference(a: &AzulPixmap, b: &AzulPixmap) -> Option<(u32, u32, [u8; 4], [u8; 4])> {
    let w = a.width();
    a.data()
        .chunks_exact(4)
        .zip(b.data().chunks_exact(4))
        .enumerate()
        .find(|(_, (p, q))| p != q)
        .map(|(i, (p, q))| {
            let i = i as u32;
            (i % w, i / w, [p[0], p[1], p[2], p[3]], [q[0], q[1], q[2], q[3]])
        })
}

fn differing_pixels(a: &AzulPixmap, b: &AzulPixmap) -> usize {
    a.data()
        .chunks_exact(4)
        .zip(b.data().chunks_exact(4))
        .filter(|(p, q)| p != q)
        .count()
}

fn run(seed: u64, dpi: u32, frames: usize) {
    let mut r = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let mut scene = Scene::random(&mut r);
    let mut lw = window(dpi);
    lay_out(&mut lw, &scene);
    let mut last = Presented::first(&lw);
    let mut history: Vec<String> = Vec::new();
    for frame in 0..frames {
        let what = scene.mutate(&mut r);
        history.push(what.clone());
        lay_out(&mut lw, &scene);
        let presented = present(&lw, &mut last);
        let truth = full_redraw(&lw);
        if let Some((x, y, painted, full)) = first_difference(&last.pixels, &truth) {
            let recent: Vec<&String> = history.iter().rev().take(4).collect();
            panic!(
                "seed {seed} at {dpi} dpi, frame {frame} ({what}): the incremental frame differs \
                 from a full redraw in {} pixels, the first at ({x}, {y}): painted {painted:?}, \
                 full redraw {full:?}; the last changes, newest first: {recent:?}; the frame \
                 repainted {:?} (None: in full)",
                differing_pixels(&last.pixels, &truth),
                presented.painted_px.map(|_| presented.damage.clone())
            );
        }

        // The same scene laid out again changes nothing: the frame
        // rasterises no pixel and stays the frame it was.
        lay_out(&mut lw, &scene);
        let idle = present(&lw, &mut last);
        assert_eq!(
            idle.painted_px,
            Some(0.0),
            "seed {seed} at {dpi} dpi, frame {frame} ({what}): a layout of the unchanged scene \
             repainted {:?} px (None: in full)",
            idle.painted_px
        );
        assert!(
            first_difference(&last.pixels, &truth).is_none(),
            "seed {seed} at {dpi} dpi, frame {frame}: the idle frame changed pixels"
        );
    }
}

#[test]
fn random_frames_at_1x_are_painted_as_a_full_redraw_paints_them() {
    for seed in 1..=6 {
        run(seed, 96, 40);
    }
}

#[test]
fn random_frames_at_2x_are_painted_as_a_full_redraw_paints_them() {
    for seed in 101..=104 {
        run(seed, 192, 30);
    }
}

/// One change of one kind on a fixed page: the frame is the full redraw's
/// and repaints a region of the size of the change, not the window.
fn one_change(change: impl Fn(&mut Scene), budget_px: f64, what: &str) {
    let base = Scene {
        shapes: vec![
            Shape {
                x: 20,
                y: 20,
                w: 60,
                h: 40,
                rgba: [200, 30, 30, 255],
                radius: 0,
                border: false,
                shadow: false,
                opacity: None,
                translate: None,
                rotate: false,
                clips_child: false,
            },
            Shape {
                x: 60,
                y: 40,
                w: 50,
                h: 50,
                rgba: [30, 30, 200, 160],
                radius: 8,
                border: true,
                shadow: true,
                opacity: Some(70),
                translate: Some((10, 5)),
                rotate: false,
                clips_child: true,
            },
            Shape {
                x: 120,
                y: 150,
                w: 40,
                h: 40,
                rgba: [30, 160, 30, 255],
                radius: 0,
                border: false,
                shadow: false,
                opacity: None,
                translate: None,
                rotate: false,
                clips_child: false,
            },
        ],
        scroll_y: 14.0,
        panel: false,
    };
    for dpi in [96, 192] {
        let mut lw = window(dpi);
        lay_out(&mut lw, &base);
        let mut last = Presented::first(&lw);
        let mut scene = base.clone();
        change(&mut scene);
        lay_out(&mut lw, &scene);
        let frame = present(&lw, &mut last);
        let truth = full_redraw(&lw);
        if let Some((x, y, painted, full)) = first_difference(&last.pixels, &truth) {
            // `AZ_HARNESS_DUMP=1`: the list of the failing frame, item by item.
            if std::env::var_os("AZ_HARNESS_DUMP").is_some() {
                for (i, item) in list(&lw).items.iter().enumerate() {
                    eprintln!("[harness] {i:3} {}", format!("{item:?}").chars().take(160).collect::<String>());
                }
            }
            panic!(
                "{what} at {dpi} dpi: the incremental frame differs from a full redraw in {} \
                 pixels, the first at ({x}, {y}): painted {painted:?}, full {full:?}; the frame \
                 repainted {:?}",
                differing_pixels(&last.pixels, &truth),
                frame.damage
            );
        }
        let s = f64::from(dpi) / 96.0;
        let painted = frame
            .painted_px
            .unwrap_or_else(|| panic!("{what} at {dpi} dpi: the frame was repainted in full"));
        assert!(
            painted <= budget_px * s * s,
            "{what} at {dpi} dpi: repainted {painted} px, more than the change's {} px",
            budget_px * s * s
        );
    }
}

#[test]
fn a_moved_box_repaints_where_it_was_and_where_it_is() {
    // Old and new rects of a 60x40 box moved by (15, 10): 2 x 2400 px.
    one_change(|s| s.shapes[0].x += 15, 2.0 * 75.0 * 50.0, "a move");
}

#[test]
fn a_resized_box_repaints_its_old_and_new_extent() {
    one_change(|s| s.shapes[0].w += 20, 80.0 * 40.0, "a resize");
}

#[test]
fn a_recoloured_box_repaints_its_box() {
    one_change(|s| s.shapes[2].rgba = [10, 10, 10, 255], 40.0 * 40.0, "a recolour");
}

#[test]
fn a_scroll_step_repaints_the_strip_it_uncovers() {
    // The 110 px wide box moved by 14 px: a strip, plus its scroll bar.
    one_change(|s| s.scroll_y += 14.0, 110.0 * 40.0, "a scroll");
}

#[test]
fn a_box_under_an_overlapping_one_repaints_through_it() {
    one_change(|s| s.shapes[0].rgba = [250, 200, 0, 255], 60.0 * 40.0, "an overlapped recolour");
}

#[test]
fn a_clipped_child_repaints_inside_its_clip() {
    one_change(|s| s.shapes[1].w += 6, 2.0 * 90.0 * 90.0, "a nested clip");
}

#[test]
fn a_transformed_box_repaints_where_it_is_painted() {
    one_change(
        |s| s.shapes[1].translate = Some((22, -6)),
        2.0 * 90.0 * 90.0,
        "a transform",
    );
}

#[test]
fn an_opacity_change_repaints_its_group() {
    one_change(|s| s.shapes[1].opacity = Some(35), 2.0 * 90.0 * 90.0, "an opacity");
}

/// A rebuilt page that gains a scroll box (a panel that opens) - or loses
/// one - repaints the panel, not the window: the scroll box only one of the
/// two lists has is in the changed part of the list, at its own offset.
#[test]
fn a_page_that_gains_or_loses_a_scroll_box_repaints_only_the_box() {
    one_change(|s| s.panel = true, 2.0 * 160.0 * 80.0, "a panel opened");
}

/// A box whose height tweens over a column of rows (a ribbon's content
/// growing over the page) moves the rows by whole device pixels per frame,
/// and the CPU frame presents that as a move of their pixels: each frame
/// equals a full redraw and repaints far less than the rows it moved.
#[test]
fn a_height_tween_moves_what_it_pushes_by_a_blit() {
    height_tween_blits(true);
}

/// The same with rows that paint no background of their own over a white
/// list (a file list's rows): what is behind them is one flat colour where
/// they were and where they are, so they blit as well.
#[test]
fn a_height_tween_moves_rows_without_a_background_by_a_blit() {
    height_tween_blits(false);
}

fn height_tween_blits(rows_have_backgrounds: bool) {
    for dpi in [96u32, 192] {
        let mut rows = Dom::create_div()
            .with_css("display: flex; flex-direction: column; flex-grow: 1; background: #ffffff;");
        for k in 0..14 {
            let color = if k % 2 == 0 { "#e8eef8" } else { "#ffffff" };
            let background = if rows_have_backgrounds {
                format!("background: {color};")
            } else {
                String::new()
            };
            rows = rows.with_child(Dom::create_div().with_css(&format!(
                "height: 14px; {background} border-bottom: 1px solid #c0c8d8;"
            )));
        }
        let page = Dom::create_html().with_child(
            Dom::create_body()
                .with_css("margin: 0; display: flex; flex-direction: column; background: #ffffff;")
                .with_child(
                    Dom::create_div()
                        .with_class("grow".into())
                        .with_css("height: 10px; background: #446688; animation: height 260ms ease-out;"),
                )
                .with_child(rows),
        );
        let mut lw = window(dpi);
        let ws = lw.current_window_state.clone();
        lw.layout_and_generate_display_list(
            StyledDom::create_from_dom(page),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let _ = lw.prepare_frame_cpu();
        let mut last = Presented::first(&lw);
        let grow = NodeId::new(2);
        let _ = lw.apply_content_change(azul_layout::overlay::ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: grow,
            props: vec![azul_css::props::property::CssProperty::const_height(
                azul_css::props::layout::LayoutHeight::const_px(70),
            )],
            override_only: false,
        });
        assert_eq!(lw.css_transitions.len(), 1, "harness: the height tweens");
        let s = dpi_factor(&lw);
        let mut frames = 0;
        let mut painted_total = 0.0f64;
        while !lw.css_transitions.is_empty() && frames < 60 {
            frames += 1;
            let _ = lw.tick_animations(0.016);
            if lw.take_transition_relayout() {
                let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
                let ws = lw.current_window_state.clone();
                lw.layout_and_generate_display_list(
                    result.styled_dom,
                    &ws,
                    &RendererResources::default(),
                    &ExternalSystemCallbacks::rust_internal(),
                    &mut None,
                )
                .expect("the frame lays out");
            }
            let _ = lw.take_animation_frame_work();
            let _ = lw.prepare_frame_cpu();
            let height = lw.layout_results[&DomId::ROOT_ID]
                .layout_tree
                .dom_to_layout
                .get(&grow)
                .and_then(|l| l.first())
                .and_then(|l| lw.layout_results[&DomId::ROOT_ID].layout_tree.get(*l))
                .and_then(|n| n.used_size)
                .expect("the box is laid out")
                .height;
            assert!(
                ((height * s) - (height * s).round()).abs() < 1e-3,
                "{dpi} dpi, frame {frames}: the tween's height {height} is off the device pixel grid"
            );
            let frame = present(&lw, &mut last);
            let truth = full_redraw(&lw);
            assert!(
                first_difference(&last.pixels, &truth).is_none(),
                "{dpi} dpi, frame {frames}: the frame differs from a full redraw in {} pixels",
                differing_pixels(&last.pixels, &truth)
            );
            eprintln!(
                "[height tween] {dpi} dpi frame {frames}: height {height}, painted {:?} px, blits {}",
                frame.painted_px, last.blits
            );
            painted_total += frame.painted_px.unwrap_or(f64::from(W * H) * f64::from(s * s));
        }
        assert!(frames >= 8, "harness: the tween ran {frames} frames");
        assert!(
            last.blits * 2 >= frames,
            "{dpi} dpi: {} of {frames} frames moved the rows by a blit",
            last.blits
        );
        // The rows are 14 x 15 px tall over the window's width: what a
        // frame without a move repaints. With the move a frame repaints the
        // box's new strip and the exposed rows.
        let rows_area = f64::from(W) * 15.0 * 14.0 * f64::from(s * s);
        assert!(
            painted_total / (frames as f64) < rows_area / 3.0,
            "{dpi} dpi: a frame repainted {} px on average, the moved rows are {rows_area} px",
            painted_total / (frames as f64)
        );
    }
}

/// A flat box whose top edge follows a growing neighbour (a pane under a
/// ribbon that opens) repaints the strip its edge left, not its whole box.
#[test]
fn a_box_moved_and_shortened_repaints_only_its_edge_strips() {
    one_change(
        |s| {
            s.shapes[2].y += 6;
            s.shapes[2].h -= 6;
        },
        40.0 * 8.0,
        "a moved edge",
    );
}

/// The rows a list view shows: 40 rows of 14 px, as wide as its box.
extern "C" fn list_rows(
    _data: azul_core::refany::RefAny,
    info: azul_core::callbacks::VirtualViewCallbackInfo,
) -> azul_core::callbacks::VirtualViewReturn {
    use azul_core::geom::LogicalRect;
    let width = info.bounds.logical_size.width;
    let mut rows = Dom::create_div().with_css(&format!("width: {width}px;"));
    for k in 0..40 {
        let color = if k % 2 == 0 { "#e8f4e8" } else { "#ffffff" };
        rows = rows.with_child(
            Dom::create_div().with_css(&format!("height: 14px; background: {color};")),
        );
    }
    let size = LogicalSize::new(width, 40.0 * 14.0);
    azul_core::callbacks::VirtualViewReturn::with_dom(
        rows,
        LogicalRect::new(LogicalPosition::zero(), size),
        LogicalRect::new(LogicalPosition::zero(), size),
    )
}

/// What a ribbon opening over a file browser pushes down: a bordered side
/// pane on a left-to-right gradient that scrolls its own rows, and a list
/// view, both shortened as the bar above them grows. Each frame equals a full
/// redraw and repaints the bar's new strip and the panes' edges, not the
/// panes: their pixels move by a blit (a frame repainted the whole window).
#[test]
fn a_height_tween_moves_bordered_panes_and_a_list_view_by_a_blit() {
    tween_the_bar_over_the_panes(10, 70);
}

/// The ribbon closing again: the panes move up and the list view grows into
/// the room the bar gave up. The grown view rides its pane, so the blit moves
/// its pixels and only its new band repaints (each such frame repainted the
/// whole list, and the many rects of a frame then merged into the window).
#[test]
fn a_closing_bar_moves_the_growing_list_view_by_a_blit() {
    tween_the_bar_over_the_panes(70, 10);
}

/// Tweens the bar above the panes from `from` to `to` px: every frame equals a
/// full redraw and repaints less than a quarter of the panes on average.
fn tween_the_bar_over_the_panes(from: isize, to: isize) {
    for dpi in [96u32, 192] {
        let mut side = Dom::create_div().with_css(
            "width: 100px; border: 1px solid #8899aa; overflow-y: auto; \
             background: linear-gradient(to right, #f4f4f4, #d4d8dc);",
        );
        for k in 0..20 {
            let color = if k % 2 == 0 { "#e8eef8" } else { "#ffffff" };
            side = side.with_child(
                Dom::create_div().with_css(&format!("height: 14px; background: {color};")),
            );
        }
        let list = Dom::create_virtual_view(
            azul_core::refany::RefAny::new(()),
            azul_core::callbacks::VirtualViewCallback::create(list_rows),
        )
        .with_css("flex-grow: 1; background: #ffffff; overflow-y: auto;");
        let page = Dom::create_html().with_child(
            Dom::create_body()
                .with_css("margin: 0; display: flex; flex-direction: column; background: #ffffff;")
                .with_child(
                    Dom::create_div()
                        .with_class("grow".into())
                        .with_css(&format!(
                            "height: {from}px; background: #446688; animation: height 260ms ease-out;"
                        )),
                )
                .with_child(
                    Dom::create_div()
                        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0;")
                        .with_child(side)
                        .with_child(list),
                ),
        );
        let mut lw = window(dpi);
        let ws = lw.current_window_state.clone();
        lw.layout_and_generate_display_list(
            StyledDom::create_from_dom(page),
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the page lays out");
        let _ = lw.prepare_frame_cpu();
        let mut last = Presented::first(&lw);
        let grow = NodeId::new(2);
        let _ = lw.apply_content_change(azul_layout::overlay::ContentChange::NodeCss {
            dom_id: DomId::ROOT_ID,
            node_id: grow,
            props: vec![azul_css::props::property::CssProperty::const_height(
                azul_css::props::layout::LayoutHeight::const_px(to),
            )],
            override_only: false,
        });
        assert_eq!(lw.css_transitions.len(), 1, "harness: the height tweens");
        let s = dpi_factor(&lw);
        let mut frames = 0;
        let mut painted_total = 0.0f64;
        while !lw.css_transitions.is_empty() && frames < 60 {
            frames += 1;
            let _ = lw.tick_animations(0.016);
            if lw.take_transition_relayout() {
                let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
                let ws = lw.current_window_state.clone();
                lw.layout_and_generate_display_list(
                    result.styled_dom,
                    &ws,
                    &RendererResources::default(),
                    &ExternalSystemCallbacks::rust_internal(),
                    &mut None,
                )
                .expect("the frame lays out");
            }
            let _ = lw.take_animation_frame_work();
            let _ = lw.prepare_frame_cpu();
            let frame = present(&lw, &mut last);
            let truth = full_redraw(&lw);
            assert!(
                first_difference(&last.pixels, &truth).is_none(),
                "{dpi} dpi, frame {frames}: the frame differs from a full redraw in {} pixels \
                 (first {:?})",
                differing_pixels(&last.pixels, &truth),
                first_difference(&last.pixels, &truth)
            );
            painted_total += frame.painted_px.unwrap_or(f64::from(W * H) * f64::from(s * s));
        }
        assert!(frames >= 8, "harness: the tween ran {frames} frames");
        // The panes below the bar fill the window under it: what a frame
        // repaints that does not move them.
        let panes_area = f64::from(W) * f64::from(H - 70.0) * f64::from(s * s);
        assert!(
            painted_total / (frames as f64) < panes_area / 4.0,
            "{dpi} dpi: a frame repainted {} px on average, the panes are {panes_area} px",
            painted_total / (frames as f64)
        );
    }
}

/// A patched frame whose list pairs item for item with the list before it
/// (every item the same node's, at the same index) logs no node boxes as its
/// damage: the renderers' item diff is exact for it, and the boxes of all the
/// frame moved buried that under the whole page (each frame of a ribbon
/// opening over a file browser repainted the window).
#[test]
fn a_patched_frame_whose_list_pairs_logs_no_node_boxes() {
    let mut rows = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; background: #ffffff;");
    for k in 0..14 {
        let color = if k % 2 == 0 { "#e8eef8" } else { "#ffffff" };
        rows = rows.with_child(
            Dom::create_div().with_css(&format!("height: 14px; background: {color};")),
        );
    }
    let page = Dom::create_html().with_child(
        Dom::create_body()
            .with_css("margin: 0; display: flex; flex-direction: column; background: #ffffff;")
            .with_child(
                Dom::create_div()
                    .with_css("height: 10px; background: #446688; animation: height 260ms ease-out;"),
            )
            .with_child(rows),
    );
    let mut lw = window(96);
    let ws = lw.current_window_state.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let _ = lw.apply_content_change(azul_layout::overlay::ContentChange::NodeCss {
        dom_id: DomId::ROOT_ID,
        node_id: NodeId::new(2),
        props: vec![azul_css::props::property::CssProperty::const_height(
            azul_css::props::layout::LayoutHeight::const_px(70),
        )],
        override_only: false,
    });
    let mut patched = 0;
    let mut frames = 0;
    while !lw.css_transitions.is_empty() && frames < 60 {
        frames += 1;
        let _ = lw.tick_animations(0.016);
        if lw.take_transition_relayout() {
            let previous = list(&lw);
            let result = lw.layout_results.remove(&DomId::ROOT_ID).expect("laid out");
            let ws = lw.current_window_state.clone();
            lw.layout_and_generate_display_list(
                result.styled_dom,
                &ws,
                &RendererResources::default(),
                &ExternalSystemCallbacks::rust_internal(),
                &mut None,
            )
            .expect("the frame lays out");
            let now = list(&lw);
            let pairs = previous.items.len() == now.items.len()
                && previous.layout_node_mapping == now.layout_node_mapping;
            if lw.layout_cache.last_build_was_patched && pairs {
                patched += 1;
                assert_eq!(
                    lw.layout_cache.last_patch_damage.as_deref(),
                    Some(&[][..]),
                    "frame {frames}: the list pairs with the one before it - the diff decides"
                );
            }
        }
        let _ = lw.take_animation_frame_work();
    }
    assert!(patched >= 4, "harness: {patched} paired patched frames of {frames}");
}
