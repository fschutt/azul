//! A scroll box keeps its scroll BLIT on a scrolled page, and so does a page
//! with a fixed box on it.
//!
//! The CPU backends present a scroll step by moving the pixels that are
//! still visible (`cpurender::execute_scroll_shift`) and repainting only
//! the strip that scrolled into view, plus whatever is painted OVER the
//! frame (its scrollbar, a fixed header) at its place and where the move
//! dragged it. Two frames lost that path and repainted far more than the
//! step changed:
//!
//! - a scroll box on a PAGE that is scrolled: `scroll_fast_path_eligible`
//!   gave up on every frame nested in a scrolled one ("the content around
//!   it is placed by an offset this check does not know") and repainted
//!   its whole clip - every scroll box on a page taller than its window,
//!   once the page had moved at all;
//! - a SPLIT frame: a fixed box painted between two pushes of the page's
//!   frame (`DisplayListGenerator::enter_scroll_chain`). The repaint list
//!   took everything after the first half as an overlay, so the whole
//!   second half of the page - which the move had already put in place -
//!   was repainted on every step.
//!
//! Each test builds the frame the backends would present - a full render at
//! the old offsets, the shared shift recipe, a damage-only repaint at the
//! new ones - and asserts it (a) looks exactly like a full render at the
//! new offsets and (b) repainted less than half of the scrolled clip.
//! (a) holds with and without the blit; (b) is the blit.

use azul_core::{
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
};
use azul_css::props::basic::{ColorU, FontRef};
use azul_layout::{
    cpurender::{self, AzulPixmap, CpuRenderState, RenderOptions, ScrollOffsetMap},
    glyph_cache::GlyphCache,
    solver3::display_list::{BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect},
    text3::cache::FontManager,
};
use rust_fontconfig::FcFontCache;

pub(crate) const W: u32 = 200;
pub(crate) const H: u32 = 200;

const PAGE: u64 = 1;
const BOX: u64 = 2;

pub(crate) fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
}

pub(crate) const fn rgb(r: u8, g: u8, b: u8) -> ColorU {
    ColorU { r, g, b, a: 255 }
}

pub(crate) fn fill(r: LogicalRect, color: ColorU) -> DisplayListItem {
    DisplayListItem::Rect {
        bounds: WindowLogicalRect(r),
        color,
        border_radius: BorderRadius::default(),
    }
}

fn push(scroll_id: u64, clip: LogicalRect, content: LogicalSize) -> DisplayListItem {
    DisplayListItem::PushScrollFrame {
        clip_bounds: WindowLogicalRect(clip),
        content_size: content,
        scroll_id,
    }
}

/// A scroll container as the display-list generator emits it: its CLIP,
/// then its scroll frame (`push_node_clips`). The raster's scroll frame is
/// an offset only - it relies on this clip - so a frame pushed without it
/// paints its content unclipped, which no layout ever produces.
fn open_frame(scroll_id: u64, clip: LogicalRect, content: LogicalSize) -> [DisplayListItem; 2] {
    [
        DisplayListItem::PushClip {
            bounds: WindowLogicalRect(clip),
            border_radius: BorderRadius::default(),
        },
        push(scroll_id, clip, content),
    ]
}

/// The end of an [`open_frame`]: the scroll frame, then its clip.
fn close_frame() -> [DisplayListItem; 2] {
    [DisplayListItem::PopScrollFrame, DisplayListItem::PopClip]
}

/// `n` opaque stripes `h` px tall, alternating two colours, from `(x, y)`:
/// content whose every row differs from the next, so a move by the wrong
/// amount - or none - shows.
fn stripes(x: f32, y: f32, w: f32, h: f32, n: usize) -> Vec<DisplayListItem> {
    (0..n)
        .map(|k| {
            let color = if k % 2 == 0 {
                rgb(220, 40, 40)
            } else {
                rgb(40, 160, 60)
            };
            fill(rect(x, y + h * k as f32, w, h), color)
        })
        .collect()
}

fn offsets(pairs: &[(u64, (f32, f32))]) -> ScrollOffsetMap {
    pairs.iter().copied().collect()
}

