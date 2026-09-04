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

use crate::solver3::layout_tree::LayoutNodeId;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    events::ProcessEventResult,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gl::OptionGlContextPtr,
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
    e2e_pump_continuation, process_debug_event, DebugEvent, DebugRequest, DebugResponseData,
    E2eSession, E2eStepResult, E2eTest, E2eTestResult, ResponseData,
};

// ── Headless window scaffolding ──────────────────────────────────────────────

struct Runner {
    layout_window: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
    /// The sync baseline the state-diff pass reads, i.e.
    /// `CommonWindowState::previous_window_state`. `determine_all_events`
    /// derives EVERY synthetic event (MouseDown/Up, KeyDown/Up, WindowFocusIn/
    /// Out, WindowMove/Resize) from `current` vs `previous`, so without this
    /// field the diff is always empty and no pointer or key event exists at
    /// all. Advanced by `ModifyWindowState` / `QueueWindowStateSequence`
    /// exactly where the DLL calls `set_previous_window_state`.
    previous_window_state: Option<FullWindowState>,
    /// Pointer→node resolution, i.e. `CommonWindowState::cpu_hit_tester`.
    ///
    /// The runner has no WebRender, so this is the same `CpuHitTester` the
    /// headless / CPU-mode desktop backends use, rebuilt from the layout
    /// results (see [`Runner::rebuild_hit_tester`]).
    cpu_hit_tester: azul_layout::headless::CpuHitTester,
    /// CPU renderer + retained damage state (port of the headless backend).
    cpu_backend: CpuBackend,
    /// The app-level font cache, i.e. `AppInternal::fc_cache`. Re-installed on
    /// the layout window's font manager at the top of every `regenerate_layout`,
    /// exactly like the DLL does.
    app_fc_cache: FcFontCache,
    /// The async font registry, i.e. `AppInternal::font_registry`.
    #[cfg(feature = "font_async_registry")]
    font_registry: Option<Arc<azul_layout::FcFontRegistry>>,
    /// Set by a `ModifyWindowState` whose size changed. Consumed by
    /// [`Runner::service`], which then runs the SAME resize decision the
    /// shells run (`LayoutWindow::resize_needs_full_regeneration`): full DOM
    /// regeneration only when a recorded window-size query answer flips or a
    /// CSS breakpoint / orientation is crossed; otherwise a relayout of the
    /// existing `StyledDom` at the new size. This is what lets the corpus
    /// assert `dom_regenerations == 0` across a plain resize.
    resize_pending: bool,
    /// Set by a `ModifyWindowState` whose DPI changed. Always a full
    /// regeneration: a scale change invalidates every cached rasterisation
    /// and every shaped run measured at the old scale.
    dpi_pending: bool,
    /// `CallbackChange`s this host could not apply faithfully (see
    /// [`Runner::unsupported`]). Non-empty ⇒ the scenario FAILS: it asked the
    /// engine to do something the headless runner cannot do, so whatever it
    /// asserted afterwards was asserted against a window where that something
    /// never happened.
    unsupported_changes: Vec<String>,
    /// The redraw a rendered frame asked for, i.e. the platform loop's
    /// `request_redraw()`.
    ///
    /// PORT of the tail of every DLL present path (x11 `mod.rs:4031`, wayland
    /// `mod.rs:4761`, windows `mod.rs:1062`/`1297`, macos `mod.rs:6334`):
    ///
    /// ```ignore
    /// // If any scrollbar is actively fading (0 < opacity < 1), schedule
    /// // another frame so the fade-out animation runs to completion.
    /// if lw.gpu_state_manager.scrollbar_fade_active { self.request_redraw(); }
    /// ```
    ///
    /// This host had no such re-arm, so the frame driven by `tick_ms` /
    /// `wait_frame` was the LAST one: a `wait` yields with a resume deadline,
    /// the pump sleeps, and `service()` then finds no pending change and
    /// renders nothing. Every state that settles on ELAPSED TIME — and the
    /// scrollbar fade, at `fade_delay` 500 ms + `fade_duration` 200 ms, is the
    /// one the corpus exercises — stayed frozen at whatever the last explicit
    /// frame left behind, so `scrollbar_fade_active` was still true when the
    /// scenario asked whether the window had settled.
    pending_redraw: bool,
}

