    use azul_css::shape::{CssShape, ShapePath};

    use super::*;

    fn path_shape(d: &str) -> CssShape {
        CssShape::Path(ShapePath { data: d.into() })
    }

    // --- shape-outside: path() ----------------------------------------------

    #[test]
    fn css_path_shape_builds_path_boundary_not_rect_fallback() {
        // A right triangle (0,0)-(100,0)-(0,100).
        let shape = path_shape("M 0 0 L 100 0 L 0 100 Z");
        let rbox = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let boundary = ShapeBoundary::from_css_shape(&shape, rbox, &mut None);
        match boundary {
            ShapeBoundary::Path { segments } => {
                assert!(!segments.is_empty(), "path() must flatten to real segments");
                assert!(matches!(segments[0], PathSegment::MoveTo(_)));
                assert!(segments.iter().any(|s| matches!(s, PathSegment::Close)));
            }
            other => panic!("expected ShapeBoundary::Path, got {other:?}"),
        }
    }

    #[test]
    fn empty_or_garbage_path_falls_back_to_rectangle() {
        let rbox = Rect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
        };
        let boundary = ShapeBoundary::from_css_shape(&path_shape("   "), rbox, &mut None);
        assert!(
            matches!(boundary, ShapeBoundary::Rectangle(_)),
            "unparseable path() should fall back to the reference rectangle"
        );
    }

    #[test]
    fn path_triangle_narrows_line_box_per_scanline() {
        // Right triangle with the hypotenuse running (100,0) -> (0,100).
        // At scanline y, the shape spans x in [0, 100 - y]. So the available band
        // must NARROW as y increases — the proof that real path geometry (not a
        // full-width rect) drives the per-line exclusion.
        let shape = path_shape("M 0 0 L 100 0 L 0 100 Z");
        let rbox = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let boundary = ShapeBoundary::from_css_shape(&shape, rbox, &mut None);

        let spans_top = get_shape_horizontal_spans(&boundary, 10.0, 1.0);
        let spans_bot = get_shape_horizontal_spans(&boundary, 80.0, 1.0);

        assert_eq!(spans_top.len(), 1, "single span expected near the top");
        assert_eq!(spans_bot.len(), 1, "single span expected near the bottom");

        let width_top = spans_top[0].1 - spans_top[0].0;
        let width_bot = spans_bot[0].1 - spans_bot[0].0;

        // Geometry check: width ~= 100 - y (line center is y + 0.5).
        assert!(
            (width_top - 89.5).abs() < 1.5,
            "top width {width_top} != ~89.5"
        );
        assert!(
            (width_bot - 19.5).abs() < 1.5,
            "bottom width {width_bot} != ~19.5"
        );
        assert!(
            width_top > width_bot,
            "path() exclusion band must narrow with y ({width_top} !> {width_bot})"
        );

        // And it must differ from a plain full-width rectangle (which would be 0..100
        // at every scanline) — i.e. this is not the old rect/empty stub.
        assert!(width_bot < 50.0, "rect fallback would give full width here");
    }

    #[test]
    fn path_with_hole_carves_out_interior_via_even_odd() {
        // Outer square 0..100 with an inner reversed square 30..70 (a hole). At a
        // scanline through the hole, even-odd fill yields two spans straddling the hole.
        let shape =
            path_shape("M 0 0 L 100 0 L 100 100 L 0 100 Z M 30 30 L 30 70 L 70 70 L 70 30 Z");
        let rbox = Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let boundary = ShapeBoundary::from_css_shape(&shape, rbox, &mut None);
        let spans = get_shape_horizontal_spans(&boundary, 50.0, 1.0);
        assert_eq!(
            spans.len(),
            2,
            "hole should split the band into two spans: {spans:?}"
        );
    }

    // --- ruby ----------------------------------------------------------------

    #[test]
    #[allow(clippy::float_cmp)] // exact, representable expected values
    fn ruby_annotation_font_scale_is_real_not_06_fudge() {
        // The annotation is sized at the used font-size of the ruby-text run, which the
        // UA stylesheet sets to 50% of the base — NOT a 0.6 per-character fudge.
        let base_font_size = 20.0_f32;
        let annotation_font_size = base_font_size * RUBY_ANNOTATION_FONT_SCALE;
        assert_eq!(annotation_font_size, 10.0);
        assert!(
            (RUBY_ANNOTATION_FONT_SCALE - 0.6).abs() > f32::EPSILON,
            "annotation scale must not be the old 0.6 magic ratio"
        );
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact, representable expected values
    fn ruby_box_reserves_max_width_and_stacks_annotation_above_base() {
        // Wider base, narrower annotation: reserved inline-size = base width.
        let (w, h) = ruby_reserved_box(80.0, 30.0, 24.0, 12.0);
        assert_eq!(w, 80.0, "reserved width is the wider of base/annotation");
        // Block-size stacks the annotation line above the base line => base reserves
        // vertical space for the annotation.
        assert_eq!(h, 36.0, "block-size = base line + annotation line");
        assert!(
            h > 24.0,
            "ruby box must reserve extra vertical space for the annotation"
        );

        // Narrower base, wider annotation: reserved inline-size = annotation width.
        let (w2, _) = ruby_reserved_box(20.0, 50.0, 24.0, 12.0);
        assert_eq!(w2, 50.0, "a long annotation widens the reserved box");
    }
