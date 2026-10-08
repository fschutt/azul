//! The font manager: font loading, memory fonts and their tiers, and the fonts it retires.

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

/// How a registered in-memory face ranks against fonts on disk.
///
/// The distinction exists because "register a font by name" means two different
/// things. A font the caller explicitly supplied for a family *is* that family
/// and must beat anything installed, exactly as CSS says. A font offered as a
/// stand-in for a generic family - the 14 standard PDF fonts answering
/// `sans-serif`, say - must not, or a Win-1252 subset would displace the
/// system's full Unicode faces on every desktop.
///
/// Without the second tier the choice is all-or-nothing: claim `sans-serif` and
/// wreck desktop, or leave it alone and have nothing at all on a target with no
/// fonts on disk, such as wasm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryFontTier {
    /// Wins over anything on disk. The right tier for a font the caller named.
    Primary,
    /// Used only after disk resolution has had its turn. The right tier for a
    /// last-resort face standing in for a generic family.
    Fallback,
}

/// One in-memory face registered under a family name, with the style attributes
/// needed to choose the right face for a CSS `(weight, italic/oblique)` query.
///
/// [`FontManager::register_named_font`] registers several faces under the *same*
/// family name (e.g. `Helvetica` regular, bold, oblique). Keying
/// [`FontManager::memory_families`] by family alone therefore collapsed them —
/// the last registration won and `font-weight: bold` silently rendered in the
/// regular face. Each face now records its own weight/style so resolution can
/// pick the closest one (see `getters::split_memory_matches`).
#[derive(Debug, Clone)]
pub struct MemoryFace {
    /// Whether this face outranks the disk or only backstops it.
    pub tier: MemoryFontTier,
    /// The `FontMatch` the resolver emits when this face is chosen.
    pub font_match: rust_fontconfig::FontMatch,
    /// OS/2 weight of this face (static fonts). For a variable font this is the
    /// default-instance weight; `weight_axis` carries the selectable range.
    pub weight: FcWeight,
    /// `head`/OS-2 italic bit.
    pub italic: bool,
    /// OS/2 oblique bit.
    pub oblique: bool,
    /// OS/2 width class.
    pub stretch: FcStretch,
    /// For a variable font, the `wght` axis `(min, max)` in user units; `None`
    /// for a static face. Lets a single VF satisfy any requested weight.
    pub weight_axis: Option<(f32, f32)>,
}

/// Style attributes parsed from a font's bytes (OS/2 + `head`), used to index a
/// registered face in [`FontManager::memory_families`].
#[derive(Debug, Clone, Copy)]
pub(super) struct FaceStyle {
    weight: FcWeight,
    italic: bool,
    oblique: bool,
    stretch: FcStretch,
    weight_axis: Option<(f32, f32)>,
}

impl Default for FaceStyle {
    fn default() -> Self {
        Self {
            weight: FcWeight::Normal,
            italic: false,
            oblique: false,
            stretch: FcStretch::Normal,
            weight_axis: None,
        }
    }
}

/// Parse a face's weight / italic / oblique / stretch from its bytes via
/// rust-fontconfig (which reads OS/2 `usWeightClass`/`usWidthClass` and the
/// `head` italic bit). Falls back to upright Normal when the font can't be
/// parsed, so registration never fails on a malformed face.
pub(super) fn parse_face_style(bytes: &[u8], family: &str) -> FaceStyle {
    let Some(faces) = rust_fontconfig::FcParseFontBytes(bytes, family) else {
        return FaceStyle::default();
    };
    let Some((pat, _)) = faces.into_iter().next() else {
        return FaceStyle::default();
    };
    FaceStyle {
        weight: pat.weight,
        italic: pat.italic == PatternMatch::True,
        oblique: pat.oblique == PatternMatch::True,
        stretch: pat.stretch,
        weight_axis: None,
    }
}

/// The font GC's grace pool: faces evicted from `parsed_fonts` wait here for
/// one resurrection window (see `FontManager::condemned_fonts`).
#[derive(Debug)]
pub struct CondemnedFonts<T> {
    /// Evicted faces, each stamped with the GC generation that evicted it.
    pub faces: HashMap<FontId, (T, u64)>,
    /// Monotonic GC pass counter.
    pub generation: u64,
}

impl<T> Default for CondemnedFonts<T> {
    fn default() -> Self {
        Self {
            faces: HashMap::new(),
            generation: 0,
        }
    }
}

/// The embedded-font GC's grace pool (see
/// [`FontManager::collect_embedded_fonts`]): `StyleFontFamily::Ref` faces no
/// display list of the window showed at its last run, by hash.
#[derive(Debug, Default)]
pub struct CondemnedEmbeddedFonts {
    /// Condemned faces, each stamped with the run that condemned it.
    pub faces: HashMap<u64, (azul_css::props::basic::FontRef, u64)>,
    /// Monotonic GC run counter.
    pub generation: u64,
    /// How many faces were dropped, ever: part of every manager's "this
    /// DOM's fonts are the ones last resolved" signature, so a DOM that still
    /// names a dropped face (its text hidden) registers it again on its next
    /// layout instead of skipping the registration.
    pub dropped: u64,
}

