//! Fonts an app registers by family name while it runs.
//!
//! The faces an app ships go in before its first window
//! (`AppConfig::add_font`, `cache::set_app_fonts`). A face that arrives
//! later - a mail's remote web font, fetched after the mail is shown - is
//! registered here, for the whole process: every window draws the text that
//! names its family in it at its next layout, a window opened later and a
//! replaced font cache included.
//!
//! A registered face gets ONE `FontId` for good, and each font cache it is
//! put in holds it under that id, so a chain resolved by one window loads in
//! any other. Registering bumps a generation the layout reads: a window
//! resolves its font chains again, lays its documents out cold and shapes
//! its text afresh, since what it shaped before fell back to another face.
//! A registered face works like an `@font-face` rule: it wins over an
//! installed face of the same family.
//!
//! The bytes stay for the life of the process; there is no unregistering.

use std::{
    borrow::Cow,
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
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
    /// The TrueType / OpenType bytes, shared with the face a font manager
    /// loads from them ([`shared_bytes`]).
    bytes: Arc<[u8]>,
    /// The code points of its cmap, read once.
    ranges: Vec<UnicodeRange>,
    /// Its weight, slant and stretch, read once: `parse_face_style` parses
    /// the whole face (every cmap mapping, every name record, a copy of the
    /// bytes per face). [`ensure_in`] parsed every registered face on every
    /// call - once per registration of each later face, and twice per chain
    /// resolution of every window once anything was registered.
    style: FaceStyle,
}

/// Every face registered at runtime, in registration order.
static FACES: RwLock<Vec<RuntimeFace>> = RwLock::new(Vec::new());

/// How many registrations changed [`FACES`]: the layout's signal that font
/// chains and shaped text may be stale.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Registers the face `bytes` under `family` for the process. Returns
/// `false` for bytes that are no font (nothing registered). Registering the
/// same bytes under the same family again changes nothing.
///
/// The registry keeps its own copy of `bytes` (one, when the face is new).
/// A caller that holds the face as an `Arc<[u8]>` hands it to
/// [`register_font_shared`] instead, which keeps that allocation.
#[must_use]
pub fn register_font(family: &str, bytes: &[u8]) -> bool {
    register_with(family, bytes, || Arc::from(bytes))
}

/// [`register_font`] for a face the caller holds as an `Arc<[u8]>`: the
/// registry keeps `bytes` itself - the font caches and every face loaded
/// from it share that allocation - and copies nothing (a web font, WOFF or
/// WOFF2, is unpacked into a plain font of its own, as by `register_font`).
#[must_use]
pub fn register_font_shared(family: &str, bytes: Arc<[u8]>) -> bool {
    register_with(family, &bytes, || Arc::clone(&bytes))
}

/// Registers the face `bytes`; `kept` makes the registry's allocation of
/// them, called only when the face is new and is a plain font.
fn register_with(family: &str, bytes: &[u8], kept: impl FnOnce() -> Arc<[u8]>) -> bool {
    // A web font (WOFF, WOFF2) is unpacked once into the plain font every reader takes.
    let unpacked: Option<Arc<[u8]>> = sfnt_of(bytes).map(Arc::from);
    let bytes = unpacked.as_deref().unwrap_or(bytes);
    let ranges = cmap_coverage(bytes);
    if ranges.is_empty() {
        return false;
    }
    let norm = rust_fontconfig::utils::normalize_family_name(family);
    let known = |faces: &[RuntimeFace]| faces.iter().any(|f| f.norm == norm && *f.bytes == *bytes);
    if FACES.read().is_ok_and(|faces| known(&faces)) {
        return true;
    }
    // Parsed outside the lock; a face registered meanwhile is found below.
    let style = parse_face_style(bytes, family);
    let Ok(mut faces) = FACES.write() else {
        return false;
    };
    if known(&faces) {
        return true;
    }
    #[cfg(test)]
    audit_a8_tests::BYTES_KEPT.with(|n| n.set(n.get() + 1));
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
    GENERATION.fetch_add(1, Ordering::AcqRel);
    true
}

/// How many registrations changed the registry so far; 0 before the first.
#[must_use]
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Acquire)
}

