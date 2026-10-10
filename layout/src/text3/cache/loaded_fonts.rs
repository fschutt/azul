//! The fonts a layout has loaded: the font pool, owned-or-shared fonts and the font context.

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, HashMap},
        BTreeSet, HashSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage,
    props::{basic::ColorU, style::StyleBackgroundContent},
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{
    FcFontCache, FcPattern, FcStretch, FcWeight, FontId, PatternMatch, UnicodeRange,
};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;
use crate::text3::script::{script_to_language, Language, Script};
#[allow(clippy::wildcard_imports)]
// the text layout cache's items, re-exported from the sibling modules by mod.rs
use super::*;

/// A map of pre-loaded fonts, keyed by `FontId` (from rust-fontconfig)
///
/// This is passed to the shaper - no font loading happens during shaping
/// The fonts are loaded BEFORE layout based on the font chains and text content.
///
/// Provides both `FontId` and hash-based lookup for efficient glyph operations.
#[derive(Debug, Clone)]
pub struct LoadedFonts<T> {
    /// Primary storage: `FontId` -> Font
    pub fonts: HashMap<FontId, T>,
    /// Reverse index: `font_hash` -> `FontId` for fast hash-based lookups
    hash_to_id: HashMap<u64, FontId>,
}

impl<T: ParsedFontTrait> LoadedFonts<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            hash_to_id: HashMap::new(),
        }
    }

    /// Insert a font with its `FontId`
    pub fn insert(&mut self, font_id: FontId, font: T) {
        let hash = font.get_hash();
        self.hash_to_id.insert(hash, font_id);
        self.fonts.insert(font_id, font);
    }

    /// Get a font by `FontId`
    #[must_use]
    pub fn get(&self, font_id: &FontId) -> Option<&T> {
        self.fonts.get(font_id)
    }

    /// Get a font by its hash
    #[must_use]
    pub fn get_by_hash(&self, hash: u64) -> Option<&T> {
        self.hash_to_id.get(&hash).and_then(|id| self.fonts.get(id))
    }

    /// Get the `FontId` for a hash
    #[must_use]
    pub fn get_font_id_by_hash(&self, hash: u64) -> Option<&FontId> {
        self.hash_to_id.get(&hash)
    }

    /// Check if a `FontId` is present
    #[must_use]
    pub fn contains_key(&self, font_id: &FontId) -> bool {
        self.fonts.contains_key(font_id)
    }

    /// Check if a hash is present
    #[must_use]
    pub fn contains_hash(&self, hash: u64) -> bool {
        self.hash_to_id.contains_key(&hash)
    }

    /// Iterate over all fonts
    pub fn iter(&self) -> impl Iterator<Item = (&FontId, &T)> {
        self.fonts.iter()
    }

    /// Get the number of loaded fonts
    #[must_use]
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for LoadedFonts<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ParsedFontTrait> FromIterator<(FontId, T)> for LoadedFonts<T> {
    fn from_iter<I: IntoIterator<Item = (FontId, T)>>(iter: I) -> Self {
        let mut loaded = Self::new();
        for (id, font) in iter {
            loaded.insert(id, font);
        }
        loaded
    }
}

/// Enum that wraps either a fontconfig-resolved font (T) or a direct `FontRef`.
///
/// This allows the shaping code to handle both fontconfig-resolved fonts
/// and embedded fonts (`FontRef`) uniformly through the `ParsedFontTrait` interface.
#[derive(Debug, Clone)]
pub enum FontOrRef<T> {
    /// A font loaded via fontconfig
    Font(T),
    /// A direct `FontRef` (embedded font, bypasses fontconfig)
    Ref(azul_css::props::basic::FontRef),
}

impl<T: ParsedFontTrait> ShallowClone for FontOrRef<T> {
    fn shallow_clone(&self) -> Self {
        match self {
            Self::Font(f) => Self::Font(f.shallow_clone()),
            Self::Ref(r) => Self::Ref(r.clone()),
        }
    }
}

