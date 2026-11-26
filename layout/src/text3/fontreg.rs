//! Fonts an app registers by family name: the faces it ships and the faces that arrive while it
//! runs.
//!
//! The faces an app ships go in before its first window (`AppConfig::add_font`,
//! [`RegisteredFonts::set_app_fonts`]). A face that arrives later - a mail's remote web font,
//! fetched after the mail is shown - is registered with [`RegisteredFonts::register_font`]: every
//! window of the app draws the text that names its family in it at its next layout, a window
//! opened later and a replaced font cache included.
//!
//! A registered face gets ONE `FontId` for good, and each font cache it is put in holds it under
//! that id, so a chain resolved by one window loads in any other. Registering bumps a generation
//! the layout reads ([`RegisteredFonts::generation`]): a window resolves its font chains again,
//! lays its documents out cold and shapes its text afresh, since what it shaped before fell back
//! to another face. A registered face works like an `@font-face` rule: it wins over an installed
//! face of the same family.
//!
//! The bytes stay for the life of the registry; there is no unregistering.
//!
//! # Who owns them
//!
//! A [`RegisteredFonts`] is a plain value, not process-wide state. A `FontManager` holds one
//! (`FontManager::registered_fonts`), and `FontManager::new` makes a fresh one: a window built on
//! its own, a test and a text image each register in a registry of their own and see nothing of
//! any other. An app makes ONE (`AppInternal::create`), its font manager hands that to every
//! window it makes (`FontManager::clone_shared` shares it) and a child window takes the one of
//! its parent, so that a face registered through one window is seen by all of them.
//!
//! A layout callback is handed the registry of its window with its info
//! (`LayoutCallbackInfo::registered_fonts`, opaque to azul-core): the font cache snapshot it makes
//! (`FontCacheSnapshot::from_layout_info`) is built on it, so that a document it paginates is
//! measured with the faces the screen draws with.

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, RwLock,
    },
};

use rust_fontconfig::{FcFontCache, FontId, UnicodeRange};

use crate::text3::cache::{
    cmap_coverage, memory_face, memory_font_pattern, parse_face_style, FaceStyle, MemoryFace,
    MemoryFontTier,
};

/// One face registered at runtime.
struct RuntimeFace {
    /// The family name as the app gave it (what the font cache records).
    family: String,
    /// The normalized family name font stacks are matched on.
    norm: String,
    /// The face's id, the same in every font cache it is put in.
    id: FontId,
    /// The TrueType / OpenType bytes, shared with the face a font manager loads from them
    /// (`FcFont.bytes` of every font cache the face is put in is this same allocation).
    bytes: Arc<[u8]>,
    /// The code points of its cmap, read once.
    ranges: Vec<UnicodeRange>,
    /// Its weight, slant and stretch, read once: `parse_face_style` parses
    /// the whole face (every cmap mapping, every name record, a copy of the
    /// bytes per face). [`RegisteredFonts::ensure_in`] parsed every registered face on every
    /// call - once per registration of each later face, and twice per chain
    /// resolution of every window once anything was registered.
    style: FaceStyle,
}

/// One face the app ships ([`RegisteredFonts::set_app_fonts`]): every font manager built on the
/// registry registers it.
#[derive(Debug, Clone)]
pub(crate) struct AppFace {
    /// The family a stylesheet names it by.
    pub(crate) family: String,
    /// The TrueType / OpenType bytes, shared with every font cache a manager puts the face into
    /// (rust-fontconfig 5.1 `FcFont.bytes: Arc<[u8]>`).
    pub(crate) bytes: Arc<[u8]>,
    /// The code points of its cmap, read once when the app registered it.
    pub(crate) ranges: Vec<UnicodeRange>,
    /// Its weight, slant and stretch, read once when the app registered it (every font manager
    /// built on the registry registers it).
    pub(crate) style: FaceStyle,
}

