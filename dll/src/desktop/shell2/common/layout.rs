//! Cross-platform layout regeneration logic
//!
//! This module contains the unified layout regeneration workflow that is shared across all
//! platforms. Previously, this logic was duplicated in every platform's window implementation.
//!
//! The regenerate_layout function takes direct field references instead of using trait methods
//! to avoid borrow checker issues (similar to invoke_callbacks pattern).

use azul_layout::solver3::LayoutNodeId;
use std::{cell::RefCell, sync::Arc};

use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, LayoutCallbackInfoRefData},
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{ImageCache, RendererResources},
};
use azul_css::system::SystemStyle;
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::registry::FcFontRegistry;
use rust_fontconfig::FcFontCache;
use webrender::{RenderApi as WrRenderApi, Transaction as WrTransaction};

use super::debug_server::{self, LogCategory};
use crate::{
    desktop::{csd, wr_translate2},
    log_debug,
};
use azul_css::LayoutDebugMessage;

/// Delay in ms before scrollbar overlay starts fading out after scroll stops.
const SCROLLBAR_FADE_DELAY_MS: u64 = 500;
/// Duration in ms of the scrollbar fade-out animation.
const SCROLLBAR_FADE_DURATION_MS: u64 = 200;

/// After layout, publish every scrollable container into the ScrollManager.
///
/// The body moved to `azul_layout::managers::scroll_registration` so the e2e
/// runner and `layout/tests` run the SAME code instead of a hand-maintained
/// port and hand-seeded fixtures.
fn register_scroll_nodes(layout_window: &mut LayoutWindow) {
    let now: azul_core::task::Instant = std::time::Instant::now().into();
    azul_layout::managers::scroll_registration::register_scroll_nodes(layout_window, &now);
}

/// Publish what the layout pass just decided about scrolling into the
/// managers: which nodes are scroll containers (and how big they and their
/// content are), then the scrollbar fade opacities that follow from it.
///
/// EVERY code path that RAN A LAYOUT must call this before returning. A path
/// that lays out and skips it leaves the `ScrollManager` describing the
/// PREVIOUS tree: a scroll container that appeared in this pass (a CSS
/// breakpoint switching a widget to its compact form, content that just grew
/// past its box) has no scroll state at all, so it cannot be scrolled, its
/// thumb has no geometry, and `synchronize_scrollbar_opacity` reads a
/// last-activity time of `None` and paints the bar at opacity 0 — the bar is
/// in the display list and invisible on screen.
fn publish_scroll_state(layout_window: &mut LayoutWindow) {
    // Register scrollable nodes and calculate scrollbar states
    register_scroll_nodes(layout_window);

    // Synchronize scrollbar opacity with GPU cache.
    // Note: Display list translation happens in generate_frame(), not here —
    // this enables smooth fade-in/fade-out without display list rebuild.
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    for (dom_id, layout_result) in &layout_window.layout_results {
        LayoutWindow::synchronize_scrollbar_opacity(
            &mut layout_window.gpu_state_manager,
            &layout_window.scroll_manager,
            *dom_id,
            &layout_result.layout_tree,
            &system_callbacks,
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                SCROLLBAR_FADE_DELAY_MS,
            )),
            azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                SCROLLBAR_FADE_DURATION_MS,
            )),
        );
    }
}

/// Result of `regenerate_layout()` indicating whether the DOM structure changed.
///
/// When the DOM is structurally unchanged (same node types, hierarchy, classes,
/// IDs, inline styles, callback events), the expensive layout pipeline
/// (CSS cascade, flexbox, display list) can be skipped. Only image callbacks
/// need to be re-invoked since their content (e.g. GL textures) may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutRegenerateResult {
    /// DOM structure changed — full layout was performed
    /// (CSS cascade, flexbox, and display list were all recomputed).
    LayoutChanged,
    /// DOM structure is unchanged — layout was reused from previous frame.
    /// Image callbacks still need to be re-invoked since their content
    /// (e.g. GL textures) may have changed, but the expensive CSS cascade
    /// and flexbox passes were skipped.
    LayoutUnchanged,
}

// ---------------------------------------------------------------------------
// E2E DOM mount override
// ---------------------------------------------------------------------------
//
// The document installed by the debug-server `mount` op used to live in two
// process-global statics here. It now lives on the window it applies to
// (`LayoutWindow::e2e_mount`, written by `CallbackChange::RemountDom`), so two
// windows can no longer share one mounted DOM. `regenerate_layout` reads it
// below; see `azul_layout::window::E2eMountOverride` for the dirty-flag
// semantics (the mounted DOM is cloned forward, not re-parsed, so DOM-mutation
// ops applied to it survive a `RefreshDom`).

/// Regenerate layout after DOM changes.
///
/// This function implements the complete layout regeneration workflow:
/// 1. Invoke user's layout callback to get new DOM
/// 2. Conditionally inject Client-Side Decorations (CSD)
/// 3. Perform layout and generate display list
/// 4. Calculate scrollbar states
/// 5. Rebuild WebRender display list
/// 6. Synchronize scrollbar opacity with GPU cache
///
/// This workflow is identical across all platforms (macOS, Windows, X11, Wayland).
///
/// ## Parameters
///
/// Takes direct references to window fields to avoid borrow checker issues.
/// This is the same pattern used in `invoke_single_callback`.
///
/// ## Return Value
///
/// Returns `Ok(LayoutChanged)` if full layout was performed,
/// `Ok(LayoutUnchanged)` if the DOM was structurally unchanged and layout was reused,
/// or an error message on failure.
/// Per-phase wall-clock for one `regenerate_layout`, reported as ONE line.
///
/// `emit_phase_heap` marks the same boundaries but measures HEAP, needs the
/// `probe` feature, and says nothing about time. The 2026-08-07 mouse-resize
/// investigation needed the time split and had no way to get it: the only
/// timing was a single 654-942 ms total for the whole function.
///
/// One line per relayout rather than one per phase, because a drag produces
/// hundreds of relayouts and 20 lines each is unreadable. Costs an `Instant::now`
/// per boundary and nothing else when the gate is closed.
struct PhaseTimer {
    enabled: bool,
    start: std::time::Instant,
    last: std::time::Instant,
    marks: Vec<(&'static str, f64)>,
}

impl PhaseTimer {
    fn new() -> Self {
        let enabled = crate::desktop::shell2::common::log_gate::should_log(
            LogCategory::Window,
            azul_core::log_filter::Level::Debug,
        );
        let now = std::time::Instant::now();
        Self {
            enabled,
            start: now,
            last: now,
            marks: Vec::new(),
        }
    }

    fn mark(&mut self, name: &'static str) {
        if !self.enabled {
            return;
        }
        let now = std::time::Instant::now();
        self.marks
            .push((name, (now - self.last).as_secs_f64() * 1000.0));
        self.last = now;
    }

