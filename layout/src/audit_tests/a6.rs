//! A6, input and hit-testing.
//!
//! The guards that need no private item: what the CPU rasteriser draws is what the hit tester
//! answers (A6-01 .. A6-03). The ones next to private items are in `headless.rs`
//! (`audit_a6_tests`), `event_determination.rs` and `managers/hover.rs`.

/// A box drawn by the CPU rasteriser is hit at the pixels it covers, and nowhere else: the same
/// display lists and the same values go to both, and every pixel of the frame is asked.
#[cfg(all(feature = "cpurender", feature = "text_layout"))]
mod drawn_is_hit {
    use std::collections::{BTreeMap, HashMap};

    use azul_core::{
        dom::{Dom, DomId, FormattingContext, NodeId},
        geom::{LogicalPosition, LogicalRect, LogicalSize},
        resources::{RendererResources, TransformKey},
        styled_dom::StyledDom,
        transform::ComputedTransform3D,
    };
    use azul_css::props::basic::ColorU;

    use crate::{
        cpurender::{
            render_with_font_manager_and_scroll, AzulPixmap, CpuRenderState, RenderOptions,
            ScrollOffsetMap,
        },
        glyph_cache::GlyphCache,
        headless::CpuHitTester,
        managers::gpu_state::GpuStateManager,
        solver3::{
            display_list::{BorderRadius, DisplayList, DisplayListItem, WindowLogicalRect},
            layout_tree::{LayoutNodeHot, LayoutTree},
        },
        text3::cache::FontManager,
        window::DomLayoutResult,
    };

    const HOST: DomId = DomId { inner: 0 };
    const CHILD: DomId = DomId { inner: 1 };
    /// The child's nodes: its picture root, and the label in it.
    const ROOT: usize = 0;
    const LABEL: usize = 1;

    const FRAME_W: u32 = 1024;
    const FRAME_H: u32 = 640;

    fn p(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition { x, y }
    }

    fn r(x: f32, y: f32, width: f32, height: f32) -> LogicalRect {
        LogicalRect {
            origin: p(x, y),
            size: LogicalSize { width, height },
        }
    }

    fn wlr(rect: LogicalRect) -> WindowLogicalRect {
        WindowLogicalRect::new(rect.origin, rect.size)
    }

