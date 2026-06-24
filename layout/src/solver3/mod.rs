//! solver3/mod.rs
//!
//! Next-generation CSS layout engine with proper formatting context separation

pub mod cache;
pub mod calc;
pub mod counters;
pub mod display_list;
pub mod fc;
pub mod geometry;
pub mod getters;
pub mod layout_tree;
pub mod paged_layout;
pub mod pagination;
pub mod positioning;
pub mod scrollbar;
pub mod sizing;
pub mod taffy_bridge;

/// Lazy `debug_info` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_info {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_info_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_warning` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_warning {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_warning_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_error` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_error {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_error_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_log` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_log {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_log_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_box_props` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_box_props {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_box_props_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_css_getter` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_css_getter {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_css_getter_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_bfc_layout` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_bfc_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_bfc_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_ifc_layout` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_ifc_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_ifc_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_table_layout` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_table_layout {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_table_layout_inner(format!($($arg)*));
        }
    };
}

/// Lazy `debug_display_type` macro - only evaluates format args when `debug_messages` is Some
#[macro_export]
macro_rules! debug_display_type {
    ($ctx:expr, $($arg:tt)*) => {
        if $ctx.debug_messages.is_some() {
            $ctx.debug_display_type_inner(format!($($arg)*));
        }
    };
}

use std::{collections::{BTreeMap, HashMap}, sync::Arc};

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{DocumentId, ScrollPosition},
    resources::RendererResources,
    selection::{TextCursor, TextSelection},
    styled_dom::StyledDom,
};

/// Sentinel value for "position not yet computed". No real position is ever `f32::MIN`.
pub(crate) const POSITION_UNSET: LogicalPosition = LogicalPosition { x: f32::MIN, y: f32::MIN };

/// Maximum number of scrollbar-induced reflow iterations before layout gives up.
/// Scrollbar appearance can change container size, which may trigger further scrollbar
/// changes. This limit prevents infinite loops in pathological layouts.
const MAX_SCROLLBAR_REFLOW_ITERATIONS: usize = 10;

/// Vec-based position storage indexed by layout-tree node index.
/// Replaces `BTreeMap<usize, LogicalPosition>` for O(1) access and cache-friendly iteration.
pub type PositionVec = Vec<LogicalPosition>;

/// Get position for node index, returning None if unset.
///
/// Note: only the `x` component is checked against the sentinel. This is sufficient
/// because `POSITION_UNSET` always sets both `x` and `y` to `f32::MIN`, and `pos_set`
/// always writes both components together.
#[inline]
#[must_use] pub fn pos_get(positions: &PositionVec, idx: usize) -> Option<LogicalPosition> {
    positions.get(idx).copied().filter(|p| p.x != f32::MIN)
}

/// Set position for node index. Grows the vec if needed.
#[inline]
pub fn pos_set(positions: &mut PositionVec, idx: usize, pos: LogicalPosition) {
    if idx >= positions.len() {
        positions.resize(idx + 1, POSITION_UNSET);
    }
    positions[idx] = pos;
}

/// Check if position has been set for node index.
#[inline]
#[must_use] pub fn pos_contains(positions: &PositionVec, idx: usize) -> bool {
    positions.get(idx).is_some_and(|p| p.x != f32::MIN)
}
use azul_css::{
    props::property::{CssProperty, CssPropertyCategory},
    LayoutDebugMessage, LayoutDebugMessageType,
};

use self::{
    display_list::generate_display_list,
    geometry::IntrinsicSizes,
    getters::get_writing_mode,
    layout_tree::{generate_layout_tree, LayoutTree},
    sizing::calculate_intrinsic_sizes,
};
#[cfg(feature = "text_layout")]
pub use crate::font_traits::TextLayoutCache;
use crate::{
    font_traits::ParsedFontTrait,
    solver3::{
        cache::LayoutCache,
        display_list::DisplayList,
        fc::LayoutConstraints,
        layout_tree::DirtyFlag,
    },
};

/// Central context for a single layout pass.
pub struct LayoutContext<'a, T: ParsedFontTrait> {
    pub styled_dom: &'a StyledDom,
    #[cfg(feature = "text_layout")]
    pub font_manager: &'a crate::font_traits::FontManager<T>,
    #[cfg(not(feature = "text_layout"))]
    pub font_manager: core::marker::PhantomData<&'a T>,
    /// Text selections for rendering highlights. Populated from `MultiCursorState`.
    pub text_selections: &'a BTreeMap<DomId, TextSelection>,
    pub debug_messages: &'a mut Option<Vec<LayoutDebugMessage>>,
    pub counters: &'a mut HashMap<(usize, String), i32>,
    pub viewport_size: LogicalSize,
    /// Fragmentation context for CSS Paged Media (PDF generation)
    /// When Some, layout respects page boundaries and generates one `DisplayList` per page
    pub fragmentation_context: Option<&'a mut crate::paged::FragmentationContext>,
    /// Whether the text cursor should be drawn (managed by `CursorManager` blink timer)
    /// When false, the cursor is in the "off" phase of blinking and should not be rendered.
    /// When true (default), the cursor is visible.
    pub cursor_is_visible: bool,
    /// All active cursor locations from `MultiCursorState` / `CursorManager`.
    /// Each entry is (`dom_id`, `node_id`, cursor). Multiple entries = multi-cursor mode.
    /// Empty = no active cursor. The last entry is the primary cursor.
    pub cursor_locations: Vec<(DomId, NodeId, TextCursor)>,
    /// IME preedit (composition) text to render inline at the cursor position.
    /// When Some, the text should be rendered with an underline decoration.
    pub preedit_text: Option<String>,
    /// Text content overrides from in-progress edits (`dirty_text_nodes`).
    /// When a text node has been edited but not yet committed to the DOM,
    /// the layout pipeline should read from here instead of `StyledDom`.
    /// Key: (`DomId`, `NodeId` of the text node), Value: the edited text string.
    pub dirty_text_overrides: BTreeMap<(DomId, NodeId), String>,
    /// Per-node multi-slot cache (Taffy-inspired 9+1 architecture).
    /// Moved out of `LayoutCache` via `std::mem::take` for the duration of layout,
    /// then moved back after the layout pass completes.
    pub cache_map: cache::LayoutCacheMap,
    /// Image cache for resolving `background-image: url(...)` references.
    pub image_cache: &'a azul_core::resources::ImageCache,
    /// System style containing colors, fonts, metrics, and theme information.
    /// Used for selection colors, caret styling, and other system-themed elements.
    pub system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    /// Callback to get the current system time. Used for profiling inside layout.
    /// On WASM targets (where `std::time::Instant::now()` panics), callers should
    /// supply a no-op or platform-specific implementation.
    pub get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    /// Memoised `get_scrollbar_style` results, keyed by DOM node id.
    /// `compute_scrollbar_info_core` is called many times per node
    /// per layout pass (BFC path + Taffy flex/grid path + display
    /// list), and each call previously did 9 cascade walks. Once
    /// populated, subsequent callers in the same `LayoutContext`
    /// (a single render) return a clone.
    ///
    /// Uses `RefCell` so shared `&self` borrows (e.g. in the Taffy
    /// bridge's `get_core_container_style`) can mutate the cache
    /// without lifting the ctx to `&mut`. Keyed by `NodeId` so
    /// entries span DOMs in iframe-style nested documents if that
    /// ever becomes a thing.
    pub scrollbar_style_cache:
        core::cell::RefCell<HashMap<NodeId, getters::ComputedScrollbarStyle>>,
}