#[derive(Debug)]
pub struct FontManager<T> {
    /// The font-path cache. `FcFontCache` in rust-fontconfig 4.1 is
    /// already a shared handle internally (`Arc<RwLock<_>>`), so no
    /// further `Arc<...>` wrapping is needed — clones are cheap and
    /// all clones see builder writes instantly.
    pub fc_cache: FcFontCache,
    /// Holds the actual parsed font (usually with the font bytes attached).
    /// Wrapped in Arc so multiple `FontManager` instances can share the same
    /// pool of already-parsed fonts (avoids re-reading from disk).
    pub parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>,
    /// Faces the font GC evicted, kept for a RESURRECTION window instead of
    /// dropped: any hash resolution against a condemned face moves it back
    /// into `parsed_fonts`, and only a face nobody resolved for two GC
    /// generations is truly dropped.
    ///
    /// This exists because the GC's original safety argument — "eviction is
    /// always safe, the next layout re-loads what it needs" — is FALSE when
    /// a shaping cache serves cached glyphs without re-loading: the display
    /// list then carries a `font_hash` whose face is gone and the renderer
    /// silently loses the text (the azwriter textless-window bug; the
    /// failing hash appeared verbatim in the GC's eviction trace).
    pub condemned_fonts: Arc<Mutex<CondemnedFonts<T>>>,
    // Cache for font chains - populated by resolve_all_font_chains() before layout
    // This is read-only during layout - no locking needed for reads
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    /// Cache for direct `FontRefs` (embedded fonts like Material Icons)
    /// These are fonts referenced via `FontStack::Ref` that bypass fontconfig
    /// Faces handed to us directly by the DOM as `StyleFontFamily::Ref`
    /// (Material Icons and every other `FontStack::Ref`).
    ///
    /// `Arc<Mutex<..>>`, NOT `Mutex<..>`, and that is load-bearing: this pool
    /// must be SHARED by every manager cloned from this one, exactly like
    /// `parsed_fonts` above. See `clone_shared`.
    pub embedded_fonts: Arc<Mutex<HashMap<u64, azul_css::props::basic::FontRef>>>,
    /// `embedded_fonts`' faces the embedded-font GC condemned, shared with it.
    pub condemned_embedded_fonts: Arc<Mutex<CondemnedEmbeddedFonts>>,
    /// Reverse map: `font_family_hash` → actual `StyleFontFamilyVec`.
    /// Accumulated across DOMs. Used by font collection and text shaping to
    /// resolve compact cache hashes without `get_property_slow`.
    pub font_hash_to_families: HashMap<u64, azul_css::props::basic::font::StyleFontFamilyVec>,
    /// Optional link back to the live `FcFontRegistry`. When present,
    /// chain resolution uses
    /// [`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]
    /// which lazy-parses system fonts as the DOM requests them
    /// (scout-on-demand). `None` falls back to querying whatever is
    /// already in the shared cache.
    pub registry: Option<Arc<rust_fontconfig::registry::FcFontRegistry>>,
    /// `FxHash` of the `prev_font_hashes` slice at the moment the last
    /// successful `collect_and_resolve_font_chains_with_registration`
    /// call populated `font_chain_cache`. Lets repeated layouts of the
    /// same DOM skip the ~1.5 ms (cold) / ~0.9 ms (warm) chain resolver
    /// when the set of font-family hashes has not changed. Cleared
    /// whenever `font_chain_cache` is explicitly emptied.
    pub last_resolved_font_stacks_sig: Option<u64>,
    /// Index of every font registered by FAMILY NAME into `fc_cache`'s
    /// in-memory font table (bundled fonts, embedder fonts, the built-in
    /// mock test fonts): normalized family name → the `FontMatch` the
    /// resolver should emit for it.
    ///
    /// WHY THIS EXISTS (architectural, see `resolve_font_chains_fast`):
    /// the fast chain resolver in rust-fontconfig 4.4
    /// (`FcFontRegistry::request_fonts_fast`) resolves families purely
    /// against `known_paths` — i.e. fonts that exist as FILES ON DISK.
    /// In-memory fonts are invisible to it, so a family registered with
    /// `FcFontCache::with_memory_fonts` could never be matched by name
    /// on the production path (which always has a live registry): it
    /// silently fell back to a system font. This index is consulted
    /// FIRST, before the disk probe, so a memory-registered family wins
    /// exactly as CSS says it should.
    pub memory_families: HashMap<String, Vec<MemoryFace>>,
    /// Baked static instances of variable fonts, keyed by a hash of the original
    /// VF bytes. A variable font is expanded into one static face per weight
    /// bucket (see `register_named_font`); this caches the minted faces so the
    /// several spelling registrations of the same VF don't re-bake it.
    pub(super) vf_bake_cache: HashMap<u64, Vec<(FontId, FaceStyle)>>,
    /// [`Self::get_loaded_fonts`]' snapshot, with the size and fingerprint of
    /// the pool it was taken from. Shared like `parsed_fonts`: a face added
    /// through any handle - the managers sharing the pool, the rasterizer
    /// locking it directly - changes the fingerprint, and the next call takes
    /// a new snapshot.
    pub(super) loaded_fonts_memo: Arc<Mutex<Option<(usize, u64, Arc<LoadedFonts<T>>)>>>,
}

impl<T: ParsedFontTrait> FontManager<T> {
    /// A second manager sharing this one's font pool.
    ///
    /// `fc_cache`, `parsed_fonts` and `registry` are shared handles — a font
    /// parsed through either manager is visible to both — and the resolved
    /// chain/name caches are copied. Use this to lay out the same content
    /// OUTSIDE the window pipeline (e.g. DOM→PDF from a callback) with
    /// exactly the fonts the window resolved on screen: same fallback
    /// chains, same embedded fonts, no re-parse from disk.
    #[must_use]
    pub fn clone_shared(&self) -> Self {
        Self {
            fc_cache: self.fc_cache.clone(),
            parsed_fonts: Arc::clone(&self.parsed_fonts),
            condemned_fonts: Arc::clone(&self.condemned_fonts),
            font_chain_cache: self.font_chain_cache.clone(),
            // SHARED, not copied. This used to be
            // `Mutex::new(self.embedded_fonts.lock().map(|m| m.clone())...)`,
            // i.e. a fork: a face registered in one manager was invisible to
            // every other, even though this method is documented as sharing
            // font data and `parsed_fonts` right above genuinely does.
            //
            // That fork is a silent-tofu generator. Whoever registers an
            // embedded face (the DOM's `StyleFontFamily::Ref`) and whoever
            // later shapes with it are frequently different managers - a child
            // window, a tray icon, an off-screen render - and the shaper then
            // falls back to a system face with no glyph at the icon's
            // private-use codepoint. Every step reports success.
            embedded_fonts: Arc::clone(&self.embedded_fonts),
            condemned_embedded_fonts: Arc::clone(&self.condemned_embedded_fonts),
            font_hash_to_families: self.font_hash_to_families.clone(),
            registry: self.registry.clone(),
            // Deliberately reset: the sig gates a chain-resolver skip that is
            // only valid against THIS manager's font_chain_cache history.
            last_resolved_font_stacks_sig: None,
            memory_families: self.memory_families.clone(),
            vf_bake_cache: self.vf_bake_cache.clone(),
            loaded_fonts_memo: Arc::clone(&self.loaded_fonts_memo),
        }
    }

    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cache cannot be initialized.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        // Generic families as Chrome resolves them (macOS `sans-serif` =
        // Helvetica): every window's fonts go through a FontManager.
        crate::font::loading::use_browser_generic_families(&fc_cache);
        let mut fm = Self {
            fc_cache,
            parsed_fonts: Arc::new(Mutex::new(HashMap::new())),
            condemned_fonts: Arc::new(Mutex::new(CondemnedFonts::default())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: Arc::new(Mutex::new(HashMap::new())),
            condemned_embedded_fonts: Arc::default(),
            font_hash_to_families: HashMap::new(),
            registry: None,
            last_resolved_font_stacks_sig: None,
            memory_families: HashMap::new(),
            vf_bake_cache: HashMap::new(),
            loaded_fonts_memo: Arc::new(Mutex::new(None)),
        };
        fm.register_builtin_mock_fonts();
        Ok(fm)
    }

