//! Headless driver for E2E JSON scenarios.
//!
//! [`run_e2e_test`] runs an [`E2eTest`] end-to-end through the REAL server
//! op-dispatch (`super::full::process_debug_event` + the scenario runner
//! `resume_e2e_continuation`, pumped via `super::full::e2e_pump_continuation`) —
//! no HTTP, no timer, no DLL. It emulates just the slice of the platform event
//! loop the E2E path needs: build a [`CallbackInfo`] over a headless
//! [`LayoutWindow`], apply the window-state changes the runner pushes
//! (`modify_window_state` for the setup resize / DPI), consume the process-global
//! mount XML the `mount` op installs, and relayout between yields.
//!
//! The window/callback scaffolding mirrors the pattern in
//! `tests/contenteditable_e2e.rs` (LayoutWindow + `RendererResources::default()`
//! + `ExternalSystemCallbacks::rust_internal()` + `FullWindowState`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    gl::OptionGlContextPtr,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
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
    window::LayoutWindow,
    window_state::FullWindowState,
};

use super::full::{
    process_debug_event, e2e_clear_continuation, e2e_pump_continuation, DebugEvent,
    DebugRequest, DebugResponseData, E2eTest, E2eTestResult, ResponseData,
};

// ── Process-global mount sink ────────────────────────────────────────────────
//
// The `mount` op calls `crate::e2e::hooks::set_mount_xml(Some(doc))` with the
// fully-built mount document; the shell (here: this runner) reads it back on the
// next relayout. Host hooks are plain `fn` pointers, so the sink is a static.
// `RUN_LOCK` serializes whole runs because this sink and `E2E_CONTINUATION` are
// process-global.
static RUN_LOCK: Mutex<()> = Mutex::new(());
static MOUNT_XML: Mutex<Option<String>> = Mutex::new(None);
static MOUNT_DIRTY: Mutex<bool> = Mutex::new(false);

fn mount_sink(xml: Option<String>) {
    if let Ok(mut g) = MOUNT_XML.lock() {
        *g = xml;
    }
    if let Ok(mut d) = MOUNT_DIRTY.lock() {
        *d = true;
    }
}

/// Take the mount document iff the `mount`/`unmount` op set it since last check.
fn take_mount_if_dirty() -> Option<Option<String>> {
    let dirty = MOUNT_DIRTY.lock().map(|mut d| std::mem::replace(&mut *d, false)).unwrap_or(false);
    if dirty {
        Some(MOUNT_XML.lock().ok().and_then(|g| g.clone()))
    } else {
        None
    }
}

// ── Headless window scaffolding ──────────────────────────────────────────────

struct Runner {
    layout_window: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
    /// Last-mounted styled DOM, kept so a setup resize / DPI change can relayout.
    mounted_dom: Option<StyledDom>,
}