pub(crate) struct Raster {
    fonts: FontManager<FontRef>,
    resources: RendererResources,
    glyphs: GlyphCache,
}

impl Raster {
    pub(crate) fn new() -> Self {
        Self {
            // No text in these lists: an empty manager is the honest input.
            fonts: FontManager::new(FcFontCache::default()).expect("FontManager::new"),
            resources: RendererResources::default(),
            glyphs: GlyphCache::new(),
        }
    }

    /// The frame a full repaint draws at `at`.
    pub(crate) fn full(&mut self, dl: &DisplayList, at: &ScrollOffsetMap) -> AzulPixmap {
        cpurender::render_with_font_manager_and_scroll(
            dl,
            &self.resources,
            &self.fonts,
            RenderOptions {
                width: W as f32,
                height: H as f32,
                dpi_factor: 1.0,
            },
            &mut self.glyphs,
            &CpuRenderState::new(at.clone()),
        )
        .expect("the list renders")
    }

    /// The frame the CPU backends present for a scroll from `before` to
    /// `after` (`headless/mod.rs render_frame` and the e2e twin): the old
    /// frame, the shared collector and shift recipe, then a repaint of the
    /// damage only. Returns the frame and its damage.
    fn scrolled(
        &mut self,
        dl: &DisplayList,
        before: &ScrollOffsetMap,
        after: &ScrollOffsetMap,
    ) -> (AzulPixmap, Vec<LogicalRect>) {
        let mut frame = self.full(dl, before);
        let mut damage = Vec::new();
        for (scroll_id, clip, delta, offset) in
            cpurender::collect_scroll_shifts(dl, after, before, 1.0)
        {
            let out = cpurender::execute_scroll_shift(
                &mut frame, dl, scroll_id, &clip, delta, offset, 1.0, after,
            );
            damage.extend(out.damage);
        }
        self.repaint(dl, &mut frame, after, &damage);
        (frame, damage)
    }

    /// Repaint `damage` of `frame` from `dl` at `at`, the backends'
    /// damage-only raster (`render_display_list_damaged`).
    pub(crate) fn repaint(
        &mut self,
        dl: &DisplayList,
        frame: &mut AzulPixmap,
        at: &ScrollOffsetMap,
        damage: &[LogicalRect],
    ) {
        cpurender::render_display_list_damaged(
            dl,
            frame,
            1.0,
            &self.resources,
            &self.fonts,
            &mut self.glyphs,
            &CpuRenderState::new(at.clone()),
            damage,
        )
        .expect("the damage repaints");
    }
}

/// How many window pixels `damage` repaints (overlaps counted once).
fn damaged_pixels(damage: &[LogicalRect]) -> usize {
    let mut hit = vec![false; (W * H) as usize];
    for r in damage {
        let x0 = r.origin.x.floor().max(0.0) as u32;
        let y0 = r.origin.y.floor().max(0.0) as u32;
        let x1 = ((r.origin.x + r.size.width).ceil().max(0.0) as u32).min(W);
        let y1 = ((r.origin.y + r.size.height).ceil().max(0.0) as u32).min(H);
        for y in y0..y1 {
            for x in x0..x1 {
                hit[(y * W + x) as usize] = true;
            }
        }
    }
    hit.iter().filter(|h| **h).count()
}

/// The first pixel where two frames differ.
pub(crate) fn first_difference(a: &AzulPixmap, b: &AzulPixmap) -> Option<(u32, u32, [u8; 4], [u8; 4])> {
    let (da, db) = (a.data(), b.data());
    for y in 0..H {
        for x in 0..W {
            let i = ((y * W + x) * 4) as usize;
            let pa = [da[i], da[i + 1], da[i + 2], da[i + 3]];
            let pb = [db[i], db[i + 1], db[i + 2], db[i + 3]];
            if pa != pb {
                return Some((x, y, pa, pb));
            }
        }
    }
    None
}

