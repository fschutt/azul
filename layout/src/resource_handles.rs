//! ABI-stable snapshot handles for the app's live resources.
//!
//! [`FontCacheSnapshot`] and [`ImageCacheSnapshot`] are `#[repr(C)]` boxed handles a callback
//! obtains via `CallbackInfo::get_font_cache_clone()` /
//! `CallbackInfo::get_image_cache_clone()` and hands to consumers that lay
//! content out OUTSIDE the window pipeline - `Pdf::from_styled_dom_with_resources`
//! being the canonical one ("print exactly what is on screen").
//!
//! Both are SNAPSHOT HANDLES, not live views: cloning is cheap because the
//! heavy state is refcounted (parsed font faces sit behind
//! `Arc<Mutex<HashMap<..>>>` shared by [`FontManager::clone_shared`]; decoded
//! image pixels sit behind refcounted `ImageRef`s), so nothing is re-parsed,
//! re-discovered or re-decoded - the point is speed, not memory. The handle
//! stays valid after the callback returns, so a print job can run off-thread.

use core::ffi::c_void;

#[cfg(feature = "text_layout")]
use azul_css::props::basic::FontRef;

#[cfg(feature = "text_layout")]
type InnerFontManager = crate::text3::cache::FontManager<FontRef>;

/// Boxed snapshot of the window's font resolution state: shared parsed-font
/// pool, resolved fallback chains, embedded/in-memory fonts, registry link.
///
/// Obtain via `CallbackInfo::get_font_cache_clone()`. `Clone` derives another
/// shared handle (cheap); dropping releases only this handle's box.
#[repr(C)]
#[derive(Debug)]
pub struct FontCacheSnapshot {
    /// Boxed [`FontManager`](crate::text3::cache::FontManager) (opaque over
    /// the ABI; null when the `text_layout` feature is compiled out).
    pub ptr: *mut c_void,
    /// Standard azul destructor latch: `false` on moved-out copies so only
    /// one side of an FFI move runs the destructor.
    pub run_destructor: bool,
}

impl FontCacheSnapshot {
    /// Snapshot for use inside a LAYOUT callback: builds family resolution +
    /// fallback chains over the system font cache the callback exposes, and
    /// the app's registered fonts the window draws with - the faces the app
    /// ships (`AppConfig::add_font`) and the ones it registered while it
    /// runs - which the callback's info carries: the snapshot measures with
    /// the faces the screen does. No per-window embedded fonts exist yet at
    /// layout time, so nothing is lost versus
    /// `CallbackInfo::get_font_cache_clone` - this is the off-thread-pagination
    /// input an app can hand to `Pdf::compute_pagination` without ever seeing
    /// the internal manager. A host that has no registered fonts to hand over
    /// (a web page) gets a snapshot with registered fonts of its own: only
    /// the faces azul bundles.
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn from_layout_info(info: &azul_core::callbacks::LayoutCallbackInfo) -> Self {
        use std::sync::Arc;

        use crate::text3::fontreg::RegisteredFonts;

        // What the shell handed the layout pass is an `Arc<RegisteredFonts>`, opaque to azul-core.
        let registered = info
            .registered_fonts()
            .and_then(|handle| handle.downcast_ref::<Arc<RegisteredFonts>>());
        let manager = match registered {
            Some(fonts) => crate::font_traits::FontManager::with_registered_fonts(
                info.get_font_cache(),
                Arc::clone(fonts),
            ),
            None => crate::font_traits::FontManager::new(info.get_font_cache()),
        };
        match manager {
            Ok(fm) => Self::from_font_manager(fm),
            Err(_) => Self::empty(),
        }
    }

    /// Wrap a font manager into an ABI handle.
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub fn from_font_manager(fm: InnerFontManager) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(fm)).cast(),
            run_destructor: true,
        }
    }

    /// Borrow the wrapped font manager, if any.
    #[cfg(feature = "text_layout")]
    #[must_use]
    pub const fn as_font_manager(&self) -> Option<&InnerFontManager> {
        unsafe { self.ptr.cast::<InnerFontManager>().as_ref() }
    }

    /// An empty handle (also what the non-`text_layout` build returns).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Clone for FontCacheSnapshot {
    fn clone(&self) -> Self {
        #[cfg(feature = "text_layout")]
        {
            if let Some(fm) = self.as_font_manager() {
                return Self::from_font_manager(fm.clone_shared());
            }
        }
        Self::empty()
    }
}