impl<T: ParsedFontTrait> LayoutContext<'_, T> {
    /// Internal method - called by `debug_log`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_log_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage {
                message: message.into(),
                location: "solver3".into(),
                message_type: Default::default(),
            });
        }
    }

    /// Internal method - called by `debug_info`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_info_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::info(message));
        }
    }

    /// Internal method - called by `debug_warning`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_warning_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::warning(message));
        }
    }

    /// Internal method - called by `debug_error`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_error_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::error(message));
        }
    }

    /// Internal method - called by `debug_box_props`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_box_props_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::box_props(message));
        }
    }

    /// Internal method - called by `debug_css_getter`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_css_getter_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::css_getter(message));
        }
    }

    /// Internal method - called by `debug_bfc_layout`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_bfc_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::bfc_layout(message));
        }
    }

    /// Internal method - called by `debug_ifc_layout`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_ifc_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::ifc_layout(message));
        }
    }

    /// Internal method - called by `debug_table_layout`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_table_layout_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::table_layout(message));
        }
    }

    /// Internal method - called by `debug_display_type`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_display_type_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage::display_type(message));
        }
    }
}

/// Main entry point for the incremental, cached layout engine.
///
/// `new_dom` is borrowed, not owned — every use inside is `&new_dom`,
/// so taking ownership was a pure formality that forced every caller
/// to `styled_dom.clone()` the DOM before calling. The clone was
/// ~2 MiB per render on excel.html; kept at the borrow now.
#[cfg(feature = "text_layout")]
/// Web-backend opt-out for display-list generation.
///
/// When set, [`layout_document`] runs the full positioning pipeline
/// (intrinsic sizing, taffy block/flex/grid, relative/sticky/absolute
/// adjustment → `calculated_positions`) but **skips
/// `generate_display_list`**, returning an empty [`DisplayList`]. The
/// web backend emits TLV DOM patches, not a display list, so it needs
/// the geometry in `calculated_positions` but nothing the painter
/// produces. This also lets the AArch64→wasm lift drop the entire
/// `display_list` painter surface (those symbols are classified `Leaf`
/// in `dll/src/web/symbol_table.rs::classify_for_name`, so the
/// transitive lifter never descends into them). Defaults `false` →
/// desktop/native behaviour is unchanged.
pub static SKIP_DISPLAY_LIST: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Set [`SKIP_DISPLAY_LIST`].
///
/// Provided as a function (rather than the
/// caller touching the static directly) so the web backend's
/// `dll`-crate caller reaches it through a normal `bl` into this
/// `azul_layout` function — keeping the static's address computation
/// intra-crate (direct `adrp+add`) instead of a cross-crate GOT load,
/// which the AArch64→wasm lift mirrors more reliably.
pub fn set_skip_display_list(skip: bool) {
    SKIP_DISPLAY_LIST.store(skip, core::sync::atomic::Ordering::Relaxed);
}

