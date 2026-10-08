//! The layout of a broad corpus matches its golden dumps: a characterization
//! net for the whole layout engine.
//!
//! Every document below is laid out and printed as a canonical text dump -
//! every layout node (its parent, DOM node, formatting context, border box
//! rounded to 1/64 px, overflow size, scrollbar need, escaped margins,
//! baseline, intrinsic widths), each inline formatting context's lines (y,
//! item and glyph count, first and last x) and every display-list item (its
//! kind and bounds, a text run's glyph count, a scroll frame's content size)
//! - and the dump is compared with the file of its group in `tests/golden/`.
//!
//! The corpus: every HTML fixture the repo keeps for layout tests (the mail
//! corpus and the normalized web-platform tests the reftest runner lays out),
//! documents built here for each layout feature (block flow and margin
//! collapsing, floats and clearance, inline text, bidi, lists and markers,
//! tables, flex, grid, multicol, positioned boxes, overflow and scrolling,
//! `text-overflow`, text decoration, writing modes, deep nesting), paged
//! documents through both paged entry points (the slicer and the
//! break-token page loop), and three widget DOMs (status bar, ribbon, list
//! view).
//!
//! Fonts: an EMPTY `FcFontCache` - no system font is ever read - plus the
//! mock fonts every `FontManager` registers and the proportional stress font
//! of `tests/fonts`, which also stands in for the generic families. The dump
//! is the same on every machine.
//!
//! Written ONCE with `AZ_GOLDEN_WRITE=1`, before `solver3/fc.rs` and
//! `text3/cache.rs` were split into modules (REFACTOR13): during that
//! refactor a diff is a regression to find, never a file to regenerate. A
//! later, deliberate layout change rewrites them with `AZ_GOLDEN_WRITE=1`
//! and its commit shows the diff.

use std::{
    collections::BTreeMap,
    fmt::Write as _,
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
};

use azul_core::{
    dom::{Dom, DomId, DomVec},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::{IdNamespace, ImageCache, RendererResources},
    styled_dom::StyledDom,
    task::{get_system_time_libstd, GetSystemTimeCallback},
};
use azul_css::{AzString, StringVec};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    font_traits::{FontManager, ParsedFontTrait, TextLayoutCache},
    paged::FragmentationContext,
    solver3::{
        display_list::{DisplayList, DisplayListItem},
        layout_tree::LayoutTree,
        paged_layout::{layout_document_paged_with_config, layout_document_tokenized},
        pagination::FakePageConfig,
    },
    text3::{
        cache::{MemoryFontTier, ShapedItem, UnifiedLayout},
        default::PathLoader,
    },
    widgets::{
        list_view::{ListView, ListViewRow},
        ribbon::{Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab},
        statusbar::{StatusBar, StatusBarSegment, StatusBarSync, StatusBarSyncKind, StatusBarZoom},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
    Solver3LayoutCache,
};
use rust_fontconfig::{FcFontCache, UnicodeRange};

/// The proportional stress font of `tests/fonts` (see its README).
const PROP: &str = "Azul Mock Prop";

fn manifest_dir() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// Registers the proportional mock font under its own name and, as the
/// last resort, under every generic family.
fn register_fonts<T: ParsedFontTrait>(fonts: &mut FontManager<T>) {
    let bytes = std::fs::read(manifest_dir().join("tests/fonts/azul-mock-prop.ttf"))
        .expect("tests/fonts/azul-mock-prop.ttf");
    let ascii = || vec![UnicodeRange { start: 0x20, end: 0x7E }];
    fonts.register_named_font(PROP, &bytes, ascii());
    for generic in ["serif", "sans-serif", "monospace", "system-ui", "cursive", "fantasy"] {
        fonts.register_named_font_in_tier(generic, &bytes, ascii(), MemoryFontTier::Fallback);
    }
}

/// `v` on the 1/64 px grid (`-0` printed as `0`).
fn px(v: f32) -> String {
    if !v.is_finite() {
        return format!("{v}");
    }
    let q = (v * 64.0).round() / 64.0;
    if q == 0.0 {
        "0".to_string()
    } else {
        format!("{q}")
    }
}

// ---------------------------------------------------------------------------
// The dump
// ---------------------------------------------------------------------------

fn dump_tree(out: &mut String, tree: &LayoutTree, positions: &[LogicalPosition]) {
    for (i, node) in tree.nodes.iter().enumerate() {
        let warm = tree.warm.get(i);
        let _ = write!(out, "n{i}");
        match node.parent {
            Some(p) => {
                let _ = write!(out, " p{p}");
            }
            None => out.push_str(" root"),
        }
        match node.dom_node_id {
            Some(d) => {
                let _ = write!(out, " d{}", d.index());
            }
            None => out.push_str(" d-"),
        }
        let _ = write!(out, " {:?}", node.formatting_context);
        if let Some(a) = tree.cold.get(i).and_then(|c| c.anonymous_type) {
            let _ = write!(out, " anon={a:?}");
        }
        if let Some(w) = warm {
            if let Some(p) = &w.pseudo_element {
                let _ = write!(out, " pseudo={p:?}");
            }
        }
        match positions.get(i) {
            Some(p) => {
                let _ = write!(out, " @{},{}", px(p.x), px(p.y));
            }
            None => out.push_str(" @-"),
        }
        match node.used_size {
            Some(s) => {
                let _ = write!(out, " {}x{}", px(s.width), px(s.height));
            }
            None => out.push_str(" -x-"),
        }
        if let Some(w) = warm {
            if let Some(ov) = w.overflow_content_size {
                let _ = write!(out, " ov={}x{}", px(ov.width), px(ov.height));
            }
            if let Some(sb) = &w.scrollbar_info {
                if sb.needs_vertical || sb.needs_horizontal {
                    let _ = write!(
                        out,
                        " sb={}{}",
                        if sb.needs_vertical { "v" } else { "" },
                        if sb.needs_horizontal { "h" } else { "" }
                    );
                }
            }
            if let Some(t) = w.escaped_top_margin {
                let _ = write!(out, " mt={}", px(t));
            }
            if let Some(b) = w.escaped_bottom_margin {
                let _ = write!(out, " mb={}", px(b));
            }
            if let Some(b) = w.baseline {
                let _ = write!(out, " bl={}", px(b));
            }
            if let Some(ic) = w.intrinsic_sizes {
                let _ = write!(
                    out,
                    " ic={}/{}",
                    px(ic.min_content_width),
                    px(ic.max_content_width)
                );
            }
            if let Some(r) = w.relative_position {
                let _ = write!(out, " rel={},{}", px(r.x), px(r.y));
            }
        }
        out.push('\n');
        if let Some(inline) = warm.and_then(|w| w.inline_layout_result.as_ref()) {
            dump_lines(out, &inline.layout);
        }
    }
}