impl<T: ParsedFontTrait> ParsedFontTrait for FontOrRef<T> {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        match self {
            Self::Font(f) => f.shape_text(text, script, language, direction, style),
            Self::Ref(r) => r.shape_text(text, script, language, direction, style),
        }
    }

    fn get_hash(&self) -> u64 {
        match self {
            Self::Font(f) => f.get_hash(),
            Self::Ref(r) => r.get_hash(),
        }
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        match self {
            Self::Font(f) => f.get_glyph_size(glyph_id, font_size),
            Self::Ref(r) => r.get_glyph_size(glyph_id, font_size),
        }
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            Self::Font(f) => f.get_hyphen_glyph_and_advance(font_size),
            Self::Ref(r) => r.get_hyphen_glyph_and_advance(font_size),
        }
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            Self::Font(f) => f.get_kashida_glyph_and_advance(font_size),
            Self::Ref(r) => r.get_kashida_glyph_and_advance(font_size),
        }
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        match self {
            Self::Font(f) => f.has_glyph(codepoint),
            Self::Ref(r) => r.has_glyph(codepoint),
        }
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        match self {
            Self::Font(f) => f.get_vertical_metrics(glyph_id),
            Self::Ref(r) => r.get_vertical_metrics(glyph_id),
        }
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        match self {
            Self::Font(f) => f.get_font_metrics(),
            Self::Ref(r) => r.get_font_metrics(),
        }
    }

    fn num_glyphs(&self) -> u16 {
        match self {
            Self::Font(f) => f.num_glyphs(),
            Self::Ref(r) => r.num_glyphs(),
        }
    }

    fn get_space_width(&self) -> Option<usize> {
        match self {
            Self::Font(f) => f.get_space_width(),
            Self::Ref(r) => r.get_space_width(),
        }
    }
}

/// Bundles all font-related state that can be shared across layout passes.
///
/// Separates font concerns from layout/rendering state (`LayoutWindow`).
/// Each test/render creates a fresh `LayoutWindow` from a shared `FontContext`,
/// avoiding stale layout cache reuse while keeping parsed fonts warm.
///
/// Usage:
/// ```ignore
/// let ctx = FontContext::from_fc_cache(fc_cache);
/// ctx.pre_resolve_chains(&styled_dom, &platform);
/// ctx.load_fonts_for_chains();
///
/// // Per-test: create fresh LayoutWindow from context
/// let mut window = LayoutWindow::from_font_context(&ctx)?;
/// window.layout_and_generate_display_list(styled_dom, ...)?;
/// ```
#[derive(Debug, Clone)]
pub struct FontContext {
    /// The shared font cache. As of rust-fontconfig 4.1 this type is
    /// itself backed by `Arc<RwLock<_>>`, so cloning is cheap and all
    /// clones see builder-thread writes immediately — no more `Arc<T>`
    /// wrapping is needed and no more stale-snapshot refresh dance.
    pub fc_cache: FcFontCache,
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, azul_css::props::basic::FontRef>>>,
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    pub embedded_fonts: HashMap<u64, azul_css::props::basic::FontRef>,
    /// Reverse map: `font_family_hash` → actual `StyleFontFamilyVec`.
    /// Accumulated across DOMs for persistence. Copied to `FontManager` on `LayoutWindow`
    /// creation.
    pub font_hash_to_families: HashMap<u64, azul_css::props::basic::font::StyleFontFamilyVec>,
    /// Optional link back to the live `FcFontRegistry`. Present iff the
    /// caller wants the scout-on-demand path
    /// ([`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]),
    /// which priority-bumps the builder for not-yet-parsed families
    /// rather than falling back to the empty-snapshot response.
    pub registry: Option<Arc<rust_fontconfig::registry::FcFontRegistry>>,
}

