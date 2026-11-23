//! Peak memory and time of a paged layout (the one printpdf's `from_html` runs) of a
//! document with one extreme box, for `/usr/bin/time -v`:
//!
//! ```text
//! cargo build --release -p azul-layout --example paged_extremes
//! /usr/bin/time -v target/release/examples/paged_extremes 'height:1e12px'
//! ```
//!
//! The argument is the `style` of one `<p>x</p>`; `--paragraphs N` lays out N paragraphs of
//! text instead (a long real document). Prints the number of pages, how many of them carry
//! an item, and the findings of the layout.

use std::collections::BTreeMap;

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    paged::FragmentationContext,
    resources::{IdNamespace, ImageCache, RendererResources},
    task::GetSystemTimeCallback,
};
use azul_layout::{
    font_traits::{FontManager, TextLayoutCache},
    solver3::{
        cache::LayoutCache, paged_layout::layout_document_paged_with_config,
        pagination::FakePageConfig,
    },
    text3::default::PathLoader,
    xml::DomXmlExt,
};
use rust_fontconfig::FcFontCache;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let html = match args.as_slice() {
        [flag, n] if flag == "--paragraphs" => {
            let n: usize = n.parse().expect("a paragraph count");
            let mut body = String::new();
            for i in 0..n {
                body.push_str(&format!(
                    "<p>Paragraph {i}: the quick brown fox jumps over the lazy dog, again and \
                     again, until the line wraps and the page fills up with text.</p>"
                ));
            }
            format!("<html><body>{body}</body></html>")
        }
        [style] => format!("<html><body><p style=\"{style}\">x</p></body></html>"),
        _ => panic!("usage: paged_extremes '<style of the p>' | --paragraphs N"),
    };

    let page = LogicalSize::new(595.0, 842.0);
    let styled_dom = Dom::from_xml_string(&html);
    let mut font_manager = FontManager::new(FcFontCache::default()).expect("font manager");
    let loader = PathLoader::new();
    let mut messages = Some(Vec::new());
    let start = std::time::Instant::now();
    let pages = layout_document_paged_with_config(
        &mut LayoutCache::default(),
        &mut TextLayoutCache::new(),
        FragmentationContext::new_paged(page),
        &styled_dom,
        LogicalRect::new(LogicalPosition::zero(), page),
        &mut font_manager,
        &BTreeMap::new(),
        &mut messages,
        None,
        &RendererResources::default(),
        IdNamespace(0),
        DomId::ROOT_ID,
        move |bytes, index| loader.load_font_shared(bytes, index),
        FakePageConfig::new(),
        &ImageCache::default(),
        GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        },
        false,
    )
    .expect("the paged layout");
    let elapsed = start.elapsed();
    let with_items = pages.iter().filter(|p| !p.items.is_empty()).count();
    println!(
        "pages={} with_items={} layout_ms={}",
        pages.len(),
        with_items,
        elapsed.as_millis()
    );
    for message in messages.unwrap_or_default() {
        let text = format!("{message:?}");
        if text.contains("arning") {
            println!("{text}");
        }
    }
}