    /// Log the phases that actually cost something, biggest first. Phases under
    /// 0.5 ms are summed into one "rest" entry — with ~20 boundaries the noise
    /// otherwise hides the two or three that matter.
    fn report(&self) {
        if !self.enabled {
            return;
        }
        let total = (self.last - self.start).as_secs_f64() * 1000.0;
        let mut sorted: Vec<_> = self.marks.iter().filter(|(_, ms)| *ms >= 0.5).collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
        let rest: f64 = self
            .marks
            .iter()
            .filter(|(_, ms)| *ms < 0.5)
            .map(|(_, ms)| ms)
            .sum();
        let mut line = format!("[phases] total {total:.1}ms |");
        for (name, ms) in sorted {
            line.push_str(&format!(" {name} {ms:.1}ms |"));
        }
        line.push_str(&format!(" rest {rest:.1}ms"));
        log_debug!(LogCategory::Window, "{}", line);
    }
}

/// Should `regenerate_layout` block on `FcFontRegistry::request_fonts()` for
/// this pass?
///
/// Extracted from `regenerate_layout` so the decision has a truth table that
/// can be asserted directly; see `dll/tests/font_cache_regression.rs`. The
/// shipped bug was a one-line boolean mistake inside a function that needs a
/// window, a GL context and a live registry to reach, i.e. a bug in a place
/// no test could see.
///
/// - `build_complete`: the registry has finished parsing every font it found.
/// - `cache_empty`: the cache layout is about to run against holds no patterns.
///
/// The rule is **request while the build is INCOMPLETE**, not merely while the
/// cache is EMPTY. The original condition was `cache_empty || build_complete`,
/// and on macOS both disjuncts are false on the first layout: the cache
/// already holds 2 patterns (not "empty") while the scan of ~370 system fonts
/// is still running (not "complete"). It therefore never called
/// `request_fonts()` and laid out against a two-font cache holding none of
/// "Helvetica Neue", "Lucida Grande" or "System Font" — every macOS UI family
/// missed and all text fell through to LAST-RESORT. Linux happened to win the
/// race and looked fine, which is why this read as a macOS-only bug.
///
/// Requesting while incomplete is safe and is the entire point of the call:
/// `request_fonts()` BLOCKS until the requested families are parsed (measured
/// 186 ms cold on macOS), so the snapshot taken after it contains them. The
/// original worry — replacing a COMPLETE cache with an INCOMPLETE snapshot —
/// is addressed by skipping once the build is complete, not by treating any
/// non-empty cache as good enough.
pub fn should_request_fonts(build_complete: bool, cache_empty: bool) -> bool {
    !build_complete || cache_empty
}

pub fn regenerate_layout(
    layout_window: &mut LayoutWindow,
    app_data: &Arc<RefCell<RefAny>>,
    current_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    gl_context_ptr: &OptionGlContextPtr,
    fc_cache: &Arc<FcFontCache>,
    font_registry: &Option<Arc<FcFontRegistry>>,
    system_style: &Arc<SystemStyle>,
    icon_provider: &SharedIconProvider,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    relayout_reason: azul_core::callbacks::RelayoutReason,
) -> Result<LayoutRegenerateResult, String> {
    log_debug!(LogCategory::Layout, "[regenerate_layout] START");
    // Engine observability: the whole produce side (callback + solve + DL)
    // reports as scope "layout"; the probe spans inside land per-phase.
    #[cfg(feature = "telemetry")]
    let _frame_pump = azul_layout::telemetry::FramePump::begin("layout");
    azul_layout::probe::emit_phase_heap("start");
    let mut phases = PhaseTimer::new();

    // E2E observability: count DOM regenerations (sticky until
    // `reset_frame_counters`) so that a test can assert an interaction did not
    // trigger a DOM rebuild storm.
    layout_window.sync_frame_report();
    layout_window.frame_report.dom_regenerations = layout_window
        .frame_report
        .dom_regenerations
        .saturating_add(1);

    // Hand the window the two app-level things any DOM it styles needs: the
    // system style (system colour keywords) and the icon storage. Both used to
    // be set further down, just before layout — which was too late for the one
    // path that needs them EARLIEST. Icons resolve while the DOM is still a
    // tree, before the cascade, and a VirtualView callback (which runs during
    // layout, and only has `&mut LayoutWindow` to reach anything) had no way to
    // see a provider that the shell only held in this stack frame.
    layout_window.set_system_style(system_style.clone());
    layout_window.set_icon_provider(icon_provider.clone());

    // If the async font registry is available, request commonly-used fonts
    // and block until they are ready (eliminates FOUC). On cache hits this
    // is effectively free; on first run it blocks until the Scout + Builder
    // threads have parsed the needed fonts.
    azul_layout::probe::emit_phase_heap("before_registry_check");
    phases.mark("before_registry_check");
    if let Some(registry) = font_registry.as_ref() {
        // Avoid replacing a complete font cache (e.g. loaded from disk cache at
        // startup) with an incomplete snapshot while background builder threads
        // are still parsing fonts.  This prevents a race condition where only
        // some variants of a font family (e.g. only the Italic variant of
        // "System Font") are available when the snapshot is taken, causing
        // incorrect font selection on some launches.
        let current_cache_empty = layout_window.font_manager.fc_cache.is_empty();
        let build_complete = registry.is_build_complete();

        // The rule, and WHY it is not `cache_empty || build_complete`, lives on
        // `should_request_fonts` above — where it is covered by a truth table
        // in `dll/tests/font_cache_regression.rs`.
        if should_request_fonts(build_complete, current_cache_empty) {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] Requesting fonts from registry..."
            );
            let mut font_stacks = rust_fontconfig::config::tokenize_common_families(
                rust_fontconfig::OperatingSystem::current(),
            );
            // The DETECTED system fonts come first: the crate's list is a
            // static per-OS guess, and the one font the first frame will
            // definitely shape with is the DE's configured UI font (the DOM
            // styles resolve through SystemStyle). A desktop configured to
            // an off-list family (a riced Mint, a corporate theme) otherwise
            // pays the whole first layout in tofu for exactly that font.
            for f in [
                system_style.fonts.ui_font.as_ref(),
                system_style.fonts.title_font.as_ref(),
                system_style.fonts.monospace_font.as_ref(),
            ]
            .into_iter()
            .flatten()
            {
                let tokens = rust_fontconfig::config::tokenize_lowercase(f.as_str());
                if !tokens.is_empty() && !font_stacks.contains(&tokens) {
                    font_stacks.insert(0, tokens);
                }
            }
            azul_layout::probe::emit_phase_heap_extra(
                "after_tokenize",
                registry.chain_cache_len() as u64,
            );
            registry.request_fonts(&font_stacks);
            azul_layout::probe::emit_phase_heap_extra(
                "after_request_fonts",
                registry.chain_cache_len() as u64,
            );
            // Snapshot the registry into an FcFontCache for use during layout
            layout_window
                .font_manager
                .replace_fc_cache(registry.shared_cache());
            azul_layout::probe::emit_phase_heap("after_shared_cache");
            phases.mark("after_shared_cache");
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] Font registry snapshot complete"
            );
        } else {
            // Build complete: the registry snapshot is complete BY DEFINITION,
            // so replacing is always safe — and sometimes REQUIRED. The
            // window's own cache can still be the EMPTY pre-scan clone (cold
            // start: the app-level cache was `default()` and the window
            // cloned it before the builder drained), and keeping it shaped
            // the whole first frame against the two bundled memory fonts —
            // every family missed and the UI rendered .notdef end to end
            // (the measured Mint cold-start tofu, 2026-08-29; same class as
            // the macOS `should_request_fonts` bug, one seam deeper). The
            // length gate keeps the steady state free: once the caches
            // agree this branch costs one usize compare per pass.
            let registry_len = registry.cache.len();
            if layout_window.font_manager.fc_cache.len() != registry_len {
                layout_window
                    .font_manager
                    .replace_fc_cache(registry.shared_cache());
                log_debug!(
                    LogCategory::Layout,
                    "[regenerate_layout] Font build complete — refreshed the stale window \
                     snapshot ({} fonts)",
                    registry_len
                );
            } else {
                log_debug!(
                    LogCategory::Layout,
                    "[regenerate_layout] Font cache in sync with the completed build"
                );
            }
        }

        // VERIFY the detected system UI font actually resolved, whichever
        // branch ran. A configured family the scan cannot locate must degrade
        // to the fallback list with a LOUD warning, never silently — a
        // screen of .notdef is corrupted output, and the warning is the only
        // difference between "misconfigured theme" and "font system broke".
        if let Some(ui) = system_style.fonts.ui_font.as_ref() {
            let pattern = rust_fontconfig::FcPattern {
                name: Some(ui.as_str().to_string()),
                ..Default::default()
            };
            let mut trace = Vec::new();
            if layout_window
                .font_manager
                .fc_cache
                .query(&pattern, &mut trace)
                .is_none()
            {
                crate::plog_warn!(
                    "[fonts] the system UI font {:?} (from SystemStyle) was NOT found by the \
                     font scan — text set in it falls back to the platform default list. \
                     Check the family name and the scanned font directories.",
                    ui.as_str()
                );
            }
        }
    } else {
        azul_layout::probe::emit_phase_heap("before_fc_clone");
        phases.mark("before_fc_clone");
        // Fallback: use the provided fc_cache directly
        layout_window
            .font_manager
            .replace_fc_cache((**fc_cache).clone());
        azul_layout::probe::emit_phase_heap("after_fc_clone");
        phases.mark("after_fc_clone");
    }
    azul_layout::probe::emit_phase_heap("after_font_snapshot");
    phases.mark("after_font_snapshot");

    // 1. Call user's layout callback to get new DOM
    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Calling layout_callback"
    );

    // Create reference data container (syntax sugar to reduce parameter count).
    // The image cache is the LayoutWindow's own (single authority); a snapshot
    // of the refcounted-handle map (shares pixels; ImageCache itself has no
    // Clone per the double-free audit) sidesteps the borrow against the
    // &mut layout_window used below.
    let image_cache_snapshot = ImageCache {
        image_id_map: layout_window.image_cache.image_id_map.clone(),
    };
    let layout_ref_data = LayoutCallbackInfoRefData {
        image_cache: &image_cache_snapshot,
        gl_context: gl_context_ptr,
        system_fonts: &layout_window.font_manager.fc_cache,
        system_style: system_style.clone(),
        active_route: current_window_state.active_route.as_ref(),
        // #28 (d): monitor snapshot for content-bounding in layout() — the
        // platforms write the live list into layout_window.monitors; a
        // poisoned/contended lock degrades to "no info" rather than blocking
        // the layout pass.
        monitors: layout_window
            .monitors
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| azul_core::window::MonitorVec::from_const_slice(&[])),
    };

    let mut callback_info = LayoutCallbackInfo::new_with_reason(
        &layout_ref_data,
        current_window_state.size,
        current_window_state.theme,
        relayout_reason,
    );

    // Wire the callback's stored ctx (host-handle for managed FFIs,
    // PyCallableWrapper for Python, None for native Rust) so
    // `info.get_ctx()` reaches it. Without this, the macro-generated
    // host-invoker thunk sees `OptionRefAny::None` and returns the
    // kind's default (empty body) — which is exactly the "default DOM"
    // symptom we'd otherwise observe in the rendered window.
    callback_info.set_callable_ptr(&current_window_state.layout_callback.ctx);

    let app_data_borrowed = app_data.borrow_mut();
    azul_layout::probe::emit_phase_heap("before_callback");
    phases.mark("before_callback");

    // Clear any stale recording from an earlier callback on this thread —
    // the drain below must see ONLY what this invocation queried.
    let _ = azul_core::callbacks::take_recorded_size_queries();
    let _ = azul_core::callbacks::take_recorded_style_dependencies();

    // The layout callback IS app code (DOM construction): give it a
    // cb:<name> span so "app builds the DOM" separates from engine solving.
    let _cb_span =
        azul_layout::probe::Probe::span_for_fn(current_window_state.layout_callback.cb as usize);
    let user_dom =
        (current_window_state.layout_callback.cb)((*app_data_borrowed).clone(), callback_info);
    drop(_cb_span);

    drop(app_data_borrowed); // Release borrow

    // Capture which window-size queries (`window_width_less_than` & co.) the callback
    // made, synchronously on this same thread. This recording is the resize
    // fast path's evidence: a size change that flips none of these answers
    // (and crosses no CSS breakpoint) skips this whole produce side and
    // re-flows the existing DOM. See LayoutWindow::size_queries_would_flip.
    layout_window.recorded_size_queries = azul_core::callbacks::take_recorded_size_queries();
    // ... and which facets of the OS style it declared it reads. This is the
    // theme switch's evidence, the way the size queries are the resize's: a
    // system-style change touching nothing declared here re-styles the
    // existing StyledDom instead of re-invoking this callback. See
    // LayoutWindow::system_style_change_needs_full_regeneration.
    layout_window.recorded_style_dependencies =
        azul_core::callbacks::take_recorded_style_dependencies();
    azul_layout::probe::emit_phase_heap("after_callback");
    phases.mark("after_callback");

    // Software menu bar — the Linux fallback. Injected at the *Dom* level (before
    // `create_from_dom`) so the bar's `with_css` rules are scoped by
    // `scope_inline_css` in the same flatten pass as the rest of the window (a
    // separately-flattened + appended StyledDom would never get scoped). Windows
    // and macOS use the native HMENU / app menu; GNOME/KDE export the menu to the
    // desktop panel — so we only build our own bar on Linux when none of those
    // apply (see `inject_software_menubar`).
    #[cfg(target_os = "linux")]
    let user_dom = inject_software_menubar(user_dom);

    // 1.4. PRE-CASCADE DIFF (user directive 2026-08-08: "the start should just
    // scan over the NodeHierarchy to discover anything that changed, which is
    // iterating over a minimal array, in no world should this ever take 93ms").
    //
    // Fingerprint the fresh callback DOM in flatten order, in two tiers —
    // STRUCTURE (hierarchy + node content, css excluded) and STYLE (inline
    // css + subtree-scoped .with_css sheets). Equal on both tiers ⇒ the
    // retained StyledDom, its CASCADE, its shaped text and its warm layout
    // caches are all still valid: skip create_from_dom (the 67 ms cascade),
    // icon resolution, CSD injection and the post-hoc equivalence walk
    // entirely. `is_layout_equivalent` (below) stays as the fallback for the
    // FULL arm — it catches value-equal DOMs whose fingerprints moved
    // (over-sensitivity costs a cascade, never correctness).
    //
    // The skip is disabled while an E2E mount override is active (the app
    // DOM is not what is on screen) and when the retained DOM's node count
    // differs from the fingerprint walk (CSD titlebar injection shifts
    // NodeIds — the transfer indices would mis-target; those windows keep
    // the full path until the offset mapping exists).
    let precascade = if layout_window.e2e_mount.xml().is_none() {
        Some(azul_core::diff::fingerprint_dom(&user_dom))
    } else {
        layout_window.last_dom_fingerprints = None;
        None
    };
    let window_size_changed_precheck = {
        let old_dims = layout_window.current_window_state.size.dimensions;
        let new_dims = current_window_state.size.dimensions;
        const SIZE_CHANGE_THRESHOLD: f32 = 0.5;
        (old_dims.width - new_dims.width).abs() > SIZE_CHANGE_THRESHOLD
            || (old_dims.height - new_dims.height).abs() > SIZE_CHANGE_THRESHOLD
    };
    let precascade_skip = match (&precascade, layout_window.last_dom_fingerprints.as_ref()) {
        (Some((fp, _)), Some(prev)) => {
            fp.structure_root == prev.structure_root
                && fp.style_root == prev.style_root
                && layout_window
                    .layout_results
                    .get(&azul_core::dom::DomId::ROOT_ID)
                    .is_some_and(|lr| lr.styled_dom.node_data.as_ref().len() == fp.structure.len())
        }
        _ => false,
    };

    if precascade_skip {
        let (_fp, transfers) = precascade.expect("precascade_skip implies fingerprints");
        log_debug!(
            LogCategory::Layout,
            "[regenerate_layout] pre-cascade fingerprints equal (both tiers) — skipping              cascade/icons/CSD; transferring {} image callbacks + {} callback lists",
            transfers.image_callbacks.len(),
            transfers.callbacks.len()
        );
        phases.mark("precascade_skip");

        let mut old_result = layout_window
            .layout_results
            .remove(&azul_core::dom::DomId::ROOT_ID)
            .expect("checked by precascade_skip");
        // mem::take, not a field move — DomLayoutResult must stay whole so the
        // unchanged exit can put it back into the map.
        let mut retained = core::mem::take(&mut old_result.styled_dom);

        // Fresh RefAny payloads onto the retained DOM (same transfer the
        // equivalence branch has always done — the callbacks may reference
        // new app state even though the DOM shape is identical).
        {
            let node_data_mut = retained.node_data.as_mut();
            for (idx, new_cb) in &transfers.image_callbacks {
                if let Some(nd) = node_data_mut.get_mut(*idx) {
                    nd.node_type = azul_core::dom::NodeType::Image(
                        azul_css::css::BoxOrStatic::heap(azul_core::resources::ImageRef::callback(
                            new_cb.callback.clone(),
                            new_cb.refany.clone(),
                        )),
                    );
                }
            }
            for (idx, new_cbs) in &transfers.callbacks {
                if let Some(nd) = node_data_mut.get_mut(*idx) {
                    nd.callbacks = new_cbs.clone();
                }
            }
            // The callbacks just installed are clones of the FRESH build's
            // datasets; the retained nodes still hold last frame's. Merge
            // them the way the full path's `transfer_states` does (merge
            // callback when the node has one, fresh wins otherwise) and
            // re-point the fresh callbacks at the result. Without this an
            // identical rebuild reset every widget's callback state — a
            // slider's drag died on its second move whenever the app's
            // `RefreshDom` was followed by a redraw-driven relayout — and
            // split the widget across two allocations.
            for (idx, fresh) in &transfers.datasets {
                azul_core::diff::merge_fresh_dataset(node_data_mut, *idx, fresh.clone());
            }
        }

        // Re-derive hover/focus/active flags from the managers. A state
        // delta concurrent with this RefreshDom must not be lost — but it
        // needs only a warm RELAYOUT of the retained DOM (states are
        // runtime overlays), never the cascade the full path would pay.
        let states_before: Vec<_> = retained
            .styled_nodes
            .as_ref()
            .iter()
            .map(|n| n.styled_node_state.clone())
            .collect();
        let retained =
            apply_runtime_states_before_layout(retained, layout_window, current_window_state);
        let states_changed = retained
            .styled_nodes
            .as_ref()
            .iter()
            .map(|n| &n.styled_node_state)
            .ne(states_before.iter());

        // An identical DOM whose inline docking changed (a panel dropped on
        // another zone, torn off, docked back) still has to re-graft: the
        // retained layout reflects the OLD docking.
        let docks_changed = layout_window.transient_docks_changed();
        if !window_size_changed_precheck && !states_changed && !docks_changed {
            // Put the retained result back untouched.
            old_result.styled_dom = retained;
            layout_window
                .layout_results
                .insert(azul_core::dom::DomId::ROOT_ID, old_result);
            // THE DOM DID NOT CHANGE — THE DATA BEHIND IT MAY HAVE. A VirtualView
            // renders from a RefAny the fingerprint cannot see (a map's tile
            // cache, a virtual list's rows); `merge_fresh_dataset` above just
            // handed it the app's new state. The full path re-invokes every
            // view (`reset_all_invocation_flags` inside
            // `layout_and_generate_display_list`); this exit used to re-invoke
            // none, so a `RefreshDom` whose only change lived inside a dataset
            // left the view showing last frame's data until some unrelated
            // event relaid out. Queue them all — the frame path drains the
            // queue (`drain_virtual_view_updates`) before it paints.
            layout_window.queue_all_virtual_view_reinvoke();
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] COMPLETE (pre-cascade skip, layout unchanged)"
            );
            azul_layout::probe::emit_phase_heap("end_precascade_unchanged");
            phases.mark("end_precascade_unchanged");
            phases.report();
            // The DOM did not change, but the popup set may have: a callback's
            // `set_transient_window_open` changes NOTHING in the tree (that is
            // its point — no app flag), so this exit is exactly the one a
            // swatch click lands on. The reconcile is an empty diff when
            // nothing is open or forced.
            reconcile_transient_windows(layout_window, current_window_state);
            return Ok(LayoutRegenerateResult::LayoutUnchanged);
        }

        // Size and/or runtime states moved: warm-relayout the retained DOM
        // (solver3 reconcile sees the same StyledDom object → full cache
        // reuse; this is the R1 semantics without having built a throwaway
        // cascade first).
        azul_layout::probe::emit_phase_heap("before_layout_dl");
        phases.mark("before_layout_dl_precascade");
        layout_window
            .layout_and_generate_display_list(
                retained,
                current_window_state,
                renderer_resources,
                &ExternalSystemCallbacks::rust_internal(),
                debug_messages,
            )
            .map_err(|e| format!("Layout error: {:?}", e))?;
        azul_layout::probe::emit_phase_heap("after_layout_and_dl");
        phases.mark("after_layout_and_dl");
        layout_window.current_window_state = current_window_state.clone();

        // Overlay text (in-flight edits) is re-applied by the layout funnel
        // itself (`LayoutWindow::layout_and_generate_display_list`), once, on
        // every path — not here.

        // The warm relayout is a REAL layout pass: it can add or remove scroll
        // containers (a CSS breakpoint swapping a widget to its compact form
        // does exactly that without changing the DOM, which is why this path
        // was taken at all). Publish the result like the full path does, or
        // the ScrollManager keeps describing the previous tree.
        publish_scroll_state(layout_window);

        log_debug!(
            LogCategory::Layout,
            "[regenerate_layout] COMPLETE (pre-cascade skip, warm relayout)"
        );
        phases.mark("end_precascade_relayout");
        phases.report();
        // The warm path returns BEFORE the tail of this function, so the
        // popup reconcile must run here too — or a rebuild that happens to be
        // structurally identical (every RefreshDom from a click handler that
        // only flips a bool) silently skips it. That is exactly how the
        // continuity test first failed: passes 3 and 4 took this exit, the
        // manager was never told the popup was still open / now closed, and
        // the diff the backend reads went stale.
        reconcile_transient_windows(layout_window, current_window_state);
        return Ok(LayoutRegenerateResult::LayoutChanged);
    }
    let prev_dom_fingerprints = layout_window.last_dom_fingerprints.take();
    if let Some((fp, _)) = &precascade {
        // Full produce ahead — record what we are about to adopt.
        layout_window.last_dom_fingerprints = Some(fp.clone());
    }
    phases.mark("precascade_full");

    // 1.5. Flatten recursive Dom → StyledDom (single deferred cascade pass)
    //
    // The user callback now returns a recursive `Dom` with CSS attached via `.with_css()` (@scope-like).
    // We collect all CSS objects, flatten the tree, and run a single cascade pass.
    // E2E `mount` override: replace the app's DOM wholesale with the test's
    // inline XML+CSS document (reusing the existing XML→StyledDom parser).
    //
    // `style_user_dom` is the cascade AND the icon resolution that has to
    // precede it — see `LayoutWindow::style_user_dom`. It is a method on the
    // window rather than two calls here so that the OTHER producer of a user
    // DOM, a VirtualView callback deep inside layout, resolves icons too.

    let e2e_mount_xml = layout_window.e2e_mount.xml().map(str::to_string);
    let e2e_mount_dirty = layout_window.e2e_mount.take_dirty();
    let mut user_styled_dom = match e2e_mount_xml {
        Some(xml) => {
            let must_reparse = e2e_mount_dirty;
            let existing = (!must_reparse)
                .then(|| {
                    layout_window
                        .layout_results
                        .get(&azul_core::dom::DomId { inner: 0 })
                        .map(|lr| lr.styled_dom.clone())
                })
                .flatten();
            match existing {
                // Keep the already-mounted DOM (with any debug DOM mutations
                // applied to it) instead of rebuilding it from the XML.
                Some(styled) => styled,
                None => match azul_layout::xml::parse_xml_to_styled_dom_resolving_icons(
                    &xml,
                    icon_provider,
                    system_style,
                ) {
                    Ok(styled) => {
                        log_debug!(
                            LogCategory::Layout,
                            "[regenerate_layout] using E2E mount override ({} bytes of XML)",
                            xml.len()
                        );
                        styled
                    }
                    Err(e) => {
                        crate::log_error!(
                            LogCategory::Layout,
                            "[regenerate_layout] E2E mount XML failed to parse: {e:?} — falling \
                             back to the app DOM"
                        );
                        layout_window.style_user_dom(user_dom)
                    }
                },
            }
        }
        None => layout_window.style_user_dom(user_dom),
    };
    azul_layout::probe::emit_phase_heap("after_create_from_dom");
    phases.mark("after_create_from_dom");

    // 3. Conditionally inject Client-Side Decorations (CSD)
    //
    // IMPORTANT: CSD injection MUST happen BEFORE state migration (step 3.5)
    // and manager updates (step 3.6). The old layout_result.styled_dom contains
    // the full DOM *with* the titlebar from the previous frame. If we reconcile
    // old-DOM-with-titlebar vs new-DOM-without-titlebar, the node_moves will be
    // wrong (all user NodeIds would be off by the titlebar node count). By
    // injecting the titlebar first, both old and new DOMs have matching structure
    // and reconciliation produces correct node mappings.
    let mut styled_dom = if csd::should_inject_csd(
        current_window_state.flags.has_decorations,
        current_window_state.flags.decorations,
    ) {
        log_debug!(
            LogCategory::Layout,
            "[regenerate_layout] Injecting CSD decorations"
        );
        csd::wrap_user_dom_with_decorations(
            user_styled_dom,
            &current_window_state.title,
            true,         // inject titlebar
            system_style, // pass SystemStyle for native look
        )
    } else if current_window_state.flags.decorations
        == azul_core::window::WindowDecorations::NoTitleAutoInject
        && !cfg!(any(target_os = "windows", target_os = "linux"))
    {
        // Auto-inject a Titlebar at the top of the user's DOM.
        // The titlebar is a regular layout widget with DragStart/Drag/DoubleClick
        // callbacks — no special event-system hooks required.
        //
        // `NoTitleAutoInject` means "native controls visible, native title hidden,
        // app draws its own title". That requires a frame that shows window
        // controls WITHOUT a title bar — which only macOS provides (traffic
        // lights over a title-less bar). Windows (WS_CAPTION) and Linux (KWin/
        // Mutter server-side decorations, or X11 WM decorations) ALWAYS draw a
        // full titlebar including the title text, so a software titlebar here is
        // a duplicate "fake" bar below the real one (the double-titlebar bug).
        // On those platforms the native caption already renders the title and
        // handles dragging, so we leave the user DOM untouched and inject only on
        // macOS. (Apps wanting fully custom chrome should use
        // `WindowDecorations::None` + `has_decorations` → full CSD with buttons.)
        log_debug!(
            LogCategory::Layout,
            "[regenerate_layout] Auto-injecting Titlebar (NoTitleAutoInject)"
        );
        inject_software_titlebar(
            layout_window,
            user_styled_dom,
            &current_window_state.title,
            system_style,
        )
    } else {
        user_styled_dom
    };
    azul_layout::probe::emit_phase_heap("after_csd");
    phases.mark("after_csd");

    // 3.4. Re-compute inheritance and compact cache on the composed tree.
    //
    // The user's layout callback may have merged multiple StyledDom subtrees via
    // append_child(). Each subtree was independently styled (restyle → apply_ua_css
    // → compute_inherited_values → build_compact_cache), but append_child() only
    // concatenates the CSS caches — it does NOT re-run inheritance or rebuild the
    // compact cache. This causes two correctness bugs:
    //
    //   1. Inherited properties (color, font-size, direction) from parent nodes
    //      do not flow into appended child subtrees.
    //   2. The compact cache entries from child subtrees are stale — they reflect
    //      the child's isolated cascade, not the composed tree with parent overrides.
    //
    // Additionally, CSD injection (step 3) may have prepended titlebar nodes via
    // another append_child(), further invalidating the cache.
    //
    // Re-running inheritance + compact cache rebuild on the fully composed tree
    // fixes both issues. Cost: one extra O(n) pass — acceptable for correctness.
    styled_dom.recompute_inheritance_and_compact_cache();
    azul_layout::probe::emit_phase_heap("after_recompute_cache");
    phases.mark("after_recompute_cache");

    // 3.5. STATE MIGRATION: Transfer heavy resources from old DOM to new DOM
    // This allows components like video players to preserve their decoder handles
    // across frame updates without polluting the application data model.
    //
    // ALSO: Update FocusManager, ScrollManager, etc. with new NodeIds!
    // The node_moves tell us: old NodeId X is now new NodeId Y
    //
    // NOTE: This runs AFTER CSD injection so that both old and new DOMs have
    // matching structure (both include titlebar nodes). This ensures reconciliation
    // produces correct node mappings and manager NodeIds are not invalidated by
    // a subsequent titlebar injection shifting all indices.
    // Old DOM (previous frame). On the INITIAL render there is no previous
    // layout result yet — diff against an EMPTY old DOM so every node counts as
    // newly-mounted (InitialMount). That is what makes first-frame AfterMount
    // callbacks fire (e.g. the MapWidget's tile-fetch kickoff, camera/mic/video
    // capture threads). Previously the whole reconcile pass was gated on an
    // existing old layout, so frame 0 was skipped and AfterMount NEVER fired for
    // an app whose first DOM already contains the widget — only the synthetic
    // empty→full path (headless_lifecycle test) ever exercised it. The `.to_vec()`
    // clones below release the `layout_results` borrow before the later
    // `update_managers_with_node_moves(layout_window, …)` &mut borrow.
    // Filled at the diff seam below, consumed after the solve (see 3.5b / 5b).
    // Locals rather than window state: First and Last are two points in THIS
    // function, and parking them on `LayoutWindow` would invite a later frame
    // to read a stale half-pair.
    let mut anim_first_rects: std::collections::BTreeMap<
        azul_core::dom::NodeId,
        azul_core::geom::LogicalRect,
    > = std::collections::BTreeMap::new();
    let mut anim_node_moves: Vec<azul_core::diff::NodeMove> = Vec::new();
    let mut anim_new_node_data: Vec<azul_core::dom::NodeData> = Vec::new();
    let mut anim_new_hierarchy: Vec<azul_core::styled_dom::NodeHierarchyItem> = Vec::new();

    {
        let (old_node_data, old_hierarchy): (
            Vec<azul_core::dom::NodeData>,
            Vec<azul_core::styled_dom::NodeHierarchyItem>,
        ) = match layout_window
            .layout_results
            .get(&azul_core::dom::DomId::ROOT_ID)
        {
            Some(old_layout_result) => (
                old_layout_result.styled_dom.node_data.as_ref().to_vec(),
                old_layout_result
                    .styled_dom
                    .node_hierarchy
                    .as_ref()
                    .to_vec(),
            ),
            None => (Vec::new(), Vec::new()),
        };

        // Get new node data (from current frame — now also includes titlebar)
        let mut new_node_data: Vec<azul_core::dom::NodeData> =
            styled_dom.node_data.as_ref().to_vec();
        let new_hierarchy: Vec<azul_core::styled_dom::NodeHierarchyItem> =
            styled_dom.node_hierarchy.as_ref().to_vec();

        // Build layout maps for reconciliation (empty for now - we just need node moves)
        let old_layout_map = azul_core::OrderedMap::default();
        let new_layout_map = azul_core::OrderedMap::default();

        // Run reconciliation to find matched nodes
        let diff_result = azul_core::diff::reconcile_dom(
            &old_node_data,
            &new_node_data,
            &old_hierarchy,
            &new_hierarchy,
            &old_layout_map,
            &new_layout_map,
            azul_core::dom::DomId::ROOT_ID,
            azul_core::task::Instant::now(),
        );

        // Execute state migration for matched nodes with merge callbacks
        if !diff_result.node_moves.is_empty() {
            let mut old_node_data_mut = old_node_data.clone();
            azul_core::diff::transfer_states(
                &mut old_node_data_mut,
                &mut new_node_data,
                &diff_result.node_moves,
            );

            // Update the styled_dom with the merged node data
            styled_dom.node_data = new_node_data.into();

            // Runtime CSS overrides follow node identity too — same contract
            // as the dataset transfer above and the manager NodeId updates
            // below. Without this, every `set_css_property` patch reverted on
            // the next app-driven rebuild (the ribbon's collapsed band and
            // open gallery panel "un-toggled" whenever any callback returned
            // RefreshDom, e.g. the ribbon's own tab-click).
            if let Some(old_layout_result) = layout_window
                .layout_results
                .get(&azul_core::dom::DomId::ROOT_ID)
            {
                styled_dom.migrate_user_overrides_from(
                    &old_layout_result.styled_dom.css_property_cache.ptr,
                    &diff_result.node_moves,
                );
            }

            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] State migration: {} node moves processed",
                diff_result.node_moves.len()
            );
        }

        // 3.5b. CAPTURE "FIRST" FOR ENGINE-DRIVEN LAYOUT ANIMATION
        //
        // This is the only moment both geometries are reachable: the old
        // layout_results still hold the PREVIOUS frame's solved rects, and
        // `node_moves` says which old node became which new one. The matching
        // "Last" rects do not exist yet — the new tree has not been solved — so
        // First is stashed here and the pair is completed after the solve.
        //
        // No application involvement: an app that returns a different DOM gets
        // the transition for free, because the diff already knows what moved.
        // Nothing is seeded yet, so a frame that ends up not animating has paid
        // only for this map.
        anim_first_rects = diff_result
            .node_moves
            .iter()
            .filter_map(|m| {
                let r =
                    layout_window.get_node_bounds(azul_core::dom::DomId::ROOT_ID, m.old_node_id)?;
                Some((m.old_node_id, layout_rect_to_logical(r)))
            })
            .collect();
        anim_node_moves = diff_result.node_moves.clone();
        anim_new_node_data = styled_dom.node_data.as_ref().to_vec();
        anim_new_hierarchy = styled_dom.node_hierarchy.as_ref().to_vec();

        // 3.6. UPDATE MANAGERS WITH NEW NODE IDS
        // The node_moves tell us which old NodeIds map to which new NodeIds.
        // We need to update FocusManager, ScrollManager, etc. so they point to
        // the correct nodes in the new DOM.
        update_managers_with_node_moves(
            layout_window,
            &diff_result.node_moves,
            azul_core::dom::DomId::ROOT_ID,
        );

        // 3.7. QUEUE LIFECYCLE EVENTS FOR DISPATCH
        //
        // Mount / Update / Resize events target NEW NodeIds — they resolve
        // cleanly against the freshly-installed `layout_results` later in
        // the dispatch path.
        //
        // Unmount events are different: their `target.node` is an OLD NodeId
        // that does NOT exist in the new tree. By the time
        // `dispatch_events_propagated` runs, `layout_results` has already
        // been replaced by the new layout, so a NodeId-based lookup will
        // miss the BeforeUnmount callback. To keep that callback firing we
        // resolve it RIGHT HERE — while the OLD `old_node_data` slice is
        // still in scope — and stash a `(CoreCallbackData, SyntheticEvent)`
        // pair on the layout window. The dispatcher drains this side queue
        // and invokes the callbacks directly, bypassing the DOM lookup.
        for event in diff_result.events {
            use azul_core::events::{ComponentEventFilter, EventFilter, EventType};
            if event.event_type == EventType::Unmount {
                let old_node_id = event
                    .target
                    .node
                    .into_crate_internal()
                    .map(|nid| nid.index());
                if let Some(idx) = old_node_id {
                    if let Some(nd) = old_node_data.get(idx) {
                        for cb in nd.get_callbacks().as_ref().iter() {
                            if matches!(
                                cb.event,
                                EventFilter::Component(ComponentEventFilter::BeforeUnmount)
                            ) {
                                layout_window
                                    .pending_unmount_invocations
                                    .push((cb.clone(), event.clone()));
                            }
                        }
                    }
                }
            } else {
                layout_window.pending_lifecycle_events.push(event);
            }
        }
    }
    azul_layout::probe::emit_phase_heap("after_state_migrate");
    phases.mark("after_state_migrate");

    // NOTE: dirty_text_nodes is NOT applied to the StyledDom here.
    // The V3 architecture has two paths:
    //   - Initial Layout Path: reads from StyledDom (committed state from layout callback)
    //   - Relayout Path: reads from dirty_text_nodes (optimistic edits in LayoutCache)
    // The DOM text is intentionally stale. After layout_and_generate_display_list
    // runs on the new DOM, update_text_cache_after_edit will be called for each
    // dirty_text_node to patch the LayoutCache with the edited content.
    // dirty_text_nodes keys are remapped in update_managers_with_node_moves (step 8).

    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] StyledDom: {} nodes, {} hierarchy",
        styled_dom.styled_nodes.len(),
        styled_dom.node_hierarchy.len()
    );

    // 3.5 CRITICAL: Apply focus/hover/active states BEFORE layout
    // The layout callback creates a fresh StyledDom with default states (focused=false, etc.)
    // We need to synchronize the StyledNodeState with the current runtime state
    // (FocusManager.focused_node, mouse hover position, etc.) BEFORE the display list is generated
    let mut styled_dom =
        apply_runtime_states_before_layout(styled_dom, layout_window, current_window_state);
    azul_layout::probe::emit_phase_heap("after_runtime_states");
    phases.mark("after_runtime_states");

    // 3.7 OPTIMIZATION: Check if the new DOM is structurally identical to the old DOM.
    // If so, we can skip the expensive layout pipeline (CSS cascade, flexbox, display list)
    // and reuse the layout from the previous frame. Only image callbacks need re-invocation
    // since their content (e.g. GL textures) may have changed.
    //
    // IMPORTANT: We must NOT skip layout when the window size changed, even if the DOM
    // structure is identical. Flexbox positions/sizes depend on the viewport dimensions,
    // so a resize invalidates all computed positions. Without this check, image callbacks
    // would receive stale bounds after a window resize.
    let window_size_changed = {
        let old_dims = layout_window.current_window_state.size.dimensions;
        let new_dims = current_window_state.size.dimensions;
        // Half a logical pixel — below this threshold, size differences are
        // subpixel rounding noise and do not warrant a full relayout.
        const SIZE_CHANGE_THRESHOLD: f32 = 0.5;
        (old_dims.width - new_dims.width).abs() > SIZE_CHANGE_THRESHOLD
            || (old_dims.height - new_dims.height).abs() > SIZE_CHANGE_THRESHOLD
    };

    // The equivalence check runs REGARDLESS of whether the size changed — it
    // used to be gated on `!window_size_changed`, which meant the one case
    // where the DOM is most likely identical (a pure resize) bypassed the
    // check and threw the fresh-but-identical DOM into a from-scratch layout.
    // The size decides what happens WITH the equivalence result (below), not
    // whether it is worth knowing.
    if let Some(old_layout_result) = layout_window
        .layout_results
        .get(&azul_core::dom::DomId::ROOT_ID)
    {
        if azul_core::styled_dom::is_layout_equivalent(&old_layout_result.styled_dom, &styled_dom) {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] DOM structurally unchanged (size_changed={window_size_changed})"
            );

            // Transfer the new image callback RefAnys to the old DOM's nodes.
            // The old layout result keeps all its positions/sizes/display list data,
            // but the image callback data needs to be updated so that re-invocation
            // uses the freshly-created RefAny (which may reference new app state).
            let old_node_data = old_layout_result.styled_dom.node_data.as_ref();
            let new_node_data = styled_dom.node_data.as_ref();
            // Collect updates first to avoid borrow issues
            let mut image_updates: Vec<(usize, azul_core::callbacks::CoreImageCallback)> =
                Vec::new();
            for (idx, (old_nd, new_nd)) in
                old_node_data.iter().zip(new_node_data.iter()).enumerate()
            {
                if let (
                    azul_core::dom::NodeType::Image(ref _old_img),
                    azul_core::dom::NodeType::Image(ref new_img),
                ) = (&old_nd.node_type, &new_nd.node_type)
                {
                    if let azul_core::resources::DecodedImage::Callback(new_cb) = new_img.get_data()
                    {
                        image_updates.push((idx, new_cb.clone()));
                    }
                }
            }

            // Now apply image callback updates to old DOM's node data
            if !image_updates.is_empty() {
                let old_layout_result_mut = layout_window
                    .layout_results
                    .get_mut(&azul_core::dom::DomId::ROOT_ID)
                    .expect("layout_result must exist after get() succeeded");
                let old_node_data_mut = old_layout_result_mut.styled_dom.node_data.as_mut();
                for (idx, new_cb) in image_updates {
                    if let Some(old_nd) = old_node_data_mut.get_mut(idx) {
                        old_nd.node_type =
                            azul_core::dom::NodeType::Image(azul_css::css::BoxOrStatic::heap(
                                azul_core::resources::ImageRef::callback(
                                    new_cb.callback.clone(),
                                    new_cb.refany.clone(),
                                ),
                            ));
                    }
                }
            }

            // Also transfer any updated callback data (RefAny) for event callbacks
            // so that future events use fresh app state references.
            //
            // The DATASET (and a VirtualView's content refany) travel with
            // them. `transfer_states` above merged the fresh build's datasets
            // with last frame's and re-pointed the fresh callbacks at the
            // result, so `styled_dom`'s nodes are unified: callbacks and
            // dataset on ONE allocation. Copying only the callbacks onto the
            // retained node left its dataset on last frame's allocation —
            // every later callback mutated the one the dataset no longer was,
            // and the NEXT rebuild merged from the stale dataset (a released
            // slider came back mid-drag). Copy all three, so the retained
            // node IS the unified one.
            let mut callback_updates: Vec<(
                usize,
                azul_core::callbacks::CoreCallbackDataVec,
                Option<azul_core::refany::RefAny>,
                Option<azul_core::refany::RefAny>,
            )> = Vec::new();
            {
                let old_nd_ref = layout_window
                    .layout_results
                    .get(&azul_core::dom::DomId::ROOT_ID)
                    .expect("layout_result must exist after get() succeeded")
                    .styled_dom
                    .node_data
                    .as_ref();
                let new_nd_ref = styled_dom.node_data.as_ref();
                for (idx, (_old_nd, new_nd)) in old_nd_ref.iter().zip(new_nd_ref.iter()).enumerate()
                {
                    let dataset = new_nd.get_dataset().cloned();
                    let vv_refany = new_nd
                        .get_virtual_view_node_ref()
                        .map(|vv| vv.refany.clone());
                    if !new_nd.callbacks.as_ref().is_empty()
                        || dataset.is_some()
                        || vv_refany.is_some()
                    {
                        callback_updates.push((idx, new_nd.callbacks.clone(), dataset, vv_refany));
                    }
                }
            }
            if !callback_updates.is_empty() {
                let old_layout_result_mut = layout_window
                    .layout_results
                    .get_mut(&azul_core::dom::DomId::ROOT_ID)
                    .expect("layout_result must exist after get() succeeded");
                let old_node_data_mut = old_layout_result_mut.styled_dom.node_data.as_mut();
                for (idx, new_callbacks, dataset, vv_refany) in callback_updates {
                    if let Some(old_nd) = old_node_data_mut.get_mut(idx) {
                        old_nd.callbacks = new_callbacks;
                        if let Some(ds) = dataset {
                            old_nd.set_dataset(azul_core::refany::OptionRefAny::Some(ds));
                        }
                        if let (Some(r), Some(vv)) = (vv_refany, old_nd.get_virtual_view_node()) {
                            vv.refany = r;
                        }
                    }
                }
            }

            if !window_size_changed {
                // Same rule as the pre-cascade exit above: the DOM is
                // equivalent, the transferred datasets / VirtualView refanys
                // are not necessarily — re-invoke every view in place.
                layout_window.queue_all_virtual_view_reinvoke();
                log_debug!(
                    LogCategory::Layout,
                    "[regenerate_layout] COMPLETE (layout unchanged)"
                );
                azul_layout::probe::emit_phase_heap("end_unchanged");
                phases.mark("end_unchanged");
                phases.report();
                // Same reason as the pre-cascade exit above: the popup set can
                // change while the tree does not.
                reconcile_transient_windows(layout_window, current_window_state);
                return Ok(LayoutRegenerateResult::LayoutUnchanged);
            }

            // R1 — THE EQUIVALENCE RESULT IS USED, NOT THROWN AWAY. The size
            // changed but the DOM did not, so the freshly-produced StyledDom
            // is DROPPED and the PREVIOUS one — the exact object solver3's
            // warm caches (shaping, intrinsic widths) were built against — is
            // re-laid-out at the new size below. Same-object identity makes
            // cache reconciliation trivial instead of heuristic. The fresh
            // DOM lost nothing in the swap: state migration COPIES from the
            // old result (`.to_vec()` clones), it does not drain it, and the
            // callback/image updates above were applied to the OLD dom.
            //
            // (This branch is rare by design: the resize fast path skips this
            // whole function unless a breakpoint crossed. It catches full
            // regenerations — breakpoint crossings, RefreshDom — that produced
            // an identical DOM while the size also moved.)
            if let Some(old_result) = layout_window
                .layout_results
                .remove(&azul_core::dom::DomId::ROOT_ID)
            {
                log_debug!(
                    LogCategory::Layout,
                    "[regenerate_layout] size changed, DOM equivalent — relaying out the \
                     PREVIOUS StyledDom (warm caches) at the new size"
                );
                styled_dom = old_result.styled_dom;
            }
        }
    }
    azul_layout::probe::emit_phase_heap("after_equivalence_check");
    phases.mark("after_equivalence_check");

    // GRANULAR DIFF (task #15b): the pre-cascade fingerprints name exactly
    // which pre-order nodes changed. Reconcile can skip re-hashing every
    // node whose SELF + ANCESTORS are unchanged on BOTH tiers (ancestors
    // matter: style changes inherit downward). Only when the flattened
    // node count matches the walk (CSD injection shifts NodeIds — those
    // windows skip the hint) — the diff then feeds solver3 instead of
    // being thrown away.
    if let (Some((new_fp, _)), Some(prev_fp)) = (&precascade, &prev_dom_fingerprints) {
        let n = styled_dom.node_data.as_ref().len();
        if new_fp.structure.len() == n
            && prev_fp.structure.len() == n
            && new_fp.style.len() == n
            && prev_fp.style.len() == n
        {
            let hierarchy = styled_dom.node_hierarchy.as_container();
            let mut clean = vec![false; n];
            for i in 0..n {
                let self_clean = new_fp.structure[i] == prev_fp.structure[i]
                    && new_fp.style[i] == prev_fp.style[i];
                let parent_clean = hierarchy
                    .get(azul_core::id::NodeId::new(i))
                    .and_then(|h| h.parent_id())
                    .map_or(true, |p| clean.get(p.index()).copied().unwrap_or(false));
                clean[i] = self_clean && parent_clean;
            }
            layout_window.layout_cache.dom_diff_clean = Some(clean);
        }
    }

    // 4. Perform layout with solver3
    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Calling layout_and_generate_display_list"
    );

    azul_layout::probe::emit_phase_heap("before_layout_dl");
    phases.mark("before_layout_dl");

    layout_window
        .layout_and_generate_display_list(
            styled_dom,
            current_window_state,
            renderer_resources,
            &ExternalSystemCallbacks::rust_internal(),
            debug_messages,
        )
        .map_err(|e| format!("Layout error: {:?}", e))?;
    azul_layout::probe::emit_phase_heap("after_layout_and_dl");
    phases.mark("after_layout_and_dl");

    // CRITICAL: Update layout_window's cached window state so the next
    // regenerate_layout correctly detects size changes.  Without this,
    // resizing back to the original dimensions would be a no-op because
    // the stale layout_window.current_window_state still held the old size.
    layout_window.current_window_state = current_window_state.clone();

    // Overlay text (in-flight edits) is re-applied by the layout funnel
    // itself (`LayoutWindow::layout_and_generate_display_list`), once, on
    // every path — not here.

    log_debug!(
        LogCategory::Layout,
        "[regenerate_layout] Layout completed, {} DOMs",
        layout_window.layout_results.len()
    );

    // 5b. SEED LAYOUT ANIMATIONS ("Last" is now solved)
    //
    // The other half of 3.5b. Every diff correspondence whose rect actually
    // changed becomes a FLIP; identity transforms are skipped by `seed_moves`
    // so a static frame allocates nothing. Keyed by reconciliation identity, so
    // a node that keeps animating across several rebuilds is RETARGETED — the
    // spring keeps its position and velocity instead of snapping and restarting.
    if !anim_node_moves.is_empty() {
        // Collected BEFORE seeding: the "Last" accessor borrows `layout_window`
        // to read the freshly solved rects, and seeding borrows it mutably to
        // reach the manager. Two statements, so the read is finished before the
        // write starts.
        let correspondences = azul_core::animation::correspondences_from_moves(
            &anim_node_moves,
            &anim_new_node_data,
            &anim_new_hierarchy,
            |old_id| anim_first_rects.get(&old_id).copied(),
            |new_id| {
                layout_window
                    .get_node_bounds(azul_core::dom::DomId::ROOT_ID, new_id)
                    .map(layout_rect_to_logical)
            },
        );
        // Rebuild the identity→NodeId bridge BEFORE seeding, so a key seeded
        // this frame is already resolvable when the first tick composites it.
        // Rebuilt wholesale: after a rebuild the previous NodeIds are
        // meaningless, and a surviving stale entry would push this frame's
        // transform onto whatever unrelated node inherited the array slot.
        layout_window.anim_key_to_node = azul_core::animation::anim_keys_for_moves(
            &anim_node_moves,
            &anim_new_node_data,
            &anim_new_hierarchy,
        )
        .into_iter()
        .collect();

        let seeded = azul_core::animation::seed_moves(
            &mut layout_window.animations,
            correspondences,
            azul_core::animation::InterpolationMode::Spring(azul_core::animation::Spring::SMOOTH),
        );
        if seeded > 0 {
            log_debug!(
                LogCategory::Layout,
                "[regenerate_layout] Seeded {} layout animation(s) from {} node move(s)",
                seeded,
                anim_node_moves.len()
            );
        }
    }

    // 5. + 6. Register scrollable nodes / scrollbar states, then sync the
    // scrollbar fade opacities that follow from them.
    publish_scroll_state(layout_window);

    // 7. Permission diff — scan the styled DOM for permission-bearing
    // NodeTypes (GeolocationProbe / CameraPreview / SensorProbe / …) and
    // refresh the refcount on PermissionManager. Subscribe / Release diff
    // events accumulate in the manager's queue; the platform shell drains
    // and dispatches them via `crate::desktop::extra::permission::apply_diff_events`.
    //
    // Today the only permission-bearing NodeType is GeolocationProbe (P3.1);
    // CameraPreview / SensorProbe land in P6 and just add arms here. A probe
    // in the tree subscribes Capability::Geolocation, so the platform backend
    // turns it into the OS location prompt. Snapshot the (capability, node)
    // pairs first so we don't hold a borrow on `layout_results` while the
    // diff mutates `permission_manager`.
    let mut wants_gamepad = false;
    let mut wants_sensors = false;
    let permission_bearing: Vec<(
        azul_layout::managers::permission::Capability,
        azul_core::dom::DomNodeId,
    )> = {
        let mut pairs = Vec::new();
        for (dom_id, layout_result) in layout_window.layout_results.iter() {
            for (i, nd) in layout_result
                .styled_dom
                .node_data
                .as_ref()
                .iter()
                .enumerate()
            {
                if let azul_core::dom::NodeType::GeolocationProbe(_) = nd.get_node_type() {
                    pairs.push((
                        azul_layout::managers::permission::Capability::Geolocation,
                        azul_core::dom::DomNodeId {
                            dom: *dom_id,
                            node: azul_core::dom::NodeId::from_usize(i).into(),
                        },
                    ));
                }
                // MWA-A1 arming signals: nodes listening for GamepadInput /
                // SensorChanged tell the capability pump which hardware
                // sources to poll — and whether its wake-up timer needs to
                // run at all (no listeners → no polling → no timer).
                for cb in nd.get_callbacks().as_ref().iter() {
                    use azul_core::events::{EventFilter, HoverEventFilter, WindowEventFilter};
                    match &cb.event {
                        EventFilter::Hover(HoverEventFilter::GamepadInput)
                        | EventFilter::Window(WindowEventFilter::GamepadInput) => {
                            wants_gamepad = true;
                        }
                        EventFilter::Hover(HoverEventFilter::SensorChanged)
                        | EventFilter::Window(WindowEventFilter::SensorChanged) => {
                            wants_sensors = true;
                        }
                        _ => {}
                    }
                }
            }
        }
        pairs
    };
    layout_window
        .gamepad_manager
        .set_has_listeners(wants_gamepad);
    layout_window
        .sensor_manager
        .set_has_listeners(wants_sensors);
    layout_window.permission_manager.diff_layout(|emit| {
        for (capability, node_id) in &permission_bearing {
            emit(*capability, *node_id);
        }
    });
    let permission_events = layout_window.permission_manager.take_pending_events();
    if !permission_events.is_empty() {
        crate::desktop::extra::permission::apply_diff_events(&permission_events);
    }

    // 7b. Geolocation diff — symmetric to the permission pass. Walks
    // every DOM in this window for `NodeType::GeolocationProbe` nodes
    // and feeds their configs to the manager. Subscribe / Release /
    // Reconfigure events emitted by the manager route through the
    // platform backend, which starts or stops the native
    // CLLocationManager / FusedLocationProviderClient / geoclue
    // subscription.
    {
        // Snapshot the configs first so we don't hold a borrow on
        // `layout_window.layout_results` while mutating
        // `layout_window.geolocation_manager`.
        let mut probe_configs: Vec<azul_core::geolocation::GeolocationProbeConfig> = Vec::new();
        for layout_result in layout_window.layout_results.values() {
            for nd in layout_result.styled_dom.node_data.as_ref().iter() {
                if let azul_core::dom::NodeType::GeolocationProbe(cfg) = nd.get_node_type() {
                    probe_configs.push(*cfg);
                }
            }
        }
        layout_window.geolocation_manager.diff_layout(|emit| {
            for cfg in &probe_configs {
                emit(*cfg);
            }
        });
    }
    let geolocation_events = layout_window.geolocation_manager.take_pending_events();
    if !geolocation_events.is_empty() {
        crate::desktop::extra::geolocation::apply_diff_events(&geolocation_events);
    }

    // 7a / 7c–7f (async drains: permission results, geolocation fixes,
    // biometric availability + requests + results, keyring requests +
    // results) MOVED to shell2::common::capability_pump::pump() (MWA-A1).
    // They have nothing to do with layout — and living here meant an idle
    // app (blocked in WaitMessage / select / the NSApp loop) never drained
    // them at all. The pump runs at the top of every process_window_events
    // pass and on CAPABILITY_PUMP_TIMER_ID ticks; this function keeps only
    // the DOM-derived subscription diffs (steps 7 / 7b above + the
    // listener-flag walk), because subscriptions ARE a function of layout.

    // 7g. (PDF export is now the standalone headless `Pdf::from_dom` API in
    // dll::desktop::extra::pdf — no window-coupled per-frame export drain.)

    // (7h-pre sensor ensure/poll and 7i-pre gamepad ensure/poll moved into
    // capability_pump::pump(), gated on the listener flags computed above —
    // no listeners, no native subscription, no polling.)

    // Register the platform microphone-capture backend once (ALSA on Linux) so
    // MicrophoneWidget captures real audio where available; OnceLock-guarded.
    crate::desktop::extra::audio::ensure_mic_backend();

    // Register the platform camera-capture backend once (v4l2 via rscam on
    // Linux) so CameraWidget shows the real camera where available; guarded.
    crate::desktop::extra::camera::ensure_camera_backend();
    crate::desktop::extra::screencap::ensure_screen_backend();
    // The platform frame scaler the capture fan-out uses (vImage on macOS;
    // the portable scaler elsewhere) — same seam, same guard.
    crate::desktop::extra::resample::ensure_frame_resampler();
    // Same seam for the async OS file picker: on iOS / Android
    // `FileDialog::open_file_async` dispatches to the dispatchers this
    // installs; the desktop answers the same call synchronously via tfd.
    crate::desktop::extra::file_picker::ensure_file_picker_backend();

    log_debug!(LogCategory::Layout, "[regenerate_layout] COMPLETE");
    azul_layout::probe::emit_phase_heap("end");
    phases.mark("end");
    phases.report();

    // A focus parked before the FIRST layout (a create callback, most
    // commonly) is applied right after the layout that made it resolvable,
    // so the caret is seeded in this very frame instead of one event pass
    // later. The blink Timer lands in the engine's timer map; the OS blink
    // timer is armed by the next pass's FinalizePendingFocusChanges arm,
    // which on a desktop shell arrives with the window's first real event.
    // The e2e runner performs the same call at its layout tail (parity).
    if layout_window.focus_manager.has_deferred_focus_target() {
        layout_window.finalize_pending_focus_changes();
    }

    // <transient-window>: now that the parent is laid out, find every node
    // that says `open=true`, lay each one's subtree out as its own dom, and
    // bring the set of open popups in line. The manager matches windows to
    // their source node across rebuilds, so a popup that is still open after
    // this pass is MOVED (if its anchor shifted) rather than closed and
    // re-opened — the flicker class the screenshare fix chased out of image
    // nodes, and far worse on a window. The backend reads the diff after
    // this returns and creates/moves/destroys surfaces accordingly.
    reconcile_transient_windows(layout_window, current_window_state);

    Ok(LayoutRegenerateResult::LayoutChanged)
}

