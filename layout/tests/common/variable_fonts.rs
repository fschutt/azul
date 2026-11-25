//! What the variable-font tests share: the variable faces checked into this repository, a font
//! cache that holds one of them, a document whose text is set in it, the face a chain draws that
//! text in, and the advance of one glyph of that face.
//!
//! Every function takes the cache or the manager it works in: no test sees the faces of another.

use std::path::PathBuf;

use azul_core::{dom::Dom, styled_dom::StyledDom};
use azul_layout::{
    font::parsed::ParsedFont,
    solver3::getters::{
        collect_font_stacks_from_styled_dom, resolve_font_chains, resolve_font_chains_with_registry,
        ResolvedFontChains,
    },
    text3::cache::{FontChainKeyOrRef, FontManager},
};
use rust_fontconfig::{FcFontCache, FcFontPath, FontFallbackChain, FontId};

/// A real `wght`-variable TrueType face (axis 300-700, default 400, monospaced), so the bytes of
/// one of its instances are all a test needs to compare.
pub fn red_hat_mono_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../doc/fonts/RedHatMono-VariableFont_wght.ttf")
}

/// The multi-axis fixture `scripts/gen_variable_test_font.py` writes: `wght`, `wdth`, `slnt`,
/// `ital`, `opsz`, one glyph for each (`W D S I O`), see `tests/fonts/README.md`.
pub fn mock_variable_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/azul-mock-variable.ttf")
}

/// A static face with no axis at all.
pub fn static_face_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/azul-mock-prop.ttf")
}

/// A font file registered the way a system scan registers one: as a FILE, at its default
/// instance. `(cache, family, id of the face, the file's bytes)`.
pub fn cache_with_file(path: &std::path::Path) -> (FcFontCache, String, FontId, Vec<u8>) {
    let file = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let (pattern, _) = rust_fontconfig::FcParseFontBytes(&file, "face")
        .and_then(|faces| faces.into_iter().next())
        .expect("the file parses");
    let family = pattern
        .family
        .clone()
        .expect("premise: the face names its family");
    let cache = FcFontCache::default();
    let id = cache.insert_fast_pattern(
        pattern,
        FcFontPath {
            path: path.to_string_lossy().into_owned(),
            font_index: 0,
            bytes_hash: 0,
        },
    );
    (cache, family, id, file)
}

/// A document of one block of `text` set in `family` with the declarations `css`, inside the
/// block `outer_css` (declarations the text inherits from).
pub fn page(family: &str, outer_css: &str, css: &str, text: &str) -> StyledDom {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div().with_css(outer_css).with_child(
            Dom::create_div()
                .with_css(&format!("font-family: \"{family}\"; font-size: 16px; {css}"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
        ),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// The chains the resolver builds for `styled`, in `cache` (no registry, no memory families).
pub fn resolve(styled: &StyledDom, cache: &FcFontCache) -> ResolvedFontChains {
    let collected = collect_font_stacks_from_styled_dom(styled, &azul_css::system::Platform::current());
    resolve_font_chains(&collected, cache, None)
}

/// The chain of the text of `styled`: the one chain whose first family is `family`.
pub fn chain_of(resolved: &ResolvedFontChains, family: &str) -> FontFallbackChain {
    let mut found: Vec<FontFallbackChain> = resolved
        .chains
        .iter()
        .filter_map(|(key, chain)| match key {
            FontChainKeyOrRef::Chain(key)
                if key.font_families.first().map(String::as_str) == Some(family) =>
            {
                Some(chain.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "one chain for the one family of the page");
    found.remove(0)
}

/// The face a chain draws first: the first face of its first CSS group that has one.
pub fn first_face(chain: &FontFallbackChain) -> FontId {
    chain
        .css_fallbacks
        .iter()
        .find_map(|group| group.fonts.first())
        .map(|face| face.id)
        .expect("the family matched a face")
}

/// The id of the face the text set with `css` in `family` is drawn in.
pub fn drawn_face(cache: &FcFontCache, family: &str, css: &str) -> FontId {
    let styled = page(family, "", css, "W");
    first_face(&chain_of(&resolve(&styled, cache), family))
}

/// The bytes of that face.
pub fn drawn_bytes(cache: &FcFontCache, family: &str, css: &str) -> Vec<u8> {
    let id = drawn_face(cache, family, css);
    cache
        .get_font_bytes(&id)
        .expect("the face drawn has bytes")
        .as_slice()
        .to_vec()
}

/// The advance of the glyph of `ch` in the face `bytes`, in font units.
pub fn advance_of(bytes: &[u8], ch: char) -> u16 {
    let font = ParsedFont::from_bytes(bytes, 0, &mut Vec::new()).expect("the face parses");
    let glyph = font
        .lookup_glyph_index(ch as u32)
        .unwrap_or_else(|| panic!("the face has a glyph for {ch:?}"));
    font.get_horizontal_advance(glyph)
}

/// Whether the face `bytes` is still a variable font (has axes).
pub fn is_variable(bytes: &[u8]) -> bool {
    rust_fontconfig::FcReadVariation(bytes, 0).is_some()
}

/// The bytes of the instance of the face `file` at `coordinates` (axis tag, value).
pub fn instance_bytes(file: &[u8], coordinates: &[([u8; 4], f32)]) -> Vec<u8> {
    rust_fontconfig::FcCreateVariableInstance(file, 0, coordinates)
        .expect("the face has an instance at these coordinates")
}

/// A font manager over a font cache of its own, as an app's window has one.
pub type Manager = FontManager<azul_css::props::basic::FontRef>;

/// The chain of the text of `page(family, "", css, "W")` through a font manager: the path the
/// DOM's text takes, which sees the faces registered by family name.
pub fn chain_through(manager: &Manager, family: &str, css: &str) -> FontFallbackChain {
    let styled = page(family, "", css, "W");
    let collected =
        collect_font_stacks_from_styled_dom(&styled, &azul_css::system::Platform::current());
    let resolved = resolve_font_chains_with_registry(
        &collected,
        &manager.fc_cache,
        None,
        Some(&[]),
        &manager.memory_families,
        &manager.settings,
    );
    chain_of(&resolved, family)
}

/// The bytes of the first face of `chain`, from `cache`.
pub fn bytes_of(cache: &FcFontCache, chain: &FontFallbackChain) -> Vec<u8> {
    cache
        .get_font_bytes(&first_face(chain))
        .expect("the face drawn has bytes")
        .as_slice()
        .to_vec()
}

/// The code points of the face `file`, as its cmap gives them.
pub fn coverage_of(file: &[u8]) -> Vec<rust_fontconfig::UnicodeRange> {
    rust_fontconfig::FcParseFontBytes(file, "face")
        .and_then(|faces| faces.into_iter().next())
        .expect("the file parses")
        .0
        .unicode_ranges
}