impl Drop for FontCacheSnapshot {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            #[cfg(feature = "text_layout")]
            unsafe {
                drop(Box::from_raw(self.ptr.cast::<InnerFontManager>()));
            }
            self.ptr = core::ptr::null_mut();
            self.run_destructor = false;
        }
    }
}

// SAFETY: the wrapped FontManager's shared state is Arc/Mutex-guarded; the
// handle exists precisely so print jobs can run off the UI thread.
#[cfg(feature = "text_layout")]
unsafe impl Send for FontCacheSnapshot {}

/// Boxed snapshot of the app's registered images (`css id -> ImageRef`).
///
/// Obtain via `CallbackInfo::get_image_cache_clone()`. The map is copied but
/// every `ImageRef` is a refcounted handle - decoded pixel data is shared,
/// never duplicated or re-decoded.
#[repr(C)]
#[derive(Debug)]
pub struct ImageCacheSnapshot {
    /// Boxed [`azul_core::resources::ImageCache`] (opaque over the ABI).
    pub ptr: *mut c_void,
    /// Standard azul destructor latch (see [`FontCacheSnapshot::run_destructor`]).
    pub run_destructor: bool,
}

impl ImageCacheSnapshot {
    /// Wrap an image cache into an ABI handle.
    #[must_use]
    pub fn from_image_cache(cache: azul_core::resources::ImageCache) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(cache)).cast(),
            run_destructor: true,
        }
    }

    /// Borrow the wrapped image cache, if any.
    #[must_use]
    pub const fn as_image_cache(&self) -> Option<&azul_core::resources::ImageCache> {
        unsafe { self.ptr.cast::<azul_core::resources::ImageCache>().as_ref() }
    }

    /// An empty handle.
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Clone for ImageCacheSnapshot {
    fn clone(&self) -> Self {
        // `azul_core::resources::ImageCache` has no `Clone` impl (house rule
        // after the derive(Clone)+Drop double-free audit); clone the map of
        // refcounted `ImageRef` handles explicitly — pixels stay shared.
        self.as_image_cache().map_or_else(Self::empty, |c| {
            Self::from_image_cache(azul_core::resources::ImageCache {
                image_id_map: c.image_id_map.clone(),
            })
        })
    }
}

impl Drop for ImageCacheSnapshot {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(
                    self.ptr.cast::<azul_core::resources::ImageCache>(),
                ));
            }
            self.ptr = core::ptr::null_mut();
            self.run_destructor = false;
        }
    }
}

// SAFETY: ImageRef's refcount is atomic; the snapshot exists to outlive the
// callback (off-thread print jobs).
unsafe impl Send for ImageCacheSnapshot {}

/// Boxed pagination analysis (`page_breaks::PaginationInfo`) over the ABI.
///
/// The document-editor precalculation result: page count, page of any Y, and
/// every break position, WITHOUT any per-page display list having been
/// materialized. Obtain via `Pdf::compute_pagination`.
#[cfg(feature = "text_layout")]
#[repr(C)]
#[derive(Debug)]
pub struct PaginationSnapshot {
    /// Boxed [`crate::solver3::page_breaks::PaginationInfo`] (opaque over the ABI).
    pub ptr: *mut c_void,
    /// Standard azul destructor latch (see [`FontCacheSnapshot::run_destructor`]).
    pub run_destructor: bool,
}

/// The boxed payload behind [`PaginationSnapshot::ptr`]: the break analysis
/// plus (when the computing entry had the layout cache in scope) every
/// break's STRUCTURAL DOM position. Internal - the handle is opaque over the
/// ABI, which is exactly what lets this grow without an ABI change.
#[cfg(feature = "text_layout")]
#[derive(Debug)]
pub struct PaginationAnalysis {
    pub info: crate::solver3::page_breaks::PaginationInfo,
    /// One entry per break of `info.breaks` when available; empty when the
    /// computing path could not derive structural positions.
    pub structural: Vec<crate::solver3::paged_layout::StructuralBreak>,
}

#[cfg(feature = "text_layout")]
impl PaginationSnapshot {
    /// Wrap a pagination analysis into an ABI handle (no structural breaks -
    /// prefer [`Self::from_analysis`] where the layout cache was in scope).
    #[must_use]
    pub fn from_info(info: crate::solver3::page_breaks::PaginationInfo) -> Self {
        Self::from_analysis(PaginationAnalysis {
            info,
            structural: Vec::new(),
        })
    }