/// One line per line box: its y, how many items and glyphs it holds, and
/// where its first item starts and its last one ends.
fn dump_lines(out: &mut String, layout: &UnifiedLayout) {
    let mut lines: BTreeMap<usize, (usize, usize, f32, f32, f32)> = BTreeMap::new();
    for it in &layout.items {
        let width = it.item.bounds().width;
        let glyphs = match &it.item {
            ShapedItem::Cluster(c) => c.glyphs.len(),
            _ => 0,
        };
        let line = lines.entry(it.line_index).or_insert((
            0,
            0,
            f32::INFINITY,
            f32::NEG_INFINITY,
            it.position.y,
        ));
        line.0 += 1;
        line.1 += glyphs;
        line.2 = line.2.min(it.position.x);
        line.3 = line.3.max(it.position.x + width);
    }
    for (k, (items, glyphs, x0, x1, y)) in lines {
        let _ = writeln!(
            out,
            "  line {k} y={} items={items} glyphs={glyphs} x={}..{}",
            px(y),
            px(x0),
            px(x1)
        );
    }
}

fn dump_display_list(out: &mut String, dl: &DisplayList) {
    for (j, item) in dl.items.iter().enumerate() {
        let debug = format!("{item:?}");
        let kind: String = debug.chars().take_while(char::is_ascii_alphanumeric).collect();
        let _ = write!(out, "  dl{j} {kind}");
        if let Some(b) = item.bounds() {
            let _ = write!(
                out,
                " {},{} {}x{}",
                px(b.origin.x),
                px(b.origin.y),
                px(b.size.width),
                px(b.size.height)
            );
        }
        match item {
            DisplayListItem::Text {
                glyphs,
                font_size_px,
                ..
            } => {
                let _ = write!(out, " glyphs={} fs={}", glyphs.len(), px(*font_size_px));
            }
            DisplayListItem::PushScrollFrame { content_size, .. } => {
                let _ = write!(
                    out,
                    " content={}x{}",
                    px(content_size.width),
                    px(content_size.height)
                );
            }
            _ => {}
        }
        out.push('\n');
    }
}

fn dump_window(out: &mut String, lw: &LayoutWindow) {
    for (dom_id, result) in &lw.layout_results {
        let _ = writeln!(out, "dom {dom_id:?}");
        dump_tree(out, &result.layout_tree, &result.calculated_positions);
        dump_display_list(out, &result.display_list);
    }
}

fn panic_text(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-string panic payload)".to_string()
    }
}

// ---------------------------------------------------------------------------
// The three ways a document is laid out
// ---------------------------------------------------------------------------

/// Lays `styled` out in a window `width` x `height` and dumps it.
fn dump_in_window(out: &mut String, styled: StyledDom, width: f32, height: f32) {
    let run = catch_unwind(AssertUnwindSafe(move || {
        let mut lw = LayoutWindow::new(FcFontCache::default()).map_err(|e| format!("{e:?}"))?;
        register_fonts(&mut lw.font_manager);
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(width, height);
        lw.current_window_state = ws.clone();
        let mut debug = None;
        lw.layout_and_generate_display_list(
            styled,
            &ws,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut debug,
        )
        .map_err(|e| format!("{e:?}"))?;
        let mut text = String::new();
        dump_window(&mut text, &lw);
        Ok::<String, String>(text)
    }));
    match run {
        Ok(Ok(text)) => out.push_str(&text),
        Ok(Err(e)) => {
            let _ = writeln!(out, "layout error: {e}");
        }
        Err(payload) => {
            let _ = writeln!(out, "panic: {}", panic_text(&payload));
        }
    }
}

fn parse_and_dump_in_window(out: &mut String, xml: &str, width: f32, height: f32) {
    match azul_layout::xml::parse_xml_to_styled_dom(xml) {
        Ok(styled) => dump_in_window(out, styled, width, height),
        Err(e) => {
            let _ = writeln!(out, "parse error: {e:?}");
        }
    }
}

fn fresh_layout_cache() -> Solver3LayoutCache {
    Solver3LayoutCache {
        prev_viewport: LogicalRect {
            origin: LogicalPosition::zero(),
            size: LogicalSize::zero(),
        },
        ..Default::default()
    }
}

