//! The reftest suite: a test page and its reference must paint the same.
//!
//! Both pages come from `tests/wpt/normalized/` (strict XHTML written by
//! `scripts/refci/vendor_wpt.py`) and go through the same path an app's HTML
//! takes: `azul_layout::xml::parse_xml_to_styled_dom` (the document loader)
//! -> an 800 x 600 `LayoutWindow` -> the CPU renderer. Test and
//! reference are rendered by the same engine with the same fonts, so the
//! comparison holds on every OS and needs no browser.
//!
//! WPT semantics (docs/writing-tests/reftests.md and wptrunner's
//! `check_pass`): a test passes when at least one `match` reference matches
//! and every `mismatch` reference does not; two renders match when both the
//! largest per-channel difference and the number of differing pixels are
//! zero, or fall inside the page's `fuzzy` ranges. A `crash` test (a local
//! `*-crash.html`) passes when it renders at all.

use std::{
    collections::{BTreeMap, HashMap},
    panic::{catch_unwind, AssertUnwindSafe},
    path::Path,
    time::Instant,
};

use azul_core::{dom::DomId, geom::LogicalSize, resources::RendererResources};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    cpurender::{self, AzulPixmap, RenderOptions},
    glyph_cache::GlyphCache,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

use crate::expect::{self, CaseResult, Expectations, Outcome};

/// The WPT reftest viewport.
const WIDTH: f32 = 800.0;
const HEIGHT: f32 = 600.0;

struct Link {
    kind: String,
    reference: String,
    /// `(maxDifference, totalPixels)` inclusive ranges; `None` = exact.
    fuzzy: Option<((u64, u64), (u64, u64))>,
}

struct Test {
    path: String,
    gap: String,
    links: Vec<Link>,
}

pub fn run(root: &Path, out: &Path, filter: Option<&str>) -> bool {
    let started = Instant::now();
    let manifest = root.join("reftests.tsv");
    let tests = match read_manifest(&manifest) {
        Ok(t) => t,
        Err(e) => {
            println!("wpt reftest: cannot read {}: {e}", manifest.display());
            return false;
        }
    };
    let tests: Vec<Test> = tests
        .into_iter()
        .filter(|t| filter.map_or(true, |f| t.path.contains(f)))
        .collect();
    if tests.is_empty() {
        println!("wpt reftest: no test selected (filter {filter:?})");
        return filter.is_some();
    }
    let exp = Expectations::load(&root.join("reftest_expectations.txt"));
    let diffs = out.join("reftest_diffs");
    let _ = std::fs::create_dir_all(&diffs);

    // One font scan for the whole run; each page gets a fresh window.
    let fonts = FcFontCache::build();
    let normalized = root.join("normalized");
    // References shared by several tests (the green squares) render once.
    let mut uses: HashMap<&str, usize> = HashMap::new();
    for t in &tests {
        for l in &t.links {
            *uses.entry(l.reference.as_str()).or_default() += 1;
        }
    }
    let mut shared: HashMap<String, Result<AzulPixmap, String>> = HashMap::new();

    let mut results = Vec::with_capacity(tests.len());
    for t in &tests {
        let (outcome, detail) = evaluate(t, &normalized, &fonts, &uses, &mut shared, &diffs);
        results.push(CaseResult {
            id: t.path.clone(),
            outcome,
            gap: t.gap.clone(),
            detail,
        });
    }
    expect::gate("reftest", &results, &exp, out, started)
}

