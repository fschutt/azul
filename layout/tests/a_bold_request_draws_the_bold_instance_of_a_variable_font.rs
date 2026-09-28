//! A bold request on a variable font draws the font's BOLD instance.
//!
//! macOS draws its whole UI from one variable font, `SFNS.ttf` ("System
//! Font"): a `wght` axis from 1 to 1000, with Bold at 700. rust-fontconfig
//! indexes the file ONCE, at its default instance (400), so a bold request
//! matched that face and drew it regular. That is why the macOS bold chain
//! had been routed to Helvetica Neue, and why the titlebar's bold title came
//! out in a different typeface from everything around it.
//!
//! The resolver swaps a variable face whose `wght` axis spans the request
//! for a static instance baked at that weight.

use std::collections::HashMap;

use azul_layout::{
    font::parsed::bake_weight_instance,
    solver3::getters::{resolve_font_chains, CollectedFontStacks},
    text3::cache::{FontChainKeyOrRef, FontSelector, FontStyle},
};
use rust_fontconfig::{FcFontCache, FcFontPath, FcWeight, FontFallbackChain};

/// A real `wght`-variable TrueType face checked into this repository
/// (axis 300-700, default 400), so the test runs on every host.
fn red_hat_mono() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../doc/fonts/RedHatMono-VariableFont_wght.ttf")
}

/// The chain the resolver builds for `family` alone at `weight`.
fn chain_for(fc_cache: &FcFontCache, family: &str, weight: FcWeight) -> FontFallbackChain {
    let collected = CollectedFontStacks {
        font_stacks: vec![vec![FontSelector {
            family: family.to_string(),
            weight,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        }]],
        hash_to_index: HashMap::new(),
        font_refs: HashMap::new(),
    };
    resolve_font_chains(&collected, fc_cache, None)
        .chains
        .into_iter()
        .find_map(|(key, chain)| match key {
            FontChainKeyOrRef::Chain(key) if key.weight == weight => Some(chain),
            _ => None,
        })
        .expect("the stack resolves to a chain")
}

/// The variable font is registered as a FILE, exactly as a system scan
/// registers `SFNS.ttf`, and asked for at 700. The face the chain draws must
/// be the file's 700 instance: registered at Bold, and byte for byte what
/// baking the file at `wght = 700` produces.
#[test]
fn a_bold_request_on_a_variable_font_file_draws_its_700_instance() {
    let path = red_hat_mono();
    let file = std::fs::read(&path).expect("doc/fonts/RedHatMono-VariableFont_wght.ttf is checked in");
    let (pattern, _) = rust_fontconfig::FcParseFontBytes(&file, "RedHatMono")
        .and_then(|faces| faces.into_iter().next())
        .expect("the file parses");
    assert_eq!(
        pattern.weight,
        FcWeight::Normal,
        "premise: the file's default instance is Regular"
    );
    let family = pattern
        .family
        .clone()
        .expect("premise: the face names its family");

    let fc_cache = FcFontCache::default();
    let _source = fc_cache.insert_fast_pattern(
        pattern,
        FcFontPath {
            path: path.to_string_lossy().into_owned(),
            font_index: 0,
            bytes_hash: 0,
        },
    );

    let chain = chain_for(&fc_cache, &family, FcWeight::Bold);
    let face = chain
        .css_fallbacks
        .iter()
        .find_map(|group| group.fonts.first())
        .expect("the family matched");
    let meta = fc_cache
        .get_metadata_by_id(&face.id)
        .expect("the face drawn is registered in this cache");
    assert_eq!(
        meta.weight,
        FcWeight::Bold,
        "a bold request drew the variable font's default instance ({:?}), not its 700 instance",
        meta.weight
    );

    let drawn = fc_cache
        .get_font_bytes(&face.id)
        .expect("the face drawn has bytes");
    let bold = bake_weight_instance(&file, 0, 700.0).expect("the file bakes at wght 700");
    assert!(
        drawn.as_slice() == bold.as_slice(),
        "the face drawn ({} bytes) is not the file's 700 instance ({} bytes)",
        drawn.as_slice().len(),
        bold.len()
    );

    // A regular request keeps the file itself: nothing to bake.
    let regular = chain_for(&fc_cache, &family, FcWeight::Normal);
    let face = regular
        .css_fallbacks
        .iter()
        .find_map(|group| group.fonts.first())
        .expect("the family matched");
    assert_eq!(
        fc_cache.get_metadata_by_id(&face.id).map(|m| m.weight),
        Some(FcWeight::Normal)
    );
    assert!(
        fc_cache
            .get_font_bytes(&face.id)
            .is_some_and(|b| b.as_slice() == file.as_slice()),
        "a regular request must draw the file as it is"
    );
}

/// macOS: `system:title:bold`, the face the titlebar draws its title in, is
/// the SYSTEM font at 700. Not Helvetica Neue, and not SF's regular
/// instance. Needs `SFNS.ttf`, which every macOS since 10.15 ships.
#[test]
fn the_bold_system_title_face_on_macos_is_the_system_font_at_700() {
    use azul_core::{dom::Dom, styled_dom::StyledDom};
    use azul_layout::solver3::getters::collect_font_stacks_from_styled_dom;

    if !std::path::Path::new("/System/Library/Fonts/SFNS.ttf").exists() {
        eprintln!("SKIP: no /System/Library/Fonts/SFNS.ttf on this host (macOS only)");
        return;
    }

    let fc_cache = FcFontCache::build();
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_css("font-family: system:title:bold; font-size: 13px;")
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Window Title",
            )),
    );
    let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let collected =
        collect_font_stacks_from_styled_dom(&styled, &azul_css::system::Platform::MacOs);
    let resolved = resolve_font_chains(&collected, &fc_cache, None);

    // The macOS bold chain is the only one that lists Lucida Grande.
    let chain = resolved
        .chains
        .iter()
        .find_map(|(key, chain)| match key {
            FontChainKeyOrRef::Chain(key)
                if key.weight == FcWeight::Bold
                    && key
                        .font_families
                        .iter()
                        .any(|f| f.as_str() == "Lucida Grande") =>
            {
                Some(chain)
            }
            _ => None,
        })
        .expect("the bold title stack resolves");
    let group = chain
        .css_fallbacks
        .iter()
        .find(|group| !group.fonts.is_empty())
        .expect("a family of the stack matched");
    assert_eq!(
        group.css_name.as_str(),
        "System Font",
        "the bold title is not drawn in the system font"
    );
    let meta = fc_cache
        .get_metadata_by_id(&group.fonts[0].id)
        .expect("the face drawn is registered in this cache");
    assert_eq!(
        meta.weight,
        FcWeight::Bold,
        "the system font is drawn at {:?}, not at its Bold instance",
        meta.weight
    );
}
