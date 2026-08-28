//! ABI-stable snapshot handles for the app's live resources.
//!
//! [`FontCacheSnapshot`] and [`ImageCacheSnapshot`] are `#[repr(C)]` boxed handles a callback
//! obtains via `CallbackInfo::get_font_cache_clone()` /
//! `CallbackInfo::get_image_cache_clone()` and hands to consumers that lay
//! content out OUTSIDE the window pipeline — `Pdf::from_styled_dom_with_resources`
//! being the canonical one ("print exactly what is on screen").
//!
//! Both are SNAPSHOT HANDLES, not live views: cloning is cheap because the
//! heavy state is refcounted (parsed font faces sit behind
//! `Arc<Mutex<HashMap<..>>>` shared by [`FontManager::clone_shared`]; decoded
//! image pixels sit behind refcounted `ImageRef`s), so nothing is re-parsed,
//! re-discovered or re-decoded — the point is speed, not memory. The handle
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
/// every `ImageRef` is a refcounted handle — decoded pixel data is shared,
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

#[cfg(feature = "text_layout")]
impl PaginationSnapshot {
    /// Wrap a pagination analysis into an ABI handle.
    #[must_use]
    pub fn from_info(info: crate::solver3::page_breaks::PaginationInfo) -> Self {
        Self {
            ptr: Box::into_raw(Box::new(info)).cast(),
            run_destructor: true,
        }
    }

    /// Borrow the wrapped analysis, if any.
    #[must_use]
    pub const fn as_info(&self) -> Option<&crate::solver3::page_breaks::PaginationInfo> {
        unsafe {
            self.ptr
                .cast::<crate::solver3::page_breaks::PaginationInfo>()
                .as_ref()
        }
    }

    /// An empty handle (0 pages — the failure value).
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
            .is_some_and(|b| {
                matches!(
                    b.kind,
                    crate::solver3::page_breaks::BreakKind::Avoided { .. }
                )
            })
    }

    /// Which page a document-space Y lands on ("what page is this node on?"
    /// — the editor query that needs NO page to be materialized).
    #[must_use]
    pub fn page_of_y(&self, y: f32) -> usize {
        self.as_info()
            .map_or(0, |i| crate::solver3::page_breaks::page_of_y(&i.breaks, y))
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
                drop(Box::from_raw(
                    self.ptr
                        .cast::<crate::solver3::page_breaks::PaginationInfo>(),
                ));
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
}