    fn hot(node: usize, rect: LogicalRect, parent: Option<usize>) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: Default::default(),
            dom_node_id: Some(NodeId::new(node)),
            used_size: Some(rect.size),
            formatting_context: FormattingContext::default(),
            parent,
        }
    }

    fn layout(
        styled_dom: StyledDom,
        nodes: Vec<LayoutNodeHot>,
        calculated_positions: Vec<LogicalPosition>,
        display_list: std::sync::Arc<DisplayList>,
    ) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes,
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            }
            .into(),
            calculated_positions,
            viewport: LogicalRect::zero(),
            display_list,
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// `body` with `children` plain `div`s.
    fn styled(children: usize) -> StyledDom {
        let css = azul_css::parser2::new_from_str("").0;
        let divs: Vec<Dom> = (0..children).map(|_| Dom::create_div()).collect();
        let mut d = Dom::create_body().with_children(divs.into());
        StyledDom::create(&mut d, css)
    }

    /// `scale(k)` then `translate(tx, ty)` about the box's corner.
    fn scaled(k: f32, tx: f32, ty: f32) -> ComputedTransform3D {
        ComputedTransform3D {
            m: [
                [k, 0.0, 0.0, 0.0],
                [0.0, k, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [tx, ty, 0.0, 1.0],
            ],
        }
    }

    /// A turn by `degrees` about the point `(cx, cy)` of the box.
    fn turned(degrees: f32, cx: f32, cy: f32) -> ComputedTransform3D {
        let (s, c) = degrees.to_radians().sin_cos();
        ComputedTransform3D {
            m: [
                [c, s, 0.0, 0.0],
                [-s, c, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [cx - (cx * c - cy * s), cy - (cx * s + cy * c), 0.0, 1.0],
            ],
        }
    }

    /// One scene: a view at `view_box` showing a child whose root holds one red label box. The
    /// same lists and values are painted and hit-tested.
    struct Scene {
        results: BTreeMap<DomId, DomLayoutResult>,
        gpu: GpuStateManager,
        /// The frames' values by key id, for the rasteriser.
        values: HashMap<usize, ComputedTransform3D>,
    }

    /// `frames`: the matrix of the root's and of the label's reference frame (`None`: the node
    /// has no frame, and its list no `PushReferenceFrame`).
    fn scene(
        view_box: LogicalRect,
        content_offset: LogicalPosition,
        root_box: LogicalRect,
        label_box: LogicalRect,
        frames: [Option<ComputedTransform3D>; 2],
    ) -> Scene {
        let mut gpu = GpuStateManager::default();
        let mut values = HashMap::new();
        let mut items = Vec::new();
        let mut pops = 0;
        for (node, rect, matrix) in [(ROOT, root_box, frames[0]), (LABEL, label_box, frames[1])] {
            let Some(matrix) = matrix else {
                continue;
            };
            let key = TransformKey::unique();
            let cache = gpu.get_or_create_cache(CHILD);
            cache.css_transform_keys.insert(NodeId::new(node), key);
            cache
                .css_current_transform_values
                .insert(NodeId::new(node), matrix);
            values.insert(key.id, matrix);
            items.push(DisplayListItem::PushReferenceFrame {
                transform_key: key,
                // Not the live value: the rasteriser must take the one under the key.
                initial_transform: ComputedTransform3D::IDENTITY,
                bounds: wlr(rect),
            });
            pops += 1;
        }
        items.push(DisplayListItem::Rect {
            bounds: wlr(label_box),
            color: ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            border_radius: BorderRadius::default(),
        });
        for _ in 0..pops {
            items.push(DisplayListItem::PopReferenceFrame);
        }
        let child_list = std::sync::Arc::new(DisplayList {
            items,
            ..Default::default()
        });
        let host_list = std::sync::Arc::new(DisplayList {
            items: vec![DisplayListItem::VirtualView {
                child_dom_id: CHILD,
                bounds: wlr(view_box),
                clip_rect: wlr(view_box),
                content_offset,
            }],
            ..Default::default()
        });
        let mut results = BTreeMap::new();
        results.insert(HOST, layout(styled(0), Vec::new(), Vec::new(), host_list));
        results.insert(
            CHILD,
            layout(
                styled(1),
                vec![hot(ROOT, root_box, None), hot(LABEL, label_box, Some(ROOT))],
                vec![root_box.origin, label_box.origin],
                child_list,
            ),
        );
        Scene {
            results,
            gpu,
            values,
        }
    }

    fn paint(scene: &Scene) -> AzulPixmap {
        let mut state = CpuRenderState::new(ScrollOffsetMap::new()).with_virtual_view_display_lists(
            [(CHILD, scene.results[&CHILD].display_list.clone())]
                .into_iter()
                .collect(),
        );
        state.transforms = scene.values.clone();
        let fonts =
            FontManager::new(rust_fontconfig::FcFontCache::default()).expect("FontManager::new");
        render_with_font_manager_and_scroll(
            &scene.results[&HOST].display_list,
            &RendererResources::default(),
            &fonts,
            RenderOptions {
                width: FRAME_W as f32,
                height: FRAME_H as f32,
                dpi_factor: 1.0,
            },
            &mut GlyphCache::new(),
            &state,
        )
        .expect("the frame paints")
    }

    /// The frame, pixel by pixel: one the label covers (red) is a hit of the label at its
    /// centre, one it does not touch (white, and white all around: a resampled corner fades
    /// over a pixel) is not; a pixel on its edge (blended) is not asked. Every pixel in and
    /// around the label is asked, the rest of the frame on a 5 px lattice. Returns how many
    /// pixels the label covers.
    fn every_drawn_pixel_is_hit(scene: &Scene, what: &str) -> usize {
        const DRAWN: u8 = 1;
        const EMPTY: u8 = 2;
        let frame = paint(scene);
        assert_eq!((frame.width, frame.height), (FRAME_W, FRAME_H));
        let data = frame.data();
        let kind: Vec<u8> = (0..(FRAME_W * FRAME_H) as usize)
            .map(|i| {
                let (red, green, blue) = (data[i * 4], data[i * 4 + 1], data[i * 4 + 2]);
                if red >= 250 && green <= 5 && blue <= 5 {
                    DRAWN
                } else if red >= 250 && green >= 250 && blue >= 250 {
                    EMPTY
                } else {
                    0
                }
            })
            .collect();
        let at = |x: u32, y: u32| kind[(y * FRAME_W + x) as usize];
        // Where the label is drawn.
        let (mut x0, mut y0, mut x1, mut y1) = (FRAME_W, FRAME_H, 0, 0);
        let mut covered = 0;
        for y in 0..FRAME_H {
            for x in 0..FRAME_W {
                if at(x, y) == DRAWN {
                    covered += 1;
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
            }
        }
        assert!(covered > 0, "{what}: the label is drawn somewhere");

        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&scene.results, Some(&scene.gpu));
        let gpu = &scene.gpu;
        for y in 0..FRAME_H {
            for x in 0..FRAME_W {
                let near = x + 12 >= x0 && x <= x1 + 12 && y + 12 >= y0 && y <= y1 + 12;
                if !near && (x % 5 != 0 || y % 5 != 0) {
                    continue;
                }
                let drawn = match at(x, y) {
                    DRAWN => true,
                    EMPTY => {
                        // White all around (the frame's edge counts as white).
                        let clear = (y.saturating_sub(1)..=(y + 1).min(FRAME_H - 1)).all(|ny| {
                            (x.saturating_sub(1)..=(x + 1).min(FRAME_W - 1))
                                .all(|nx| at(nx, ny) == EMPTY)
                        });
                        if !clear {
                            continue;
                        }
                        false
                    }
                    _ => continue, // an edge pixel
                };
                let hit = tester
                    .hit_test_scrolled(
                        LogicalPosition::new(x as f32 + 0.5, y as f32 + 0.5),
                        &|_, _| None,
                        &|d, n| gpu.painted_transform_of(d, n),
                    )
                    .iter()
                    .any(|(d, n, _)| *d == CHILD && *n == NodeId::new(LABEL));
                assert_eq!(
                    hit, drawn,
                    "{what}: pixel ({x}, {y}) is {} and the label is {} there",
                    if drawn { "drawn" } else { "empty" },
                    if hit { "hit" } else { "not hit" },
                );
            }
        }
        covered
    }

    /// The layout-rect path: no frame anywhere, the view's box and its content offset place the
    /// label. Child (300, 200) 80 x 16 at view (40, 100) + offset (-56, -36): window (284, 264).
    /// And cut by the view's top edge: with offset (-56, -208) it lies at window (284, 92), its
    /// upper 8 rows above the view's box at y = 100: they are neither drawn nor hit.
    /// Mutation: `content_offset` subtracted in `resolve_virtual_view_placements` moves the hit
    /// box 112 px right of the drawn one; the viewport clip dropped hits the 8 hidden rows.
    #[test]
    fn a_box_in_a_view_is_hit_at_the_pixels_it_covers() {
        let whole = scene(
            r(40.0, 100.0, 900.0, 500.0),
            p(-56.0, -36.0),
            r(0.0, 0.0, 1000.0, 600.0),
            r(300.0, 200.0, 80.0, 16.0),
            [None, None],
        );
        let covered = every_drawn_pixel_is_hit(&whole, "plain");
        assert_eq!(covered, 80 * 16, "the label is 80 x 16 whole pixels");

        let cut = scene(
            r(40.0, 100.0, 900.0, 500.0),
            p(-56.0, -208.0),
            r(0.0, 0.0, 1000.0, 600.0),
            r(300.0, 200.0, 80.0, 16.0),
            [None, None],
        );
        let covered = every_drawn_pixel_is_hit(&cut, "cut by the view");
        assert_eq!(covered, 80 * 8, "the rows 100..108 of the label show");
    }

    /// The transformed path, the editor's zoom: the root scaled 1.5 about the view point
    /// (615, 381) (`scale(1.5)` then `translate(-307.5, -190.5)`), the label at child (700, 300),
    /// 80 x 16, turned 30 degrees about its centre, the view at (40, 100) with content offset
    /// (-256, -156). Its centre is drawn at
    /// `(40, 100) + (-256, -156) + 1.5 (740, 308) + (-307.5, -190.5) = (586.5, 215.5)`,
    /// and it covers `80 x 16 x 1.5^2 = 2880` px, of which its resampled edge is blended.
    /// Mutation: a frame's matrix about the window's corner instead of its box's
    /// (`frame_to_local` without the origin) hits none of the drawn pixels.
    #[test]
    fn a_turned_label_under_a_zoomed_root_is_hit_at_the_pixels_it_covers() {
        let scene = scene(
            r(40.0, 100.0, 900.0, 500.0),
            p(-256.0, -156.0),
            r(0.0, 0.0, 1000.0, 600.0),
            r(700.0, 300.0, 80.0, 16.0),
            [
                Some(scaled(1.5, -307.5, -190.5)),
                Some(turned(30.0, 40.0, 8.0)),
            ],
        );
        let covered = every_drawn_pixel_is_hit(&scene, "zoomed and turned");
        assert!(
            (2000..=2880).contains(&covered),
            "the label covers 2880 px less its blended edge: {covered}"
        );
        // And its centre is where the hand computation puts it.
        let mut tester = CpuHitTester::new();
        tester.rebuild_from_layout_with_gpu(&scene.results, Some(&scene.gpu));
        let gpu = &scene.gpu;
        let front = tester.hit_test_scrolled(
            LogicalPosition::new(586.5, 215.5),
            &|_, _| None,
            &|d, n| gpu.painted_transform_of(d, n),
        );
        assert_eq!(
            front.first().map(|(d, n, _)| (*d, *n)),
            Some((CHILD, NodeId::new(LABEL)))
        );
    }

    /// The zoom alone (no frame on the label), and the turn alone (no zoom): each frame kind on
    /// its own, so a fault in one is not hidden by the other.
    #[test]
    fn each_frame_alone_is_hit_at_the_pixels_it_covers() {
        let zoomed = scene(
            r(40.0, 100.0, 900.0, 500.0),
            p(-256.0, -156.0),
            r(0.0, 0.0, 1000.0, 600.0),
            r(700.0, 300.0, 80.0, 16.0),
            [Some(scaled(1.5, -307.5, -190.5)), None],
        );
        // (40 - 256 + 1050 - 307.5, 100 - 156 + 450 - 190.5) = (526.5, 203.5), 120 x 24: the
        // columns 527..=645 and the rows 204..=226 are whole, 119 x 23 = 2737.
        let covered = every_drawn_pixel_is_hit(&zoomed, "zoomed");
        assert!((2600..=2880).contains(&covered), "120 x 24 px: {covered}");

        let turned_alone = scene(
            r(40.0, 100.0, 900.0, 500.0),
            p(-56.0, -36.0),
            r(0.0, 0.0, 1000.0, 600.0),
            r(300.0, 200.0, 80.0, 16.0),
            [None, Some(turned(-75.0, 0.0, 12.0))],
        );
        let covered = every_drawn_pixel_is_hit(&turned_alone, "turned");
        assert!((850..=1280).contains(&covered), "80 x 16 px turned: {covered}");
    }
}