/// Paged media through the slicer (`layout_document_paged_with_config`):
/// the tree, then every page's display list.
fn dump_paged_slicer(out: &mut String, xml: &str, page: LogicalSize) {
    let run = catch_unwind(AssertUnwindSafe(|| {
        let styled = azul_layout::xml::parse_xml_to_styled_dom(xml).map_err(|e| format!("{e:?}"))?;
        let mut fonts = FontManager::new(FcFontCache::default()).map_err(|e| format!("{e:?}"))?;
        register_fonts(&mut fonts);
        let mut cache = fresh_layout_cache();
        let mut text_cache = TextLayoutCache::new();
        let loader = PathLoader::new();
        let pages = layout_document_paged_with_config(
            &mut cache,
            &mut text_cache,
            FragmentationContext::new_paged(page),
            &styled,
            LogicalRect {
                origin: LogicalPosition::zero(),
                size: page,
            },
            &mut fonts,
            &BTreeMap::new(),
            &mut None,
            None,
            &RendererResources::default(),
            IdNamespace(0),
            DomId::ROOT_ID,
            |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
                loader.load_font_shared(bytes, index)
            },
            FakePageConfig::new(),
            &ImageCache::default(),
            GetSystemTimeCallback {
                cb: get_system_time_libstd,
            },
            false,
        )
        .map_err(|e| format!("{e:?}"))?;
        let mut text = String::new();
        if let Some(tree) = cache.tree.as_ref() {
            dump_tree(&mut text, tree, &cache.calculated_positions);
        }
        for (i, dl) in pages.iter().enumerate() {
            let _ = writeln!(text, "page {i}");
            dump_display_list(&mut text, dl);
        }
        Ok::<String, String>(text)
    }));
    match run {
        Ok(Ok(text)) => out.push_str(&text),
        Ok(Err(e)) => {
            let _ = writeln!(out, "layout error: {e}");
        }
        Err(payload) => {
            let _ = writeln!(out, "panic: {}", panic_text(&payload));
        }
    }
}

/// Paged media through the break-token page loop
/// (`layout_document_tokenized`): every page's fitted size, outgoing token
/// and display list.
fn dump_paged_tokens(out: &mut String, xml: &str, page: LogicalSize) {
    let run = catch_unwind(AssertUnwindSafe(|| {
        let styled = azul_layout::xml::parse_xml_to_styled_dom(xml).map_err(|e| format!("{e:?}"))?;
        let mut fonts = FontManager::new(FcFontCache::default()).map_err(|e| format!("{e:?}"))?;
        register_fonts(&mut fonts);
        let mut cache = fresh_layout_cache();
        let mut text_cache = TextLayoutCache::new();
        let loader = PathLoader::new();
        let pages = layout_document_tokenized(
            &mut cache,
            &mut text_cache,
            &styled,
            LogicalRect {
                origin: LogicalPosition::zero(),
                size: page,
            },
            &mut fonts,
            &mut None,
            &ImageCache::default(),
            GetSystemTimeCallback {
                cb: get_system_time_libstd,
            },
            |bytes: std::sync::Arc<rust_fontconfig::FontBytes>, index: usize| {
                loader.load_font_shared(bytes, index)
            },
            &RendererResources::default(),
            IdNamespace(0),
            DomId::ROOT_ID,
            page.height,
            32,
        )
        .map_err(|e| format!("{e:?}"))?;
        let mut text = String::new();
        for (i, p) in pages.iter().enumerate() {
            let _ = writeln!(
                text,
                "token page {i} content={} outgoing={:?}",
                px(p.content_block_size),
                p.outgoing
            );
            dump_display_list(&mut text, &p.display_list);
        }
        Ok::<String, String>(text)
    }));
    match run {
        Ok(Ok(text)) => out.push_str(&text),
        Ok(Err(e)) => {
            let _ = writeln!(out, "layout error: {e}");
        }
        Err(payload) => {
            let _ = writeln!(out, "panic: {}", panic_text(&payload));
        }
    }
}

// ---------------------------------------------------------------------------
// Comparing with the golden file
// ---------------------------------------------------------------------------

/// Compares `dump` with `tests/golden/<group>.txt` - or writes it there when
/// `AZ_GOLDEN_WRITE` is set.
fn matches_golden(group: &str, dump: &str) {
    let path = manifest_dir().join("tests/golden").join(format!("{group}.txt"));
    if std::env::var_os("AZ_GOLDEN_WRITE").is_some() {
        std::fs::create_dir_all(path.parent().expect("tests/golden")).expect("tests/golden");
        std::fs::write(&path, dump).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        return;
    }
    let golden = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} - the golden dumps are written once with AZ_GOLDEN_WRITE=1",
            path.display()
        )
    });
    if golden == dump {
        return;
    }
    let actual = std::env::temp_dir().join(format!("az_golden_{group}.actual.txt"));
    let _ = std::fs::write(&actual, dump);
    let mut report = String::new();
    let mut doc = "(before the first document)";
    let mut shown = 0;
    let (want, got): (Vec<&str>, Vec<&str>) = (golden.lines().collect(), dump.lines().collect());
    for i in 0..want.len().max(got.len()) {
        let (w, g) = (want.get(i).copied(), got.get(i).copied());
        if let Some(header) = w.filter(|l| l.starts_with("=== ")) {
            doc = header;
        }
        if w != g && shown < 40 {
            if shown == 0 || report.lines().last() != Some(doc) {
                let _ = writeln!(report, "{doc}");
            }
            let _ = writeln!(report, "  line {}:\n    golden: {:?}\n    now:    {:?}", i + 1, w, g);
            shown += 1;
        }
    }
    panic!(
        "the layout of group `{group}` differs from tests/golden/{group}.txt ({} vs {} lines; the \
         whole dump is in {}):\n{report}",
        want.len(),
        got.len(),
        actual.display()
    );
}