    /// Register a font by FAMILY NAME from raw bytes, as an in-memory font
    /// in the shared `FcFontCache`.
    ///
    /// This is the ONE hook an embedder (or a test) uses to make a font
    /// resolvable by `font-family: "<family>"`. It mints one `FontId` for
    /// the font, inserts it into the fontconfig cache's memory-font table
    /// (so `get_font_bytes` / `load_fonts_from_disk` find it with no
    /// special-casing) and indexes it in [`Self::memory_families`] so the
    /// fast chain resolver can match it by name.
    ///
    /// `coverage` are the codepoint ranges the font actually covers.
    /// Passing the true ranges matters: the chain's coverage walk
    /// (`covering_font`) skips any font that reports no coverage, and a font claiming
    /// coverage it doesn't have would render .notdef instead of falling
    /// back.
    ///
    /// Returns the `FontId` the family now resolves to.
    pub fn register_named_font(
        &mut self,
        family: &str,
        bytes: &[u8],
        coverage: Vec<UnicodeRange>,
    ) -> FontId {
        self.register_named_font_in_tier(family, bytes, coverage, MemoryFontTier::Primary)
    }

    /// Register an in-memory face under `family` at an explicit
    /// [`MemoryFontTier`].
    ///
    /// [`Self::register_named_font`] is this with [`MemoryFontTier::Primary`].
    /// Use [`MemoryFontTier::Fallback`] to offer a face for a family without
    /// displacing whatever is installed - a caller that ships stand-in fonts for
    /// `serif`/`sans-serif`/`monospace` wants the system's faces to win on a
    /// desktop and its own to be there on wasm, and that is the tier that does
    /// both.
    pub fn register_named_font_in_tier(
        &mut self,
        family: &str,
        bytes: &[u8],
        coverage: Vec<UnicodeRange>,
        tier: MemoryFontTier,
    ) -> FontId {
        let norm = rust_fontconfig::utils::normalize_family_name(family);

        // Variable fonts: expand into one STATIC instance per weight bucket so the
        // ordinary static weight-selection path (see `split_memory_matches` /
        // `pick_memory_face`) picks the right one, with NO changes to shaping,
        // glyph decode, or PDF embedding — each baked instance is an ordinary
        // static font. Falls through to the static path below if the font is not a
        // bakeable variable font (baking failed / no glyf variations).
        if let Some((min, def, max)) = crate::font::parsed::read_wght_axis(bytes, 0) {
            if let Some(id) = self
                .register_variable_instances(&norm, family, bytes, &coverage, min, def, max, tier)
            {
                return id;
            }
        }

        // The weight/style come from the font BYTES (OS/2), not the registration
        // name: registering `Helvetica-Bold.ttf` under either "Helvetica-Bold" or
        // its internal family "Helvetica" must both yield weight=Bold. A font can
        // (and Helvetica does) reuse the same family name across faces, so faces
        // are distinguished by (weight, italic, oblique), never by name alone.
        let style = parse_face_style(bytes, family);

        // IDEMPOTENT: several `FontManager`s (one per window, plus the PDF
        // writer) share one `FcFontCache`. Registering the same face twice would
        // mint a second `FontId` for the same bytes, orphan the first in the
        // cache's metadata table and make the id non-deterministic. Reuse an
        // existing memory font ONLY when family AND (weight, italic, oblique)
        // match — a bold face must not be deduplicated against the regular one.
        let mut existing: Vec<(FontId, Vec<UnicodeRange>)> = Vec::new();
        self.fc_cache.for_each_pattern(|pattern, id| {
            let fam_hit = pattern
                .family
                .as_deref()
                .is_some_and(|f| rust_fontconfig::utils::normalize_family_name(f) == norm);
            let style_hit = pattern.weight == style.weight
                && (pattern.italic == PatternMatch::True) == style.italic
                && (pattern.oblique == PatternMatch::True) == style.oblique;
            if fam_hit && style_hit {
                existing.push((*id, pattern.unicode_ranges.clone()));
            }
        });
        let id = if let Some((id, ranges)) = existing
            .into_iter()
            .find(|(id, _)| self.fc_cache.is_memory_font(id))
        {
            self.index_memory_face(&norm, id, ranges, &style, tier);
            id
        } else {
            let pattern = rust_fontconfig::FcPattern {
                name: Some(family.to_string()),
                family: Some(family.to_string()),
                italic: if style.italic {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                },
                oblique: if style.oblique {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                },
                bold: if style.weight >= FcWeight::Bold {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                },
                weight: style.weight,
                stretch: style.stretch,
                unicode_ranges: coverage.clone(),
                ..Default::default()
            };
            let id = FontId::new();
            self.fc_cache.with_memory_font_with_id(
                id,
                pattern,
                rust_fontconfig::FcFont {
                    bytes: bytes.to_vec(),
                    font_index: 0,
                    id: family.to_string(),
                },
            );
            self.index_memory_face(&norm, id, coverage, &style, tier);
            id
        };
        id
    }

