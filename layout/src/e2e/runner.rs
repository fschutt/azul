//! Headless driver for E2E JSON scenarios.
//!
//! [`run_e2e_test`] runs an [`E2eTest`] end-to-end through the REAL server
//! op-dispatch (`super::full::process_debug_event` + the scenario runner
//! `resume_e2e_continuation`, pumped via `super::full::e2e_pump_continuation`) —
//! no HTTP, no timer, no DLL. It emulates the slice of the platform event loop
//! the E2E path needs, and it emulates it by PORTING the DLL, not by
//! approximating it:
//!
//! * [`Runner::apply_user_change`] is a port of `PlatformWindow::apply_user_change`
//!   (`dll/src/desktop/shell2/common/event.rs`) for the `CallbackChange`
//!   variants the E2E op set can produce — DOM mutation included.
//! * [`Runner::regenerate_layout`] / [`Runner::relayout_only`] port
//!   `regenerate_layout` / `incremental_relayout`
//!   (`dll/src/desktop/shell2/common/layout.rs`) plus the render + damage tail
//!   of the headless backend (`dll/src/desktop/shell2/headless/mod.rs`).
//! * [`super::cpu_backend::CpuBackend`] is the port of that backend's CPU
//!   renderer, which is what fills `LayoutWindow::frame_report` — without it
//!   every damage assertion reports "nothing was repainted (stale screen)".
//! * The font pipeline mirrors `AppInternal::create` + the font-snapshot block
//!   at the top of `regenerate_layout`, because font RESOLUTION differs between
//!   "one `FcFontCache::build()` for the whole process" and "an async registry
//!   snapshot re-installed on every DOM regeneration" — and the corpus contains
//!   a scenario (`mock_font_exact_metrics`) whose verdict depends on exactly
//!   that difference.
//!
//! The window/callback scaffolding mirrors the pattern in
//! `tests/contenteditable_e2e.rs` (LayoutWindow + `RendererResources::default()`
//! + `ExternalSystemCallbacks::rust_internal()` + `FullWindowState`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    events::ProcessEventResult,
    gl::OptionGlContextPtr,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollPosition,
    refany::{OptionRefAny, RefAny},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{MonitorVec, RawWindowHandle},
    xml::ComponentMap,
};
use azul_css::system::SystemStyle;
use rust_fontconfig::FcFontCache;

use azul_layout::{
    callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
    window::{LayoutWindow, MAX_EVENT_RECURSION_DEPTH},
    window_state::FullWindowState,
};

use super::cpu_backend::CpuBackend;
use super::full::{
    process_debug_event, e2e_pump_continuation, DebugEvent, DebugRequest, DebugResponseData,
    E2eSession, E2eTest, E2eTestResult, ResponseData,
};

// ── Headless window scaffolding ──────────────────────────────────────────────

struct Runner {
    layout_window: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
    /// CPU renderer + retained damage state (port of the headless backend).
    cpu_backend: CpuBackend,
    /// The app-level font cache, i.e. `AppInternal::fc_cache`. Re-installed on
    /// the layout window's font manager at the top of every `regenerate_layout`,
    /// exactly like the DLL does.
    app_fc_cache: FcFontCache,
    /// The async font registry, i.e. `AppInternal::font_registry`.
    #[cfg(feature = "font_async_registry")]
    font_registry: Option<Arc<azul_layout::FcFontRegistry>>,
    /// Set by a `ModifyWindowState` whose size or DPI changed — the DLL answers
    /// that with `RelayoutReason::Resize` + `mark_frame_needs_regeneration()`,
    /// which invalidates every cached rasterisation.
    resize_pending: bool,
}

impl Runner {
    fn new(width: f32, height: f32, dpi: u32) -> Self {
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(width, height);
        ws.size.dpi = dpi;

        // Port of `AppInternal::create`'s font setup: the app starts with an
        // async registry and an EMPTY `FcFontCache` (or a disk-cache snapshot);
        // the cache is populated from the registry at the first layout. This is
        // NOT the same as handing the window one eagerly-built `FcFontCache`:
        // the registry snapshot replaces the window's cache handle, which is
        // what makes in-memory (`register_named_font`) families behave the way
        // they do in a real app.
        #[cfg(feature = "font_async_registry")]
        let (app_fc_cache, font_registry) = {
            // `FcFontRegistry::new()` already returns an `Arc<Self>`.
            let registry = azul_layout::FcFontRegistry::new();
            let had_cache = registry.load_from_disk_cache();
            registry.spawn_scout_and_builders();
            // DETERMINISM: block until the scout has published the font set
            // (no-op when a disk cache was loaded; 5 s cap inside).
            //
            // Without this the fonts a DOM resolves depend on HOW FAR the
            // background builders happened to get before that particular
            // layout ran — so the same scenario resolves a 1-font fallback
            // chain when a step services a mount immediately and a 7-font one
            // when a `wait` delays it, and any assertion over font resources
            // silently measures thread scheduling. A verdict that moves with
            // background-thread progress is exactly the flake class this suite
            // exists to eliminate.
            registry.wait_for_scout();
            let cache = if had_cache.is_some() {
                registry.shared_cache()
            } else {
                FcFontCache::default()
            };
            (cache, Some(registry))
        };
        #[cfg(not(feature = "font_async_registry"))]
        let app_fc_cache = FcFontCache::build();

        Self {
            layout_window: LayoutWindow::new(app_fc_cache.clone()).expect("LayoutWindow::new"),
            renderer_resources: RendererResources::default(),
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state: ws,
            cpu_backend: CpuBackend::new(),
            app_fc_cache,
            #[cfg(feature = "font_async_registry")]
            font_registry,
            resize_pending: false,
        }
    }

