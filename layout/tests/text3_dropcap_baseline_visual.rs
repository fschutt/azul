//! VISUAL PNGs for baseline alignment, mixed font sizes, vertical-align, and
//! the float:left dropcap — rendered through the full solver3 + text3 + CPU
//! rasterizer so the geometry is eyeball-checkable. Companion to the exact
//! arithmetic asserts in `text3_baseline_exact.rs`.
//!
//! Every mock-font glyph is a filled rectangle, so each PNG shows exactly where
//! each glyph box landed. Output: `target/text3_visual/<scenario>.png`.
//!
//! Run SAFELY (memory-capped):
//! ```text
//! systemd-run --user --scope -p MemoryMax=8G -p MemorySwapMax=0 -q -- \
//!   bash -c 'cd /home/fs/Development/azul && timeout 700 cargo test -p azul-layout \
//!     --test text3_dropcap_baseline_visual -- --test-threads=2 --nocapture 2>&1' | tail -40
//! ```

#![cfg(all(feature = "cpurender", feature = "text_layout", feature = "font_loading"))]

use std::path::PathBuf;

use azul_core::dom::Dom;
use azul_css::props::basic::FontRef;
use azul_layout::{
    cpurender::{render_component_preview, ComponentPreviewOptions},
    font::loading::build_font_cache,
    font_traits::FontManager,
    xml::DomXmlExt,
};

fn out_dir() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.push("target");
    p.push("text3_visual");
    std::fs::create_dir_all(&p).expect("create target/text3_visual");
    p
}

fn font_manager() -> FontManager<FontRef> {
    // Built-in Mock Mono / Wide are enough for these scenarios.
    FontManager::<FontRef>::new(build_font_cache()).expect("font manager")
}

fn render_to_png(scenario: &str, html: &str, fm: &FontManager<FontRef>, w: f32, h: f32) -> PathBuf {
    let styled_dom = Dom::from_xml_string(html);
    let opts = ComponentPreviewOptions {
        width: Some(w),
        height: Some(h),
        dpi_factor: 1.0,
        ..Default::default()
    };
    let result = render_component_preview(&styled_dom, fm, opts, None)
        .unwrap_or_else(|e| panic!("[{scenario}] render failed: {e}"));
    assert!(!result.png_data.is_empty(), "[{scenario}] empty png");
    let path = out_dir().join(format!("{scenario}.png"));
    std::fs::write(&path, &result.png_data).unwrap_or_else(|e| panic!("[{scenario}] write: {e}"));
    println!("[{scenario}] wrote {} ({} bytes)", path.display(), result.png_data.len());
    path
}

/// Mixed font sizes on one line: 20px + 40px Mono. Eyeball: the small run's
/// glyph BOTTOMS sit on the big run's baseline (not top-aligned).
#[test]
fn baseline_mixed_sizes() {
    let fm = font_manager();
    let html = r#"
<html><head><style>
  body { margin: 0; background: white; }
  .small { font-family: "Azul Mock Mono"; font-size: 20px; color: #cc2222; }
  .big { font-family: "Azul Mock Mono"; font-size: 40px; color: #2266cc; }
  #line { line-height: 40px; }
</style></head><body>
  <div id="line"><span class="small">small</span><span class="big">BIG</span><span class="small">small</span></div>
</body></html>"#;
    render_to_png("baseline_mixed_sizes", html, &fm, 400.0, 60.0);
}

/// vertical-align super / sub / baseline. Eyeball: 'super' box rides high,
/// 'sub' box rides low, both smaller-looking due to shift.
#[test]
fn vertical_align_variants() {
    let fm = font_manager();
    let html = r#"
<html><head><style>
  body { margin: 0; background: white; }
  #line { font-family: "Azul Mock Mono"; font-size: 40px; line-height: 60px; color: #222; }
  .sup { vertical-align: super; color: #cc2222; }
  .sub { vertical-align: sub; color: #2266cc; }
</style></head><body>
  <div id="line">x<span class="sup">S</span>y<span class="sub">B</span>z</div>
</body></html>"#;
    render_to_png("vertical_align_variants", html, &fm, 400.0, 80.0);
}

/// Dropcap: float:left 80px first letter, 20px body wraps around it, then
/// reflows full-width below. Eyeball: first ~4 body lines are indented past the
/// dropcap; lines below the cap start at x=0.
#[test]
fn dropcap_float() {
    let fm = font_manager();
    let html = r#"
<html><head><style>
  body { margin: 0; background: white; }
  #cap { float: left; font-family: "Azul Mock Mono"; font-size: 80px; color: #cc2222; line-height: 80px; margin-right: 6px; }
  #body { font-family: "Azul Mock Mono"; font-size: 20px; width: 300px; line-height: 20px; color: #2266cc; }
</style></head><body>
  <div id="cap">A</div>
  <div id="body">lorem ipsum dolor sit amet consectetur adipiscing elit sed do eiusmod tempor incididunt ut labore et dolore magna aliqua ut enim ad minim veniam quis nostrud</div>
</body></html>"#;
    render_to_png("dropcap_float", html, &fm, 360.0, 200.0);
}