// M12.7: keep this out-of-line so the web lift sees it as its own wasm fn
// (not inlined into layout_dom_recursive). An opt-folded infinite loop in the
// solver (a mis-lifted loop exit) is otherwise hidden inside the giant inlined
// layout_dom_recursive; de-inlining lets AZ_FUEL/AZ_WASM_DEBUG name the actual
// source fn — and may itself prevent the inlining-induced fold. No perf cost on
// desktop (called once per layout).
#[inline(never)]
// node counts / indices / tree-len values fed to az_mark debug markers as u32; bounded.
#[allow(clippy::cast_possible_truncation)]
pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
    cache: &mut LayoutCache,
    text_cache: &mut TextLayoutCache,
    new_dom: &StyledDom,
    viewport: LogicalRect,
    font_manager: &crate::font_traits::FontManager<T>,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    text_selections: &BTreeMap<DomId, TextSelection>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    gpu_value_cache: Option<&azul_core::gpu::GpuValueCache>,
    renderer_resources: &azul_core::resources::RendererResources,
    id_namespace: azul_core::resources::IdNamespace,
    dom_id: DomId,
    cursor_is_visible: bool,
    cursor_locations: Vec<(DomId, NodeId, TextCursor)>,
    preedit_text: Option<String>,
    image_cache: &azul_core::resources::ImageCache,
    system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
) -> Result<DisplayList> {
    use crate::window::LayoutWindow;

    // Secondary mapping: anonymous wrappers (dom_node_id == None)
    // by (parent_new_idx, ordinal-among-anon-siblings). An
    // unchanged DOM produces the same anon wrappers in the same
    // order under the same parent — matching by position here
    // preserves their cache slots too. Without this, anon
    // wrappers re-allocate empty every reconcile and invalidate
    // their ancestors via `mark_dirty`.
    fn collect_anon_children_by_parent(
        tree: &LayoutTree,
    ) -> HashMap<usize, Vec<usize>> {
        let mut map: HashMap<usize, Vec<usize>> =
            HashMap::new();
        for (idx, node) in tree.nodes.iter().enumerate() {
            if node.dom_node_id.is_some() {
                continue;
            }
            if let Some(parent) = node.parent {
                map.entry(parent).or_default().push(idx);
            }
        }
        map
    }

    // Reset IFC ID counter at the start of each layout pass
    // This ensures IFCs get consistent IDs across frames when the DOM structure is stable
    layout_tree::IfcId::reset_counter();
    // in layout_document returns the rc=5 Err (the error enum can't be captured
    // reliably in the lift). The last value seen = the step that errored next.
    { let _ = (0xDD00_0001u32); }
    // If 0 here → the LogicalRect HFA arg was lost across the lifted call.

    if let Some(msgs) = debug_messages.as_mut() {
        msgs.push(LayoutDebugMessage::info(format!(
            "[Layout] layout_document called - viewport: ({:.1}, {:.1}) size ({:.1}x{:.1})",
            viewport.origin.x, viewport.origin.y, viewport.size.width, viewport.size.height
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "[Layout] DOM has {} nodes",
            new_dom.node_data.len()
        )));
    }

    // Create temporary context without counters for tree generation
    let mut counter_values = HashMap::new();
    let mut ctx_temp = LayoutContext {
        scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
        cursor_is_visible,
        cursor_locations: cursor_locations.clone(),
        preedit_text: preedit_text.clone(),
        dirty_text_overrides: BTreeMap::new(),
        cache_map: cache::LayoutCacheMap::default(), // temp context doesn't need real cache
        image_cache,
        system_style: system_style.clone(),
        get_system_time_fn,
    };

    crate::probe::sample_peak_rss("rss:enter_layout_document");

    // --- Step 0: record DOM pointer / viewport for diagnostics only ---
    //
    // NOTE: there is intentionally NO pointer-identity fast path here.
    // Comparing `new_dom as *const StyledDom as usize` against a stored
    // pointer is UNSOUND across layout passes: each `regenerate_layout`
    // builds a fresh `StyledDom`, and after the previous one is dropped
    // (e.g. `layout_and_generate_display_list` calls `layout_results.clear()`
    // before re-laying out), the allocator/stack frequently hands the new,
    // *different* StyledDom the SAME address. A pointer match therefore does
    // NOT prove the content is unchanged — it would return the previous
    // frame's display list for a structurally different DOM (e.g. an image
    // removed from the tree would still appear in `scan_used_images`,
    // breaking resource GC). The Step 1.1 structural-identity cache below
    // (root `subtree_hash` + viewport) is the correct, content-based skip;
    // it costs one ~600 µs reconcile pass but cannot be fooled by address
    // reuse.
    let dom_ptr = std::ptr::from_ref::<StyledDom>(new_dom) as usize;
    cache.prev_dom_ptr = dom_ptr;
    cache.prev_viewport = viewport;

    // --- Step 1: Reconciliation & Invalidation ---
    crate::probe::reset_peak();
    let (new_tree_val, mut recon_result) =
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport)?;
    // [g56 FIX] Box the LayoutTree onto the HEAP. The lifted `&mut new_tree` passed to
    // calculate_intrinsic_sizes was mis-lifted (callee saw nodes.len()=0 while the caller saw 2)
    // because a stack/SROA'd `new_tree`'s address doesn't survive the cross-function lifted call
    // (taking `&new_tree` lifted to 0x0). A heap allocation has a stable absolute wasm address
    // that lifts reliably (cf. M8.4 "heap allocations work fine"). Deref coercion handles the
    // `&new_tree`/`&mut new_tree`/`new_tree.field` sites unchanged.
    let mut new_tree = Box::new(new_tree_val);
    { let _ = (0xDD00_0002u32); }
    // [az-diag g51 REVERT] 0x71 = reconcile_and_invalidate returned OK (no InvalidTree in reconcile).
    unsafe { crate::az_mark(0x60704_u32, (0x71u32)); }
    // [az-diag g54 REVERT] 0x40740 = new_tree.nodes.len() RIGHT AFTER reconcile. If 0 → reconcile
    // built an empty LayoutTree (the bug is in reconcile_recursive/create_node_from_dom). If 2 →
    // reconcile is fine and the tree gets emptied/mis-lifted downstream (check 0x40744 at the loop).
    unsafe { crate::az_mark(0x60740_u32, (new_tree.nodes.len() as u32)); }
    crate::probe::sample_peak_rss("rss:after_reconcile");
    crate::probe::sample_phase_peak("rss:peak_during_reconcile");

    // --- Step 1.1: Structural-identity display-list cache ---
    //
    // If the reconciled root subtree_hash matches the cached one AND
    // the viewport is unchanged, nothing structural has moved — skip
    // layout, positioning, AND display-list generation and return
    // the cached display list verbatim.
    //
    // This fires on re-renders of an unchanged DOM: the reconcile
    // pass still walks and hashes the tree, but that's ~600 µs vs
    // the ~4 ms it would otherwise cost to re-emit the display list.
    if let Some((cached_hash, cached_viewport, cached_dl)) = &cache.cached_display_list {
        let new_root_hash = new_tree
            .cold(new_tree.root)
            .map(|c| c.subtree_hash);
        if new_root_hash == Some(*cached_hash) && *cached_viewport == viewport {
            let _p = crate::probe::Probe::span("display_list_cache_hit");
            return Ok(cached_dl.clone());
        }
    }

    // Step 1.2: Clear Taffy Caches for Dirty Nodes
    for &node_idx in &recon_result.intrinsic_dirty {
        if let Some(warm) = new_tree.warm_mut(node_idx) {
            warm.taffy_cache.clear();
        }
    }

    // Step 1.3: Compute CSS Counters
    // This must be done after tree generation but before layout,
    // as list markers need counter values during formatting context layout
    {
        let _p = crate::probe::Probe::span("compute_counters");
        cache::compute_counters(new_dom, &new_tree, &mut counter_values);
    }
    // [az-diag g51 REVERT] 0x72 = compute_counters done (InvalidTree, if any, is after here).
    unsafe { crate::az_mark(0x60704_u32, (0x72u32)); }

    // Step 1.4: Resize and invalidate per-node cache (Taffy-inspired 9+1 slot cache)
    // Move cache_map out of LayoutCache for the duration of layout (avoids borrow conflicts).
    // It will be moved back after the layout pass completes.
    //
    // Critically: the old `cache_map.entries` is indexed by OLD
    // layout-tree positions. The NEW tree may have re-ordered
    // indices (anonymous wrapper slots shifted, whitespace nodes
    // dropped, etc.). A plain `resize_with(default)` would silently
    // serve the wrong node's cached result for any shifted index.
    //
    // Re-map by stable identity: build `old_layout_idx → new_layout_idx`
    // via the `(dom_node_id → layout_idx)` tables on both trees,
    // then move each surviving cache entry into its new slot. Nodes
    // without a matching DOM id (pure anonymous wrappers) fall
    // through to the default (empty, i.e. dirty) entry.
    let mut cache_map = std::mem::take(&mut cache.cache_map);
    let _probe_cache_remap = Some(crate::probe::Probe::span("cache_map_remap"));
    if let Some(old_tree) = cache.tree.as_ref() {
        let mut remapped = cache::LayoutCacheMap::default();
        remapped.entries.resize_with(new_tree.nodes.len(), Default::default);

        // Primary mapping: DOM id → layout idx on both sides. This
        // covers every node that has a corresponding DOM node.
        for (dom_id, new_indices) in &new_tree.dom_to_layout {
            let Some(old_indices) = old_tree.dom_to_layout.get(dom_id) else {
                continue;
            };
            for (pair_idx, &new_layout_idx) in new_indices.iter().enumerate() {
                let Some(&old_layout_idx) = old_indices.get(pair_idx) else {
                    continue;
                };
                if old_layout_idx >= cache_map.entries.len()
                    || new_layout_idx >= remapped.entries.len()
                {
                    continue;
                }
                remapped.entries[new_layout_idx] =
                    core::mem::take(&mut cache_map.entries[old_layout_idx]);
            }
        }

        // Build old-parent → [old_anon_indices] and
        // new-parent → [new_anon_indices]; match by pair position.
        let old_anon_by_parent = collect_anon_children_by_parent(old_tree);
        let new_anon_by_parent = collect_anon_children_by_parent(&new_tree);

        // For each new parent we know: look up its old twin by the
        // dom-id mapping we just populated, then match anon children
        // positionally within that parent.
        // Build a new→old layout-idx lookup from the primary pass.
        let mut new_to_old_layout_idx: HashMap<usize, usize> =
            HashMap::new();
        for (dom_id, new_indices) in &new_tree.dom_to_layout {
            let Some(old_indices) = old_tree.dom_to_layout.get(dom_id) else {
                continue;
            };
            for (pair_idx, &new_layout_idx) in new_indices.iter().enumerate() {
                if let Some(&old_layout_idx) = old_indices.get(pair_idx) {
                    new_to_old_layout_idx.insert(new_layout_idx, old_layout_idx);
                }
            }
        }

        for (new_parent_idx, new_anon_children) in new_anon_by_parent {
            let Some(&old_parent_idx) = new_to_old_layout_idx.get(&new_parent_idx) else {
                continue;
            };
            let Some(old_anon_children) = old_anon_by_parent.get(&old_parent_idx) else {
                continue;
            };
            for (ord, &new_anon_idx) in new_anon_children.iter().enumerate() {
                let Some(&old_anon_idx) = old_anon_children.get(ord) else {
                    continue;
                };
                if old_anon_idx >= cache_map.entries.len()
                    || new_anon_idx >= remapped.entries.len()
                {
                    continue;
                }
                remapped.entries[new_anon_idx] =
                    core::mem::take(&mut cache_map.entries[old_anon_idx]);
            }
        }

        cache_map = remapped;
    } else {
        cache_map.resize_to_tree(new_tree.nodes.len());
    }
    drop(_probe_cache_remap);
    crate::probe::sample_peak_rss("rss:after_cache_remap");
    for &node_idx in &recon_result.intrinsic_dirty {
        cache_map.mark_dirty(node_idx, &new_tree.nodes);
    }
    for &node_idx in &recon_result.layout_roots {
        cache_map.mark_dirty(node_idx, &new_tree.nodes);
    }

    // Now create the real context with computed counters
    let mut ctx = LayoutContext {
        scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
        cursor_is_visible,
        cursor_locations,
        preedit_text,
        dirty_text_overrides: BTreeMap::new(),
        cache_map, // Moved from LayoutCache; will be moved back after layout
        image_cache,
        system_style,
        get_system_time_fn,
    };

    // --- Step 1.5: Early Exit Optimization ---
    // M12.7: `&& cache.tree.is_some()` — this "nothing changed, reuse cached
    // layout" fast path REQUIRES a cached tree; on COLD layout cache.tree is
    // None, so entering here would hit `ok_or(InvalidTree)`. recon_result must
    // be dirty on cold (the viewport-resize dirties the root), but if
    // is_clean() mis-evaluates we'd wrongly early-exit → InvalidTree. Guarding
    // on a cached tree is both correct (can't reuse what isn't there) and
    // robust. (rc=5 post-reconcile, step=2: this was the failing `?`.)
    if recon_result.is_clean() && cache.tree.is_some() {
        debug_log!(ctx, "No changes, returning existing display list");
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        // Use cached scroll IDs if available, otherwise compute them
        let scroll_ids = if cache.scroll_ids.is_empty() {
            use crate::window::LayoutWindow;
            let (scroll_ids, scroll_id_to_node_id) =
                LayoutWindow::compute_scroll_ids(tree, new_dom);
            cache.scroll_ids = scroll_ids.clone();
            cache.scroll_id_to_node_id = scroll_id_to_node_id;
            scroll_ids
        } else {
            cache.scroll_ids.clone()
        };

        if SKIP_DISPLAY_LIST.load(core::sync::atomic::Ordering::Relaxed) {
            return Ok(DisplayList::default());
        }
        return generate_display_list(
            &mut ctx,
            tree,
            &cache.calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        );
    }

    { let _ = (0xDD00_0003u32); }
    // [az-diag g51 REVERT] 0x80 = reached the incremental layout loop (past early-exit + remap + dirty loops).
    unsafe { crate::az_mark(0x60704_u32, (0x80u32)); }
    // [az-diag g65 PATH-B VALIDATION] new_tree is still valid here (=2). Clone it into the HEAP-backed
    // cache.tree (set AFTER the remap+early-exit which read the OLD cache.tree). cache is the stable
    // &mut arg (read correctly throughout), so cache.tree is NOT a deep-SP-relative stack local. At the
    // sizing call we read BOTH: stack new_tree (expect 0=corrupted) vs heap cache.tree (expect 2 if
    // path B sidesteps the SP-drift/wild-store). If heap=2, the full cache.tree refactor will fix it.
    cache.tree = Some((*new_tree).clone());
    // [az-diag g66] disambiguate the g65 heap=1: read BOTH right after the clone. 0x407C0 = stack
    // new_tree.nodes.len() (source), 0x407C4 = clone cache.tree.nodes.len(). If src=2 & clone=1 →
    // Vec::clone MIS-LIFTS (drops a node) → the full MOVE-based cache.tree refactor avoids it (do it).
    // If src=1=clone → corruption already reached line 758 (heisenbug) → move won't help.
    unsafe {
        crate::az_mark(0x607C0_u32, (new_tree.nodes.len() as u32));
        crate::az_mark(0x607C4_u32, (cache.tree.as_ref().map_or(999, |t| t.nodes.len()) as u32));
    }

    // --- Step 2: Incremental Layout Loop (handles scrollbar-induced reflows) ---
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > MAX_SCROLLBAR_REFLOW_ITERATIONS {
            debug_warning!(ctx, "Scrollbar reflow loop hit limit of {} iterations, breaking to avoid infinite loop", MAX_SCROLLBAR_REFLOW_ITERATIONS);
            break;
        }

        calculated_positions = {
            let _p = crate::probe::Probe::span("clone_calculated_positions");
            cache.calculated_positions.clone()
        };
        // [az-diag g70 RELIABLE free-band] 0x60780 = nodes.len AFTER the in-loop calculated_positions.clone().
        unsafe { crate::az_mark(0x60780_u32, (new_tree.nodes.len() as u32)); }
        let mut reflow_needed_for_scrollbars = false;

        {
            crate::probe::reset_peak();
            // [az-diag g70 RELIABLE free-band] 0x60784 = nodes.len AFTER reset_peak (before the calc Span).
            unsafe { crate::az_mark(0x60784_u32, (new_tree.nodes.len() as u32)); }
            let _p = crate::probe::Probe::span("calc_intrinsic_sizes");
            // [az-diag g70 RELIABLE free-band] 0x60788 = nodes.len AFTER the calc_intrinsic_sizes Span.
            unsafe { crate::az_mark(0x60788_u32, (new_tree.nodes.len() as u32)); }
            // [az-diag g72 FIX] REMOVED the g48 `#[cfg(feature="web_lift")] panic!(...)` that lived
            // here. web-transpiler => azul-layout?/web_lift IS enabled (dll/Cargo.toml:651), so this
            // panic WAS compiled in, and with `-Z build-std-features=panic_immediate_abort` it lowered
            // to a bare `brk #0x1` right after the 0x90 marker — aborting BEFORE calculate_intrinsic_sizes.
            // The whole-session "new_tree 2→0 corruption" was a MIRAGE: the beforeCall marker store was
            // dead-code-eliminated (after the abort), so the harness read uninitialized 0, not a corrupted
            // tree. Native disasm of layout_document proved it: 0x90 marker store → `brk #0x1` → no `bl
            // calculate_intrinsic_sizes` anywhere. (The prior "string absent ⇒ web_lift off" check was
            // wrong — panic_immediate_abort strips the message string.)
            // [az-diag g65 PATH-B VALIDATION] 0x40748 = stack new_tree.nodes.len() (expect 0),
            // 0x4074C = HEAP cache.tree.nodes.len() (expect 2 if path B sidesteps the corruption).
            unsafe {
                crate::az_mark(0x60748_u32, (new_tree.nodes.len() as u32));
                crate::az_mark(0x6074C_u32, (cache.tree.as_ref().map_or(999, |t| t.nodes.len()) as u32));
            }
            calculate_intrinsic_sizes(
                &mut ctx,
                &mut new_tree,
                text_cache,
                &recon_result.intrinsic_dirty,
            )?;
        }
        crate::probe::sample_peak_rss("rss:after_calc_intrinsic");
        crate::probe::sample_phase_peak("rss:peak_during_intrinsic");
        // divergence is inside calculate_intrinsic_sizes (the SIMD/text intrinsic pass).
        { let _ = (0xDD00_0005u32); }

        for &root_idx in &recon_result.layout_roots {
            let (cb_pos, cb_size) = get_containing_block_for_node(
                &new_tree,
                new_dom,
                root_idx,
                &calculated_positions,
                viewport,
            );
            // 0x05, the divergence is INSIDE get_containing_block_for_node (or the for-loop
            // entry); if 0x53 but not 0x55, it's the margin logic / box_props.unpack below.
            { let _ = (0xDD00_0053u32); }
            // get_containing_block_for_node(viewport)). 800 here but viewport=800 ⟹ OK;
            // 0 here with viewport=800 ⟹ get_containing_block_for_node lost it (HFA return).

            // For ROOT nodes (no parent), we need to account for their margin.
            // The containing block position from viewport is (0, 0), but the root's
            // content starts at (margin + border + padding, margin + border + padding).
            // We pass margin-adjusted position so calculate_content_box_pos works correctly.
            let root_node = &new_tree.nodes[root_idx];
            let root_bp = root_node.box_props.unpack();
            { let _ = (0xDD00_0054u32); }

            let is_root_with_margin = root_node.parent.is_none()
                && (root_bp.margin.left != 0.0 || root_bp.margin.top != 0.0);

            let adjusted_cb_pos = if is_root_with_margin {
                LogicalPosition::new(
                    cb_pos.x + root_bp.margin.left,
                    cb_pos.y + root_bp.margin.top,
                )
            } else {
                cb_pos
            };
            { let _ = (0xDD00_0056u32); }

            // DEBUG: Log containing block info for this root
            if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                let dom_name = root_node
                    .dom_node_id
                    .and_then(|id| new_dom.node_data.as_container().internal.get(id.index())).map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

                debug_msgs.push(LayoutDebugMessage::new(
                    LayoutDebugMessageType::PositionCalculation,
                    format!(
                        "[LAYOUT ROOT {}] {} - CB pos=({:.2}, {:.2}), adjusted=({:.2}, {:.2}), \
                         CB size=({:.2}x{:.2}), viewport=({:.2}x{:.2}), margin=({:.2}, {:.2})",
                        root_idx,
                        dom_name,
                        cb_pos.x,
                        cb_pos.y,
                        adjusted_cb_pos.x,
                        adjusted_cb_pos.y,
                        cb_size.width,
                        cb_size.height,
                        viewport.size.width,
                        viewport.size.height,
                        root_bp.margin.left,
                        root_bp.margin.top
                    ),
                ));
            }

            // Purge after intrinsic sizing — frees child_intrinsics Vecs,
            // IntrinsicSizeCalculator temporaries, text measurement caches.
            crate::probe::hint_purge_allocator();
            crate::probe::sample_peak_rss("rss:before_root_layout");
            crate::probe::reset_peak();
            // 0x57 = it RETURNED. If step stays 0x55, calculate_layout_for_subtree diverges.
            { let _ = (0xDD00_0055u32); }
            // This is exactly what calc_used_size reads as `viewport`. 0 here pinpoints the
            // loss to the ctx build (viewport.size → ctx.viewport_size copy).
            // 0x5E = Err. Do NOT propagate (continue to the cache store) so layout-real can
            // see whether the geometry was computed regardless of a (possibly spurious,
            // niche-Result-mis-discriminated) Err.
            let _clr = {
                let _p = crate::probe::Probe::span("root_layout_pass");
                cache::calculate_layout_for_subtree(
                    &mut ctx,
                    &mut new_tree,
                    text_cache,
                    root_idx,
                    adjusted_cb_pos,
                    cb_size,
                    &mut calculated_positions,
                    &mut reflow_needed_for_scrollbars,
                    &mut cache.float_cache,
                    cache::ComputeMode::PerformLayout,
                )
            };
            { let _ = (if _clr.is_ok() { 0xDD00_0057u32 } else { 0xDD00_005Eu32 }); }
            crate::probe::sample_peak_rss("rss:after_root_layout");
            crate::probe::sample_phase_peak("rss:peak_during_root_layout");

            // CRITICAL: Insert the root node's own position into calculated_positions
            // This is necessary because calculate_layout_for_subtree only inserts
            // positions for children, not for the root itself.
            //
            // For root nodes, the position should be at (margin.left, margin.top) relative
            // to the viewport origin, because the margin creates space between the viewport
            // edge and the element's border-box.
            if !pos_contains(&calculated_positions, root_idx) {
                let root_node = &new_tree.nodes[root_idx];
                let root_bp2 = root_node.box_props.unpack();

                // Calculate the root's border-box position by adding margins to viewport origin
                // This is different from non-root nodes which inherit their position from
                // their containing block.
                let root_position = LogicalPosition::new(
                    cb_pos.x + root_bp2.margin.left,
                    cb_pos.y + root_bp2.margin.top,
                );

                // DEBUG: Log root positioning
                if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                    let dom_name = root_node
                        .dom_node_id
                        .and_then(|id| new_dom.node_data.as_container().internal.get(id.index())).map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

                    debug_msgs.push(LayoutDebugMessage::new(
                        LayoutDebugMessageType::PositionCalculation,
                        format!(
                            "[ROOT POSITION {}] {} - Inserting position=({:.2}, {:.2}) (viewport origin + margin), \
                             margin=({:.2}, {:.2}, {:.2}, {:.2})",
                            root_idx,
                            dom_name,
                            root_position.x,
                            root_position.y,
                            root_bp2.margin.top,
                            root_bp2.margin.right,
                            root_bp2.margin.bottom,
                            root_bp2.margin.left
                        ),
                    ));
                }

                pos_set(&mut calculated_positions, root_idx, root_position);
            }
        }
        // (step 6). If step stays 5, the divergence is in calculate_layout_for_subtree.
        { let _ = (0xDD00_0006u32); }

        {
            let _p = crate::probe::Probe::span("reposition_clean_subtrees");
            cache::reposition_clean_subtrees(
                new_dom,
                &new_tree,
                &recon_result.layout_roots,
                &mut calculated_positions,
            );
        }

        if reflow_needed_for_scrollbars {
            debug_log!(ctx,
                "Scrollbars changed container size, starting full reflow (loop {})",
                loop_count
            );
            recon_result.layout_roots.clear();
            recon_result.layout_roots.insert(new_tree.root);
            recon_result.intrinsic_dirty = (0..new_tree.nodes.len()).collect();
            continue;
        }

        break;
    }

    // +spec:positioning:8d1286 - normal flow, relative, float, absolute positioning dispatch
    // +spec:positioning:bdfc81 - Layout divided into sizing (Step 2) then positioning (Step 3)
    // --- Step 3: Adjust Relatively Positioned Elements ---
    // +spec:positioning:a831e8 - inline content width uses pre-relative-offset positions (satisfied by post-layout relative adjustment)
    // +spec:positioning:e2647b - Relative positioning applied after line height calculation, so line height is not adjusted for relative offsets
    // +spec:positioning:77a2d2 - Relatively positioned boxes considered without their offset during auto height
    // +spec:positioning:b47ac2 - Relatively positioned boxes considered without their offset for block auto height
    // Relative offsets applied AFTER layout, so auto-height calculation sees normal-flow positions.
    // This must be done BEFORE positioning out-of-flow elements, because
    // relatively positioned elements establish containing blocks for their
    // absolutely positioned descendants. If we adjust relative positions after
    // positioning absolute elements, the absolute elements will be positioned
    // relative to the wrong (pre-adjustment) position of their containing block.
    // Pass the viewport to correctly resolve percentage offsets for the root element.
    {
        let _p = crate::probe::Probe::span("adjust_relative_positions");
        positioning::adjust_relative_positions(
            &mut ctx,
            &new_tree,
            &mut calculated_positions,
            viewport,
        );
    }

    // --- Step 3.25: Adjust Sticky Positioned Elements ---
    // Sticky elements are laid out in normal flow, then their visual position
    // is clamped based on scroll offset and inset properties relative to the
    // nearest scrollport. Must happen after relative positioning but before
    // absolute positioning (sticky elements establish containing blocks).
    {
        let _p = crate::probe::Probe::span("adjust_sticky_positions");
        positioning::adjust_sticky_positions(
            &mut ctx,
            &new_tree,
            &mut calculated_positions,
            scroll_offsets,
            viewport,
        );
    }

    // --- Step 3.5: Position Out-of-Flow Elements ---
    // This must be done AFTER adjusting relative positions, so that absolutely
    // positioned elements are positioned relative to the final (post-adjustment)
    // position of their relatively positioned containing blocks.
    {
        let _p = crate::probe::Probe::span("position_out_of_flow");
        positioning::position_out_of_flow_elements(
            &mut ctx,
            &mut new_tree,
            text_cache,
            &mut calculated_positions,
            viewport,
        );
    }

    // --- Step 3.75: Compute Stable Scroll IDs ---
    // This must be done AFTER layout but BEFORE display list generation
    let (scroll_ids, scroll_id_to_node_id) = {
        let _p = crate::probe::Probe::span("compute_scroll_ids");
        LayoutWindow::compute_scroll_ids(&new_tree, new_dom)
    };

    crate::probe::sample_peak_rss("rss:before_display_list");
    crate::probe::reset_peak();
    // --- Step 4: Generate Display List & Update Cache ---
    let display_list = if SKIP_DISPLAY_LIST.load(core::sync::atomic::Ordering::Relaxed) {
        // Web backend: positions are done; the painter is dead weight.
        DisplayList::default()
    } else {
        let _p = crate::probe::Probe::span("generate_display_list");
        generate_display_list(
            &mut ctx,
            &new_tree,
            &calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        )?
    };
    crate::probe::sample_phase_peak("rss:peak_during_display_list");

    // Move cache_map back into LayoutCache before dropping ctx
    let _p_writeback = crate::probe::Probe::span("cache_writeback");
    let cache_map_back = std::mem::take(&mut ctx.cache_map);

    // Cache the freshly-generated display list keyed on the root's
    // subtree_hash + viewport. If the next `layout_document` call
    // sees matching values after reconcile, it returns this clone
    // directly and skips all downstream work.
    let root_subtree_hash = new_tree
        .cold(new_tree.root)
        .map_or(layout_tree::SubtreeHash(0), |c| c.subtree_hash);
    cache.cached_display_list = Some((root_subtree_hash, viewport, display_list.clone()));

    cache.tree = Some(*new_tree); // [g56] unbox the heap LayoutTree back into the cache
    cache.previous_positions = std::mem::replace(&mut cache.calculated_positions, calculated_positions);
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    // + calculated_positions.len in the low bits. If step stays 3, it diverged earlier.
    { let _ = (0xDD00_0004u32 | ((cache.calculated_positions.len() as u32 & 0xfff) << 4)); }
    cache.counters = counter_values;
    cache.cache_map = cache_map_back;
    crate::probe::sample_peak_rss("rss:after_layout_document");

    Ok(display_list)
}