    /// Wrap the full analysis (break positions + structural DOM paths).
    #[must_use]
    pub fn from_analysis(analysis: PaginationAnalysis) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(analysis)).cast(),
            run_destructor: true,
        }
    }

    /// Borrow the full boxed payload, if any.
    #[must_use]
    const fn as_analysis(&self) -> Option<&PaginationAnalysis> {
        unsafe { self.ptr.cast::<PaginationAnalysis>().as_ref() }
    }

    /// Borrow the wrapped analysis, if any.
    #[must_use]
    pub const fn as_info(&self) -> Option<&crate::solver3::page_breaks::PaginationInfo> {
        match self.as_analysis() {
            Some(a) => Some(&a.info),
            None => None,
        }
    }

    /// Root-to-node child-index DOM path of break `index` - where inserting a
    /// break node BEFORE the addressed node reproduces this page boundary
    /// structurally (consumable by `DomSplit::at_path`). EMPTY when out of
    /// range, when the break has no block at/after it, or when the snapshot
    /// was computed by a path without structural mapping.
    #[must_use]
    pub fn break_path(&self, index: usize) -> azul_css::corety::U32Vec {
        self.as_analysis()
            .and_then(|a| a.structural.get(index))
            .and_then(|b| b.path.clone())
            .map_or_else(azul_css::corety::U32Vec::new, Into::into)
    }

    /// Root-to-node child-index DOM path of the block break `index` SPLITS:
    /// the block the break lands in, when the break falls between two of
    /// its lines (a paragraph taller than the space left on the page). Its
    /// next page starts inside that block, at
    /// [`Self::break_line_start_run`] / [`Self::break_line_start_byte`].
    /// EMPTY when the break falls between blocks (then [`Self::break_path`]
    /// names the block the next page starts with), out of range, or without
    /// structural mapping.
    #[must_use]
    pub fn break_line_path(&self, index: usize) -> azul_css::corety::U32Vec {
        self.as_analysis()
            .and_then(|a| a.structural.get(index))
            .and_then(|b| b.line_path.clone())
            .map_or_else(azul_css::corety::U32Vec::new, Into::into)
    }

    /// The text run (of the block [`Self::break_line_path`] names) the first
    /// line of break `index`'s next page starts in. `None` when the break
    /// splits no block.
    #[must_use]
    pub fn break_line_start_run(&self, index: usize) -> azul_css::corety::OptionU32 {
        self.as_analysis()
            .and_then(|a| a.structural.get(index))
            .and_then(|b| b.line_start)
            .map(|line_start| line_start.run_index)
            .into()
    }

    /// The byte (in [`Self::break_line_start_run`]) the first line of break
    /// `index`'s next page starts at. `None` when the break splits no block.
    #[must_use]
    pub fn break_line_start_byte(&self, index: usize) -> azul_css::corety::OptionU32 {
        self.as_analysis()
            .and_then(|a| a.structural.get(index))
            .and_then(|b| b.line_start)
            .map(|line_start| line_start.item_index)
            .into()
    }

    /// Root-to-node child-index DOM path of the TEXT node the first line of
    /// break `index`'s next page begins in, when the break splits a block
    /// ([`Self::break_line_path`] names the block, this the text node inside
    /// it). With [`Self::break_line_text_byte`] it says where to cut the
    /// DOM's own text: the page view of an editor renders the block from
    /// there. EMPTY when the break splits no block or its line starts in
    /// generated content.
    #[must_use]
    pub fn break_line_text_path(&self, index: usize) -> azul_css::corety::U32Vec {
        self.as_analysis()
            .and_then(|a| a.structural.get(index))
            .and_then(|b| b.line_text.as_ref())
            .map_or_else(azul_css::corety::U32Vec::new, |t| t.path.clone().into())
    }

    /// The byte of the text node [`Self::break_line_text_path`] names that
    /// the first line of break `index`'s next page begins at: its first
    /// letter, counted in the DOM's text (whitespace as written, not as the
    /// line layout collapsed it). `None` when the break splits no block.
    #[must_use]
    pub fn break_line_text_byte(&self, index: usize) -> azul_css::corety::OptionU32 {
        self.as_analysis()
            .and_then(|a| a.structural.get(index))
            .and_then(|b| b.line_text.as_ref())
            .map(|t| t.byte)
            .into()
    }

    /// An empty handle (0 pages - the failure value).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }

    /// Number of pages (0 for an empty/failed handle).
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.as_info().map_or(0, |i| i.page_count)
    }

    /// Total document-space content height.
    #[must_use]
    pub fn total_content_height(&self) -> f32 {
        self.as_info().map_or(0.0, |i| i.total_content_height)
    }

    /// Number of page BREAKS (= `page_count - 1` for non-degenerate docs).
    #[must_use]
    pub fn break_count(&self) -> usize {
        self.as_info().map_or(0, |i| i.breaks.len())
    }

    /// Document-space Y of break `index` (0.0 out of range).
    #[must_use]
    pub fn break_y(&self, index: usize) -> f32 {
        self.as_info()
            .and_then(|i| i.breaks.get(index))
            .map_or(0.0, |b| b.y)
    }

    /// Whether break `index` was FORCED by CSS (`break-before/after`).
    #[must_use]
    pub fn break_is_forced(&self, index: usize) -> bool {
        self.as_info()
            .and_then(|i| i.breaks.get(index))
            .is_some_and(|b| matches!(b.kind, crate::solver3::page_breaks::BreakKind::Forced))
    }

    /// Whether break `index` was MOVED by an avoid-rule (break-inside /
    /// widows-orphans / atomic lines or rows).
    #[must_use]
    pub fn break_was_avoided(&self, index: usize) -> bool {
        self.as_info()
            .and_then(|i| i.breaks.get(index))
            .is_some_and(|b| matches!(b.kind, crate::solver3::page_breaks::BreakKind::Avoided(..)))
    }

    /// Which page a document-space Y lands on ("what page is this node on?"
    /// - the editor query that needs NO page to be materialized).
    #[must_use]
    pub fn page_of_y(&self, y: f32) -> usize {
        self.as_info().map_or(0, |i| {
            crate::solver3::page_breaks::page_of_y(i.breaks.as_ref(), y)
        })
    }
}

