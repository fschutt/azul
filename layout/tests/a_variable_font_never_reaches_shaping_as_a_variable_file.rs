//! A variable face a CSS family resolves to is never drawn as the variable file: shaping, glyph
//! decode and the PDF writer see an ordinary STATIC face (no `fvar`, no `gvar`) at the exact
//! coordinates, the default instance included.
//!
//! Before, a regular request drew a system FILE as it was (the raw variable font, at its default
//! instance, in the shaper and in the PDF) and a face registered in memory was cut into nine
//! static faces by weight bucket at registration. A static face is untouched.
//!
//! The three ways a face enters a chain: A, registered from memory by family name
//! (`FontManager::register_named_font`; printpdf's `register_input_fonts`), B, a font FILE found
//! by the system scan, and C, registered at runtime (`RegisteredFonts`, unit-tested in
//! `text3/fontreg_test.rs`, where its `ensure_in` is visible).

use rust_fontconfig::FcFontCache;

use crate::variable_fonts::{
    bytes_of, cache_with_file, chain_through, coverage_of, drawn_bytes, instance_bytes,
    is_variable, red_hat_mono_path, static_face_path, Manager,
};

/// B: a variable FILE, as the system scan registers `SFNS.ttf`.
#[test]
fn a_variable_file_is_drawn_as_a_static_instance_at_every_weight() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    assert!(is_variable(&file), "premise: the file is a variable font");
    for (css, wght) in [
        ("font-weight: 400", None),
        ("font-weight: bold", Some(700.0)),
        ("", None),
    ] {
        let drawn = drawn_bytes(&cache, &family, css);
        assert!(!is_variable(&drawn), "`{css}`: the face drawn is still a variable font");
        let expected = instance_bytes(&file, &wght.map_or(Vec::new(), |w| vec![(*b"wght", w)]));
        assert!(
            drawn == expected,
            "`{css}`: the face drawn is not the instance of the file at {wght:?}"
        );
    }
}

/// A: registered from memory, once, as the raw variable font.
#[test]
fn a_variable_font_registered_in_memory_is_drawn_as_a_static_instance() {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let coverage = coverage_of(&file);
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    let _id = manager.register_named_font("Azul Memory Variable", &file, coverage);
    for (css, wght) in [("font-weight: 400", None), ("font-weight: 700", Some(700.0))] {
        let chain = chain_through(&manager, "Azul Memory Variable", css);
        let drawn = bytes_of(&manager.fc_cache, &chain);
        assert!(!is_variable(&drawn), "`{css}`: the face drawn is still a variable font");
        let expected = instance_bytes(&file, &wght.map_or(Vec::new(), |w| vec![(*b"wght", w)]));
        assert!(drawn == expected, "`{css}`: not the instance of the file at {wght:?}");
    }
}

/// A static face is drawn as the file, untouched, whatever is asked of it.
#[test]
fn a_static_face_is_drawn_as_the_file() {
    let (cache, family, _source, file) = cache_with_file(&static_face_path());
    for css in ["", "font-weight: 700", "font-weight: 350", "font-stretch: 80%"] {
        assert!(
            drawn_bytes(&cache, &family, css) == file,
            "`{css}`: a static face must be drawn as the file"
        );
    }
}

/// B as the desktop meets it: the variable font is an INSTALLED file, found by the registry's scan,
/// and the chain is resolved by the registry-backed resolver every window uses
/// (`resolve_font_chains_fast`), which makes the instances in the registry's own cache.
#[test]
fn an_installed_variable_font_is_drawn_as_a_static_instance_through_the_registry() {
    use std::collections::{BTreeSet, HashMap};

    use azul_layout::{
        solver3::getters::{resolve_font_chains_fast, CollectedFontStacks},
        text3::cache::{FontRequest, FontSelector},
    };

    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("azul-variable-through-the-registry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a directory of installed fonts");
    std::fs::write(dir.join("RedHatMono-VariableFont_wght.ttf"), &file).expect("an installed font");
    let registry = rust_fontconfig::registry::FcFontRegistry::new_with_configs(
        rust_fontconfig::config::FcScanConfig {
            font_dirs: vec![dir.clone()],
            priority_families: Vec::new(),
        },
        rust_fontconfig::FcFallbackConfig::empty(),
    );
    registry.spawn_scout_and_builders();
    registry.wait_for_scout();

    let used: BTreeSet<char> = ['a', 'b'].into_iter().collect();
    for (weight, coordinates) in [
        (400.0, Vec::new()),
        (650.0, vec![(*b"wght", 650.0)]),
        (700.0, vec![(*b"wght", 700.0)]),
        (1000.0, vec![(*b"wght", 700.0)]),
    ] {
        let collected = CollectedFontStacks {
            font_stacks: vec![vec![FontSelector {
                family: "Red Hat Mono".to_string(),
                request: FontRequest::default().with_weight(weight),
                ..FontSelector::default()
            }]],
            hash_to_index: HashMap::new(),
            font_refs: HashMap::new(),
        };
        let resolved = resolve_font_chains_fast(
            &collected,
            &registry,
            &used,
            &HashMap::new(),
            &azul_layout::settings::TextSettings::default(),
        );
        let chain = resolved.chains.values().next().expect("the stack's chain");
        let face = chain
            .css_fallbacks
            .iter()
            .find(|group| group.css_name == "Red Hat Mono")
            .and_then(|group| group.fonts.first())
            .expect("the installed family matched")
            .id;
        let drawn = registry
            .shared_cache()
            .get_font_bytes(&face)
            .expect("the face drawn has bytes")
            .as_slice()
            .to_vec();
        assert!(!is_variable(&drawn), "weight {weight}: the face drawn is still a variable font");
        assert!(
            drawn == instance_bytes(&file, &coordinates),
            "weight {weight}: not the instance of the file at {coordinates:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A face with CFF2 outlines (Cantarell, `wght` 100 to 800) is made static the same way: no `fvar`,
/// the bytes of the instance of the file at the weight asked for (clamped to the axis).
#[test]
fn a_cff2_variable_font_is_drawn_as_a_static_instance() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/Cantarell-VF.otf");
    let (cache, family, _source, file) = cache_with_file(&path);
    assert!(is_variable(&file), "premise: the file is a variable font");
    for (css, coordinates) in [
        ("font-weight: 400", Vec::new()),
        ("font-weight: 100", vec![(*b"wght", 100.0)]),
        ("font-weight: 650", vec![(*b"wght", 650.0)]),
        ("font-weight: 900", vec![(*b"wght", 800.0)]),
    ] {
        let drawn = drawn_bytes(&cache, &family, css);
        assert!(!is_variable(&drawn), "`{css}`: the face drawn is still a variable font");
        assert!(
            drawn == instance_bytes(&file, &coordinates),
            "`{css}`: not the instance of the file at {coordinates:?}"
        );
        // what azul's own parser makes of the instance, for the record
        let parsed = azul_layout::font::parsed::ParsedFont::from_bytes(&drawn, 0, &mut Vec::new());
        eprintln!(
            "[instance] Cantarell-VF.otf `{css}`: {} bytes, parses as a face: {}",
            drawn.len(),
            parsed.is_some()
        );
    }
}