impl Runner {
    fn new(width: f32, height: f32, dpi: u32) -> Self {
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(width, height);
        ws.size.dpi = dpi;
        Self {
            layout_window: LayoutWindow::new(FcFontCache::build()).expect("LayoutWindow::new"),
            renderer_resources: RendererResources::default(),
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state: ws,
            mounted_dom: None,
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
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
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

    /// Apply the `CallbackChange`s the runner pushed this pump + consume any
    /// pending mount, relayouting as needed. This is the headless equivalent of
    /// the DLL's `apply_user_change` + `regenerate_layout` slice: the op handlers
    /// in `process_debug_event` push changes rather than mutating directly, and
    /// the platform event loop applies them between frames — here, between pumps.
    fn service(&mut self, changes: &Arc<Mutex<Vec<CallbackChange>>>) {
        let drained = changes
            .lock()
            .map(|mut c| std::mem::take(&mut *c))
            .unwrap_or_default();

        let mut new_window_state: Option<FullWindowState> = None;
        let mut scroll_tos: Vec<(DomId, NodeHierarchyItemId, LogicalPosition, bool)> = Vec::new();
        let mut scroll_into_views: Vec<(
            DomNodeId,
            azul_layout::managers::scroll_into_view::ScrollIntoViewOptions,
        )> = Vec::new();
        for ch in drained {
            match ch {
                CallbackChange::ModifyWindowState { state } => new_window_state = Some(state),
                CallbackChange::ScrollTo { dom_id, node_id, position, unclamped } => {
                    scroll_tos.push((dom_id, node_id, position, unclamped));
                }
                CallbackChange::ScrollIntoView { node_id, options } => {
                    scroll_into_views.push((node_id, options));
                }
                // The remaining variants (timers, images, menus, …) are not
                // exercised by the E2E op set.
                _ => {}
            }
        }

        // 1. Window-state change (focus / blur / move / resize / DPI / keyboard).
        //    Relayout only on a size or DPI change (mirrors the real shell: a
        //    focus/move/keyboard change does not rebuild layout).
        let mut relayout_needed = false;
        if let Some(state) = new_window_state {
            let old_size = self.window_state.size.dimensions;
            let old_dpi = self.window_state.size.dpi;
            self.window_state = state;
            relayout_needed = self.window_state.size.dimensions != old_size
                || self.window_state.size.dpi != old_dpi;
            // The DLL applies ModifyWindowState through the state-diff pass, which
            // runs the keyboard default action (Tab → focus next, Esc → clear).
            self.run_keyboard_default_action();
        }

        // 2. Mount / unmount / relayout.
        match take_mount_if_dirty() {
            Some(Some(xml)) => {
                if let Ok(sd) = azul_layout::xml::parse_xml_to_styled_dom(&xml) {
                    self.mounted_dom = Some(sd.clone());
                    self.layout(sd);
                }
            }
            Some(None) => {
                self.mounted_dom = None;
            }
            None => {
                if relayout_needed {
                    if let Some(dom) = self.mounted_dom.clone() {
                        // A DPI change invalidates the layout cache; clearing is a
                        // superset of what a pure resize needs.
                        self.layout_window.clear_caches();
                        self.layout(dom);
                    }
                }
            }
        }

        // 3. Scroll changes — applied after (re)layout so the scroll nodes are
        //    registered. Matches event.rs `CallbackChange::ScrollTo` handling.
        if !scroll_tos.is_empty() {
            let now = self.now();
            for (dom_id, node_id, position, unclamped) in scroll_tos {
                let Some(nid) = node_id.into_crate_internal() else { continue };
                if unclamped {
                    self.layout_window.scroll_manager.set_scroll_position_unclamped(
                        dom_id, nid, position, now.clone(),
                    );
                } else {
                    self.layout_window.scroll_manager.scroll_to(
                        dom_id,
                        nid,
                        position,
                        std::time::Duration::from_millis(0).into(),
                        azul_core::events::EasingFunction::Linear,
                        now.clone(),
                    );
                }
            }
            self.layout_window.scroll_manager.calculate_scrollbar_states();
        }
        for (node_id, options) in scroll_into_views {
            let now = self.now();
            azul_layout::managers::scroll_into_view::scroll_node_into_view(
                node_id,
                &self.layout_window.layout_results,
                &mut self.layout_window.scroll_manager,
                options,
                now,
            );
        }
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
    let _guard = RUN_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);

    // Install the mount sink and clear any state a previous (possibly panicked)
    // run may have left in the process globals.
    super::hooks::set_host_hooks(super::hooks::E2eHostHooks {
        set_mount_xml: Some(mount_sink),
        ..super::hooks::E2eHostHooks::NONE
    });
    e2e_clear_continuation();
    *MOUNT_XML.lock().unwrap() = None;
    *MOUNT_DIRTY.lock().unwrap() = false;

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
    runner.with_callback_info(&callback_changes, |ci| {
        let _ = process_debug_event(&request, ci, &mut app_data, &component_map);
    });
    runner.service(&callback_changes);

    // Pump the continuation until it terminates (the result is sent on the final
    // resume). A generous cap guards against a non-terminating scenario.
    let mut iterations = 0usize;
    loop {
        let (_needs_update, still_pending, resume_not_before) =
            runner.with_callback_info(&callback_changes, e2e_pump_continuation);

        if let Some(deadline) = resume_not_before {
            let now = Instant::now();
            if now < deadline {
                std::thread::sleep(deadline - now);
            }
        }
        runner.service(&callback_changes);

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
