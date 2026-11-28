//! E75: the two chain resolvers over the machine's own fonts, for stacks and texts like the demo apps'.
//!
//! The window's font manager has no registry in production (`FontManager::with_registry` is called by tests only), so every
//! layout resolves its chains with the pattern-map resolver over the registry's shared cache; the registry's fast resolver
//! (`resolve_font_chains_fast`: cmap probes of the candidate files, "128 ms to 5 ms" in the comment that introduced it) is
//! used by tests alone. This test runs BOTH through the one entry the layout uses (`collect_and_resolve_font_chains_covering`)
//! on a registry whose scan is complete, for a list of stacks and texts (Latin, accents, Greek, Cyrillic, CJK, Arabic,
//! Hebrew, Devanagari, Thai, emoji, symbols), and prints for each pair: how long each took (the first call, which parses
//! the files it probes, and the median of the next four) and the characters whose face is not the same file in the two
//! chains. It asserts nothing about speed; it asserts that Basic Latin text is drawn by the same file in both.
//!
//! It needs a machine with fonts and prints a table: `cargo test --release -p azul-layout --test all -- --ignored
//! --nocapture the_fast_and_the_slow_chain_resolver`.

use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

use azul_css::props::basic::PhysicalSize;
use azul_layout::{
    solver3::getters::{collect_and_resolve_font_chains_covering, ResolvedFontChains},
    text3::cache::{covering_font, FontChainKeyOrRef},
};
use rust_fontconfig::{registry::FcFontRegistry, FcFontCache, FontId, OwnedFontSource};

use crate::variable_fonts::{page, Manager};

/// The stacks of the demo apps and of the web, as `font-family` values.
const STACKS: &[(&str, &str)] = &[
    ("system-ui", "system-ui"),
    ("sans-serif", "sans-serif"),
    ("serif", "serif"),
    ("monospace", "monospace"),
    ("flora hand", "\"EB Garamond\", Georgia, \"Times New Roman\", serif"),
    ("web stack", "-apple-system, BlinkMacSystemFont, \"Segoe UI\", Roboto, \"Helvetica Neue\", Arial, sans-serif"),
    ("code stack", "\"SF Mono\", Menlo, Consolas, \"DejaVu Sans Mono\", monospace"),
];

/// The texts: one script or kind each, and a mixed one.
const TEXTS: &[(&str, &str)] = &[
    ("ascii", "The quick brown fox jumps over the lazy dog 0123456789 .,;:!?()[]{}"),
    ("latin-1", "Crème brûlée à la façon de l'été, naïve Zoë, Åsa, Øre, ß, ÿ"),
    ("greek", "Η γρήγορη καφέ αλεπού πηδάει πάνω από το τεμπέλικο σκυλί"),
    ("cyrillic", "Съешь же ещё этих мягких французских булок, да выпей чаю"),
    ("cjk", "漢字かなカナ 한국어 中文字体测试 日本語のテキスト"),
    ("arabic", "نص عربي للاختبار مع بعض الكلمات"),
    ("hebrew", "טקסט בעברית לבדיקה עם כמה מילים"),
    ("devanagari", "देवनागरी लिपि में एक परीक्षण वाक्य"),
    ("thai", "ข้อความภาษาไทยสำหรับทดสอบการแสดงผล"),
    ("emoji", "😀 🎉 👍 ❤️ 🚀 🌍 ✅ ❌"),
    ("symbols", "→ ← ↑ ↓ ✓ ✗ ★ ☆ • … © ® ™ ± × ÷ ≤ ≥ ≠ ∞ ∑ √ ½ ¼ € £ ¥"),
    ("mixed", "Hello 世界 مرحبا Привет नमस्ते 😀 → ✓"),
];

/// How many starts of each resolver a case makes: the first parses what the probe reads, the rest are warm.
const REPEATS: usize = 5;

fn registry_with_the_machines_fonts() -> std::sync::Arc<FcFontRegistry> {
    let registry = FcFontRegistry::new();
    registry.set_persist_on_complete(false);
    registry.spawn_scout_and_builders();
    let deadline = Instant::now() + Duration::from_secs(180);
    while !registry.is_build_complete() {
        assert!(Instant::now() < deadline, "the scan of the machine's fonts did not complete in three minutes");
        std::thread::sleep(Duration::from_millis(20));
    }
    registry
}