/// The faces registered by family name for one app - the ones it ships and the ones it registers
/// while it runs - and the generation that counts the registrations.
///
/// Held behind an `Arc` by the `FontManager`s of the app: see the module docs for who makes one
/// and who shares it. Not rust-fontconfig's `FcFontRegistry`, which scans the system's fonts.
#[derive(Default)]
pub struct RegisteredFonts {
    /// The faces the app ships, installed by [`Self::set_app_fonts`].
    app_faces: RwLock<Vec<AppFace>>,
    /// Every face registered while the app runs, in registration order.
    faces: RwLock<Vec<RuntimeFace>>,
    /// How many registrations changed `faces`: the layout's signal that font chains and shaped
    /// text may be stale.
    generation: AtomicU64,
    /// Whether the app has made its one check of the system UI font
    /// ([`Self::claim_system_ui_font_check`]).
    system_ui_font_checked: AtomicBool,
    /// Tests: how often this registry has parsed the style of a face (`parse_style`).
    #[cfg(test)]
    style_parses: core::sync::atomic::AtomicUsize,
    /// Tests: how many faces this registry has kept the bytes of (the new ones).
    #[cfg(test)]
    bytes_kept: core::sync::atomic::AtomicUsize,
}

impl core::fmt::Debug for RegisteredFonts {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Counts, not faces: a face is a whole font file.
        f.debug_struct("RegisteredFonts")
            .field("app_faces", &self.app_faces.read().map_or(0, |faces| faces.len()))
            .field("faces", &self.faces.read().map_or(0, |faces| faces.len()))
            .field("generation", &self.generation())
            .field(
                "system_ui_font_checked",
                &self.system_ui_font_checked.load(Ordering::Acquire),
            )
            .finish()
    }
}

impl RegisteredFonts {
    /// Claims the one check of the system UI font an app makes after its font scan
    /// (`font::loading::check_system_ui_font`): `true` for the first caller, `false` for every
    /// later one, of every window and every regeneration of the layout. The windows of an app
    /// hold this one registry, so the check - and its warning - is the app's, made once.
    pub fn claim_system_ui_font_check(&self) -> bool {
        !self.system_ui_font_checked.swap(true, Ordering::AcqRel)
    }

    /// Installs the faces the app ships (`AppConfig::bundled_fonts`): every font manager built on
    /// this registry registers them (`FontManager::register_builtin_ui_fonts` has the rules).
    /// Replaces what an earlier call installed. A face whose bytes do not parse is left out.
    ///
    /// Not a registration in the sense of [`Self::generation`]: the faces go in before the first
    /// window exists, and a manager made afterwards has them.
    pub fn set_app_fonts(&self, fonts: &[azul_core::resources::NamedFont]) {
        let faces: Vec<AppFace> = fonts
            .iter()
            .filter_map(|font| {
                let ranges = cmap_coverage(font.bytes.as_ref());
                (!ranges.is_empty()).then(|| AppFace {
                    family: font.name.as_str().to_string(),
                    bytes: Arc::from(font.bytes.as_ref()),
                    ranges,
                    style: self.parse_style(font.bytes.as_ref(), font.name.as_str()),
                })
            })
            .collect();
        if let Ok(mut slot) = self.app_faces.write() {
            *slot = faces;
        }
    }

    /// The faces the app ships ([`Self::set_app_fonts`]), as the font manager registers them.
    pub(crate) fn app_faces(&self) -> Vec<AppFace> {
        self.app_faces
            .read()
            .map_or_else(|_| Vec::new(), |faces| faces.clone())
    }

    /// Registers the face `bytes` under `family`. Returns `false` for bytes that are no font
    /// (nothing registered). Registering the same bytes under the same family again changes
    /// nothing.
    ///
    /// The registry keeps its own copy of `bytes` (one, when the face is new). A caller that
    /// holds the face as an `Arc<[u8]>` hands it to [`Self::register_font_shared`] instead, which
    /// keeps that allocation.
    #[must_use]
    pub fn register_font(&self, family: &str, bytes: &[u8]) -> bool {
        self.register_with(family, bytes, || Arc::from(bytes))
    }

    /// [`Self::register_font`] for a face the caller holds as an `Arc<[u8]>`: the registry keeps
    /// `bytes` itself - the font caches and every face loaded from it share that allocation - and
    /// copies nothing (a web font, WOFF or WOFF2, is unpacked into a plain font of its own, as by
    /// `register_font`).
    #[must_use]
    pub fn register_font_shared(&self, family: &str, bytes: Arc<[u8]>) -> bool {
        self.register_with(family, &bytes, || Arc::clone(&bytes))
    }