#[cfg(feature = "text_layout")]
impl Clone for PaginationSnapshot {
    fn clone(&self) -> Self {
        self.as_info()
            .map_or_else(Self::empty, |i| Self::from_info(i.clone()))
    }
}

#[cfg(feature = "text_layout")]
impl Drop for PaginationSnapshot {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr.cast::<PaginationAnalysis>()));
            }
            self.ptr = core::ptr::null_mut();
            self.run_destructor = false;
        }
    }
}

// SAFETY: plain data (Ys + kinds), no interior mutability.
#[cfg(feature = "text_layout")]
unsafe impl Send for PaginationSnapshot {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_cache_handle_clone_shares_refs_and_drops_clean() {
        let inner = azul_core::resources::ImageCache::default();
        let handle = ImageCacheSnapshot::from_image_cache(inner);
        let clone = handle.clone();
        assert!(handle.as_image_cache().is_some());
        assert!(clone.as_image_cache().is_some());
        drop(handle);
        assert!(clone.as_image_cache().is_some());
    }

    #[test]
    fn empty_handles_are_null_and_droppable() {
        let f = FontCacheSnapshot::empty();
        let i = ImageCacheSnapshot::empty();
        assert!(i.as_image_cache().is_none());
        drop(f);
        drop(i);
    }

    /// `f` with the info of a layout callback as the shells make it: over the window's `fonts` and,
    /// where the host has them to hand over, the app's `registered` fonts.
    #[cfg(feature = "text_layout")]
    fn in_a_layout_callback<R>(
        fonts: &rust_fontconfig::FcFontCache,
        registered: Option<&std::sync::Arc<crate::text3::fontreg::RegisteredFonts>>,
        f: impl FnOnce(&azul_core::callbacks::LayoutCallbackInfo) -> R,
    ) -> R {
        use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackInfoRefData, TextDirection};

