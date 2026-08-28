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
pub use layout_tree::LayoutNodeId;
pub mod break_token;
pub mod page_breaks;
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

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::{DocumentId, ScrollPosition},
    resources::RendererResources,
    selection::{TextCursor, TextSelection},
    styled_dom::StyledDom,
};

/// Sentinel value for "position not yet computed". No real position is ever `f32::MIN`.
pub(crate) const POSITION_UNSET: LogicalPosition = LogicalPosition {
    x: f32::MIN,
    y: f32::MIN,
};

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
#[must_use]
pub fn pos_get(positions: &PositionVec, idx: usize) -> Option<LogicalPosition> {
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
#[must_use]
pub fn pos_contains(positions: &PositionVec, idx: usize) -> bool {
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
        cache::LayoutCache, display_list::DisplayList, fc::LayoutConstraints,
        layout_tree::DirtyFlag,
    },
};

/// Central context for a single layout pass.
#[derive(Debug)]
pub struct LayoutContext<'a, T: ParsedFontTrait> {
    pub styled_dom: &'a StyledDom,
    /// Per-document memo for [`crate::solver3::getters::get_style_properties`].
    ///
    /// Resolving a node's style allocates ~6.5 KB (a font stack of heap
    /// `String`s, a background-content Vec, a font-features Vec, a cloned
    /// family list), and the sizing pass and the inline-layout pass each
    /// resolve the same node independently. heaptrack measured 6.33 MB of
    /// peak heap over 966 calls on a 3-page document.
    ///
    /// It lives HERE, on the context, so its lifetime is exactly the
    /// document's. An earlier attempt used a thread-local keyed on
    /// `ptr::from_ref(styled_dom)`, which passed every library test and
    /// reddened sixteen reftests: a `StyledDom` is dropped and the next one
    /// reuses the address, so the second document was served the first
    /// one's styles. Ownership makes that unrepresentable rather than
    /// merely unlikely.
    pub style_cache: getters::StyleCache,
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
    /// Per-pass record: layout-tree indices of IFC roots whose LINE LAYOUT
    /// was recomputed this pass (`layout_flow` ran — `GlyphSwap` reuse does
    /// NOT count). The DL-patching invalidation signal: a re-flowed IFC's
    /// text items changed shape and must re-emit; everything else can be
    /// copied+translated. Drained into the patch machinery by
    /// `layout_document`; empty on GlyphSwap-only passes.
    pub reflowed_ifcs: std::collections::BTreeSet<usize>,
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
    /// Per-node multi-slot cache (Taffy-inspired 9+1 architecture).
    /// Moved out of `LayoutCache` via `std::mem::take` for the duration of layout,
    /// then moved back after the layout pass completes.
    pub cache_map: cache::LayoutCacheMap,
    /// Image cache for resolving `background-image: url(...)` references.
    pub image_cache: &'a azul_core::resources::ImageCache,
    /// Content overlay for resolving quickly-mutable node content (swapped
    /// images, produced callback frames): readers resolve overlay→DOM via
    /// [`crate::overlay::ResolvedContent`]. `None` outside a live window
    /// (standalone paged/PDF layout, unit tests) — the DOM is then authoritative.
    pub content_overlay: Option<&'a crate::overlay::ContentOverlay>,
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
    /// The one overlay→DOM content read order for this context's DOM.
    /// Layout (intrinsic sizing) and display-list build resolve node content
    /// exclusively through this — never via `NodeType::Image` directly.
    #[must_use]
    pub const fn resolved_content(&self) -> crate::overlay::ResolvedContent<'_> {
        crate::overlay::ResolvedContent {
            overlay: self.content_overlay,
            styled_dom: self.styled_dom,
            dom_id: self.styled_dom.dom_id,
        }
    }

    /// Internal method - called by `debug_log`! macro after checking `debug_messages.is_some()`
    #[inline]
    pub fn debug_log_inner(&mut self, message: String) {
        if let Some(messages) = self.debug_messages.as_mut() {
            messages.push(LayoutDebugMessage {
                message: message.into(),
                location: "solver3".into(),
                message_type: LayoutDebugMessageType::default(),
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
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if document layout fails.
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
    content_overlay: Option<&crate::overlay::ContentOverlay>,
    system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    // The CSS DIFF, folded into the solver's own dirtiness. The solver's
    // reconcile compares DOM structure/content and is BLIND to a changed
    // stylesheet over an identical tree — a CSS-only remount reused stale
    // geometry (200px measured where the new sheet said 40px) and the
    // structural-identity DL cache served stale paint. Each entry is a node
    // whose computed style changed, with the WORST `RelayoutScope` across
    // its changed properties; `RelayoutScope::None` entries are paint-only
    // changes (colour/background/...) that must repaint without a single
    // layout pass — the aggressive-caching half of this contract.
    css_dirty: &[(NodeId, azul_css::props::property::RelayoutScope)],
) -> Result<std::sync::Arc<DisplayList>> {
    use crate::window::LayoutWindow;

    // Secondary mapping: anonymous wrappers (dom_node_id == None)
    // by (parent_new_idx, ordinal-among-anon-siblings). An
    // unchanged DOM produces the same anon wrappers in the same
    // order under the same parent — matching by position here
    // preserves their cache slots too. Without this, anon
    // wrappers re-allocate empty every reconcile and invalidate
    // their ancestors via `mark_dirty`.
    fn collect_anon_children_by_parent(tree: &LayoutTree) -> HashMap<usize, Vec<usize>> {
        let mut map: HashMap<usize, Vec<usize>> = HashMap::new();
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
    {
        let _ = (0xDD00_0001u32);
    }
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
        style_cache: Default::default(),
        scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
        reflowed_ifcs: std::collections::BTreeSet::new(),
        cursor_is_visible,
        cursor_locations: cursor_locations.clone(),
        preedit_text: preedit_text.clone(),
        cache_map: cache::LayoutCacheMap::default(), // temp context doesn't need real cache
        image_cache,
        content_overlay,
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
    let doc_setup_span = crate::probe::Probe::span("doc_setup_to_reconcile");
    let dom_ptr = std::ptr::from_ref::<StyledDom>(new_dom) as usize;
    cache.prev_dom_ptr = dom_ptr;
    cache.prev_viewport = viewport;

    // --- Step 1: Reconciliation & Invalidation ---
    crate::probe::reset_peak();
    // RESIZE-ONLY SKIP: the incremental-relayout resize path hands solver3
    // the SAME StyledDom object with zero DOM/style dirt — the viewport is
    // the only delta. Reconcile would walk 1209 nodes to rediscover
    // "everything reused, nothing dirty" (~9.6 ms/pass on big.md) and the
    // cache_map remap would rebuild an identity mapping. Take the retained
    // tree as-is instead: indices are unchanged (so the per-node size/layout
    // caches keep working WITHOUT a remap), layout_roots={root} re-runs the
    // top-down pass at the new size, and the censuses read full-reuse. The
    // hint is a one-shot latch set ONLY by the resize-latch call sites
    // (common/layout.rs incremental_relayout_for_resize); restyle-driven
    // incremental relayouts keep full reconcile — they NEED its fingerprint
    // diff for paint-dirty classification. The dom-id sanity check guards
    // against a stale hint meeting a swapped DOM.
    drop(doc_setup_span);
    let resize_only = core::mem::take(&mut cache.resize_only_hint);
    let dom_len_for_hint = new_dom.node_data.as_ref().len();
    let (new_tree_val, mut recon_result) = if resize_only
        && cache.tree.as_ref().is_some_and(|t| {
            t.dom_to_layout
                .last_key_value()
                .is_none_or(|(max_id, _)| max_id.index() < dom_len_for_hint)
        }) {
        let _p = crate::probe::Probe::span("reconcile_skipped_resize_only");
        cache.last_reconcile_was_skipped = true;
        let tree = cache.tree.take().expect("checked is_some above");
        // DL-patching input: the sizes the PREVIOUS pass computed — the
        // layout pass below overwrites used_size in place (same tree
        // object), so this is the only moment they can be captured.
        cache.previous_sizes = tree.nodes.iter().map(|n| n.used_size).collect();
        let mut r = cache::ReconciliationResult::default();
        r.layout_roots.insert(tree.root);
        r.reused_nodes = tree.nodes.len();
        r.fresh_nodes = 0;
        (tree, r)
    } else {
        cache.last_reconcile_was_skipped = false;
        // DL-patching input for the STRUCTURE-PRESERVED case (a text edit):
        // the sizes the previous pass computed, captured before reconcile
        // consumes the old tree.
        if let Some(t) = cache.tree.as_ref() {
            cache.previous_sizes = t.nodes.iter().map(|n| n.used_size).collect();
        }
        let dom_diff_clean = cache.dom_diff_clean.take();
        cache::reconcile_and_invalidate(&mut ctx_temp, cache, viewport, dom_diff_clean)?
    };
    // The reuse census, persisted where a test can read it — see the field
    // docs on LayoutCache for why this pair is the ONLY external observable
    // that distinguishes "reused the warm tree" from "rebuilt it from
    // scratch" (both produce identical pixels).
    cache.last_reconcile_reused = recon_result.reused_nodes;
    cache.last_reconcile_fresh = recon_result.fresh_nodes;
    cache.last_fingerprint_skips = recon_result.fingerprint_skips;
    // Structure preserved = the same-shaped tree: full reuse+fresh
    // accounting at an UNCHANGED node count (content changes make their
    // node FRESH — a text edit — but assign the same indices in the same
    // traversal; adds/removes change the count and disqualify). Fresh nodes
    // are force-re-emitted by the DL patch below, so a same-count REPLACE is
    // sound too: its new content never splices stale items.
    cache.last_reconcile_structure_preserved = !cache.last_reconcile_was_skipped
        && recon_result.reused_nodes + recon_result.fresh_nodes == new_tree_val.nodes.len()
        && cache.previous_sizes.len() == new_tree_val.nodes.len();
    // [g56 FIX] Box the LayoutTree onto the HEAP. The lifted `&mut new_tree` passed to
    // calculate_intrinsic_sizes was mis-lifted (callee saw nodes.len()=0 while the caller saw 2)
    // because a stack/SROA'd `new_tree`'s address doesn't survive the cross-function lifted call
    // (taking `&new_tree` lifted to 0x0). A heap allocation has a stable absolute wasm address
    // that lifts reliably (cf. M8.4 "heap allocations work fine"). Deref coercion handles the
    // `&new_tree`/`&mut new_tree`/`new_tree.field` sites unchanged.
    let mut new_tree = Box::new(new_tree_val);
    {
        let _ = (0xDD00_0002u32);
    }
    // [az-diag g51 REVERT] 0x71 = reconcile_and_invalidate returned OK (no InvalidTree in reconcile).
    unsafe {
        crate::az_mark(0x60704_u32, (0x71u32));
    }
    // [az-diag g54 REVERT] 0x40740 = new_tree.nodes.len() RIGHT AFTER reconcile. If 0 → reconcile
    // built an empty LayoutTree (the bug is in reconcile_recursive/create_node_from_dom). If 2 →
    // reconcile is fine and the tree gets emptied/mis-lifted downstream (check 0x40744 at the loop).
    unsafe {
        crate::az_mark(0x60740_u32, (new_tree.nodes.len() as u32));
    }
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
    if let Some((cached_hash, cached_viewport, cached_gpu_fp, cached_dl)) =
        &cache.cached_display_list
    {
        let new_root_hash = new_tree
            .cold(LayoutNodeId::new(new_tree.root))
            .map(|c| c.subtree_hash);
        // The GPU-KEY-POPULATION fingerprint is the third key component. The
        // emitted list is a function of which nodes carry transform/opacity
        // keys (`PushReferenceFrame` exists exactly for keyed nodes), and
        // diff-driven animation mints its keys AFTER the first layout — so on
        // (hash, viewport) alone this hit served the PRE-KEY list back:
        // no reference frames, no GPU damage, a frozen animation. Population
        // only, not values: values change every tick, and serving this cached
        // list across ticks while values flow through the GPU channel is the
        // point of the design.
        let gpu_fp =
            gpu_value_cache.map_or(0, azul_core::gpu::GpuValueCache::dl_emission_fingerprint);
        if css_dirty.is_empty()
            && new_root_hash == Some(*cached_hash)
            && *cached_viewport == viewport
            && *cached_gpu_fp == gpu_fp
        {
            let _p = crate::probe::Probe::span("display_list_cache_hit");
            #[cfg(feature = "std")]
            if std::env::var_os("AZ_ANIM_DEBUG").is_some() {
                eprintln!(
                    "[dlcache] HIT fp={gpu_fp:x} items={}",
                    cached_dl.items.len()
                );
            }
            let dl = cached_dl.clone();
            // The served list is IDENTICAL to the previous frame — this is
            // NOT a splice. Left stale from the last real build, the
            // renderers' patched-build damage override would replay the old
            // patch rects on an idle frame (op_add_remove_timer step 30:
            // expected no damage, got rects).
            cache.last_build_was_patched = false;
            cache.last_patch_damage = None;
            return Ok(dl);
        }
    }

    // --- Step 1.15: fold the CSS DIFF into the solver's dirtiness ---
    //
    // Mirrors the reconcile classifier exactly (layout-affecting ->
    // intrinsic_dirty + layout_roots, the same pair a DirtyFlag::Layout node
    // gets). Paint-only entries (`RelayoutScope::None`) deliberately insert
    // NOTHING: with the Step 1.1 cache bypassed above, a clean reconcile
    // falls into the early-exit below, which reuses the whole tree and
    // re-emits the display list from the NEW styled_dom — repaint with zero
    // layout work. IfcOnly is treated as SizingOnly: over-invalidation is
    // correct (the IFC re-shapes inside the sizing pass); under-invalidation
    // is the 200px bug.
    if !css_dirty.is_empty() {
        let needs_layout_work = css_dirty
            .iter()
            .any(|(_, scope)| *scope != azul_css::props::property::RelayoutScope::None);
        if needs_layout_work {
            let mut dom_to_tree: HashMap<NodeId, usize> =
                HashMap::with_capacity(new_tree.nodes.len());
            for (idx, node) in new_tree.nodes.iter().enumerate() {
                if let Some(d) = node.dom_node_id {
                    dom_to_tree.insert(d, idx);
                }
            }
            for (dom_id_dirty, scope) in css_dirty {
                if *scope == azul_css::props::property::RelayoutScope::None {
                    continue;
                }
                if let Some(&idx) = dom_to_tree.get(dom_id_dirty) {
                    recon_result.intrinsic_dirty.insert(idx);
                    recon_result.layout_roots.insert(idx);
                }
            }
        }
    }

    // Step 1.2: Clear Taffy Caches for Dirty Nodes — and their ANCESTORS.
    //
    // The per-node `taffy_cache` persists across passes (it rides the warm
    // carry), keyed by taffy on (known_dimensions, available_space,
    // run_mode). Input changes miss by key on their own; what the key can
    // NOT see is a content change INSIDE the measured subtree — a deep text
    // edit changes what a flex item measures to at the SAME inputs. The
    // dirty node alone isn't enough: every ancestor's measure derives from
    // it, so the closure must be cleared (same propagation rule as
    // `calculate_intrinsic_sizes`' dirty_closure).
    //
    // This closure-clear is what made it safe to DELETE the unconditional
    // "clear every child before every flex layout" hammer in
    // `layout_taffy_subtree` (taffy_bridge.rs, from Nov 2025 — it predates
    // reconcile-driven dirtiness and was forcing 312 min/max-content
    // subtree re-layouts per pass on big.md, ~40% of a steady resize).
    {
        let closure =
            sizing::compute_dirty_ancestor_closure(&new_tree, &recon_result.intrinsic_dirty);
        for &node_idx in &closure {
            if let Some(warm) = new_tree.warm_mut(LayoutNodeId::new(node_idx)) {
                warm.taffy_cache.clear();
                warm.measured_content_sizes = (None, None);
            }
        }
    }

    // Step 1.3: Compute CSS Counters
    // This must be done after tree generation but before layout,
    // as list markers need counter values during formatting context layout
    if resize_only && !cache.counters.is_empty() {
        // CSS counters are a pure function of the DOM — on a resize-skip
        // pass the DOM is byte-identical, so last pass's values are THE
        // values (0.4 ms of ::marker recount per drag frame otherwise).
        counter_values.clone_from(&cache.counters);
    } else {
        let _p = crate::probe::Probe::span("compute_counters");
        cache::compute_counters(new_dom, &new_tree, &mut counter_values);
    }
    // [az-diag g51 REVERT] 0x72 = compute_counters done (InvalidTree, if any, is after here).
    unsafe {
        crate::az_mark(0x60704_u32, (0x72u32));
    }

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
    let probe_cache_remap = Some(crate::probe::Probe::span("cache_map_remap"));
    if let Some(old_tree) = cache.tree.as_ref() {
        let mut remapped = cache::LayoutCacheMap::default();
        remapped
            .entries
            .resize_with(new_tree.nodes.len(), Default::default);

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
                if old_layout_idx >= LayoutNodeId::new(cache_map.entries.len())
                    || new_layout_idx >= LayoutNodeId::new(remapped.entries.len())
                {
                    continue;
                }
                remapped.entries[new_layout_idx.index()] =
                    core::mem::take(&mut cache_map.entries[old_layout_idx.index()]);
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
        let mut new_to_old_layout_idx: HashMap<LayoutNodeId, LayoutNodeId> = HashMap::new();
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
            let Some(&old_parent_idx) =
                new_to_old_layout_idx.get(&LayoutNodeId::new(new_parent_idx))
            else {
                continue;
            };
            let Some(old_anon_children) = old_anon_by_parent.get(&old_parent_idx.index()) else {
                continue;
            };
            for (ord, &new_anon_idx) in new_anon_children.iter().enumerate() {
                let Some(&old_anon_idx) = old_anon_children.get(ord) else {
                    continue;
                };
                if old_anon_idx >= cache_map.entries.len() || new_anon_idx >= remapped.entries.len()
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
    drop(probe_cache_remap);
    crate::probe::sample_peak_rss("rss:after_cache_remap");
    for &node_idx in &recon_result.intrinsic_dirty {
        cache_map.mark_dirty(node_idx, &new_tree.nodes);
    }
    for &node_idx in &recon_result.layout_roots {
        cache_map.mark_dirty(node_idx, &new_tree.nodes);
    }

    // Now create the real context with computed counters
    let mut ctx = LayoutContext {
        style_cache: Default::default(),
        scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
        styled_dom: new_dom,
        font_manager,
        text_selections,
        debug_messages,
        counters: &mut counter_values,
        viewport_size: viewport.size,
        fragmentation_context: None,
        reflowed_ifcs: std::collections::BTreeSet::new(),
        cursor_is_visible,
        cursor_locations,
        preedit_text,
        cache_map, // Moved from LayoutCache; will be moved back after layout
        image_cache,
        content_overlay,
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
        // The reuse census must describe THIS pass: zero sizing work, full
        // tree reuse. Leaving the previous pass's numbers in place made a
        // paint-only repaint look like it charged the cold pass's layout
        // work (the census is load-bearing — tests assert on it).
        cache.last_intrinsic_dirty = 0;
        cache.last_reconcile_reused = recon_result.reused_nodes;
        let tree = cache.tree.as_ref().ok_or(LayoutError::InvalidTree)?;

        // Use cached scroll IDs if available, otherwise compute them
        let scroll_ids = if cache.scroll_ids.is_empty() {
            use crate::window::LayoutWindow;
            let (scroll_ids, scroll_id_to_node_id) =
                LayoutWindow::compute_scroll_ids(tree, new_dom);
            cache.scroll_ids.clone_from(&scroll_ids);
            cache.scroll_id_to_node_id = scroll_id_to_node_id;
            scroll_ids
        } else {
            cache.scroll_ids.clone()
        };

        if SKIP_DISPLAY_LIST.load(core::sync::atomic::Ordering::Relaxed) {
            cache.record_full_emission();
            return Ok(std::sync::Arc::new(DisplayList::default()));
        }
        let dl = generate_display_list(
            &mut ctx,
            tree,
            &cache.calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
        )
        .map(std::sync::Arc::new)?;
        // Refresh the structural-identity cache with what was JUST emitted.
        // This early exit re-emits precisely because something the cache key
        // could not see changed (a paint-only CSS diff, a GPU-key mint):
        // leaving the old entry in place would serve STALE PAINT on the next
        // call, and not storing at all would re-emit every frame for the
        // whole life of an animation.
        let root_hash = tree
            .cold(LayoutNodeId::new(tree.root))
            .map(|c| c.subtree_hash);
        let gpu_fp =
            gpu_value_cache.map_or(0, azul_core::gpu::GpuValueCache::dl_emission_fingerprint);
        if let Some(h) = root_hash {
            cache.cached_display_list = Some((h, viewport, gpu_fp, dl.clone()));
        }
        cache.last_dynamic_context = ctx
            .styled_dom
            .get_css_property_cache()
            .dynamic_context
            .as_deref()
            .cloned();
        // Full re-emit, not a splice — clear the patched-build flags so the
        // renderers' damage override cannot replay stale patch rects, and
        // retire the patch log: the renderers' item diff against the list
        // they last presented covers everything from here.
        cache.record_full_emission();
        return Ok(dl);
    }

    {
        let _ = (0xDD00_0003u32);
    }
    // [az-diag g51 REVERT] 0x80 = reached the incremental layout loop (past early-exit + remap + dirty loops).
    unsafe {
        crate::az_mark(0x60704_u32, (0x80u32));
    }
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
        crate::az_mark(
            0x607C4_u32,
            (cache.tree.as_ref().map_or(999, |t| t.nodes.len()) as u32),
        );
    }

    // --- Step 2: Incremental Layout Loop (handles scrollbar-induced reflows) ---
    let mut calculated_positions = cache.calculated_positions.clone();
    let mut loop_count = 0;
    loop {
        loop_count += 1;
        if loop_count > MAX_SCROLLBAR_REFLOW_ITERATIONS {
            debug_warning!(
                ctx,
                "Scrollbar reflow loop hit limit of {} iterations, breaking to avoid infinite loop",
                MAX_SCROLLBAR_REFLOW_ITERATIONS
            );
            break;
        }

        calculated_positions = {
            let _p = crate::probe::Probe::span("clone_calculated_positions");
            cache.calculated_positions.clone()
        };
        // [az-diag g70 RELIABLE free-band] 0x60780 = nodes.len AFTER the in-loop calculated_positions.clone().
        unsafe {
            crate::az_mark(0x60780_u32, (new_tree.nodes.len() as u32));
        }
        let mut reflow_needed_for_scrollbars = false;

        {
            crate::probe::reset_peak();
            // [az-diag g70 RELIABLE free-band] 0x60784 = nodes.len AFTER reset_peak (before the calc Span).
            unsafe {
                crate::az_mark(0x60784_u32, (new_tree.nodes.len() as u32));
            }
            let _p = crate::probe::Probe::span("calc_intrinsic_sizes");
            // [az-diag g70 RELIABLE free-band] 0x60788 = nodes.len AFTER the calc_intrinsic_sizes Span.
            unsafe {
                crate::az_mark(0x60788_u32, (new_tree.nodes.len() as u32));
            }
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
                crate::az_mark(
                    0x6074C_u32,
                    (cache.tree.as_ref().map_or(999, |t| t.nodes.len()) as u32),
                );
            }
            cache.last_intrinsic_dirty = recon_result.intrinsic_dirty.len();
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
        {
            let _ = (0xDD00_0005u32);
        }

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
            {
                let _ = (0xDD00_0053u32);
            }
            // get_containing_block_for_node(viewport)). 800 here but viewport=800 ⟹ OK;
            // 0 here with viewport=800 ⟹ get_containing_block_for_node lost it (HFA return).

            // For ROOT nodes (no parent), we need to account for their margin.
            // The containing block position from viewport is (0, 0), but the root's
            // content starts at (margin + border + padding, margin + border + padding).
            // We pass margin-adjusted position so calculate_content_box_pos works correctly.
            let root_node = &new_tree.nodes[root_idx];
            let root_bp = root_node.box_props.unpack();
            {
                let _ = (0xDD00_0054u32);
            }

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
            {
                let _ = (0xDD00_0056u32);
            }

            // DEBUG: Log containing block info for this root
            if let Some(debug_msgs) = ctx.debug_messages.as_mut() {
                let dom_name = root_node
                    .dom_node_id
                    .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                    .map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

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
            {
                let _ = (0xDD00_0055u32);
            }
            // This is exactly what calc_used_size reads as `viewport`. 0 here pinpoints the
            // loss to the ctx build (viewport.size → ctx.viewport_size copy).
            // 0x5E = Err. Do NOT propagate (continue to the cache store) so layout-real can
            // see whether the geometry was computed regardless of a (possibly spurious,
            // niche-Result-mis-discriminated) Err.
            let clr = {
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
            {
                let _ = (if clr.is_ok() {
                    0xDD00_0057u32
                } else {
                    0xDD00_005Eu32
                });
            }
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
                        .and_then(|id| new_dom.node_data.as_container().internal.get(id.index()))
                        .map_or_else(|| "Unknown".to_string(), |n| format!("{:?}", n.node_type));

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
        {
            let _ = (0xDD00_0006u32);
        }

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
            debug_log!(
                ctx,
                "Scrollbars changed container size, starting full reflow (loop {})",
                loop_count
            );
            recon_result.layout_roots.clear();
            recon_result.layout_roots.insert(new_tree.root);
            // Deliberately NOT touching intrinsic_dirty. A scrollbar toggle
            // changes AVAILABLE SPACE (container inner width shrinks/grows by
            // scrollbar_width); intrinsic min/max-content widths are CONTENT-
            // derived and cannot change because a scrollbar appeared. This
            // line used to mark EVERY node intrinsic-dirty
            // (`(0..nodes.len()).collect()`), so any resize that toggled a
            // scrollbar re-measured the whole document's intrinsics — 75 ms
            // of the measured 166 ms on big.md's first in-range resize, and
            // the same sledgehammer pattern as the old drop-the-tree-on-
            // viewport-change. Nodes whose intrinsics genuinely changed are
            // already in the set from reconciliation; the root layout_root
            // above re-resolves every SIZE against the new inner width.
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
    // (changed-node set, geometry-base rects) of a PATCHED build; consumed
    // after the final list exists to add the changed nodes' ITEM visual
    // bounds from both the old and the new list (shadow fringes).
    let mut patch_damage_pending: Option<(std::collections::BTreeSet<usize>, Vec<LogicalRect>)> =
        None;
    let display_list = if SKIP_DISPLAY_LIST.load(core::sync::atomic::Ordering::Relaxed) {
        // Web backend: positions are done; the painter is dead weight.
        DisplayList::default()
    } else {
        let _p = crate::probe::Probe::span("generate_display_list");
        // DL PATCHING (task 12 round 2): on a resize-skip pass the tree
        // object and its indices are unchanged, so the previous display
        // list's per-node item runs can substitute for the paint calls of
        // every node that neither re-flowed its inline content nor changed
        // size — their items just translate by the node's position delta.
        // `AZ_NO_DL_PATCH=1` is the runtime escape hatch (and the A/B lever
        // for the golden-equality test).
        let patch_disabled = !display_list::dl_patching_enabled();
        // Two ways in: the resize-skip pass (same tree object), or a
        // STRUCTURE-PRESERVED reconcile with no CSS dirt — a text edit
        // reflowed its IFC, everything else splices (per-IFC text patching,
        // USER mandate 2026-08-17). CSS-dirty passes must not splice: their
        // items changed paint without reflowing.
        // Cascade-context equality: an @media/theme/OS flip restyles REUSED
        // nodes without touching NodeData or css_dirty (the reconcile is
        // blind to the cascade), so the structure-preserved arm must also
        // prove the context the cached DL was built under is unchanged.
        // The resize-skip arm keeps its own contract (the harvested
        // breakpoints guarantee no answer flips between crossings).
        let cascade_ctx_unchanged = {
            let cur = ctx
                .styled_dom
                .get_css_property_cache()
                .dynamic_context
                .as_deref();
            #[cfg(feature = "std")]
            if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
                match (cur, cache.last_dynamic_context.as_ref()) {
                    (Some(a), Some(b)) if a != b => {
                        eprintln!(
                            "[CTXDIFF] vw={}/{} vh={}/{} theme={} media={} pseudo={} lang={} focus={}/{} os={} cont={}",
                            a.viewport_width, b.viewport_width,
                            a.viewport_height, b.viewport_height,
                            a.theme == b.theme, a.media_type == b.media_type,
                            a.pseudo_state == b.pseudo_state,
                            a.language == b.language,
                            a.window_focused, b.window_focused,
                            a.os == b.os && a.os_version == b.os_version && a.desktop_env == b.desktop_env,
                            a.container_width.to_bits() == b.container_width.to_bits()
                                && a.container_height.to_bits() == b.container_height.to_bits()
                                && a.container_name == b.container_name,
                        );
                    }
                    (a, b) if a.is_some() != b.is_some() => {
                        eprintln!(
                            "[CTXDIFF] presence cur={} stored={}",
                            a.is_some(),
                            b.is_some()
                        );
                    }
                    _ => {}
                }
            }
            match (cur, cache.last_dynamic_context.as_ref()) {
                (Some(a), Some(b)) => a == b,
                (None, None) => true,
                _ => false,
            }
        };
        let structure_ok = cache.last_reconcile_was_skipped
            || (cache.last_reconcile_structure_preserved
                && css_dirty.is_empty()
                && cascade_ctx_unchanged);
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            eprintln!(
                "[PATCHGATE] skipped={} preserved={} css_dirty={} structure_ok={} disabled={} \
                 prev_sizes={} nodes={} prev_pos={} pos={} ctx_same={} reflowed_ifcs={:?} fresh={:?}",
                cache.last_reconcile_was_skipped,
                cache.last_reconcile_structure_preserved,
                css_dirty.len(),
                structure_ok,
                patch_disabled,
                cache.previous_sizes.len(),
                new_tree.nodes.len(),
                cache.calculated_positions.len(),
                calculated_positions.len(),
                cascade_ctx_unchanged,
                ctx.reflowed_ifcs,
                recon_result.fresh_indices,
            );
        }
        // (previous_sizes vs NODES is intended: the cache keeps one entry
        // per node, so equal lengths mean the size snapshot covers the tree.)
        #[allow(clippy::suspicious_operation_groupings)]
        let patch = if structure_ok
            && !patch_disabled
            && cache.previous_sizes.len() == new_tree.nodes.len()
            && cache.calculated_positions.len() == calculated_positions.len()
        {
            cache
                .cached_display_list
                .as_ref()
                .and_then(|(_, _, _, prev_dl)| {
                    let new_sizes: Vec<Option<LogicalSize>> =
                        new_tree.nodes.iter().map(|n| n.used_size).collect();
                    // Fresh/content-changed nodes must RE-EMIT, never splice —
                    // their previous items describe content that no longer
                    // exists (the text-edit case, and same-count replaces). The
                    // intrinsic-dirty ANCESTOR chain is deliberately not here:
                    // an ancestor whose geometry did not change splices its
                    // unchanged items (size/position changes re-emit via
                    // PatchState's own size diff).
                    let mut reemit = ctx.reflowed_ifcs.clone();
                    reemit.extend(recon_result.fresh_indices.iter().copied());
                    display_list::PatchState::build(
                        prev_dl,
                        &cache.calculated_positions, // last pass's positions (replaced later)
                        &calculated_positions,
                        &cache.previous_sizes,
                        &new_sizes,
                        &reemit,
                    )
                })
        } else {
            None
        };
        // Field writes, not `cache.record_*()`: `patch` borrows
        // `cache.cached_display_list` for the rest of this build, and a
        // `&mut self` method call would conflict with it where disjoint
        // field writes do not. Same bookkeeping as `record_full_emission`.
        if patch.is_some() {
            // The damage is recorded below, once the final list exists.
            cache.last_build_was_patched = true;
            cache.last_patch_damage = None;
        } else {
            cache.build_seq = cache.build_seq.wrapping_add(1);
            cache.last_full_build_seq = cache.build_seq;
            cache.last_build_was_patched = false;
            cache.last_patch_damage = None;
            cache.patch_damage_log.clear();
        }
        if patch.is_some() {
            // The patch's own damage, phase 1 of 2: the CHANGED-node set
            // (re-emitted OR moved/resized) plus old ∪ new NODE bounds as the
            // geometry base (covers vacated areas and items with no bounds).
            // Phase 2 — after the final list exists — unions the changed
            // nodes' ITEM visual bounds from BOTH lists on top: paint like
            // box-shadow extends OUTSIDE the node box, so node rects alone
            // under-damage a shadow fringe.
            let mut rects: Vec<LogicalRect> = Vec::new();
            let mut changed: std::collections::BTreeSet<usize> = ctx.reflowed_ifcs.clone();
            changed.extend(recon_result.fresh_indices.iter().copied());
            for (idx, node) in new_tree.nodes.iter().enumerate() {
                let old_pos = pos_get(&cache.calculated_positions, idx);
                let new_pos = pos_get(&calculated_positions, idx);
                let old_size = cache.previous_sizes.get(idx).copied().flatten();
                let new_size = node.used_size;
                if !(changed.contains(&idx) || old_pos != new_pos || old_size != new_size) {
                    continue;
                }
                changed.insert(idx);
                if let (Some(p), Some(sz)) = (old_pos, old_size) {
                    rects.push(LogicalRect::new(p, sz));
                }
                if let (Some(p), Some(sz)) = (new_pos, new_size) {
                    rects.push(LogicalRect::new(p, sz));
                }
            }
            patch_damage_pending = Some((changed, rects));
        }
        if patch.is_some() {
            drop(crate::probe::Probe::span("dl_patched_pass"));
            // Round-3 presentation hint from the SAME inputs the patch used.
            // The re-emit set must match PatchState's exactly: reflowed IFCs
            // PLUS size-changed nodes — a size-changed node re-emitted its
            // items, so blitting its old pixels without repainting is stale.
            let new_sizes: Vec<Option<LogicalSize>> =
                new_tree.nodes.iter().map(|n| n.used_size).collect();
            let mut reemit_full = ctx.reflowed_ifcs.clone();
            reemit_full.extend(recon_result.fresh_indices.iter().copied());
            for (i, (prev, new)) in cache
                .previous_sizes
                .iter()
                .zip(new_sizes.iter())
                .enumerate()
            {
                if prev != new {
                    reemit_full.insert(i);
                }
            }
            let parents: Vec<Option<usize>> = new_tree.nodes.iter().map(|n| n.parent).collect();
            let opaque_bg: Vec<bool> = (0..new_tree.nodes.len())
                .map(|i| {
                    new_tree
                        .get(LayoutNodeId::new(i))
                        .and_then(|node| node.dom_node_id)
                        .is_some_and(|dom_id| {
                            let state = &ctx.styled_dom.styled_nodes.as_container()[dom_id]
                                .styled_node_state;
                            getters::get_background_color(ctx.styled_dom, dom_id, state).a == 255
                        })
                })
                .collect();
            cache.last_patch_move = display_list::compute_patch_move_summary(
                &cache.calculated_positions,
                &calculated_positions,
                &cache.previous_sizes,
                &new_sizes,
                &reemit_full,
                &parents,
                &opaque_bg,
            );
        } else {
            cache.last_patch_move = None;
        }
        display_list::generate_display_list_impl(
            &mut ctx,
            &new_tree,
            &calculated_positions,
            scroll_offsets,
            &scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
            patch,
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
        .cold(LayoutNodeId::new(new_tree.root))
        .map_or(layout_tree::SubtreeHash(0), |c| c.subtree_hash);
    // The DL is shared, not copied: one allocation serves the cache slot, the
    // caller's `DomLayoutResult`, and any virtual-view snapshot maps. Rare
    // post-build patches (`patch_node_image` / `patch_text_glyphs` / the
    // virtual-view placeholder swap) go through `Arc::make_mut`.
    let display_list = std::sync::Arc::new(display_list);
    // Record the population fingerprint of the SAME GpuValueCache the emission
    // above consumed (the &-param is immutable for the whole pass, so entry
    // fingerprint == emission fingerprint). Keys minted after this call — the
    // scrollbar registration in the caller, animation seeding — change the
    // manager's cache, the next call's fingerprint differs, and the cache
    // correctly misses once and re-seeds.
    let gpu_fp = gpu_value_cache.map_or(0, azul_core::gpu::GpuValueCache::dl_emission_fingerprint);
    #[cfg(feature = "std")]
    if std::env::var_os("AZ_ANIM_DEBUG").is_some() {
        eprintln!(
            "[dlcache] STORE fp={gpu_fp:x} items={}",
            display_list.items.len()
        );
    }
    // Patched-build damage, phase 2: union the changed nodes' ITEM visual
    // bounds from the OLD list (still in the cache slot here) and the NEW
    // one on top of the geometry base. `visual_bounds()` includes paint
    // that extends outside the node box (box-shadow fringes) — the same
    // bounds the item diff damages with.
    if let Some((changed, mut rects)) = patch_damage_pending.take() {
        {
            let mut add_items = |dl: &DisplayList| {
                for (i, m) in dl.layout_node_mapping.iter().enumerate() {
                    if let Some((idx, _)) = m {
                        if changed.contains(idx) {
                            if let Some(b) = dl.items[i].visual_bounds() {
                                rects.push(b);
                            }
                        }
                    }
                }
            };
            if let Some((_, _, _, old_dl)) = cache.cached_display_list.as_ref() {
                add_items(old_dl);
            }
            add_items(&display_list);
        }
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PATCH_DEBUG").is_some() {
            eprintln!("[PATCHDMG] rects={rects:?}");
        }
        // Onto the LOG, not just the slot: a second patched build before the
        // next present (a callback's css patch followed by the RefreshDom it
        // returns) must not erase this one's vacated rects. Field writes for
        // the same borrow reason as above; same bookkeeping as
        // `record_patch_damage`.
        cache.build_seq = cache.build_seq.wrapping_add(1);
        cache
            .patch_damage_log
            .push((cache.build_seq, rects.clone()));
        if cache.patch_damage_log.len() > cache::PATCH_DAMAGE_LOG_CAP {
            let excess = cache.patch_damage_log.len() - cache::PATCH_DAMAGE_LOG_CAP;
            cache.patch_damage_log.drain(..excess);
        }
        cache.last_patch_damage = Some(rects);
    }
    // The context this build was made under — the structure-preserved patch
    // arm compares the NEXT pass's context against it. Clone only on CHANGE:
    // the steady state (every keystroke) is an equal context, and the clone
    // allocates (AzString language, container name).
    {
        let cur = ctx
            .styled_dom
            .get_css_property_cache()
            .dynamic_context
            .as_deref();
        let same = match (cur, cache.last_dynamic_context.as_ref()) {
            (Some(a), Some(b)) => a == b,
            (None, None) => true,
            _ => false,
        };
        if !same {
            cache.last_dynamic_context = cur.cloned();
        }
    }
    cache.cached_display_list = Some((root_subtree_hash, viewport, gpu_fp, display_list.clone()));

    cache.tree = Some(*new_tree); // [g56] unbox the heap LayoutTree back into the cache
    cache.previous_positions =
        std::mem::replace(&mut cache.calculated_positions, calculated_positions);
    cache.viewport = Some(viewport);
    cache.scroll_ids = scroll_ids;
    cache.scroll_id_to_node_id = scroll_id_to_node_id;
    // + calculated_positions.len in the low bits. If step stays 3, it diverged earlier.
    {
        let _ = (0xDD00_0004u32 | ((cache.calculated_positions.len() as u32 & 0xfff) << 4));
    }
    let _doc_tail_span = crate::probe::Probe::span("doc_tail_writeback");
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
    if let Some(parent_idx) = tree.get(LayoutNodeId::new(node_idx)).and_then(|n| n.parent) {
        if let Some(parent_node) = tree.get(LayoutNodeId::new(parent_idx)) {
            let pos = pos_get(calculated_positions, parent_idx).unwrap_or(viewport.origin);
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
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use azul_core::dom::{Dom, FormattingContext};

    use super::*;
    use crate::solver3::{
        geometry::{EdgeSizes, PackedBoxProps, ResolvedBoxProps},
        layout_tree::{LayoutNodeCold, LayoutNodeHot, LayoutNodeWarm},
    };

    // ==================================================================
    // Fixtures
    // ==================================================================

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition::new(x, y)
    }

    fn size(w: f32, h: f32) -> LogicalSize {
        LogicalSize::new(w, h)
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect {
            origin: pos(x, y),
            size: size(w, h),
        }
    }

    fn edges(top: f32, right: f32, bottom: f32, left: f32) -> EdgeSizes {
        EdgeSizes {
            top,
            right,
            bottom,
            left,
        }
    }

    /// `ResolvedBoxProps` with the given padding + border and no margin.
    fn bp(padding: EdgeSizes, border: EdgeSizes) -> ResolvedBoxProps {
        ResolvedBoxProps {
            margin: EdgeSizes::default(),
            padding,
            border,
            ..ResolvedBoxProps::default()
        }
    }

    fn hot(
        parent: Option<usize>,
        dom_node_id: Option<NodeId>,
        used_size: Option<LogicalSize>,
        props: &ResolvedBoxProps,
    ) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps::pack(props),
            dom_node_id,
            used_size,
            formatting_context: FormattingContext::default(),
            parent,
        }
    }

    /// A `LayoutTree` carrying only what `get_containing_block_for_node` reads:
    /// hot nodes (parent link, box props, `used_size`, `dom_node_id`).
    fn tree_of(nodes: Vec<LayoutNodeHot>) -> LayoutTree {
        let n = nodes.len();
        LayoutTree {
            nodes,
            warm: vec![LayoutNodeWarm::default(); n],
            cold: vec![LayoutNodeCold::default(); n],
            root: 0,
            dom_to_layout: BTreeMap::new(),
            children_arena: Vec::new(),
            children_offsets: vec![(0, 0); n],
            subtree_needs_intrinsic: vec![false; n],
        }
    }

    /// Box props survive a lossy i16×10 pack/unpack, so geometry derived from
    /// them is compared with a tolerance well under a tenth of a pixel.
    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    /// `body` — one real DOM node, so `NodeId::ZERO` is always in range.
    fn body_dom() -> StyledDom {
        let mut dom = Dom::create_body();
        let (css, _warnings) = azul_css::parser2::new_from_str("");
        StyledDom::create(&mut dom, css)
    }

    // ==================================================================
    // POSITION_UNSET — the sentinel the three pos_* helpers are built on
    // ==================================================================

    #[test]
    fn position_unset_sets_both_components_to_f32_min() {
        // `pos_get`/`pos_contains` only test `x`; that shortcut is only sound
        // while the sentinel writes BOTH components. Pin the invariant here.
        assert_eq!(POSITION_UNSET.x, f32::MIN);
        assert_eq!(POSITION_UNSET.y, f32::MIN);
    }

    // ==================================================================
    // pos_get / pos_contains — bounds + sentinel (numeric)
    // ==================================================================

    #[test]
    fn pos_get_on_an_empty_vec_is_none_for_every_index() {
        let positions: PositionVec = Vec::new();
        for idx in [0usize, 1, 7, 1_000, usize::MAX / 2, usize::MAX] {
            assert!(pos_get(&positions, idx).is_none(), "idx {idx}");
            assert!(!pos_contains(&positions, idx), "idx {idx}");
        }
    }

    #[test]
    fn pos_get_past_the_end_is_none_and_never_panics() {
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 3, pos(1.0, 2.0));
        assert_eq!(positions.len(), 4);

        for idx in [4usize, 5, 100, usize::MAX] {
            assert!(pos_get(&positions, idx).is_none(), "idx {idx}");
            assert!(!pos_contains(&positions, idx), "idx {idx}");
        }
    }

    #[test]
    fn pos_get_at_zero_round_trips() {
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, 0.0));

        let got = pos_get(&positions, 0).expect("index 0 was set");
        assert_eq!(got.x, 0.0);
        assert_eq!(got.y, 0.0);
        assert!(pos_contains(&positions, 0));
    }

    #[test]
    fn an_explicitly_written_sentinel_reads_back_as_unset() {
        // Writing POSITION_UNSET is indistinguishable from never writing at all.
        // That is by design (it's how `pos_set`'s gap-fill works), but it means a
        // caller can never store the sentinel as a real position.
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, POSITION_UNSET);

        assert_eq!(positions.len(), 1);
        assert!(pos_get(&positions, 0).is_none());
        assert!(!pos_contains(&positions, 0));
    }

    #[test]
    fn a_position_whose_x_is_f32_min_reads_back_as_unset_even_with_a_real_y() {
        // Only `x` is compared against the sentinel, so `y` is silently discarded
        // whenever `x` happens to land exactly on f32::MIN.
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(f32::MIN, 42.0));

        assert!(pos_get(&positions, 0).is_none());
        assert!(!pos_contains(&positions, 0));
        // ...even though the value really is in the vec.
        assert_eq!(positions[0].y, 42.0);
    }

    #[test]
    fn negative_f32_max_is_the_same_bit_pattern_as_the_sentinel() {
        // f32::MIN == -f32::MAX, so a genuinely computed x of -f32::MAX (e.g. a
        // wildly out-of-flow element) is swallowed by the sentinel check.
        assert_eq!(f32::MIN, -f32::MAX);

        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(-f32::MAX, 0.0));
        assert!(pos_get(&positions, 0).is_none());
    }

    #[test]
    fn a_position_whose_y_is_f32_min_is_still_reported_as_set() {
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, f32::MIN));

        let got = pos_get(&positions, 0).expect("x is not the sentinel, so it is set");
        assert_eq!(got.x, 0.0);
        assert_eq!(got.y, f32::MIN);
        assert!(pos_contains(&positions, 0));
    }

    #[test]
    fn nan_positions_are_considered_set_and_survive_the_round_trip() {
        // NaN != f32::MIN is true, so a NaN position is "set" — it propagates into
        // layout rather than being filtered out as unset.
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(f32::NAN, f32::NAN));

        let got = pos_get(&positions, 0).expect("NaN passes the sentinel filter");
        assert!(got.x.is_nan());
        assert!(got.y.is_nan());
        assert!(pos_contains(&positions, 0));
    }

    #[test]
    fn infinite_and_extreme_positions_round_trip_unchanged() {
        let cases = [
            pos(f32::INFINITY, f32::NEG_INFINITY),
            pos(f32::MAX, -f32::MIN_POSITIVE),
            pos(-0.0, 0.0),
            pos(-1e30, 1e30),
            pos(f32::MIN_POSITIVE, f32::EPSILON),
        ];
        for (idx, case) in cases.iter().enumerate() {
            let mut positions: PositionVec = Vec::new();
            pos_set(&mut positions, idx, *case);

            let got = pos_get(&positions, idx).expect("non-sentinel x is set");
            assert_eq!(got.x.to_bits(), case.x.to_bits(), "case {idx} x");
            assert_eq!(got.y.to_bits(), case.y.to_bits(), "case {idx} y");
            assert!(pos_contains(&positions, idx), "case {idx}");
        }
    }

    #[test]
    fn pos_contains_always_agrees_with_pos_get() {
        // The predicate and the getter must never disagree — a divergence would
        // make `pos_get(..).unwrap()` guarded by `pos_contains` panic.
        let values = [
            pos(0.0, 0.0),
            pos(-0.0, -0.0),
            POSITION_UNSET,
            pos(f32::MIN, 1.0),
            pos(1.0, f32::MIN),
            pos(f32::NAN, 0.0),
            pos(f32::INFINITY, f32::INFINITY),
            pos(f32::NEG_INFINITY, 0.0),
            pos(f32::MAX, f32::MIN_POSITIVE),
            pos(-f32::MAX, 0.0),
        ];
        let mut positions: PositionVec = Vec::new();
        for (idx, v) in values.iter().enumerate() {
            pos_set(&mut positions, idx, *v);
        }
        for idx in 0..values.len() + 4 {
            assert_eq!(
                pos_contains(&positions, idx),
                pos_get(&positions, idx).is_some(),
                "idx {idx} disagrees"
            );
        }
    }

    // ==================================================================
    // pos_set — growth semantics (numeric)
    // ==================================================================

    #[test]
    fn pos_set_beyond_the_end_grows_and_fills_the_gap_with_the_sentinel() {
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 3, pos(10.0, 20.0));

        assert_eq!(positions.len(), 4, "grows to exactly idx + 1");
        for idx in 0..3 {
            assert!(
                pos_get(&positions, idx).is_none(),
                "gap idx {idx} must be unset"
            );
            assert!(!pos_contains(&positions, idx), "gap idx {idx}");
            assert_eq!(positions[idx].x, POSITION_UNSET.x);
            assert_eq!(positions[idx].y, POSITION_UNSET.y);
        }
        let got = pos_get(&positions, 3).expect("idx 3 was set");
        assert_eq!(got.x, 10.0);
        assert_eq!(got.y, 20.0);
    }

    #[test]
    fn pos_set_inside_the_vec_neither_grows_nor_shrinks_it() {
        let mut positions: PositionVec = vec![POSITION_UNSET; 5];
        pos_set(&mut positions, 0, pos(1.0, 1.0));
        assert_eq!(positions.len(), 5);

        pos_set(&mut positions, 4, pos(2.0, 2.0));
        assert_eq!(positions.len(), 5);
        assert!(pos_contains(&positions, 0));
        assert!(pos_contains(&positions, 4));
        assert!(!pos_contains(&positions, 2), "untouched slots stay unset");
    }

    #[test]
    fn pos_set_overwrites_an_existing_entry_in_place() {
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 1, pos(1.0, 1.0));
        pos_set(&mut positions, 1, pos(-5.5, -6.5));

        assert_eq!(positions.len(), 2);
        let got = pos_get(&positions, 1).expect("still set");
        assert_eq!(got.x, -5.5);
        assert_eq!(got.y, -6.5);
    }

    #[test]
    fn pos_set_can_reset_an_entry_back_to_unset() {
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(3.0, 4.0));
        assert!(pos_contains(&positions, 0));

        pos_set(&mut positions, 0, POSITION_UNSET);
        assert!(!pos_contains(&positions, 0));
        assert!(pos_get(&positions, 0).is_none());
    }

    #[test]
    fn repeated_growth_preserves_every_earlier_entry() {
        let mut positions: PositionVec = Vec::new();
        for idx in (0..64).rev() {
            // Descending order: the first call allocates the whole vec, the rest
            // write inside it — the reverse of the ascending growth path.
            pos_set(&mut positions, idx, pos(idx as f32, -(idx as f32)));
        }
        assert_eq!(positions.len(), 64);
        for idx in 0..64 {
            let got = pos_get(&positions, idx).expect("all 64 were written");
            assert_eq!(got.x, idx as f32);
            assert_eq!(got.y, -(idx as f32));
        }

        // A single jump far past the end must keep everything already written.
        pos_set(&mut positions, 4_095, pos(1.0, 1.0));
        assert_eq!(positions.len(), 4_096);
        for idx in 0..64 {
            assert!(pos_contains(&positions, idx), "idx {idx} lost after regrow");
        }
        for idx in 64..4_095 {
            assert!(
                !pos_contains(&positions, idx),
                "new slot {idx} must be unset"
            );
        }
        assert!(pos_contains(&positions, 4_095));
    }

    // ==================================================================
    // MAX_SCROLLBAR_REFLOW_ITERATIONS — the anti-infinite-loop bound
    // ==================================================================

    #[test]
    fn the_scrollbar_reflow_bound_is_a_usable_positive_limit() {
        // `loop_count > MAX` is the only thing standing between a pathological
        // scrollbar oscillation and a hung frame. 0 would mean "never lay out".
        const _: () = assert!(
            MAX_SCROLLBAR_REFLOW_ITERATIONS >= 1 && MAX_SCROLLBAR_REFLOW_ITERATIONS <= 64,
            "the scrollbar reflow bound must be a usable positive limit; an absurd bound = a hung frame"
        );
    }

    // ==================================================================
    // get_containing_block_for_node (numeric, private)
    // ==================================================================

    #[test]
    fn a_root_node_gets_the_viewport_as_its_containing_block() {
        let dom = body_dom();
        let tree = tree_of(vec![hot(
            None,
            Some(NodeId::ZERO),
            Some(size(10.0, 10.0)),
            &ResolvedBoxProps::default(),
        )]);
        let viewport = rect(7.0, 9.0, 800.0, 600.0);

        let (cb_pos, cb_size) =
            get_containing_block_for_node(&tree, &dom, 0, &Vec::new(), viewport);

        assert_eq!(cb_pos.x, 7.0);
        assert_eq!(cb_pos.y, 9.0);
        assert_eq!(cb_size.width, 800.0);
        assert_eq!(cb_size.height, 600.0);
    }

    #[test]
    fn an_out_of_range_node_index_falls_back_to_the_viewport() {
        let dom = body_dom();
        let tree = tree_of(vec![hot(
            None,
            None,
            Some(size(10.0, 10.0)),
            &ResolvedBoxProps::default(),
        )]);
        let viewport = rect(0.0, 0.0, 800.0, 600.0);

        for idx in [1usize, 99, usize::MAX] {
            let (cb_pos, cb_size) =
                get_containing_block_for_node(&tree, &dom, idx, &Vec::new(), viewport);
            assert_eq!(cb_pos.x, 0.0, "idx {idx}");
            assert_eq!(cb_size.width, 800.0, "idx {idx}");
            assert_eq!(cb_size.height, 600.0, "idx {idx}");
        }
    }

    #[test]
    fn a_dangling_parent_index_falls_back_to_the_viewport() {
        // Node 1 claims parent 999, which does not exist. The function must take
        // the viewport branch rather than index out of bounds.
        let dom = body_dom();
        let tree = tree_of(vec![
            hot(
                None,
                None,
                Some(size(10.0, 10.0)),
                &ResolvedBoxProps::default(),
            ),
            hot(
                Some(999),
                None,
                Some(size(10.0, 10.0)),
                &ResolvedBoxProps::default(),
            ),
        ]);
        let viewport = rect(1.0, 2.0, 300.0, 400.0);

        let (cb_pos, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &Vec::new(), viewport);
        assert_eq!(cb_pos.x, 1.0);
        assert_eq!(cb_pos.y, 2.0);
        assert_eq!(cb_size.width, 300.0);
        assert_eq!(cb_size.height, 400.0);
    }

    #[test]
    fn a_dom_backed_parent_shrinks_the_containing_block_by_border_and_padding() {
        let dom = body_dom();
        let parent_props = bp(edges(10.0, 10.0, 10.0, 10.0), edges(5.0, 5.0, 5.0, 5.0));
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(200.0, 100.0)),
                &parent_props,
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(30.0, 40.0));

        let (cb_pos, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));

        // content origin = margin-box pos + border + padding
        assert!(close(cb_pos.x, 45.0), "x was {}", cb_pos.x);
        assert!(close(cb_pos.y, 55.0), "y was {}", cb_pos.y);
        // content size = border-box - (border + padding) on both sides
        assert!(close(cb_size.width, 170.0), "width was {}", cb_size.width);
        assert!(close(cb_size.height, 70.0), "height was {}", cb_size.height);
    }

    #[test]
    fn an_anonymous_parent_offsets_the_origin_but_keeps_the_border_box_size() {
        // The `dom_node_id == None` arm returns `used_size` verbatim — it shifts the
        // origin inward by border+padding but does NOT shrink the size, unlike the
        // DOM-backed arm above. Asserted as-is so any future fix trips this test.
        let dom = body_dom();
        let parent_props = bp(edges(10.0, 10.0, 10.0, 10.0), edges(5.0, 5.0, 5.0, 5.0));
        let tree = tree_of(vec![
            hot(None, None, Some(size(200.0, 100.0)), &parent_props),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, 0.0));

        let (cb_pos, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));

        assert!(close(cb_pos.x, 15.0), "x was {}", cb_pos.x);
        assert!(close(cb_pos.y, 15.0), "y was {}", cb_pos.y);
        assert_eq!(
            cb_size.width, 200.0,
            "anonymous arm does not subtract padding/border"
        );
        assert_eq!(cb_size.height, 100.0);
    }

    #[test]
    fn an_unpositioned_parent_falls_back_to_the_viewport_origin() {
        let dom = body_dom();
        let parent_props = bp(edges(1.0, 0.0, 0.0, 2.0), edges(3.0, 0.0, 0.0, 4.0));
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(200.0, 100.0)),
                &parent_props,
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let viewport = rect(100.0, 200.0, 800.0, 600.0);

        // `calculated_positions` is empty: the parent has no computed position yet.
        let (cb_pos, _) = get_containing_block_for_node(&tree, &dom, 1, &Vec::new(), viewport);

        // viewport origin + border.left + padding.left, and the same on the y axis.
        assert!(close(cb_pos.x, 106.0), "x was {}", cb_pos.x);
        assert!(close(cb_pos.y, 204.0), "y was {}", cb_pos.y);
    }

    #[test]
    fn a_parent_with_a_sentinel_position_is_treated_as_unpositioned() {
        // pos_get filters the sentinel, so a parent whose stored x is f32::MIN
        // resolves against the viewport origin instead.
        let dom = body_dom();
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(200.0, 100.0)),
                &ResolvedBoxProps::default(),
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, POSITION_UNSET);

        let (cb_pos, _) = get_containing_block_for_node(
            &tree,
            &dom,
            1,
            &positions,
            rect(11.0, 13.0, 800.0, 600.0),
        );
        assert_eq!(cb_pos.x, 11.0);
        assert_eq!(cb_pos.y, 13.0);
    }

    #[test]
    fn a_parent_without_a_used_size_yields_a_zero_sized_containing_block() {
        let dom = body_dom();
        let tree = tree_of(vec![
            hot(None, Some(NodeId::ZERO), None, &ResolvedBoxProps::default()),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, 0.0));

        let (_, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));
        assert_eq!(cb_size.width, 0.0);
        assert_eq!(cb_size.height, 0.0);
    }

    #[test]
    fn huge_parent_padding_saturates_instead_of_wrapping_the_origin() {
        // PackedBoxProps stores edges as i16 tenths-of-a-pixel: 1e30px clamps to
        // +3276.7px. Wrapping would push the containing block's origin NEGATIVE.
        let dom = body_dom();
        let parent_props = bp(edges(1e30, 1e30, 1e30, 1e30), edges(1e30, 1e30, 1e30, 1e30));
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(200.0, 100.0)),
                &parent_props,
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, 0.0));

        let (cb_pos, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));

        assert!(cb_pos.x.is_finite() && cb_pos.y.is_finite());
        assert!(
            cb_pos.x > 0.0,
            "saturated padding must stay positive, got {}",
            cb_pos.x
        );
        assert!(
            cb_pos.x <= 2.0 * 3276.7 + 1.0,
            "clamped to the i16 ×10 range"
        );
        // border + padding dwarf the border-box, so the content box floors at zero.
        assert_eq!(cb_size.width, 0.0);
        assert_eq!(cb_size.height, 0.0);
    }

    #[test]
    fn a_nan_parent_position_propagates_but_does_not_panic_or_corrupt_the_size() {
        let dom = body_dom();
        let parent_props = bp(edges(10.0, 10.0, 10.0, 10.0), EdgeSizes::default());
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(200.0, 100.0)),
                &parent_props,
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(f32::NAN, f32::NAN));

        let (cb_pos, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));

        assert!(
            cb_pos.x.is_nan() && cb_pos.y.is_nan(),
            "NaN flows through unchanged"
        );
        // The size path never touches the position, so it must stay clean.
        assert!(close(cb_size.width, 180.0), "width was {}", cb_size.width);
        assert!(close(cb_size.height, 80.0), "height was {}", cb_size.height);
    }

    #[test]
    fn a_nan_parent_used_size_floors_the_containing_block_at_zero() {
        // inner_size() ends in `.max(0.0)`, which discards NaN — the containing
        // block collapses to 0 rather than exporting NaN into sizing.
        let dom = body_dom();
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(f32::NAN, f32::NAN)),
                &ResolvedBoxProps::default(),
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, 0.0));

        let (_, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));
        assert!(!cb_size.width.is_nan() && !cb_size.height.is_nan());
        assert_eq!(cb_size.width, 0.0);
        assert_eq!(cb_size.height, 0.0);
    }

    #[test]
    fn an_infinite_parent_used_size_stays_infinite_rather_than_becoming_nan() {
        let dom = body_dom();
        let parent_props = bp(edges(10.0, 10.0, 10.0, 10.0), edges(5.0, 5.0, 5.0, 5.0));
        let tree = tree_of(vec![
            hot(
                None,
                Some(NodeId::ZERO),
                Some(size(f32::INFINITY, f32::INFINITY)),
                &parent_props,
            ),
            hot(Some(0), None, None, &ResolvedBoxProps::default()),
        ]);
        let mut positions: PositionVec = Vec::new();
        pos_set(&mut positions, 0, pos(0.0, 0.0));

        let (_, cb_size) =
            get_containing_block_for_node(&tree, &dom, 1, &positions, rect(0.0, 0.0, 800.0, 600.0));
        assert!(cb_size.width.is_infinite() && cb_size.width.is_sign_positive());
        assert!(cb_size.height.is_infinite() && cb_size.height.is_sign_positive());
    }

    #[test]
    fn degenerate_viewports_pass_through_the_root_arm_untouched() {
        // The root arm is a pure identity on the viewport — it neither clamps
        // negatives nor sanitises NaN. Pin that so callers know to pre-validate.
        let dom = body_dom();
        let tree = tree_of(vec![hot(None, None, None, &ResolvedBoxProps::default())]);

        let (p, s) =
            get_containing_block_for_node(&tree, &dom, 0, &Vec::new(), rect(0.0, 0.0, 0.0, 0.0));
        assert_eq!(s.width, 0.0);
        assert_eq!(s.height, 0.0);
        assert_eq!(p.x, 0.0);

        let (_, s) = get_containing_block_for_node(
            &tree,
            &dom,
            0,
            &Vec::new(),
            rect(0.0, 0.0, -800.0, -600.0),
        );
        assert_eq!(s.width, -800.0, "negative viewport is not clamped");

        let (p, s) = get_containing_block_for_node(
            &tree,
            &dom,
            0,
            &Vec::new(),
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
        );
        assert!(
            p.x.is_nan() && s.width.is_nan(),
            "NaN viewport is not sanitised"
        );

        let (_, s) = get_containing_block_for_node(
            &tree,
            &dom,
            0,
            &Vec::new(),
            rect(0.0, 0.0, f32::MAX, f32::MAX),
        );
        assert_eq!(s.width, f32::MAX);
    }

    // ==================================================================
    // LayoutError — Display (serializer)
    // ==================================================================

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn every_layout_error_variant_renders_a_distinct_non_empty_message() {
        let variants = [
            LayoutError::InvalidTree,
            LayoutError::SizingFailed,
            LayoutError::PositioningFailed,
            LayoutError::DisplayListFailed,
            LayoutError::Text(crate::font_traits::LayoutError::BidiError(
                "boom".to_string(),
            )),
        ];
        let rendered: Vec<String> = variants.iter().map(ToString::to_string).collect();

        for msg in &rendered {
            assert!(!msg.is_empty(), "empty Display output");
            assert!(!msg.trim().is_empty(), "whitespace-only Display output");
        }
        for i in 0..rendered.len() {
            for j in (i + 1)..rendered.len() {
                assert_ne!(
                    rendered[i], rendered[j],
                    "variants {i} and {j} render alike"
                );
            }
        }
    }

    #[test]
    fn layout_error_display_matches_the_documented_wording() {
        assert_eq!(LayoutError::InvalidTree.to_string(), "Invalid layout tree");
        assert_eq!(
            LayoutError::SizingFailed.to_string(),
            "Sizing calculation failed"
        );
        assert_eq!(
            LayoutError::PositioningFailed.to_string(),
            "Position calculation failed"
        );
        assert_eq!(
            LayoutError::DisplayListFailed.to_string(),
            "Display list generation failed"
        );
    }

    #[test]
    fn layout_error_display_ignores_width_and_precision_specifiers() {
        // `write!(f, "...")` bypasses the formatter's padding/truncation, so a
        // caller aligning errors in a table gets no alignment at all.
        let e = LayoutError::InvalidTree;
        assert_eq!(format!("{e:>60}"), "Invalid layout tree");
        assert_eq!(format!("{e:.3}"), "Invalid layout tree");
        assert_eq!(format!("{e:^5}"), "Invalid layout tree");
    }

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn the_text_variant_embeds_the_inner_error_and_survives_hostile_payloads() {
        let payloads = [
            String::new(),
            "\u{202e}rtl-override \u{0}nul".to_string(),
            "日本語のエラー 🎉".to_string(),
            "x".repeat(100_000),
            "\"quotes\" and \\backslashes\\".to_string(),
        ];
        for payload in payloads {
            let err = LayoutError::Text(crate::font_traits::LayoutError::ShapingError(
                payload.clone(),
            ));
            let msg = err.to_string();
            assert!(
                msg.starts_with("Text layout error: "),
                "unexpected prefix for {} byte payload",
                payload.len()
            );
            // The inner error is rendered with `{:?}`, so it is escaped, not raw —
            // but it must never be truncated away entirely.
            assert!(msg.len() >= "Text layout error: ".len() + payload.len());
        }
    }

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn the_text_variant_renders_a_default_font_selector_without_panicking() {
        let err = LayoutError::Text(crate::font_traits::LayoutError::FontNotFound(
            crate::font_traits::FontSelector::default(),
        ));
        let msg = err.to_string();
        assert!(msg.starts_with("Text layout error: "));
        assert!(
            msg.contains("serif"),
            "the default family should show up: {msg}"
        );
    }

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn from_text_layout_error_wraps_into_the_text_variant() {
        let inner = crate::font_traits::LayoutError::InvalidText("bad".to_string());
        let wrapped: LayoutError = inner.into();

        assert!(matches!(wrapped, LayoutError::Text(_)));
        assert!(wrapped.to_string().contains("bad"));

        // The `?` sugar used all over solver3 goes through the same From impl.
        fn propagates() -> Result<()> {
            let failed: std::result::Result<(), crate::font_traits::LayoutError> = Err(
                crate::font_traits::LayoutError::HyphenationError("nope".to_string()),
            );
            failed?;
            Ok(())
        }
        assert!(matches!(propagates(), Err(LayoutError::Text(_))));
    }

    #[test]
    fn layout_error_is_a_std_error_without_a_source() {
        use std::error::Error;
        let e = LayoutError::SizingFailed;
        assert!(e.source().is_none());
        // Debug must also be usable (it is what `Result::unwrap` prints).
        assert!(!format!("{e:?}").is_empty());
    }

    // ==================================================================
    // LayoutContext debug sinks + the lazy debug_* macros
    // ==================================================================

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    mod debug_sinks {
        use std::collections::{BTreeMap, HashMap};

        use azul_core::{dom::DomId, selection::TextSelection, styled_dom::StyledDom};
        use azul_css::{props::basic::FontRef, LayoutDebugMessage, LayoutDebugMessageType};

        use super::{body_dom, size};
        use crate::{
            font_traits::FontManager,
            solver3::{cache, LayoutContext},
        };

        /// Owns everything a `LayoutContext` borrows, so a test can build one,
        /// poke it, drop it, and then inspect the captured messages.
        struct Env {
            styled_dom: StyledDom,
            font_manager: FontManager<FontRef>,
            text_selections: BTreeMap<DomId, TextSelection>,
            counters: HashMap<(usize, String), i32>,
            image_cache: azul_core::resources::ImageCache,
            debug_messages: Option<Vec<LayoutDebugMessage>>,
        }

        impl Env {
            fn new(debug_messages: Option<Vec<LayoutDebugMessage>>) -> Self {
                Self {
                    styled_dom: body_dom(),
                    font_manager: FontManager::new(rust_fontconfig::FcFontCache::default())
                        .expect("FontManager over an empty font cache"),
                    text_selections: BTreeMap::new(),
                    counters: HashMap::new(),
                    image_cache: azul_core::resources::ImageCache::default(),
                    debug_messages,
                }
            }

            fn ctx(&mut self) -> LayoutContext<'_, FontRef> {
                LayoutContext {
                    style_cache: Default::default(),
                    scrollbar_style_cache: core::cell::RefCell::new(HashMap::new()),
                    styled_dom: &self.styled_dom,
                    font_manager: &self.font_manager,
                    text_selections: &self.text_selections,
                    debug_messages: &mut self.debug_messages,
                    counters: &mut self.counters,
                    viewport_size: size(800.0, 600.0),
                    fragmentation_context: None,
                    reflowed_ifcs: std::collections::BTreeSet::new(),
                    cursor_is_visible: true,
                    cursor_locations: Vec::new(),
                    preedit_text: None,
                    cache_map: cache::LayoutCacheMap::default(),
                    image_cache: &self.image_cache,
                    content_overlay: None,
                    system_style: None,
                    get_system_time_fn: azul_core::task::GetSystemTimeCallback {
                        cb: azul_core::task::get_system_time_libstd,
                    },
                }
            }
        }

        #[test]
        fn each_debug_sink_appends_exactly_one_message_of_its_own_type() {
            let mut env = Env::new(Some(Vec::new()));
            {
                let mut ctx = env.ctx();
                ctx.debug_log_inner("log".to_string());
                ctx.debug_info_inner("info".to_string());
                ctx.debug_warning_inner("warning".to_string());
                ctx.debug_error_inner("error".to_string());
                ctx.debug_box_props_inner("box_props".to_string());
                ctx.debug_css_getter_inner("css_getter".to_string());
                ctx.debug_bfc_layout_inner("bfc".to_string());
                ctx.debug_ifc_layout_inner("ifc".to_string());
                ctx.debug_table_layout_inner("table".to_string());
                ctx.debug_display_type_inner("display".to_string());
            }

            let msgs = env.debug_messages.expect("Some(vec) was passed in");
            let expected = [
                ("log", LayoutDebugMessageType::Info),
                ("info", LayoutDebugMessageType::Info),
                ("warning", LayoutDebugMessageType::Warning),
                ("error", LayoutDebugMessageType::Error),
                ("box_props", LayoutDebugMessageType::BoxProps),
                ("css_getter", LayoutDebugMessageType::CssGetter),
                ("bfc", LayoutDebugMessageType::BfcLayout),
                ("ifc", LayoutDebugMessageType::IfcLayout),
                ("table", LayoutDebugMessageType::TableLayout),
                ("display", LayoutDebugMessageType::DisplayType),
            ];
            assert_eq!(msgs.len(), expected.len(), "one message per call, in order");
            for (msg, (text, ty)) in msgs.iter().zip(expected) {
                assert_eq!(msg.message.as_str(), text);
                assert_eq!(msg.message_type, ty);
                assert!(
                    !msg.location.as_str().is_empty(),
                    "location must be recorded"
                );
            }
        }

        #[test]
        fn debug_log_inner_tags_the_message_with_the_solver3_location() {
            // `debug_log_inner` builds the message by hand (location = "solver3");
            // every other sink goes through LayoutDebugMessage::* (#[track_caller]
            // → a file:line inside this module).
            let mut env = Env::new(Some(Vec::new()));
            {
                let mut ctx = env.ctx();
                ctx.debug_log_inner("hello".to_string());
                ctx.debug_info_inner("hello".to_string());
            }

            let msgs = env.debug_messages.expect("Some(vec)");
            assert_eq!(msgs[0].location.as_str(), "solver3");
            assert!(
                msgs[1].location.as_str().contains(".rs:"),
                "track_caller location, got {:?}",
                msgs[1].location.as_str()
            );
        }

        #[test]
        fn the_debug_sinks_are_no_ops_when_debug_messages_is_none() {
            // The macros guard on `is_some()`, but the inner fns must be safe when
            // called directly (they are `pub`).
            let mut env = Env::new(None);
            {
                let mut ctx = env.ctx();
                ctx.debug_log_inner("log".to_string());
                ctx.debug_error_inner("error".to_string());
                ctx.debug_table_layout_inner("table".to_string());
            }
            assert!(env.debug_messages.is_none(), "must not materialise a Vec");
        }

        #[test]
        fn debug_messages_preserve_hostile_payloads_byte_for_byte() {
            let payloads = [
                String::new(),
                "\u{0}\u{7}\u{1b}[31m".to_string(),
                "日本語 🎉 \u{202e}reversed".to_string(),
                "line\nbreak\ttab\r\n".to_string(),
                "{}{{}} {:?} %s %n".to_string(), // format-string lookalikes
                "x".repeat(200_000),
            ];
            let mut env = Env::new(Some(Vec::new()));
            {
                let mut ctx = env.ctx();
                for p in &payloads {
                    ctx.debug_info_inner(p.clone());
                }
            }

            let msgs = env.debug_messages.expect("Some(vec)");
            assert_eq!(msgs.len(), payloads.len());
            for (msg, payload) in msgs.iter().zip(&payloads) {
                assert_eq!(msg.message.as_str(), payload.as_str());
            }
        }

        #[test]
        fn the_debug_macros_push_one_message_each_when_capturing() {
            let mut env = Env::new(Some(Vec::new()));
            {
                let mut ctx = env.ctx();
                debug_log!(ctx, "log {}", 1);
                debug_info!(ctx, "info {}", 2);
                debug_warning!(ctx, "warning {}", 3);
                debug_error!(ctx, "error {}", 4);
                debug_box_props!(ctx, "box_props {}", 5);
                debug_css_getter!(ctx, "css_getter {}", 6);
                debug_bfc_layout!(ctx, "bfc {}", 7);
                debug_ifc_layout!(ctx, "ifc {}", 8);
                debug_table_layout!(ctx, "table {}", 9);
                debug_display_type!(ctx, "display {}", 10);
            }

            let msgs = env.debug_messages.expect("Some(vec)");
            assert_eq!(msgs.len(), 10);
            assert_eq!(msgs[0].message.as_str(), "log 1");
            assert_eq!(msgs[9].message.as_str(), "display 10");
        }

        #[test]
        fn the_debug_macros_do_not_evaluate_their_arguments_when_not_capturing() {
            // This laziness is the whole point of the macros: a `format!` per node
            // per pass would dominate a release layout. A regression here is silent.
            let evaluations = core::cell::Cell::new(0u32);
            let bump = |c: &core::cell::Cell<u32>| {
                c.set(c.get() + 1);
                c.get()
            };

            let mut env = Env::new(None);
            {
                let mut ctx = env.ctx();
                debug_log!(ctx, "{}", bump(&evaluations));
                debug_info!(ctx, "{}", bump(&evaluations));
                debug_warning!(ctx, "{}", bump(&evaluations));
                debug_error!(ctx, "{}", bump(&evaluations));
                debug_box_props!(ctx, "{}", bump(&evaluations));
                debug_css_getter!(ctx, "{}", bump(&evaluations));
                debug_bfc_layout!(ctx, "{}", bump(&evaluations));
                debug_ifc_layout!(ctx, "{}", bump(&evaluations));
                debug_table_layout!(ctx, "{}", bump(&evaluations));
                debug_display_type!(ctx, "{}", bump(&evaluations));
            }
            assert_eq!(evaluations.get(), 0, "format args must stay unevaluated");

            // ...and they ARE evaluated (exactly once) when capturing.
            let mut env = Env::new(Some(Vec::new()));
            {
                let mut ctx = env.ctx();
                debug_log!(ctx, "{}", bump(&evaluations));
            }
            assert_eq!(evaluations.get(), 1);
            assert_eq!(env.debug_messages.as_ref().map(Vec::len), Some(1));
        }
    }

    // ==================================================================
    // set_skip_display_list — the web-backend opt-out flag
    // ==================================================================

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    #[test]
    fn set_skip_display_list_round_trips_and_is_idempotent() {
        use core::sync::atomic::Ordering;

        let previous = SKIP_DISPLAY_LIST.load(Ordering::Relaxed);

        set_skip_display_list(true);
        assert!(SKIP_DISPLAY_LIST.load(Ordering::Relaxed));
        set_skip_display_list(true);
        assert!(
            SKIP_DISPLAY_LIST.load(Ordering::Relaxed),
            "double-set is idempotent"
        );

        set_skip_display_list(false);
        assert!(!SKIP_DISPLAY_LIST.load(Ordering::Relaxed));

        // Restore whatever the process was using — other tests share this static.
        set_skip_display_list(previous);
        assert_eq!(SKIP_DISPLAY_LIST.load(Ordering::Relaxed), previous);
    }

    // ==================================================================
    // layout_document — the entry point, driven with degenerate viewports
    // ==================================================================

    #[cfg(all(feature = "text_layout", feature = "font_loading"))]
    mod document {
        use std::collections::BTreeMap;

        use azul_core::{
            dom::{Dom, DomId},
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            resources::RendererResources,
            styled_dom::StyledDom,
        };
        use azul_css::props::basic::FontRef;

        use crate::{
            font_traits::{FontManager, TextLayoutCache},
            solver3::{cache::LayoutCache, display_list::DisplayList, layout_document, Result},
        };

        fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
            LogicalRect {
                origin: LogicalPosition::new(x, y),
                size: LogicalSize::new(w, h),
            }
        }

        /// `body > div` — no text nodes, so an empty (font-less) `FontManager` is
        /// enough to exercise the whole reconcile → size → position → paint chain.
        fn simple_dom() -> StyledDom {
            let mut dom = Dom::create_body().with_child(Dom::create_div());
            let (css, _warnings) =
                azul_css::parser2::new_from_str("div { width: 50px; height: 20px; }");
            StyledDom::create(&mut dom, css)
        }

        fn run(
            cache: &mut LayoutCache,
            dom: &StyledDom,
            viewport: LogicalRect,
        ) -> Result<std::sync::Arc<DisplayList>> {
            let mut text_cache = TextLayoutCache::new();
            let font_manager: FontManager<FontRef> =
                FontManager::new(rust_fontconfig::FcFontCache::default())
                    .expect("FontManager over an empty font cache");
            let renderer_resources = RendererResources::default();
            let image_cache = azul_core::resources::ImageCache::default();
            let mut debug_messages = None;

            layout_document(
                cache,
                &mut text_cache,
                dom,
                viewport,
                &font_manager,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &mut debug_messages,
                None,
                &renderer_resources,
                azul_core::resources::IdNamespace(0),
                DomId::ROOT_ID,
                false,
                Vec::new(),
                None,
                &image_cache,
                None,
                None,
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
                &[],
            )
        }

        /// [`run`] with a caller-supplied `GpuValueCache` — for the tests that
        /// pin how the DL cache reacts to the KEY POPULATION changing.
        fn run_with_gpu(
            cache: &mut LayoutCache,
            dom: &StyledDom,
            viewport: LogicalRect,
            gpu: &azul_core::gpu::GpuValueCache,
        ) -> Result<std::sync::Arc<DisplayList>> {
            let mut text_cache = TextLayoutCache::new();
            let font_manager: FontManager<FontRef> =
                FontManager::new(rust_fontconfig::FcFontCache::default())
                    .expect("FontManager over an empty font cache");
            let renderer_resources = RendererResources::default();
            let image_cache = azul_core::resources::ImageCache::default();
            let mut debug_messages = None;

            layout_document(
                cache,
                &mut text_cache,
                dom,
                viewport,
                &font_manager,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &mut debug_messages,
                Some(gpu),
                &renderer_resources,
                azul_core::resources::IdNamespace(0),
                DomId::ROOT_ID,
                false,
                Vec::new(),
                None,
                &image_cache,
                None,
                None,
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
                &[],
            )
        }

        /// [`run`] with a caller-supplied CSS diff — the css_dirty channel.
        fn run_with_css(
            cache: &mut LayoutCache,
            dom: &StyledDom,
            viewport: LogicalRect,
            css_dirty: &[(
                azul_core::dom::NodeId,
                azul_css::props::property::RelayoutScope,
            )],
        ) -> Result<std::sync::Arc<DisplayList>> {
            let mut text_cache = TextLayoutCache::new();
            let font_manager: FontManager<FontRef> =
                FontManager::new(rust_fontconfig::FcFontCache::default())
                    .expect("FontManager over an empty font cache");
            let renderer_resources = RendererResources::default();
            let image_cache = azul_core::resources::ImageCache::default();
            let mut debug_messages = None;

            layout_document(
                cache,
                &mut text_cache,
                dom,
                viewport,
                &font_manager,
                &BTreeMap::new(),
                &BTreeMap::new(),
                &mut debug_messages,
                None,
                &renderer_resources,
                azul_core::resources::IdNamespace(0),
                DomId::ROOT_ID,
                false,
                Vec::new(),
                None,
                &image_cache,
                None,
                None,
                azul_core::task::GetSystemTimeCallback {
                    cb: azul_core::task::get_system_time_libstd,
                },
                css_dirty,
            )
        }

        /// The AGGRESSIVE-CACHING CONTRACT of the CSS-diff channel, both
        /// directions:
        ///
        /// A PAINT-ONLY diff (`RelayoutScope::None` — colour, background)
        /// must bypass the structural-identity DL cache (fresh paint) while
        /// charging ZERO layout work — geometry byte-identical, no intrinsic
        /// recomputation. A SIZING diff must actually re-solve. And an EMPTY
        /// diff over the unchanged DOM must still hit the cache (the same
        /// Arc) — including right after a paint-only pass, which requires the
        /// early-exit path to have refreshed the cache with what it emitted,
        /// or the NEXT hit would serve the stale-paint list.
        #[test]
        fn css_diff_paint_repaints_without_layout_and_sizing_relayouts() {
            use azul_css::props::property::RelayoutScope;

            let dom = simple_dom();
            let mut cache = LayoutCache::default();
            let viewport = rect(0.0, 0.0, 800.0, 600.0);

            let Ok(first) = run(&mut cache, &dom, viewport) else {
                return; // font-less environment
            };
            let positions_before = cache.calculated_positions.clone();

            // Paint-only: fresh list (repainted), identical geometry, zero
            // sizing work.
            let div = azul_core::dom::NodeId::new(1);
            let second = run_with_css(&mut cache, &dom, viewport, &[(div, RelayoutScope::None)])
                .expect("paint-only pass");
            assert!(
                !std::sync::Arc::ptr_eq(&first, &second),
                "a paint-only diff must re-emit the display list (stale paint otherwise)"
            );
            assert_eq!(
                cache.last_intrinsic_dirty, 0,
                "a paint-only diff must charge ZERO sizing work"
            );
            assert_eq!(
                cache.calculated_positions, positions_before,
                "a paint-only diff must not move anything"
            );

            // Unchanged pass right after: must HIT (same Arc as the SECOND
            // list — the early-exit refreshed the cache with what it emitted).
            let third = run(&mut cache, &dom, viewport).expect("unchanged pass");
            assert!(
                std::sync::Arc::ptr_eq(&second, &third),
                "an unchanged pass after a paint-only repaint must hit the DL cache \
                 with the REPAINTED list, not re-emit or serve the stale one"
            );

            // Sizing: must actually re-solve.
            let fourth = run_with_css(
                &mut cache,
                &dom,
                viewport,
                &[(div, RelayoutScope::SizingOnly)],
            )
            .expect("sizing pass");
            assert!(
                !std::sync::Arc::ptr_eq(&third, &fourth),
                "a sizing diff must not serve the cached list"
            );
            assert!(
                cache.last_intrinsic_dirty >= 1,
                "a sizing diff must charge sizing work for the dirty node"
            );
        }

        /// The structural-identity DL cache must MISS when the GPU-KEY
        /// POPULATION changes, in BOTH directions.
        ///
        /// Diff-driven animation mints its transform keys AFTER the first
        /// layout of the new DOM (First/Last need solved rects), then the
        /// shell regenerates the display list. The very next relayout of the
        /// UNCHANGED DOM used to hit this cache on (root hash, viewport)
        /// alone and serve the PRE-KEY list back — no `PushReferenceFrame`,
        /// so no GPU damage, so a frozen animation and byte-identical
        /// screenshots from the first tick on. The reverse direction is the
        /// retirement leak: a settled animation retires its key, and a cache
        /// hit would keep a reference frame whose fallback matrix is the
        /// BAKED mid-flight transform.
        #[test]
        fn dl_cache_misses_when_the_gpu_key_population_changes() {
            use azul_core::gpu::GpuValueCache;
            use azul_core::resources::TransformKey;
            use azul_core::transform::ComputedTransform3D;

            let count_refframes = |dl: &DisplayList| {
                dl.items
                    .iter()
                    .filter(|i| {
                        matches!(
                            i,
                            crate::solver3::display_list::DisplayListItem::PushReferenceFrame { .. }
                        )
                    })
                    .count()
            };

            let dom = simple_dom();
            let mut cache = LayoutCache::default();
            let viewport = rect(0.0, 0.0, 800.0, 600.0);
            let empty_gpu = GpuValueCache::default();

            let Ok(first) = run_with_gpu(&mut cache, &dom, viewport, &empty_gpu) else {
                // Font-less environment — nothing to compare (same escape as
                // the sibling tests).
                return;
            };
            assert!(
                cache.cached_display_list.is_some(),
                "cold pass must seed the DL cache"
            );
            assert_eq!(count_refframes(&first), 0, "no keys, no reference frames");

            // A key is minted for a node — as animation seeding does, AFTER
            // this DOM has already been laid out and cached.
            let mut animated_gpu = GpuValueCache::default();
            let animated_node = azul_core::dom::NodeId::new(1); // the div
            animated_gpu
                .anim_transform_keys
                .insert(animated_node, TransformKey::unique());
            animated_gpu.anim_current_transform_values.insert(
                animated_node,
                ComputedTransform3D::new_translation(120.0, 0.0, 0.0),
            );

            let second = run_with_gpu(&mut cache, &dom, viewport, &animated_gpu)
                .expect("warm relayout with keys");
            assert_eq!(
                count_refframes(&second),
                1,
                "a freshly keyed node must get a reference frame — 0 means the \
                 cache served the pre-key display list back"
            );

            // Retirement: the key goes away, the frame must too. A stale hit
            // here would leave a reference frame falling back to its BAKED
            // (mid-flight) matrix — a node permanently offset by its last
            // sampled position.
            let third = run_with_gpu(&mut cache, &dom, viewport, &empty_gpu)
                .expect("warm relayout after retirement");
            assert_eq!(
                count_refframes(&third),
                0,
                "a retired key must not leave a stale reference frame"
            );

            // And the cache still WORKS when nothing changed: same population
            // twice in a row is a hit (same Arc, not merely equal contents).
            let fourth = run_with_gpu(&mut cache, &dom, viewport, &empty_gpu)
                .expect("warm relayout, unchanged population");
            assert!(
                std::sync::Arc::ptr_eq(&third, &fourth),
                "an unchanged population must still hit the cache"
            );
        }

        /// Regression: an EMPTY, unstyled div (the MicrophoneWidget pattern —
        /// zero height, auto width, only there to carry a dataset + AfterMount
        /// callback) must still receive a computed position. It used to stay at
        /// the `POSITION_UNSET` sentinel, and its Border/HitTestArea items were
        /// emitted at (f32::MIN, f32::MIN) and dropped by the display-list
        /// guard — harmless for paint, but the hit area silently vanished.
        #[test]
        fn an_empty_div_still_receives_a_position() {
            // Text siblings force anonymous-box wrapping around the inline
            // runs — the empty divs sit BETWEEN anonymous blocks, which is
            // the arrangement that used to skip them.
            let mut dom = Dom::create_body()
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "azul-self-test",
                ))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "probing platform APIs",
                ))
                .with_child(Dom::create_div())
                .with_child(Dom::create_div());
            let (css, _warnings) = azul_css::parser2::new_from_str("body { font-size: 14px; }");
            let dom = StyledDom::create(&mut dom, css);

            let mut cache = LayoutCache::default();
            if run(&mut cache, &dom, rect(0.0, 0.0, 800.0, 600.0)).is_err() {
                return; // font-less environment quirk — nothing to assert
            }
            let tree = cache.tree.as_ref().expect("tree must be cached");
            for (idx, node) in tree.nodes.iter().enumerate() {
                if node.dom_node_id.is_none() {
                    continue;
                }
                assert!(
                    super::pos_get(&cache.calculated_positions, idx).is_some(),
                    "layout node {idx} (dom {:?}, {} children) never received a position",
                    node.dom_node_id,
                    tree.children(idx).len(),
                );
            }
        }

        #[test]
        fn a_zero_sized_viewport_lays_out_without_panicking() {
            let dom = simple_dom();
            let mut cache = LayoutCache::default();

            // Err is acceptable (a font-less environment may legitimately fail);
            // a panic, an infinite scrollbar reflow, or a NaN position is not.
            if run(&mut cache, &dom, rect(0.0, 0.0, 0.0, 0.0)).is_ok() {
                for p in &cache.calculated_positions {
                    assert!(
                        !p.x.is_nan() && !p.y.is_nan(),
                        "NaN position from a 0×0 viewport"
                    );
                }
            }
        }

        #[test]
        fn degenerate_viewports_never_panic_or_hang() {
            // MAX_SCROLLBAR_REFLOW_ITERATIONS is the only guard against a reflow
            // oscillation, so each of these must terminate through it or earlier.
            let viewports = [
                rect(0.0, 0.0, f32::NAN, f32::NAN),
                rect(f32::NAN, f32::NAN, 800.0, 600.0),
                rect(0.0, 0.0, -800.0, -600.0),
                rect(0.0, 0.0, f32::MAX, f32::MAX),
                rect(0.0, 0.0, f32::INFINITY, f32::INFINITY),
                rect(-1e30, -1e30, 1.0, 1.0),
                rect(0.0, 0.0, f32::MIN_POSITIVE, f32::MIN_POSITIVE),
            ];
            for viewport in viewports {
                let dom = simple_dom();
                let mut cache = LayoutCache::default();
                let _ = run(&mut cache, &dom, viewport);
            }
        }

        #[test]
        fn laying_out_the_same_dom_twice_is_stable_and_populates_the_cache() {
            let dom = simple_dom();
            let mut cache = LayoutCache::default();
            let viewport = rect(0.0, 0.0, 800.0, 600.0);

            let first = run(&mut cache, &dom, viewport);
            if first.is_err() {
                // No fonts available in this environment — nothing to compare.
                return;
            }
            assert!(
                cache.cached_display_list.is_some(),
                "cold pass must seed the DL cache"
            );
            assert_eq!(cache.viewport, Some(viewport));
            let positions_after_first = cache.calculated_positions.clone();

            // Second pass: the structural-identity cache should short-circuit, and
            // must not corrupt the stored geometry on the way out.
            let second = run(&mut cache, &dom, viewport);
            assert!(
                second.is_ok(),
                "a warm relayout of an unchanged DOM must succeed"
            );
            assert_eq!(
                cache.calculated_positions.len(),
                positions_after_first.len(),
                "warm pass changed the node count"
            );
            for (warm, cold) in cache
                .calculated_positions
                .iter()
                .zip(&positions_after_first)
            {
                assert_eq!(warm.x.to_bits(), cold.x.to_bits(), "warm pass moved a node");
                assert_eq!(warm.y.to_bits(), cold.y.to_bits(), "warm pass moved a node");
            }
        }
    }
}