/// The file and the face index a chain draws `ch` with, `None` when no face of the chain covers it.
fn drawn_by(cache: &FcFontCache, resolved: &ResolvedFontChains, ch: char) -> Option<String> {
    let chain = resolved.chains.iter().find_map(|(key, chain)| match key {
        FontChainKeyOrRef::Chain(_) => Some(chain),
        FontChainKeyOrRef::Ref(_) => None,
    })?;
    let id: FontId = covering_font(chain, ch)?;
    Some(match cache.get_font_by_id(&id)? {
        OwnedFontSource::Disk(path) => format!("{}#{}", path.path.rsplit('/').next().unwrap_or(&path.path), path.font_index),
        OwnedFontSource::Memory(_) => "memory".to_string(),
    })
}

fn timed(mut run: impl FnMut() -> ResolvedFontChains) -> (ResolvedFontChains, f64, f64) {
    let mut times = Vec::new();
    let mut last = None;
    for _ in 0..REPEATS {
        let started = Instant::now();
        let resolved = run();
        times.push(started.elapsed().as_secs_f64() * 1000.0);
        last = Some(resolved);
    }
    let first = times[0];
    let mut rest = times[1..].to_vec();
    rest.sort_by(f64::total_cmp);
    (last.expect("one run"), first, rest[rest.len() / 2])
}

#[test]
#[ignore = "measures and prints on the machine's own fonts: run it with --ignored --nocapture"]
fn the_fast_and_the_slow_chain_resolver_on_the_machines_fonts() {
    let registry = registry_with_the_machines_fonts();
    let platform = azul_css::system::Platform::current();
    let viewport = PhysicalSize::new(800.0, 600.0);
    println!(
        "[E75] {} faces in the registry's cache, {} cores",
        registry.shared_cache().list().len(),
        std::thread::available_parallelism().map_or(0, std::num::NonZeroUsize::get)
    );
    println!("[E75] stack | text | slow first/median ms | fast first/median ms | chars | chars drawn by another file | first of them");

    let mut latin_differences = Vec::new();
    for (stack_name, stack) in STACKS {
        for (text_name, text) in TEXTS {
            let styled = page("x", "", &format!("font-family: {stack};"), text);
            let also = BTreeSet::new();
            // THE SLOW PATH: what production does. A manager over the registry's shared cache, no registry attached.
            let slow = Manager::new(registry.shared_cache()).expect("font manager");
            let (slow_chains, slow_first, slow_median) = timed(|| {
                collect_and_resolve_font_chains_covering(&styled, &slow.fc_cache, &slow, &platform, viewport, &also)
            });
            // THE FAST PATH: the same manager with the registry attached.
            let fast = Manager::new(registry.shared_cache())
                .expect("font manager")
                .with_registry(std::sync::Arc::clone(&registry));
            let (fast_chains, fast_first, fast_median) = timed(|| {
                collect_and_resolve_font_chains_covering(&styled, &fast.fc_cache, &fast, &platform, viewport, &also)
            });

            let chars: BTreeSet<char> = text.chars().filter(|c| !c.is_whitespace() && !c.is_control()).collect();
            let cache = registry.shared_cache();
            let mut differing = Vec::new();
            for ch in &chars {
                let a = drawn_by(&cache, &slow_chains, *ch);
                let b = drawn_by(&cache, &fast_chains, *ch);
                if a != b {
                    differing.push(format!("{ch} slow {a:?} fast {b:?}"));
                }
            }
            println!(
                "[E75] {stack_name} | {text_name} | {slow_first:.1}/{slow_median:.1} | {fast_first:.1}/{fast_median:.1} | {} | {} | {}",
                chars.len(),
                differing.len(),
                differing.first().map_or_else(String::new, Clone::clone)
            );
            if *text_name == "ascii" && !differing.is_empty() {
                latin_differences.push(format!("{stack_name}: {differing:?}"));
            }
        }
    }
    assert!(
        latin_differences.is_empty(),
        "Basic Latin is drawn by another file in the two resolvers' chains: {latin_differences:#?}"
    );
}