// +spec:containing-block:159830 - Containing block chain: parent content-box for in-flow, viewport for initial containing block
// +spec:containing-block:22fbaa - computes the element's original containing block (before positioning effects)
// +spec:containing-block:238fc5 - containing block dimensions calculated here (CSS 2.2 §9.1.2 forward ref to §10)
// +spec:containing-block:263629 - block element's content-box establishes the containing block for its line boxes
// +spec:containing-block:2a5280 - boxes act as containing blocks for descendants; CB = parent's content box
// +spec:containing-block:6776cb - boxes positioned w.r.t. containing block but not confined; overflow allowed
// +spec:containing-block:718894 - CB derived from parent content-box edges; root uses initial CB (viewport)
// +spec:containing-block:a2aa37 - box edges act as containing block for descendants; initial containing block = viewport
// +spec:containing-block:e23b3f - CSS 2.2 §10.1: initial containing block = viewport; static/relative = parent content-box; fixed = viewport
// +spec:containing-block:e8fdb2 - Containing block resolution (CSS2 §9.1.2, §10.1)
// +spec:overflow:9a2b11 - containing block is content-box of parent; boxes may overflow it
// +spec:positioning:acc663 - containing block definition: element boxes positioned relative to containing block
pub(super) fn get_containing_block_for_node(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    node_idx: usize,
    calculated_positions: &PositionVec,
    viewport: LogicalRect,
) -> (LogicalPosition, LogicalSize) {
    if let Some(parent_idx) = tree.get(node_idx).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(parent_idx) {
            let pos = pos_get(calculated_positions, parent_idx)
                .unwrap_or(viewport.origin);
            let size = parent_node.used_size.unwrap_or_default();
            // Position in calculated_positions is the margin-box position
            // To get content-box, add: border + padding (NOT margin, that's already in pos)
            let pbp = parent_node.box_props.unpack();
            let content_pos = LogicalPosition::new(
                pos.x + pbp.border.left + pbp.padding.left,
                pos.y + pbp.border.top + pbp.padding.top,
            );

            if let Some(dom_id) = parent_node.dom_node_id {
                let styled_node_state = &styled_dom
                    .styled_nodes
                    .as_container()
                    .get(dom_id)
                    .map(|n| &n.styled_node_state)
                    .copied()
                    .unwrap_or_default();
                // +spec:containing-block:c205e5 - writing mode of containing block used for inner_size (orthogonal flow awareness)
                let writing_mode =
                    get_writing_mode(styled_dom, dom_id, styled_node_state).unwrap_or_default();
                let content_size = pbp.inner_size(size, writing_mode);
                return (content_pos, content_size);
            }

            return (content_pos, size);
        }
    }
    
    // +spec:containing-block:41bdfc - ICB equals viewport; overflow:hidden on root clips to ICB
    // +spec:containing-block:1eed60 - Initial containing block establishes a BFC; viewport is the ICB
    // +spec:containing-block:99866f - Containing block is a rectangle for sizing/positioning; ICB from viewport
    // +spec:containing-block:22f09b - viewport serves as initial containing block for root element
    // Root element's containing block is the initial containing block (CSS 2.2 §10.1, CSS Display 3 §2.8).
    // +spec:containing-block:2fd7b1 - ICB equals viewport; principal writing mode propagated to ICB
    // Root element's containing block is the initial containing block (CSS 2.2 §10.1, CSS Display 3 §2.8).
    // The principal writing mode is propagated to the ICB and viewport (css-writing-modes-4 §8.1).
    // +spec:containing-block:5efb84 - Root element's containing block is the initial containing block
    // +spec:containing-block:6278fb - initial containing block is the viewport; also serves as initial fixed containing block
    // Root element's containing block is the initial containing block (CSS 2.2 §10.1, CSS Display 3 §2.8).
    // For ROOT nodes: the containing block is the viewport (initial containing block).
    // Do NOT subtract margin here - margins are handled in calculate_used_size().
    // The margin creates space between viewport edge and element's border-box,
    // but the available space for calculating width/height percentages
    // is still the full viewport size.
    (viewport.origin, viewport.size)
}