    /// Registers the face `bytes`; `kept` makes the registry's allocation of them, called only
    /// when the face is new and is a plain font.
    fn register_with(
        &self,
        family: &str,
        bytes: &[u8],
        kept: impl FnOnce() -> Arc<[u8]>,
    ) -> bool {
        // A web font (WOFF, WOFF2) is unpacked once into the plain font every reader takes.
        let unpacked: Option<Arc<[u8]>> = sfnt_of(bytes).map(Arc::from);
        let bytes = unpacked.as_deref().unwrap_or(bytes);
        let ranges = cmap_coverage(bytes);
        if ranges.is_empty() {
            return false;
        }
        let norm = rust_fontconfig::utils::normalize_family_name(family);
        let known =
            |faces: &[RuntimeFace]| faces.iter().any(|f| f.norm == norm && *f.bytes == *bytes);
        if self.faces.read().is_ok_and(|faces| known(&faces)) {
            return true;
        }
        // Parsed outside the lock; a face registered meanwhile is found below.
        let style = self.parse_style(bytes, family);
        let Ok(mut faces) = self.faces.write() else {
            return false;
        };
        if known(&faces) {
            return true;
        }
        #[cfg(test)]
        self.bytes_kept.fetch_add(1, Ordering::Relaxed);
        let bytes = match unpacked {
            Some(unpacked) => unpacked,
            None => kept(),
        };
        faces.push(RuntimeFace {
            family: family.to_string(),
            norm,
            id: FontId::new(),
            bytes,
            ranges,
            style,
        });
        self.generation.fetch_add(1, Ordering::AcqRel);
        true
    }

    /// The weight, slant and stretch of the face `bytes`: a whole-face parse. Every parse of a
    /// face's style by this registry goes through here - once for a face when it is registered or
    /// set, never when it is put in a font cache ([`Self::ensure_in`]).
    // `self` owns the counter of the parses, which only the tests keep
    #[allow(clippy::unused_self)]
    fn parse_style(&self, bytes: &[u8], family: &str) -> FaceStyle {
        #[cfg(test)]
        self.style_parses.fetch_add(1, Ordering::Relaxed);
        parse_face_style(bytes, family)
    }

    /// How many registrations changed the registry so far; 0 before the first.
    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    /// Puts every registered face that `fc_cache` does not hold yet into it, under its id, and
    /// returns each face's index entry with its normalized family (the
    /// `FontManager::memory_families` form).
    pub(crate) fn ensure_in(&self, fc_cache: &FcFontCache) -> Vec<(String, MemoryFace)> {
        let Ok(faces) = self.faces.read() else {
            return Vec::new();
        };
        faces
            .iter()
            .map(|face| {
                let style = face.style;
                if !fc_cache.is_memory_font(&face.id) {
                    fc_cache.with_memory_font_with_id(
                        face.id,
                        memory_font_pattern(&face.family, &style, face.ranges.clone()),
                        rust_fontconfig::FcFont {
                            // The registry's allocation itself (rust-fontconfig 5.1).
                            bytes: Arc::clone(&face.bytes),
                            font_index: 0,
                            id: face.family.clone(),
                        },
                    );
                }
                let entry = memory_face(
                    face.id,
                    face.ranges.clone(),
                    &style,
                    MemoryFontTier::Primary,
                );
                (face.norm.clone(), entry)
            })
            .collect()
    }

    /// `memory_families` (a font manager's index) with the registered faces added, each put into
    /// `fc_cache` first: what a window's chain resolution matches family names against. Borrowed
    /// as it is while nothing is registered.
    pub(crate) fn with_runtime_faces<'a>(
        &self,
        memory_families: &'a HashMap<String, Vec<MemoryFace>>,
        fc_cache: &FcFontCache,
    ) -> Cow<'a, HashMap<String, Vec<MemoryFace>>> {
        if self.generation() == 0 {
            return Cow::Borrowed(memory_families);
        }
        let mut merged = memory_families.clone();
        for (norm, face) in self.ensure_in(fc_cache) {
            let faces = merged.entry(norm).or_default();
            if !faces.iter().any(|f| f.font_match.id == face.font_match.id) {
                faces.push(face);
            }
        }
        Cow::Owned(merged)
    }

    /// The bytes of the registered face `id`, shared; `None` for any other id. Tests: the font
    /// caches and the faces loaded from them hold this allocation.
    #[cfg(test)]
    pub(crate) fn bytes_of(&self, id: &FontId) -> Option<Arc<[u8]>> {
        let faces = self.faces.read().ok()?;
        faces
            .iter()
            .find(|face| face.id == *id)
            .map(|face| Arc::clone(&face.bytes))
    }
}