    /// Append (or refresh) a face in [`Self::memory_families`] under `norm`,
    /// de-duplicating by `FontId` so repeated registrations don't grow the list.
    fn index_memory_face(
        &mut self,
        norm: &str,
        id: FontId,
        unicode_ranges: Vec<UnicodeRange>,
        style: &FaceStyle,
        tier: MemoryFontTier,
    ) {
        let face = MemoryFace {
            tier,
            font_match: rust_fontconfig::FontMatch {
                id,
                unicode_ranges,
                fallbacks: Vec::new(),
            },
            weight: style.weight,
            italic: style.italic,
            oblique: style.oblique,
            stretch: style.stretch,
            weight_axis: style.weight_axis,
        };
        let faces = self.memory_families.entry(norm.to_string()).or_default();
        if let Some(slot) = faces.iter_mut().find(|f| f.font_match.id == id) {
            *slot = face;
        } else {
            faces.push(face);
        }
    }

    /// Expand a variable font (with a `wght` axis over `[min, max]`, default
    /// `def`) into one baked STATIC instance per standard weight bucket and
    /// register each as an in-memory face under `norm`. Returns the face nearest
    /// the fvar default, or `None` if no instance could be baked (caller then
    /// falls back to registering the raw bytes as a single static face).
    ///
    /// Baking is done once per unique VF bytes and cached (`vf_bake_cache`) so the
    /// several spelling registrations of the same font don't re-bake it.
    // Weight axis values are clamped to [1, 1000] and rounded before the cast, so
    // the f32 -> u16 conversion is bounded and sign-safe.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn register_variable_instances(
        &mut self,
        norm: &str,
        family: &str,
        bytes: &[u8],
        coverage: &[UnicodeRange],
        min: f32,
        def: f32,
        max: f32,
        tier: MemoryFontTier,
    ) -> Option<FontId> {
        let base = parse_face_style(bytes, family);
        let hash = {
            use core::hash::Hasher;
            let mut h = DefaultHasher::new();
            h.write(bytes);
            h.finish()
        };
        let def_bucket = FcWeight::from_u16(def.round().clamp(1.0, 1000.0) as u16);

        // Same VF already baked under another spelling: re-index, don't re-bake.
        if let Some(cached) = self.vf_bake_cache.get(&hash).cloned() {
            for (id, style) in &cached {
                self.index_memory_face(norm, *id, coverage.to_vec(), style, tier);
            }
            return cached
                .iter()
                .find(|(_, s)| s.weight == def_bucket)
                .or_else(|| cached.first())
                .map(|(id, _)| *id);
        }

        let lo = min.round().clamp(1.0, 1000.0) as u16;
        let hi = max.round().clamp(1.0, 1000.0) as u16;
        let mut baked: Vec<(FontId, FaceStyle)> = Vec::new();
        for w in [100u16, 200, 300, 400, 500, 600, 700, 800, 900] {
            if w < lo || w > hi {
                continue;
            }
            let Some(inst_bytes) =
                crate::font::parsed::bake_weight_instance(bytes, 0, f32::from(w))
            else {
                continue;
            };
            let style = FaceStyle {
                weight: FcWeight::from_u16(w),
                italic: base.italic,
                oblique: base.oblique,
                stretch: base.stretch,
                weight_axis: None,
            };
            let pattern = rust_fontconfig::FcPattern {
                name: Some(family.to_string()),
                family: Some(family.to_string()),
                italic: if style.italic {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                },
                oblique: if style.oblique {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                },
                bold: if style.weight >= FcWeight::Bold {
                    PatternMatch::True
                } else {
                    PatternMatch::False
                },
                weight: style.weight,
                stretch: style.stretch,
                unicode_ranges: coverage.to_vec(),
                ..Default::default()
            };
            let id = FontId::new();
            self.fc_cache.with_memory_font_with_id(
                id,
                pattern,
                rust_fontconfig::FcFont {
                    bytes: inst_bytes,
                    font_index: 0,
                    id: family.to_string(),
                },
            );
            self.index_memory_face(norm, id, coverage.to_vec(), &style, tier);
            baked.push((id, style));
        }

        if baked.is_empty() {
            return None;
        }
        let default_id = baked
            .iter()
            .find(|(_, s)| s.weight == def_bucket)
            .or_else(|| baked.first())
            .map(|(id, _)| *id)
            .unwrap();
        self.vf_bake_cache.insert(hash, baked);
        Some(default_id)
    }

    /// Register the built-in mock test fonts (see
    /// [`crate::text3::mock_fonts`]), then the bundled UI fonts
    /// ([`Self::register_builtin_ui_fonts`]). Called from every constructor:
    /// the mock families are only reachable if a stylesheet names them, and
    /// having them always present means tests exercise the *same* font
    /// path as production instead of a test-only bypass.
    pub fn register_builtin_mock_fonts(&mut self) {
        for (family, bytes) in crate::text3::mock_fonts::BUILTIN_MOCK_FONTS {
            self.register_named_font(family, bytes, crate::text3::mock_fonts::mock_font_ranges());
        }
        self.register_builtin_ui_fonts();
    }

    /// Register the UI fonts azul bundles for its widget themes (see
    /// [`crate::text3::ui_fonts`]: flora's EB Garamond), in the FALLBACK tier
    /// so an installed copy of the family wins. Static faces, so every
    /// constructor after the first takes the idempotent reuse path.
    pub fn register_builtin_ui_fonts(&mut self) {
        for (family, bytes) in crate::text3::ui_fonts::bundled_ui_fonts() {
            if bytes.is_empty() {
                continue;
            }
            self.register_named_font_in_tier(
                family,
                bytes,
                crate::text3::ui_fonts::eb_garamond_ranges(),
                MemoryFontTier::Fallback,
            );
        }
    }

    /// Create a `FontManager` sharing the font-path cache handle.
    ///
    /// The `parsed_fonts` pool starts empty. Fonts loaded during the first
    /// layout pass are cached and will be available on subsequent calls
    /// if you clone the `parsed_fonts` Arc before creating the next instance.
    /// For full sharing, prefer `from_arc_shared()`.
    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cache cannot be initialized.
    pub fn from_shared(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Self::new(fc_cache)
    }

    /// Create a `FontManager` sharing both the font-path cache and the
    /// already-parsed font data with another `FontManager`.
    ///
    /// This avoids re-reading and re-parsing font files from disk when
    /// rendering multiple documents that use the same fonts.
    /// # Errors
    ///
    /// Returns a `LayoutError` if the font cache cannot be initialized.
    pub fn from_arc_shared(
        fc_cache: FcFontCache,
        parsed_fonts: Arc<Mutex<HashMap<FontId, T>>>,
    ) -> Result<Self, LayoutError> {
        crate::font::loading::use_browser_generic_families(&fc_cache);
        let mut fm = Self {
            fc_cache,
            parsed_fonts,
            condemned_fonts: Arc::new(Mutex::new(CondemnedFonts::default())),
            font_chain_cache: HashMap::new(),
            embedded_fonts: Arc::new(Mutex::new(HashMap::new())),
            condemned_embedded_fonts: Arc::default(),
            font_hash_to_families: HashMap::new(),
            registry: None,
            last_resolved_font_stacks_sig: None,
            memory_families: HashMap::new(),
            vf_bake_cache: HashMap::new(),
            loaded_fonts_memo: Arc::new(Mutex::new(None)),
        };
        fm.register_builtin_mock_fonts();
        Ok(fm)
    }

    /// Attach a `FcFontRegistry` to this `FontManager` so subsequent
    /// chain-resolution calls use the on-demand path
    /// ([`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]).
    #[must_use]
    pub fn with_registry(
        mut self,
        registry: Arc<rust_fontconfig::registry::FcFontRegistry>,
    ) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Get a shareable handle to the parsed-font pool.
    ///
    /// Pass this to `from_arc_shared()` to create a new `FontManager` that
    /// reuses already-parsed fonts.
    #[must_use]
    pub fn shared_parsed_fonts(&self) -> Arc<Mutex<HashMap<FontId, T>>> {
        Arc::clone(&self.parsed_fonts)
    }

    /// Set the font chain cache from externally resolved chains
    ///
    /// This should be called with the result of `resolve_font_chains()` or
    /// `collect_and_resolve_font_chains()` from `solver3::getters`.
    pub fn set_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache = chains;
        self.last_resolved_font_stacks_sig = None;
    }

    /// Set the font chain cache and record the input signature so
    /// subsequent layouts with the same `prev_font_hashes` skip the
    /// resolver. Pass `sig = None` if the caller cannot compute a
    /// reliable signature — equivalent to the single-arg
    /// `set_font_chain_cache`.
    pub fn set_font_chain_cache_with_sig(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        sig: Option<u64>,
    ) {
        // (2026-06-10: reverted to HashMap — the empty-map RawIter hang behind the 2026-06-05
        // BTreeMap migration was the un-mirrored hashbrown EMPTY_GROUP static, fixed
        // transpiler-side.)
        self.font_chain_cache = chains;
        self.last_resolved_font_stacks_sig = sig;
    }

    /// Merge additional font chains into the existing cache
    ///
    /// Useful when processing multiple DOMs that may have different font requirements.
    pub fn merge_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache.extend(chains);
    }

    /// Get a reference to the font chain cache
    #[must_use]
    pub const fn get_font_chain_cache(
        &self,
    ) -> &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain> {
        &self.font_chain_cache
    }

    /// Get an embedded font by its hash (used for `WebRender` registration)
    /// Returns the `FontRef` if it exists in the `embedded_fonts` cache.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn get_embedded_font_by_hash(
        &self,
        font_hash: u64,
    ) -> Option<azul_css::props::basic::FontRef> {
        let mut embedded = self.embedded_fonts.lock().unwrap();
        if let Some(font) = embedded.get(&font_hash) {
            return Some(font.clone());
        }
        // A face the embedded-font GC condemned is still drawn by someone
        // who asks for it: back into the pool.
        let (font, _) = self
            .condemned_embedded_fonts
            .lock()
            .unwrap()
            .faces
            .remove(&font_hash)?;
        embedded.insert(font_hash, font.clone());
        Some(font)
    }

    /// Get a parsed font by its hash (used for `WebRender` registration)
    /// Returns the parsed font if it exists in the `parsed_fonts` cache.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn get_font_by_hash(&self, font_hash: u64) -> Option<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // Linear search through all cached fonts to find one with matching hash
        let found = parsed
            .iter()
            .find(|(_, font)| font.get_hash() == font_hash)
            .map(|(_, font)| font.clone());
        drop(parsed);
        if found.is_some() {
            return found;
        }
        // RESURRECTION: the hash is still referenced (someone is resolving
        // it), so a GC-condemned face moves back into the live pool. This is
        // what makes the font GC safe against shaping caches that serve
        // cached glyphs without re-loading their face.
        let mut condemned = self.condemned_fonts.lock().unwrap();
        let id = condemned
            .faces
            .iter()
            .find(|(_, (font, _))| font.get_hash() == font_hash)
            .map(|(id, _)| *id)?;
        let (font, _) = condemned.faces.remove(&id)?;
        drop(condemned);
        if env_flag!("AZ_FONT_GC_TRACE") {
            eprintln!("[azul][font][gc] RESURRECT id={id} face_hash={font_hash}");
        }
        self.parsed_fonts.lock().unwrap().insert(id, font.clone());
        Some(font)
    }

    /// THE font lookup: resolve a `font_hash` — the value layout stamps onto every
    /// shaped glyph and carries in `DisplayListItem::Text` — back to the face that
    /// produced it.
    ///
    /// A `FontManager` shapes with faces from TWO pools: `parsed_fonts` (loaded from
    /// the resolved font chains) and `embedded_fonts` (handed to it directly by the
    /// DOM as `StyleFontFamily::Ref` — Material Icons and every other
    /// `FontStack::Ref`). Both can put a hash in the display list, so a renderer that
    /// consults only one of them silently drops user-visible text. That is exactly
    /// what shipped in 0.2.0: the CPU renderer searched `parsed_fonts` alone, so
    /// every widget icon vanished with `[cpurender] Font hash … not found in
    /// FontManager` while layout had happily measured and positioned it.
    ///
    /// Every renderer resolves through this one function, so "layout produced this
    /// hash" and "the renderer can draw this hash" cannot disagree.
    ///
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn resolve_font_by_hash(&self, font_hash: u64) -> Option<azul_css::props::basic::FontRef>
    where
        T: Into<azul_css::props::basic::FontRef> + Clone,
    {
        if let Some(embedded) = self.get_embedded_font_by_hash(font_hash) {
            return Some(embedded);
        }
        self.get_font_by_hash(font_hash).map(Into::into)
    }

    /// Register an embedded `FontRef` for later lookup by hash
    /// This is called when using `FontStack::Ref` during shaping
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn register_embedded_font(&self, font_ref: &azul_css::props::basic::FontRef) {
        let hash = font_ref.get_hash();
        let mut embedded = self.embedded_fonts.lock().unwrap();
        self.condemned_embedded_fonts
            .lock()
            .unwrap()
            .faces
            .remove(&hash);
        embedded.insert(hash, font_ref.clone());
    }

    /// How many `StyleFontFamily::Ref` faces the pool holds, condemned ones
    /// included.
    ///
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn embedded_font_count(&self) -> usize {
        self.embedded_fonts.lock().unwrap().len()
            + self.condemned_embedded_fonts.lock().unwrap().faces.len()
    }

    /// How many embedded faces the GC has dropped, ever (see
    /// [`CondemnedEmbeddedFonts::dropped`]).
    ///
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn embedded_fonts_dropped(&self) -> u64 {
        self.condemned_embedded_fonts.lock().unwrap().dropped
    }

    /// EMBEDDED FONT GC: the `StyleFontFamily::Ref` faces no display list of
    /// the window draws with (`live`: the font hashes of every DOM's display
    /// list) are CONDEMNED, and dropped when still undrawn two runs later -
    /// the pool's handle and every family of the reverse map that names one.
    /// What a page of a document loaded (the fonts of a PDF page's SVG) is
    /// freed once the page scrolls away and its DOM goes; before, the pool
    /// kept every face it was ever handed.
    ///
    /// A face drawn again before it is dropped comes back (a resolution
    /// resurrects it, [`Self::get_embedded_font_by_hash`]); a DOM that still
    /// names a dropped one registers it again on its next layout (the
    /// [`CondemnedEmbeddedFonts::dropped`] count is in its signature).
    ///
    /// Returns how many faces were dropped.
    ///
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn collect_embedded_fonts(&mut self, live: &HashSet<u64>) -> usize {
        let mut embedded = self.embedded_fonts.lock().unwrap();
        let mut condemned = self.condemned_embedded_fonts.lock().unwrap();
        condemned.generation = condemned.generation.saturating_add(1);
        let generation = condemned.generation;
        // Drawn again: back.
        let back: Vec<u64> = condemned
            .faces
            .keys()
            .filter(|hash| live.contains(hash))
            .copied()
            .collect();
        for hash in back {
            if let Some((font, _)) = condemned.faces.remove(&hash) {
                embedded.insert(hash, font);
            }
        }
        // Undrawn: condemned.
        let undrawn: Vec<u64> = embedded
            .keys()
            .filter(|hash| !live.contains(hash))
            .copied()
            .collect();
        for hash in undrawn {
            if let Some(font) = embedded.remove(&hash) {
                condemned.faces.insert(hash, (font, generation));
            }
        }
        // Undrawn for two runs: dropped.
        let mut dropped = HashSet::new();
        condemned.faces.retain(|hash, (_, condemned_at)| {
            let keep = generation.saturating_sub(*condemned_at) < 2;
            if !keep {
                dropped.insert(*hash);
            }
            keep
        });
        condemned.dropped = condemned.dropped.saturating_add(dropped.len() as u64);
        drop(condemned);
        drop(embedded);
        if !dropped.is_empty() {
            use azul_css::props::basic::font::StyleFontFamily;
            self.font_hash_to_families.retain(|_, families| {
                !families.as_ref().iter().any(|family| {
                    matches!(family, StyleFontFamily::Ref(font) if dropped.contains(&font.get_hash()))
                })
            });
        }
        dropped.len()
    }

    /// Get a snapshot of all currently loaded fonts
    ///
    /// This returns a copy of all parsed fonts, which can be passed to the shaper.
    /// No locking is required after this call - the returned `HashMap` is independent.
    ///
    /// NOTE: This should be called AFTER loading all required fonts for a layout pass.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    ///
    /// Memoized: every inline formatting context of a layout asks, and the
    /// pool rarely changes between them. Building the two maps per call was a
    /// fifth of a 300-contact list's layout (1 500 IFCs x every loaded face,
    /// AzContacts, 2026-10-06); checking the pool's fingerprint allocates
    /// nothing.
    #[must_use]
    pub fn get_loaded_fonts(&self) -> Arc<LoadedFonts<T>> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // Order-free over every (id, face) pair: a face added, dropped or
        // replaced under the same id changes it.
        let fingerprint = parsed.iter().fold(0u64, |acc, (id, font)| {
            #[allow(clippy::cast_possible_truncation)] // the halves of the u128 id
            let id = (id.0 as u64) ^ ((id.0 >> 64) as u64).rotate_left(32);
            acc.wrapping_add(
                (id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ font.get_hash())
                    .wrapping_mul(0xC2B2_AE3D_27D4_EB4F),
            )
        });
        let mut memo = self.loaded_fonts_memo.lock().unwrap();
        if let Some((len, seen, fonts)) = memo.as_ref() {
            if *len == parsed.len() && *seen == fingerprint {
                return Arc::clone(fonts);
            }
        }
        let fonts: Arc<LoadedFonts<T>> = Arc::new(
            parsed
                .iter()
                .map(|(id, font)| (*id, font.shallow_clone()))
                .collect(),
        );
        *memo = Some((parsed.len(), fingerprint, Arc::clone(&fonts)));
        fonts
    }

    /// Get the set of `FontIds` that are currently loaded
    ///
    /// This is useful for computing which fonts need to be loaded
    /// (diff with required fonts).
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn get_loaded_font_ids(&self) -> HashSet<FontId> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // M12.7: skip hashbrown's RawIterRange on an empty map — its NEON
        // control-byte group-scan mis-lifts to wasm and iterates forever
        // (the headless web layout uses an empty font cache → parsed is
        // empty here). is_empty() is len-based (no iteration), so it is safe.
        if parsed.is_empty() {
            return HashSet::new();
        }
        unsafe { crate::az_mark(0x60788, 0xA1) };
        let out = parsed.keys().copied().collect();
        drop(parsed);
        unsafe { crate::az_mark(0x6078C, 0xA2) };
        out
    }

    /// The metrics of `font_stack`'s FIRST AVAILABLE FONT (CSS Fonts 4 §5.5:
    /// the first face of the family list that covers U+0020 SPACE), in font
    /// units - the face the strut of a block container's line boxes takes
    /// its ascent and descent from (CSS 2.2 §10.8.1), resolved through the
    /// same chain the shaper resolves the container's text with. `None`
    /// while the stack's chain is unresolved or the face is not loaded yet.
    #[must_use]
    pub fn first_available_font_metrics(&self, font_stack: &FontStack) -> Option<LayoutFontMetrics> {
        match font_stack {
            FontStack::Ref(font_ref) => Some(font_ref.get_font_metrics()),
            FontStack::Stack(selectors) => {
                let chain = self
                    .font_chain_cache
                    .get(&FontChainKey::from_selectors(selectors))?;
                let id = covering_font(chain, ' ')
                    .or_else(|| chain.resolve_codepoint(u32::from(' ')).map(|(id, _)| id))?;
                let parsed = self.parsed_fonts.lock().ok()?;
                parsed.get(&id).map(ParsedFontTrait::get_font_metrics)
            }
        }
    }

    /// Insert a loaded font into the cache
    ///
    /// Returns the old font if one was already present for this `FontId`.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn insert_font(&self, font_id: FontId, font: T) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.insert(font_id, font)
    }

    /// Insert multiple loaded fonts into the cache
    ///
    /// This is more efficient than calling `insert_font` multiple times
    /// because it only acquires the lock once.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn insert_fonts(&self, fonts: impl IntoIterator<Item = (FontId, T)>) {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        for (font_id, font) in fonts {
            parsed.insert(font_id, font);
        }
    }

    /// One-shot helper that resolves "what fonts does `chains` need
    /// that this manager hasn't loaded yet" and loads them via the
    /// supplied `load_fn` closure (typically
    /// `PathLoader::load_font_shared` for the production lazy-decode
    /// path). Updates `parsed_fonts` in place and returns any failures
    /// for the caller to log.
    ///
    /// Replaces the same four-step `collect → compute_diff →
    /// load_from_disk → insert_fonts` dance previously inlined in
    /// `LayoutWindow::layout_document`, the CPU rasterizer pre-fill
    /// in `cpurender.rs`, and `FontContext::load_fonts_for_chains`.
    pub fn load_missing_for_chains<F>(
        &self,
        chains: &crate::solver3::getters::ResolvedFontChains,
        load_fn: F,
    ) -> Vec<(FontId, String)>
    where
        F: Fn(Arc<rust_fontconfig::FontBytes>, usize) -> Result<T, LayoutError>,
    {
        use crate::solver3::getters::{
            collect_font_ids_from_chains, compute_fonts_to_load, load_fonts_from_disk,
        };
        let required = collect_font_ids_from_chains(chains);
        let already = self.get_loaded_font_ids();
        let to_load = compute_fonts_to_load(&required, &already);
        if to_load.is_empty() {
            return Vec::new();
        }
        let result = load_fonts_from_disk(&to_load, &self.fc_cache, load_fn);
        self.insert_fonts(result.loaded);
        result.failed
    }

    /// Give every character of `content` a face that can draw it, extending
    /// the cached chains and loading the new faces — the edit-time half of
    /// [`missing_coverage_faces`] (the full layout applies the same helper to
    /// the content overlay before it loads its chains).
    ///
    /// A key with no cached chain gets the same on-miss resolution shaping
    /// itself would perform, so the extension has something to hang off.
    /// Returns the number of faces added; 0 is the steady state for every
    /// keystroke in a script the chain already covers. The chain-resolution
    /// signature is left alone: a chain that grew is still the chain for the
    /// same DOM.
    pub fn extend_chains_for_content<F>(&mut self, content: &[InlineContent], load_fn: F) -> usize
    where
        F: Fn(Arc<rust_fontconfig::FontBytes>, usize) -> Result<T, LayoutError>,
    {
        let loaded = self.get_loaded_fonts();
        let fc_cache = self.fc_cache.clone();
        let registry = self.registry.clone();
        let additions = {
            let cache = &self.font_chain_cache;
            missing_coverage_faces(
                content,
                &mut |key| {
                    Some(
                        cache
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| resolve_chain_on_miss(key, &fc_cache)),
                    )
                },
                &fc_cache,
                registry.as_deref(),
                &loaded,
            )
        };
        if additions.is_empty() {
            return 0;
        }
        let mut resolved = crate::solver3::getters::ResolvedFontChains::default();
        let mut added = 0usize;
        for (key, faces) in additions {
            let chain = self
                .font_chain_cache
                .entry(key.clone())
                .or_insert_with(|| resolve_chain_on_miss(&key, &fc_cache));
            added += faces.len();
            append_coverage_faces(chain, faces);
            resolved
                .chains
                .insert(FontChainKeyOrRef::Chain(key), chain.clone());
        }
        let _failed = self.load_missing_for_chains(&resolved, load_fn);
        added
    }

    /// Replace the backing `FcFontCache` and re-register the built-in memory fonts.
    ///
    /// Memory fonts (the mock test fonts, and any `register_named_font` bytes) live
    /// ONLY inside the cache. A bare `self.fc_cache = new` therefore strands them: their
    /// `FontId`s stay in `memory_families` but their bytes are gone with the old cache,
    /// so chain resolution matches them yet loading fails and text silently falls back
    /// (e.g. `font-family: "Azul Mock Mono"` measuring with the fallback font's metrics).
    /// Use this whenever the cache is swapped for a fresh snapshot (registry handle,
    /// rebuilt system cache) instead of assigning the field directly.
    pub fn replace_fc_cache(&mut self, fc_cache: FcFontCache) {
        self.fc_cache = fc_cache;
        self.drop_dangling_memory_faces();
        self.register_builtin_mock_fonts();
    }

    /// Evict every entry of the memory-font INDEX (`memory_families`,
    /// `vf_bake_cache`) whose `FontId` the *current* `fc_cache` does not know.
    ///
    /// `memory_families` is not a font store, it is an index INTO the cache: the
    /// bytes live in `fc_cache`, the index only remembers which `FontId` a
    /// (family, weight, slant) resolves to. Swapping the cache therefore
    /// invalidates the whole index at once, and leaving it in place is worse
    /// than losing it — a dangling id still MATCHES during chain resolution, so
    /// `font-family: "X"` resolves "successfully" to an id that
    /// `load_missing_for_chains` can no longer load, and the text silently
    /// re-measures with the fallback font's metrics (line-height 1.2 instead of
    /// the face's own ascent/descent).
    ///
    /// Re-registering the built-in mock fonts does NOT repair this by itself:
    /// `register_named_font` cannot find the family in the fresh cache, so it
    /// mints a NEW `FontId`; `index_memory_face` de-duplicates by `FontId`, so
    /// the new face is APPENDED next to the dead one; and `pick_memory_face`
    /// returns the FIRST face of the best weight — i.e. the dead one, forever.
    /// (It also grew the index by one dead face per cache swap, and the DLL
    /// swaps on every `regenerate_layout`.)
    fn drop_dangling_memory_faces(&mut self) {
        let fc_cache = &self.fc_cache;
        self.memory_families.retain(|_, faces| {
            faces.retain(|f| fc_cache.is_memory_font(&f.font_match.id));
            !faces.is_empty()
        });
        // Same reasoning for the variable-font bake cache: its ids are handed
        // straight back to `index_memory_face` on the "already baked" path.
        self.vf_bake_cache
            .retain(|_, baked| baked.iter().all(|(id, _)| fc_cache.is_memory_font(id)));
    }

    /// Remove a font from the cache
    ///
    /// Returns the removed font if it was present.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    #[must_use]
    pub fn remove_font(&self, font_id: &FontId) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.remove(font_id)
    }

    /// FONT GC — evict everything the CURRENT document no longer references.
    ///
    /// `keep_ids` are the `FontId`s reachable from the font chains just resolved
    /// for this document; `keep_hashes` are the font-family hashes present in its
    /// CSS property cache. Anything else belonged to a node that is gone.
    ///
    /// Without this, `parsed_fonts` / `font_hash_to_families` only ever GREW: a
    /// font loaded for one node stayed resident for the life of the window even
    /// after the node (and every other user of that family) disappeared — an app
    /// that cycles fonts (font picker, editor, live CSS) leaked every font it ever
    /// touched.
    ///
    /// Eviction is always safe: `load_missing_for_chains` re-loads any font a
    /// later layout turns out to need. The cost of a wrong guess is one re-parse,
    /// never a missing glyph.
    ///
    /// Returns the number of parsed fonts evicted.
    /// # Panics
    ///
    /// Panics if the internal font-cache mutex is poisoned.
    pub fn garbage_collect_fonts(
        &mut self,
        keep_ids: &HashSet<FontId>,
        keep_hashes: &HashSet<u64>,
    ) -> usize {
        let trace = env_flag!("AZ_FONT_GC_TRACE");
        let mut condemned = self.condemned_fonts.lock().unwrap();
        condemned.generation = condemned.generation.saturating_add(1);
        let generation = condemned.generation;

        // Phase 1: CONDEMN (not drop) everything outside the keep-set. A
        // condemned face stays resolvable by hash — `get_font_by_hash`
        // resurrects it — because shaping caches legitimately keep serving
        // glyphs stamped with its hash without re-loading the face.
        let evicted = {
            let mut parsed = self.parsed_fonts.lock().unwrap();
            let before = parsed.len();
            let goners: Vec<FontId> = parsed
                .keys()
                .filter(|id| !keep_ids.contains(*id))
                .copied()
                .collect();
            for id in goners {
                if let Some(font) = parsed.remove(&id) {
                    if trace {
                        eprintln!(
                            "[azul][font][gc] CONDEMN id={id} face_hash={} gen={generation}",
                            font.get_hash()
                        );
                    }
                    condemned.faces.insert(id, (font, generation));
                }
            }
            before.saturating_sub(parsed.len())
        };

        // Phase 2: truly drop faces nobody resolved for two generations —
        // the leak the GC exists for (font-cycling apps retained every font
        // they ever touched) stays fixed.
        condemned.faces.retain(|id, (font, gen)| {
            let live = generation.saturating_sub(*gen) < 2;
            if !live && trace {
                eprintln!(
                    "[azul][font][gc] DROP id={id} face_hash={} (condemned at gen {gen}, now \
                     {generation})",
                    font.get_hash()
                );
            }
            live
        });
        drop(condemned);

        self.font_hash_to_families
            .retain(|h, _| keep_hashes.contains(h));
        evicted
    }
}
