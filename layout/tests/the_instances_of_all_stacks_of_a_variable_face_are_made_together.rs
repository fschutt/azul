//! A document that sets one variable face in several weights and sizes asks the resolver for one static
//! instance of it per distinct set of coordinates. The first layout of a window made them one after the other
//! (eight instances of macOS's SFNS.ttf: 108 ms of its 331 ms); the resolver now collects the distinct pairs of
//! all its chains and makes them together, then draws each chain at them. The chains are the same either way:
//! this holds each of them to the instance of the file at its own coordinates, and the cache to one instance
//! per distinct coordinates (a pair asked twice is made once).

use std::collections::HashMap;

use azul_layout::{
    solver3::getters::{font_coordinates, resolve_font_chains_with_registry, CollectedFontStacks},
    text3::cache::{FontChainKeyOrRef, FontRequest, FontSelector},
};

use crate::variable_fonts::{
    cache_with_file, instance_bytes, is_variable, mock_variable_path, red_hat_mono_path,
};

fn collected(family: &str, requests: &[FontRequest]) -> CollectedFontStacks {
    CollectedFontStacks {
        font_stacks: requests
            .iter()
            .map(|request| {
                vec![FontSelector {
                    family: family.to_string(),
                    request: request.clone(),
                    ..FontSelector::default()
                }]
            })
            .collect(),
        hash_to_index: HashMap::new(),
        font_refs: HashMap::new(),
    }
}

/// The chains of `stacks` through the resolver of a window, by the request each was resolved for.
fn resolve(
    cache: &rust_fontconfig::FcFontCache,
    stacks: &CollectedFontStacks,
) -> Vec<(FontRequest, rust_fontconfig::FontFallbackChain)> {
    let settings = azul_layout::settings::TextSettings::default();
    let resolved =
        resolve_font_chains_with_registry(stacks, cache, None, Some(&[]), &HashMap::new(), &settings);
    resolved
        .chains
        .iter()
        .filter_map(|(key, chain)| match key {
            FontChainKeyOrRef::Chain(key) => Some((key.request.clone(), chain.clone())),
            FontChainKeyOrRef::Ref(_) => None,
        })
        .collect()
}

#[test]
fn six_weights_of_one_face_are_six_instances_each_drawn_at_its_own() {
    let (cache, family, source, file) = cache_with_file(&red_hat_mono_path());
    assert!(is_variable(&file), "premise: a variable face");
    let weights = [300.0, 400.0, 500.0, 600.0, 650.0, 700.0];
    let requests: Vec<FontRequest> = weights
        .iter()
        .map(|w| FontRequest::default().with_weight(*w))
        .collect();
    let chains = resolve(&cache, &collected(&family, &requests));
    assert_eq!(chains.len(), weights.len(), "one chain a weight");
    assert_eq!(
        cache.variable_instance_count(&source),
        weights.len(),
        "one instance a distinct set of coordinates"
    );
    for (request, chain) in chains {
        let weight = request.weight.to_f32();
        // 400 is the face's default: its instance has no coordinate of its own.
        let coordinates: Vec<([u8; 4], f32)> = if (weight - 400.0).abs() < 0.01 {
            Vec::new()
        } else {
            vec![(*b"wght", weight)]
        };
        let face = chain
            .css_fallbacks
            .iter()
            .find_map(|group| group.fonts.first())
            .expect("the family matched")
            .id;
        let drawn = cache.get_font_bytes(&face).expect("the face drawn has bytes");
        assert!(!is_variable(drawn.as_slice()), "weight {weight}: still a variable font");
        assert!(
            drawn.as_slice() == instance_bytes(&file, &coordinates).as_slice(),
            "weight {weight}: not the instance of the file at {coordinates:?}"
        );
    }
}

#[test]
fn a_pair_asked_twice_is_made_once() {
    let (cache, family, source, _file) = cache_with_file(&red_hat_mono_path());
    let request = FontRequest::default().with_weight(650.0);
    // the same stack three times, and a second one at another weight
    let stacks = collected(&family, &[request.clone(), request.clone(), request, FontRequest::default().with_weight(700.0)]);
    let chains = resolve(&cache, &stacks);
    assert_eq!(chains.len(), 2, "the stack resolved once however often it was collected");
    assert_eq!(cache.variable_instance_count(&source), 2);
}

#[test]
fn the_sizes_of_a_face_with_an_optical_size_axis_are_the_distinct_coordinates() {
    let (cache, family, source, _file) = cache_with_file(&mock_variable_path());
    let variation = cache.variation(&source).expect("premise: the mock face is variable");
    assert!(variation.axes.iter().any(|axis| &axis.tag == b"opsz"), "premise: it has an opsz axis");
    let sizes = [12.0_f32, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0, 20.0, 18.0];
    let requests: Vec<FontRequest> = sizes
        .iter()
        .map(|size| FontRequest::default().at_font_size(*size))
        .collect();
    let mut distinct: Vec<Vec<([u8; 4], f32)>> = Vec::new();
    for request in &requests {
        let coordinates = font_coordinates(request, &variation);
        if !distinct.contains(&coordinates) {
            distinct.push(coordinates);
        }
    }
    let chains = resolve(&cache, &collected(&family, &requests));
    assert!(!chains.is_empty());
    assert_eq!(
        cache.variable_instance_count(&source),
        distinct.len(),
        "one instance a distinct set of coordinates: {distinct:?}"
    );
}