fn evaluate(
    t: &Test,
    normalized: &Path,
    fonts: &FcFontCache,
    uses: &HashMap<&str, usize>,
    shared: &mut HashMap<String, Result<AzulPixmap, String>>,
    diffs: &Path,
) -> (Outcome, String) {
    let test = match render_page(fonts, &normalized.join(&t.path)) {
        Ok(p) => p,
        Err(e) => return (Outcome::Error, format!("test: {e}")),
    };
    if t.links.iter().all(|l| l.kind == "crash") {
        return (Outcome::Pass, "rendered".to_string());
    }

    let mut any_match: Option<bool> = None;
    let mut all_mismatch = true;
    let mut details = Vec::new();
    for link in &t.links {
        let reference_owned;
        let reference: &AzulPixmap = if uses.get(link.reference.as_str()).copied().unwrap_or(0) > 1
        {
            let entry = shared
                .entry(link.reference.clone())
                .or_insert_with(|| render_page(fonts, &normalized.join(&link.reference)));
            match entry {
                Ok(p) => &*p,
                Err(e) => return (Outcome::Error, format!("reference {}: {e}", link.reference)),
            }
        } else {
            match render_page(fonts, &normalized.join(&link.reference)) {
                Ok(p) => {
                    reference_owned = p;
                    &reference_owned
                }
                Err(e) => return (Outcome::Error, format!("reference {}: {e}", link.reference)),
            }
        };
        let d = cpurender::pixel_diff(reference, &test, 0);
        let equal =
            d.dimensions_match && renders_equal(u64::from(d.max_delta), d.diff_count, link.fuzzy);
        details.push(format!(
            "{} {}: maxdiff {} pixels {}",
            link.kind,
            short(&link.reference),
            d.max_delta,
            d.diff_count
        ));
        let wrong = if link.kind == "mismatch" {
            equal
        } else {
            !equal
        };
        if wrong {
            write_diff(diffs, &t.path, &test, reference);
        }
        if link.kind == "mismatch" {
            all_mismatch &= !equal;
        } else {
            any_match = Some(any_match.unwrap_or(false) || equal);
        }
    }
    let pass = any_match.unwrap_or(true) && all_mismatch;
    (
        if pass { Outcome::Pass } else { Outcome::Fail },
        details.join("; "),
    )
}

/// wptrunner's `check_pass` for one test/reference pair.
fn renders_equal(max_delta: u64, pixels: u64, fuzzy: Option<((u64, u64), (u64, u64))>) -> bool {
    match fuzzy {
        None => max_delta == 0 && pixels == 0,
        Some(((d0, d1), (p0, p1))) => {
            (pixels == 0 && p0 == 0)
                || (max_delta == 0 && d0 == 0)
                || ((d0..=d1).contains(&max_delta) && (p0..=p1).contains(&pixels))
        }
    }
}

/// Parse, lay out and paint one page; a panic is an error, not a crash of
/// the run (the AzMail receipt panicked the renderer).
fn render_page(fonts: &FcFontCache, path: &Path) -> Result<AzulPixmap, String> {
    let xml = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let fonts = fonts.clone();
    match catch_unwind(AssertUnwindSafe(move || render_xml(fonts, &xml))) {
        Ok(r) => r,
        Err(payload) => Err(format!("panic: {}", panic_message(&payload))),
    }
}

fn render_xml(fonts: FcFontCache, xml: &str) -> Result<AzulPixmap, String> {
    // The document loader (what `azul-doc reftest` and the debug server's
    // `mount` use): it keeps the `<html>` element with its attributes - the
    // tree loader (`parse_xml` -> `dom_from_parsed_xml`) re-creates a bare
    // `<html>` and loses e.g. the reference's `<html style="background:
    // green">` (background-color-body-propagation-ref).
    let styled =
        azul_layout::xml::parse_xml_to_styled_dom(xml).map_err(|e| format!("parse: {e}"))?;

    let mut lw = LayoutWindow::new(fonts).map_err(|e| format!("window: {e:?}"))?;
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(WIDTH, HEIGHT);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = None;
    lw.layout_and_generate_display_list(styled, &ws, &rr, &sc, &mut dbg)
        .map_err(|e| format!("layout: {e:?}"))?;
    let result = lw
        .get_layout_result(&DomId::ROOT_ID)
        .ok_or_else(|| "no layout result".to_string())?;
    let mut glyphs = GlyphCache::new();
    cpurender::render_with_font_manager(
        &result.display_list,
        &rr,
        &lw.font_manager,
        RenderOptions {
            width: WIDTH,
            height: HEIGHT,
            dpi_factor: 1.0,
        },
        &mut glyphs,
    )
    .map_err(|e| format!("render: {e}"))
}