    fn now(&self) -> azul_core::task::Instant {
        (self.system_callbacks.get_system_time_fn.cb)()
    }

    /// Build a `CallbackInfo` over the current window/state and run `f` with it.
    /// `ref_data` and the transient locals it borrows are dropped when `f`
    /// returns, releasing the borrow so the caller can relayout.
    fn with_callback_info<R>(
        &mut self,
        changes: &Arc<Mutex<Vec<CallbackChange>>>,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> R {
        let previous_window_state: Option<FullWindowState> = None;
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;

        let ref_data = CallbackInfoRefData {
            layout_window: &self.layout_window,
            renderer_resources: &self.renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &self.window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &self.system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: crate::icu::IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let mut callback_info = CallbackInfo::new(
            &ref_data,
            changes,
            DomNodeId { dom: DomId::ROOT_ID, node: NodeHierarchyItemId::NONE },
            azul_core::geom::OptionLogicalPosition::None,
            azul_core::geom::OptionLogicalPosition::None,
        );
        f(&mut callback_info)
    }

    /// Run the full layout pipeline for `styled_dom` and re-register scroll nodes.
    fn layout(&mut self, styled_dom: StyledDom) {
        let mut dbg = Some(Vec::new());
        self.layout_window
            .layout_and_generate_display_list(
                styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .expect("layout_and_generate_display_list");
        self.register_scroll_nodes();
    }

    /// Apply the `CallbackChange`s the runner pushed this pump, then finish the
    /// frame the way the platform event loop does.
    ///
    /// `needs_update` is the debug-op `needs_update` flag: the DLL's debug timer
    /// returns `Update::RefreshDom` for it, which the event loop turns into a
    /// full `regenerate_layout()`.
    fn service(&mut self, changes: &Arc<Mutex<Vec<CallbackChange>>>, needs_update: bool) {
        let drained = changes
            .lock()
            .map(|mut c| std::mem::take(&mut *c))
            .unwrap_or_default();

        // Each change is applied IN ORDER, as it is drained — NOT collapsed into
        // "the last one wins". The real shell runs `apply_user_change` once per
        // change and takes the MAX of the results; collapsing loses transient
        // states (a `key_down`+`key_up` pair that lands in a single continuation
        // slice would leave only the key-RELEASED state, and Tab-to-focus-next
        // would silently do nothing).
        let mut result = ProcessEventResult::DoNothing;
        for ch in drained {
            result = result.max(self.apply_user_change(&ch));
        }
        if needs_update {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        // A pending mount/unmount always needs the DOM rebuilt, even if the op
        // that produced it somehow did not set `needs_update`. (`RemountDom`
        // already returns `ShouldRegenerateDomCurrentWindow` from
        // `apply_user_change`; this covers a mount left dirty by an earlier
        // pass that never got to regenerate.)
        if self.layout_window.e2e_mount.is_dirty() {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        // A size/DPI change invalidates every rasterised pixel — same thing
        // WM_DPICHANGED / the X11 DPI path do (`frame_needs_regeneration`).
        if self.resize_pending {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }

        self.layout_window.sync_frame_report();
        self.layout_window.frame_report.terminal_result = result as u8;

        match result {
            ProcessEventResult::DoNothing => {}
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows => self.regenerate_layout(),
            ProcessEventResult::ShouldIncrementalRelayout => self.relayout_only(),
            ProcessEventResult::ShouldReRenderCurrentWindow
            | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => self.render_and_record(),
        }
    }

    /// Port of `PlatformWindow::process_window_events`
    /// (`dll/src/desktop/shell2/common/event.rs`) — the state-diff pass, and the
    /// thing `FrameReport::relayout_iterations` counts.
    ///
    /// The DLL records `max(depth + 1)` in an observability wrapper around this
    /// function and sets `hit_depth_cap` when the recursion is broken off at
    /// `MAX_EVENT_RECURSION_DEPTH`. Both are ported here VERBATIM, because the
    /// number an assertion reads has to mean the same thing in both hosts:
    ///
    /// * `0` — no event pass ran at all. That is what an idle frame looks like:
    ///   a clock tick with no state delta produces a repaint and nothing else.
    /// * `1` — one pass, converged.
    /// * `>1` — the pass had to be re-entered (a callback changed state that
    ///   raised new events); this is the invalidation-loop signal.
    ///
    /// Before this existed the runner hard-coded `1` inside the
    /// `ModifyWindowState` arm, so an idle frame reported the same number as a
    /// converged event pass and `assert_work_bounded` could not tell "no work"
    /// from "one pass of work".
    fn process_window_events(&mut self, depth: usize) -> ProcessEventResult {
        #[allow(clippy::cast_possible_truncation)]
        let depth_u32 = depth as u32;
        self.layout_window.sync_frame_report();
        let r = &mut self.layout_window.frame_report;
        r.relayout_iterations = r.relayout_iterations.max(depth_u32 + 1);

        if depth >= MAX_EVENT_RECURSION_DEPTH {
            // The DLL log_warn's here and returns; the flag is what turns that
            // silent cap into a red assertion.
            self.layout_window.frame_report.hit_depth_cap = true;
            return ProcessEventResult::DoNothing;
        }

        // The state-diff pass. The E2E op set reaches exactly one branch of it
        // without a platform window: the keyboard default action (Tab → focus
        // next / previous, Escape → clear focus).
        let focus_before = self
            .layout_window
            .focus_manager
            .get_focused_node()
            .copied();
        self.run_keyboard_default_action();
        let focus_after = self
            .layout_window
            .focus_manager
            .get_focused_node()
            .copied();

        let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;
        // Port of the DLL's focus recursion (`event.rs`: a focus change re-enters
        // the pass so the newly focused node's own state is evaluated).
        if focus_before != focus_after {
            if depth + 1 < MAX_EVENT_RECURSION_DEPTH {
                result = result.max(self.process_window_events(depth + 1));
            } else {
                self.layout_window.frame_report.hit_depth_cap = true;
            }
        }
        result
    }

    /// Port of `PlatformWindow::apply_user_change`
    /// (`dll/src/desktop/shell2/common/event.rs`) for the `CallbackChange`
    /// variants the E2E op set can produce. Each arm mirrors the DLL's arm —
    /// including its relayout / display-list bookkeeping, which is what makes
    /// the damage the assertions observe the SAME damage the real host produces.
    #[allow(clippy::too_many_lines)]
    fn apply_user_change(&mut self, change: &CallbackChange) -> ProcessEventResult {
        match change {
            // === Window State ===
            CallbackChange::ModifyWindowState { state } => {
                let old = std::mem::replace(&mut self.window_state, state.clone());
                let size_changed = self.window_state.size.dimensions != old.size.dimensions;
                let dpi_changed = self.window_state.size.dpi != old.size.dpi;
                if size_changed || dpi_changed {
                    self.resize_pending = true;
                }
                if state.flags.close_requested {
                    return ProcessEventResult::DoNothing;
                }

                // Port of the DLL's `anything_changed` gate: the state-diff pass
                // runs ONCE per ModifyWindowState **that actually changed
                // something**, and NOT AT ALL for a state re-push. That gate is
                // what makes `relayout_iterations` mean what it says — a
                // repaint request (`tick_ms` / `wait_frame`, which re-push the
                // current state) is not an event pass and must not be counted
                // as one.
                let anything_changed = size_changed
                    || dpi_changed
                    || self.window_state.mouse_state != old.mouse_state
                    || self.window_state.keyboard_state != old.keyboard_state
                    || self.window_state.window_focused != old.window_focused
                    || self.window_state.flags.has_focus != old.flags.has_focus
                    || self.window_state.position != old.position;

                let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;
                if anything_changed {
                    result = result.max(self.process_window_events(0));
                }
                result
            }

            // === Focus ===
            CallbackChange::SetFocusTarget { target } => {
                use azul_layout::managers::focus_cursor::resolve_focus_target;
                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;

                let now = self.now();
                let lw = &mut self.layout_window;
                let current_focus = lw.focus_manager.get_focused_node().copied();
                match resolve_focus_target(target, &lw.layout_results, current_focus) {
                    Ok(Some(new_focus)) => {
                        lw.focus_manager.set_focused_node(Some(new_focus));
                        lw.scroll_node_into_view(new_focus, ScrollIntoViewOptions::nearest(), now);
                        lw.finalize_pending_focus_changes();
                        ProcessEventResult::ShouldReRenderCurrentWindow
                    }
                    Ok(None) => {
                        lw.focus_manager.set_focused_node(None);
                        lw.finalize_pending_focus_changes();
                        ProcessEventResult::ShouldReRenderCurrentWindow
                    }
                    Err(_) => ProcessEventResult::DoNothing,
                }
            }

            // === Content Modifications ===
            CallbackChange::ChangeNodeText { node_id, text } => {
                let dom_id = node_id.dom;
                let Some(internal_node_id) = node_id.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(&dom_id) {
                    let idx = internal_node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        layout_result.styled_dom.node_data.as_container_mut()[internal_node_id]
                            .set_node_type(azul_core::dom::NodeType::Text(
                                azul_css::css::BoxOrStatic::heap(text.clone()),
                            ));
                    }
                }
                // The incremental layout cache keys its shaped-text runs on the
                // DOM pointer, which a text mutation does not change — so the
                // next relayout happily reused the OLD glyph runs and the screen
                // kept showing the previous text (damage was reported, yet not
                // one pixel differed). Drop the incremental cache so the text is
                // re-shaped…
                lw.layout_cache.reset_incremental();
                // …and rebuild the display list, which otherwise still carries
                // the old glyph run.
                lw.regenerate_display_list_for_dom(dom_id);
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::ChangeNodeImage { dom_id, node_id, image, update_type: _ } => {
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                    let idx = node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_node_type(azul_core::dom::NodeType::Image(
                                azul_css::css::BoxOrStatic::heap(image.clone()),
                            ));
                    }
                }
                lw.regenerate_display_list_for_dom(*dom_id);
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ChangeNodeImageMask { dom_id, node_id, mask } => {
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                    let idx = node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_clip_mask(mask.clone());
                    }
                }
                lw.regenerate_display_list_for_dom(*dom_id);
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::ChangeNodeCssProperties { dom_id, node_id, properties } => {
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                    let idx = node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        use azul_css::dynamic_selector::CssPropertyWithConditions;
                        let new_props: Vec<CssPropertyWithConditions> = properties
                            .as_ref()
                            .iter()
                            .map(|p| CssPropertyWithConditions::simple(p.clone()))
                            .collect();
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_css_props(new_props.into());

                        // STALE-SCREEN FIX: `set_css_props` only writes the
                        // node's INLINE property vec. Layout and the display
                        // list read the CSS PROPERTY CACHE, which still holds the
                        // cascaded value — so the node kept its old paint, the
                        // display list came out identical, the diff reported no
                        // damage and the screen went stale. Push the same
                        // properties through the user-override channel (the one
                        // the resolver consults FIRST).
                        let props_slice: Vec<azul_css::props::property::CssProperty> =
                            properties.as_ref().iter().cloned().collect();
                        drop(
                            layout_result
                                .styled_dom
                                .restyle_user_property(node_id, &props_slice),
                        );
                    }
                }
                lw.regenerate_display_list_for_dom(*dom_id);
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::OverrideNodeCssProperties { dom_id, node_id, properties } => {
                // Fast-path override channel: writes land in
                // `CssPropertyCache::user_overridden_properties`, which the
                // property resolver consults first.
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                    let idx = node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        let props_slice: Vec<azul_css::props::property::CssProperty> =
                            properties.as_ref().iter().cloned().collect();
                        drop(
                            layout_result
                                .styled_dom
                                .restyle_user_property(node_id, &props_slice),
                        );
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::UpdateVirtualView { dom_id, node_id } => {
                let mut updates = BTreeMap::new();
                let mut set = azul_core::FastBTreeSet::new();
                set.insert(*node_id);
                updates.insert(*dom_id, set);
                self.layout_window.queue_virtual_view_updates(updates);
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::UpdateAllVirtualViews => {
                self.layout_window.queue_all_virtual_view_reinvoke();
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::UpdateImageCallback { .. }
            | CallbackChange::UpdateAllImageCallbacks => {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === DOM structure ===
            CallbackChange::InsertChildNode {
                dom_id, parent_node_id, node_type_str, position, classes, id,
            } => {
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                    let parent_idx = parent_node_id.index();
                    if parent_idx < layout_result.styled_dom.node_data.as_ref().len() {
                        let node_type = parse_node_type_from_str(node_type_str.as_str());
                        let mut dom = azul_core::dom::Dom::create_node(node_type);
                        if let Some(id_str) = id.as_ref() {
                            dom = dom.with_id(id_str.clone());
                        }
                        for class in classes.iter() {
                            dom = dom.with_class(class.clone());
                        }
                        // Style it (empty CSS — the author rules are unavailable
                        // here; they are re-applied by `restyle_retained` below).
                        let css = azul_css::css::Css::empty();
                        let styled = StyledDom::create(&mut dom, css);

                        // `append_child` always attaches to the DOM's ROOT — the
                        // requested `parent_node_id` was accepted, validated and
                        // then IGNORED, so every inserted node landed as a last
                        // child of <html>. Append first, then RE-PARENT.
                        let sd = &mut layout_result.styled_dom;
                        let new_id = NodeId::new(sd.node_data.as_ref().len());
                        let root_id = sd.root.into_crate_internal().unwrap_or(NodeId::ZERO);
                        let root_last_before =
                            sd.node_hierarchy.as_container()[root_id].last_child_id();
                        sd.append_child(styled);

                        if *parent_node_id != root_id {
                            // The hierarchy is a FLAT DFS array whose
                            // `first_child_id(n)` is DERIVED as `n + 1`. A node
                            // appended at the end can therefore only ever be a
                            // LAST child, and only of a parent that already has
                            // children. Anything else needs a full re-index of
                            // the DOM (and every node-keyed manager), so it is
                            // rejected instead of silently corrupting the tree.
                            let parent_last =
                                sd.node_hierarchy.as_container()[*parent_node_id].last_child_id();
                            if let Some(parent_last) = parent_last {
                                // 1. unlink the new node from the root chain
                                {
                                    let h = &mut sd.node_hierarchy;
                                    h.as_container_mut()[root_id].last_child =
                                        NodeId::into_raw(&root_last_before);
                                    if let Some(rl) = root_last_before {
                                        h.as_container_mut()[rl].next_sibling =
                                            NodeId::into_raw(&None);
                                    }
                                    // 2. link it as the parent's new last child
                                    h.as_container_mut()[parent_last].next_sibling =
                                        NodeId::into_raw(&Some(new_id));
                                    h.as_container_mut()[new_id].previous_sibling =
                                        NodeId::into_raw(&Some(parent_last));
                                    h.as_container_mut()[new_id].next_sibling =
                                        NodeId::into_raw(&None);
                                    h.as_container_mut()[new_id].parent =
                                        NodeId::into_raw(&Some(*parent_node_id));
                                    h.as_container_mut()[*parent_node_id].last_child =
                                        NodeId::into_raw(&Some(new_id));
                                }
                                // 3. keep the cascade bookkeeping consistent
                                let sibling_index = {
                                    let h = sd.node_hierarchy.as_container();
                                    parent_node_id.az_children(&h).count().saturating_sub(1)
                                };
                                let ci = sd.cascade_info.as_mut();
                                ci[parent_last.index()].is_last_child = false;
                                ci[new_id.index()].index_in_parent =
                                    u32::try_from(sibling_index).unwrap_or(u32::MAX);
                                ci[new_id.index()].is_last_child = true;
                                sd.finalize_non_leaf_nodes();
                            }
                        }
                        let _ = position; // only append-as-last-child is representable

                        // Re-run the author cascade from the retained stylesheet:
                        // the node was styled with an EMPTY css above, so without
                        // this it would never match rules like `.hot { width: 80px }`
                        // — the "inserted node never gets the author cascade" bug.
                        sd.extend_author_scopes_for_appended(new_id, *parent_node_id);
                        sd.restyle_retained();
                        // `append_child` composes the trees but does NOT re-run
                        // inheritance or rebuild the compact cache: the appended
                        // node would keep its isolated cascade (no inherited
                        // font-size/color, no UA defaults, no compact-cache entry)
                        // and measure 0×0.
                        sd.recompute_inheritance_and_compact_cache();
                    }
                }
                // The tree changed shape: the incremental layout cache (keyed on
                // the DOM pointer) would otherwise reuse the old tree, and the
                // stored display list still describes the OLD tree.
                lw.layout_cache.reset_incremental();
                lw.regenerate_display_list_for_dom(*dom_id);
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::DeleteNode { dom_id, node_id } => {
                let lw = &mut self.layout_window;
                if let Some(layout_result) = lw.layout_results.get_mut(dom_id) {
                    let idx = node_id.index();
                    let node_count = layout_result.styled_dom.node_data.as_ref().len();
                    if idx < node_count && idx != 0 {
                        // Tombstone: set node to empty Div and unlink it.
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_node_type(azul_core::dom::NodeType::Div);
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_ids_and_classes(Vec::new().into());
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_callbacks(Vec::new().into());

                        let hierarchy = &mut layout_result.styled_dom.node_hierarchy;
                        let prev_sib = hierarchy.as_container()[*node_id].previous_sibling_id();
                        let next_sib = hierarchy.as_container()[*node_id].next_sibling_id();
                        let parent = hierarchy.as_container()[*node_id].parent_id();

                        if let Some(prev) = prev_sib {
                            hierarchy.as_container_mut()[prev].next_sibling =
                                NodeId::into_raw(&next_sib);
                        }
                        if let Some(next) = next_sib {
                            hierarchy.as_container_mut()[next].previous_sibling =
                                NodeId::into_raw(&prev_sib);
                        } else if let Some(p) = parent {
                            hierarchy.as_container_mut()[p].last_child =
                                NodeId::into_raw(&prev_sib);
                        }

                        hierarchy.as_container_mut()[*node_id].parent = 0;
                        hierarchy.as_container_mut()[*node_id].previous_sibling = 0;
                        hierarchy.as_container_mut()[*node_id].next_sibling = 0;
                        hierarchy.as_container_mut()[*node_id].last_child = 0;
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::SetNodeIdsAndClasses { dom_id, node_id, ids_and_classes } => {
                if let Some(layout_result) = self.layout_window.layout_results.get_mut(dom_id) {
                    let idx = node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        layout_result.styled_dom.node_data.as_container_mut()[*node_id]
                            .set_ids_and_classes(ids_and_classes.clone());
                    }
                }
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::RemountDom { xml } => {
                // The E2E `mount` / `unmount` document is per-window state, not
                // a process-global sink: store it on the window and let
                // `regenerate_layout` read it back on the next pass.
                self.layout_window
                    .e2e_mount
                    .set(xml.as_ref().map(|s| s.as_str().to_string()));
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
            }

            // === Scroll ===
            CallbackChange::ScrollTo { dom_id, node_id, position, unclamped } => {
                let now = self.now();
                if let Some(internal_node_id) = node_id.into_crate_internal() {
                    let lw = &mut self.layout_window;
                    if *unclamped {
                        lw.scroll_manager.set_scroll_position_unclamped(
                            *dom_id, internal_node_id, *position, now,
                        );
                    } else {
                        lw.scroll_manager.scroll_to(
                            *dom_id,
                            internal_node_id,
                            *position,
                            std::time::Duration::from_millis(0).into(),
                            azul_core::events::EasingFunction::Linear,
                            now,
                        );
                    }
                    // Recalculate scrollbar geometry so CPU-side hit testing has
                    // up-to-date thumb positions.
                    lw.scroll_manager.calculate_scrollbar_states();
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::ScrollIntoView { node_id, options } => {
                let now = self.now();
                let lw = &mut self.layout_window;
                azul_layout::managers::scroll_into_view::scroll_node_into_view(
                    *node_id,
                    &lw.layout_results,
                    &mut lw.scroll_manager,
                    *options,
                    now,
                );
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Font cache ===
            CallbackChange::ReloadSystemFonts => {
                self.layout_window
                    .font_manager
                    .replace_fc_cache(FcFontCache::build());
                ProcessEventResult::DoNothing
            }

            // === Propagation control (consumed by the dispatch loop) ===
            CallbackChange::StopPropagation
            | CallbackChange::StopImmediatePropagation
            | CallbackChange::PreventDefault => ProcessEventResult::DoNothing,

            // Everything else (timers, threads, menus, tooltips, clipboard, text
            // editing, drag & drop, window creation, routing, undo/redo) needs
            // facilities only the DLL host has — a platform window, a timer
            // driver, an OS clipboard. No scenario in `e2e/` reaches them; when
            // one does, port its arm here from `event.rs` rather than widening
            // this catch-all.
            _ => ProcessEventResult::DoNothing,
        }
    }

    /// Port of `common::layout::regenerate_layout` + the headless backend's
    /// render/damage tail: refresh the font snapshot, install the pending mount
    /// document (or keep the already-mounted, possibly-mutated DOM), re-run
    /// layout and render a frame.
    fn regenerate_layout(&mut self) {
        self.refresh_font_snapshot();

        self.layout_window.sync_frame_report();
        self.layout_window.frame_report.dom_regenerations =
            self.layout_window.frame_report.dom_regenerations.saturating_add(1);

        // E2E `mount` override: replace the DOM wholesale with the test's inline
        // XML+CSS document, but ONLY when the mount is dirty — otherwise keep the
        // already-mounted DOM (with any debug DOM mutations applied to it).
        let mount_change = self
            .layout_window
            .e2e_mount
            .take_dirty()
            .then(|| self.layout_window.e2e_mount.xml().map(str::to_string));
        let styled_dom = match mount_change {
            Some(Some(xml)) => match azul_layout::xml::parse_xml_to_styled_dom(&xml) {
                Ok(sd) => Some(sd),
                Err(_) => None,
            },
            Some(None) => {
                // `unmount`: drop the mounted document entirely.
                self.layout_window.layout_results.clear();
                self.cpu_backend.previous_display_list = None;
                self.resize_pending = false;
                return;
            }
            None => self
                .layout_window
                .layout_results
                .remove(&DomId::ROOT_ID)
                .map(|lr| lr.styled_dom),
        };

        let Some(mut styled_dom) = styled_dom else {
            self.resize_pending = false;
            return;
        };

        // A DPI or size change invalidates every cached rasterisation and every
        // shaped run measured at the old scale.
        if self.resize_pending {
            self.layout_window.clear_caches();
            self.resize_pending = false;
        }

        // Step 3.4 of `regenerate_layout`: re-run inheritance + rebuild the
        // compact cache on the composed tree.
        styled_dom.recompute_inheritance_and_compact_cache();

        self.layout(styled_dom);
        self.render_and_record();
    }

    /// Port of `common::layout::incremental_relayout` + the headless backend's
    /// render/damage tail: re-run layout on the EXISTING (already mutated)
    /// `StyledDom`, then render.
    ///
    /// This is NOT the same as `regenerate_layout()` for an in-place DOM
    /// mutation: `regenerate_layout` short-circuits on
    /// `is_layout_equivalent(old, new)`, and after an in-place mutation "old"
    /// and "new" are the same DOM — so layout would be skipped and the frame
    /// would keep the pre-mutation shaped text and geometry forever.
    fn relayout_only(&mut self) {
        if let Some(layout_result) = self.layout_window.layout_results.remove(&DomId::ROOT_ID) {
            self.layout(layout_result.styled_dom);
        }
        self.render_and_record();
    }

    /// CPU-render the current frame and publish its damage onto the
    /// `LayoutWindow`, where `CallbackInfo::get_layout_window()` — and therefore
    /// an E2E assertion — can see it.
    fn render_and_record(&mut self) {
        let width = self.window_state.size.dimensions.width;
        let height = self.window_state.size.dimensions.height;
        #[allow(clippy::cast_precision_loss)]
        let dpi = self.window_state.size.dpi as f32 / 96.0;
        self.cpu_backend.render_frame(
            &self.layout_window,
            &self.renderer_resources,
            width,
            height,
            dpi,
        );
        let paint = self.cpu_backend.last_frame_damage.clone();
        let present = self.cpu_backend.last_present_damage.clone();
        self.layout_window.record_frame(paint, present);

        // Publish the DAMAGE-DRIVEN framebuffer so `assert_damage_sound`'s
        // `pixel_identity` check can compare it against an independent full
        // repaint (`CallbackInfo::take_screenshot`). Only this host can: the DLL
        // presents from the GPU, which is why the op FAILS there rather than
        // silently skipping the check.
        #[cfg(feature = "cpurender")]
        if let Some(frame) = self.cpu_backend.last_frame.as_ref() {
            super::full::e2e_set_presented_frame(&self.layout_window, frame);
        }
    }

    /// Port of the font-snapshot block at the top of `regenerate_layout`: the
    /// window's font cache is re-installed from the async registry (or from the
    /// app-level cache when there is none) before every DOM regeneration.
    fn refresh_font_snapshot(&mut self) {
        #[cfg(feature = "font_async_registry")]
        if let Some(registry) = self.font_registry.as_ref() {
            // Avoid replacing a complete font cache with an incomplete snapshot
            // while the background builder threads are still parsing fonts.
            let current_cache_empty = self.layout_window.font_manager.fc_cache.is_empty();
            let build_complete = registry.is_build_complete();
            if current_cache_empty || build_complete {
                let font_stacks = rust_fontconfig::config::tokenize_common_families(
                    rust_fontconfig::OperatingSystem::current(),
                );
                registry.request_fonts(&font_stacks);
                self.layout_window
                    .font_manager
                    .replace_fc_cache(registry.shared_cache());
            }
            return;
        }
        // Fallback: use the app-level cache directly.
        self.layout_window
            .font_manager
            .replace_fc_cache(self.app_fc_cache.clone());
    }

    /// Port of the DLL event loop's keyboard-default-action pass: Tab →
    /// FocusNext/Previous, Escape → ClearFocus. Runs after a keyboard-state
    /// change lands (the real shell derives it in the state-diff pass).
    fn run_keyboard_default_action(&mut self) {
        use azul_core::events::DefaultAction;
        use azul_layout::default_actions::{
            default_action_to_focus_target, determine_keyboard_default_action,
        };
        use azul_layout::managers::focus_cursor::resolve_focus_target;
        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;

        let ks = self.window_state.keyboard_state.clone();
        let now = self.now();
        let lw = &mut self.layout_window;
        let focused = lw.focus_manager.get_focused_node().copied();
        let result = determine_keyboard_default_action(&ks, focused, &lw.layout_results, false);
        if !result.has_action() {
            lw.finalize_pending_focus_changes();
            return;
        }

        let mut new_focus: Option<DomNodeId> = None;
        let mut do_clear = false;
        match &result.action {
            DefaultAction::FocusNext
            | DefaultAction::FocusPrevious
            | DefaultAction::FocusFirst
            | DefaultAction::FocusLast => {
                if let Some(target) = default_action_to_focus_target(&result.action) {
                    if let Ok(resolved) = resolve_focus_target(&target, &lw.layout_results, focused) {
                        new_focus = resolved;
                    }
                }
            }
            DefaultAction::ClearFocus => do_clear = true,
            _ => {}
        }

        if let Some(nf) = new_focus {
            lw.focus_manager.set_focused_node(Some(nf));
            lw.scroll_node_into_view(nf, ScrollIntoViewOptions::nearest(), now);
        } else if do_clear {
            lw.focus_manager.set_focused_node(None);
        }
        lw.finalize_pending_focus_changes();
    }

    /// Port of the DLL's `register_scroll_nodes` (dll/.../common/layout.rs):
    /// after layout, push each scrollable container's bounds into the
    /// ScrollManager so scroll ops + reads work.
    fn register_scroll_nodes(&mut self) {
        let now = self.now();
        let lw = &mut self.layout_window;
        let mut regs: Vec<(DomId, NodeId, LogicalRect, LogicalSize, f32, f32, bool, bool)> =
            Vec::new();
        for (dom_id, layout_result) in &lw.layout_results {
            for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
                let Some(sb) = layout_result
                    .layout_tree
                    .warm(node_idx)
                    .and_then(|w| w.scrollbar_info.as_ref())
                else {
                    continue;
                };
                if !(sb.needs_vertical || sb.needs_horizontal) {
                    continue;
                }
                let Some(dom_node_id) = node.dom_node_id else {
                    continue;
                };
                let border_box_size = node.used_size.unwrap_or_default();
                let resolved = node.box_props.unpack();
                let border = &resolved.border;
                let container_size = LogicalSize {
                    width: (border_box_size.width - border.left - border.right).max(0.0),
                    height: (border_box_size.height - border.top - border.bottom).max(0.0),
                };
                let container_origin = layout_result
                    .calculated_positions
                    .get(node_idx)
                    .copied()
                    .unwrap_or_else(LogicalPosition::zero);
                let container_rect = LogicalRect { origin: container_origin, size: container_size };
                let content_size = layout_result.layout_tree.get_content_size(node_idx);
                let thickness = sb.scrollbar_width.max(sb.scrollbar_height);
                regs.push((
                    *dom_id,
                    dom_node_id,
                    container_rect,
                    content_size,
                    thickness,
                    sb.visual_width_px,
                    sb.needs_horizontal,
                    sb.needs_vertical,
                ));
            }
        }
        for (dom_id, node_id, container_rect, content_size, thickness, vis, h, v) in regs {
            lw.scroll_manager.register_or_update_scroll_node(
                dom_id, node_id, container_rect, content_size, now.clone(), thickness, vis, h, v,
            );
        }
        lw.scroll_manager.calculate_scrollbar_states();
    }
}

/// Port of `parse_node_type_from_str` (dll/.../common/event.rs) — the `insert_node`
/// op's `node_type` string (`"div"`, `"p"`, `"text:HELLO"`, …) → `NodeType`.
fn parse_node_type_from_str(s: &str) -> azul_core::dom::NodeType {
    use azul_core::dom::NodeType;
    if let Some(text) = s.strip_prefix("text:") {
        return NodeType::Text(azul_css::css::BoxOrStatic::heap(text.to_string().into()));
    }
    match s.to_lowercase().as_str() {
        "html" => NodeType::Html,
        "head" => NodeType::Head,
        "body" => NodeType::Body,
        "p" => NodeType::P,
        "article" => NodeType::Article,
        "section" => NodeType::Section,
        "nav" => NodeType::Nav,
        "aside" => NodeType::Aside,
        "header" => NodeType::Header,
        "footer" => NodeType::Footer,
        "main" => NodeType::Main,
        "h1" => NodeType::H1,
        "h2" => NodeType::H2,
        "h3" => NodeType::H3,
        "h4" => NodeType::H4,
        "h5" => NodeType::H5,
        "h6" => NodeType::H6,
        "br" => NodeType::Br,
        "hr" => NodeType::Hr,
        "pre" => NodeType::Pre,
        "blockquote" => NodeType::BlockQuote,
        "ul" => NodeType::Ul,
        "ol" => NodeType::Ol,
        "li" => NodeType::Li,
        "table" => NodeType::Table,
        "thead" => NodeType::THead,
        "tbody" => NodeType::TBody,
        "tr" => NodeType::Tr,
        "th" => NodeType::Th,
        "td" => NodeType::Td,
        "form" => NodeType::Form,
        "label" => NodeType::Label,
        "input" => NodeType::Input,
        "button" => NodeType::Button,
        _ => NodeType::Div,
    }
}

fn fail_result(test: &E2eTest, reason: &str) -> E2eTestResult {
    E2eTestResult {
        name: test.name.clone(),
        status: "fail".into(),
        duration_ms: 0,
        step_count: test.steps.len(),
        steps_passed: 0,
        steps_failed: test.steps.len(),
        steps: Vec::new(),
        final_screenshot: Some(format!("[runner] {reason}")),
    }
}

/// Run a single E2E JSON test end-to-end through the REAL server op-dispatch,
/// headlessly. Returns the server's own [`E2eTestResult`] (pass/fail + per-step
/// results) — the same value the HTTP `run_e2e_tests` command produces.
#[must_use]
pub fn run_e2e_test(test: &E2eTest) -> E2eTestResult {
    // Start this scenario on REAL time. The `tick_ms` op advances a clock that
    // is scoped to the calling thread, and worker threads are reused across
    // scenarios — without this reset the next scenario scheduled onto this
    // thread would start with the previous one's accumulated offset.
    azul_core::task::reset_test_clock();

    // This scenario's own scheduler slot. It is a LOCAL, not a `Runner` field,
    // only because `Runner::with_callback_info` takes `&mut self` and the
    // dispatcher needs `&mut` on the session at the same time — borrowck, not
    // ambient state. It has exactly the lifetime of this run.
    let mut session = E2eSession::new();

    let (w, h, dpi) = match &test.setup {
        Some(s) => (s.window_width as f32, s.window_height as f32, s.dpi),
        None => (800.0, 600.0, 96),
    };
    let mut runner = Runner::new(w, h, dpi);

    let (tx, rx) = std::sync::mpsc::channel();
    let request = DebugRequest {
        request_id: 1,
        event: DebugEvent::RunE2eTests { tests: vec![test.clone()], snapshots: None },
        window_id: None,
        wait_for_render: false,
        response_tx: tx,
    };
    let mut app_data = RefAny::new(());
    let component_map = Arc::new(Mutex::new(ComponentMap::default()));
    let callback_changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

    // First dispatch: RunE2eTests sets up the continuation and runs it until the
    // first yield (or completion).
    let needs_update = runner.with_callback_info(&callback_changes, |ci| {
        process_debug_event(&request, ci, &mut app_data, &component_map, &mut session)
    });
    runner.service(&callback_changes, needs_update);

    // Pump the continuation until it terminates (the result is sent on the final
    // resume). A generous cap guards against a non-terminating scenario.
    let mut iterations = 0usize;
    loop {
        let (needs_update, still_pending, resume_not_before) = runner
            .with_callback_info(&callback_changes, |ci| {
                e2e_pump_continuation(ci, &mut session)
            });

        if let Some(deadline) = resume_not_before {
            let now = Instant::now();
            if now < deadline {
                std::thread::sleep(deadline - now);
            }
        }
        runner.service(&callback_changes, needs_update);

        if !still_pending {
            break;
        }
        iterations += 1;
        assert!(
            iterations < 100_000,
            "e2e runner: continuation for '{}' did not terminate",
            test.name
        );
    }

    match rx.try_recv() {
        Ok(DebugResponseData::Ok { data: Some(ResponseData::E2eResults(r)), .. }) => r
            .results
            .into_iter()
            .next()
            .unwrap_or_else(|| fail_result(test, "RunE2eTests returned no results")),
        Ok(DebugResponseData::Ok { .. }) => {
            fail_result(test, "RunE2eTests returned a non-E2eResults response")
        }
        Ok(DebugResponseData::Err(e)) => fail_result(test, &e),
        Err(_) => fail_result(test, "RunE2eTests produced no response"),
    }
}