/// A 120x100 scroll box of stripes, 150px down a 1000px page scrolled by
/// 100px (so on screen at y=50), with its bar painted over its right edge
/// after its frame - the order `paint_scrollbars` emits. The box scrolls by
/// 10px while the page stays where it is.
#[test]
fn a_scroll_box_on_a_scrolled_page_keeps_its_blit() {
    let box_clip = rect(20.0, 150.0, 120.0, 100.0);
    // Behind everything: what the window shows outside the page.
    let mut items = vec![fill(rect(0.0, 0.0, 200.0, 200.0), rgb(128, 128, 128))];
    items.extend(open_frame(PAGE, rect(0.0, 0.0, 200.0, 200.0), LogicalSize::new(200.0, 1000.0)));
    items.push(fill(rect(0.0, 0.0, 200.0, 1000.0), rgb(250, 250, 250)));
    items.extend(open_frame(BOX, box_clip, LogicalSize::new(120.0, 600.0)));
    items.extend(stripes(20.0, 150.0, 120.0, 10.0, 60));
    items.extend(close_frame());
    // The box's own bar, over its content, inside the page.
    items.push(fill(rect(128.0, 150.0, 12.0, 100.0), rgb(30, 30, 30)));
    items.extend(close_frame());
    let dl = DisplayList {
        items,
        ..Default::default()
    };

    let before = offsets(&[(PAGE, (0.0, 100.0)), (BOX, (0.0, 0.0))]);
    let after = offsets(&[(PAGE, (0.0, 100.0)), (BOX, (0.0, 10.0))]);
    let mut raster = Raster::new();
    let (presented, damage) = raster.scrolled(&dl, &before, &after);
    let expected = raster.full(&dl, &after);

    assert_eq!(
        first_difference(&presented, &expected),
        None,
        "the presented frame must look like a full repaint at the new offsets (damage {damage:?})"
    );
    let clip_area = (box_clip.size.width * box_clip.size.height) as usize;
    let repainted = damaged_pixels(&damage);
    assert!(
        repainted * 2 <= clip_area,
        "a 10px step of a 120x100 box on a scrolled page must be a blit - the 10px strip that \
         scrolled in plus the bar over it - not a repaint of the box: {repainted} of its \
         {clip_area} px repainted (damage {damage:?})"
    );
}

/// A page split around a fixed header: the page's frame is closed for the
/// header and reopened for the positioned cards painted after it
/// (`enter_scroll_chain` / `leave_scroll_chain`). The page scrolls by 20px.
#[test]
fn a_page_with_a_fixed_header_keeps_its_blit() {
    let page_clip = rect(0.0, 0.0, 200.0, 200.0);
    let mut items = vec![
        fill(rect(0.0, 0.0, 200.0, 200.0), rgb(255, 255, 255)),
        push(PAGE, page_clip, LogicalSize::new(200.0, 1000.0)),
        fill(rect(0.0, 0.0, 200.0, 1000.0), rgb(250, 250, 250)),
        DisplayListItem::PopScrollFrame,
        // The fixed header: outside every frame, so it stays put.
        fill(rect(0.0, 0.0, 200.0, 30.0), rgb(30, 60, 200)),
        // The page again, for the cards painted after the header.
        push(PAGE, page_clip, LogicalSize::new(200.0, 1000.0)),
    ];
    items.extend(stripes(20.0, 30.0, 160.0, 20.0, 48));
    items.push(DisplayListItem::PopScrollFrame);
    let dl = DisplayList {
        items,
        ..Default::default()
    };

    let before = offsets(&[(PAGE, (0.0, 0.0))]);
    let after = offsets(&[(PAGE, (0.0, 20.0))]);
    let mut raster = Raster::new();
    let (presented, damage) = raster.scrolled(&dl, &before, &after);
    let expected = raster.full(&dl, &after);

    assert_eq!(
        first_difference(&presented, &expected),
        None,
        "the presented frame must look like a full repaint at the new offsets (damage {damage:?})"
    );
    let clip_area = (page_clip.size.width * page_clip.size.height) as usize;
    let repainted = damaged_pixels(&damage);
    assert!(
        repainted * 2 <= clip_area,
        "a 20px step of a page under a fixed header must be a blit - the strip that scrolled in \
         plus the header and its dragged copy - not a repaint of the cards the move already put \
         in place: {repainted} of {clip_area} px repainted (damage {damage:?})"
    );
}