pub fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "(non-string panic payload)".to_string()
    }
}

fn short(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// `<test> | <reference> | <difference>` of the region that differs (plus a
/// margin), as one PNG: red where the two differ, a faded test elsewhere.
fn write_diff(dir: &Path, test_path: &str, test: &AzulPixmap, reference: &AzulPixmap) {
    if test.width != reference.width || test.height != reference.height {
        return;
    }
    let (w, h) = (test.width as usize, test.height as usize);
    let (t, r) = (test.data(), reference.data());
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            if t[i..i + 3] != r[i..i + 3] {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 {
        // Equal pixels (a mismatch test): show the whole page.
        (x0, y0, x1, y1) = (0, 0, w - 1, h - 1);
    }
    const MARGIN: usize = 16;
    const GUTTER: usize = 4;
    let (x0, y0) = (x0.saturating_sub(MARGIN), y0.saturating_sub(MARGIN));
    let (x1, y1) = ((x1 + MARGIN).min(w - 1), (y1 + MARGIN).min(h - 1));
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    let out_w = cw * 3 + GUTTER * 2;
    let Some(mut img) = AzulPixmap::new(out_w as u32, ch as u32) else {
        return;
    };
    {
        let data = img.data_mut();
        for y in 0..ch {
            for x in 0..cw {
                let src = ((y0 + y) * w + (x0 + x)) * 4;
                let differs = t[src..src + 3] != r[src..src + 3];
                for (panel, px) in [(0usize, &t[src..src + 4]), (1, &r[src..src + 4])] {
                    let dst = (y * out_w + panel * (cw + GUTTER) + x) * 4;
                    data[dst..dst + 4].copy_from_slice(px);
                }
                let dst = (y * out_w + 2 * (cw + GUTTER) + x) * 4;
                let px: [u8; 4] = if differs {
                    [255, 0, 0, 255]
                } else {
                    let f = |c: u8| 170 + c / 3;
                    [f(t[src]), f(t[src + 1]), f(t[src + 2]), 255]
                };
                data[dst..dst + 4].copy_from_slice(&px);
            }
            for g in [cw, 2 * cw + GUTTER] {
                for k in 0..GUTTER {
                    let dst = (y * out_w + g + k) * 4;
                    data[dst..dst + 4].copy_from_slice(&[128, 0, 128, 255]);
                }
            }
        }
    }
    if let Ok(png) = img.encode_png() {
        let name = test_path.replace(['/', '\\'], "__");
        let _ = std::fs::write(dir.join(format!("{name}.png")), png);
    }
}

fn read_manifest(path: &Path) -> Result<Vec<Test>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut tests: BTreeMap<String, Test> = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        if line.starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        let (path, kind, reference, fuzzy, gap) = match cols.as_slice() {
            [p, k, r, f, g] => (*p, *k, *r, *f, *g),
            _ => return Err(format!("line {}: expected 5 tab-separated columns", n + 1)),
        };
        let t = tests.entry(path.to_string()).or_insert_with(|| Test {
            path: path.to_string(),
            gap: gap.to_string(),
            links: Vec::new(),
        });
        t.links.push(Link {
            kind: kind.to_string(),
            reference: reference.to_string(),
            fuzzy: parse_fuzzy(fuzzy),
        });
    }
    Ok(tests.into_values().collect())
}

/// `a-b;c-d` (maxDifference;totalPixels, inclusive), or `-` for exact.
fn parse_fuzzy(s: &str) -> Option<((u64, u64), (u64, u64))> {
    let range = |r: &str| -> Option<(u64, u64)> {
        let (lo, hi) = r.split_once('-').unwrap_or((r, r));
        Some((lo.trim().parse().ok()?, hi.trim().parse().ok()?))
    };
    let (d, p) = s.split_once(';')?;
    Some((range(d)?, range(p)?))
}
