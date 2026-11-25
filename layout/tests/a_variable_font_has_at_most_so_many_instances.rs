//! A variable font file is drawn through a bounded number of static instances.
//!
//! The cache (`FcFontCache`, rust-fontconfig 5.2) makes one instance per canonical set of
//! coordinates, keyed by the CONTENT of the face, and keeps at most `FC_MAX_INSTANCES_PER_FACE` of
//! them per face: a request beyond that is drawn by the instance NEAREST to it (an id nothing
//! unregisters, so no run holds a dead face). The instances are faces by id only - no query, no
//! list finds them - and live and die with the cache and its clones.
//!
//! Before, a variable font registered from memory was cut into one face per weight bucket at
//! registration (and again for every spelling of its family), all of them visible to every query.

use azul_layout::{
    solver3::getters::select_variable_instances,
    text3::cache::FontRequest,
};
use rust_fontconfig::{
    FcFontCache, FontFallbackChain, FontId, FontMatch, FC_MAX_INSTANCES_PER_FACE,
};

use crate::variable_fonts::{
    cache_with_file, chain_through, coverage_of, first_face, instance_bytes, mock_variable_path,
    red_hat_mono_path, Manager,
};

/// A chain with one CSS group holding `source`, for `select_variable_instances` to look at.
fn chain_of_source(source: FontId) -> FontFallbackChain {
    let mut chain = FontFallbackChain::empty(&["X".to_string()]);
    chain.css_fallbacks.push(rust_fontconfig::CssFallbackGroup {
        css_name: "X".to_string(),
        fonts: vec![FontMatch {
            id: source,
            unicode_ranges: Vec::new(),
            fallbacks: Vec::new(),
        }],
        script_fonts: Vec::new(),
    });
    chain
}

/// The face the chain of `source` draws for `request`.
fn face_for(cache: &FcFontCache, source: FontId, request: &FontRequest) -> FontId {
    let mut chain = chain_of_source(source);
    select_variable_instances(&mut chain, request, cache);
    first_face(&chain)
}

#[test]
fn a_hundred_weights_make_at_most_the_bound_of_instances() {
    let (cache, _family, source, file) = cache_with_file(&red_hat_mono_path());
    let listed = cache.list().len();
    assert_eq!(cache.variable_instance_count(&source), 0, "premise: nothing made yet");

    // 100 distinct weights, 4 apart: 300, 304, ... 696 (the axis runs from 300 to 700)
    let weights: Vec<f32> = (0..100).map(|i| 300.0 + 4.0 * i as f32).collect();
    let faces: Vec<FontId> = weights
        .iter()
        .map(|w| face_for(&cache, source, &FontRequest::default().with_weight(*w)))
        .collect();

    assert_eq!(
        cache.variable_instance_count(&source),
        FC_MAX_INSTANCES_PER_FACE,
        "a hundred weights, one file: the instances stop at the bound"
    );
    // the first requests got a face of their own, each the instance at exactly its weight
    for (weight, face) in weights.iter().zip(&faces).take(FC_MAX_INSTANCES_PER_FACE) {
        let bytes = cache.get_font_bytes(face).expect("an instance has bytes");
        assert!(
            bytes.as_slice() == instance_bytes(&file, &[(*b"wght", *weight)]).as_slice(),
            "the instance for wght {weight} is not the file at that weight"
        );
    }
    let own: std::collections::HashSet<FontId> =
        faces[..FC_MAX_INSTANCES_PER_FACE].iter().copied().collect();
    assert_eq!(own.len(), FC_MAX_INSTANCES_PER_FACE, "32 requests, 32 faces");
    // every later request is drawn by the NEAREST of them: the last one made, 4 * 31 above 300
    let nearest = faces[FC_MAX_INSTANCES_PER_FACE - 1];
    for (i, face) in faces.iter().enumerate().skip(FC_MAX_INSTANCES_PER_FACE) {
        assert_eq!(*face, nearest, "weight {} is not drawn by the nearest instance", weights[i]);
    }
    // the instances are not in the cache's lists: no query finds them
    assert_eq!(cache.list().len(), listed, "the instances are invisible to `list`");
    // ... but a face the cache itself drew has bytes and metadata by id
    assert!(cache.get_metadata_by_id(&nearest).is_some());
}

/// The state is the cache's own: a second cache holds none of them, and a clone holds all.
#[test]
fn the_instances_live_and_die_with_the_cache() {
    let (cache, _family, source, _file) = cache_with_file(&red_hat_mono_path());
    let request = FontRequest::default().with_weight(650.0);
    let face = face_for(&cache, source, &request);

    let clone = cache.clone();
    assert_eq!(clone.variable_instance_count(&source), 1, "a clone shares them");
    assert_eq!(face_for(&clone, source, &request), face, "and answers with the same face");

    let (other, _family, other_source, _file) = cache_with_file(&red_hat_mono_path());
    assert_eq!(other.variable_instance_count(&other_source), 0, "another cache shares nothing");
    assert!(
        other.get_font_bytes(&face).is_none(),
        "and does not know the instance faces of the first"
    );
}