impl crate::callbacks::CallbackInfo {
    /// Registers the font `bytes` under the family name `family` for the whole app, while it
    /// runs: text whose `font-family` names it is drawn in it at each window's next layout
    /// (return `Update::RefreshDom` to lay out at once). Weight and style come from the face
    /// itself, so the faces of one family are registered one by one. Returns `false` for bytes
    /// that are no font.
    pub fn register_font(&mut self, family: azul_css::AzString, bytes: azul_css::U8Vec) -> bool {
        let fonts = &self.get_layout_window().font_manager;
        let registered = fonts
            .registered_fonts
            .register_font(family.as_str(), bytes.as_ref());
        if registered {
            // This window's cache takes the face now; every other cache takes it when its window
            // resolves its fonts again.
            drop(fonts.registered_fonts.ensure_in(&fonts.fc_cache));
        }
        registered
    }

    /// [`Self::register_font`] for bytes the app shares (`SharedBytes`): the registry keeps that
    /// allocation - every window's font cache and every face loaded from it share it with the
    /// app's own handle - and copies nothing. The bytes stay alive as long as the app's
    /// registry, as every registered face does, whatever the app does with its handle
    /// afterwards.
    pub fn register_font_shared(
        &mut self,
        family: azul_css::AzString,
        bytes: azul_css::shared_bytes::SharedBytes,
    ) -> bool {
        let fonts = &self.get_layout_window().font_manager;
        let registered = fonts
            .registered_fonts
            .register_font_shared(family.as_str(), bytes.to_arc());
        if registered {
            drop(fonts.registered_fonts.ensure_in(&fonts.fc_cache));
        }
        registered
    }
}

/// `bytes` as a plain TrueType / OpenType font when they are a WOFF or a WOFF2 (a web font: its
/// tables compressed): the tables unpacked and written out as an sfnt. `None` for anything else
/// (a plain font is used as it is) and for a web font that does not unpack.
#[must_use]
pub fn sfnt_of(bytes: &[u8]) -> Option<Vec<u8>> {
    use allsorts::{binary::read::ReadScope, font_data::FontData, tables::FontTableProvider};
    let magic = bytes.get(..4)?;
    if magic != b"wOFF" && magic != b"wOF2" {
        return None;
    }
    let data = ReadScope::new(bytes).read::<FontData<'_>>().ok()?;
    let provider = data.table_provider(0).ok()?;
    let mut tags = provider.table_tags()?;
    tags.sort_unstable();
    tags.dedup();
    let mut tables = Vec::with_capacity(tags.len());
    for tag in tags {
        let table = provider.table_data(tag).ok()??;
        tables.push((tag, table.into_owned()));
    }
    if tables.is_empty() {
        return None;
    }
    let cff = u32::from_be_bytes(*b"CFF ");
    let version = if tables.iter().any(|(tag, _)| *tag == cff) {
        u32::from_be_bytes(*b"OTTO")
    } else {
        0x0001_0000
    };
    Some(write_sfnt(version, &tables))
}

/// An sfnt of `tables` (sorted by tag): the offset table, the table records, each table on a
/// four-byte boundary with its checksum.
fn write_sfnt(version: u32, tables: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = tables.len() as u16;
    let entry_selector = 15 - count.max(1).leading_zeros() as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = count * 16 - search_range;
    let mut out = Vec::new();
    out.extend_from_slice(&version.to_be_bytes());
    for field in [count, search_range, entry_selector, range_shift] {
        out.extend_from_slice(&field.to_be_bytes());
    }
    let mut offset = 12 + 16 * tables.len();
    let mut body = Vec::new();
    for (tag, data) in tables {
        let checksum = data
            .chunks(4)
            .map(|word| {
                let mut padded = [0u8; 4];
                padded[..word.len()].copy_from_slice(word);
                u32::from_be_bytes(padded)
            })
            .fold(0u32, u32::wrapping_add);
        for field in [*tag, checksum, offset as u32, data.len() as u32] {
            out.extend_from_slice(&field.to_be_bytes());
        }
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
        offset = 12 + 16 * tables.len() + body.len();
    }
    out.extend_from_slice(&body);
    out
}

#[cfg(test)]
#[path = "fontreg_test.rs"]
mod fontreg_test;