impl Runner {
    fn new(width: f32, height: f32, dpi: u32, animations: bool) -> Self {
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
            layout_window: {
                let mut lw = LayoutWindow::new(app_fc_cache.clone()).expect("LayoutWindow::new");
                // Tweens OFF unless the scenario asked for them (`setup.animations`).
                // The default stays off so a scenario that never drives the clock
                // cannot screenshot geometry mid-glide — but "off, always, with no
                // flag" is what left the animated caret and the selection tween
                // with ZERO e2e coverage. Turning them on is deterministic here:
                // `run_e2e_test` freezes this thread's clock and only `tick_ms` /
                // `wait` advance it, so a tween's progress is a pure function of
                // the ops the scenario ran.
                lw.system_animations_override = Some(if animations {
                    azul_core::resources::SystemAnimations::default()
                } else {
                    azul_core::resources::SystemAnimations::disabled()
                });
                lw
            },
            renderer_resources: RendererResources::default(),
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state: ws,
            previous_window_state: None,
            cpu_hit_tester: azul_layout::headless::CpuHitTester::new(),
            cpu_backend: CpuBackend::new(),
            app_fc_cache,
            #[cfg(feature = "font_async_registry")]
            font_registry,
            resize_pending: false,
            dpi_pending: false,
            unsupported_changes: Vec::new(),
            pending_redraw: false,
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
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
            azul_core::geom::OptionLogicalPosition::None,
            azul_core::geom::OptionLogicalPosition::None,
        );
        f(&mut callback_info)
    }

    /// Run the full layout pipeline for `styled_dom` and re-register scroll nodes.
    ///
    /// `new_generation` is the shells' distinction between `regenerate_layout`
    /// (the app's layout callback built `styled_dom` from its model -
    /// `LayoutWindow::layout_new_generation`) and a relayout of the DOM it
    /// already rendered (`layout_and_generate_display_list`); only the former
    /// may retire acked text edits.
    fn layout(&mut self, styled_dom: StyledDom, new_generation: bool) {
        let mut dbg = Some(Vec::new());
        let laid_out = if new_generation {
            self.layout_window.layout_new_generation(
                styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
        } else {
            self.layout_window.layout_and_generate_display_list(
                styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
        };
        laid_out.expect("layout_and_generate_display_list");
        // Same CRITICAL sync as the DLL paths (regenerate_layout's tail /
        // incremental_relayout): the cached state is the "old size" the resize
        // decision diffs against. Without it, every resize after the first
        // compared against the size of the last FULL layout — conservative
        // (extra full regenerations), but wrong.
        self.layout_window.current_window_state = self.window_state.clone();
        self.register_scroll_nodes();
        self.rebuild_hit_tester();
    }

    /// Rebuild [`Runner::cpu_hit_tester`] from the current layout results.
    ///
    /// Port of the headless backend's post-`regenerate_layout` rebuild
    /// (`dll/.../shell2/headless/mod.rs`), which carries this comment: *without
    /// this rebuild that tester stays empty, so every click hit-tests to
    /// nothing and widget callbacks never fire*.
    ///
    /// Called from exactly two places, and both are load-bearing:
    ///
    /// * [`Runner::layout`] — the single funnel every layout pass goes through
    ///   (`regenerate_layout` for mount / remount / resize / DPI, and
    ///   `relayout_only` for an in-place DOM or style mutation). Rebuilding
    ///   here is what keeps a `click` after a `set_node_text` from testing the
    ///   pre-mutation geometry.
    /// * the tail of [`Runner::service`] — the paths that produce a new frame
    ///   WITHOUT running layout (`ShouldReRenderCurrentWindow`,
    ///   `ShouldUpdateDisplayListCurrentWindow`,
    ///   `UpdateHitTesterAndProcessAgain` — the last one names it outright) all
    ///   land there, and a display-list rebuild can move the `VirtualView`
    ///   placements this tester translates child DOMs by. It also guarantees
    ///   the invariant that actually matters: at the START of every op the
    ///   tester agrees with the frame on screen. A stale tester does not fail
    ///   loudly — it silently answers with the WRONG node, which is worse than
    ///   the `unsupported` refusal this replaced.
    fn rebuild_hit_tester(&mut self) {
        self.cpu_hit_tester.rebuild_from_layout_with_gpu(
            &self.layout_window.layout_results,
            Some(&self.layout_window.gpu_state_manager),
        );
    }

    /// Port of `PlatformWindow::update_hit_test_at`
    /// (`dll/src/desktop/shell2/common/event.rs`): resolve the pointer position
    /// to nodes and publish the result on the hover manager, which is where
    /// `determine_all_events` reads the mouse target from and where
    /// `CallbackInfo::get_hit_node` / text selection look it up.
    ///
    /// The CPU→`FullHitTest` conversion is the SAME function the desktop
    /// shells' `perform_hit_test` uses
    /// ([`azul_layout::headless::convert_cpu_hit_test_to_full`]) — not a
    /// re-derivation.
    fn update_hit_test_at(&mut self, position: LogicalPosition) {
        use azul_layout::managers::hover::InputPointId;
        self.update_hit_test_for(InputPointId::Mouse, position);
    }

    /// [`Self::update_hit_test_at`] for ANY input point (9b-ii-c): a second
    /// seat's hit test goes into that seat's own hover history, so its press
    /// is targeted at the node under that cursor - the same split the dll's
    /// `update_seat_hit_test_at` makes.
    fn update_hit_test_for(
        &mut self,
        input_id: azul_layout::managers::hover::InputPointId,
        position: LogicalPosition,
    ) {

        let focused_node = self.layout_window.focus_manager.get_focused_node().copied();
        let hit_test = {
            let scroll_manager = &self.layout_window.scroll_manager;
            let gpu = &self.layout_window.gpu_state_manager;
            let resolve = |d: azul_core::dom::DomId, n: azul_core::dom::NodeId| {
                scroll_manager.get_current_offset(d, n)
            };
            let resolve_tf = |d: azul_core::dom::DomId, n: azul_core::dom::NodeId| {
                gpu.caches
                    .get(&d)
                    .and_then(|c| c.css_current_transform_values.get(&n))
                    .copied()
            };
            let hits = self
                .cpu_hit_tester
                .hit_test_scrolled(position, &resolve, &resolve_tf);
            azul_layout::headless::convert_cpu_hit_test_to_full(
                &self.cpu_hit_tester,
                &hits,
                focused_node,
                &self.layout_window.layout_results,
                position,
                &resolve,
                &resolve_tf,
            )
        };
        self.layout_window
            .hover_manager
            .push_hit_test(input_id, hit_test);
    }

    /// Publish one hit test per live touch point and drive the gesture
    /// manager's per-finger sessions.
    ///
    /// This is the port of the X11 XI2 touch handler
    /// (`dll/src/desktop/shell2/linux/x11/mod.rs`), which does exactly these
    /// two things next to writing `touch_state`: it feeds
    /// `gesture_drag_manager.touch_down/touch_move/touch_up` (what
    /// `detect_pinch` / `detect_rotation` / `detect_swipe_direction` consume)
    /// and lets the state-diff pass derive the touch events.
    ///
    /// DELIBERATE DEVIATION: the shells pass the pointer's SCREEN position as
    /// the second coordinate; a headless window has no screen, so the window
    /// position is reused. Only multi-window gesture bookkeeping reads it.
    fn sync_touch_points(&mut self, old_points: &[azul_core::window::TouchPoint]) {
        use azul_layout::managers::hover::InputPointId;

        let now = self.now();
        let window_position = self.window_state.position;
        let focused_node = self.layout_window.focus_manager.get_focused_node().copied();
        let new_points: Vec<azul_core::window::TouchPoint> =
            self.window_state.touch_state.touch_points.as_ref().to_vec();

        for point in &new_points {
            let hit_test = {
                let scroll_manager = &self.layout_window.scroll_manager;
                let gpu = &self.layout_window.gpu_state_manager;
                let resolve = |d: azul_core::dom::DomId, n: azul_core::dom::NodeId| {
                    scroll_manager.get_current_offset(d, n)
                };
                let resolve_tf = |d: azul_core::dom::DomId, n: azul_core::dom::NodeId| {
                    gpu.caches
                        .get(&d)
                        .and_then(|c| c.css_current_transform_values.get(&n))
                        .copied()
                };
                let hits =
                    self.cpu_hit_tester
                        .hit_test_scrolled(point.position, &resolve, &resolve_tf);
                azul_layout::headless::convert_cpu_hit_test_to_full(
                    &self.cpu_hit_tester,
                    &hits,
                    focused_node,
                    &self.layout_window.layout_results,
                    point.position,
                    &resolve,
                    &resolve_tf,
                )
            };
            self.layout_window
                .hover_manager
                .push_hit_test(InputPointId::Touch(point.id), hit_test);

            match old_points.iter().find(|q| q.id == point.id) {
                None => self.layout_window.gesture_drag_manager.touch_down(
                    point.id,
                    point.position,
                    now.clone(),
                    window_position,
                    point.position,
                ),
                Some(before) if before.position != point.position => {
                    let _ = self.layout_window.gesture_drag_manager.touch_move(
                        point.id,
                        point.position,
                        now.clone(),
                        point.position,
                    );
                }
                Some(_) => {}
            }
        }
        for point in old_points {
            if !new_points.iter().any(|p| p.id == point.id) {
                self.layout_window.gesture_drag_manager.touch_up(
                    point.id,
                    point.position,
                    now.clone(),
                    point.position,
                );
            }
        }
    }

    /// Drop the hover history of every touch point that is no longer down.
    ///
    /// Runs at the END of [`Runner::service`], not inside
    /// [`Runner::sync_touch_points`]: `determine_all_events` resolves the
    /// TARGET of a `TouchEnd` through that history, so purging before the pass
    /// would send every touch-up to the mouse target instead of to the node the
    /// finger was actually on. Purging afterwards keeps
    /// `HoverManager::hover_histories` from growing one entry per touch id
    /// forever, which is exactly the kind of per-interaction growth the
    /// `[idle/growth]` family exists to catch.
    fn purge_ended_touch_points(&mut self) {
        use azul_layout::managers::hover::InputPointId;

        let live: Vec<u64> = self
            .window_state
            .touch_state
            .touch_points
            .as_ref()
            .iter()
            .map(|p| p.id)
            .collect();
        let stale: Vec<InputPointId> = self
            .layout_window
            .hover_manager
            .get_active_input_points()
            .into_iter()
            .filter(|id| matches!(id, InputPointId::Touch(t) if !live.contains(t)))
            .collect();
        for id in stale {
            self.layout_window.hover_manager.remove_input_point(&id);
        }
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
        // The redraw the PREVIOUS frame asked for (see `pending_redraw`). The
        // platform loops service `request_redraw()` on the next turn of the
        // loop, which is exactly here — and it is the only thing that lets a
        // time-driven animation advance across a step that pushes no change of
        // its own (`wait`).
        let mut result = if core::mem::take(&mut self.pending_redraw) {
            ProcessEventResult::ShouldReRenderCurrentWindow
        } else {
            ProcessEventResult::DoNothing
        };
        for ch in drained {
            result = result.max(self.apply_user_change(&ch));
        }
        // Timers, AFTER this pass's changes: an op that ARMS a timer (focusing a
        // contenteditable arms the caret blink) has to have armed it before the
        // pump looks, or the timer would always be one op late.
        result = result.max(self.pump_timers());
        // The ops above committed text through `apply_user_change` (a
        // `text_input` op is `CreateTextInput`), never through the event
        // pass - the post-commit notifications they owe are drained here.
        result = result.max(self.dispatch_text_notifications());
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
        // RESIZE POLICY (user ruling 2026-08-08) — the same decision every
        // desktop shell makes, through the same LayoutWindow fn, so the corpus
        // tests exactly what the shells run. A DPI change is always a full
        // regeneration; a size change re-invokes layout() only when a recorded
        // window-size query answer flips or a CSS breakpoint / orientation is
        // crossed. Everything else re-flows the existing StyledDom.
        if self.dpi_pending {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        } else if self.resize_pending {
            let old_logical = self
                .layout_window
                .current_window_state
                .size
                .get_logical_size();
            let full = self
                .layout_window
                .resize_needs_full_regeneration(old_logical, self.window_state.size.dimensions);
            result = result.max(if full {
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
            } else {
                ProcessEventResult::ShouldIncrementalRelayout
            });
        }

        self.layout_window.sync_frame_report();
        self.layout_window.frame_report.terminal_result = result as u8;

        match result {
            ProcessEventResult::DoNothing => {}
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows => self.regenerate_layout(),
            ProcessEventResult::ShouldIncrementalRelayout => self.relayout_only(),
            // The name IS the contract. A paint-only restyle (`:hover` /
            // `:focus` changing a colour) mutates the styled DOM's property
            // cache and asks for exactly this — but the DISPLAY LIST still
            // carries the old paint, so rendering without rebuilding it shows
            // the pre-restyle pixels and reports zero damage.
            //
            // This was invisible while every pointer op set `needs_update`: the
            // forced `regenerate_layout()` rebuilt the display list as a side
            // effect, so `:hover` appeared to work for the wrong reason. With
            // the op no longer fabricating that rebuild, the arm has to do the
            // work its own name promises.
            //
            // `UpdateHitTesterAndProcessAgain` is grouped here because it ranks
            // ABOVE `ShouldUpdateDisplayListCurrentWindow` in
            // `ProcessEventResult`'s order — it may never do less work than the
            // result it dominates. (The real shells map it to a full
            // regeneration; a display-list rebuild is the floor, not the cap.)
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                self.layout_window
                    .regenerate_display_list_for_dom(DomId::ROOT_ID);
                self.render_and_record();
            }
            ProcessEventResult::ShouldReRenderCurrentWindow => self.render_and_record(),
        }

        self.arm_tween_timer();

        // Keep servicing the redraws the frames themselves ask for, until the
        // window stops changing. The platform loops do this across turns of the
        // event loop; here it has to happen INSIDE one `service()`, because the
        // next thing the pump runs is the scenario's next step — and if that
        // step is an idleness assertion, it reads whatever this call left
        // behind. The scrollbar fade is 700 ms of WALL CLOCK (`fade_delay` 500 +
        // `fade_duration` 200) and each headless frame costs about a
        // millisecond, so this is a real-time-paced loop, exactly like a shell
        // redrawing at the display's rate — not a spin that fabricates time.
        self.pump_pending_redraws();

        // The frame is now final for this op. Re-derive the pointer→node map
        // from it so the NEXT op's hit test cannot read geometry that a
        // display-list-only path (the render-only arms above) just moved. See
        // [`Runner::rebuild_hit_tester`].
        self.rebuild_hit_tester();
        self.purge_ended_touch_points();
    }

    /// Arm the caret / selection tween driver if the display-list pass this
    /// frame ran left a tween in flight — port of the shared site at the tail
    /// of the DLL's `process_window_events`
    /// (`dll/src/desktop/shell2/common/event.rs`). The timer self-terminates
    /// via its `RefAny`'d flag when the tween finishes, so there is no matching
    /// stop call.
    ///
    /// PLACEMENT DIFFERS FROM THE DLL ON PURPOSE. The DLL arms inside the event
    /// pass because a shell's frame ends there; this host's frame ends in
    /// [`Runner::service`], and the ops that move a caret (`text_input`,
    /// `move_cursor`, `set_text_selection`) reach `apply_user_change` straight
    /// from `service` without a state-diff pass. Arming inside
    /// `process_window_events` alone would leave every op-driven tween
    /// un-driven — which is indistinguishable, from a scenario, from the tween
    /// not existing.
    fn arm_tween_timer(&mut self) {
        use azul_core::task::CARET_TWEEN_TIMER_ID;

        if !self.layout_window.text_edit_manager.tween.is_active()
            || self
                .layout_window
                .timers
                .contains_key(&CARET_TWEEN_TIMER_ID)
        {
            return;
        }
        let timer = self.layout_window.create_caret_tween_timer();
        self.layout_window.add_timer(CARET_TWEEN_TIMER_ID, timer);
    }

    /// Run every timer that is due, i.e. the timer half of the DLL's
    /// `PlatformWindow::process_timers_and_threads` +
    /// `PlatformWindow::invoke_expired_timers`
    /// (`dll/src/desktop/shell2/common/event.rs`).
    ///
    /// WHY IT EXISTS: `AddTimer` / `RemoveTimer` / `StartCursorBlinkTimer` /
    /// `StopCursorBlinkTimer` were all declared `unsupported("no timer driver")`
    /// — every one of `LayoutWindow`'s pieces (`tick_timers`, `run_single_timer`,
    /// `time_until_next_timer_ms`) existed, but nothing in this host ever drove
    /// them. Caret blink was therefore untestable and any behaviour that only
    /// happens on a timer expiry could not be expressed as a scenario.
    ///
    /// TIME. `Instant::now()` honours the thread-scoped test clock that the
    /// `tick_ms` op advances (`azul_core::task::advance_test_clock_ms`), so a
    /// scenario drives timers by *asserting* time rather than by sleeping
    /// through it: `tick_ms 600` expires a 530 ms blink, deterministically, in
    /// microseconds.
    ///
    /// READINESS is decided by `Timer::invoke`, not here — `tick_timers`
    /// deliberately returns every registered timer and `invoke` returns
    /// `DoNothing`/`Continue` for one whose delay or interval has not elapsed.
    /// That is why pumping on every `service()` is cheap and correct rather
    /// than a spin.
    ///
    /// The `Update` a timer callback returns is NOT the only way a rebuild gets
    /// requested (465060f5b): `apply_user_change` runs a whole event pass for
    /// `ModifyWindowState` / `CreateTextInput`, and a user callback dispatched
    /// inside it can itself return `Update::RefreshDom`, which surfaces as a
    /// `ShouldRegenerateDom*` RESULT. Folding both into one `max` is what keeps
    /// a requested DOM rebuild from being downgraded to a relayout of the DOM it
    /// was supposed to replace — the bug just fixed on the DLL side.
    fn pump_timers(&mut self) -> ProcessEventResult {
        use azul_core::callbacks::Update;
        use azul_core::task::TimerId;

        if self.layout_window.timers.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        let frame_start = self.now();
        let due: Vec<TimerId> = self.layout_window.tick_timers(frame_start.clone());

        let window_handle = RawWindowHandle::Unsupported;
        let gl_context = OptionGlContextPtr::None;

        let mut result = ProcessEventResult::DoNothing;
        let mut needs_dom_regeneration = false;

        for timer_id in due {
            let (changes, update) = {
                let Self {
                    layout_window,
                    window_state,
                    previous_window_state,
                    renderer_resources,
                    system_callbacks,
                    ..
                } = self;
                layout_window.run_single_timer(
                    timer_id.id,
                    frame_start.clone(),
                    &window_handle,
                    &gl_context,
                    Arc::new(SystemStyle::default()),
                    system_callbacks,
                    previous_window_state,
                    window_state,
                    renderer_resources,
                )
            };

            // Applied IMMEDIATELY, before the next timer runs, so inter-timer
            // visibility works: a timer that removes another timer must actually
            // have removed it by the time that one is reached. (A `Timer` that
            // asked to terminate arrives here as a `RemoveTimer` change appended
            // by `run_single_timer` itself.)
            for change in &changes {
                result = result.max(self.apply_user_change(change));
            }
            if matches!(update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                needs_dom_regeneration = true;
            }
        }

        if needs_dom_regeneration {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }
        result
    }

    /// Service the redraws a rendered frame asked for (`pending_redraw`), until
    /// the window stops changing.
    ///
    /// The cap is a SAFETY NET for a flag that never clears, not the expected
    /// exit: the scrollbar fade is driven by a monotonic clock, so it always
    /// terminates on its own. Hitting the cap deliberately leaves the state
    /// machine visibly un-settled rather than hanging the run — which is the
    /// outcome `assert_state_machines_idle` exists to report.
    fn pump_pending_redraws(&mut self) {
        const MAX_REDRAW_FRAMES: usize = 4096;
        let mut frames = 0usize;
        while self.pending_redraw && frames < MAX_REDRAW_FRAMES {
            self.render_and_record();
            frames += 1;
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
    ///
    /// The pass itself mirrors the DLL's ordering: determine events → `:hover`
    /// restyle → user callbacks → click-to-focus → keyboard default action →
    /// focus events → re-entry. It used to run ONLY the keyboard branch, which
    /// is why no pointer op could focus a node headlessly.
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

        // ── 1. EVENT DETERMINATION ───────────────────────────────────────
        //
        // `determine_all_events` is the ONLY thing that turns a window-state
        // delta into events. It reads the pointer target off the hover
        // manager, which the callers of this pass (`ModifyWindowState`,
        // `QueueWindowStateSequence`, `RequestHitTestUpdate`) fill in via
        // `update_hit_test_at` — exactly as the DLL's platform layer does
        // before calling `process_window_events`.
        let previous_state = self
            .previous_window_state
            .clone()
            .unwrap_or_else(|| self.window_state.clone());
        let timestamp = self.now();
        let wheel_delta = self.layout_window.scroll_manager.pending_wheel_event;
        let mut synthetic_events = {
            let lw = &self.layout_window;
            let providers: Vec<&dyn azul_core::events::EventProvider> = vec![
                &lw.text_input_manager,
                &lw.sensor_manager,
                &lw.gamepad_manager,
                &lw.geolocation_manager,
                &lw.permission_manager,
                &lw.biometric_manager,
                &lw.keyring_manager,
                &lw.media_session_manager,
                // Media playback (11c) — the six media events fire from here.
                &lw.media_player_manager,
            ];
            azul_layout::event_determination::determine_all_events(
                &self.window_state,
                &previous_state,
                &lw.hover_manager,
                &lw.focus_manager,
                &lw.file_drop_manager,
                Some(&lw.gesture_drag_manager),
                &providers,
                wheel_delta,
                self.layout_window.scroll_manager.pending_wheel_seat,
                timestamp,
            )
        };

        // PRESS-TARGET CAPTURE: the node a button was pressed on gets that
        // button's release even when the pointer released elsewhere. The DLL
        // does this immediately after determination (shell2/common/event.rs),
        // and `HoverManager::apply_press_target_capture` says to "call once
        // per pass, after `determine_all_events`, before dispatch" — this
        // runner is a port of that pass and had not been given the call, so
        // `press_targets` stayed empty here and the Click synthesis in
        // `event_determination`, which is guarded on
        // `press_target_for(seat).is_some()`, never fired. A pointer click
        // therefore ran no handler in the harness while the device was fine.
        {
            let lw = &mut self.layout_window;
            let layout_results = &lw.layout_results;
            let in_release_path =
                |press: azul_core::dom::DomNodeId, release: azul_core::dom::DomNodeId| -> bool {
                    // Is `press` the release target or one of its DOM ancestors
                    // (i.e. already on the release's propagation path)?
                    if press.dom != release.dom {
                        return false;
                    }
                    let (Some(press_node), Some(mut current)) = (
                        press.node.into_crate_internal(),
                        release.node.into_crate_internal(),
                    ) else {
                        return false;
                    };
                    let Some(lr) = layout_results.get(&release.dom) else {
                        return false;
                    };
                    let hierarchy = lr.styled_dom.node_hierarchy.as_container();
                    loop {
                        if current == press_node {
                            return true;
                        }
                        match hierarchy.get(current).and_then(|n| n.parent_id()) {
                            Some(parent) => current = parent,
                            None => return false,
                        }
                    }
                };
            lw.hover_manager
                .apply_press_target_capture(&mut synthetic_events, &in_release_path);
        }

        // Clear the one-shot pending-event flags now that this pass has
        // collected them — one event per change, not one per frame (the DLL
        // does this at the same point, right after determination).
        {
            let lw = &mut self.layout_window;
            lw.sensor_manager.clear_pending_event();
            lw.gamepad_manager.clear_pending_event();
            lw.geolocation_manager.clear_pending_event();
            lw.permission_manager.clear_pending_changed();
            lw.biometric_manager.clear_pending_event();
            lw.keyring_manager.clear_pending_event();
            lw.gesture_drag_manager.clear_pen_event_pending();
            lw.gesture_drag_manager.clear_native_gesture();
            lw.media_player_manager.clear_pending_event();
        }

        if synthetic_events.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        let mut result = ProcessEventResult::DoNothing;

        // ── 2. INCREMENTAL `:hover` RESTYLE ──────────────────────────────
        // Enter/leave targets of THIS pass, restyled now so pure-CSS `:hover`
        // rules take effect without a DOM regeneration.
        {
            let mut per_dom: BTreeMap<DomId, azul_core::styled_dom::HoverChange> = BTreeMap::new();
            for ev in &synthetic_events {
                let is_enter = ev.event_type == azul_core::events::EventType::MouseEnter;
                let is_leave = ev.event_type == azul_core::events::EventType::MouseLeave;
                if !is_enter && !is_leave {
                    continue;
                }
                let Some(node) = ev.target.node.into_crate_internal() else {
                    continue;
                };
                let entry = per_dom.entry(ev.target.dom).or_insert_with(|| {
                    azul_core::styled_dom::HoverChange {
                        left_nodes: Vec::new(),
                        entered_nodes: Vec::new(),
                    }
                });
                if is_enter {
                    entry.entered_nodes.push(node);
                } else {
                    entry.left_nodes.push(node);
                }
            }
            if !per_dom.is_empty() {
                result = result.max(apply_hover_restyle(&mut self.layout_window, per_dom));
            }
        }

        // The hit test the callbacks (and the click-to-focus pass below) see.
        let hit_test_for_dispatch = self
            .layout_window
            .hover_manager
            .get_current(&azul_layout::managers::hover::InputPointId::Mouse)
            .cloned();

        // ── 2b. PRE-CALLBACK INPUT INTERPRETER ───────────────────────────
        //
        // Port of the DLL's `pre_filter` step (`event.rs`: build
        // `InputInterpreterInfo`, call `layout_window.input_interpreter`, then
        // apply the resulting pre-callback `SystemChange`s BEFORE user dispatch).
        // THIS STAGE DID NOT EXIST: the runner derived the KeyDown event but
        // never ran the interpreter, so the keyboard editing ops it produces —
        // arrow-key caret movement and Backspace/Delete
        // (`SystemChange::ApplySelectionOp`) — were silently dropped. Every
        // keyboard-delete e2e test only passed because the `key_down` op used to
        // shortcut Backspace/Delete straight to the C-API `delete_backward`; once
        // that shortcut is removed to match native macOS, the real path has to
        // run here. We apply `ApplySelectionOp` (arrows / Backspace / Delete) via
        // the same `apply_selection_op` the DLL's arm calls; other pre-callback
        // changes (clipboard shortcuts, select-all) still flow through the
        // existing `CallbackChange` machinery.
        {
            use azul_core::events::{InputInterpreterInfo, InputInterpreterState, SystemChange};
            let pre_filter = {
                let lw = &self.layout_window;
                // The other seats' focus, so a seat's key resolves against
                // its own field here too (9b-ii-a-i-d-iv).
                let seat_focus =
                    azul_core::events::seat_focus_of_events(&synthetic_events, &lw.focus_manager);
                let info = InputInterpreterInfo {
                    seat_focus: &seat_focus,
                    events: &synthetic_events,
                    hit_test: hit_test_for_dispatch.as_ref(),
                    keyboard_state: &self.window_state.keyboard_state,
                    mouse_state: &self.window_state.mouse_state,
                    state: InputInterpreterState {
                        focused_node: lw.focus_manager.get_focused_node().copied(),
                        // Mirrors the dll shell: only a text-editing focus
                        // claims the arrow keys; every other focused widget
                        // gets to see them.
                        focus_is_editable: lw
                            .focus_manager
                            .get_focused_node()
                            .and_then(|f| {
                                let node = f.node.into_crate_internal()?;
                                let lr = lw.layout_results.get(&f.dom)?;
                                Some(crate::solver3::getters::is_node_contenteditable_inherited(
                                    &lr.styled_dom,
                                    node,
                                ))
                            })
                            .unwrap_or(false),
                        click_count: 1,
                        drag_start_position: if self.window_state.mouse_state.left_down
                            && lw.text_edit_manager.has_active_editing()
                        {
                            self.window_state.mouse_state.cursor_position.get_position()
                        } else {
                            None
                        },
                        has_selection: lw
                            .text_edit_manager
                            .multi_cursor
                            .as_ref()
                            .map(|mc| {
                                mc.selections.iter().any(|s| {
                                    matches!(
                                        &s.selection,
                                        azul_core::selection::Selection::Range(_)
                                    )
                                })
                            })
                            .unwrap_or(false),
                    },
                };
                azul_core::events::default_input_interpreter(&info)
            };
            for change in &pre_filter.system_changes {
                match change {
                    SystemChange::ApplySelectionOp {
                        target,
                        op,
                        seat_id,
                    } => {
                        if self
                            .layout_window
                            .apply_selection_op_for_seat(*seat_id, *target, op)
                        {
                            result = result
                                .max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                        }
                    }
                    // Port of the DLL's `apply_system_change` arm
                    // (`event.rs`, `SystemChange::TextSelectionClick`): the
                    // interpreter turns a MouseDown over a hovered node into
                    // this change, and applying it is what places the caret
                    // AT THE CLICKED CHARACTER. This arm DID NOT EXIST here:
                    // only `ApplySelectionOp` was applied, so in every e2e
                    // scenario a click focused the editable (caret seeded at
                    // the END by the focus path) but never moved the caret to
                    // the click — `e2e/bug-textinput-resize-select-visual.json`
                    // is the scenario that caught it.
                    SystemChange::TextSelectionClick {
                        position,
                        timestamp,
                    } => {
                        let time_ms = self.now().duration_since(timestamp).as_millis_u64();
                        if self
                            .layout_window
                            .process_mouse_click_for_selection(*position, time_ms)
                            .is_some()
                        {
                            result = result
                                .max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                        }
                    }
                    // Port of the DLL's `TextSelectionDrag` arm, including its
                    // node-drag suppression.
                    SystemChange::TextSelectionDrag {
                        start_position,
                        current_position,
                    } => {
                        if !self.layout_window.gesture_drag_manager.is_node_drag_active()
                            && self
                                .layout_window
                                .process_mouse_drag_for_selection(
                                    *start_position,
                                    *current_position,
                                )
                                .is_some()
                        {
                            result = result
                                .max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
                        }
                    }
                    // Still unported (dropped, as the whole set was before):
                    // AddCursorAtClick (Cmd+click multi-cursor), the clipboard
                    // trio (deferred post-callback in the DLL), auto-scroll
                    // timer arms. Port them next to a scenario that needs them.
                    _ => {}
                }
            }
        }

        // ── 3. USER CALLBACK DISPATCH (W3C capture → target → bubble) ────
        let old_focus = self.layout_window.focus_manager.get_focused_node().copied();
        let (changes_result, callback_update, prevent_default) =
            self.dispatch_events_propagated(&synthetic_events);
        result = result.max(changes_result);

        // The wheel delta has now been delivered; clear it so no later pass
        // re-fires a stale Scroll event.
        self.layout_window.scroll_manager.pending_wheel_event = None;

        let mut should_recurse = false;
        if matches!(
            callback_update,
            azul_core::callbacks::Update::RefreshDom
                | azul_core::callbacks::Update::RefreshDomAllWindows
        ) {
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            should_recurse = true;
        }

        // ── 3b. POST-CALLBACK TEXT INPUT ─────────────────────────────────
        //
        // Port of the DLL's `post_callback_filter_system_changes` →
        // `SystemChange::ApplyPendingTextInput` → `ApplyTextChangeset` tail.
        // The DECISION is not re-derived here: the same `azul_core` function
        // both hosts call answers it.
        //
        // THIS STAGE DID NOT EXIST. Text recorded but not yet applied — which
        // is what a native shell has at this point in the pass, because it
        // calls `record_text_input` BEFORE running the pass — was never landed
        // by this host, and a callback's `prevent_default()` never killed a
        // recorded edit either. The e2e corpus could reach text only through
        // `CallbackChange::CreateTextInput`, whose own arm records, dispatches
        // and applies in one go; a KeyDown handler's veto is structurally
        // invisible to that shape, so no scenario could express the thing every
        // shell does on every keystroke.
        {
            use azul_core::events::SystemChange;

            let new_focus_now = self.layout_window.focus_manager.get_focused_node().copied();
            let post_changes = azul_core::events::post_callback_filter_system_changes(
                prevent_default,
                &[],
                old_focus,
                new_focus_now,
            );
            if post_changes
                .iter()
                .any(|c| matches!(c, SystemChange::ApplyPendingTextInput))
            {
                let changeset_result = self.layout_window.apply_text_changeset();
                if !changeset_result.dirty_nodes.is_empty() {
                    result = result.max(if changeset_result.needs_relayout {
                        ProcessEventResult::ShouldIncrementalRelayout
                    } else {
                        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                    });
                    self.layout_window.scroll_selection_into_view(
                        azul_layout::window::SelectionScrollType::Cursor,
                        azul_layout::window::ScrollMode::Instant,
                    );
                }
            } else if prevent_default {
                // A vetoed edit must DIE, not wait: the pending record would
                // otherwise survive into the next pass, whose unconditional
                // apply would land the vetoed character late.
                self.layout_window.text_input_manager.clear_changeset();
            }
        }

        // ── 4. MOUSE CLICK-TO-FOCUS (W3C default action) ─────────────────
        // The deepest focusable ancestor of the deepest hit node takes focus
        // on MouseDown. This is the default action that makes `click` able to
        // focus anything at all — before the hit tester was wired, the ONLY
        // way to move focus headlessly was Tab.
        let mut mouse_click_focus_changed = false;
        if !prevent_default
            && synthetic_events
                .iter()
                .any(|e| e.event_type == azul_core::events::EventType::MouseDown)
        {
            // ONE rule, shared with the dll (9g-ii-e-ii): the nearest focusable
            // ancestor of the FRONT-MOST hit, in that hit's own DOM. This used
            // to walk every hit DOM and let the last focusable win, so a
            // focusable host node under a VirtualView page took a click meant
            // for the page.
            // The hit test of the seat that PRESSED (9b-ii-c): a second
            // cursor's press focuses what is under the second cursor, not
            // what the primary happens to hover.
            let press_seat = synthetic_events
                .iter()
                .find(|e| e.event_type == azul_core::events::EventType::MouseDown)
                .map_or(
                    azul_core::window::PRIMARY_POINTER_SEAT,
                    azul_layout::managers::hover::seat_of_event,
                );
            let hit_for_focus = if press_seat == azul_core::window::PRIMARY_POINTER_SEAT {
                hit_test_for_dispatch.clone()
            } else {
                self.layout_window
                    .hover_manager
                    .get_current(&azul_layout::managers::hover::InputPointId::for_seat(press_seat))
                    .cloned()
            };
            let clicked_focusable_node = hit_for_focus.as_ref().and_then(|hit_test| {
                let results = &self.layout_window.layout_results;
                crate::managers::hover::focusable_under_pointer(
                    hit_test,
                    |dom_id, nid| {
                        results.get(&dom_id).is_some_and(|lr| {
                            lr.styled_dom
                                .node_data
                                .as_container()
                                .get(nid)
                                .is_some_and(azul_core::dom::NodeData::is_focusable)
                        })
                    },
                    |dom_id, nid| {
                        results.get(&dom_id).and_then(|lr| {
                            lr.styled_dom
                                .node_hierarchy
                                .as_container()
                                .get(nid)
                                .and_then(|h| h.parent_id())
                        })
                    },
                )
            });

            if press_seat != azul_core::window::PRIMARY_POINTER_SEAT {
                // The DLL's `SystemChange::SetSeatFocus` (9b-ii-a-i-d-iv): a second
                // seat's press moves ITS focus onto the focusable under ITS
                // cursor, or to nothing; the primary's focus, caret and ring
                // are untouched.
                //
                // Through `set_seat_focus` rather than a second copy of its
                // body, which is why this was broken: the copy moved the seat's
                // focus and scrolled it into view but never ran the
                // `:seat-focus` restyle the method carries, so a seat's CLICK
                // changed focus without restyling anything and only the
                // `set_seat_focus` OP could make the pseudo-class paint.
                let seat_old = self.layout_window.focus_manager.focused_node_for(press_seat);
                if seat_old != clicked_focusable_node {
                    result = result.max(self.set_seat_focus(press_seat, clicked_focusable_node));
                }
            } else if let Some(new_focus_target) = clicked_focusable_node {
                if old_focus.and_then(|f| f.node.into_crate_internal())
                    != new_focus_target.node.into_crate_internal()
                {
                    result = result.max(self.set_focus(Some(new_focus_target), old_focus, false));
                    mouse_click_focus_changed = true;
                }
            } else if old_focus.is_some() {
                // Mirror of the DLL: a mousedown that reached no focusable node
                // BLURS (`None` is a legitimate focus target) - except on a
                // scrollbar, which keeps focus like a browser's.
                let on_scrollbar = hit_test_for_dispatch.as_ref().is_some_and(|ht| {
                    ht.hovered_nodes
                        .values()
                        .any(|h| !h.scrollbar_hit_test_nodes.is_empty())
                });
                if !on_scrollbar {
                    result = result.max(self.set_focus(None, old_focus, false));
                    mouse_click_focus_changed = true;
                }
            }
        }

        // ── 5. KEYBOARD DEFAULT ACTIONS (Tab / Shift+Tab / Escape) ───────
        // Gated on a KeyDown in THIS pass, like the DLL. Before that gate
        // existed the runner re-ran the action on every recursion level, so a
        // single Tab walked the focus ring MAX_EVENT_RECURSION_DEPTH times and
        // set `hit_depth_cap` on every key press (it only produced the right
        // answer because 7 steps over 3 focusables is a net +1).
        let mut default_action_focus_changed = false;
        if !prevent_default
            && synthetic_events
                .iter()
                .any(|e| e.event_type == azul_core::events::EventType::KeyDown)
        {
            let (r, changed) = self.run_keyboard_default_action(&synthetic_events);
            result = result.max(r);
            default_action_focus_changed = changed;
        }

        // ── 6. FOCUS EVENTS + RE-ENTRY ───────────────────────────────────
        if (default_action_focus_changed || mouse_click_focus_changed)
            && depth + 1 < MAX_EVENT_RECURSION_DEPTH
        {
            let new_focus = self.layout_window.focus_manager.get_focused_node().copied();

            // Collapse any selection: standard UI behaviour on focus change.
            if let Some(mc) = self.layout_window.text_edit_manager.multi_cursor.as_mut() {
                if let Some(cursor) = mc.get_primary_cursor() {
                    mc.set_single_cursor(cursor);
                }
            }

            let now = self.now();
            let mut focus_events = Vec::new();
            if let Some(old_node) = old_focus {
                focus_events.push(azul_core::events::SyntheticEvent::new(
                    azul_core::events::EventType::Blur,
                    azul_core::events::EventSource::User,
                    old_node,
                    now.clone(),
                    azul_core::events::EventData::None,
                ));
            }
            if let Some(new_node) = new_focus {
                focus_events.push(azul_core::events::SyntheticEvent::new(
                    azul_core::events::EventType::Focus,
                    azul_core::events::EventSource::User,
                    new_node,
                    now,
                    azul_core::events::EventData::None,
                ));
            }
            if !focus_events.is_empty() {
                let (focus_result, focus_update, _) =
                    self.dispatch_events_propagated(&focus_events);
                result = result.max(focus_result);
                if matches!(
                    focus_update,
                    azul_core::callbacks::Update::RefreshDom
                        | azul_core::callbacks::Update::RefreshDomAllWindows
                ) {
                    result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                }
            }

            // CRITICAL (verbatim from the DLL): advance the sync baseline
            // BEFORE recursing, or the SAME MouseDown / Tab is re-detected at
            // depth+1 and its default action fires again on every level.
            self.previous_window_state = Some(self.window_state.clone());
            result = result.max(self.process_window_events(depth + 1));
        } else if should_recurse && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            self.previous_window_state = Some(self.window_state.clone());
            result = result.max(self.process_window_events(depth + 1));
        }

        // Post-commit text notifications, drained at the end of this pass
        // like every other pass kind (port of the DLL's pass-end law).
        result = result.max(self.dispatch_text_notifications());

        // Finalize pending focus changes (caret init for contenteditable) —
        // the DLL's end-of-pass `SystemChange::FinalizePendingFocusChanges`.
        self.layout_window.finalize_pending_focus_changes();

        // DELIBERATE DEVIATION, floored not faithful: the DLL returns `result`
        // as-is, so a pass whose events changed nothing observable returns
        // DoNothing and the shell skips the frame. This host's damage
        // machinery is fed by `render_and_record` (see `Runner::service`), and
        // an event pass that produced no frame at all leaves the frame report
        // describing the PREVIOUS op. Flooring at "repaint" costs an extra
        // no-damage render and never hides one.
        result.max(ProcessEventResult::ShouldReRenderCurrentWindow)
    }

    /// A NON-primary seat's focus move (9b-ii-a-i-d): the same body as the
    /// `SetSeatFocusTarget` arm minus the resolution - move only that seat's
    /// entry, scroll it into view and restyle `:seat-focus`. No caret arming
    /// and no `:focus` restyle: the text-edit session stays the primary's
    /// (9b-ii-a-i-d-ii) and `:focus` is the primary's pseudo-class alone.
    fn set_seat_focus(&mut self, seat: u64, new_focus: Option<DomNodeId>) -> ProcessEventResult {
        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
        let now = self.now();
        let lw = &mut self.layout_window;
        let old_focus = lw.focus_manager.focused_node_for(seat);
        lw.focus_manager.set_focused_node_for(seat, new_focus);
        if let Some(n) = new_focus {
            lw.scroll_node_into_view(n, ScrollIntoViewOptions::nearest(), now);
        }
        // `:seat-focus` (9b-ii-a-i-d-iii-a): the same restyle the primary's
        // `:focus` gets, on the seat's own pseudo-class, and only while no
        // OTHER seat still focuses the node that lost this seat.
        let result = apply_seat_focus_restyle(lw, old_focus, new_focus);
        result.max(ProcessEventResult::ShouldReRenderCurrentWindow)
    }

    /// Port of the DLL's `apply_system_change(SystemChange::SetFocus { .. })`:
    /// move focus, scroll the new node into view, and apply the `:focus`
    /// restyle so focus styling lands on THIS frame instead of the next resize.
    fn set_focus(
        &mut self,
        new_focus: Option<DomNodeId>,
        old_focus: Option<DomNodeId>,
        visible: bool,
    ) -> ProcessEventResult {
        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;

        let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
        let new_focus_node_id = new_focus.and_then(|f| f.node.into_crate_internal());

        let now = self.now();
        let window_state = self.window_state.clone();
        let lw = &mut self.layout_window;
        // BEFORE the restyle below, which is what rebuilds the display list the
        // focus ring is emitted into - see the dll's `SystemChange::SetFocus`.
        lw.focus_manager
            .set_focused_node_with_visibility(new_focus, visible);
        if let Some(focus_node) = new_focus {
            lw.scroll_node_into_view(focus_node, ScrollIntoViewOptions::nearest(), now);
        }
        arm_caret_for_focus(lw, new_focus, &window_state);

        let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;
        if old_focus_node_id != new_focus_node_id {
            result = result.max(apply_focus_restyle(
                lw,
                old_focus_node_id,
                new_focus_node_id,
            ));
        }
        result
    }

    /// Port of `PlatformWindow::dispatch_events_propagated`
    /// (`dll/src/desktop/shell2/common/event.rs`): plan the callback
    /// invocations for a batch of `SyntheticEvent`s using the W3C
    /// capture→target→bubble model, invoke them, then apply every
    /// `CallbackChange` they produced through [`Runner::apply_user_change`].
    ///
    /// Returns `(max ProcessEventResult, merged Update, any preventDefault)`.
    /// `preventDefault` is what suppresses the click-to-focus and keyboard
    /// default actions above, so this cannot be short-circuited to "no
    /// callbacks exist in an XML mount" — a scenario that mounts a component
    /// carrying callbacks would then silently take the default action anyway.
    #[allow(clippy::too_many_lines)]
    /// Port of the DLL's `dispatch_text_notifications`: deliver the `Input`
    /// (edits committed outside the text-input record pipeline) and
    /// `TextChanged` (every landed edit, after the commit) notifications a
    /// pass left in the layout window. Every pass that can commit text ends
    /// with this - the event pass AND `service`, whose ops (`text_input`,
    /// `key_down`) commit through `apply_user_change` without ever entering
    /// the event pass; before, `TextChanged` never fired for a scripted
    /// keystroke while it fired for a platform one.
    fn dispatch_text_notifications(&mut self) -> ProcessEventResult {
        use azul_core::{
            callbacks::Update,
            events::{EventData, EventSource, EventType, SyntheticEvent},
        };

        let mut result = ProcessEventResult::DoNothing;
        let edited = self.layout_window.take_text_edit_notifications();
        let changed = self.layout_window.take_text_changed_notifications();
        if edited.is_empty() && changed.is_empty() {
            return result;
        }
        let now = self.now();
        for (event_type, targets) in [(EventType::Input, edited), (EventType::TextChanged, changed)] {
            if targets.is_empty() {
                continue;
            }
            let events: Vec<_> = targets
                .into_iter()
                .map(|node| {
                    SyntheticEvent::new(
                        event_type,
                        EventSource::User,
                        node,
                        now.clone(),
                        EventData::None,
                    )
                })
                .collect();
            let (dispatch_result, update, _) = self.dispatch_events_propagated(&events);
            result = result.max(dispatch_result);
            if matches!(update, Update::RefreshDom | Update::RefreshDomAllWindows) {
                result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
        }
        result
    }

    fn dispatch_events_propagated(
        &mut self,
        events: &[azul_core::events::SyntheticEvent],
    ) -> (ProcessEventResult, azul_core::callbacks::Update, bool) {
        use azul_core::{
            callbacks::{CoreCallbackData, Update},
            events::EventFilter,
            id::NodeId as CoreNodeId,
        };

        struct PlannedInvocation {
            dom_id: DomId,
            node_id: NodeId,
            callback_data: CoreCallbackData,
        }

        // Phase 1 — build the dispatch plan (read-only over the layout window).
        let planned_callbacks: Vec<PlannedInvocation> = {
            let lw = &self.layout_window;
            let focused_node = lw.focus_manager.get_focused_node().copied();
            let mut planned = Vec::new();

            for event in events {
                let event_filters =
                    azul_core::events::event_type_to_filters(event.event_type, &event.data);

                for filter in &event_filters {
                    match filter {
                        EventFilter::Hover(_) => {
                            let dom_id = event.target.dom;
                            let Some(layout_result) = lw.layout_results.get(&dom_id) else {
                                continue;
                            };

                            let node_hierarchy = {
                                let items = layout_result.styled_dom.node_hierarchy.as_container();
                                let nodes: Vec<azul_core::id::Node> = (0..items.len())
                                    .map(|i| {
                                        let item = &items.internal[i];
                                        azul_core::id::Node {
                                            parent: CoreNodeId::from_usize(item.parent),
                                            previous_sibling: CoreNodeId::from_usize(
                                                item.previous_sibling,
                                            ),
                                            next_sibling: CoreNodeId::from_usize(item.next_sibling),
                                            last_child: CoreNodeId::from_usize(item.last_child),
                                        }
                                    })
                                    .collect();
                                azul_core::id::NodeHierarchy::new(nodes)
                            };

                            let node_data_container =
                                layout_result.styled_dom.node_data.as_container();
                            let mut callback_map: BTreeMap<CoreNodeId, Vec<EventFilter>> =
                                BTreeMap::new();
                            for node_idx in 0..node_data_container.len() {
                                let node_id = CoreNodeId::new(node_idx);
                                if let Some(nd) = node_data_container.get(node_id) {
                                    let matching: Vec<EventFilter> = nd
                                        .get_callbacks()
                                        .as_ref()
                                        .iter()
                                        .filter(|cb| cb.event == *filter)
                                        .map(|cb| cb.event)
                                        .collect();
                                    if !matching.is_empty() {
                                        callback_map.insert(node_id, matching);
                                    }
                                }
                            }
                            if callback_map.is_empty() {
                                continue;
                            }

                            let mut event_clone = event.clone();
                            let prop_result = azul_core::events::propagate_event(
                                &mut event_clone,
                                &node_hierarchy,
                                &callback_map,
                            );

                            for (node_id, matched_filter) in &prop_result.callbacks_to_invoke {
                                let Some(nd) = node_data_container.get(*node_id) else {
                                    continue;
                                };
                                for cb in nd.get_callbacks().as_ref() {
                                    if cb.event == *matched_filter {
                                        planned.push(PlannedInvocation {
                                            dom_id,
                                            node_id: *node_id,
                                            callback_data: cb.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        EventFilter::Focus(_) => {
                            // Focus events fire on the focused node only.
                            let Some(focused) = focused_node else {
                                continue;
                            };
                            let Some(node_id) = focused.node.into_crate_internal() else {
                                continue;
                            };
                            let Some(lr) = lw.layout_results.get(&focused.dom) else {
                                continue;
                            };
                            let ndc = lr.styled_dom.node_data.as_container();
                            let Some(nd) = ndc.get(node_id) else { continue };
                            for cb in nd.get_callbacks().as_ref() {
                                if cb.event == *filter {
                                    planned.push(PlannedInvocation {
                                        dom_id: focused.dom,
                                        node_id,
                                        callback_data: cb.clone(),
                                    });
                                }
                            }
                        }
                        EventFilter::Window(_)
                        | EventFilter::Application(_)
                        | EventFilter::External(_) => {
                            // Window / Application / External events fire on
                            // EVERY node carrying a matching callback. External
                            // (media, 11c) joins them because a player's state
                            // change is not hit-tested: there is no node under
                            // a `TimeUpdate`.
                            for (dom_id, lr) in &lw.layout_results {
                                let ndc = lr.styled_dom.node_data.as_container();
                                for node_idx in 0..ndc.len() {
                                    let node_id = CoreNodeId::new(node_idx);
                                    let Some(nd) = ndc.get(node_id) else { continue };
                                    for cb in nd.get_callbacks().as_ref() {
                                        if cb.event == *filter {
                                            planned.push(PlannedInvocation {
                                                dom_id: *dom_id,
                                                node_id,
                                                callback_data: cb.clone(),
                                            });
                                        }
                                    }
                                }
                            }
                        }
                        EventFilter::Component(_) => {
                            // Lifecycle events carry their target node; no
                            // propagation.
                            let dom_id = event.target.dom;
                            let Some(node_id) = event.target.node.into_crate_internal() else {
                                continue;
                            };
                            let Some(lr) = lw.layout_results.get(&dom_id) else {
                                continue;
                            };
                            let ndc = lr.styled_dom.node_data.as_container();
                            let Some(nd) = ndc.get(node_id) else { continue };
                            for cb in nd.get_callbacks().as_ref() {
                                if cb.event == *filter {
                                    planned.push(PlannedInvocation {
                                        dom_id,
                                        node_id,
                                        callback_data: cb.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
            planned
        };

        if planned_callbacks.is_empty() {
            return (ProcessEventResult::DoNothing, Update::DoNothing, false);
        }

        // Phase 2 — invoke.
        let previous_window_state = self.previous_window_state.clone();
        let gl_context = OptionGlContextPtr::None;
        let window_handle = RawWindowHandle::Unsupported;
        let system_style = Arc::new(SystemStyle::default());

        let mut all_updates: Vec<Update> = Vec::new();
        let mut all_changes: Vec<CallbackChange> = Vec::new();
        let mut any_prevent_default = false;
        let mut propagation_stopped = false;
        let mut propagation_stopped_node: Option<(DomId, NodeId)> = None;

        for planned in planned_callbacks {
            // W3C stopPropagation: remaining handlers on the SAME node still
            // run; the first handler on a different node ends the dispatch.
            if propagation_stopped
                && propagation_stopped_node
                    .is_none_or(|(dom, nid)| dom != planned.dom_id || nid != planned.node_id)
            {
                break;
            }

            let mut callback =
                azul_layout::callbacks::Callback::from_core(planned.callback_data.callback);
            let hit_node = DomNodeId {
                dom: planned.dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(planned.node_id)),
            };

            let (changes, update) = {
                let lw = &mut self.layout_window;
                lw.invoke_single_callback_at(
                    hit_node,
                    &mut callback,
                    &mut planned.callback_data.refany.clone(),
                    &window_handle,
                    &gl_context,
                    system_style.clone(),
                    &ExternalSystemCallbacks::rust_internal(),
                    &previous_window_state,
                    &self.window_state,
                    &self.renderer_resources,
                )
            };

            all_updates.push(update);

            let mut should_stop_immediate = false;
            let mut should_stop_propagation = false;
            for change in &changes {
                match change {
                    CallbackChange::PreventDefault => any_prevent_default = true,
                    CallbackChange::StopImmediatePropagation => should_stop_immediate = true,
                    CallbackChange::StopPropagation => should_stop_propagation = true,
                    _ => {}
                }
            }
            all_changes.extend(changes);

            if should_stop_propagation && !propagation_stopped {
                propagation_stopped = true;
                propagation_stopped_node = Some((planned.dom_id, planned.node_id));
            }
            if should_stop_immediate {
                break;
            }
        }

        let mut changes_result = ProcessEventResult::DoNothing;
        for change in &all_changes {
            changes_result = changes_result.max(self.apply_user_change(change));
        }

        let merged_update = all_updates
            .iter()
            .copied()
            .fold(Update::DoNothing, Update::max);

        (changes_result, merged_update, any_prevent_default)
    }

    /// Port of `PlatformWindow::apply_user_change`
    /// (`dll/src/desktop/shell2/common/event.rs`) for the `CallbackChange`
    /// variants the E2E op set can produce. Each arm mirrors the DLL's arm —
    /// including its relayout / display-list bookkeeping, which is what makes
    /// the damage the assertions observe the SAME damage the real host produces.
    #[allow(clippy::too_many_lines)]
    fn apply_user_change(&mut self, change: &CallbackChange) -> ProcessEventResult {
        match change {
            // A script asking to run a script. The headless runner is ALREADY
            // executing a scenario when it gets here, and `E2eSession` has one
            // continuation slot per window — accepting this would overwrite
            // the run in progress with no trace. Refused loudly rather than
            // silently, because a scenario that quietly stops half way is the
            // worst of the available outcomes.
            // Cancelling in the headless runner: nothing here was started by
            // ExecuteE2eJson (it refuses, below), so there is never a handle
            // to cancel. A no-op, not an error — see `stop_e2e_json`.
            // The headless runner is where the animation e2e tests actually
            // execute, so this arm is the one that matters: it steps the
            // integrator by an exact `dt` with no wall clock involved, which is
            // what makes a mid-flight assertion reproducible.
            // The headless runner drives the same physics timer, so a wheel
            // gesture closes here exactly as it does in a shell — otherwise an
            // e2e scenario would never see `ScrollEnd` and could not pin it.
            CallbackChange::SettleScrollGesture => {
                self.layout_window.scroll_manager.settle_scroll_gesture();
                ProcessEventResult::DoNothing
            }

            // The headless runner drives the same state machine as a shell,
            // so an e2e scenario can pin the six media events.
            CallbackChange::MediaTransport { node, op } => {
                self.layout_window.media_player_manager.apply(*node, *op);
                ProcessEventResult::DoNothing
            }

            CallbackChange::SetAnimationMomentum {
                node,
                velocity_x,
                velocity_y,
            } => {
                if let Some(n) = node.node.into_crate_internal() {
                    self.layout_window
                        .apply_animation_momentum(n, *velocity_x, *velocity_y);
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }

            CallbackChange::TickAnimations { dt_micros, steps } => {
                // Idle-transparent: `tick_ms` routes through here on EVERY
                // scenario (one engine clock), so a tick with nothing to
                // animate must not charge the window a display-list pass —
                // that turned every timer-driven damage assertion
                // (caret blink's "idle frame does no work") into FULL damage.
                let had_work = !self.layout_window.animations.is_empty()
                    || self.layout_window.has_zombies()
                    || self.layout_window.has_track_work();
                if !had_work {
                    return ProcessEventResult::DoNothing;
                }
                let dt = *dt_micros as f32 / 1_000_000.0;
                for _ in 0..(*steps).max(1) {
                    self.layout_window.tick_animations(dt);
                }
                // Sample the tracks for THIS frame — may invoke COMPONENT
                // animation functions with a full TimerCallbackInfo; their
                // queued changes apply exactly like timer changes.
                let track_changes = {
                    let frame_start = self.now();
                    let Self {
                        layout_window,
                        window_state,
                        previous_window_state,
                        renderer_resources,
                        system_callbacks,
                        ..
                    } = self;
                    layout_window.run_track_frames(
                        dt,
                        frame_start,
                        &RawWindowHandle::Unsupported,
                        &OptionGlContextPtr::None,
                        Arc::new(SystemStyle::default()),
                        system_callbacks,
                        previous_window_state,
                        window_state,
                        renderer_resources,
                    )
                };
                let mut extra = ProcessEventResult::DoNothing;
                for ch in &track_changes {
                    extra = extra.max(self.apply_user_change(ch));
                }
                // A layout-affecting `animation` transition (width, margins)
                // must re-solve, not just repaint — the display-list rebuild
                // reads geometry the solver has not recomputed yet.
                extra.max(if self.layout_window.take_transition_relayout() {
                    ProcessEventResult::ShouldIncrementalRelayout
                } else if self.layout_window.take_transition_patched() {
                    // Every transitioning value was PATCHED into the DL in
                    // place: no rebuild, just re-render — the DL diff turns
                    // the patched items into bounded damage.
                    ProcessEventResult::ShouldReRenderCurrentWindow
                } else {
                    ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                })
            }

            CallbackChange::StopE2eJson { .. } => ProcessEventResult::DoNothing,

            CallbackChange::ExecuteE2eJson { .. } => {
                crate::e2e::full::log(
                    crate::e2e::full::LogLevel::Warn,
                    crate::e2e::full::LogCategory::Callbacks,
                    "execute_e2e_json ignored: already inside an E2E run. Nested                      scripts would overwrite the outer run's continuation. This holds                      for BOTH execution modes: Sync would additionally block the very                      thread that has to drive the outer run.",
                    None,
                );
                ProcessEventResult::DoNothing
            }

            // === Window State ===
            CallbackChange::ModifyWindowState { state } => {
                let old = std::mem::replace(&mut self.window_state, state.clone());
                let size_changed = self.window_state.size.dimensions != old.size.dimensions;
                let dpi_changed = self.window_state.size.dpi != old.size.dpi;
                let mouse_state_changed = self.window_state.mouse_state != old.mouse_state;
                // The other cursors (9b-ii-c): the dll's arm makes the same
                // distinction, or a seat op would neither hit-test nor diff.
                let seats_changed = self.window_state.pointer_seats != old.pointer_seats;
                if size_changed {
                    self.resize_pending = true;
                }
                if dpi_changed {
                    self.dpi_pending = true;
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
                // `touch_state` was MISSING from this list, and the string
                // `touch_state` appeared nowhere in this file. A touch op
                // mutated the state, the gate answered "nothing changed", the
                // pass never ran and no touch event was ever determined — with
                // no `unsupported`, no `send_err` and a green `ok`. 48 corpus
                // lines executed nothing and passed.
                let touch_state_changed = self.window_state.touch_state != old.touch_state;
                // Captured BEFORE `old` is moved into `previous_window_state`
                // below; `sync_touch_points` needs the previous point set to
                // tell a new finger from a moved one.
                let old_touch_points: Vec<azul_core::window::TouchPoint> = if touch_state_changed {
                    old.touch_state.touch_points.as_ref().to_vec()
                } else {
                    Vec::new()
                };
                let anything_changed = size_changed
                    || dpi_changed
                    || touch_state_changed
                    || seats_changed
                    || self.window_state.mouse_state != old.mouse_state
                    || self.window_state.keyboard_state != old.keyboard_state
                    // The other seats' keyboards (9b-ii-a-i-d-v-a): the same
                    // hole as `touch_state` above - a seat `key_down` op
                    // mutated `keyboard_seats`, the gate said "nothing
                    // changed", no pass ran, and the seat-Tab scenario
                    // reported the seat's focus unmoved with no error.
                    || self.window_state.keyboard_seats != old.keyboard_seats
                    || self.window_state.window_focused != old.window_focused
                    || self.window_state.flags.has_focus != old.flags.has_focus
                    || self.window_state.position != old.position;

                let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;
                if anything_changed {
                    // Advance the sync baseline BEFORE the pass — it is what
                    // `determine_all_events` diffs `current` against, so
                    // forgetting it makes every event pass see a zero delta
                    // and produce nothing.
                    self.previous_window_state = Some(old);
                }
                // Mouse state changed → re-resolve the pointer target before
                // the pass, exactly where the DLL calls `update_hit_test_at`.
                if mouse_state_changed {
                    if let Some(pos) = self.window_state.mouse_state.cursor_position.get_position()
                    {
                        self.update_hit_test_at(pos);
                    }
                }
                // Same idea for touch, one hit test PER FINGER — see
                // [`Runner::sync_touch_points`].
                if seats_changed {
                    let seats: Vec<(u64, Option<LogicalPosition>)> = self
                        .window_state
                        .pointer_seats
                        .as_ref()
                        .iter()
                        .map(|s| (s.seat_id, s.state.cursor_position.get_position()))
                        .collect();
                    for (seat_id, pos) in seats {
                        if let Some(pos) = pos {
                            self.update_hit_test_for(
                                azul_layout::managers::hover::InputPointId::for_seat(seat_id),
                                pos,
                            );
                        }
                    }
                }
                if touch_state_changed {
                    self.sync_touch_points(&old_touch_points);
                }
                if anything_changed {
                    result = result.max(self.process_window_events(0));
                }
                result
            }

            // === Focus ===
            // A NON-primary seat's focus (9b-ii-a-i-d): the DLL's arm - resolve
            // against that seat's own focus, move only its entry, scroll it
            // into view. No caret arming: the text-edit session stays the
            // primary's (9b-ii-a-i-d-ii).
            CallbackChange::SetSeatFocusTarget { seat_id, target } => {
                use azul_layout::managers::focus_cursor::{resolve_focus_target, FocusResolution};
                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;

                let now = self.now();
                let lw = &mut self.layout_window;
                let current = lw.focus_manager.focused_node_for(*seat_id);
                let out_of_scope = lw.focus_out_of_scope_doms();
                let new_focus =
                    match resolve_focus_target(target, &lw.layout_results, current, &out_of_scope) {
                        Ok(FocusResolution::Resolved(n)) => Some(n),
                        Ok(FocusResolution::ClearRequested) => None,
                        Ok(FocusResolution::NotFound | FocusResolution::Deferred) | Err(_) => {
                            return ProcessEventResult::DoNothing;
                        }
                    };
                if new_focus == current {
                    return ProcessEventResult::DoNothing;
                }
                lw.focus_manager.set_focused_node_for(*seat_id, new_focus);
                if let Some(n) = new_focus {
                    lw.scroll_node_into_view(n, ScrollIntoViewOptions::nearest(), now);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::SetFocusTarget { target } => {
                use azul_layout::managers::focus_cursor::{
                    resolve_focus_target_or_defer, FocusResolution,
                };
                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;

                let now = self.now();
                let window_state = self.window_state.clone();
                let lw = &mut self.layout_window;
                // `resolve_focus_target` cannot tell "matched nothing" from "no
                // layout exists to match against yet": both are `Ok(None)`, and
                // this arm applied that as CLEAR FOCUS. A `set_focus` issued
                // before the first layout — the ordinary case for a `create`
                // callback — therefore vanished, which is what apps papered
                // over with a short timer. `Deferred` means do NOTHING: the
                // target is parked on the focus manager and re-resolved by
                // `finalize_pending_focus_changes` after the next layout pass.
                let out_of_scope = lw.focus_out_of_scope_doms();
                match resolve_focus_target_or_defer(
                    &mut lw.focus_manager,
                    target,
                    &lw.layout_results,
                    &out_of_scope,
                ) {
                    Ok(FocusResolution::Resolved(new_focus)) => {
                        lw.focus_manager.set_focused_node(Some(new_focus));
                        lw.scroll_node_into_view(new_focus, ScrollIntoViewOptions::nearest(), now);
                        arm_caret_for_focus(lw, Some(new_focus), &window_state);
                        lw.finalize_pending_focus_changes();
                        ProcessEventResult::ShouldReRenderCurrentWindow
                    }
                    Ok(FocusResolution::ClearRequested) => {
                        lw.focus_manager.set_focused_node(None);
                        arm_caret_for_focus(lw, None, &window_state);
                        lw.finalize_pending_focus_changes();
                        ProcessEventResult::ShouldReRenderCurrentWindow
                    }
                    // A MISS is not a clear: a selector that matched nothing
                    // (e.g. a VirtualView that has not materialized yet) must
                    // not destroy the current focus/caret.
                    Ok(FocusResolution::NotFound) => ProcessEventResult::DoNothing,
                    Ok(FocusResolution::Deferred) => ProcessEventResult::DoNothing,
                    Err(_) => ProcessEventResult::DoNothing,
                }
            }

            // === Content Modifications ===
            CallbackChange::ChangeNodeAccessibilityState { node_id, states } => {
                // Mirrors the DLL arm: update the node's declaration in place so
                // an e2e scenario observes the same accessibility tree a real
                // host would. Widgets that toggle WITHOUT rebuilding (accordion,
                // switch, checkbox) publish through here, so a scenario asserting
                // on announced state needs it applied, not dropped.
                if let Some(nid) = node_id.node.into_crate_internal() {
                    if let Some(lr) = self.layout_window.layout_results.get_mut(&node_id.dom) {
                        let mut nodes = lr.styled_dom.node_data.as_container_mut();
                        if let Some(node) = nodes.get_mut(nid) {
                            let mut info = node
                                .accessibility
                                .as_ref()
                                .map_or_else(Default::default, |b| (**b).clone());
                            info.states = states.clone();
                            node.set_accessibility_info(info);
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::ChangeNodeAccessibilityValue { node_id, value } => {
                if let Some(nid) = node_id.node.into_crate_internal() {
                    if let Some(lr) = self.layout_window.layout_results.get_mut(&node_id.dom) {
                        let mut nodes = lr.styled_dom.node_data.as_container_mut();
                        if let Some(node) = nodes.get_mut(nid) {
                            let mut info = node
                                .accessibility
                                .as_ref()
                                .map_or_else(Default::default, |b| (**b).clone());
                            info.accessibility_value = azul_css::OptionString::Some(value.clone());
                            node.set_accessibility_info(info);
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::ChangeNodeText { node_id, text } => {
                let dom_id = node_id.dom;
                let Some(internal_node_id) = node_id.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                let lw = &mut self.layout_window;

                // NO-OP SHORT CIRCUIT. Setting the text to the byte-identical
                // string used to throw away the ENTIRE incremental shaped-text
                // cache and re-shape every run in the DOM, then relayout the
                // whole root — the maximum work in the engine, for a write that
                // changed nothing. It also went green: the re-shape reproduces
                // identical glyphs, so the display list is identical, so the
                // damage is `none` and `assert_damage {"kind":"none"}` passed
                // while the engine did everything. That IS over-invalidation,
                // and it was invisible to every assertion the harness had.
                let unchanged = lw.layout_results.get(&dom_id).is_some_and(|lr| {
                    let nodes = lr.styled_dom.node_data.as_container();
                    nodes.get(internal_node_id).is_some_and(|node| {
                        matches!(
                            node.get_node_type(),
                            azul_core::dom::NodeType::Text(existing)
                                if existing.as_str() == text.as_str()
                        )
                    })
                });
                if unchanged {
                    return ProcessEventResult::DoNothing;
                }

                if let Some(layout_result) = lw.layout_results.get_mut(&dom_id) {
                    let idx = internal_node_id.index();
                    if idx < layout_result.styled_dom.node_data.as_ref().len() {
                        layout_result.styled_dom.node_data.as_container_mut()[internal_node_id]
                            .set_node_type(azul_core::dom::NodeType::Text(
                                azul_css::css::BoxOrStatic::heap(text.clone()),
                            ));
                    }
                }
                // NO cache reset (USER mandate: per-IFC text patching). The
                // reconcile fingerprints node CONTENT, so the changed text
                // node hashes differently, misses its cached shaping, and
                // re-shapes exactly its own IFC — while the warm tree and the
                // previous display list survive, which is what lets the
                // STRUCTURE-PRESERVED DL patch splice every untouched node
                // and re-emit only the edited paragraph. The old
                // `reset_incremental()` here was the hammer that made every
                // text edit a cold full pass and a full-frame repaint.
                // Staleness is guarded by bug_dom_mutation_no_damage's pixel
                // LIVENESS assert; the cheapness by dl_text_patch.
                ProcessEventResult::ShouldIncrementalRelayout
            }

            CallbackChange::RecordDocumentEdit { changeset } => {
                self.layout_window.record_document_edit(changeset.clone());
                ProcessEventResult::DoNothing
            }
            CallbackChange::MarkDocumentEditApplied { id } => {
                let _ = self.layout_window.mark_document_edit_applied(*id);
                ProcessEventResult::DoNothing
            }
            CallbackChange::MarkDocumentEditAppliedWithInverse { id, inverse } => {
                let _ = self
                    .layout_window
                    .mark_document_edit_applied_with_inverse(*id, inverse.clone());
                ProcessEventResult::DoNothing
            }
            CallbackChange::MarkTextRevisionSynced { revision } => {
                self.layout_window.mark_text_revision_synced(*revision);
                ProcessEventResult::DoNothing
            }
            CallbackChange::UndoStructuralEdit => {
                let _ = self.layout_window.undo_structural_edit();
                ProcessEventResult::DoNothing
            }
            CallbackChange::RedoStructuralEdit => {
                let _ = self.layout_window.redo_structural_edit();
                ProcessEventResult::DoNothing
            }

            CallbackChange::ChangeNodeImage {
                dom_id,
                node_id,
                image,
                update_type: _,
            } => {
                // The content chokepoint: overlay write + journal + in-place DL
                // patch (paint tier) or incremental-cache reset (relayout
                // tier). The StyledDom is NEVER mutated.
                let result =
                    self.layout_window
                        .apply_content_change(crate::overlay::ContentChange::Image {
                            dom_id: *dom_id,
                            node_id: *node_id,
                            image: image.clone(),
                        });
                result.tier.to_process_event_result()
            }

            CallbackChange::ChangeNodeImageMask {
                dom_id,
                node_id,
                mask,
            } => self
                .layout_window
                .apply_content_change(crate::overlay::ContentChange::ImageMask {
                    dom_id: *dom_id,
                    node_id: *node_id,
                    mask: mask.clone(),
                })
                .tier
                .to_process_event_result(),

            CallbackChange::ChangeNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => {
                // Same one-line delegation as the DLL host — the chokepoint
                // owns inline-vec sync, cascade restyle, DL rebuild and tier.
                self.layout_window
                    .apply_content_change(crate::overlay::ContentChange::NodeCss {
                        dom_id: *dom_id,
                        node_id: *node_id,
                        props: properties.as_ref().to_vec(),
                        override_only: false,
                    })
                    .tier
                    .to_process_event_result()
            }

            CallbackChange::OverrideNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => self
                .layout_window
                .apply_content_change(crate::overlay::ContentChange::NodeCss {
                    dom_id: *dom_id,
                    node_id: *node_id,
                    props: properties.as_ref().to_vec(),
                    override_only: true,
                })
                .tier
                .to_process_event_result(),

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
                dom_id,
                parent_node_id,
                node_type_str,
                position,
                classes,
                id,
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

            CallbackChange::SetNodeIdsAndClasses {
                dom_id,
                node_id,
                ids_and_classes,
            } => {
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
            CallbackChange::ScrollTo {
                dom_id,
                node_id,
                position,
                unclamped,
            } => {
                let now = self.now();
                if let Some(internal_node_id) = node_id.into_crate_internal() {
                    let lw = &mut self.layout_window;
                    if *unclamped {
                        lw.scroll_manager.set_scroll_position_unclamped(
                            *dom_id,
                            internal_node_id,
                            *position,
                            now,
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

                    // Mirror of the DLL arm: a VirtualView on this node decides
                    // from the new offset whether it has to re-materialize (an
                    // edge approach, a jump past its window) — queued for the
                    // drain at the top of the next frame — and its host item
                    // is re-pointed at the new offset either way, because a
                    // VirtualView has no scroll frame to move it.
                    let queued = lw.check_and_queue_virtual_view_reinvoke(*dom_id, internal_node_id);
                    lw.patch_virtual_view_content_offset(*dom_id, internal_node_id);
                    if queued {
                        return ProcessEventResult::ShouldUpdateDisplayListCurrentWindow;
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // #28 (a): mirror of the DLL arm — full-geometry reconfigure
            // (Some = set, None = keep) via the two stores a VV invoke
            // writes, WITHOUT re-invoking the callback.
            CallbackChange::SetVirtualViewGeometry {
                dom_id,
                node_id,
                materialized,
                virtual_rect,
            } => {
                if let Some(internal_node_id) = node_id.into_crate_internal() {
                    let lw = &mut self.layout_window;
                    let (kept_scroll, kept_virtual) = lw
                        .virtual_view_manager
                        .get_declared_sizes(*dom_id, internal_node_id);
                    let kept_origin = lw
                        .virtual_view_manager
                        .materialized_window_origin(*dom_id, internal_node_id);
                    let new_mat: Option<LogicalRect> = (*materialized).into();
                    let new_virt: Option<LogicalRect> = (*virtual_rect).into();
                    // `None` = keep. The streaming case sets only
                    // `virtual_rect`, so the materialized window (and every
                    // pixel on screen) is untouched while the bar re-scales.
                    let eff_virtual = new_virt.map(|r| r.size).or(kept_virtual);
                    let eff_scroll = new_mat.map(|r| r.size).or(kept_scroll).or(eff_virtual);
                    let eff_origin = new_mat
                        .map(|r| r.origin)
                        .or(kept_origin)
                        .unwrap_or_else(LogicalPosition::zero);
                    if let (Some(s), Some(v)) = (eff_scroll, eff_virtual) {
                        let _ = lw.virtual_view_manager.update_virtual_view_info(
                            *dom_id,
                            internal_node_id,
                            eff_origin,
                            s,
                            v,
                        );
                        lw.scroll_manager.update_virtual_scroll_bounds(
                            *dom_id,
                            internal_node_id,
                            v,
                            Some(eff_origin),
                        );
                        lw.scroll_manager.calculate_scrollbar_states();
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            CallbackChange::ScrollIntoView { node_id, options } => {
                let now = self.now();
                let lw = &mut self.layout_window;
                let hops = lw.nested_dom_hops();
                let hop = move |d: DomId| hops.get(&d).copied();
                azul_layout::managers::scroll_into_view::scroll_node_into_view(
                    *node_id,
                    &lw.layout_results,
                    &mut lw.scroll_manager,
                    *options,
                    now,
                    &hop,
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

            // === Window lifetime ===
            CallbackChange::CloseWindow => {
                self.window_state.flags.close_requested = true;
                ProcessEventResult::DoNothing
            }

            // === Text editing ===
            CallbackChange::InsertText {
                dom_id,
                node_id,
                text,
            } => {
                use azul_layout::managers::text_input::TextInputSource;
                let lw = &mut self.layout_window;
                let dom_node_id = DomNodeId {
                    dom: *dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                };
                let old_inline_content = lw.get_text_before_textinput(*dom_id, *node_id);
                let old_text = lw.extract_text_from_inline_content(&old_inline_content);
                lw.text_input_manager.record_input(
                    dom_node_id,
                    text.to_string(),
                    old_text,
                    TextInputSource::Programmatic,
                );
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::DeleteBackward { dom_id, node_id } => {
                self.apply_capi_delete(*dom_id, *node_id, false)
            }
            CallbackChange::DeleteForward { dom_id, node_id } => {
                self.apply_capi_delete(*dom_id, *node_id, true)
            }
            // Same route as every `MoveCursor{Left,Right,…}` arm, and as the
            // DLL's. Setting the cursor straight on the multi-cursor state
            // skips the display-list rebuild `handle_cursor_movement` does, so
            // a programmatic move repainted the OLD caret position — the
            // pre-fix body the DLL already replaced. `extend_selection` is
            // false because this variant carries an absolute cursor, not a
            // movement.
            CallbackChange::MoveCursor {
                dom_id,
                node_id,
                cursor,
            } => {
                self.layout_window
                    .handle_cursor_movement(*dom_id, *node_id, *cursor, false);
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::SetSelection {
                dom_id: _,
                node_id: _,
                selection,
            } => {
                use azul_core::selection::Selection;
                if let Some(mc) = self.layout_window.text_edit_manager.multi_cursor.as_mut() {
                    match selection {
                        Selection::Cursor(cursor) => mc.set_single_cursor(*cursor),
                        Selection::Range(range) => mc.set_single_range(*range),
                    }
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::SetTextChangeset { changeset } => {
                self.layout_window
                    .text_input_manager
                    .set_changeset(changeset.clone());
                ProcessEventResult::DoNothing
            }

            // === Cursor movement ===
            CallbackChange::MoveCursorLeft {
                dom_id,
                node_id,
                extend_selection,
            } => self.move_cursor(*dom_id, *node_id, *extend_selection, |layout, cursor| {
                layout.move_cursor_left(*cursor, &mut None)
            }),
            CallbackChange::MoveCursorRight {
                dom_id,
                node_id,
                extend_selection,
            } => self.move_cursor(*dom_id, *node_id, *extend_selection, |layout, cursor| {
                layout.move_cursor_right(*cursor, &mut None)
            }),
            CallbackChange::MoveCursorUp {
                dom_id,
                node_id,
                extend_selection,
            } => self.move_cursor(*dom_id, *node_id, *extend_selection, |layout, cursor| {
                layout.move_cursor_up(*cursor, &mut None, &mut None)
            }),
            CallbackChange::MoveCursorDown {
                dom_id,
                node_id,
                extend_selection,
            } => self.move_cursor(*dom_id, *node_id, *extend_selection, |layout, cursor| {
                layout.move_cursor_down(*cursor, &mut None, &mut None)
            }),
            CallbackChange::MoveCursorToLineStart {
                dom_id,
                node_id,
                extend_selection,
            } => self.move_cursor(*dom_id, *node_id, *extend_selection, |layout, cursor| {
                layout.move_cursor_to_line_start(*cursor, &mut None)
            }),
            CallbackChange::MoveCursorToLineEnd {
                dom_id,
                node_id,
                extend_selection,
            } => self.move_cursor(*dom_id, *node_id, *extend_selection, |layout, cursor| {
                layout.move_cursor_to_line_end(*cursor, &mut None)
            }),
            // Document start/end are NOT a `move_cursor_in_node` movement in the
            // DLL either — they read the first/last cluster straight off the
            // inline layout.
            CallbackChange::MoveCursorToDocumentStart {
                dom_id,
                node_id,
                extend_selection,
            } => {
                use azul_core::selection::{CursorAffinity, TextCursor};
                let lw = &mut self.layout_window;
                let first = lw
                    .get_inline_layout_for_node(*dom_id, *node_id)
                    .and_then(|layout| layout.items.first().and_then(|i| i.item.as_cluster()))
                    .map(|c| TextCursor {
                        cluster_id: c.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    });
                if let Some(doc_start) = first {
                    lw.handle_cursor_movement(*dom_id, *node_id, doc_start, *extend_selection);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::MoveCursorToDocumentEnd {
                dom_id,
                node_id,
                extend_selection,
            } => {
                use azul_core::selection::{CursorAffinity, TextCursor};
                let lw = &mut self.layout_window;
                let last = lw
                    .get_inline_layout_for_node(*dom_id, *node_id)
                    .and_then(|layout| layout.items.last().and_then(|i| i.item.as_cluster()))
                    .map(|c| TextCursor {
                        cluster_id: c.source_cluster_id,
                        affinity: CursorAffinity::Trailing,
                    });
                if let Some(doc_end) = last {
                    lw.handle_cursor_movement(*dom_id, *node_id, doc_end, *extend_selection);
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Multi-cursor / selection ===
            CallbackChange::AddCursor {
                dom_id,
                node_id,
                cursor,
            } => {
                use azul_core::selection::MultiCursorState;
                let lw = &mut self.layout_window;
                if let Some(mc) = lw.text_edit_manager.multi_cursor.as_mut() {
                    let _ = mc.add_cursor(*cursor);
                } else {
                    let dom_node_id = DomNodeId {
                        dom: *dom_id,
                        node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                    };
                    lw.text_edit_manager.multi_cursor =
                        Some(MultiCursorState::new_with_cursor(*cursor, dom_node_id, 0));
                }
                lw.text_edit_manager.mark_dirty();
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }
            CallbackChange::AddSelectionRange {
                dom_id,
                node_id,
                range,
            } => {
                use azul_core::selection::MultiCursorState;
                let lw = &mut self.layout_window;
                if let Some(mc) = lw.text_edit_manager.multi_cursor.as_mut() {
                    let _ = mc.add_selection(*range);
                } else {
                    let dom_node_id = DomNodeId {
                        dom: *dom_id,
                        node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
                    };
                    let mut mc = MultiCursorState::new_with_cursor(range.start, dom_node_id, 0);
                    mc.set_single_range(*range);
                    lw.text_edit_manager.multi_cursor = Some(mc);
                }
                lw.text_edit_manager.mark_dirty();
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }
            CallbackChange::RemoveSelectionById { selection_id } => {
                let lw = &mut self.layout_window;
                if let Some(mc) = lw.text_edit_manager.multi_cursor.as_mut() {
                    let _ = mc.remove_selection(*selection_id);
                    lw.text_edit_manager.mark_dirty();
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }
            CallbackChange::SetSelectAllRange { target: _, range } => {
                if let Some(mc) = self.layout_window.text_edit_manager.multi_cursor.as_mut() {
                    mc.set_single_range(*range);
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::ProcessTextSelectionClick { position, time_ms } => {
                self.layout_window
                    .process_mouse_click_for_selection(*position, *time_ms);
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::ScrollActiveCursorIntoView => {
                self.layout_window.scroll_selection_into_view(
                    azul_layout::window::SelectionScrollType::Cursor,
                    azul_layout::window::ScrollMode::Instant,
                );
                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            // === Cursor blink STATE (the blink TIMER is a separate story, below) ===
            CallbackChange::SetCursorVisibility { visible } => {
                let lw = &mut self.layout_window;
                lw.text_edit_manager.blink.set_visibility(*visible);
                if let Some(dom_id) = lw.text_edit_manager.get_editing_dom_id() {
                    lw.regenerate_display_list_for_dom(dom_id);
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }
            CallbackChange::ToggleCursorVisibility => {
                let now = self.now();
                let lw = &mut self.layout_window;
                if lw.text_edit_manager.blink.should_blink(&now) {
                    lw.text_edit_manager.blink.toggle_visibility();
                } else {
                    lw.text_edit_manager.blink.set_visibility(true);
                }
                if let Some(dom_id) = lw.text_edit_manager.get_editing_dom_id() {
                    lw.regenerate_display_list_for_dom(dom_id);
                }
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            }
            CallbackChange::ResetCursorBlink => {
                let now = self.now();
                self.layout_window
                    .text_edit_manager
                    .blink
                    .reset_blink_on_input(now);
                ProcessEventResult::DoNothing
            }

            // === Drag & drop payload (the GESTURE that starts a drag is input) ===
            CallbackChange::SetDragData { mime_type, data } => {
                if let Some(ctx) = self
                    .layout_window
                    .gesture_drag_manager
                    .get_drag_context_mut()
                {
                    if let Some(node_drag) = ctx.as_node_drag_mut() {
                        node_drag
                            .drag_data
                            .set_data(mime_type.clone(), data.clone());
                    }
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::AcceptDrop => {
                if let Some(ctx) = self
                    .layout_window
                    .gesture_drag_manager
                    .get_drag_context_mut()
                {
                    if let Some(node_drag) = ctx.as_node_drag_mut() {
                        node_drag.drop_accepted = true;
                    }
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::SetDropEffect { effect } => {
                if let Some(ctx) = self
                    .layout_window
                    .gesture_drag_manager
                    .get_drag_context_mut()
                {
                    if let Some(node_drag) = ctx.as_node_drag_mut() {
                        node_drag.drop_effect = *effect;
                    }
                }
                ProcessEventResult::DoNothing
            }

            // ── NOT SUPPORTED HEADLESSLY ────────────────────────────────────
            //
            // Everything below needs a facility this host does not have. Each
            // one FAILS THE SCENARIO by name (see `Runner::unsupported`) instead
            // of being dropped on the floor: a change that is silently ignored
            // produces a test that executes nothing and PASSES, which in a
            // generated corpus is indistinguishable from a real pass and would
            // certify thousands of scenarios that never ran.
            //
            // The variants are listed EXPLICITLY, with no `_` arm, so that a new
            // `CallbackChange` in `layout/src/callbacks.rs` is a COMPILE ERROR
            // here and forces a decision: port it (preferred — the reference is
            // `dll/src/desktop/shell2/common/event.rs::apply_user_change`) or
            // declare it unsupported.

            // Synthetic pointer input. `click` / `double_click` / `drag` all
            // land here: a SEQUENCE of window states that has to be applied ONE
            // AT A TIME, each with its own hit test and its own state-diff pass
            // — collapsing them to "the last one wins" would leave only the
            // button-RELEASED state and no MouseDown would ever exist (the same
            // transient-input bug documented in `Runner::service`).
            CallbackChange::QueueWindowStateSequence { states } => {
                let mut result = ProcessEventResult::DoNothing;
                for queued_state in states {
                    let old = self.window_state.clone();
                    self.previous_window_state = Some(old.clone());

                    // The DLL copies exactly these fields (not the whole
                    // state): the queued states are built from a clone of the
                    // current state, so anything else would be a no-op copy.
                    {
                        let current = &mut self.window_state;
                        current.mouse_state = queued_state.mouse_state;
                        current.keyboard_state = queued_state.keyboard_state.clone();
                        current.title = queued_state.title.clone();
                        current.size = queued_state.size;
                        current.position = queued_state.position;
                        current.flags = queued_state.flags;
                    }
                    // Not in the DLL's arm, which has no equivalent bookkeeping:
                    // this host caches rasterisations per size/DPI, so a queued
                    // size change has to invalidate them or the frame keeps the
                    // old scale (`ModifyWindowState` does the same).
                    if self.window_state.size.dimensions != old.size.dimensions {
                        self.resize_pending = true;
                    }
                    if self.window_state.size.dpi != old.size.dpi {
                        self.dpi_pending = true;
                    }

                    if let Some(pos) = queued_state.mouse_state.cursor_position.get_position() {
                        self.update_hit_test_at(pos);
                    }

                    result = result.max(self.process_window_events(0));
                }
                result
            }
            CallbackChange::RequestHitTestUpdate { position } => {
                self.update_hit_test_at(*position);
                ProcessEventResult::DoNothing
            }
            CallbackChange::InjectNativeGesture { .. } => {
                self.unsupported("InjectNativeGesture", "no platform gesture source")
            }

            // Accessibility action, i.e. what a screen reader asks for.
            //
            // PORT of the DLL's arm, which routes through
            // `PlatformWindow::dispatch_accessibility_actions`: apply the action
            // to the managers, THEN dispatch the synthetic events it mapped to.
            // Doing only the first half is the exact bug the DLL shipped once —
            // AT-SPI `do_action` was accepted, decoded to the right node, and
            // then invoked no callback at all — so this host does both halves or
            // the port is worthless.
            //
            // Not `unsupported`: nothing here needs a platform. The whole action
            // path lives in `LayoutWindow`, which this host owns.
            CallbackChange::PerformAccessibilityAction {
                dom_id,
                node_id,
                action,
            } => {
                use azul_core::events::{
                    EventData, EventFilter, EventSource, EventType, FocusEventFilter,
                    HoverEventFilter, KeyModifiers, MouseButton, MouseEventData, SyntheticEvent,
                };

                let affected = self.layout_window.process_accessibility_action(
                    *dom_id,
                    *node_id,
                    action.clone(),
                    azul_core::task::Instant::now(),
                );

                // NOT gated on `affected.is_empty()`. Focus / Blur / the
                // Scroll* family / SetTextSelection all mutate manager state and
                // map to NO callback, so their affected map is empty while the
                // screen is genuinely stale — which is why every platform
                // backend calls `request_redraw()` unconditionally after a
                // batch. `ShouldReRenderCurrentWindow` is this host's equivalent.
                {
                    let timestamp = self.now();
                    let mut events = Vec::new();
                    for (node, (filters, _needs_relayout)) in &affected {
                        // Synthetic pointer events carry the node's centre so a
                        // callback reading the cursor position sees an in-bounds
                        // point (same choice the DLL makes).
                        let centre = self.layout_window.get_node_layout_rect(*node).map_or(
                            LogicalPosition { x: 0.0, y: 0.0 },
                            |r| LogicalPosition {
                                x: r.origin.x + r.size.width / 2.0,
                                y: r.origin.y + r.size.height / 2.0,
                            },
                        );
                        let mouse_data = || {
                            EventData::Mouse(MouseEventData {
                                position: centre,
                                button: MouseButton::Left,
                                buttons: 0,
                                modifiers: KeyModifiers::default(),
                                ..Default::default()
                            })
                        };
                        for f in filters {
                            let (event_type, data) = match f {
                                EventFilter::Hover(HoverEventFilter::MouseUp)
                                | EventFilter::Focus(FocusEventFilter::MouseUp) => {
                                    (EventType::MouseUp, mouse_data())
                                }
                                EventFilter::Hover(HoverEventFilter::MouseDown)
                                | EventFilter::Focus(FocusEventFilter::MouseDown) => {
                                    (EventType::MouseDown, mouse_data())
                                }
                                _ => continue,
                            };
                            events.push(SyntheticEvent::new(
                                event_type,
                                EventSource::Synthetic,
                                *node,
                                timestamp.clone(),
                                data,
                            ));
                        }
                    }

                    // The action already moved focus / scroll / cursor state, so
                    // the frame is stale even when it mapped to no callback.
                    let mut result = ProcessEventResult::ShouldReRenderCurrentWindow;
                    if !events.is_empty() {
                        let (r, _update, _) = self.dispatch_events_propagated(&events);
                        result = result.max(r);
                    }
                    result
                }
            }

            // === Timers ===
            //
            // Port of the DLL's four arms. There, `lw.timers.insert(..)` records
            // the timer and the platform trait's `start_timer` arms the OS
            // wakeup that will get the loop back to `process_timers_and_threads`.
            // This host has no OS: `LayoutWindow::timers` IS the registry and
            // [`Runner::pump_timers`] IS the loop, so the insert alone is the
            // whole job. Time comes from `Instant::now()`, which honours the
            // thread-scoped test clock the `tick_ms` op advances, so a timer
            // fires when the SCENARIO says it does — no sleeping, no race.
            //
            // HOW THE FIRST TWO ARE REACHED FROM A SCENARIO. `AddTimer` /
            // `RemoveTimer` are produced by `CallbackInfo::add_timer` /
            // `remove_timer`, an APP-callback API — and a scenario is HTML + CSS
            // + ops, so it cannot install the Rust `TimerCallback` fn pointer an
            // `AddTimer` carries. The `add_timer` / `remove_timer` DEBUG OPS
            // (`DebugEvent::AddTimer` / `RemoveTimer` in `full.rs`) close that
            // gap: they build a timer around a callback the e2e module itself
            // owns and push it through the same two `CallbackInfo` methods a
            // real app calls, so these arms run for real.
            // `e2e/op-add-remove-timer.json` is the guard.
            CallbackChange::AddTimer { timer_id, timer } => {
                self.layout_window.add_timer(*timer_id, timer.clone());
                ProcessEventResult::DoNothing
            }
            CallbackChange::RemoveTimer { timer_id } => {
                self.layout_window.remove_timer(timer_id);
                ProcessEventResult::DoNothing
            }
            CallbackChange::StartCursorBlinkTimer => {
                use azul_core::task::CURSOR_BLINK_TIMER_ID;
                // Idempotent, like the DLL's arm: re-arming an already-running
                // blink would reset `last_run` and stall the caret forever under
                // a stream of input events.
                if !self
                    .layout_window
                    .text_edit_manager
                    .blink
                    .is_blink_timer_active()
                {
                    self.layout_window
                        .text_edit_manager
                        .blink
                        .set_blink_timer_active(true);
                    let window_state = self.window_state.clone();
                    let timer = self.layout_window.create_cursor_blink_timer(&window_state);
                    self.layout_window.add_timer(CURSOR_BLINK_TIMER_ID, timer);
                }
                ProcessEventResult::DoNothing
            }
            CallbackChange::StopCursorBlinkTimer => {
                use azul_core::task::CURSOR_BLINK_TIMER_ID;
                if self
                    .layout_window
                    .text_edit_manager
                    .blink
                    .is_blink_timer_active()
                {
                    self.layout_window
                        .text_edit_manager
                        .blink
                        .set_blink_timer_active(false);
                }
                self.layout_window.remove_timer(&CURSOR_BLINK_TIMER_ID);
                ProcessEventResult::DoNothing
            }

            // No thread pump: nothing polls thread writebacks.
            CallbackChange::AddThread { .. } => self.unsupported(
                "AddThread",
                "no thread pump — the writeback would never run",
            ),
            CallbackChange::RemoveThread { .. } => {
                self.unsupported("RemoveThread", "no thread pump")
            }

            // No OS integration.
            CallbackChange::SetCopyContent { .. } => {
                self.unsupported("SetCopyContent", "no OS clipboard")
            }
            CallbackChange::SetCutContent { .. } => {
                self.unsupported("SetCutContent", "no OS clipboard")
            }
            CallbackChange::CreateNewWindow { .. } => {
                self.unsupported("CreateNewWindow", "single-window host")
            }
            CallbackChange::SetSystemAudioTakeover { .. } => {
                self.unsupported("SetSystemAudioTakeover", "no system audio")
            }
            CallbackChange::SetPointerLock { locked } => {
                // No pointer to grab headlessly, but the FLAG is the thing
                // `RawMouseMotion` is gated on, so honouring it here is what
                // lets a scenario exercise raw motion at all.
                self.layout_window
                    .current_window_state
                    .mouse_state
                    .is_cursor_locked = *locked;
                ProcessEventResult::DoNothing
            }
            CallbackChange::BeginInteractiveMove => {
                self.unsupported("BeginInteractiveMove", "no window manager")
            }
            CallbackChange::OpenMenu { .. } => self.unsupported("OpenMenu", "no native menu host"),
            // Pointer capture is pure engine state: mirror the DLL arm.
            CallbackChange::CapturePointer { node, seat_id } => {
                self.layout_window.pointer_capture =
                    Some(crate::managers::hover::PointerCapture {
                        seat_id: *seat_id,
                        node: *node,
                    });
                ProcessEventResult::DoNothing
            }
            CallbackChange::ReleasePointerCapture => {
                self.layout_window.pointer_capture = None;
                ProcessEventResult::DoNothing
            }
            // The hold is engine state too; the popup window it leads to is a
            // second platform window the headless runner does not create, but
            // the manager's bookkeeping (and a scenario asserting on it) works.
            CallbackChange::SetTransientWindowOpen { node, open } => {
                let Some(node_id) = node.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                if node.dom != DomId::ROOT_ID {
                    return ProcessEventResult::DoNothing;
                }
                if self
                    .layout_window
                    .transient_windows
                    .set_forced_open(node_id, *open)
                {
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow
                } else {
                    ProcessEventResult::DoNothing
                }
            }
            CallbackChange::SetTransientWindowTorn { node, torn } => {
                let Some(node_id) = node.node.into_crate_internal() else {
                    return ProcessEventResult::DoNothing;
                };
                if node.dom != DomId::ROOT_ID {
                    return ProcessEventResult::DoNothing;
                }
                if self.layout_window.set_transient_window_torn(node_id, *torn) {
                    ProcessEventResult::ShouldRegenerateDomCurrentWindow
                } else {
                    ProcessEventResult::DoNothing
                }
            }
            CallbackChange::PickScreenColor => {
                // The request is recorded (a scenario can assert the widget
                // asked); there is no screen to read headless, so the answer
                // is an immediate "cancelled" on the next pass.
                let id = self.layout_window.eyedropper_manager.begin_request();
                crate::managers::eyedropper::push_result(
                    crate::managers::eyedropper::EyedropperResult {
                        request_id: id,
                        color: None,
                    },
                );
                ProcessEventResult::DoNothing
            }
            CallbackChange::ShowTooltip { .. } => {
                self.unsupported("ShowTooltip", "tooltips are a second platform window")
            }
            CallbackChange::HideTooltip => {
                self.unsupported("HideTooltip", "tooltips are a second platform window")
            }

            // css-id registrations go through the content chokepoint into the
            // LayoutWindow's OWN ImageCache (the single authority) — the DL
            // build resolves `background-image: url(...)` against it, so the
            // returned tier makes the change visible NOW (the old handler was
            // `unsupported`, and the DLL's was `DoNothing`).
            CallbackChange::AddImageToCache { id, image } => {
                let result = self.layout_window.apply_content_change(
                    crate::overlay::ContentChange::ImageById {
                        id: id.clone(),
                        image: Some(image.clone()),
                    },
                );
                result.tier.to_process_event_result()
            }
            CallbackChange::RemoveImageFromCache { id } => {
                let result = self.layout_window.apply_content_change(
                    crate::overlay::ContentChange::ImageById {
                        id: id.clone(),
                        image: None,
                    },
                );
                result.tier.to_process_event_result()
            }

            // No app data / undo manager: the runner's `RefAny` app data is `()`,
            // so a snapshot or an undo would restore nothing.
            // The runner owns its LayoutWindow directly, and every arm here
            // returns a ProcessEventResult.
            CallbackChange::PlayHaptic { request } => {
                self.layout_window.haptic_manager.play_request(*request);
                // No repaint: a haptic changes nothing on screen.
                ProcessEventResult::DoNothing
            }
            CallbackChange::RequestSoftKeyboard { visible } => {
                self.layout_window
                    .text_edit_manager
                    .request_soft_keyboard(*visible);
                ProcessEventResult::DoNothing
            }
            CallbackChange::SetNowPlaying { info } => {
                // Recorded, never published: the runner is headless and has no
                // session bus. Recording it anyway is what lets a test assert
                // that a callback published what it meant to.
                self.layout_window.media_session_manager.set(info.clone());
                // No repaint: the media widget is not part of this window.
                ProcessEventResult::DoNothing
            }
            CallbackChange::SetRemoteSelections { owner, selections } => {
                if let Some(mc) = self.layout_window.text_edit_manager.multi_cursor.as_mut() {
                    mc.set_owner_selections(*owner, selections.as_ref());
                }
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::SetSelectionOwnerColor { owner, color } => {
                self.layout_window
                    .text_edit_manager
                    .set_owner_color(*owner, color.clone().into_option());
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
            CallbackChange::CommitUndoSnapshot => {
                self.unsupported("CommitUndoSnapshot", "no app-data undo manager")
            }
            CallbackChange::UndoAppState => {
                self.unsupported("UndoAppState", "no app-data undo manager")
            }
            CallbackChange::RedoAppState => {
                self.unsupported("RedoAppState", "no app-data undo manager")
            }

            // Text input. `process_text_input` records the changeset and
            // reports the affected nodes; the host then dispatches one `Input`
            // event per node and only THEN applies the changeset, so an
            // `On::Input` callback observes the pre-edit text exactly as it
            // does in the DLL. Applying only the first half would edit the text
            // while no callback ever fired.
            CallbackChange::WheelInput { delta_x, delta_y } => {
                // Port of the DLL arm: the platform wheel ingress against the
                // hover hit test, then the physics timer applies the queued
                // input. This runner has no shell to arm the timer at a pass
                // tail, so it is armed here when the queue was idle.
                use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                use crate::managers::hover::InputPointId;
                use crate::managers::scroll_state::{ScrollInputDevice, ScrollInputSource};
                let now = self.now();
                let lw = &mut self.layout_window;
                let recorded = lw.scroll_manager.record_scroll_from_hit_test(
                    *delta_x,
                    *delta_y,
                    ScrollInputSource::WheelDiscrete,
                    ScrollInputDevice::MouseWheel,
                    &lw.hover_manager,
                    &InputPointId::Mouse,
                    now,
                );
                if recorded.is_some() && !lw.timers.contains_key(&SCROLL_MOMENTUM_TIMER_ID) {
                    use crate::scroll_timer::{scroll_physics_timer_callback, ScrollPhysicsState};
                    use crate::timer::{Timer, TimerCallbackType};
                    let physics = lw
                        .system_style
                        .as_ref()
                        .map(|s| s.scroll_physics.clone())
                        .unwrap_or_default();
                    let interval_ms = physics.timer_interval_ms.max(1);
                    let state =
                        ScrollPhysicsState::new(lw.scroll_manager.get_input_queue(), physics);
                    let timer = Timer::create(
                        azul_core::refany::RefAny::new(state),
                        scroll_physics_timer_callback as TimerCallbackType,
                        self.system_callbacks.get_system_time_fn,
                    )
                    .with_interval(azul_core::task::Duration::System(
                        azul_core::task::SystemTimeDiff::from_millis(u64::from(interval_ms)),
                    ));
                    self.layout_window.add_timer(SCROLL_MOMENTUM_TIMER_ID, timer);
                }
                self.process_window_events(0)
            }
            CallbackChange::CreateTextInput { text } => {
                let affected_nodes = self.layout_window.process_text_input(text.as_str());
                if affected_nodes.is_empty() {
                    return ProcessEventResult::DoNothing;
                }

                let now = self.now();
                let text_events: Vec<_> = affected_nodes
                    .keys()
                    .map(|dom_node_id| {
                        azul_core::events::SyntheticEvent::new(
                            azul_core::events::EventType::Input,
                            azul_core::events::EventSource::User,
                            *dom_node_id,
                            now.clone(),
                            azul_core::events::EventData::None,
                        )
                    })
                    .collect();

                let mut result = ProcessEventResult::DoNothing;
                let (text_changes_result, text_update, text_prevent_default) =
                    self.dispatch_events_propagated(&text_events);
                // A callback veto kills the recorded edit — same as the DLL:
                // clearing it also stops any later apply from landing it late.
                if text_prevent_default {
                    self.layout_window.text_input_manager.clear_changeset();
                    return result.max(text_changes_result);
                }
                result = result.max(text_changes_result);
                if matches!(
                    text_update,
                    azul_core::callbacks::Update::RefreshDom
                        | azul_core::callbacks::Update::RefreshDomAllWindows
                ) {
                    result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                }

                let changeset_result = self.layout_window.apply_text_changeset();
                if !changeset_result.dirty_nodes.is_empty() {
                    result = result.max(if changeset_result.needs_relayout {
                        ProcessEventResult::ShouldIncrementalRelayout
                    } else {
                        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                    });
                    self.layout_window.scroll_selection_into_view(
                        azul_layout::window::SelectionScrollType::Cursor,
                        azul_layout::window::ScrollMode::Instant,
                    );
                }
                result
            }

            // The runner mounts XML documents; it never invokes a layout
            // callback, which is the only thing a route switch changes.
            CallbackChange::SwitchRoute { .. } => {
                self.unsupported("SwitchRoute", "no layout callback — the runner mounts XML")
            }
        }
    }

    /// Port of `PlatformWindow::apply_capi_delete`
    /// (`dll/src/desktop/shell2/common/event.rs`) — the `DeleteBackward` /
    /// `DeleteForward` arms, routed onto the SAME path Backspace and Delete
    /// take.
    ///
    /// This host used to carry the PRE-FIX body the DLL deleted: primary
    /// CURSOR only via `text3::edit::delete_backward` / `delete_forward`, so a
    /// Range selection was invisible to it (it deleted one grapheme next to the
    /// selection's cursor instead of the selection), nothing was recorded for
    /// undo, and the caret kept blinking through the edit. A scenario that
    /// deleted through this host therefore validated semantics no shell has.
    fn apply_capi_delete(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        forward: bool,
    ) -> ProcessEventResult {
        let target = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };
        let now = self.now();
        let lw = &mut self.layout_window;
        if lw.delete_selection(target, forward).is_none() {
            return ProcessEventResult::DoNothing;
        }
        lw.text_edit_manager.blink.reset_blink_on_input(now);
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    }

    /// Shared body of the eight `MoveCursor*` arms (port of the DLL's, which are
    /// the same call with a different closure).
    fn move_cursor(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        extend_selection: bool,
        f: impl FnOnce(
            &azul_layout::text3::cache::UnifiedLayout,
            &azul_core::selection::TextCursor,
        ) -> azul_core::selection::TextCursor,
    ) -> ProcessEventResult {
        let lw = &mut self.layout_window;
        if let Some(new_cursor) = lw.move_cursor_in_node(dom_id, node_id, f) {
            lw.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
        }
        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    /// Record a `CallbackChange` this host cannot apply faithfully, and FAIL the
    /// scenario for it (`run_e2e_test` turns a non-empty list into a red test).
    ///
    /// This is deliberately not a `log_warn` and not a `DoNothing`: an ignored
    /// change makes a scenario that exercises nothing report the same "pass" as
    /// one that exercised everything.
    fn unsupported(&mut self, variant: &str, why: &str) -> ProcessEventResult {
        self.unsupported_changes.push(format!(
            "e2e runner: CallbackChange::{variant} is not supported by the headless runner \
             ({why}) — this scenario cannot be executed faithfully (port the arm from \
             dll/src/desktop/shell2/common/event.rs::apply_user_change)"
        ));
        ProcessEventResult::DoNothing
    }

    /// Port of `common::layout::regenerate_layout` + the headless backend's
    /// render/damage tail: refresh the font snapshot, install the pending mount
    /// document (or keep the already-mounted, possibly-mutated DOM), re-run
    /// layout and render a frame.
    fn regenerate_layout(&mut self) {
        self.refresh_font_snapshot();

        self.layout_window.sync_frame_report();
        self.layout_window.frame_report.dom_regenerations = self
            .layout_window
            .frame_report
            .dom_regenerations
            .saturating_add(1);

        // E2E `mount` override: replace the DOM wholesale with the test's inline
        // XML+CSS document, but ONLY when the mount is dirty — otherwise keep the
        // already-mounted DOM (with any debug DOM mutations applied to it).
        let mount_change = self
            .layout_window
            .e2e_mount
            .take_dirty()
            .then(|| self.layout_window.e2e_mount.xml().map(str::to_string));
        // Whether this pass produces a genuinely NEW tree. Only then is there
        // anything to reconcile: the `None` arm below reuses the SAME
        // `StyledDom` (it is taken back out of `layout_results`), so diffing it
        // against itself would be meaningless work.
        let mut is_new_tree = false;
        let styled_dom = match mount_change {
            Some(Some(xml)) => match azul_layout::xml::parse_xml_to_styled_dom(&xml) {
                Ok(sd) => {
                    is_new_tree = true;
                    Some(sd)
                }
                Err(_) => None,
            },
            Some(None) => {
                // `unmount`: drop the mounted document entirely.
                self.layout_window.layout_results.clear();
                self.cpu_backend.previous_display_list = None;
                self.resize_pending = false;
                self.dpi_pending = false;
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
            self.dpi_pending = false;
            return;
        };

        // A DPI or size change invalidates every cached rasterisation and every
        // shaped run measured at the old scale.
        if self.resize_pending || self.dpi_pending {
            self.layout_window.clear_caches();
            self.resize_pending = false;
            self.dpi_pending = false;
        }

        // Step 3.4 of `regenerate_layout`: re-run inheritance + rebuild the
        // compact cache on the composed tree.
        styled_dom.recompute_inheritance_and_compact_cache();

        // RECONCILE. This runner is a hand-port of the desktop
        // `regenerate_layout` and, until now, omitted this step entirely — so
        // nothing keyed on `node_moves` (state transfer, manager remap,
        // CSS-override migration, animation) was exercised by ANY headless
        // test. Both implementations now call the same pair.
        let now = self.now();
        let pending = if is_new_tree {
            Some(
                self.layout_window
                    .begin_reconciliation(DomId::ROOT_ID, &mut styled_dom, now),
            )
        } else {
            None
        };

        self.layout(styled_dom, true);

        // Last geometry exists now, so pairs complete and enters start.
        if let Some(pending) = pending {
            self.layout_window
                .finish_reconciliation(DomId::ROOT_ID, &pending);

            // REBUILD the display list once, if anything started animating.
            //
            // Ordering makes this unavoidable: the display list is produced by
            // `layout_and_generate_display_list`, but a FLIP cannot be computed
            // until AFTER layout (it needs Last geometry), so the list above was
            // built while no animation keys existed — and the builder only emits
            // `PushReferenceFrame` for a node that HAS a key. Without this pass
            // the per-frame transform updates have nothing to drive and the
            // element jumps to its destination instead of travelling there.
            //
            // Once per transition START, not per frame: every subsequent frame
            // is a pure GPU-key update, which is the property the whole design
            // rests on.
            if !self.layout_window.animations.is_empty() {
                // DISPLAY LIST only — the solved layout is reused untouched.
                // `regenerate_display_list_for_dom` also hands the builder the
                // GPU value cache, which is what lets it see the animation keys
                // minted a moment ago and emit the reference frames.
                self.layout_window
                    .regenerate_display_list_for_dom(DomId::ROOT_ID);
            }
        }

        // Parity with the DLL's `regenerate_layout` tail: a focus parked
        // before the FIRST layout is applied right after the layout that made
        // it resolvable, so the caret is seeded in this very frame.
        if self.layout_window.focus_manager.has_deferred_focus_target() {
            self.layout_window.finalize_pending_focus_changes();
        }

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
        // The resize fast path lands here: the pending size change is consumed
        // by re-laying-out the existing StyledDom at the new
        // `self.window_state` size — WITHOUT `clear_caches()`. Keeping the
        // warm shaping/intrinsics caches is the entire point of the fast path
        // (shaping depends on font size and DPI, not on the viewport); DPI
        // changes never come through here (always a full regeneration).
        self.resize_pending = false;
        if let Some(layout_result) = self.layout_window.layout_results.remove(&DomId::ROOT_ID) {
            self.layout(layout_result.styled_dom, false);
        }
        self.render_and_record();
    }

    /// CPU-render the current frame and publish its damage onto the
    /// `LayoutWindow`, where `CallbackInfo::get_layout_window()` — and therefore
    /// an E2E assertion — can see it.
    fn render_and_record(&mut self) {
        // The shells drain the queued VirtualView re-invocations right before
        // every frame (`drain_virtual_view_updates`, dll/.../common/layout.rs):
        // an edge approach seen by the ScrollTo arm, a `trigger_virtual_view_rerender`
        // from a callback. This host never did — a `wheel` past a view's edge
        // queued a re-materialization that no frame ran, so the pages stayed
        // frozen on their first window in E2E while the shells moved on.
        // (The re-materialized child's list is built by the invoke itself and
        // the parent's item is re-pointed by the drain; the hit tester is
        // re-derived at the end of `service()`.)
        self.layout_window.process_pending_virtual_view_updates(
            &self.window_state,
            &self.renderer_resources,
            &self.system_callbacks,
        );

        // The scrollbar thumb transform and fade opacity live in the GPU value
        // cache, which the WebRender builders refresh every frame and the CPU
        // path has to refresh by hand. `LayoutWindow::refresh_scrollbar_gpu_cache_for_cpu_frame`
        // says so in its own doc comment ("before `CpuBackend::render_frame`"),
        // and ALL SEVEN DLL platform loops call it — this host did not. So the
        // cache was only ever advanced by a full relayout: `scrollbar_fade_active`
        // never became true, `has_gpu_damage` never became true from a fade, and
        // NO SCROLLBAR FADE WAS OBSERVABLE IN E2E AT ALL. The `full.rs:5285`
        // leak check for "an idle scrollbar'd window re-presenting forever"
        // could not fire either.
        //
        // `prepare_frame_cpu` is the shared per-frame content preparation
        // (journal frame clock + RenderImageCallback invocation through the
        // content chokepoint + that scrollbar refresh). Before it existed this
        // host never invoked image callbacks at all — every callback image
        // rendered as the announced grey placeholder in E2E.
        let gpu_cache_moved = self.layout_window.prepare_frame_cpu();

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

        // "If any scrollbar is actively fading (0 < opacity < 1), schedule
        // another frame so the fade-out animation runs to completion." — the
        // tail of every DLL present path, ported. See `Runner::pending_redraw`.
        //
        // `gpu_cache_moved` is the extra term the DLL does not need and this
        // host does: the frame that lands the fade on opacity 0.0 clears
        // `scrollbar_fade_active` and still repaints the strip the scrollbar
        // vacated, so stopping on the flag alone leaves the LAST frame carrying
        // damage. A shell does not care (nothing asks it whether it settled);
        // an idleness assertion reads exactly that frame. One more frame after
        // the last change is what makes "settled" observable.
        self.pending_redraw =
            self.layout_window.gpu_state_manager.scrollbar_fade_active || gpu_cache_moved;

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
    /// FocusNext/Previous, Escape → ClearFocus. Runs once per pass that saw a
    /// `KeyDown`, which is the DLL's `has_key_event` gate.
    ///
    /// Returns `(result, focus_changed)`; the caller uses `focus_changed` to
    /// decide whether to dispatch Blur/Focus and re-enter the pass.
    ///
    /// Runs once per SEAT that has a `KeyDown` in `synthetic_events`, in
    /// arrival order (9b-ii-a-i-d-v-a): each seat's own keyboard state and
    /// own focus drive its action, the same loop the dll shell runs. Before
    /// this the runner read only the primary's keyboard and focus, so a seat's
    /// Tab in a scenario did nothing (the seat's keycode lives in ITS keyboard
    /// state, which the primary-only read never saw).
    fn run_keyboard_default_action(
        &mut self,
        synthetic_events: &[azul_core::events::SyntheticEvent],
    ) -> (ProcessEventResult, bool) {
        let mut seats: Vec<u64> = Vec::new();
        for e in synthetic_events
            .iter()
            .filter(|e| e.event_type == azul_core::events::EventType::KeyDown)
        {
            let seat = azul_layout::managers::hover::seat_of_event(e);
            if !seats.contains(&seat) {
                seats.push(seat);
            }
        }
        let mut result = ProcessEventResult::DoNothing;
        let mut changed = false;
        for seat in seats {
            let (r, c) = self.run_keyboard_default_action_for_seat(seat);
            result = result.max(r);
            changed |= c;
        }
        (result, changed)
    }

    /// One seat's slice of [`Self::run_keyboard_default_action`]. The
    /// returned `focus_changed` is only ever `true` for the PRIMARY seat: the
    /// pass's Blur/Focus re-entry reads the primary's focus, and a seat's
    /// focus move has no Blur/Focus dispatch in the runner (the dll's seat
    /// drain owns that; 9b-ii-a-i-d-iii is the open styling/a11y half).
    fn run_keyboard_default_action_for_seat(&mut self, seat: u64) -> (ProcessEventResult, bool) {
        use azul_core::events::DefaultAction;
        use azul_layout::default_actions::{
            default_action_to_focus_target, determine_keyboard_default_action_with_editing,
        };
        use azul_layout::managers::focus_cursor::resolve_focus_target;

        let is_primary = seat == azul_core::window::PRIMARY_POINTER_SEAT;
        let ks = if is_primary {
            self.window_state.keyboard_state.clone()
        } else {
            self.window_state
                .keyboard_seat(seat)
                .cloned()
                .unwrap_or_else(|| self.window_state.keyboard_state.clone())
        };
        let focused = self.layout_window.focus_manager.focused_node_for(seat);
        let editing_state = self
            .layout_window
            .build_editing_query_state_for_seat(seat, focused);
        let action = determine_keyboard_default_action_with_editing(
            &ks,
            focused,
            &self.layout_window.layout_results,
            false,
            editing_state.as_ref(),
        );
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_ACT_DEBUG").is_some() {
            std::eprintln!(
                "[act] seat={} key={:?} focused={:?} action={:?}",
                seat, ks.current_virtual_keycode, focused, action.action
            );
        }
        if !action.has_action() {
            return (ProcessEventResult::DoNothing, false);
        }

        match &action.action {
            // The DIRECTIONAL four were missing here (9a-i-b-i): the shells
            // apply every focus action through the same resolver, but the
            // headless runner listed only the Tab family, so an arrow that
            // the decision function had already resolved to a focus move did
            // nothing in a scenario - the spatial-navigation corpus could
            // never have gone green, or red.
            DefaultAction::FocusNext
            | DefaultAction::FocusPrevious
            | DefaultAction::FocusFirst
            | DefaultAction::FocusLast
            | DefaultAction::FocusUp
            | DefaultAction::FocusDown
            | DefaultAction::FocusLeft
            | DefaultAction::FocusRight => {
                let Some(target) = default_action_to_focus_target(&action.action) else {
                    return (ProcessEventResult::DoNothing, false);
                };
                let Ok(resolved) =
                    resolve_focus_target(
                        &target,
                        &self.layout_window.layout_results,
                        focused,
                        &self.layout_window.focus_out_of_scope_doms(),
                    )
                else {
                    return (ProcessEventResult::DoNothing, false);
                };
                // Tab with nothing tabbable is a MISS, not a clear — keep the
                // current focus (mirrors the dll shell arm).
                use azul_layout::managers::focus_cursor::FocusResolution;
                let new_focus = match resolved {
                    FocusResolution::Resolved(n) => Some(n),
                    FocusResolution::ClearRequested => None,
                    FocusResolution::NotFound | FocusResolution::Deferred => {
                        return (ProcessEventResult::DoNothing, false);
                    }
                };
                if new_focus == focused {
                    return (ProcessEventResult::DoNothing, false);
                }
                if !is_primary {
                    return (self.set_seat_focus(seat, new_focus), false);
                }
                // `:focus-visible`: this is the KEYBOARD route, so the ring
                // shows. Mirrors the dll shell's arm - the runner is a port of
                // it, and a modality set in only one of the two would make the
                // harness disagree with the device about whether focus is
                // indicated.
                (self.set_focus(new_focus, focused, true), true)
            }
            DefaultAction::ClearFocus => {
                if focused.is_none() {
                    return (ProcessEventResult::DoNothing, false);
                }
                if !is_primary {
                    return (self.set_seat_focus(seat, None), false);
                }
                (self.set_focus(None, focused, false), true)
            }
            DefaultAction::InsertLineBreakAtCursor { target } => {
                // Same as the DLL shells: plain-text Enter records a literal
                // "\n" and applies it directly (the apply tail already ran
                // this pass). Veto is honored by the !prevent_default gate
                // around default actions.
                if let Some(node_id) = target.node.into_crate_internal() {
                    let old_inline = self
                        .layout_window
                        .get_text_before_textinput(target.dom, node_id);
                    let old_text = self
                        .layout_window
                        .extract_text_from_inline_content(&old_inline);
                    use crate::managers::text_input::TextInputSource;
                    self.layout_window.text_input_manager.record_input(
                        *target,
                        "\n".to_string(),
                        old_text,
                        TextInputSource::Keyboard,
                    );
                    let changeset_result = self.layout_window.apply_text_changeset();
                    let mut r = ProcessEventResult::DoNothing;
                    if !changeset_result.dirty_nodes.is_empty() {
                        r = if changeset_result.needs_relayout {
                            ProcessEventResult::ShouldIncrementalRelayout
                        } else {
                            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                        };
                        self.layout_window.scroll_selection_into_view(
                            azul_layout::window::SelectionScrollType::Cursor,
                            azul_layout::window::ScrollMode::Instant,
                        );
                    }
                    // Applied outside the record pipeline's event window —
                    // owe the host its Input dispatch (drained at pass tail).
                    self.layout_window
                        .text_edit_manager
                        .pending_edit_notifications
                        .push(*target);
                    (r, false)
                } else {
                    (ProcessEventResult::DoNothing, false)
                }
            }
            DefaultAction::SplitBlockAtCursor { .. }
            | DefaultAction::MergeWithPrevious { .. }
            | DefaultAction::MergeWithNext { .. } => {
                // Same one-liner as the DLL shells: structural edits record;
                // a materialized preview paints on the next relayout.
                if self
                    .layout_window
                    .record_structural_default_action_for_seat(seat, &action.action)
                    .is_some()
                {
                    (ProcessEventResult::ShouldIncrementalRelayout, false)
                } else {
                    (ProcessEventResult::DoNothing, false)
                }
            }
            // ACTIVATION (Enter / Space on a focused element). Dispatches the
            // SAME synthetic Click the dll shell sends, which
            // `matches_hover_filter` maps onto `HoverEventFilter::MouseUp` -
            // the very filter the accessibility path already resolves
            // `AccessibilityAction::Default` to (window.rs). Keyboard, screen
            // reader and pointer activation therefore all reach one listener
            // set. Without this arm the runner fell through to DoNothing, so
            // the headless harness could not see the device bug at all.
            DefaultAction::ActivateFocusedElement { target } => {
                let click = azul_core::events::SyntheticEvent::new(
                    azul_core::events::EventType::Click,
                    azul_core::events::EventSource::User,
                    *target,
                    self.now(),
                    azul_core::events::EventData::None,
                );
                let (r, _update, _) = self.dispatch_events_propagated(&[click]);
                (r, false)
            }
            _ => (ProcessEventResult::DoNothing, false),
        }
    }

    /// Publish layout's scroll containers into the ScrollManager.
    ///
    /// This used to be a hand-maintained PORT of the dll's copy, so a scroll
    /// bug could be fixed in one host and left standing in the other. Both
    /// call the same function now.
    fn register_scroll_nodes(&mut self) {
        let now = self.now();
        crate::managers::scroll_registration::register_scroll_nodes(&mut self.layout_window, &now);
    }
}

/// `:seat-focus` restyle for a NON-primary seat (9b-ii-a-i-d-iii-a), the port
/// of the DLL's `apply_seat_focus_restyle`: the node that lost this seat's
/// focus drops the pseudo-class unless another seat still focuses it, the
/// node that gained it takes it.
fn apply_seat_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<DomNodeId>,
    new_focus: Option<DomNodeId>,
) -> ProcessEventResult {
    use azul_core::diff::ChangeAccumulator;

    let lost = old_focus
        .filter(|n| layout_window.focus_manager.seats_focusing(n).iter().all(|s| *s == 0))
        .and_then(|n| n.node.into_crate_internal());
    let gained = new_focus.and_then(|n| n.node.into_crate_internal());
    if lost.is_none() && gained.is_none() {
        return ProcessEventResult::DoNothing;
    }
    let Some((_, layout_result)) = layout_window.layout_results.iter_mut().next() else {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    };
    let restyle_result = layout_result.styled_dom.restyle_on_seat_focus_change(lost, gained);
    if restyle_result.changed_nodes.is_empty() || restyle_result.gpu_only_changes {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    }
    let mut accumulator = ChangeAccumulator::new();
    accumulator.merge_restyle_result(&restyle_result);
    if accumulator.needs_layout() {
        ProcessEventResult::ShouldIncrementalRelayout
    } else if accumulator.needs_paint_only() {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    } else {
        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}

/// Port of the DLL's `apply_focus_restyle` (`.../common/event.rs`): apply the
/// `:focus` / `:focus-within` state change to the styled DOM and classify how
/// much work the resulting property deltas need.
///
/// Without this a click (or a Tab) moved focus but left the node painted
/// unfocused until the next full DOM regeneration.
fn apply_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<NodeId>,
    new_focus: Option<NodeId>,
) -> ProcessEventResult {
    use azul_core::{diff::ChangeAccumulator, styled_dom::FocusChange};

    let Some((_, layout_result)) = layout_window.layout_results.iter_mut().next() else {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    };

    let restyle_result = layout_result.styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: old_focus,
            gained_focus: new_focus,
        }),
        None, // hover
        None, // active
    );

    if restyle_result.changed_nodes.is_empty() || restyle_result.gpu_only_changes {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    }

    let mut accumulator = ChangeAccumulator::new();
    accumulator.merge_restyle_result(&restyle_result);
    if accumulator.needs_layout() {
        ProcessEventResult::ShouldIncrementalRelayout
    } else if accumulator.needs_paint_only() {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    } else {
        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}

/// Port of the caret half of the DLL's `SystemChange::SetFocus` /
/// `CallbackChange::SetFocusTarget` handling.
///
/// `handle_focus_change_for_cursor_blink` is what FLAGS a contenteditable focus
/// for caret initialisation; `finalize_pending_focus_changes` (already called at
/// the end of every pass) is what turns that flag into a real cursor. The runner
/// called only the second one, so the flag was never set, no cursor was ever
/// created, and `text_input` went through `record_text_input` (which only needs a
/// focused node) into `apply_text_changeset` (which needs a CURSOR) and produced
/// zero dirty nodes — a silent no-op with focus in place.
///
/// The returned `CursorBlinkTimerAction` is HONOURED: this host now has a timer
/// driver ([`Runner::pump_timers`]), so focusing a contenteditable really
/// registers `CURSOR_BLINK_TIMER_ID` and leaving one really removes it. The
/// action used to be dropped on the floor with "no timer driver, the caret is
/// drawn steady instead of blinking", which made caret blink untestable.
///
/// The DLL splits this over two seams — the platform trait's
/// `start_timer` / `stop_timer` arm the OS wakeup, and
/// `CallbackChange::StartCursorBlinkTimer` is what inserts the `Timer` into
/// `LayoutWindow::timers`. Here the two are the same thing: `timers` IS the
/// driver, so `Start` inserts and `Stop` removes, exactly as the DLL's
/// `StartCursorBlinkTimer` / `StopCursorBlinkTimer` arms do.
fn arm_caret_for_focus(
    layout_window: &mut LayoutWindow,
    new_focus: Option<DomNodeId>,
    window_state: &FullWindowState,
) {
    use azul_core::task::CURSOR_BLINK_TIMER_ID;
    use azul_layout::CursorBlinkTimerAction;

    match layout_window.handle_focus_change_for_cursor_blink(new_focus, window_state) {
        CursorBlinkTimerAction::Start(timer) => {
            layout_window.add_timer(CURSOR_BLINK_TIMER_ID, timer);
        }
        CursorBlinkTimerAction::Restart(timer) => {
            layout_window.remove_timer(&CURSOR_BLINK_TIMER_ID);
            layout_window.add_timer(CURSOR_BLINK_TIMER_ID, timer);
        }
        CursorBlinkTimerAction::Stop => {
            layout_window.remove_timer(&CURSOR_BLINK_TIMER_ID);
        }
        CursorBlinkTimerAction::NoChange => {}
    }
}

/// Port of the DLL's `apply_hover_restyle` (`.../common/event.rs`): apply this
/// pass's MouseEnter / MouseLeave targets to the styled DOM so pure-CSS
/// `:hover` rules take effect without a DOM regeneration.
fn apply_hover_restyle(
    layout_window: &mut LayoutWindow,
    changes_per_dom: BTreeMap<DomId, azul_core::styled_dom::HoverChange>,
) -> ProcessEventResult {
    use azul_core::diff::ChangeAccumulator;

    let mut result = ProcessEventResult::DoNothing;
    for (dom_id, hover_change) in changes_per_dom {
        let Some(layout_result) = layout_window.layout_results.get_mut(&dom_id) else {
            continue;
        };
        let restyle_result =
            layout_result
                .styled_dom
                .restyle_on_state_change(None, Some(hover_change), None);
        if restyle_result.changed_nodes.is_empty() {
            continue;
        }
        let r = if restyle_result.gpu_only_changes {
            ProcessEventResult::ShouldReRenderCurrentWindow
        } else {
            let mut accumulator = ChangeAccumulator::new();
            accumulator.merge_restyle_result(&restyle_result);
            if accumulator.needs_layout() {
                ProcessEventResult::ShouldIncrementalRelayout
            } else if accumulator.needs_paint_only() {
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            } else {
                ProcessEventResult::ShouldReRenderCurrentWindow
            }
        };
        result = result.max(r);
    }
    result
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
    run_e2e_test_with_dom(test, None)
}

/// [`run_e2e_test`] on a document built in Rust: `initial_dom` is laid out
/// BEFORE the first step runs, in place of the empty body a scenario
/// without a `mount` op starts from. The steps then drive that tree through
/// the same ops (`click`, `text_input`, ...) a JSON scenario uses — which is
/// how the widgets (whose DOM no HTML `mount` can express: they carry
/// datasets, callbacks and empty text nodes) get exercised end to end.
pub fn run_e2e_test_with_dom(test: &E2eTest, initial_dom: Option<StyledDom>) -> E2eTestResult {
    run_e2e_test_keeping_runner(test, initial_dom).0
}

/// [`run_e2e_test_with_dom`], handing the finished [`Runner`] back so a test
/// can inspect what the scenario left on screen (the display list, the
/// managers) beyond what the ops themselves assert.
fn run_e2e_test_keeping_runner(
    test: &E2eTest,
    initial_dom: Option<StyledDom>,
) -> (E2eTestResult, Runner) {
    if std::env::var_os("AZ_ANIM_DEBUG").is_some() {
        eprintln!("[scenario] {}", test.name);
    }
    // Start this scenario on a clean clock. The `tick_ms` / `wait` ops advance a
    // clock scoped to the calling thread, and worker threads are reused across
    // scenarios — without this reset the next scenario scheduled onto this
    // thread would start with the previous one's accumulated offset.
    azul_core::task::reset_test_clock();
    // ...and then STOP real time for this thread, so engine time is a pure
    // function of the ops this scenario runs. Otherwise elapsed time is
    // (what the scenario asked for) + (what this build, under this load, spent
    // computing), and the suite runs scenarios 8-wide: that second term is large
    // and varies run to run, which is enough to flip an assertion on a blinking
    // caret's phase while the same scenario passes 10/10 in isolation.
    //
    // Only the ENGINE clock stops. The harness keeps measuring itself with
    // `wall_clock_now()`, so reported step durations stay real.
    azul_core::task::freeze_test_clock();

    // This scenario's own scheduler slot. It is a LOCAL, not a `Runner` field,
    // only because `Runner::with_callback_info` takes `&mut self` and the
    // dispatcher needs `&mut` on the session at the same time — borrowck, not
    // ambient state. It has exactly the lifetime of this run.
    let mut session = E2eSession::new();

    let (w, h, dpi, animations) = match &test.setup {
        Some(s) => (
            s.window_width as f32,
            s.window_height as f32,
            s.dpi,
            s.animations,
        ),
        None => (800.0, 600.0, 96, false),
    };
    let mut runner = Runner::new(w, h, dpi, animations);
    if let Some(dom) = initial_dom {
        // The mount-less `regenerate_layout` arm takes the CURRENT tree back
        // out of `layout_results`, so laying the document out once here is
        // all it takes for every later pass to keep it.
        runner.layout(dom, true);
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let request = DebugRequest {
        request_id: 1,
        event: DebugEvent::RunE2eTests {
            tests: vec![test.clone()],
            snapshots: None,
        },
        window_id: None,
        wait_for_render: false,
        dom_id: None,
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

        // `resume_not_before` is never set to `Some` anywhere in the tree: a
        // `wait` yields with no deadline and advances the injectable clock
        // instead, so scenario time is a pure function of the ops a scenario ran
        // rather than of how fast the build is.
        //
        // This used to `std::thread::sleep` to the deadline. That is now
        // unreachable, and leaving it would be a landmine: the moment anything
        // repopulated the field the whole suite would silently go back to being
        // pinned to realtime — the exact regression that made
        // `bug_font_never_removed` red only on unoptimized builds. It would also
        // reintroduce a `std::time::Instant::now()` here, which panics on
        // wasm32.
        //
        // So it fails loudly instead. If you are here because this fired, the
        // fix is to advance the test clock (`advance_test_clock_ms`), not to
        // sleep.
        assert!(
            resume_not_before.is_none(),
            "e2e runner: scenario '{}' asked to resume at a wall-clock deadline. Scenario time \
             is virtual — advance the injectable clock instead of sleeping, or the suite is \
             pinned to realtime again.",
            test.name,
        );
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

    let result = match rx.try_recv() {
        Ok(DebugResponseData::Ok {
            data: Some(ResponseData::E2eResults(r)),
            ..
        }) => r
            .results
            .into_iter()
            .next()
            .unwrap_or_else(|| fail_result(test, "RunE2eTests returned no results")),
        Ok(DebugResponseData::Ok { .. }) => {
            fail_result(test, "RunE2eTests returned a non-E2eResults response")
        }
        Ok(DebugResponseData::Err(e)) => fail_result(test, &e),
        Err(_) => fail_result(test, "RunE2eTests produced no response"),
    };

    // A scenario that asked the engine for something this host cannot do is
    // RED, no matter what its assertions said: they were evaluated against a
    // window where that something never happened. Reported per unsupported
    // change, by name — see `Runner::unsupported`.
    let result = unsupported_to_failure(result, &runner.unsupported_changes);
    (result, runner)
}

/// Fold the runner's unsupported-change log into the scenario result, turning a
/// pass that skipped work into a named failure.
fn unsupported_to_failure(mut result: E2eTestResult, unsupported: &[String]) -> E2eTestResult {
    if unsupported.is_empty() {
        return result;
    }
    // Deduplicate: one line per distinct facility, not one per applied change.
    let mut seen: Vec<&String> = Vec::new();
    for u in unsupported {
        if !seen.contains(&u) {
            seen.push(u);
        }
    }
    let next_index = result.steps.len();
    for (i, message) in seen.iter().enumerate() {
        result.steps.push(E2eStepResult {
            step_index: next_index + i,
            op: "unsupported_callback_change".to_string(),
            status: "fail".to_string(),
            duration_ms: 0,
            logs: Vec::new(),
            screenshot: None,
            error: Some((*message).clone()),
            response: None,
        });
    }
    result.status = "fail".to_string();
    result.steps_failed += seen.len();
    result.step_count = result.steps.len();
    result
}

// ── Un-fork pins ─────────────────────────────────────────────────────────────
//
// This host is a PORT of the shells, not a second implementation, so every
// place it re-derived behaviour instead of calling the engine is a place where
// a scenario could go green on semantics no user has. These pin the three that
// had actually drifted.

#[cfg(test)]
mod tests {
    use azul_core::{
        callbacks::{CaretTweenInfo, Update},
        dom::{Dom, NodeId as CoreNodeId},
        events::EventFilter,
        geom::LogicalRect,
        refany::RefAny,
        selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor},
        task::{advance_test_clock_ms, freeze_test_clock, reset_test_clock, Duration},
        window::{VirtualKeyCode, VirtualKeyCodeVec},
    };
    use azul_layout::{
        callbacks::{CallbackInfo, CallbackType},
        solver3::display_list::DisplayListItem,
    };

    use super::*;

    /// body = 0, div (contenteditable) = 1, text = 2.
    const EDITOR: usize = 1;

    const CSS: &str = "* { margin: 0; padding: 0; } \
                       body { font-size: 16px; width: 600px; }";

    fn cursor(byte: u32) -> azul_core::selection::TextCursor {
        TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: byte,
            },
            affinity: CursorAffinity::Leading,
        }
    }

    fn editor_node() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(EDITOR))),
        }
    }

    /// A runner with one contenteditable div laid out and an editing session on
    /// it — the shape every text scenario mounts.
    fn editor_runner(content: &str, animations: bool, on_key_down: Option<CallbackType>) -> Runner {
        reset_test_clock();
        freeze_test_clock();

        let mut editor = Dom::create_div().with_contenteditable(true).with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(content),
        );
        if let Some(cb) = on_key_down {
            editor = editor.with_callback(
                EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
                RefAny::new(()),
                cb as usize,
            );
        }
        let mut dom = Dom::create_body().with_child(editor);
        let (css, _) = azul_css::parser2::new_from_str(CSS);
        let styled_dom = StyledDom::create(&mut dom, css);

        let mut runner = Runner::new(800.0, 600.0, 96, animations);
        runner.layout(styled_dom, true);
        runner
            .layout_window
            .focus_manager
            .set_focused_node(Some(editor_node()));
        runner.layout_window.text_edit_manager.initialize_editing(
            cursor(0),
            DomId::ROOT_ID,
            NodeId::new(EDITOR),
            0,
        );
        runner
            .layout_window
            .text_edit_manager
            .blink
            .set_visibility(true);
        runner
            .layout_window
            .regenerate_display_list_for_dom(DomId::ROOT_ID);
        runner
    }

    fn text_of(runner: &Runner) -> String {
        let content = runner
            .layout_window
            .get_text_before_textinput(DomId::ROOT_ID, NodeId::new(EDITOR));
        runner
            .layout_window
            .extract_text_from_inline_content(&content)
    }

    /// The LAST `CursorRect` item = the primary caret (the rule the tween
    /// post-pass itself uses).
    fn caret_rect(runner: &Runner) -> LogicalRect {
        runner
            .layout_window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .rev()
            .find_map(|item| match item {
                DisplayListItem::CursorRect { bounds, .. } => Some(bounds.0),
                _ => None,
            })
            .expect("the display list carries a caret")
    }

    fn no_changes() -> Arc<Mutex<Vec<CallbackChange>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    /// Press a printable key the way a shell does: RECORD the text into the
    /// changeset first, then run the state-diff pass (this is what the
    /// `key_down` op's `text` parameter drives).
    fn press_key_with_text(runner: &mut Runner, key: VirtualKeyCode, text: &str) {
        use azul_layout::managers::text_input::PendingTextEdit;

        let focused = runner
            .layout_window
            .focus_manager
            .get_focused_node()
            .copied()
            .expect("a focused node");
        let node_id = focused.node.into_crate_internal().expect("a real node");
        let old_inline = runner
            .layout_window
            .get_text_before_textinput(focused.dom, node_id);
        let old_text = runner
            .layout_window
            .extract_text_from_inline_content(&old_inline);
        let _ = runner.apply_user_change(&CallbackChange::SetTextChangeset {
            changeset: PendingTextEdit {
                node: focused,
                inserted_text: text.into(),
                old_text: old_text.into(),
            },
        });

        let mut state = runner.window_state.clone();
        state.keyboard_state.current_virtual_keycode = Some(key).into();
        state.keyboard_state.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(vec![key]);
        let _ = runner.apply_user_change(&CallbackChange::ModifyWindowState { state });
    }

    extern "C" fn veto_key_down(_data: RefAny, mut info: CallbackInfo) -> Update {
        info.prevent_default();
        Update::DoNothing
    }

    extern "C" fn observe_key_down(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    // ── 1. The C-API delete arms ─────────────────────────────────────────────

    #[test]
    fn capi_delete_backward_deletes_the_whole_selection() {
        let mut runner = editor_runner("hello world", false, None);
        runner
            .layout_window
            .text_edit_manager
            .multi_cursor
            .as_mut()
            .expect("editing session")
            .set_single_range(SelectionRange {
                start: cursor(0),
                end: cursor(6),
            });

        let _ = runner.apply_user_change(&CallbackChange::DeleteBackward {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(EDITOR),
        });

        // The pre-fix body deleted ONE grapheme at the range's cursor
        // (`get_primary_cursor` answers `range.end`), leaving "hellworld".
        assert_eq!(
            text_of(&runner),
            "world",
            "the whole range goes, not one grapheme at the primary cursor"
        );
    }

    #[test]
    fn capi_delete_forward_deletes_the_whole_selection() {
        let mut runner = editor_runner("hello world", false, None);
        runner
            .layout_window
            .text_edit_manager
            .multi_cursor
            .as_mut()
            .expect("editing session")
            .set_single_range(SelectionRange {
                start: cursor(0),
                end: cursor(6),
            });

        let _ = runner.apply_user_change(&CallbackChange::DeleteForward {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(EDITOR),
        });

        assert_eq!(text_of(&runner), "world");
    }

    #[test]
    fn capi_delete_records_undo_and_holds_the_caret_solid() {
        let mut runner = editor_runner("hello world", false, None);
        // A caret, not a range: the undo record and the blink reset are owed to
        // every delete, not only to the selection case.
        runner
            .layout_window
            .text_edit_manager
            .multi_cursor
            .as_mut()
            .expect("editing session")
            .set_single_cursor(cursor(5));
        runner
            .layout_window
            .text_edit_manager
            .blink
            .set_visibility(false);
        assert!(
            !runner
                .layout_window
                .undo_redo_manager
                .can_undo(CoreNodeId::new(EDITOR)),
            "premise: nothing is undoable before the delete"
        );

        let _ = runner.apply_user_change(&CallbackChange::DeleteBackward {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(EDITOR),
        });

        assert_eq!(text_of(&runner), "hell world");
        assert!(
            runner
                .layout_window
                .undo_redo_manager
                .can_undo(CoreNodeId::new(EDITOR)),
            "a delete is an undoable edit — the pre-fix body recorded nothing"
        );
        assert!(
            runner.layout_window.text_edit_manager.blink.is_visible,
            "editing keeps the caret solid, same as typing"
        );
    }

    // ── 2. Tweens are reachable, and deterministic on the virtual clock ──────

    #[test]
    fn animations_off_lands_the_caret_immediately() {
        let mut runner = editor_runner("hello world", false, None);
        let before = caret_rect(&runner);

        let _ = runner.apply_user_change(&CallbackChange::MoveCursor {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(EDITOR),
            cursor: cursor(6),
        });

        assert!(
            runner.layout_window.text_edit_manager.tween.caret.is_none(),
            "`setup.animations` defaults to off, so no tween is ever armed"
        );
        assert!(
            (caret_rect(&runner).origin.x - before.origin.x).abs() > 1.0,
            "premise: byte 6 is a different x from byte 0"
        );
    }

    #[test]
    fn animations_on_tweens_the_caret_on_the_virtual_clock() {
        const STEP_MS: u64 = 20;
        const DURATION_MS: u64 = 60;

        let mut runner = editor_runner("hello world", true, None);
        let changes = no_changes();
        let from = caret_rect(&runner);

        let _ = runner.apply_user_change(&CallbackChange::MoveCursor {
            dom_id: DomId::ROOT_ID,
            node_id: NodeId::new(EDITOR),
            cursor: cursor(6),
        });

        let track = runner
            .layout_window
            .text_edit_manager
            .tween
            .caret
            .clone()
            .expect("`setup.animations: true` arms the caret tween");
        assert_eq!(track.from, from, "the tween starts from the RENDERED rect");
        let to = track.to;
        assert!(
            (to.origin.x - from.origin.x).abs() > 1.0,
            "premise: the caret really moved"
        );
        assert_eq!(
            caret_rect(&runner),
            from,
            "at t = 0 the caret is still painted where it was — the move is a glide, not a jump"
        );

        // The driver timer is armed by the frame tail, exactly like the shells'.
        runner.service(&changes, false);
        assert!(
            runner
                .layout_window
                .timers
                .contains_key(&azul_core::task::CARET_TWEEN_TIMER_ID),
            "an in-flight tween arms its 16ms driver"
        );

        // Fixed steps on the FROZEN clock: the geometry at step k is a pure
        // function of k, so this asserts a number, not a race.
        for step in 1..=2u64 {
            let _ = advance_test_clock_ms(STEP_MS);
            runner.service(&changes, false);

            let t = Duration::from_millis(STEP_MS * step).div(&Duration::from_millis(DURATION_MS));
            let expected = (azul_core::resources::SystemAnimations::default()
                .caret_tween
                .cb)(
                RefAny::new(()),
                CaretTweenInfo {
                    past: from,
                    current: to,
                    t,
                },
            );
            assert_eq!(
                caret_rect(&runner),
                expected,
                "step {step}: the caret sits at the interpolator's answer for t = {t}"
            );
            assert_ne!(caret_rect(&runner), to, "step {step}: still in flight");
        }

        let _ = advance_test_clock_ms(STEP_MS);
        runner.service(&changes, false);
        assert_eq!(
            caret_rect(&runner),
            to,
            "the tween lands exactly on the target at its duration"
        );
        assert!(
            runner.layout_window.text_edit_manager.tween.caret.is_none(),
            "and retires itself"
        );
    }

    // ── 3. One text ingress: the shells' record-then-pass ────────────────────

    #[test]
    fn the_shell_ingress_lands_the_text() {
        let mut runner = editor_runner("ab", false, Some(observe_key_down));

        press_key_with_text(&mut runner, VirtualKeyCode::X, "X");

        assert_eq!(
            text_of(&runner),
            "Xab",
            "text recorded before the pass is applied BY the pass — the stage this host \
             did not have"
        );
    }

    /// The op wiring, end to end through `process_debug_event`: `key_down`
    /// carrying `text` must record BEFORE the pass and the pass must land it.
    /// The tests above drive `apply_user_change` directly and so cannot see a
    /// mistake in the op itself.
    #[test]
    fn the_key_down_op_types_through_the_shell_ingress() {
        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "key_down_text_ingress",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "mount",
                  "html": ["<div id=\"ed\" contenteditable=\"true\">ab</div>"],
                  "css": ["html, body { margin: 0; padding: 0; }",
                          "body { font-size: 24px; color: black; background: white; }",
                          "#ed { width: 300px; height: 60px; background: white; }"] },
                { "op": "wait_frame" },
                { "op": "focus_node", "selector": "#ed" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "x", "text": "X" },
                { "op": "key_up", "key": "x" },
                { "op": "wait_frame" },
                { "op": "assert_text", "selector": "#ed", "expected": "abX" }
            ]
        }))
        .expect("scenario json");

        let result = run_e2e_test(&test);
        assert_eq!(
            result.status, "pass",
            "the key_down text ingress must type at the caret focus_node seeded (end of text): {:#?}",
            result.steps
        );
    }

    #[test]
    fn a_keydown_veto_kills_the_recorded_text() {
        let mut runner = editor_runner("ab", false, Some(veto_key_down));

        press_key_with_text(&mut runner, VirtualKeyCode::X, "X");

        assert_eq!(
            text_of(&runner),
            "ab",
            "a KeyDown callback's prevent_default() vetoes the insertion, exactly as on a \
             real platform"
        );
        assert!(
            runner
                .layout_window
                .text_input_manager
                .get_pending_changeset()
                .is_none(),
            "the vetoed record dies now — surviving into the next pass would land it late"
        );
    }

    // ── 4. The real TextInput widget, through the real ops ───────────────────

    /// THE CLASS (AzWidgets 2026-08-22, "the TextInput shows a caret but
    /// typing paints nothing"): the widget is `div[contenteditable] >
    /// [p.placeholder (contenteditable=false), p.value > ""]`, focused by a
    /// CLICK (the FocusReceived path seeds the caret) and typed into through
    /// the shells' text ingress. The engine's buffer is the value `<p>`'s text
    /// — the placeholder is walled off — and the RENDERED side must follow:
    /// the typed glyph is painted, the placeholder (hidden by the widget's own
    /// TextInput callback) is not, and the value reads back.
    /// The `:focus` border is an INLINE conditional property
    /// (`CssPropertyWithConditions::on_focus`, #4286f4) - a focus restyle
    /// reports `changed_nodes=0` for it by design and relies on the
    /// display-list rebuild resolving the flag. Pin that the rebuild really
    /// paints it, and that blurring removes it.
    #[test]
    fn clicking_the_text_input_paints_the_focus_border() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "text_input_focus_border",
            "setup": { "window_width": 600, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-native-text-input-container" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");

        let (result, runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(result.status, "pass", "{:#?}", result.steps);

        // The per-side colour types are distinct wrappers
        // (Option<CssPropertyValue<StyleBorder*Color>>) and ColorU's Debug
        // prints hex, so compare on the Debug rendering of the sides -
        // #4286f4 is a colour nothing else in the demo uses.
        let has_focus_border = runner
            .layout_window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .any(|item| matches!(item, DisplayListItem::Border { colors, .. }
                if format!("{colors:?}").contains("#4286f4")));
        assert!(
            has_focus_border,
            "after clicking, the container must paint its :focus border (#4286f4)"
        );
    }

    /// A mousedown on nothing focusable BLURS. The click-to-focus block was
    /// acquisition-only, so the TextInput kept focus, caret, blink timer and
    /// editing session after a click that left it (device report 2026-08-25,
    /// re-reported 2026-08-31).
    #[test]
    fn clicking_outside_a_focused_text_input_blurs_it() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "text_input_blur_on_click_outside",
            "setup": { "window_width": 600, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-native-text-input-container" },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "__azul-native-text-input-container" },
                // Far away from the field, on bare body.
                { "op": "click", "x": 590.0, "y": 190.0 },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "\"has_focus\":false" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "a click on bare body must blur the TextInput: {:#?}",
            result.steps
        );
    }

    #[test]
    fn pressing_tab_with_no_focus_lands_on_the_first_text_input() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "tab_focuses_first_input",
            "setup": { "window_width": 600, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "\"has_focus\":true" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "Tab with nothing focused must focus the first tab stop (the \
             contenteditable TextInput container): {:#?}",
            result.steps
        );
    }

    #[test]
    fn clicking_the_empty_right_half_of_a_text_input_seats_a_caret() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        // The placeholder text is ~130px wide; x=500 is far past it, inside
        // the empty right half of the 600px field. The DEVICE bug: such a
        // click focused the field but never seated/painted a caret - only a
        // click ON the glyphs did.
        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "empty_area_click_seats_caret",
            "setup": { "window_width": 600, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "x": 500.0, "y": 13.0 },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "\"has_focus\":true" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"has_cursor\":true" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "a click in the empty part of the field must focus it AND seat a \
             visible caret: {:#?}",
            result.steps
        );
    }

    #[test]
    fn tabbing_into_a_filled_input_seats_the_caret_at_the_end() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create().with_text("42".into()).dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        // THE DEVICE BUG (2026-08-31): tabbing into the filled NumberInput
        // showed '4|2' - the mid-pass finalize burned its retry budget on
        // transient layout absence and locked in the (0,0)+Trailing seed.
        // End-of-text for "42" is the last cluster (byte 1), Trailing.
        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "tab_seats_caret_at_end",
            "setup": { "window_width": 600, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"position\":1" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"affinity\":\"trailing\"" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "Tab into a filled input must seat the caret at the END of the \
             text, never mid-text: {:#?}",
            result.steps
        );
    }

    #[test]
    fn clicking_a_text_area_on_its_placeholder_starts_the_caret() {
        use azul_layout::widgets::text_area::TextArea;

        let widget = TextArea::create()
            .with_placeholder("Multi-line text area...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 300px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        // THE DEVICE BUG (2026-08-31): the prompt used to be an overlay <p>
        // INSIDE the editable host, so a click that landed on the prompt text
        // hit that node - not the value line - and no caret session started
        // ("clicking the textarea while the placeholder is there doesn't start
        // the cursor blink"). The prompt is an engine-painted attribute now,
        // so there is no node there to hit: the click reaches the editable.
        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "text_area_click_on_prompt_starts_caret",
            "setup": { "window_width": 600, "window_height": 300, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                // Directly over where the prompt's glyphs paint.
                { "op": "click", "x": 40.0, "y": 12.0 },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "\"has_focus\":true" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"has_cursor\":true" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"blink_timer_active\":true" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "a click on the placeholder text must focus the area AND start a \
             blinking caret: {:#?}",
            result.steps
        );
    }

    #[test]
    fn typing_then_tabbing_into_a_filled_input_still_seats_the_caret_at_the_end() {
        use azul_layout::widgets::text_input::TextInput;

        // Two fields: an empty one to type into, then a FILLED one to tab to.
        let mut dom = Dom::create_body()
            .with_child(TextInput::create().dom())
            .with_child(TextInput::create().with_text("42".into()).dom());
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; height: 300px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        // THE DEVICE BUG (2026-08-31, second report): typing into the first
        // field and THEN tabbing lands the caret MID-TEXT in the second one
        // ("4|2"), i.e. at the byte offset the PREVIOUS field's session held,
        // instead of at the end of the new field's own text.
        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "type_then_tab_seats_caret_at_end",
            "setup": { "window_width": 600, "window_height": 300, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "x", "text": "x" },
                { "op": "key_up", "key": "x" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"position\":1" },
                { "op": "get_cursor_state" },
                { "op": "assert_response", "contains": "\"affinity\":\"trailing\"" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "after typing, Tab must seat the caret at the END of the next \
             field's own text, not at the previous session's byte offset: {:#?}",
            result.steps
        );
    }

    /// KEYBOARD ACCESSIBILITY, end to end through the headless window:
    /// Tab must reach a non-text widget and Space must ACTIVATE it (W3C:
    /// Space activates buttons and toggles, Enter activates buttons/links).
    ///
    /// Asserts a REAL CALLBACK FIRED, not merely that "something changed" -
    /// an `assert_changed` here passed while the device did nothing at all,
    /// because a focus restyle counts as a change. The bug it now covers:
    /// activation dispatches a synthetic `EventType::Click`, which matched
    /// no `HoverEventFilter::MouseUp` listener, so every widget ignored
    /// Enter and Space.
    /// A POINTER CLICK must activate a button, exactly once.
    ///
    /// The keyboard half is covered by
    /// `tab_then_space_activates_a_button_through_its_callback`; this is the
    /// mouse half, and it is the one that broke when widgets moved from
    /// `MouseUp` to `Click` (device: clicking a widget set focus but ran no
    /// handler).
    #[test]
    fn a_pointer_click_activates_a_button_exactly_once() {
        use azul_core::refany::RefAny;
        use azul_layout::widgets::button::{Button, ButtonOnClickCallbackType};

        #[derive(Debug)]
        struct Count(u32);

        extern "C" fn on_click(
            mut data: RefAny,
            _: azul_layout::callbacks::CallbackInfo,
        ) -> azul_core::callbacks::Update {
            if let Some(mut c) = data.downcast_mut::<Count>() {
                c.0 += 1;
            }
            azul_core::callbacks::Update::DoNothing
        }

        let mut count = RefAny::new(Count(0));
        let mut dom = Dom::create_body().with_child(
            Button::create("Press me".into())
                .with_on_click(count.clone(), on_click as ButtonOnClickCallbackType)
                .dom(),
        );
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "pointer_click_activates",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-native-button" },
                { "op": "wait_frame" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(result.status, "pass", "{:#?}", result.steps);
        let n = count.downcast_ref::<Count>().map_or(0, |c| c.0);
        assert_eq!(n, 1, "a click must run the handler exactly once, ran {n}x");
    }

    #[test]
    fn tab_then_space_activates_a_button_through_its_callback() {
        use azul_core::refany::RefAny;
        use azul_layout::widgets::button::Button;

        #[derive(Debug)]
        struct Fired(bool);

        extern "C" fn on_click(
            mut data: RefAny,
            _: azul_layout::callbacks::CallbackInfo,
        ) -> azul_core::callbacks::Update {
            if let Some(mut f) = data.downcast_mut::<Fired>() {
                f.0 = true;
            }
            azul_core::callbacks::Update::DoNothing
        }

        let mut fired = RefAny::new(Fired(false));
        let mut dom = Dom::create_body()
            .with_child(Button::create("Press me".into()).with_on_click(fired.clone(), on_click as azul_layout::widgets::button::ButtonOnClickCallbackType).dom());
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "keyboard_activation",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "\"has_focus\":true" },
                { "op": "key_down", "key": "Space" },
                { "op": "key_up", "key": "Space" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(result.status, "pass", "Tab must focus the button: {:#?}", result.steps);
        assert!(
            fired.downcast_ref::<Fired>().is_some_and(|f| f.0),
            "Space on a focused button must run its click callback",
        );
    }

    /// THE A11Y HALF of the same contract: a screen reader's default action
    /// must run the SAME callback keyboard activation runs.
    ///
    /// `process_accessibility_action(Default)` resolves to
    /// `EventFilter::Hover(MouseUp)`, and keyboard activation dispatches an
    /// `EventType::Click` whose planned filters now include that same generic
    /// MouseUp - so pointer, keyboard and assistive technology all converge on
    /// one listener set instead of three near-miss ones.
    #[test]
    fn an_accessibility_default_action_resolves_to_the_activation_filter() {
        use azul_core::{
            a11y::AccessibilityAction,
            dom::{DomId, NodeId},
            events::{EventFilter, HoverEventFilter},
        };
        use azul_layout::widgets::button::Button;

        let mut dom = Dom::create_body().with_child(Button::create("Press me".into()).dom());
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "a11y_default_action",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [{ "op": "wait_frame" }]
        }))
        .expect("scenario json");
        let (_result, mut runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));

        let affected = runner.layout_window.process_accessibility_action(
            DomId::ROOT_ID,
            NodeId::new(1),
            AccessibilityAction::Default,
            runner.now(),
        );

        let filters: Vec<EventFilter> = affected.values().flat_map(|(f, _)| f.clone()).collect();
        assert!(
            filters.contains(&EventFilter::Hover(HoverEventFilter::Click)),
            "the a11y default action must resolve to the same activation filter \
             keyboard activation reaches, got {filters:?}",
        );
    }

    /// THE FOCUS RING reached by the REAL keyboard path.
    ///
    /// `focus_ring_tween.rs` covers the ring itself by setting focus
    /// directly on the focus manager; this covers the integration - a plain
    /// Tab keypress through the window must leave a ring on screen, with NO
    /// author CSS anywhere. That is the half that was missing on the device:
    /// focus and Enter/Space activation both worked, but nothing said which
    /// control had focus, which looks exactly like Tab doing nothing.
    #[test]
    fn pressing_tab_leaves_a_visible_focus_ring() {
        use azul_layout::{solver3::display_list::DisplayListItem, widgets::button::Button};

        let mut dom = Dom::create_body().with_child(Button::create("Press me".into()).dom());
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "focus_ring_via_tab",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");
        let (result, mut runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(result.status, "pass", "{:#?}", result.steps);

        // The e2e runner disables system animations so screenshots are
        // deterministic, and the ring's duration lives there - so ask for the
        // DEFAULT config explicitly, which is the one shipping apps get.
        runner.layout_window.system_animations_override =
            Some(azul_core::resources::SystemAnimations::default());
        // The ring is appended LAST by the tween post-pass.
        runner
            .layout_window
            .regenerate_display_list_for_dom(DomId::ROOT_ID);
        let lr = runner
            .layout_window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("dom 0");
        let last_is_ring = matches!(
            lr.display_list.items.last(),
            Some(DisplayListItem::Border { .. })
        );
        assert!(
            runner.layout_window.focus_manager.get_focused_node().is_some(),
            "premise: Tab focused something",
        );
        assert!(
            last_is_ring,
            "Tab must leave a focus ring appended to the display list; last item was {:?}",
            lr.display_list.items.last().map(std::mem::discriminant),
        );
    }

    /// A focused Slider must respond to the arrow keys (1% of the range,
    /// 10% with Ctrl) - device report 2026-09-01: "the arrow keys for
    /// sliders do not work at all".
    #[test]
    fn tab_then_arrow_key_moves_a_slider() {
        use azul_core::refany::RefAny;
        use azul_layout::widgets::slider::{Slider, SliderOnValueChangeCallbackType, SliderState};

        #[derive(Debug)]
        struct Seen(Option<f32>);

        extern "C" fn on_change(
            mut data: RefAny,
            _: azul_layout::callbacks::CallbackInfo,
            state: SliderState,
        ) -> azul_core::callbacks::Update {
            if let Some(mut s) = data.downcast_mut::<Seen>() {
                s.0 = Some(state.value);
            }
            azul_core::callbacks::Update::DoNothing
        }

        let mut seen = RefAny::new(Seen(None));
        let mut dom = Dom::create_body().with_child(
            Slider::create(50.0, 0.0, 100.0)
                .with_on_value_change(seen.clone(), on_change as SliderOnValueChangeCallbackType)
                .dom(),
        );
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; height: 200px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "slider_arrow_keys",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Tab" },
                { "op": "wait_frame" },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "\"has_focus\":true" },
                { "op": "key_down", "key": "Right" },
                { "op": "key_up", "key": "Right" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");

        let (result, _runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(result.status, "pass", "Tab must focus the slider: {:#?}", result.steps);
        let got = seen.downcast_ref::<Seen>().and_then(|s| s.0);
        assert_eq!(
            got,
            Some(51.0),
            "Right arrow on a focused slider must step 1% of the range (50 -> 51)",
        );
    }

    #[test]
    fn typing_into_the_text_input_widget_paints_the_glyph() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 600px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "text_input_widget_types",
            "setup": { "window_width": 600, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-native-text-input-container" },
                { "op": "wait_frame" },
                { "op": "wait", "ms": 50 },
                { "op": "get_focus_state" },
                { "op": "assert_response", "contains": "__azul-native-text-input-container" },
                { "op": "snapshot_frame", "as": "focused_empty" },
                { "op": "text_input", "text": "Xy" },
                { "op": "wait_frame" },
                { "op": "wait", "ms": 50 },
                { "op": "assert_text", "selector": ".__azul-native-text-input-container", "expected": "Xy" },
                { "op": "assert_changed", "vs": "focused_empty", "min_damage_rects": 1 }
            ]
        }))
        .expect("scenario json");

        let (result, runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        assert_eq!(
            result.status, "pass",
            "typing into the TextInput widget must land in its value: {:#?}",
            result.steps
        );

        // What is on screen: every glyph run the final display list carries.
        let runs: Vec<usize> = runner
            .layout_window
            .get_layout_result(&DomId::ROOT_ID)
            .expect("layout result")
            .display_list
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayListItem::Text { glyphs, .. } => Some(glyphs.len()),
                _ => None,
            })
            .collect();
        assert!(
            runs.contains(&2),
            "the typed text `Xy` must be PAINTED as a two-glyph run: {runs:?}"
        );
        assert!(
            !runs.iter().any(|n| *n > 2),
            "the placeholder prompt must be hidden once there is text: {runs:?}"
        );
    }

    /// Build a `TextInput` widget in a laid-out headless window, type `abc` into
    /// it through the shell key ingress, and return the runner focused on it.
    ///
    /// The widget's shape is `container[contenteditable] > placeholder + value`,
    /// with the inline layout living on the VALUE child, not the focused
    /// container — the exact shape the keyboard-delete regression turns on.
    fn text_input_runner_typed_abc() -> (Runner, DomNodeId, NodeId) {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        // key_up between keystrokes: two identical consecutive key_downs are a
        // Some(k)->Some(k) diff (no fresh VirtualKeyDown), so the shell ingress
        // needs the release to re-arm — exactly what a real user does.
        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "text_input_typed_abc",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-native-text-input-container" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "a", "text": "a" }, { "op": "key_up", "key": "a" },
                { "op": "key_down", "key": "b", "text": "b" }, { "op": "key_up", "key": "b" },
                { "op": "key_down", "key": "c", "text": "c" }, { "op": "key_up", "key": "c" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");

        let (_result, runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        let focused = runner
            .layout_window
            .focus_manager
            .get_focused_node()
            .copied()
            .expect("clicking the text input must focus its container");
        let node_id = focused
            .node
            .into_crate_internal()
            .expect("the focused container has a node id");
        (runner, focused, node_id)
    }

    fn text_input_value(runner: &Runner, dom: DomId, node_id: NodeId) -> String {
        runner.layout_window.extract_text_from_inline_content(
            &runner.layout_window.get_text_before_textinput(dom, node_id),
        )
    }

    /// The direct-call regression: `apply_selection_op` with a Delete op on the
    /// focused TextInput HOST must delete a character.
    ///
    /// Before the fix, `apply_selection_op` read the inline layout from the host
    /// node and returned `false` the instant it was `None` — which it always is
    /// for a widget whose IFC lives on a value child — so keyboard Backspace /
    /// Delete / arrow keys were DEAD in every TextInput / TextArea. Mouse editing
    /// worked (hit testing resolves the child), and the C-API delete path worked
    /// (it never consulted the host's layout), so nothing caught it.
    #[test]
    fn keyboard_delete_op_deletes_inside_a_text_input_widget() {
        use azul_core::events::{SelectionDirection, SelectionMode, SelectionOp, SelectionStep};

        let (mut runner, focused, node_id) = text_input_runner_typed_abc();

        assert_eq!(
            text_input_value(&runner, focused.dom, node_id),
            "abc",
            "typing through the shell ingress must fill the value node",
        );
        // The precondition that IS the bug: the focused host block carries no
        // inline layout — the value child does. `apply_selection_op` must not
        // treat that as "nothing to edit".
        assert!(
            runner
                .layout_window
                .get_inline_layout_for_node(focused.dom, node_id)
                .is_none(),
            "precondition: the TextInput host block has no inline layout of its own",
        );

        let backspace = SelectionOp::new(
            SelectionDirection::Backward,
            SelectionStep::Character,
            SelectionMode::Delete,
        );
        assert!(
            runner.layout_window.apply_selection_op(focused, &backspace),
            "apply_selection_op(Delete) must report an edit (it used to bail on the \
             host's missing inline layout)",
        );
        assert_eq!(
            text_input_value(&runner, focused.dom, node_id),
            "ab",
            "Backspace must remove the last character of the value node",
        );

        // A range delete (select-all then Backspace) must clear it too.
        let select_all = SelectionOp::new(
            SelectionDirection::Backward,
            SelectionStep::Document,
            SelectionMode::Extend,
        );
        runner
            .layout_window
            .apply_selection_op(focused, &select_all);
        let delete_range = SelectionOp::new(
            SelectionDirection::Backward,
            SelectionStep::Character,
            SelectionMode::Delete,
        );
        runner
            .layout_window
            .apply_selection_op(focused, &delete_range);
        assert_eq!(
            text_input_value(&runner, focused.dom, node_id),
            "",
            "selecting the whole value and pressing Backspace must clear it",
        );
    }

    /// The end-to-end regression through the SAME shell ingress a real keystroke
    /// takes: a `key_down {"key":"Backspace"}` op must delete a character.
    ///
    /// This pins the unification — the e2e `key_down` op used to shortcut
    /// Backspace/Delete straight to the C-API `delete_backward()` (a DIFFERENT
    /// code path than native macOS, which routes KeyDown(Back) →
    /// `SystemChange::ApplySelectionOp` → `apply_selection_op`). That shortcut
    /// hid the dead keyboard-delete: every backspace test passed while the real
    /// path was broken. The op now drives the real path, so this test exercises
    /// exactly what the user's keyboard does.
    #[test]
    fn key_down_backspace_op_deletes_through_the_shell_ingress() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "key_down_backspace",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [
                { "op": "wait_frame" },
                { "op": "click", "selector": ".__azul-native-text-input-container" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "a", "text": "a" }, { "op": "key_up", "key": "a" },
                { "op": "key_down", "key": "b", "text": "b" }, { "op": "key_up", "key": "b" },
                { "op": "key_down", "key": "c", "text": "c" }, { "op": "key_up", "key": "c" },
                { "op": "wait_frame" },
                { "op": "key_down", "key": "Backspace" }, { "op": "key_up", "key": "Backspace" },
                { "op": "wait_frame" }
            ]
        }))
        .expect("scenario json");

        let (_result, runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        let focused = runner
            .layout_window
            .focus_manager
            .get_focused_node()
            .copied()
            .expect("clicking the text input must focus its container");
        let node_id = focused.node.into_crate_internal().expect("focused node id");
        assert_eq!(
            text_input_value(&runner, focused.dom, node_id),
            "ab",
            "a Backspace key_down must delete the last character through the real \
             ApplySelectionOp path (not the C-API shortcut the harness used to take)",
        );
    }

    /// The placeholder must not FLICKER while the window is slowly resized.
    ///
    /// User report: "Type something..." blinks during a slow drag-resize. The
    /// placeholder is an absolutely-positioned sibling of the value `<p>`, and
    /// the value `<p>` became a horizontal scroll box (`overflow-x: auto`) when
    /// the caret-reveal was fixed — so every width the resize sweeps through
    /// re-decides whether that box overflows. If any of that feeds back into
    /// whether the placeholder is laid out, it blinks.
    ///
    /// Sweeps one pixel at a time and requires the painted glyph count to be
    /// the SAME on every width: an empty field shows its prompt at 400 px and
    /// at 401 px alike.
    #[test]
    fn the_placeholder_does_not_flicker_while_the_window_resizes() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "placeholder_resize_flicker",
            "setup": { "window_width": 400, "window_height": 200, "dpi": 96 },
            "steps": [ { "op": "wait_frame" } ]
        }))
        .expect("scenario json");

        let (_r, mut runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));

        let painted = |r: &Runner| -> usize {
            r.layout_window
                .get_layout_result(&DomId::ROOT_ID)
                .map(|lr| {
                    lr.display_list
                        .items
                        .iter()
                        .map(|it| match it {
                            DisplayListItem::Text { glyphs, .. } => glyphs.len(),
                            _ => 0,
                        })
                        .sum()
                })
                .unwrap_or(0)
        };

        let baseline = painted(&runner);
        assert!(
            baseline > 0,
            "precondition: an empty field must paint its placeholder prompt",
        );

        // Slow resize: one pixel per frame, the way a drag delivers it.
        let mut seen: Vec<(f32, usize)> = Vec::new();
        for w in 380..=420 {
            let mut state = runner.window_state.clone();
            state.size.dimensions = azul_core::geom::LogicalSize::new(w as f32, 200.0);
            let _ = runner.apply_user_change(&CallbackChange::ModifyWindowState { state });
            seen.push((w as f32, painted(&runner)));
        }

        let odd: Vec<&(f32, usize)> = seen.iter().filter(|(_, n)| *n != baseline).collect();
        assert!(
            odd.is_empty(),
            "the placeholder blinked while resizing (expected {baseline} glyphs at every \
             width): {odd:?}",
        );
    }

    /// `overscroll-behavior` must travel CSS -> cascade -> ScrollManager.
    ///
    /// The `OverscrollBehavior` enum and every physics branch reading it
    /// existed all along, but nothing ever SET the two state fields — they
    /// were hardcoded to `Auto` at each construction site, and the property was
    /// not in the CSS property table at all, so `contain` and `none` were
    /// unreachable. This walks the whole path a stylesheet takes, through the
    /// same `register_scroll_nodes` the shell and the runner both use.
    fn overscroll_behavior_for(
        css: &str,
    ) -> (
        azul_css::props::style::scrollbar::OverscrollBehavior,
        azul_css::props::style::scrollbar::OverscrollBehavior,
    ) {
        // Inline `with_css` rather than a stylesheet rule: it runs the same
        // declaration parser, so this still proves parse -> CssProperty ->
        // cascade -> ScrollManager, without also depending on selector
        // matching. The child overflows the box, which is what makes the box
        // register as a scroll node at all.
        let mut dom = Dom::create_body()
            .with_css("width: 300px; height: 300px;")
            .with_child(
                Dom::create_div()
                    .with_css(css)
                    .with_child(Dom::create_div().with_css("width: 400px; height: 400px;")),
            );
        let styled_dom = StyledDom::create_from_dom(dom.clone());
        let _ = &mut dom;

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "overscroll_behavior",
            "setup": { "window_width": 300, "window_height": 300, "dpi": 96 },
            "steps": [ { "op": "wait_frame" } ]
        }))
        .expect("scenario json");

        let (_r, runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        let states = runner
            .layout_window
            .scroll_manager
            .get_scroll_states_for_dom(DomId::ROOT_ID);
        let node = *states
            .keys()
            .next()
            .expect("the overflowing box must register as a scroll node");
        let st = runner
            .layout_window
            .scroll_manager
            .get_scroll_state(DomId::ROOT_ID, node)
            .expect("a registered node has state");
        (st.overscroll_behavior_x, st.overscroll_behavior_y)
    }

    #[test]
    fn overscroll_behavior_reaches_the_scroll_manager() {
        use azul_css::props::style::scrollbar::OverscrollBehavior;
        const BOX: &str = "width: 100px; height: 100px; overflow: auto;";

        // Absent => the CSS initial value.
        let (x, y) = overscroll_behavior_for(BOX);
        assert_eq!(x, OverscrollBehavior::Auto, "default x");
        assert_eq!(y, OverscrollBehavior::Auto, "default y");

        // The SHORTHAND sets both axes.
        let (x, y) = overscroll_behavior_for(&format!("{BOX} overscroll-behavior: contain;"));
        assert_eq!(x, OverscrollBehavior::Contain, "shorthand must set x");
        assert_eq!(y, OverscrollBehavior::Contain, "shorthand must set y");

        // The LONGHANDS are independent.
        let (x, y) = overscroll_behavior_for(&format!(
            "{BOX} overscroll-behavior-x: none; overscroll-behavior-y: contain;"
        ));
        assert_eq!(x, OverscrollBehavior::None, "longhand x");
        assert_eq!(y, OverscrollBehavior::Contain, "longhand y");
    }

    /// The horizontal caret-reveal regression: typing past the right edge of a
    /// single-line `TextInput` must SCROLL the field so the caret stays visible.
    ///
    /// The container is `overflow-x: auto` + `scrollbar-width: none` so its
    /// overflowing value line makes it a horizontal scroll box the caret-reveal
    /// (`scroll_selection_into_view` → `find_scrollable_ancestor`) can shift. It
    /// regressed on macOS/Linux because those `cfg` blocks still carried
    /// `overflow-x: hidden` (only the Windows block had been fixed): the
    /// container never registered as a scroll node, the reveal found no
    /// scrollable ancestor and bailed, and every character typed past the right
    /// edge walked off-screen with the caret frozen at the last visible glyph —
    /// exactly the "text is invisible when I type" report. `justify-content`
    /// must also be `flex-start`, not `center`: a centred overflowing line puts
    /// the START of the text at negative x, which the clamp `[0, max]` can never
    /// bring back.
    #[test]
    fn typing_past_the_right_edge_scrolls_the_text_input_to_keep_the_caret_visible() {
        use azul_layout::widgets::text_input::TextInput;

        let widget = TextInput::create()
            .with_placeholder("Type something...".into())
            .dom();
        let mut dom = Dom::create_body().with_child(widget);
        // The same 400px body the `abc` helper uses (so the click lands and
        // focuses); the field is then overflowed by TYPING a long line rather
        // than by shrinking the field — which is exactly the user's scenario
        // (a normal-width input filled past its right edge).
        let (css, _) = azul_css::parser2::new_from_str(
            "* { margin: 0; padding: 0; } body { font-size: 16px; width: 400px; }",
        );
        let styled_dom = StyledDom::create(&mut dom, css);

        // ~90 printable characters, each a key_down{text}+key_up pair (two
        // identical consecutive downs are a no-op diff, so the release re-arms
        // the ingress — same reason the `abc` helper alternates down/up). At the
        // 11px value font this line is far wider than the ~396px field.
        // ~250 chars: at the 11px value font this is far wider than the ~394px
        // field, so it overflows several times over (the 88-char string used
        // earlier measured ~390px and merely GRAZED the edge — no overflow).
        let typed: String = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
            .chars()
            .cycle()
            .take(250)
            .collect();
        let mut steps = vec![
            serde_json::json!({ "op": "wait_frame" }),
            serde_json::json!({ "op": "click", "selector": ".__azul-native-text-input-container" }),
            serde_json::json!({ "op": "wait_frame" }),
        ];
        for ch in typed.chars() {
            let s = ch.to_string();
            steps.push(serde_json::json!({ "op": "key_down", "key": s, "text": s }));
            steps.push(serde_json::json!({ "op": "key_up", "key": s }));
        }
        steps.push(serde_json::json!({ "op": "wait_frame" }));

        let test: super::E2eTest = serde_json::from_value(serde_json::json!({
            "name": "text_input_horizontal_scroll_reveal",
            "setup": { "window_width": 500, "window_height": 200, "dpi": 96 },
            "steps": steps,
        }))
        .expect("scenario json");

        let (_result, mut runner) = run_e2e_test_keeping_runner(&test, Some(styled_dom));
        let focused = runner
            .layout_window
            .focus_manager
            .get_focused_node()
            .copied()
            .expect("clicking the text input must focus its container");
        let _node_id = focused.node.into_crate_internal().expect("focused node id");
        let dom = focused.dom;

        // The text3 layer of the bug, pinned directly: `position_one_line`'s
        // per-segment fit test used to DROP every item past `segment.width`
        // even for a nowrap line, so only ~one box-width of glyphs (≈64 of
        // 250) was ever positioned and the tail did not exist to scroll to.
        // Every typed character must be laid out and reach the display list.
        {
            let lr = runner
                .layout_window
                .get_layout_result(&dom)
                .expect("layout result");
            let painted: usize = lr
                .display_list
                .items
                .iter()
                .map(|item| match item {
                    DisplayListItem::Text { glyphs, .. } => glyphs.len(),
                    _ => 0,
                })
                .sum();
            assert!(
                painted >= typed.chars().count(),
                "every typed character must be positioned and painted \
                 (white-space:pre must not truncate at the box edge): \
                 painted {painted} glyphs for {} typed characters",
                typed.chars().count(),
            );
        }

        // The single-line field scrolls on the VALUE `<p>`, NOT the container:
        // the container is a block box the value box fills exactly (394 px in a
        // 400 px field), so the overflowing line lives INSIDE the value box and
        // the value box is where the scroll must register. The fix makes that
        // value `<p>` `overflow-x: auto`; before, `overflow-x: hidden` trapped
        // the line and registered no scroll box at all.
        let value_node = *runner
            .layout_window
            .scroll_manager
            .get_scroll_states_for_dom(dom)
            .keys()
            .next()
            .expect(
                "typing past the right edge must register a horizontal scroll box (the value \
                 <p> with overflow-x:auto); none registered — the append-only caret bug",
            );

        // Drive the caret-reveal exactly as a keystroke does: it anchors on the
        // editing session's node (the value <p>), walks to that scroll box and
        // shifts it so the caret stays in view. Idempotent — a re-run after the
        // harness's own reveal is a zero-delta no-op, so the assertion does not
        // hinge on frame timing. That this advances the offset at all proves the
        // reveal's anchor actually reaches the value scroll box.
        runner.layout_window.scroll_selection_into_view(
            azul_layout::window::SelectionScrollType::Cursor,
            azul_layout::window::ScrollMode::Instant,
        );

        let scroll = runner
            .layout_window
            .scroll_manager
            .get_scroll_state(dom, value_node)
            .expect("the value scroll box must carry a scroll state");
        assert!(
            scroll.content_rect.size.width > scroll.container_rect.size.width + 1.0,
            "precondition: the typed line must overflow the field (content {:.1} vs container {:.1})",
            scroll.content_rect.size.width,
            scroll.container_rect.size.width,
        );
        assert!(
            scroll.current_offset.x > 0.0,
            "typing past the right edge must scroll the value box so the caret follows the text; \
             offset.x stayed at {:.1} (append-only / caret frozen — the reveal never reached the \
             value scroll box)",
            scroll.current_offset.x,
        );
    }
}
