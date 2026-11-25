//! A variable font registered from memory by family name - printpdf's `from_html` path, which
//! registers every input font under three spellings - is registered ONCE, as the font it is, and
//! drawn at the exact weight the CSS asks.
//!
//! Before, `register_named_font` cut a variable font into one static face per weight bucket
//! (five of them in the 300 to 700 of this one, nine for a wider axis) at registration and again
//! for each other spelling; a weight between two buckets (350, 650) was drawn at a bucket.

use rust_fontconfig::FcFontCache;

use crate::variable_fonts::{
    bytes_of, chain_through, coverage_of, instance_bytes, red_hat_mono_path, Manager,
};

#[test]
fn registering_a_variable_font_adds_one_face_however_often_it_is_registered() {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let coverage = coverage_of(&file);
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    let before = manager.fc_cache.list().len();

    let ids: Vec<_> = ["X", "\"X\"", "'X'"]
        .iter()
        .map(|name| manager.register_named_font(name, &file, coverage.clone()))
        .collect();
    let after = manager.fc_cache.list().len();
    assert_eq!(
        after - before,
        1,
        "three spellings of one font are one face in the cache, not one per weight bucket"
    );
    assert!(
        ids.iter().all(|id| *id == ids[0]),
        "the three spellings are one face: {ids:?}"
    );

    let norm = rust_fontconfig::utils::normalize_family_name("X");
    let faces = manager
        .memory_families
        .get(&norm)
        .expect("the family is registered");
    assert_eq!(faces.len(), 1, "one face under the family");
    assert_eq!(
        faces[0].weight_axis,
        Some((300.0, 700.0)),
        "the face says which weights it covers"
    );
}

/// The three spellings draw the weights between the buckets at exactly that weight, and share
/// the instance they draw.
#[test]
fn a_weight_between_two_buckets_is_drawn_at_that_weight() {
    let file = std::fs::read(red_hat_mono_path()).expect("the face is checked in");
    let coverage = coverage_of(&file);
    let mut manager = Manager::new(FcFontCache::default()).expect("font manager");
    for name in ["X", "\"X\"", "'X'"] {
        let _ = manager.register_named_font(name, &file, coverage.clone());
    }
    for (css, wght) in [
        ("font-weight: 350", 350.0),
        ("font-weight: 650", 650.0),
        ("font-weight: 1000", 700.0),
    ] {
        let chain = chain_through(&manager, "X", css);
        let drawn = bytes_of(&manager.fc_cache, &chain);
        assert!(
            drawn == instance_bytes(&file, &[(*b"wght", wght)]),
            "`{css}` does not draw the file at wght {wght}"
        );
    }
    let same = |spelling: &str| {
        crate::variable_fonts::first_face(&chain_through(&manager, spelling, "font-weight: 650"))
    };
    assert_eq!(
        same("X"),
        same("x"),
        "one font, one instance, whichever way the family is spelled"
    );
}