/// Incremental relayout: re-run layout on the existing StyledDom without
/// calling the user's `layout_callback()`.
///
/// This is the fast path for restyle-driven changes (hover/focus CSS changes,
/// runtime `set_css_property()`, `set_node_text()`) where the DOM structure
/// hasn't changed — only styles or text content.
///
/// The StyledDom already has updated `styled_node_state` (from `restyle_on_state_change`)
/// or updated node data (from runtime edits). We just need to re-run layout
/// and regenerate the display list.
///
/// ## When to use
///
/// - `ProcessEventResult::ShouldIncrementalRelayout`
/// - After `apply_focus_restyle` detects layout-affecting CSS changes
/// - After `words_changed` / `css_properties_changed` from callbacks
///
/// ## What it skips
///
/// - User's `layout_callback()` (DOM is unchanged)
/// - CSD injection (already done)
/// - State migration / reconciliation (NodeIds haven't changed)
/// - Manager remapping (NodeIds haven't changed)
///
/// ## Per-backend wiring status — DONE (all desktop backends)
///
/// Every desktop backend now routes `ProcessEventResult::ShouldIncrementalRelayout`
/// to this fast path instead of collapsing it into the `ShouldRegenerateDomCurrentWindow`
/// arm (which triggers a *full* `regenerate_layout()` — re-invoking the user's
/// `layout_callback()` and rebuilding the StyledDom — when re-running layout on the
/// existing StyledDom would suffice for a restyle/runtime-edit: hover/focus CSS,
/// `set_css_property`, `set_node_text`).
///
/// In every backend the `ShouldIncrementalRelayout` event arm calls
/// `incremental_relayout(layout_window, &current_window_state, &mut renderer_resources,
/// &mut debug_messages)` immediately. Backends differ in how their frame-generation
/// path then presents:
///
/// - **Transaction-only generate path** (macOS, linux/x11): `generate_frame_if_needed`
///   already rebuilds the WebRender transaction from the current StyledDom WITHOUT
///   re-running layout (it calls `generate_frame()`), so the event arm just calls
///   `request_regeneration(reason)` and the existing path presents.
///     - DONE: macos/mod.rs (`process_close_event` etc., the reference arm).
///     - DONE: linux/x11/mod.rs (its `generate_frame_if_needed` is transaction-only).
///
/// - **Full-regen generate path** (windows, linux/wayland): the frame path runs the
///   FULL `regenerate_layout()` when a regeneration is pending, which would
///   OVERRIDE the incremental pass. These use `request_relayout_only()` on
///   `CommonWindowState` (`event.rs`), which raises the relayout-only request
///   AND the ordinary one (so the frame gates see that work is owed). The frame
///   path then branches: `relayout_only_pending()` ⇒ SKIP the full
///   `regenerate_layout()` (layout is already up to date) but STILL build + send the
///   WebRender transaction + present; else `regeneration_pending()` ⇒ full
///   `regenerate_layout()`. Both requests are retired after the frame is sent.
///     - DONE: windows/mod.rs — `ShouldIncrementalRelayout` event arm +
///       `send_frame_after_incremental_relayout()` helper called from the WM_PAINT
///       relayout-only branch (GPU `generate_frame` + flush / CPU hit-tester
///       rebuild — `regenerate_layout()`'s finalize tail — then `render_and_present(true)`).
///     - DONE: linux/wayland/mod.rs — both `ShouldIncrementalRelayout` event arms
///       split; `generate_frame_if_needed` runs `regenerate_layout()` only in the true
///       full case and still rebuilds the hit-tester + sends the transaction (via
///       `generate_frame()`) in both.
/// [`incremental_relayout`] + the solver3 reconcile-skip hint. ONLY for the
/// resize-latch call sites (`take_resize_relayout()` branches): there the
/// StyledDom is by construction the same object with zero DOM/style dirt, so
/// solver3 may take the retained tree as-is instead of re-walking 1209 nodes
/// to rediscover full reuse (~9.6 ms/pass) and re-mapping per-node caches to
/// an identity mapping. Restyle/scroll-driven incremental relayouts MUST keep
/// calling [`incremental_relayout`] — they need reconcile's fingerprint diff
/// for paint-dirty classification.
/// PRIVATE TO `common` — backends call
/// [`CommonWindowState::incremental_relayout`](super::event::CommonWindowState::incremental_relayout)
/// with `IncrementalRelayout::Resize`, which runs this AND the finalize tail
/// (the CPU hit-tester rebuild) that the macOS/X11 resize fast path forgot.
pub(super) fn incremental_relayout_for_resize(
    layout_window: &mut LayoutWindow,
    current_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<(), String> {
    layout_window.layout_cache.resize_only_hint = true;
    incremental_relayout(
        layout_window,
        current_window_state,
        renderer_resources,
        debug_messages,
    )
}

/// PRIVATE TO `common` — backends call
/// [`CommonWindowState::incremental_relayout`](super::event::CommonWindowState::incremental_relayout),
/// which runs this AND the finalize tail (the CPU hit-tester rebuild). A
/// relayout whose results the hit-tester never sees sends every click over a
/// moved node to whatever used to be there; that is why the bare function is
/// not reachable from a backend any more.
pub(super) fn incremental_relayout(
    layout_window: &mut LayoutWindow,
    current_window_state: &FullWindowState,
    renderer_resources: &mut RendererResources,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<(), String> {
    log_debug!(LogCategory::Layout, "[incremental_relayout] START");

    // Window category, same reasoning as regenerate_layout's span: Layout is
    // the category everyone silences (AZ_LOG=debug,-layout), and the fast
    // resize path's cost — solver3 re-flow + display list on the EXISTING
    // StyledDom — is the number the <8ms interactivity target is measured
    // against. Without this span the fast path was invisible in the log.
    let _span = crate::log_span!(
        crate::desktop::shell2::common::debug_server::LogCategory::Window,
        "incremental_relayout",
    );

    let system_callbacks = ExternalSystemCallbacks::rust_internal();

    // Re-run layout on the existing StyledDom with dirty flags already set.
    // The StyledDom in the layout_result already has updated styles/states.
    //
    // Ownership transfer: pull the existing DomLayoutResult out of the map
    // (`.remove()` instead of `.get()`), take its `styled_dom` by value, and
    // hand it to `layout_and_generate_display_list`, which will move it into
    // the freshly-inserted result. This eliminates the double clone that used
    // to happen on every resize (once here, once again inside the layout fn).
    if let Some(layout_result) = layout_window
        .layout_results
        .remove(&azul_core::dom::DomId::ROOT_ID)
    {
        // Move the StyledDom out of the old DomLayoutResult; the remaining
        // fields (positions, display list, tree) drop when `layout_result`
        // goes out of scope. `layout_and_generate_display_list` then inserts
        // a fresh DomLayoutResult built around this very StyledDom without
        // cloning it.
        let styled_dom = layout_result.styled_dom;

        layout_window
            .layout_and_generate_display_list(
                styled_dom,
                current_window_state,
                renderer_resources,
                &system_callbacks,
                debug_messages,
            )
            .map_err(|e| format!("Incremental layout error: {:?}", e))?;

        // Same CRITICAL update as regenerate_layout's tail: the cached window
        // state is what the next pass diffs against to detect size changes.
        // The resize fast path lands HERE (not in regenerate_layout), so
        // without this line every fast resize left the cached size stale —
        // and the next full regeneration would mis-detect what changed.
        layout_window.current_window_state = current_window_state.clone();
    }

    register_scroll_nodes(layout_window);

    log_debug!(LogCategory::Layout, "[incremental_relayout] COMPLETE");

    Ok(())
}

/// Apply runtime states (focus, hover, active) to the StyledDom BEFORE layout
///
/// The layout callback creates a fresh StyledDom where all StyledNodeState fields
/// are set to their defaults (focused=false, hover=false, active=false).
/// This function synchronizes those states with the current runtime state from
/// the various managers (FocusManager, mouse state, etc.) BEFORE the display list
/// is generated.
///
/// This is critical for `:focus`, `:hover`, `:active` CSS pseudo-class styling
/// to work correctly - the display list generation reads these states to determine
/// which CSS properties to apply.
fn apply_runtime_states_before_layout(
    mut styled_dom: azul_core::styled_dom::StyledDom,
    layout_window: &LayoutWindow,
    current_window_state: &FullWindowState,
) -> azul_core::styled_dom::StyledDom {
    use azul_core::dom::DomId;

    // The styled_dom is the ROOT_ID DOM (after CSD injection)
    let dom_id = DomId::ROOT_ID;

    // 1. Apply focus state
    if let Some(focused_node) = layout_window.focus_manager.get_focused_node() {
        // Only apply if the focused node is in the same DOM we're processing
        if focused_node.dom == dom_id {
            if let Some(node_id) = focused_node.node.into_crate_internal() {
                let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
                if let Some(styled_node) = styled_nodes.get_mut(node_id) {
                    styled_node.styled_node_state.focused = true;
                    log_debug!(
                        LogCategory::Layout,
                        "[apply_runtime_states_before_layout] Set focused=true for node {:?}",
                        node_id
                    );
                }
            }
        }
    }

    // 2. Apply hover state based on hover manager
    if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
        if let Some(hit_test) = last_hit_test.hovered_nodes.get(&dom_id) {
            let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
            for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
                if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                    styled_node.styled_node_state.hover = true;
                }
            }
        }
    }

    // 3. Apply active state (mouse button down on a hovered element)
    if current_window_state.mouse_state.left_down {
        if let Some(last_hit_test) = layout_window.hover_manager.get_current_mouse() {
            if let Some(hit_test) = last_hit_test.hovered_nodes.get(&dom_id) {
                let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
                for (node_id, _hit_item) in hit_test.regular_hit_test_nodes.iter() {
                    if let Some(styled_node) = styled_nodes.get_mut(*node_id) {
                        styled_node.styled_node_state.active = true;
                    }
                }
            }
        }
    }

    // 4. Apply :dragging pseudo-state from gesture_drag_manager
    // When the layout callback returns RefreshDom, the DOM is rebuilt from scratch
    // and the :dragging state that was set in event.rs on DragStart is lost.
    // Re-apply it here from the authoritative drag manager state.
    if let Some(drag_ctx) = layout_window.gesture_drag_manager.get_drag_context() {
        if let Some(node_drag) = drag_ctx.as_node_drag() {
            if node_drag.dom_id == dom_id {
                let mut styled_nodes = styled_dom.styled_nodes.as_container_mut();
                if let Some(styled_node) = styled_nodes.get_mut(node_drag.node_id) {
                    styled_node.styled_node_state.dragging = true;
                    log_debug!(
                        LogCategory::Layout,
                        "[apply_runtime_states_before_layout] Set dragging=true for node {:?}",
                        node_drag.node_id
                    );
                }

                // 5. Apply :drag-over pseudo-state on current drop target
                if let Some(drop_target) = node_drag.current_drop_target.into_option() {
                    if drop_target.dom == dom_id {
                        if let Some(target_node_id) = drop_target.node.into_crate_internal() {
                            if let Some(styled_node) = styled_nodes.get_mut(target_node_id) {
                                styled_node.styled_node_state.drag_over = true;
                                log_debug!(
                                    LogCategory::Layout,
                                    "[apply_runtime_states_before_layout] Set drag_over=true for node {:?}",
                                    target_node_id
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    styled_dom
}

/// Fold a DOM reconciliation into every piece of `NodeId`-keyed window state.
///
/// `NodeId`s are arena indices: a rebuild renumbers them, so any manager that is
/// not remapped keeps pointing at a live-but-WRONG node (deleting a preceding
/// sibling shifts every later index down by one). `node_moves` maps every
/// MATCHED old id to its new one; an old id ABSENT from it was unmounted, and
/// its state must be dropped, not kept.
///
/// This function is deliberately a two-liner: the exhaustive, can't-forget
/// dispatch lives in `LayoutWindow::remap_node_ids` (layout/src/window.rs),
/// where a new `LayoutWindow` field fails to compile until it is classified as
/// node-keyed or exempt, and every node-keyed manager implements
/// `azul_layout::managers::NodeIdRemap`.
fn update_managers_with_node_moves(
    layout_window: &mut LayoutWindow,
    node_moves: &[azul_core::diff::NodeMove],
    dom_id: azul_core::dom::DomId,
) {
    let map = azul_layout::managers::NodeIdMap::from_node_moves(node_moves);
    layout_window.remap_node_ids(dom_id, &map);
}

/// Helper function to generate WebRender frame
///
/// This should be called after regenerate_layout to submit the frame to WebRender.
/// Usually called once at the end of event processing.
pub fn generate_frame(
    layout_window: &mut LayoutWindow,
    render_api: &mut WrRenderApi,
    document_id: DocumentId,
    gl_context: &azul_core::gl::OptionGlContextPtr,
) {
    // Advance layout animations before the display list is translated, so this
    // frame shows the transform sampled for THIS frame rather than the previous
    // one. Cheap and self-gating: with nothing animating it is one `is_empty`
    // check and no clock read.
    let _still_animating = layout_window.tick_animations_now();

    // Sample the keyframed tracks for THIS frame — may invoke COMPONENT
    // animation functions with a full TimerCallbackInfo. This path has no
    // change dispatcher, so their queued changes are parked on the window
    // and the event pass drains them at its next start (one frame of
    // latency, same order guarantees as timer changes).
    if layout_window.has_track_work() {
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let frame_start = (system_callbacks.get_system_time_fn.cb)();
        let cur = layout_window.current_window_state.clone();
        let prev = layout_window.previous_window_state.clone();
        let style = layout_window
            .system_style
            .clone()
            .unwrap_or_else(|| std::sync::Arc::new(azul_css::system::SystemStyle::default()));
        let rr = std::mem::take(&mut layout_window.renderer_resources);
        let changes = layout_window.run_track_frames(
            1.0 / 60.0,
            frame_start,
            &azul_core::window::RawWindowHandle::Unsupported,
            gl_context,
            style,
            &system_callbacks,
            &prev,
            &cur,
            &rr,
        );
        layout_window.renderer_resources = rr;
        layout_window.pending_track_changes.extend(changes);
    }

    // Process any pending VirtualView updates requested by callbacks
    // This must happen BEFORE wr_translate2::generate_frame() so that the VirtualView
    // callbacks can be re-invoked and their layout results are available.
    // (GPU path: the WebRender hit-tester is refreshed by the transaction
    // itself, so there is no CPU hit-tester to hand in.)
    drain_virtual_view_updates(layout_window, None);

    let mut txn = WrTransaction::new();

    // Display list was rebuilt
    wr_translate2::generate_frame(&mut txn, layout_window, render_api, true, gl_context);

    render_api.send_transaction(wr_translate2::wr_translate_document_id(document_id), txn);
}

/// Drain the queued `VirtualView` re-invocations — `trigger_virtual_view_rerender`
/// from a callback or a background writeback, a scroll past an edge, or an
/// unchanged `RefreshDom` (both `LayoutUnchanged` exits of [`regenerate_layout`]
/// queue every view) — by re-invoking each view's callback IN PLACE on the
/// existing DOM, and keep the CPU hit-tester honest about it.
///
/// Returns whether any view was rebuilt.
///
/// THE REBUILD IS PART OF THE DRAIN. An in-place re-invoke gives the view's
/// child DOM fresh `NodeId`s, so a CPU hit-tester built before it indexes a
/// generation of nodes that no longer exists: the next pointer move resolves
/// to a stale id (cursor panic while panning the map, events on the wrong
/// node). X11, Wayland and Windows each rebuilt the tester by hand after
/// their own copy of this drain; macOS and headless did not. There is one
/// drain now, and it cannot forget.
pub fn drain_virtual_view_updates(
    layout_window: &mut LayoutWindow,
    cpu_hit_tester: Option<&mut azul_layout::headless::CpuHitTester>,
) -> bool {
    if layout_window.pending_virtual_view_updates.is_empty() {
        return false;
    }
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let current_window_state = layout_window.current_window_state.clone();
    let renderer_resources = std::mem::take(&mut layout_window.renderer_resources);
    let updated = layout_window.process_pending_virtual_view_updates(
        &current_window_state,
        &renderer_resources,
        &system_callbacks,
    );
    layout_window.renderer_resources = renderer_resources;
    let rebuilt = !updated.is_empty();
    if rebuilt {
        if let Some(cpu_ht) = cpu_hit_tester {
            cpu_ht.rebuild_from_layout_with_gpu(
                &layout_window.layout_results,
                Some(&layout_window.gpu_state_manager),
            );
        }
    }
    rebuilt
}

/// Wrap the user's `StyledDom` with a `Titlebar` at the top.
///
/// The titlebar carries DragStart / Drag / DoubleClick callbacks so that the
/// window can be moved and maximized through regular `CallbackInfo` APIs
/// (gesture manager + `modify_window_state`).  No special event-system hooks
/// are needed.
fn inject_software_titlebar(
    layout_window: &LayoutWindow,
    user_dom: azul_core::styled_dom::StyledDom,
    window_title: &str,
    system_style: &SystemStyle,
) -> azul_core::styled_dom::StyledDom {
    use azul_layout::widgets::titlebar::Titlebar;

    let titlebar = Titlebar::from_system_style(window_title.into(), system_style);
    let titlebar_dom = titlebar.dom();

    // Through the window's chokepoint, like every other user DOM: the bar's
    // three window controls ARE icon nodes (`system:titlebar-close,…`), and
    // styling this subtree on its own with `StyledDom::create` skipped icon
    // resolution entirely — so the auto-injected bar drew three empty boxes.
    let titlebar_styled = layout_window.style_user_dom(titlebar_dom);

    // Use an Html root (not Body!) so we don't get double <body> nesting.
    // StyledDom::default() creates a Body root, and the user's DOM also starts
    // with Body — nesting body>body causes double 8px UA margin.
    // Html has display:block but no margin in the UA stylesheet.
    let mut container_dom = azul_core::dom::Dom::create_html();
    let mut container =
        azul_core::styled_dom::StyledDom::create(&mut container_dom, azul_css::css::Css::empty());
    container.append_child(titlebar_styled);
    container.append_child(user_dom);
    container
}

/// Wrap a user `Dom` with a software menu bar (the Linux fallback) at the top of
/// its content — but only if the root declares a `Menu` (`Dom::with_menu_bar`)
/// and no native global menu applies (GNOME/KDE export to the panel). Operates on
/// the raw `Dom` *before* `create_from_dom` so the bar's `with_css` rules are
/// scoped in the main flatten pass. No-op (returns `user_dom` unchanged) when
/// there is no menu bar or a native global menu is in use.
#[cfg(target_os = "linux")]
fn inject_software_menubar(user_dom: azul_core::dom::Dom) -> azul_core::dom::Dom {
    use azul_core::dom::{Dom, DomVec};

    if crate::desktop::shell2::linux::gnome_menu::should_use_gnome_menus() {
        return user_dom;
    }
    let menu = match user_dom.root.get_menu_bar() {
        Some(boxed_menu) => boxed_menu.clone(),
        None => return user_dom,
    };
    let menubar = azul_layout::widgets::menubar::build_menubar_dom(&menu);

    // Html root (not Body) so we don't double-nest <body> / double the UA margin.
    // Order: menu bar first, then the user's content below it.
    Dom::create_html().with_children(DomVec::from_vec(vec![menubar, user_dom]))
}

/// `LayoutRect` (integer origin, used by the layout query API) → `LogicalRect`
/// (float, what the FLIP maths wants).
///
/// The layout query rounds positions to whole pixels on the way out; a FLIP
/// computed from those is at worst half a pixel off at the START of a
/// transition, which is invisible and converges to the exact solved rect
/// because the animation's endpoint is the layout, not this rect.
fn layout_rect_to_logical(r: azul_css::props::basic::LayoutRect) -> azul_core::geom::LogicalRect {
    azul_core::geom::LogicalRect {
        origin: azul_core::geom::LogicalPosition::new(r.origin.x as f32, r.origin.y as f32),
        size: azul_core::geom::LogicalSize::new(r.size.width as f32, r.size.height as f32),
    }
}

/// Find the open `<transient-window>`s in the root dom, lay their content out,
/// and reconcile the manager. Stores the resulting diff on the window so the
/// backend can act on it after layout.
///
/// Split out of `regenerate_layout` because it needs `&mut layout_window`
/// twice in ways the borrow checker will not allow inline: once to read the
/// root layout (for anchor rects) and once to lay out each popup's content.
pub(crate) fn reconcile_transient_windows(
    layout_window: &mut LayoutWindow,
    current_window_state: &FullWindowState,
) {
    use azul_core::dom::DomId;
    use azul_layout::transient::collect_open_transient_windows;

    // 1. What does the parent layout say is open, and where is each anchor?
    let wanted = {
        let Some(root) = layout_window.layout_results.get(&DomId::ROOT_ID) else {
            return;
        };
        let styled = &root.styled_dom;
        // Anchor rects come from the ROOT dom's layout. Borrow the result
        // immutably for the whole collection, then drop it before laying out.
        let forced: Vec<_> = layout_window.transient_windows.forced_open_nodes().to_vec();
        // A window the user docked onto a drop zone anchors to the zone.
        let overrides = layout_window.transient_windows.anchor_overrides();
        // Anchors are VIEWPORT rects (scroll taken off): the popup must open
        // where the anchor is on screen, not where the unscrolled layout has it.
        let rects: Vec<_> = collect_open_transient_windows(styled, &forced, &overrides, |node| {
            layout_window.get_node_rect_in_viewport(azul_core::dom::DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(node)),
            })
        });
        rects
    };

    // 2. Reconcile, measuring each popup's content on demand (on scratch
    //    caches — the popup window lays the content out itself).
    let diff = {
        // `reconcile` wants a closure that reads layout_window while the
        // manager is borrowed mutably — split the borrow by taking the
        // manager out, reconciling, and putting it back.
        let mut manager = core::mem::take(&mut layout_window.transient_windows);
        let diff = manager.reconcile(&wanted, |content_dom, placement| {
            let measured = layout_window.layout_transient_content(
                placement.node,
                content_dom,
                placement.size,
                current_window_state,
            )?;
            // A DROP-DOWN IS AT LEAST AS WIDE AS THE CONTROL IT DROPS OUT OF.
            // A popup on the top or bottom edge is the `<select>` shape - a
            // combo box's option list, a date field's calendar - and one
            // narrower than its own field reads as a stray context menu
            // (2026-09-01 request, alongside the same rule for native menus).
            // A MINIMUM only: wider content still wins, and an app that gave
            // an explicit `size` has already been handed it back above, so
            // this cannot override it. Left/right anchors are side panels,
            // where the anchor's width means nothing.
            let widened = if matches!(
                placement.anchor,
                azul_core::transient::TransientAnchor::Bottom
                    | azul_core::transient::TransientAnchor::Top
            ) {
                azul_core::geom::LogicalSize::new(
                    measured.width.max(placement.anchor_rect.size.width),
                    measured.height,
                )
            } else {
                measured
            };
            Some(widened)
        });
        layout_window.transient_windows = manager;
        diff
    };

    // The app's `torn` attribute tore a window off / docked it: the node
    // hears about it like it hears about a drag.
    for (node, torn, bounds) in &diff.torn_changes {
        let now = std::time::Instant::now().into();
        layout_window
            .pending_lifecycle_events
            .push(azul_core::diff::create_tearoff_event(
                *node,
                DomId::ROOT_ID,
                &now,
                *torn,
                *bounds,
            ));
    }
    if !diff.opened.is_empty() || !diff.closed.is_empty() || !diff.moved.is_empty() {
        log_debug!(
            LogCategory::Layout,
            "[transient] opened={} moved={} closed={}",
            diff.opened.len(),
            diff.moved.len(),
            diff.closed.len()
        );
    }
    // Accumulate, never assign: a layout call may run several passes before
    // the backend takes the diff, and an exit that reaches no reconcile (the
    // fingerprint-equal "layout unchanged" path) must not replay an older
    // pass's opened/closed list either — the continuity test caught a popup
    // about to be re-created that way. `merge` also cancels an open+close
    // pair the backend never saw, so nothing flashes.
    layout_window.pending_transient_diff.merge(diff);
}

/// Build a `LayoutWindow` that SHARES the app-level font manager.
///
/// This is the single decision point for "where does a window's `FontManager`
/// come from", and it exists because there are four window backends that each
/// used to answer it independently with `LayoutWindow::new(fc_cache)` - i.e.
/// a brand-new `FontManager` per window.
///
/// That is not merely wasteful. A `FontManager` owns the `embedded_fonts` pool,
/// which is the ONLY place a face handed over as `StyleFontFamily::Ref`
/// (Material Icons, and anything a font-backed icon pack resolves to) is ever
/// named. Two windows with two managers therefore disagree about which faces
/// exist, and a face registered while laying out one window is invisible to the
/// next - which surfaces as a `.notdef` tofu box, with every intermediate step
/// reporting success.
///
/// `clone_shared()` shares the `parsed_fonts` and `embedded_fonts` pools while
/// giving the window a private `fc_cache` field, so it can still swap in its own
/// registry snapshot at layout time (`replace_fc_cache`) without disturbing
/// anyone else.
///
/// Falls back to a private manager when there is no app-level one, which is the
/// old behaviour and keeps this infallible to adopt.
pub fn layout_window_sharing_fonts(
    app_font_manager: Option<
        &std::sync::Arc<azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>>,
    >,
    fc_cache: &rust_fontconfig::FcFontCache,
) -> Result<azul_layout::window::LayoutWindow, azul_layout::solver3::LayoutError> {
    match app_font_manager {
        Some(fm) => Ok(azul_layout::window::LayoutWindow::from_font_manager(
            fm.clone_shared(),
        )),
        None => azul_layout::window::LayoutWindow::new(fc_cache.clone()),
    }
}