/// One font registered under three spellings (printpdf's pattern) is one face, and two ids of
/// one font - the same bytes under two family names - share the instance they draw.
#[test]
fn one_font_registered_under_many_names_has_one_instance_per_weight() {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let coverage = coverage_of(&file);
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    let ids: Vec<FontId> = ["X", "\"X\"", "'X'"]
        .iter()
        .map(|name| manager.register_named_font(name, &file, coverage.clone()))
        .collect();
    let own = manager.register_named_font("Azul Other Name", &file, coverage);
    assert_ne!(ids[0], own, "premise: two family names are two faces in the cache");

    let at_650 = |family: &str| first_face(&chain_through(&manager, family, "font-weight: 650"));
    let first = at_650("X");
    assert_eq!(at_650("x"), first, "the spellings of one family draw one instance");
    assert_eq!(
        at_650("Azul Other Name"),
        first,
        "the same bytes under another name draw the same instance"
    );
    assert_eq!(
        manager.fc_cache.variable_instance_count(&ids[0]),
        1,
        "one instance at 650 for all of them, no nine-bucket set"
    );
    assert_eq!(manager.fc_cache.variable_instance_count(&own), 1);
}

/// What an instance costs: the size (no `fvar`, `gvar`, `avar`: smaller than the file of a real
/// font) and the time to make it, the first request of a weight, of the fixture and of Red Hat Mono. Printed
/// for the record (`-- --nocapture`); the bounds are loose, a slow runner must not fail it.
#[test]
fn an_instance_is_smaller_than_its_file_and_made_in_a_moment() {
    for path in [mock_variable_path(), red_hat_mono_path()] {
        let (cache, _family, source, file) = cache_with_file(&path);
        let started = std::time::Instant::now();
        let face = face_for(&cache, source, &FontRequest::default().with_weight(650.0));
        let first = started.elapsed();
        let size = cache.get_font_bytes(&face).expect("bytes").as_slice().len();
        let started = std::time::Instant::now();
        let again = face_for(&cache, source, &FontRequest::default().with_weight(650.0));
        let second = started.elapsed();
        eprintln!(
            "[instance] {}: file {} bytes, instance at 650 {} bytes, made in {first:?}, asked again in {second:?}",
            path.file_name().and_then(|n| n.to_str()).unwrap_or("?"),
            file.len(),
            size
        );
        assert_eq!(face, again, "asked again: the same face");
        if path == red_hat_mono_path() {
            assert!(size < file.len(), "an instance has no variation tables: {size} vs {}", file.len());
        }
        assert!(
            first < std::time::Duration::from_secs(30),
            "an instance of a small face takes {first:?}"
        );
        assert!(second < first.max(std::time::Duration::from_millis(50)), "the second ask is a lookup");
    }
}

/// The same for the larger variable faces the website ships (`doc/fonts`: EB Garamond and Grenze,
/// WOFF2 on disk, unpacked to the plain font a font cache holds): what an instance of each costs
/// at a weight between two buckets. Printed for the record, loose bounds.
#[test]
fn an_instance_of_a_larger_face_is_made_in_a_moment() {
    for name in [
        "EBGaramond-Variable.woff2",
        "EBGaramond-Variable-Italic.woff2",
        "Grenze-Variable.woff2",
    ] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../doc/fonts")
            .join(name);
        let woff = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let file = azul_layout::text3::fontreg::sfnt_of(&woff).expect("the web font unpacks");
        let cache = FcFontCache::default();
        cache.with_memory_fonts(vec![(
            rust_fontconfig::FcPattern {
                family: Some(name.to_string()),
                ..Default::default()
            },
            rust_fontconfig::FcFont {
                bytes: file.clone().into(),
                font_index: 0,
                id: name.to_string(),
            },
        )]);
        let source = cache.list()[0].1;
        let variation = cache.variation(&source).expect("a variable font");
        let axes: Vec<String> = variation
            .axes
            .iter()
            .map(|a| format!("{} {}..{} (default {})", String::from_utf8_lossy(&a.tag), a.min, a.max, a.default))
            .collect();
        let started = std::time::Instant::now();
        let face = face_for(&cache, source, &FontRequest::default().with_weight(650.0));
        let made = started.elapsed();
        let size = cache.get_font_bytes(&face).expect("bytes").as_slice().len();
        eprintln!(
            "[instance] {name}: axes [{}], file {} bytes (unpacked), instance at 650 {size} bytes, made in {made:?}",
            axes.join(", "),
            file.len()
        );
        assert!(
            rust_fontconfig::FcReadVariation(cache.get_font_bytes(&face).expect("bytes").as_slice(), 0)
                .is_none(),
            "{name}: the instance is a static face"
        );
        assert!(made < std::time::Duration::from_secs(30), "{name}: an instance takes {made:?}");
    }
}