/// Every file under `dir` with one of `exts`, in path order.
fn files_under(dir: &Path, exts: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| exts.contains(&x))
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Lays out every fixture under `../tests/<dir>` at 800 x 600.
fn fixture_group(dirs: &[&str], exts: &[&str]) -> String {
    let root = manifest_dir().join("../tests");
    let mut out = String::new();
    let mut count = 0;
    for dir in dirs {
        for file in files_under(&root.join(dir), exts) {
            let name = file.strip_prefix(&root).unwrap_or(&file).display().to_string();
            let _ = writeln!(out, "=== {name}");
            match std::fs::read_to_string(&file) {
                Ok(xml) => parse_and_dump_in_window(&mut out, &xml, 800.0, 600.0),
                Err(e) => {
                    let _ = writeln!(out, "read error: {e}");
                }
            }
            count += 1;
        }
    }
    assert!(count > 0, "no fixture found under {dirs:?}");
    out
}

#[test]
fn the_mail_corpus_lays_out_as_its_golden_dump() {
    matches_golden("mail_corpus", &fixture_group(&["mail_corpus"], &["html"]));
}

#[test]
fn the_web_platform_css_fixtures_lay_out_as_their_golden_dump() {
    matches_golden(
        "wpt_css",
        &fixture_group(&["wpt/normalized/css"], &["html", "htm", "xht", "xhtml"]),
    );
}

#[test]
fn the_web_platform_html_and_local_fixtures_lay_out_as_their_golden_dump() {
    matches_golden(
        "wpt_html_and_local",
        &fixture_group(
            &["wpt/normalized/html", "wpt/normalized/local"],
            &["html", "htm", "xht", "xhtml"],
        ),
    );
}

// ---------------------------------------------------------------------------
// Documents built for each layout feature
// ---------------------------------------------------------------------------

/// A document in the proportional mock font, `style` after the base rules.
fn doc(style: &str, body: &str) -> String {
    format!(
        "<html><head><style>body {{ margin: 0; font-family: 'Azul Mock Prop'; font-size: 16px; \
         }} {style}</style></head><body>{body}</body></html>"
    )
}

/// `leaf` inside `depth` times `open` ... `close`.
fn nested(depth: usize, open: &str, close: &str, leaf: &str) -> String {
    let mut s = String::new();
    for _ in 0..depth {
        s.push_str(open);
    }
    s.push_str(leaf);
    for _ in 0..depth {
        s.push_str(close);
    }
    s
}

