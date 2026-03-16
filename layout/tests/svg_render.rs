//! End-to-end SVG rendering tests.
//!
//! These tests exercise the full pipeline: SVG path parsing → DOM clip →
//! layout → CPU render → PNG output.

#[cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]
mod tests {
    use azul_core::dom::Dom;
    use azul_core::svg_path_parser::{parse_svg_path_d, svg_circle_to_paths, svg_rect_to_path};
    use azul_css::css::Css;
    use azul_layout::cpurender::render_dom_to_image;

    fn empty_css() -> Css {
        Css::new(Vec::new())
    }

    #[test]
    fn test_roundtrip_square() {
        // Parse a simple square path, attach as SVG clip to a red div, render to PNG
        let clip = parse_svg_path_d("M 0,0 L 100,0 L 100,100 L 0,100 Z").unwrap();
        let dom = Dom::create_div()
            .with_inline_style("width:100px;height:100px;background-color:red")
            .with_svg_clip_path(clip);

        let css = empty_css();
        let png = render_dom_to_image(dom, css, 100.0, 100.0, 1.0).unwrap();
        assert!(!png.is_empty(), "PNG should not be empty");
        // PNG magic bytes
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn test_svg_clip_circle() {
        // Circle clip on a blue div
        let circle_path = svg_circle_to_paths(50.0, 50.0, 50.0);
        let clip = azul_core::svg::SvgMultiPolygon {
            rings: azul_core::svg::SvgPathVec::from_vec(vec![circle_path]),
        };
        let dom = Dom::create_div()
            .with_inline_style("width:100px;height:100px;background-color:blue")
            .with_svg_clip_path(clip);

        let css = empty_css();
        let png = render_dom_to_image(dom, css, 100.0, 100.0, 1.0).unwrap();
        assert!(!png.is_empty());
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn test_svg_clip_rounded_rect() {
        // Rounded rect clip
        let rect_path = svg_rect_to_path(0.0, 0.0, 200.0, 100.0, 15.0, 15.0);
        let clip = azul_core::svg::SvgMultiPolygon {
            rings: azul_core::svg::SvgPathVec::from_vec(vec![rect_path]),
        };
        let dom = Dom::create_div()
            .with_inline_style("width:200px;height:100px;background-color:green")
            .with_svg_clip_path(clip);

        let css = empty_css();
        let png = render_dom_to_image(dom, css, 200.0, 100.0, 1.0).unwrap();
        assert!(!png.is_empty());
        // Write to tmp for manual inspection
        let _ = std::fs::write("/tmp/azul_svg_rounded_rect.png", &png);
    }

    #[test]
    fn test_render_empty_dom() {
        let dom = Dom::create_div()
            .with_inline_style("width:50px;height:50px");
        let css = empty_css();
        let png = render_dom_to_image(dom, css, 50.0, 50.0, 1.0).unwrap();
        assert!(!png.is_empty());
    }

    #[test]
    fn test_svg_clip_star_path() {
        // 5-pointed star
        let star = parse_svg_path_d(
            "M 50,0 L 61,35 L 98,35 L 68,57 L 79,91 L 50,70 L 21,91 L 32,57 L 2,35 L 39,35 Z"
        ).unwrap();
        let dom = Dom::create_div()
            .with_inline_style("width:100px;height:100px;background-color:gold")
            .with_svg_clip_path(star);

        let css = empty_css();
        let png = render_dom_to_image(dom, css, 100.0, 100.0, 1.0).unwrap();
        assert!(!png.is_empty());
        let _ = std::fs::write("/tmp/azul_svg_star.png", &png);
    }
}
