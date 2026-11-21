//! The renderer the headless runner does not have, as bookkeeping (E2E-R8).
//!
//! A shell registers with its renderer the fonts and the images its display
//! lists draw (`RendererResources::currently_registered_fonts` / `_images`,
//! `font_hash_map`, `image_key_map`) and drops them when no list draws them any
//! more. The in-process runner paints on the CPU and registers nothing, so those
//! counters read 0 before and after anything a scenario can do - `assert_resource_counts`
//! on `fonts` or `images` was `0 == 0`, and a `gt` control could never pass (batch 1
//! of `gen-e2e`: scenarios 13-18, "VACUOUS").
//!
//! [`VirtualRenderer`] is what a renderer WOULD hold for the lists that are on
//! screen: one font per resolvable font hash a [`DisplayListItem::Text`] draws,
//! one image per decoded image the lists draw (pictures, a `background-image`
//! that resolved, image masks, the mask of a stroke). The fonts are the set the
//! shell's `collect_font_resource_updates` registers; the images are read
//! through [`LayoutWindow::images_in_display_lists`], the scan the shell's
//! `collect_image_resource_updates` and the image GC read too. It is taken at the
//! instant of the call (a shell keeps a font a few frames after its last list:
//! this does not), so a counter that returns to its baseline in a shell returns
//! here too, and a list that holds ever more images shows as growth.
//!
//! Two things a shell's tables hold are NOT counted, on purpose: the kept
//! pictures of text shadows (`cpurender::text_halos` - the compositor's own
//! pictures, not a resource of the page) and the legacy `font_id_map` /
//! `font_families_map` (no shell writes them any more: they stay 0 everywhere).
//! The real `RendererResources` is not written either: the CPU path reads it
//! for the text halos, and populating it would be a change of the rendering.

use std::collections::BTreeSet;

use azul_core::resources::ImageRefHash;

use crate::{solver3::display_list::DisplayListItem, window::LayoutWindow};

/// The fonts and images a renderer would hold for the display lists of a window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VirtualRenderer {
    /// Hashes of the fonts the lists draw text in and the font manager resolves.
    pub fonts: BTreeSet<u64>,
    /// Hashes of the decoded images the lists draw. A callback image and a null
    /// image upload nothing, so they are not here.
    pub images: BTreeSet<ImageRefHash>,
}

impl VirtualRenderer {
    /// What the display lists of every DOM of `layout_window` draw.
    #[must_use]
    pub fn of(layout_window: &LayoutWindow) -> Self {
        let mut registry = Self::default();
        for result in layout_window.layout_results.values() {
            for item in &result.display_list.items {
                let DisplayListItem::Text { font_hash, .. } = item else {
                    continue;
                };
                let hash = font_hash.font_hash;
                // A shell registers a font it can resolve; a hash its own font
                // manager cannot resolve is text that cannot be drawn (and 0 is
                // `FontHash::invalid()`, which resolves to nothing).
                if !registry.fonts.contains(&hash)
                    && layout_window
                        .font_manager
                        .resolve_font_by_hash(hash)
                        .is_some()
                {
                    registry.fonts.insert(hash);
                }
            }
        }
        // What `build_add_image_resource_updates` uploads: pixels and textures. A
        // null image has nothing to upload and a callback image is produced after
        // layout.
        for image in layout_window.images_in_display_lists() {
            if image.is_raw_image() || image.is_gl_texture() {
                registry.images.insert(image.get_hash());
            }
        }
        registry
    }
}
