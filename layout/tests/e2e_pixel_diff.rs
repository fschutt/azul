//! E2E pixel-diff regression tests for the CPU renderer.
//!
//! These tests render DOMs via `render_dom_to_image`, then compare the output
//! against reference PNGs stored in `tests/reference_images/`.
//!
//! On first run (or when the reference doesn't exist), the rendered image is
//! saved as the new baseline.  Subsequent runs compare against that baseline.
//!
//! # Running
//!
//! ```bash
//! cargo test --test e2e_pixel_diff -p azul-layout --features "cpurender xml"
//! ```
//!
//! # Updating baselines
//!
//! Delete the reference PNG and re-run:
//! ```bash
//! rm layout/tests/reference_images/red_box.png
//! cargo test --test e2e_pixel_diff -p azul-layout --features "cpurender xml"
//! ```

#[cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]
mod tests {
    use azul_core::dom::Dom;
    use azul_css::css::Css;
    use azul_layout::cpurender::{render_dom_to_image, AzulPixmap, pixel_diff};
    use std::path::PathBuf;

    /// Directory for reference PNGs (relative to the layout crate root).
    fn reference_dir() -> PathBuf {
        let manifest = std::env::var("CARGO_MANIFEST_DIR")
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(manifest).join("tests").join("reference_images")
    }

    /// Render a DOM, compare against reference, fail if they differ.
    ///
    /// If the reference file doesn't exist, saves the rendered image as baseline
    /// and the test passes (first run bootstrapping).
    fn assert_pixel_match(
        name: &str,
        dom: Dom,
        css: Css,
        width: f32,
        height: f32,
        threshold: u8,
    ) {
        let ref_dir = reference_dir();
        std::fs::create_dir_all(&ref_dir).expect("create reference_images dir");

        let ref_path = ref_dir.join(format!("{}.png", name));
        let actual_path = ref_dir.join(format!("{}_actual.png", name));

        // Render
        let png_bytes = render_dom_to_image(dom, css, width, height, 1.0)
            .expect("render_dom_to_image failed");
        assert!(!png_bytes.is_empty(), "rendered PNG is empty");

        // Decode rendered
        let rendered = AzulPixmap::decode_png(&png_bytes)
            .expect("decode rendered PNG");

        if !ref_path.exists() {
            // First run: save as baseline
            std::fs::write(&ref_path, &png_bytes)
                .expect("save reference PNG");
            eprintln!(
                "[baseline] Saved new reference: {} ({}x{})",
                ref_path.display(), rendered.width(), rendered.height()
            );
            return;
        }

        // Load reference
        let ref_bytes = std::fs::read(&ref_path).expect("read reference PNG");
        let reference = AzulPixmap::decode_png(&ref_bytes).expect("decode reference PNG");

        let result = pixel_diff(&reference, &rendered, threshold);

        if !result.dimensions_match {
            // Save actual for debugging
            std::fs::write(&actual_path, &png_bytes).ok();
            panic!(
                "[{}] Dimension mismatch: reference={}x{}, actual={}x{}\n\
                 Actual saved to: {}",
                name, result.ref_width, result.ref_height,
                result.test_width, result.test_height,
                actual_path.display(),
            );
        }

        if result.diff_count > 0 {
            // Save actual for debugging
            std::fs::write(&actual_path, &png_bytes).ok();
            panic!(
                "[{}] Pixel diff: {}/{} pixels differ (max_delta={}, ratio={:.4})\n\
                 Reference: {}\n\
                 Actual:    {}",
                name, result.diff_count, result.total_pixels,
                result.max_delta, result.diff_ratio(),
                ref_path.display(), actual_path.display(),
            );
        }

        // Clean up _actual file on success
        let _ = std::fs::remove_file(&actual_path);
    }

    fn empty_css() -> Css {
        Css::new(Vec::new())
    }

    // =====================================================================
    // Test cases
    // =====================================================================

    #[test]
    fn red_box_100x100() {
        let dom = Dom::create_div()
            .with_inline_style("width:100px;height:100px;background-color:red");
        assert_pixel_match("red_box_100x100", dom, empty_css(), 100.0, 100.0, 0);
    }

    #[test]
    fn blue_box_with_border() {
        let dom = Dom::create_div()
            .with_inline_style(
                "width:80px;height:60px;background-color:blue;\
                 border:2px solid black"
            );
        assert_pixel_match("blue_box_with_border", dom, empty_css(), 100.0, 80.0, 0);
    }

    #[test]
    fn nested_boxes() {
        let inner = Dom::create_div()
            .with_inline_style("width:40px;height:40px;background-color:green");
        let outer = Dom::create_div()
            .with_inline_style(
                "width:100px;height:100px;background-color:#cccccc;\
                 display:flex;justify-content:center;align-items:center"
            )
            .with_child(inner);
        assert_pixel_match("nested_boxes", outer, empty_css(), 100.0, 100.0, 0);
    }

    #[test]
    fn gradient_background() {
        let dom = Dom::create_div()
            .with_inline_style(
                "width:200px;height:50px;\
                 background:linear-gradient(to right, red, blue)"
            );
        assert_pixel_match("gradient_background", dom, empty_css(), 200.0, 50.0, 2);
    }

    #[test]
    fn multiple_children_flex() {
        let child = |color: &str| -> Dom {
            Dom::create_div()
                .with_inline_style(&format!(
                    "width:30px;height:30px;background-color:{};margin:5px", color
                ))
        };
        let parent = Dom::create_div()
            .with_inline_style("width:200px;height:50px;display:flex;background-color:white")
            .with_child(child("red"))
            .with_child(child("green"))
            .with_child(child("blue"));
        assert_pixel_match("multiple_children_flex", parent, empty_css(), 200.0, 50.0, 0);
    }

    #[test]
    fn box_shadow() {
        let dom = Dom::create_div()
            .with_inline_style(
                "width:60px;height:60px;background-color:white;\
                 box-shadow:4px 4px 8px rgba(0,0,0,0.5);\
                 margin:20px"
            );
        // Threshold of 3 for anti-aliased shadow edges
        assert_pixel_match("box_shadow", dom, empty_css(), 120.0, 120.0, 3);
    }

    #[test]
    fn resize_stability() {
        // Render the same DOM at two sizes and verify each is self-consistent
        let make_dom = || {
            Dom::create_div()
                .with_inline_style("width:100%;height:100%;background-color:#336699")
        };

        // 200x150
        assert_pixel_match(
            "resize_stability_200x150",
            make_dom(), empty_css(), 200.0, 150.0, 0,
        );
        // 400x300
        assert_pixel_match(
            "resize_stability_400x300",
            make_dom(), empty_css(), 400.0, 300.0, 0,
        );
    }

    #[test]
    fn svg_clip_regression() {
        use azul_core::svg_path_parser::parse_svg_path_d;
        let clip = parse_svg_path_d("M 10,10 L 90,10 L 90,90 L 10,90 Z").unwrap();
        let dom = Dom::create_div()
            .with_inline_style("width:100px;height:100px;background-color:red")
            .with_svg_clip_path(clip);
        assert_pixel_match("svg_clip_regression", dom, empty_css(), 100.0, 100.0, 0);
    }
}