        let images = azul_core::resources::ImageCache::default();
        let gl = azul_core::gl::OptionGlContextPtr::None;
        let locale = azul_css::AzString::from("en-US");
        let window_id = azul_css::AzString::from("");
        let ref_data = LayoutCallbackInfoRefData {
            locale: &locale,
            accessed_locale: core::cell::Cell::new(false),
            accessed_text_direction: core::cell::Cell::new(false),
            text_direction: TextDirection::LeftToRight,
            image_cache: &images,
            gl_context: &gl,
            system_fonts: fonts,
            registered_fonts: registered
                .map(|registry| registry as &(dyn core::any::Any + Send + Sync)),
            system_style: std::sync::Arc::new(azul_css::system::SystemStyle::default()),
            active_route: None,
            monitors: azul_core::window::MonitorVec::from_const_slice(&[]),
            safe_area: azul_css::system::SafeAreaInsets::default(),
            global_hotkeys: azul_core::global_hotkey::GlobalHotkeyInfoVec::from_const_slice(&[]),
            window_id: &window_id,
        };
        let info = LayoutCallbackInfo::new(
            &ref_data,
            azul_core::window::WindowSize::default(),
            azul_core::window::DarkLightMode::Light,
        );
        f(&info)
    }

    /// A snapshot made in a layout callback measures with the faces the screen draws with: the
    /// faces an app ships (`AppConfig::add_font`) are the registered fonts of its windows, the
    /// callback's info carries them, and the family of a shipped face resolves in the snapshot to
    /// the very face (under the id, in the tier, with the chains) it resolves to in the window.
    /// Before, the snapshot was built over a font manager with registered fonts of its own and
    /// the family was unknown to it.
    /// Mutation: `from_layout_info` building its manager with `FontManager::new` fails it.
    #[cfg(feature = "text_layout")]
    #[test]
    fn a_snapshot_made_in_a_layout_callback_resolves_the_family_of_an_apps_shipped_face() {
        use std::{collections::BTreeSet, sync::Arc};

        use azul_core::{dom::Dom, styled_dom::StyledDom};
        use rust_fontconfig::{FcFontCache, FontId};

        use crate::{
            solver3::getters::collect_and_resolve_font_chains_with_registration,
            text3::{cache::MemoryFontTier, fontreg::RegisteredFonts},
        };

        const FAMILY: &str = "Azul Snapshot Shipped Face";
        let norm = rust_fontconfig::utils::normalize_family_name(FAMILY);
        // The app: the face it ships is in the registry its windows' font managers are made on.
        let app = Arc::new(RegisteredFonts::default());
        let face = crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes.clone();
        app.set_app_fonts(&[azul_core::resources::NamedFont::new(FAMILY.into(), face.into())]);
        let window =
            InnerFontManager::with_registered_fonts(FcFontCache::default(), Arc::clone(&app))
                .expect("the window's font manager");
        let shipped = window
            .memory_families
            .get(&norm)
            .and_then(|faces| faces.first())
            .expect("the window has the shipped face")
            .font_match
            .id;

        // The faces a DOM naming the family resolves to, in a font manager.
        let styled = StyledDom::create_from_dom(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_css(&format!("font-family: \"{FAMILY}\";"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "Gardasee",
                    )),
            ),
        );
        let platform = azul_css::system::Platform::current();
        let faces_of = |manager: &InnerFontManager| -> BTreeSet<FontId> {
            collect_and_resolve_font_chains_with_registration(
                &styled,
                &manager.fc_cache,
                manager,
                &platform,
            )
            .chains
            .values()
            .flat_map(|chain| chain.fonts().map(|face| face.id))
            .collect()
        };
        let on_screen = faces_of(&window);
        assert!(
            on_screen.contains(&shipped),
            "premise: the window draws the family with the shipped face"
        );

        // In the layout callback of the window: its info carries the window's registered fonts.
        in_a_layout_callback(&window.fc_cache, Some(&window.registered_fonts), |info| {
            let snapshot = FontCacheSnapshot::from_layout_info(info);
            let manager = snapshot.as_font_manager().expect("a font manager");
            assert!(
                Arc::ptr_eq(&manager.registered_fonts, &app),
                "the snapshot draws with the registered fonts of the app"
            );
            let indexed = manager
                .memory_families
                .get(&norm)
                .and_then(|faces| faces.first())
                .expect("the snapshot indexes the family of the shipped face");
            assert_eq!(indexed.font_match.id, shipped, "under the id the window has it under");
            assert_eq!(indexed.tier, MemoryFontTier::Fallback, "in the tier the window has it in");
            assert_eq!(
                faces_of(manager),
                on_screen,
                "pagination measures with the faces the screen draws with"
            );
        });

        // A host with no registered fonts to hand over: the snapshot has the faces azul bundles.
        in_a_layout_callback(&window.fc_cache, None, |info| {
            let snapshot = FontCacheSnapshot::from_layout_info(info);
            let manager = snapshot.as_font_manager().expect("a font manager");
            assert!(!Arc::ptr_eq(&manager.registered_fonts, &app));
            assert!(
                !manager.memory_families.contains_key(&norm),
                "the family of the shipped face is unknown to it"
            );
        });
    }
}