// [g119 az-web-lift FIX] `#[repr(C, u8)]` (was repr(Rust)): the `Text(font_traits::LayoutError)`
// variant's String/FontSelector pointer gives `Result<T, LayoutError>` a POINTER-niche disc, which
// the web lift MIS-READS → every solver3 `?`/Result return flips Ok→Err (heisenbug; g118 = collect's
// Result<(),LayoutError> arrived as Err → rc=5 InvalidTree though the out-param content was correct).
// An explicit u8 tag (0..=4) moves the Result niche to unused tag values (5..) = a simple u8 compare
// the lift handles. Same disc-mis-lift class as InlineContent/LogicalItem/ShapedItem (g117/g118).
#[derive(Debug)]
#[repr(C, u8)]
pub enum LayoutError {
    InvalidTree,
    SizingFailed,
    PositioningFailed,
    DisplayListFailed,
    Text(crate::font_traits::LayoutError),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidTree => write!(f, "Invalid layout tree"),
            Self::SizingFailed => write!(f, "Sizing calculation failed"),
            Self::PositioningFailed => write!(f, "Position calculation failed"),
            Self::DisplayListFailed => write!(f, "Display list generation failed"),
            Self::Text(e) => write!(f, "Text layout error: {e:?}"),
        }
    }
}

impl From<crate::font_traits::LayoutError> for LayoutError {
    fn from(err: crate::font_traits::LayoutError) -> Self {
        Self::Text(err)
    }
}

impl std::error::Error for LayoutError {}

pub type Result<T> = std::result::Result<T, LayoutError>;
