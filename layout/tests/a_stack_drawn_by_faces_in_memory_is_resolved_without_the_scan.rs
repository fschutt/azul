//! The chains of a DOM the faces in memory draw are resolved without the registry's scan of the system's fonts.
//!
//! The registry's probe (`FcFontRegistry::request_fonts_fast`) answers from the files the scout has listed, and waits for
//! a first match when it has listed none, half a second at most - where the scout lists everything in one batch at the
//! end (iOS, through CoreText) that is half a second on every stack of the first layout. The shell's first layout does
//! not wait for the scan when the DOM's text is drawn by faces in memory (`memory_faces_draw_all_text`), and the resolver
//! must not wait for it in the probe either: `resolve_font_chains_fast` resolves a stack those faces draw from the faces,
//! while the registry has no manifest to go by and has not listed the fonts. A stack they do not draw (a child DOM the
//! shell did not see, text that came later in the pass) waits for the listing first, as the shell's wait would have.
//!
//! The registry here is one whose scan is NOT STARTED: it has nothing to list and no manifest, so any wait is the
//! resolver's.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use azul_layout::{
    solver3::getters::{
        collect_font_stacks_from_styled_dom, collect_used_codepoints_all, resolve_font_chains_fast,
        ResolvedFontChains,
    },
    text3::cache::{faces_covering, FontChainKey, FontChainKeyOrRef, FontRequest, MemoryFontTier},
};
use rust_fontconfig::{
    config::FcScanConfig, registry::FcFontRegistry, FcFontCache, FontFallbackChain, UnicodeRange,
};

use crate::variable_fonts::{page, static_face_path, Manager};

/// The probe's own cap on the wait for a first match is 500 ms: a resolution that took less than this did not make it.
const BELOW_THE_PROBES_WAIT: Duration = Duration::from_millis(300);

/// A registry with no directory to scan and no manifest, whose scan is not started.
fn idle_registry() -> Arc<FcFontRegistry> {
    let registry = FcFontRegistry::new_with_config(FcScanConfig {
        font_dirs: Vec::new(),
        priority_families: Vec::new(),
    });
    registry.set_persist_on_complete(false);
    registry
}

/// A manager with the face `family` registered from memory in the fallback tier (the tier of azul's own faces and the
/// app's) with the Basic Latin block as its coverage. The face is a static one (a variable face would be drawn as an
/// instance made on the way, and the timings below are those of the wait).
fn manager_with_latin_face(family: &str) -> Manager {
    let file = std::fs::read(static_face_path()).expect("the face is checked in");
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    let latin = vec![UnicodeRange { start: 0x20, end: 0x7e }];
    let _ = manager.register_named_font_in_tier(family, &file, latin, MemoryFontTier::Fallback);
    manager
}

fn resolve(manager: &Manager, registry: &FcFontRegistry, css: &str, text: &str) -> ResolvedFontChains {
    let styled = page("Mem", "", css, text);
    let collected = collect_font_stacks_from_styled_dom(&styled, &azul_css::system::Platform::current());
    let chars = collect_used_codepoints_all(&styled);
    resolve_font_chains_fast(&collected, registry, &chars, &manager.memory_families, &manager.settings)
}

/// The names of the CSS groups of the one chain of `resolved` that has the group `family`, in order.
fn group_names(resolved: &ResolvedFontChains, family: &str) -> Vec<String> {
    resolved
        .chains
        .iter()
        .find_map(|(key, chain)| match key {
            FontChainKeyOrRef::Chain(key) if key.font_families.first().map(String::as_str) == Some(family) => Some(
                chain
                    .css_fallbacks
                    .iter()
                    .map(|g| g.css_name.trim_matches('"').to_string())
                    .collect(),
            ),
            _ => None,
        })
        .expect("a chain for the stack")
}

#[test]
fn a_stack_the_faces_in_memory_draw_does_not_wait_for_the_probe() {
    let registry = idle_registry();
    let manager = manager_with_latin_face("Mem");
    // The design's own stack: its face, then the serifs a desktop has - names the probe would be asked for.
    let started = Instant::now();
    let resolved = resolve(
        &manager,
        &registry,
        "font-family: \"Mem\", \"Hoefler Text\", Georgia, serif",
        "Hello, world 123",
    );
    let took = started.elapsed();
    assert!(
        took < BELOW_THE_PROBES_WAIT,
        "the resolution took {took:?}: the probe waited for a listing that has not started"
    );
    assert_eq!(group_names(&resolved, "Mem").first().map(String::as_str), Some("Mem"));
    assert!(!registry.is_scan_complete(), "premise: the scan has not run");
}

#[test]
fn a_stack_they_do_not_draw_waits_for_the_scan_to_list_the_fonts() {
    let registry = idle_registry();
    let manager = manager_with_latin_face("Mem");
    // Another thread starts the scan after a while: what the resolution is waiting for.
    let late = {
        let registry = Arc::clone(&registry);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            registry.spawn_scout_and_builders();
        })
    };
    let started = Instant::now();
    // One character the face does not draw (a CJK ideograph): the stack needs the system's fonts.
    let _ = resolve(&manager, &registry, "font-family: \"Mem\", serif", "Hello \u{6f22}");
    let took = started.elapsed();
    late.join().expect("the late start");
    assert!(
        took >= Duration::from_millis(200),
        "the resolution took {took:?}: it did not wait for the listing of the system's fonts"
    );
    assert!(registry.is_scan_complete(), "the listing was done when the resolution went on");
}

#[test]
fn a_stack_that_names_no_registered_family_waits_for_the_scan_too() {
    let registry = idle_registry();
    let manager = manager_with_latin_face("Mem");
    let late = {
        let registry = Arc::clone(&registry);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            registry.spawn_scout_and_builders();
        })
    };
    let started = Instant::now();
    let _ = resolve(&manager, &registry, "font-family: \"Some Installed Family\"", "Hello");
    let took = started.elapsed();
    late.join().expect("the late start");
    assert!(took >= Duration::from_millis(200), "the resolution took {took:?}");
    assert!(registry.is_scan_complete());
}

#[test]
fn a_character_no_face_of_the_chain_draws_waits_for_the_listing_before_its_face_is_looked_up() {
    // The shaper asks `faces_covering` for the characters its chain does not draw (generated content, the result of a
    // `text-transform`): the characters the shell's rule did not see.
    let registry = idle_registry();
    let late = {
        let registry = Arc::clone(&registry);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(250));
            registry.spawn_scout_and_builders();
        })
    };
    let families = vec!["Mem".to_string()];
    let key = FontChainKey {
        font_families: families.clone(),
        request: FontRequest::default(),
    };
    let chain = FontFallbackChain::empty(&families);
    let started = Instant::now();
    let _ = faces_covering(
        &key,
        &chain,
        std::iter::once('\u{6f22}').collect(),
        &FcFontCache::default(),
        Some(&registry),
        &|_, _| None,
    );
    let took = started.elapsed();
    late.join().expect("the late start");
    assert!(took >= Duration::from_millis(200), "the lookup took {took:?}: it did not wait for the listing");
    assert!(registry.is_scan_complete(), "the listing was done when the lookup went on");
}