impl FontContext {
    /// Create from an `FcFontCache`. Parsed fonts, font chains, and
    /// embedded fonts start empty.
    ///
    /// The resulting `FontContext` has `registry = None`, so font
    /// chain resolution only sees what's already in the cache. For
    /// the scout-on-demand path, use [`FontContext::from_registry`]
    /// instead, which keeps a handle to the registry so that chain
    /// resolution can lazy-parse families the DOM needs.
    #[must_use]
    pub fn from_fc_cache(fc_cache: FcFontCache) -> Self {
        crate::font::loading::use_browser_generic_families(&fc_cache);
        Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: HashMap::new(),
            font_hash_to_families: HashMap::new(),
            registry: None,
        }
    }

    /// Create from a live `FcFontRegistry`. The `fc_cache` field gets
    /// a *shared* handle to the registry's cache (cheap `Arc::clone`
    /// on the v4.1 shared-state cache) — writes by builder threads
    /// show up immediately in every reader. Chain resolution goes
    /// through
    /// [`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]
    /// which priority-bumps the builder for unparsed families and
    /// waits for them. This is the "scout-on-demand" path: a
    /// headless renderer can skip the eager common-stack parse and
    /// pay only the per-family cost on first use, dropping peak RSS
    /// by the common-stack metadata size (~15 MiB on macOS).
    pub fn from_registry(registry: Arc<rust_fontconfig::registry::FcFontRegistry>) -> Self {
        let fc_cache = registry.shared_cache();
        // The registry resolves with its cache's config: the shared handle
        // carries the browser generic families into it.
        crate::font::loading::use_browser_generic_families(&fc_cache);
        Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: HashMap::new(),
            font_hash_to_families: HashMap::new(),
            registry: Some(registry),
        }
    }

    /// Pre-resolve font chains for a `StyledDom`'s CSS font stacks.
    /// Call this before layout so text rendering doesn't skip glyphs.
    ///
    /// Unicode-fallback fonts are limited to the scripts actually
    /// present in the document's text content — for an ASCII-only
    /// page, this skips the ~300 MiB Arial-Unicode / CJK / Arabic
    /// pull-in entirely. See
    /// [`crate::solver3::getters::scripts_present_in_styled_dom`].
    pub fn pre_resolve_chains_for_dom(
        &mut self,
        styled_dom: &azul_core::styled_dom::StyledDom,
        platform: &azul_css::system::Platform,
    ) {
        use crate::solver3::getters::{
            collect_font_stacks_from_styled_dom, collect_used_codepoints,
            prune_chain_to_used_chars, resolve_font_chains, scripts_present_in_styled_dom,
        };
        let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);
        let scripts = scripts_present_in_styled_dom(styled_dom);
        let mut chains = resolve_font_chains(&collected, &self.fc_cache, Some(&scripts));
        // Coverage-based prune (matches `collect_and_resolve_font_chains_with_registration`).
        // A chain that matched nothing already carries its last-resort face
        // (the resolver applied `ensure_chains_nonempty`), on the tier the
        // prune never touches.
        let used_chars = collect_used_codepoints(styled_dom);
        for chain in chains.chains.values_mut() {
            prune_chain_to_used_chars(chain, &used_chars);
        }
        self.font_chain_cache = chains.into_fontconfig_chains();
    }

    /// Load parsed font bytes from disk for all fonts referenced in `font_chain_cache`.
    ///
    /// Thin wrapper that materialises a `ResolvedFontChains` from the
    /// cached chain map and delegates the actual disk-load to the
    /// shared `FontManager::load_missing_for_chains` helper, so the
    /// "collect → diff → load → insert" sequence lives in exactly
    /// one place. Failures are silently dropped here (the caller is
    /// the warmup path which has no good place to log them); use
    /// `FontManager::load_missing_for_chains` directly for diagnostics.
    pub fn load_fonts_for_chains(&self) {
        use crate::{solver3::getters::ResolvedFontChains, text3::default::PathLoader};

        let chains_map: HashMap<FontChainKeyOrRef, _> = self
            .font_chain_cache
            .iter()
            .map(|(k, v)| (FontChainKeyOrRef::Chain(k.clone()), v.clone()))
            .collect();
        let resolved = ResolvedFontChains {
            chains: chains_map,
            ..Default::default()
        };

        // Borrow our shared `parsed_fonts` Arc as a transient
        // FontManager so we can use the helper. `from_arc_shared`
        // returns a manager that mutates the same underlying pool.
        let Ok(manager) = FontManager::<azul_css::props::basic::FontRef>::from_arc_shared(
            self.fc_cache.clone(),
            self.parsed_fonts.clone(),
        ) else {
            return;
        };
        let loader = PathLoader::new();
        let _failed = manager
            .load_missing_for_chains(&resolved, |bytes, idx| loader.load_font_shared(bytes, idx));
    }

    /// Convert into a `FontManager` with all data populated.
    /// Carries the `registry` forward so the resulting manager also
    /// has the scout-on-demand path available.
    #[must_use]
    pub fn to_font_manager(&self) -> FontManager<azul_css::props::basic::FontRef> {
        let mut fm = FontManager {
            fc_cache: self.fc_cache.clone(),
            parsed_fonts: self.parsed_fonts.clone(),
            condemned_fonts: Arc::new(Mutex::new(CondemnedFonts::default())),
            font_chain_cache: self.font_chain_cache.clone(),
            embedded_fonts: Arc::new(Mutex::new(self.embedded_fonts.clone())),
            condemned_embedded_fonts: Arc::default(),
            font_hash_to_families: self.font_hash_to_families.clone(),
            registry: self.registry.clone(),
            last_resolved_font_stacks_sig: None,
            memory_families: HashMap::new(),
            vf_bake_cache: HashMap::new(),
            loaded_fonts_memo: Arc::new(Mutex::new(None)),
        };
        // Idempotent: reuses the FontIds already in the shared fc_cache.
        fm.register_builtin_mock_fonts();
        fm
    }
}