fn purpose_built() -> Vec<(&'static str, String)> {
    let long = "The quick brown fox jumps over the lazy dog and keeps running far away.";
    vec![
        (
            "block flow, sizing and centering",
            doc(
                ".a { width: 300px; height: 40px; padding: 5px 10px; border: 2px solid black; \
                 margin: 10px auto; } .b { width: 50%; min-height: 30px; box-sizing: border-box; \
                 padding: 10px; border: 3px solid red; } .c { max-width: 200px; height: calc(20px \
                 + 1em); margin-left: 25%; } .z { zoom: 2; font-size: 10px; padding: 1em; }",
                "<div class=\"a\">block one</div><div class=\"b\">half, border-box</div>\
                 <div class=\"c\">calc and a percentage margin</div>\
                 <div style=\"width: 120px; margin-left: auto;\">pushed right</div>\
                 <div align=\"center\"><div style=\"width: 100px; height: 10px;\"></div></div>\
                 <center><div style=\"width: 80px; height: 10px;\"></div></center>\
                 <div style=\"display: none\">hidden</div>\
                 <div style=\"display: contents\"><div style=\"height: 12px\">contents</div></div>\
                 <div style=\"width: min-content\">min content width</div>\
                 <div style=\"width: max-content\">max content width</div>\
                 <div style=\"width: fit-content; max-width: 90px\">fit content width here</div>\
                 <div class=\"z\">zoomed</div>\
                 <div style=\"height: 50%\">percentage of auto</div>\
                 <div style=\"min-width: 500px; max-width: 100px; height: 5px\"></div>",
            ),
        ),
        (
            "margin collapsing",
            doc(
                "",
                "<div style=\"margin: 20px 0 30px;\">a</div>\
                 <div style=\"margin: 40px 0 10px;\">b</div>\
                 <div style=\"margin-top: 15px\"><div style=\"margin-top: 25px\">\
                 <p style=\"margin: 35px 0\">nested escape</p></div></div>\
                 <div style=\"margin: 20px 0 30px\"></div>\
                 <div style=\"margin: -10px 0 5px\">negative</div>\
                 <div style=\"padding-top: 1px\"><div style=\"margin-top: 30px\">padding blocks</div></div>\
                 <div style=\"border-bottom: 1px solid black\"><div style=\"margin-bottom: 30px\">\
                 border blocks</div></div>\
                 <div style=\"overflow: hidden\"><div style=\"margin: 30px 0\">bfc root</div></div>\
                 <div style=\"margin-bottom: 50px; padding-bottom: 4px\">last padded</div>\
                 <div style=\"height: 0\"><div style=\"margin-top: 22px; height: 10px\">h0</div></div>\
                 <div style=\"padding: 4px\"><p></p></div>\
                 <div><p style=\"margin: 10px 0\"></p><p style=\"margin: 25px 0\"></p></div>\
                 <div style=\"display: flex\"><div style=\"margin: 10px\">flex item</div></div>",
            ),
        ),
        (
            "floats, clearance and shapes",
            doc(
                "body { width: 400px; font-size: 14px; }",
                &format!(
                    "<div style=\"float: left; width: 100px; height: 60px; margin: 5px\">left</div>\
                     <div style=\"float: right; width: 80px; height: 100px\">right</div>\
                     <p>{long} {long}</p><div style=\"clear: left\">cleared left</div>\
                     <div style=\"float: left; width: 50px; height: 50px\"></div>\
                     <div style=\"overflow: hidden; height: 30px\">a bfc beside a float</div>\
                     <div style=\"clear: both; margin-top: 10px\">cleared both</div>\
                     <div style=\"overflow: hidden\"><div style=\"float: left; width: 30px; \
                     height: 80px\"></div>contained float</div>\
                     <div><div style=\"float: right; width: 30px; height: 40px; clear: right\"></div>\
                     <div style=\"float: right; width: 30px; height: 40px; clear: right\"></div>\
                     stacked floats</div>\
                     <div style=\"float: left; width: 120px; height: 120px; shape-outside: \
                     circle(50%)\"></div><p>{long}</p>\
                     <div style=\"float: left; width: 500px; height: 10px\">too wide</div>\
                     <table style=\"width: 300px\"><tr><td>table beside floats</td></tr></table>"
                ),
            ),
        ),
        (
            "inline text, white space and breaking",
            doc(
                "body { width: 320px; }",
                &format!(
                    "<p>{long} {long}</p>\
                     <p style=\"line-height: 30px\">Line height <span style=\"font-size: 24px\">\
                     big</span> <span style=\"vertical-align: super\">sup</span> \
                     <span style=\"vertical-align: sub\">sub</span> end.</p>\
                     <p><span style=\"padding: 4px; border: 2px solid black; margin: 0 6px\">\
                     padded inline box that wraps onto the next line</span> text <b>bold</b> \
                     <i>italic</i></p>\
                     <p>Before <span style=\"display: inline-block; width: 60px; height: 30px; \
                     border: 1px solid\">ib</span> after <span style=\"display: inline-block; \
                     vertical-align: middle; height: 20px\">mid</span>.</p>\
                     <p style=\"text-align: center\">centered</p>\
                     <p style=\"text-align: right\">right</p>\
                     <p style=\"text-align: justify\">{long} {long}</p>\
                     <p style=\"text-indent: 30px; letter-spacing: 2px; word-spacing: 5px\">{long}</p>\
                     <p style=\"white-space: pre\">pre   formatted\n  second line</p>\
                     <p style=\"white-space: nowrap\">{long}</p>\
                     <p style=\"white-space: pre-wrap\">pre wrap   keeps   spaces {long}</p>\
                     <p style=\"white-space: pre-line\">pre line\n   collapses spaces</p>\
                     <p style=\"word-break: break-all\">Supercalifragilisticexpialidocious_no_spaces_at_all</p>\
                     <p style=\"overflow-wrap: anywhere\">Anotherverylongwordthatwrapsanywhere_in_the_box</p>\
                     <p style=\"hyphens: auto\" lang=\"en\">Hyphenation demonstrates extraordinarily \
                     incomprehensible characteristics.</p>\
                     <p>Line<br/>break<br/><br/>after a blank line</p>\
                     <p style=\"text-transform: uppercase\">upper case</p>\
                     <p style=\"font-family: 'Azul Mock Mono'\">monospace {long}</p>\
                     <p style=\"text-align-last: right; text-align: justify\">{long} {long}</p>\
                     <div>anonymous <div>block</div> inline again</div>"
                ),
            ),
        ),
        (
            "bidi",
            doc(
                "body { width: 300px; }",
                "<p dir=\"rtl\">right to left paragraph with several words that wrap</p>\
                 <p style=\"direction: rtl; unicode-bidi: bidi-override\">override text</p>\
                 <p>LTR <span dir=\"rtl\" style=\"unicode-bidi: embed\">embedded rtl</span> tail</p>\
                 <p>&#x5E9;&#x5DC;&#x5D5;&#x5DD; mixed &#x627;&#x644;&#x633;&#x644;&#x627;&#x645; text</p>\
                 <div style=\"direction: rtl\"><div style=\"width: 100px; height: 10px\">rtl</div></div>\
                 <p style=\"unicode-bidi: isolate\">isolate <bdi>bdi</bdi> <bdo dir=\"rtl\">bdo</bdo></p>",
            ),
        ),
        (
            "lists and markers",
            doc(
                "",
                "<ul><li>one</li><li>two<ul><li>nested a</li><li>nested b</li></ul></li></ul>\
                 <ol><li>first</li><li>second</li><li>third</li></ol>\
                 <ol start=\"5\" style=\"list-style-type: lower-roman\"><li>five</li><li>six</li></ol>\
                 <ul style=\"list-style-position: inside\"><li>inside marker</li></ul>\
                 <ol style=\"list-style-type: upper-alpha\"><li><div style=\"height: 30px\">block \
                 in li</div></li><li></li><li><table><tr><td>table in li</td></tr></table></li></ol>\
                 <ul style=\"list-style-type: none\"><li>no marker</li></ul>\
                 <ul><li style=\"float: left\">floated item</li></ul>\
                 <ol style=\"list-style-type: decimal-leading-zero\"><li>zero</li></ol>",
            ),
        ),
        (
            "tables",
            doc(
                "body { font-size: 12px; }",
                "<table border=\"1\"><caption>Caption</caption><thead><tr><th>H1</th><th>H2</th>\
                 </tr></thead><tbody><tr><td>a</td><td>bb</td></tr><tr><td colspan=\"2\">span \
                 two</td></tr><tr><td rowspan=\"2\">r</td><td>x</td></tr><tr><td>y</td></tr>\
                 </tbody></table>\
                 <table style=\"border-collapse: collapse; width: 300px\"><tr><td style=\"border: \
                 3px solid\">c1</td><td style=\"border: 1px dashed\">c2 longer content</td></tr></table>\
                 <table style=\"table-layout: fixed; width: 240px\"><colgroup><col style=\"width: \
                 60px\"/><col/></colgroup><tr><td>fixed</td><td>layout table cell text</td></tr></table>\
                 <table style=\"border-spacing: 5px 10px; empty-cells: hide\"><tr><td></td>\
                 <td>e</td></tr></table>\
                 <table style=\"caption-side: bottom\"><caption>Bottom</caption><tr><td \
                 style=\"vertical-align: bottom; height: 40px\">vb</td><td style=\"vertical-align: \
                 middle\">vm</td><td>t<br/>2</td><td style=\"vertical-align: baseline; font-size: \
                 20px\">bl</td></tr></table>\
                 <div style=\"display: table\"><div style=\"display: table-row\"><div \
                 style=\"display: table-cell; padding: 4px\">css table</div></div></div>\
                 <table><tr><td><table><tr><td>nested</td></tr></table></td><td>outer</td></tr></table>\
                 <table width=\"300\" cellpadding=\"4\" cellspacing=\"0\"><tr><td align=\"center\">\
                 legacy</td><td width=\"30%\">thirty</td></tr></table>\
                 <table dir=\"rtl\" style=\"border-collapse: collapse\"><tr><td style=\"border-left: \
                 4px solid\">rtl a</td><td>rtl b</td></tr></table>\
                 <table><tr><td style=\"visibility: collapse\">collapsed</td><td>kept</td></tr></table>\
                 <table style=\"width: 50px\"><tr><td>a very long cell that is wider than its table</td></tr></table>",
            ),
        ),
        (
            "flex",
            doc(
                "",
                "<div style=\"display: flex; width: 400px; height: 50px\"><div style=\"flex: 1\">a</div>\
                 <div style=\"flex: 2\">b</div><div style=\"width: 50px\">c</div></div>\
                 <div style=\"display: flex; flex-direction: column; height: 120px; \
                 justify-content: space-between\"><div>x</div><div>y</div></div>\
                 <div style=\"display: flex; flex-wrap: wrap; width: 200px\"><div style=\"width: \
                 80px; height: 20px\"></div><div style=\"width: 80px; height: 30px\"></div><div \
                 style=\"width: 80px; height: 20px\"></div></div>\
                 <div style=\"display: flex; align-items: center; height: 60px\"><div style=\"height: \
                 20px\">center</div><div style=\"align-self: flex-end\">end</div></div>\
                 <div style=\"display: inline-flex; gap: 10px\"><span>i1</span><span>i2</span></div>\
                 <div style=\"display: flex\"><div style=\"flex-shrink: 0; width: 500px\">no shrink</div></div>\
                 <div style=\"display: flex; flex-direction: row-reverse; width: 300px\"><div>1</div>\
                 <div>2</div></div>\
                 <div style=\"display: flex; width: 200px\"><p style=\"margin: 0\">a long text item \
                 in a flex row that has to wrap inside its item</p><div style=\"width: 50px\">x</div></div>",
            ),
        ),
        (
            "grid",
            doc(
                "",
                "<div style=\"display: grid; grid-template-columns: 100px 1fr 2fr; width: 400px; \
                 gap: 8px\"><div>a</div><div>b</div><div>c</div><div style=\"grid-column: span \
                 2\">d</div><div>e</div></div>\
                 <div style=\"display: grid; grid-template-rows: 30px auto; grid-template-columns: \
                 repeat(2, 50%)\"><div style=\"grid-row: 1 / 3\">tall</div><div>r1</div><div>r2</div></div>\
                 <div style=\"display: grid; grid-template-columns: auto auto; justify-items: center; \
                 align-items: end; height: 80px\"><div>x</div><div>yy</div></div>",
            ),
        ),
        (
            "multicol",
            doc(
                "body { width: 600px; }",
                &format!(
                    "<div style=\"column-count: 3; column-gap: 20px\"><p>{long}</p><p>{long}</p>\
                     <p>{long}</p><p>Four.</p><p>Five has more words.</p></div>\
                     <div style=\"column-width: 150px\"><div style=\"height: 100px\">a</div>\
                     <div style=\"height: 100px\">b</div></div>\
                     <div style=\"columns: 2\"><div style=\"float: left; width: 40px; height: \
                     40px\"></div><p>{long}</p></div>"
                ),
            ),
        ),
        (
            "positioned boxes",
            doc(
                "",
                "<div style=\"position: relative; width: 300px; height: 200px; left: 10px; top: 5px\">\
                 <div style=\"position: absolute; top: 20px; left: 30px; width: 50px; height: 50px\">abs</div>\
                 <div style=\"position: absolute; right: 0; bottom: 0; width: 40px; height: 40px\">br</div>\
                 <div style=\"height: 30px\">flow</div><div style=\"position: absolute\">static</div>\
                 <div style=\"position: relative; top: 10px; z-index: 2\">rel</div></div>\
                 <div style=\"position: fixed; top: 0; right: 0; width: 20px; height: 20px\">fixed</div>\
                 <div style=\"position: absolute; left: 50%; width: 10%; height: 10px; margin-left: \
                 -5%\"></div>\
                 <div style=\"position: sticky; top: 0\">sticky</div>\
                 <p>text <span style=\"position: relative; left: 5px; top: 3px\">moved inline</span></p>\
                 <div style=\"position: absolute; top: 10px; bottom: 10px; left: 10px; right: \
                 10px\">stretched</div>",
            ),
        ),
        (
            "overflow and scrolling",
            doc(
                "",
                "<div style=\"width: 200px; height: 100px; overflow: auto\"><div style=\"height: \
                 300px; width: 400px\">big content</div></div>\
                 <div style=\"width: 200px; height: 60px; overflow: scroll\">scroll always</div>\
                 <div style=\"width: 150px; height: 40px; overflow: hidden\"><p>hidden overflow \
                 text that is long and wraps over several lines in the box</p></div>\
                 <div style=\"width: 150px; overflow-x: auto; white-space: nowrap\">a long nowrap \
                 line in an overflow-x auto box that scrolls sideways</div>\
                 <div style=\"width: 100px; height: 50px; overflow: visible\"><div style=\"height: \
                 80px\">visible</div></div>\
                 <div style=\"height: 80px; overflow-y: scroll\"><p>a</p><p>b</p><p>c</p><p>d</p></div>",
            ),
        ),
        (
            "text-overflow",
            doc(
                "body { font-size: 14px; }",
                &format!(
                    "<p style=\"width: 120px; white-space: nowrap; overflow: hidden; text-overflow: \
                     ellipsis\">{long}</p>\
                     <p style=\"width: 120px; white-space: nowrap; overflow: hidden; text-overflow: \
                     clip\">{long}</p>\
                     <div style=\"width: 150px; overflow: hidden; text-overflow: ellipsis\">Two lines \
                     wrap and the long one overflowswithoutanybreakopportunityatall</div>"
                ),
            ),
        ),
        (
            "text decoration",
            doc(
                "",
                "<p style=\"text-decoration: underline\">underlined paragraph text that wraps</p>\
                 <p>plain <span style=\"text-decoration: line-through\">struck</span> <span \
                 style=\"text-decoration: overline\">over</span> <u>u</u> <s>s</s></p>\
                 <div style=\"text-decoration: underline\"><p>inherited decoration</p><span \
                 style=\"display: inline-block\">inline block</span></div>\
                 <p style=\"text-decoration: underline wavy red; text-underline-offset: 3px\">wavy</p>",
            ),
        ),
        (
            "writing modes",
            doc(
                "",
                "<div style=\"writing-mode: vertical-rl; height: 200px\">vertical rl text that \
                 wraps into columns</div>\
                 <div style=\"writing-mode: vertical-lr; height: 150px\"><div style=\"width: 30px; \
                 margin: 10px\">a</div><div style=\"width: 40px\">b</div></div>\
                 <div style=\"writing-mode: horizontal-tb\">horizontal</div>\
                 <div style=\"writing-mode: vertical-rl; text-orientation: upright; height: \
                 100px\">upright</div>\
                 <div style=\"writing-mode: vertical-rl\"><span style=\"text-combine-upright: \
                 all\">12</span>combine</div>",
            ),
        ),
        (
            "deep nesting",
            doc(
                "div { padding: 1px; } span { padding: 0 1px; }",
                &format!(
                    "{}{}{}",
                    nested(30, "<div>", "</div>", "deep text at the bottom of thirty blocks"),
                    nested(20, "<span>", "</span>", "deep inline"),
                    nested(
                        8,
                        "<div style=\"display: inline-block; border: 1px solid\">",
                        "</div>",
                        "ib"
                    )
                ),
            ),
        ),
        (
            "baselines and vertical alignment",
            doc(
                "",
                "<div><span style=\"font-size: 30px\">Big</span><span style=\"display: \
                 inline-block; font-size: 10px\">small<br/>two lines</span><span \
                 style=\"vertical-align: top\">top</span><span style=\"vertical-align: \
                 bottom\">bottom</span><span style=\"vertical-align: text-top\">tt</span><span \
                 style=\"vertical-align: -5px\">down</span></div>\
                 <div><span style=\"display: inline-block; height: 40px\"></span>empty ib</div>\
                 <div><span style=\"display: inline-block; overflow: hidden\">ovh ib</span>x</div>\
                 <div style=\"display: inline-table\"><div style=\"display: table-cell\">it</div></div>",
            ),
        ),
        (
            "initial letter, text-box-trim and replaced boxes",
            doc(
                "",
                "<p><span style=\"initial-letter: 3\">D</span>rop cap paragraph text that runs over \
                 several lines beside the initial letter box.</p>\
                 <p style=\"text-box-trim: trim-both; text-box-edge: cap alphabetic; border: 1px \
                 solid\">trimmed</p>\
                 <p><img width=\"50\" height=\"40\"/> image <img style=\"width: 30px; height: \
                 30px; vertical-align: middle\"/> inline</p>\
                 <div><img style=\"display: block; width: 100px; height: 20px; margin: auto\"/></div>\
                 <div contenteditable=\"true\"></div>",
            ),
        ),
        (
            "painting order, effects and backgrounds",
            doc(
                "",
                "<div style=\"border-radius: 8px; box-shadow: 2px 2px 4px black; background: \
                 linear-gradient(red, blue); width: 100px; height: 50px\"></div>\
                 <div style=\"opacity: 0.5; width: 50px; height: 20px; background: red\"></div>\
                 <div style=\"transform: rotate(10deg); width: 50px; height: 20px\">t</div>\
                 <div style=\"filter: blur(2px); width: 50px; height: 20px\">f</div>\
                 <div style=\"position: relative; z-index: -1\">behind</div>\
                 <div style=\"outline: 2px solid red; width: 40px\">outline</div>\
                 <div style=\"background: radial-gradient(red, blue); width: 40px; height: 40px\"></div>\
                 <div style=\"visibility: hidden\">invisible</div>",
            ),
        ),
        (
            "percentages and auto heights",
            "<html style=\"height: 100%\"><head><style>body { margin: 0; height: 100%; \
             font-family: 'Azul Mock Prop'; } .h { height: 50%; } </style></head><body>\
             <div class=\"h\"><div style=\"height: 100%\">full of half</div></div>\
             <div style=\"display: inline-block; height: 100%\">paper</div>\
             <div style=\"min-height: 30%\">min</div>\
             <div style=\"height: 100px\"><div style=\"height: 25%; margin-top: 10%\">q</div></div>\
             </body></html>"
                .to_string(),
        ),
    ]
}