/// The bytes of the runtime face `id`, shared, as a font manager's loader
/// takes them; `None` for any other id. A font cache holds its own copy of a
/// runtime face (`rust_fontconfig::FcFont` owns a `Vec`), and handing that out
/// copied it twice more (`get_font_by_id` clones the font, `get_font_bytes`
/// copies the clone into an `Arc`): the loaded face shares these instead.
pub(crate) fn shared_bytes(id: &FontId) -> Option<Arc<rust_fontconfig::FontBytes>> {
    let faces = FACES.read().ok()?;
    let face = faces.iter().find(|face| face.id == *id)?;
    Some(Arc::new(rust_fontconfig::FontBytes::Owned(Arc::clone(&face.bytes))))
}

/// Puts every registered face that `fc_cache` does not hold yet into it,
/// under its id, and returns each face's index entry with its normalized
/// family (the `FontManager::memory_families` form).
pub(crate) fn ensure_in(fc_cache: &FcFontCache) -> Vec<(String, MemoryFace)> {
    let Ok(faces) = FACES.read() else {
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
                        bytes: face.bytes.to_vec(),
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

/// `memory_families` (a font manager's index) with the registered faces
/// added, each put into `fc_cache` first: what a window's chain resolution
/// matches family names against. Borrowed as it is while nothing is
/// registered.
pub(crate) fn with_runtime_faces<'a>(
    memory_families: &'a HashMap<String, Vec<MemoryFace>>,
    fc_cache: &FcFontCache,
) -> Cow<'a, HashMap<String, Vec<MemoryFace>>> {
    if generation() == 0 {
        return Cow::Borrowed(memory_families);
    }
    let mut merged = memory_families.clone();
    for (norm, face) in ensure_in(fc_cache) {
        let faces = merged.entry(norm).or_default();
        if !faces.iter().any(|f| f.font_match.id == face.font_match.id) {
            faces.push(face);
        }
    }
    Cow::Owned(merged)
}

impl crate::callbacks::CallbackInfo {
    /// Registers the font `bytes` under the family name `family` for the
    /// whole app, while it runs: text whose `font-family` names it is drawn
    /// in it at each window's next layout (return `Update::RefreshDom` to
    /// lay out at once). Weight and style come from the face itself, so the
    /// faces of one family are registered one by one. Returns `false` for
    /// bytes that are no font.
    pub fn register_font(&mut self, family: azul_css::AzString, bytes: azul_css::U8Vec) -> bool {
        let registered = register_font(family.as_str(), bytes.as_ref());
        if registered {
            // This window's cache takes the face now; every other cache
            // takes it when its window resolves its fonts again.
            drop(ensure_in(&self.get_layout_window().font_manager.fc_cache));
        }
        registered
    }

