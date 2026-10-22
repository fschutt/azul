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
    geom::{LogicalPosition, LogicalRect, LogicalSize},
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
}

impl Scene {
    fn random(r: &mut Rng) -> Self {
        Self {
            shapes: (0..8).map(|_| Shape::random(r)).collect(),
            scroll_y: 0.0,
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
        Dom::create_html().with_child(body)
    }

    /// One random change. Returns what it did, for the failure message.
    fn mutate(&mut self, r: &mut Rng) -> String {
        let n = self.shapes.len();
        let i = r.below(n as u64) as usize;
        match r.below(11) {
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
    CpuRenderState::from_gpu_cache(
        lw.gpu_state_manager.get_cache(DomId::ROOT_ID),
        DomId::ROOT_ID,
        offsets,
    )
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
        }
    }
}

/// What one incremental frame did.
struct Frame {
    /// `None`: the frame fell back to a full repaint.
    painted_px: Option<f64>,
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
    let diff = if Arc::ptr_eq(&dl, &last.list) {
        Some(Vec::new())
    } else {
        cpurender::compute_display_list_damage(&last.list, &dl, &last.offsets, &new_offsets)
    };
    let frame = match diff {
        Some(mut damage) if !gpu.needs_full => {
            damage.extend(gpu.rects);
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
            }
        }
        _ => {
            last.pixels = full_redraw(lw);
            Frame { painted_px: None }
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
        let _ = present(&lw, &mut last);
        let truth = full_redraw(&lw);
        if let Some((x, y, painted, full)) = first_difference(&last.pixels, &truth) {
            let recent: Vec<&String> = history.iter().rev().take(4).collect();
            panic!(
                "seed {seed} at {dpi} dpi, frame {frame} ({what}): the incremental frame differs \
                 from a full redraw in {} pixels, the first at ({x}, {y}): painted {painted:?}, \
                 full redraw {full:?}; the last changes, newest first: {recent:?}",
                differing_pixels(&last.pixels, &truth)
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
        assert!(
            first_difference(&last.pixels, &truth).is_none(),
            "{what} at {dpi} dpi: the incremental frame differs from a full redraw in {} pixels",
            differing_pixels(&last.pixels, &truth)
        );
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