#[test]
fn the_purpose_built_documents_lay_out_as_their_golden_dump() {
    let mut out = String::new();
    for (name, xml) in purpose_built() {
        let _ = writeln!(out, "=== {name}");
        parse_and_dump_in_window(&mut out, &xml, 800.0, 600.0);
    }
    matches_golden("purpose_built", &out);
}

// ---------------------------------------------------------------------------
// Paged media
// ---------------------------------------------------------------------------

fn paged_documents() -> Vec<(&'static str, String)> {
    let words = "wrap ".repeat(300);
    vec![
        (
            "a flat stack of blocks",
            doc(
                "* { margin: 0; padding: 0; } .p { height: 150px; }",
                "<div class=\"p\">one</div><div class=\"p\">two</div><div class=\"p\">three</div>\
                 <div class=\"p\">four</div><div class=\"p\">five</div><div class=\"p\">six</div>",
            ),
        ),
        (
            "a paragraph taller than a page",
            doc(
                "* { margin: 0; } body { line-height: 20px; width: 300px; }",
                &format!("<div>short</div><div>{words}</div>"),
            ),
        ),
        (
            "forced breaks",
            doc(
                ".b { break-before: page; } p { margin: 10px 0; }",
                "<p>first</p><p class=\"b\">after a forced break</p><p>third</p>\
                 <div><p>wrapped</p><p class=\"b\">forced inside a wrapper</p></div>",
            ),
        ),
        (
            "nested wrappers split and resume",
            doc(
                "* { margin: 0; } .p { height: 90px; border: 1px solid; }",
                "<div><div><div class=\"p\">a</div><div class=\"p\">b</div><div \
                 class=\"p\">c</div></div><div class=\"p\">d</div></div><div class=\"p\">e</div>",
            ),
        ),
        (
            "floats, tables and lists across pages",
            doc(
                "p { margin: 8px 0; }",
                &format!(
                    "<div style=\"float: left; width: 80px; height: 120px\">float</div><p>{words}</p>\
                     <table border=\"1\"><tr><td>a</td><td>b</td></tr><tr><td style=\"height: \
                     150px\">tall</td><td>c</td></tr><tr><td>d</td><td>e</td></tr></table>\
                     <ol><li>one</li><li>two</li><li style=\"height: 120px\">three</li><li>four</li></ol>"
                ),
            ),
        ),
        (
            "a monolith taller than a page",
            doc(
                "* { margin: 0; } .m { height: 500px; overflow: hidden; } .p { height: 20px; }",
                "<div class=\"m\">x</div><div class=\"p\">after</div>",
            ),
        ),
    ]
}