    /// [`Self::register_font`] for bytes the app shares (`SharedBytes`): the
    /// registry keeps that allocation - every window's font cache and every face
    /// loaded from it share it with the app's own handle - and copies nothing.
    /// The bytes stay alive for the process, as every registered face does,
    /// whatever the app does with its handle afterwards.
    pub fn register_font_shared(
        &mut self,
        family: azul_css::AzString,
        bytes: azul_css::shared_bytes::SharedBytes,
    ) -> bool {
        let registered = register_font_shared(family.as_str(), bytes.to_arc());
        if registered {
            drop(ensure_in(&self.get_layout_window().font_manager.fc_cache));
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
mod tests {
    use super::*;

    /// The tables of a font, by tag.
    fn tables_of(font: &[u8]) -> Vec<(u32, Vec<u8>)> {
        use allsorts::{binary::read::ReadScope, font_data::FontData, tables::FontTableProvider};
        let data = ReadScope::new(font).read::<FontData<'_>>().unwrap();
        let provider = data.table_provider(0).unwrap();
        let mut tags = provider.table_tags().unwrap();
        tags.sort_unstable();
        tags.into_iter()
            .map(|tag| (tag, provider.table_data(tag).unwrap().unwrap().into_owned()))
            .collect()
    }

    /// `font` as a WOFF 1.0 with its tables stored as they are (allowed: a table whose
    /// compressed length is its length is not compressed).
    fn woff_of(font: &[u8]) -> Vec<u8> {
        let tables = tables_of(font);
        let directory = 44 + 20 * tables.len();
        let (mut body, mut entries) = (Vec::new(), Vec::new());
        for (tag, data) in &tables {
            entries.push((*tag, (directory + body.len()) as u32, data.len() as u32));
            body.extend_from_slice(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"wOFF");
        out.extend_from_slice(&font[..4]);
        out.extend_from_slice(&((directory + body.len()) as u32).to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        out.extend_from_slice(&(font.len() as u32).to_be_bytes());
        out.extend_from_slice(&1u16.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes());
        for _ in 0..5 {
            out.extend_from_slice(&0u32.to_be_bytes());
        }
        for (tag, offset, len) in entries {
            for field in [tag, offset, len, len, 0] {
                out.extend_from_slice(&field.to_be_bytes());
            }
        }
        out.extend_from_slice(&body);
        out
    }

    /// A mail's web font comes as a WOFF or a WOFF2: it is unpacked into the plain font, table
    /// for table, and registered as one; a plain font is taken as it is.
    #[test]
    fn a_web_font_is_unpacked_into_its_plain_font_and_registered() {
        let font = crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes.to_vec();
        assert!(sfnt_of(&font).is_none(), "a plain font is used as it is");
        let web = woff_of(&font);
        let unpacked = sfnt_of(&web).expect("the WOFF unpacks");
        assert_eq!(tables_of(&unpacked), tables_of(&font));
        assert!(register_font("Azul Fontreg Web Face", &web));
        assert!(sfnt_of(b"wOF2 not a font").is_none());
    }

    #[test]
    fn bytes_that_are_no_font_change_nothing() {
        let before = generation();
        assert!(!register_font("Azul No Font Here", &[0, 1, 2, 3]));
        assert_eq!(generation(), before);
    }

    #[test]
    fn a_face_registered_once_is_put_in_a_cache_under_one_id() {
        let bytes = &crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes;
        assert!(register_font("Azul Fontreg Unit Face", bytes));
        let again = generation();
        assert!(register_font("Azul Fontreg Unit Face", bytes));
        assert_eq!(generation(), again, "the same face again changes nothing");
        let cache = FcFontCache::default();
        let first = ensure_in(&cache);
        let second = ensure_in(&cache);
        let ids = |v: &[(String, MemoryFace)]| -> Vec<FontId> {
            v.iter()
                .filter(|(n, _)| {
                    *n == rust_fontconfig::utils::normalize_family_name("Azul Fontreg Unit Face")
                })
                .map(|(_, f)| f.font_match.id)
                .collect()
        };
        assert_eq!(ids(&first).len(), 1);
        assert_eq!(ids(&first), ids(&second));
        assert!(cache.is_memory_font(&ids(&first)[0]));
    }
}

/// Guards of the 2026-11-21 audit, area A8 (`audit/A8.md`).
#[cfg(test)]
mod audit_a8_tests {
    use super::*;

    std::thread_local! {
        /// Faces whose bytes the registry kept (new faces) on this thread.
        pub(super) static BYTES_KEPT: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
    }

    /// A8-08: a runtime face's style is parsed once, when it is registered,
    /// however often it is put into font caches. `CallbackInfo::register_font`
    /// calls `ensure_in` after each registration, and every window's chain
    /// resolution calls it twice: it parsed every registered face each time
    /// (n registrations: n (n + 1) / 2 parses).
    /// Mutation: `let style = face.style;` -> `parse_face_style(&face.bytes,
    /// &face.family)` in `ensure_in` fails it.
    #[test]
    fn a_registered_faces_style_is_parsed_once() {
        let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
        let parses = || crate::text3::cache::FACE_STYLE_PARSES.with(core::cell::Cell::get);
        let before = parses();
        for i in 0..8 {
            let bytes = &fonts[i % fonts.len()].bytes;
            assert!(register_font(&format!("Azul A8 Style Face {i}"), bytes));
            drop(ensure_in(&FcFontCache::default()));
        }
        assert_eq!(parses() - before, 8, "one parse per face registered, none per `ensure_in`");
        for _ in 0..4 {
            drop(ensure_in(&FcFontCache::default()));
        }
        assert_eq!(parses() - before, 8);
    }

    /// A8-09: a runtime face a font manager loads shares the registry's bytes
    /// (one allocation), instead of a copy of the font cache's copy.
    /// Mutation: drop the `fontreg::shared_bytes` lookup from
    /// `solver3::getters::load_fonts_from_disk` and the loaded bytes are another
    /// allocation.
    #[test]
    fn a_loaded_runtime_face_shares_the_registered_bytes() {
        let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
        let family = "Azul A8 Shared Bytes Face";
        assert!(register_font(family, &fonts[0].bytes));
        let cache = FcFontCache::default();
        let norm = rust_fontconfig::utils::normalize_family_name(family);
        let id = ensure_in(&cache)
            .into_iter()
            .find(|(n, _)| *n == norm)
            .map(|(_, face)| face.font_match.id)
            .expect("the face is put into the cache");
        let ids = std::collections::HashSet::from([id]);
        let result = crate::solver3::getters::load_fonts_from_disk(&ids, &cache, |bytes, index| {
            assert_eq!(index, 0);
            Ok(bytes)
        });
        let loaded = result.loaded.get(&id).expect("the face loads");
        let shared = shared_bytes(&id).expect("a runtime face");
        let (rust_fontconfig::FontBytes::Owned(a), rust_fontconfig::FontBytes::Owned(b)) =
            (&**loaded, &*shared)
        else {
            panic!("a runtime face's bytes are owned");
        };
        assert!(Arc::ptr_eq(a, b), "the loaded face shares the registry's bytes");
        assert_eq!(&a[..], &fonts[0].bytes[..]);
    }

    /// A8-26: a face handed over as an `Arc<[u8]>` is kept as that allocation:
    /// the registry, and the bytes every loader is handed, are the caller's
    /// (no copy); registering it again keeps nothing more.
    /// Mutation: `|| Arc::clone(&bytes)` -> `|| Arc::from(&bytes[..])` in
    /// `register_font_shared` fails the `ptr_eq`.
    #[test]
    fn a_face_handed_over_shared_is_kept_without_a_copy() {
        let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
        let family = "Azul A8 Handed Over Face";
        let bytes: Arc<[u8]> = Arc::from(&fonts[1 % fonts.len()].bytes[..]);
        let kept = || BYTES_KEPT.with(core::cell::Cell::get);
        let before = kept();
        assert!(register_font_shared(family, Arc::clone(&bytes)));
        assert!(register_font_shared(family, Arc::clone(&bytes)), "again: known");
        assert_eq!(kept() - before, 1, "one face kept");
        let norm = rust_fontconfig::utils::normalize_family_name(family);
        let id = ensure_in(&FcFontCache::default())
            .into_iter()
            .find(|(n, _)| *n == norm)
            .map(|(_, face)| face.font_match.id)
            .expect("the face is registered");
        let shared = shared_bytes(&id).expect("a runtime face");
        let rust_fontconfig::FontBytes::Owned(held) = &*shared else {
            panic!("a runtime face's bytes are owned");
        };
        assert!(Arc::ptr_eq(held, &bytes), "the registry holds the caller's allocation");
    }

    /// A8-26 (the C API's type): a face registered through `SharedBytes`
    /// shares the caller's bytes - the registry and every loaded face hold the
    /// very allocation (pointer equality) - and the bytes outlive the caller's
    /// handle.
    /// Mutation: `bytes.to_arc()` -> `Arc::from(bytes.as_slice())` in the path
    /// `CallbackInfo::register_font_shared` takes fails the `ptr_eq`.
    #[test]
    fn a_face_registered_through_shared_bytes_shares_and_outlives_the_callers_handle() {
        use azul_css::shared_bytes::SharedBytes;
        let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
        let family = "Azul A8 SharedBytes Face";
        let handle = SharedBytes::from_arc(Arc::from(&fonts[2 % fonts.len()].bytes[..]));
        let data = handle.as_slice().as_ptr();
        // what `CallbackInfo::register_font_shared` does with the handle
        assert!(register_font_shared(family, handle.to_arc()));
        drop(handle);
        let norm = rust_fontconfig::utils::normalize_family_name(family);
        let id = ensure_in(&FcFontCache::default())
            .into_iter()
            .find(|(n, _)| *n == norm)
            .map(|(_, face)| face.font_match.id)
            .expect("the face is registered");
        let shared = shared_bytes(&id).expect("a runtime face");
        let rust_fontconfig::FontBytes::Owned(held) = &*shared else {
            panic!("a runtime face's bytes are owned");
        };
        assert!(core::ptr::eq(held.as_ptr(), data), "the caller's allocation, not a copy");
        assert_eq!(&held[..], &fonts[2 % fonts.len()].bytes[..], "alive after the handle is gone");
    }
}