#[test]
fn the_paged_documents_lay_out_as_their_golden_dump() {
    let mut out = String::new();
    for (name, xml) in paged_documents() {
        let _ = writeln!(out, "=== {name} (slicer, 400 x 300 pages)");
        dump_paged_slicer(&mut out, &xml, LogicalSize::new(400.0, 300.0));
        let _ = writeln!(out, "=== {name} (break tokens, 400 x 200 pages)");
        dump_paged_tokens(&mut out, &xml, LogicalSize::new(400.0, 200.0));
    }
    matches_golden("paged", &out);
}

// ---------------------------------------------------------------------------
// Widget DOMs
// ---------------------------------------------------------------------------

fn widget_doms() -> Vec<(&'static str, Dom)> {
    let status_bar = StatusBar::new(
        vec![
            StatusBarSegment::new("Items: 12".into()),
            StatusBarSegment::new("A long status notice that does not fit its bar at all".into()),
        ]
        .into(),
    )
    .with_sync(StatusBarSync::create(
        "All folders are up to date".into(),
        StatusBarSyncKind::Connected,
    ))
    .with_zoom(StatusBarZoom::create(100.0, 10.0, 500.0))
    .dom();

    let group = |label: &str, buttons: &[&str]| {
        let mut g = RibbonGroup::new(label.into());
        for b in buttons {
            g = g.with_item(RibbonItem::LargeButton(RibbonButton::new(
                "layers".into(),
                (*b).into(),
            )));
        }
        g
    };
    let tab = RibbonTab::new("HOME".into())
        .with_group(group("Clipboard", &["Paste", "Cut", "Copy"]))
        .with_group(group("Font", &["Bold", "Italic", "Underline"]))
        .with_group(group("Editing", &["Find", "Replace", "Select"]));
    let ribbon = Ribbon::new(vec![tab].into()).dom();

    let columns = StringVec::from_vec(
        ["Name", "Size", "Modified"]
            .iter()
            .map(|s| AzString::from(*s))
            .collect::<Vec<_>>(),
    );
    let rows = (0..6)
        .map(|i| ListViewRow {
            cells: DomVec::from_vec(vec![
                Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(format!(
                    "file {i}.txt"
                ))),
                Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from(format!(
                    "{} KB",
                    i * 7
                ))),
                Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from("today")),
            ]),
            height: None.into(),
        })
        .collect::<Vec<_>>();
    let list_view = ListView::create(columns).with_rows(rows.into()).dom();

    vec![
        ("status bar", status_bar),
        ("ribbon", ribbon),
        ("list view", list_view),
    ]
}

#[test]
fn the_widget_doms_lay_out_as_their_golden_dump() {
    let mut out = String::new();
    for (name, widget) in widget_doms() {
        let _ = writeln!(out, "=== {name}");
        let mut dom = Dom::create_body()
            .with_css("margin: 0px; font-family: 'Azul Mock Prop';")
            .with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; width: 640px;")
                    .with_child(widget),
            );
        let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        dump_in_window(&mut out, styled, 1000.0, 400.0);
    }
    matches_golden("widgets", &out);
}
