//! Headless backend for testing and CPU-only rendering (`AZ_BACKEND=headless`).
//!
//! This backend implements the full `PlatformWindow` trait without
//! GPU / OpenGL. It behaves like a real platform window — DOM is laid out,
//! callbacks fire, timers tick — but rendering goes through a **CpuBackend**
//! instead of WebRender.
//!
//! ## CpuBackend
//!
//! `CpuBackend` has a similar *purpose* to the WebRender pipeline
//! (render-api, renderer, hit-tester) but is fully CPU-based and much
//! simpler. It is intentionally less efficient — the target use-case is
//! small, ancillary windows (Linux menu bars, tooltip popups) and headless
//! E2E tests, not high-framerate rendering.
//!
//! ```text
//! WebRender path:   DisplayList → WrRenderApi → Renderer (GPU) → swapBuffers
//! CpuBackend path:  DisplayList → cpurender   → Pixmap  (CPU)  → (no-op / PNG)
//! ```
//!
//! ## Headless Event Loop
//!
//! `HeadlessWindow::run()` blocks in an infinite loop just like every other
//! platform's `run()`. Instead of busy-waiting or `thread::sleep`, it
//! blocks on a **`Condvar`** that is signalled when:
//!
//! * An event is injected (via `inject_event` / debug server)
//! * A timer fires (the earliest timer deadline is used as `wait_timeout`)
//! * A background thread completes
//!
//! This means the headless loop consumes **zero CPU** when idle, just
//! like the native `WaitMessage()` / `XNextEvent()` / `NSEvent` loops
//! on real platforms.
//!
//! If nothing can wake the loop (no timers, no threads, no debug server)
//! a warning is printed to stderr and the loop blocks indefinitely
//! (the programme hangs). This is intentional — it is the same behaviour
//! you would get from a real window that nobody interacts with.
//!
//! ## Architecture
//!
//! ```text
//! HeadlessWindow
//! ├── common: CommonWindowState        (shared with all platforms)
//! ├── cpu_backend: CpuBackend          (replaces WebRender)
//! ├── event_queue: VecDeque<HeadlessEvent> (programmatic event injection)
//! └── pending_window_creates: Vec      (popup/dialog queue)
//! ```

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Condvar, Mutex},
    time::{Duration, Instant},
};

use azul_core::{
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    hit_test::DocumentId,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::{AppConfig, AppTerminationBehavior, IdNamespace, ImageCache, RendererResources},
    window::{
        AcceleratorKey, FullScreenMode, RawWindowHandle, ScrollResult, TouchPoint, TouchPointVec,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::{registry::FcFontRegistry, FcFontCache};

use crate::{
    desktop::{
        shell2::common::{
            accessibility::A11yActionQueue,
            debug_server::{self, LogCategory},
            event::{self, CommonWindowState, HitTestNode, PlatformWindow},
            WindowError,
        },
        wr_translate2::{AsyncHitTester, WrRenderApi},
    },
    impl_platform_window_getters, log_debug, log_error, log_info, log_trace, log_warn,
};

/// Events that can be injected into a HeadlessWindow for testing or
/// via the debug server.
#[derive(Debug, Clone)]
pub enum HeadlessEvent {
    /// Simulate window close
    Close,
    /// Simulate mouse move to position
    MouseMove { x: f32, y: f32 },
    /// Simulate mouse button press
    MouseDown {
        button: azul_core::events::MouseButton,
    },
    /// Simulate mouse button release
    MouseUp {
        button: azul_core::events::MouseButton,
    },
    /// Simulate key press
    KeyDown { virtual_keycode: VirtualKeyCode },
    /// Simulate key release
    KeyUp { virtual_keycode: VirtualKeyCode },
    /// Simulate text input
    TextInput { text: String },
    /// Simulate window resize
    Resize { width: f32, height: f32 },
    /// Simulate scroll wheel
    Scroll { delta_x: f32, delta_y: f32 },
    /// Simulate an OS file drag hovering the window at (x, y) (MWA-A4).
    /// Mirrors the desktop ingress: XdndPosition / draggingUpdated /
    /// IDropTarget::DragOver / wl_data_device.motion.
    FileHover { x: f32, y: f32, paths: Vec<String> },
    /// Simulate an OS file drop at (x, y) (MWA-A4). Mirrors XdndDrop /
    /// performDragOperation / IDropTarget::Drop / wl_data_device.drop.
    FileDrop { x: f32, y: f32, paths: Vec<String> },
    /// Simulate the OS file drag leaving the window without dropping
    /// (MWA-A4). Mirrors XdndLeave / draggingExited / DragLeave.
    FileHoverCancel,

    // ── Added by the input-wiring arc ────────────────────────────────────
    //
    // Everything that arc wired needed a way in here, or it could not be
    // regression-tested at all. The existing set could inject a scroll but
    // not its PHASE, a touch but not a pen, and nothing device-shaped —
    // which is exactly why several of these paths had gone stale unnoticed.
    /// Scroll with an explicit phase source, so `ScrollStart` / `ScrollEnd`
    /// can be pinned.
    ///
    /// The plain `Scroll` variant is always `WheelDiscrete`, which is why the
    /// scroll-phase work could not be tested through it: a wheel has no
    /// gesture boundaries to observe.
    ScrollPhased {
        delta_x: f32,
        delta_y: f32,
        source: azul_layout::managers::scroll_state::ScrollInputSource,
    },
    /// Move / press / release a pen, with the full sample the shells feed.
    ///
    /// `in_contact` is separate from `pressure` on purpose: a pen hovering at
    /// zero pressure and a pen touching with zero pressure are different
    /// states, and the pen event diff keys on the former.
    Pen {
        x: f32,
        y: f32,
        pressure: f32,
        tilt_x: f32,
        tilt_y: f32,
        in_contact: bool,
        is_eraser: bool,
    },
    /// A pen barrel gesture (Apple Pencil squeeze / double-tap).
    PenBarrel { squeeze: bool },
    /// A native gesture, as the platform recognizers deliver it — pinch,
    /// rotation, swipe, long-press. Bypasses the in-process detector, which
    /// is the point: it is how macOS, iOS, Android and now Wayland/X11 feed
    /// gestures, and the detector cannot produce a touchpad pinch at all.
    Gesture {
        gesture: azul_layout::managers::gesture::NativeGestureEvent,
    },
    /// A gamepad state snapshot.
    Gamepad {
        state: azul_core::gamepad::GamepadState,
    },
    /// A sensor reading.
    Sensor {
        reading: azul_core::sensors::SensorReading,
    },
    /// IME composition: set the preedit, or commit it.
    ///
    /// `commit: true` ends the composition with `text` as the committed
    /// string; `false` sets it as the in-progress preedit.
    Composition { text: String, commit: bool },
    /// A device or monitor was attached or detached.
    Hotplug { is_monitor: bool, connected: bool },
    /// Raw, pre-acceleration pointer motion. Only observable while the
    /// pointer is locked, which the harness must set first — the same gate
    /// the real backends apply.
    RawMotion { dx: f64, dy: f64 },
    /// A rotary control turned.
    Dial { delta_rad: f32 },
    /// A modifier or lock key changed.
    Modifiers {
        shift: bool,
        ctrl: bool,
        alt: bool,
        meta: bool,
        caps_lock: bool,
        num_lock: bool,
    },
}

/// MWA-A4: feed the gesture manager's input sessions exactly like the
/// desktop shells do (every OS mouse handler calls `record_input_sample`).
/// Headless previously only mutated `mouse_state`, so `detect_drag`,
/// `detect_double_click`, `detect_long_press`, swipes and node-DnD were
/// structurally invisible to headless E2E — the entire gesture surface was
/// untestable.
fn record_headless_input(window: &mut HeadlessWindow, is_button_down: bool, is_button_up: bool) {
    use crate::desktop::shell2::common::event::{
        BUTTON_STATE_LEFT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE, BUTTON_STATE_RIGHT,
    };
    let ms = &window.common.current_window_state().mouse_state;
    let mut button_state = BUTTON_STATE_NONE;
    if ms.left_down {
        button_state |= BUTTON_STATE_LEFT;
    }
    if ms.right_down {
        button_state |= BUTTON_STATE_RIGHT;
    }
    if ms.middle_down {
        button_state |= BUTTON_STATE_MIDDLE;
    }
    let pos = ms
        .cursor_position
        .get_position()
        .unwrap_or(LogicalPosition { x: 0.0, y: 0.0 });
    window.record_input_sample(pos, button_state, is_button_down, is_button_up, None);
}

/// Outcome of a single `CpuBackend::render_frame` call.
///
/// MOVED to `azul_layout::window::FrameDamage` so that it can be stored on
/// `LayoutWindow` (and therefore be reachable from `CallbackInfo`, i.e. from an
/// E2E assertion). Re-exported here so that every existing
/// `crate::desktop::shell2::headless::FrameDamage` path keeps working.
pub use azul_layout::window::{FrameDamage, FrameReport};

// The CPU frame lives in `azul_layout::cpu_backend`: one implementation for the headless window,
// the E2E runner and every shell.
pub use azul_layout::cpu_backend::{frame_may_reuse_previous_pixels, CpuBackend};

/// #27 native-backbuffer master switch, shared by every platform shell:
/// `AZ_NATIVE_BACKBUFFER=0` forces the legacy owned-pixmap + copy present
/// (also the automatic fallback when a platform's buffer can't take the
/// renderer's RGBA byte order). NOTE: in native mode `CpuBackend.last_frame`
/// stays `None` — tools that read the retained frame (live screenshot dumps)
/// need `AZ_NATIVE_BACKBUFFER=0`.
/// #32: in-place R<->B swizzle over `rects` (x, y, w, h in buffer px) of a
/// tightly-packed 4-byte-per-pixel buffer - the ONE implementation lives in
/// the renderer (`cpurender::swap_rb_in_rects`), where its overlap law is
/// tested, and every shell converts through it.
pub(crate) fn swizzle_rb_in_rects(
    buf: &mut [u8],
    stride_bytes: usize,
    buf_height: usize,
    rects: &[(i32, i32, i32, i32)],
) {
    azul_layout::cpurender::swap_rb_in_rects(buf, stride_bytes, buf_height, rects);
}

/// Copy `rects` (x, y, w, h in buffer px) of the renderer's R,G,B,A frame
/// (`src`, `src_pitch` bytes per row) into a platform buffer with its OWN row
/// pitch (`dst`, `dst_pitch` bytes per row) at the same coordinates - the one
/// damage-rect upload every CPU present shares (a Wayland `wl_shm` slot with
/// 256-byte rows, an X11 MIT-SHM segment, a tooltip buffer). `swap_rb`
/// converts to B,G,R,A on the way (ARGB8888 / the X visual's order). Rects are
/// clipped to `width` x `height` and to both buffers; overlapping rects are
/// harmless (a copy, not an in-place toggle). Returns the bytes written.
pub(crate) fn copy_rgba_rects_into(
    dst: &mut [u8],
    dst_pitch: usize,
    src: &[u8],
    src_pitch: usize,
    width: usize,
    height: usize,
    rects: &[(u32, u32, u32, u32)],
    swap_rb: bool,
) -> usize {
    let mut written = 0usize;
    // Rows that fit BOTH buffers (a configure race can leave either short).
    let rows_dst = if dst_pitch == 0 { 0 } else { dst.len() / dst_pitch };
    let rows_src = if src_pitch == 0 { 0 } else { src.len() / src_pitch };
    let h = height.min(rows_dst).min(rows_src);
    let w = width.min(dst_pitch / 4).min(src_pitch / 4);
    for &(rx, ry, rw, rh) in rects {
        let x0 = (rx as usize).min(w);
        let y0 = (ry as usize).min(h);
        let x1 = (rx as usize).saturating_add(rw as usize).min(w);
        let y1 = (ry as usize).saturating_add(rh as usize).min(h);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let n = (x1 - x0) * 4;
        for y in y0..y1 {
            let s = &src[y * src_pitch + x0 * 4..y * src_pitch + x0 * 4 + n];
            let d = &mut dst[y * dst_pitch + x0 * 4..y * dst_pitch + x0 * 4 + n];
            if swap_rb {
                for (sp, dp) in s.chunks_exact(4).zip(d.chunks_exact_mut(4)) {
                    dp[0] = sp[2];
                    dp[1] = sp[1];
                    dp[2] = sp[0];
                    dp[3] = sp[3];
                }
            } else {
                d.copy_from_slice(s);
            }
            written += n;
        }
    }
    written
}

pub fn native_backbuffer_enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        std::env::var("AZ_NATIVE_BACKBUFFER")
            .map(|v| v != "0")
            .unwrap_or(true)
    })
}

// ---------------------------------------------------------------------------
// HeadlessWindow
// ---------------------------------------------------------------------------

/// Shared wake-up state for the condvar-based event loop.
///
/// The `Condvar` is signalled whenever new work is available (event
/// injected, timer registered, thread completed).  This lets the
/// blocking loop sleep with zero CPU usage when idle.
struct WakeState {
    /// `true` when the loop should re-check for work.
    woken: bool,
}

/// Headless / CPU-only window implementing the full `PlatformWindow` trait.
///
/// Behaves identically to platform windows for layout, callbacks, and state
/// management.  Instead of a GPU context it holds a [`CpuBackend`] for
/// hit-testing and optional CPU rendering.
/// The next headless window's [`HeadlessWindow::serial`]. It starts at 1 because an
/// `owner_window_id` of 0 means "no owner".
static NEXT_SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Names `creator` (a [`HeadlessWindow::serial`]) as the owner of an owned window that
/// names none yet.
fn stamp_owner(options: &mut WindowCreateOptions, creator: u64) {
    if options.owned && options.parent_window_id == 0 {
        options.parent_window_id = creator;
    }
}

pub struct HeadlessWindow {
    /// Common window state (layout, resources, etc.) — shared with all platforms.
    pub common: CommonWindowState,
    /// CPU rendering backend (replaces WebRender).
    pub cpu_backend: CpuBackend,
    /// Whether the window is "open".
    is_open: bool,
    /// Event queue for programmatic event injection.
    event_queue: VecDeque<HeadlessEvent>,
    /// Thread poll timer running flag.
    thread_poll_timer_running: bool,
    /// Pending window creation requests (for popup menus, dialogs, etc.).
    pub pending_window_creates: Vec<WindowCreateOptions>,
    /// The windows opened from this one's callbacks (`CallbackInfo::create_window`: menus,
    /// dialogs, an app's second window), pumped by this window's loop
    /// ([`Self::pump_children`]).
    pub children: Vec<HeadlessWindow>,
    /// This window's id among the headless windows. Headless windows have no platform id,
    /// so an owned window records this number as its owner (`owner_window_id`).
    serial: u64,
    /// "Rebuild every window": a generation the root and its children share
    /// (`request_regeneration_all_windows` counts it up), and the generation this window has
    /// rebuilt for - the headless twin of the OS backends' registry walk.
    regenerate_all: Arc<std::sync::atomic::AtomicU64>,
    regenerate_all_seen: u64,
    /// Config snapshot (needed for spawning sub-windows).
    config: AppConfig,
    /// Icon provider (shared across all windows).
    icon_provider: SharedIconProvider,
    /// Font registry (needed for spawning sub-windows).
    font_registry: Option<Arc<FcFontRegistry>>,
    /// `WindowCreateOptions::create_callback`, deferred until `run()`.
    ///
    /// Every OS shell fires this after the window exists and before the
    /// first layout; headless dropped it, so an app installing its startup
    /// timer there idled forever under `AZ_BACKEND=headless`.
    create_callback: azul_layout::callbacks::OptionCallback,
    /// Condvar + mutex used to block the event loop until work arrives.
    wake_condvar: Arc<Condvar>,
    wake_mutex: Arc<Mutex<WakeState>>,
    /// Accessibility actions waiting to be applied.
    ///
    /// Named to match the four desktop backends' `accessibility_adapter`
    /// because it fills the same slot in the frame loop: headless has no OS
    /// assistive technology to talk to, so instead of an `accesskit` adapter
    /// this is a plain injectable queue. That is not a lesser thing here — it
    /// is the ONLY way a11y behaviour is testable at all, since headless is
    /// the backend the E2E corpus runs on. Fed by
    /// [`HeadlessWindow::inject_accessibility_action`], drained by
    /// [`HeadlessWindow::process_accessibility_actions`].
    pub accessibility_adapter: A11yActionQueue,
    /// Publish every painted frame's damage-driven framebuffer onto the
    /// `LayoutWindow` (`debug_server::e2e_set_presented_frame`), for the E2E
    /// `assert_damage_sound` `pixel_identity` check. On when a script can ask
    /// - the debug server or `AZ_E2E` was on when the window opened - since
    /// it copies every frame.
    pub publish_presented_frame: bool,
    /// The `<webview>` backend: the shared recorder, no browser
    /// (`common::webview::HeadlessWebViews`).
    pub webviews: crate::desktop::shell2::common::webview::HeadlessWebViews,
}

impl HeadlessWindow {
    /// What the web view backend was told - its views, their placements
    /// and the pages each went to (`managers::webview::WebViewRecorder`).
    #[must_use]
    pub const fn webview_recorder(&self) -> &azul_layout::managers::webview::WebViewRecorder {
        self.webviews.recorder()
    }

    /// Create a new headless window with the given options.
    ///
    /// This constructor mirrors the real platform window constructors:
    /// 1. Creates `LayoutWindow` with font cache
    /// 2. Initialises `CommonWindowState`
    /// 3. Sets up the `CpuBackend`
    ///
    /// No system resources (window handle, GL context) are allocated.
    pub fn new(
        options: WindowCreateOptions,
        app_data: Arc<RefCell<RefAny>>,
        undo_manager: event::SharedUndoManager,
        config: AppConfig,
        icon_provider: SharedIconProvider,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<FcFontRegistry>>,
    ) -> Result<Self, WindowError> {
        // Extract create_callback before consuming options (same as every
        // platform shell) — invoked in run() ahead of the initial layout.
        let create_callback = options.create_callback.clone();
        let bg_light = options.background_color_light;
        let bg_dark = options.background_color_dark;
        let full_window_state = options.window_state;

        // Create layout window — same as real platforms
        let mut layout_window = LayoutWindow::new(fc_cache.as_ref().clone())
            .map_err(|e| WindowError::PlatformError(format!("Layout init failed: {:?}", e)))?;
        // Headless = the e2e test driver: deterministic, no caret / selection
        // tween (a screenshot must never catch geometry mid-glide).
        layout_window.system_animations_override =
            Some(azul_core::resources::SystemAnimations::disabled());
        layout_window.current_window_state = full_window_state.clone();
        layout_window.routes = config.routes.clone();
        layout_window.set_app_localization(&config);
        layout_window.set_app_e2e(&config);
        layout_window.set_app_node_identity(&config);
        // A recording web view backend (`HeadlessWebViews`): `<webview>`s are
        // created and placed through it, and their pages played by the test.
        layout_window
            .webviews
            .set_platform(azul_layout::managers::webview::WebViewPlatform::Backend);

        let wake_condvar = Arc::new(Condvar::new());
        let wake_mutex = Arc::new(Mutex::new(WakeState { woken: false }));

        // The style comes from the CONFIG, which already carries one
        // (`AppConfig::create()` discovers it once), exactly like
        // `AppResources::new_with_font_manager` does.
        //
        // Re-discovering here read the DEVELOPER'S DESKTOP instead, which made
        // every headless test non-hermetic: the layout-sensitive ones
        // (ribbon tab geometry, caret pixels, display-list patch equality)
        // silently depended on the ambient GTK/KDE font, so the same commit
        // passed on one machine and failed on another. It also spawned ~20
        // `gsettings`/`kreadconfig` processes per window construction, in a
        // suite that builds thousands of them.
        let mut common = CommonWindowState::new(
            full_window_state,
            options.mode,
            bg_light,
            bg_dark,
            fc_cache,
            Arc::new(config.system_style.clone()),
            app_data,
            undo_manager,
        );
        common.set_layout_window(layout_window);
        common.cpu_hit_tester = Some(azul_layout::headless::CpuHitTester::new());

        Ok(Self {
            common,
            cpu_backend: CpuBackend {
                // Offscreen output must be reproducible, so it keeps the plain
                // white canvas rather than the host desktop's colour.
                follow_system_background: false,
                ..CpuBackend::for_config(&config)
            },
            is_open: true,
            event_queue: VecDeque::new(),
            thread_poll_timer_running: false,
            pending_window_creates: Vec::new(),
            children: Vec::new(),
            serial: NEXT_SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            regenerate_all: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            regenerate_all_seen: 0,
            config,
            icon_provider,
            font_registry,
            create_callback,
            wake_condvar,
            wake_mutex,
            accessibility_adapter: A11yActionQueue::new(),
            publish_presented_frame: debug_server::is_debug_enabled(),
            webviews: crate::desktop::shell2::common::webview::HeadlessWebViews::default(),
        })
    }

    /// Makes this window draw with `registered_fonts`, the faces the app ships and the ones it
    /// registers while it runs: those of the app that runs it (`AppInternal::font_manager`) or
    /// of the window that opened it. A window made by [`Self::new`] has a registry of its own,
    /// as the platform windows would without the app's font manager.
    pub fn share_registered_fonts(
        &mut self,
        registered_fonts: &Arc<azul_layout::text3::fontreg::RegisteredFonts>,
    ) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .font_manager
                .set_registered_fonts(Arc::clone(registered_fonts));
        }
    }

    // === Lifecycle ===

    /// One turn of this window's loop: the injected events, the virtual views, the
    /// accessibility actions, (root only) the notifications and global hotkeys, the timers and
    /// threads with the frames they ask for, a close the callbacks requested, and the frame a
    /// raised regeneration flag still owes. `run()` turns it for the root window;
    /// [`Self::pump_children`] for every child window.
    pub fn pump_once(&mut self, is_root: bool) {
        // ── Phase 0: another window asked every window to rebuild ─
        let generation = self
            .regenerate_all
            .load(std::sync::atomic::Ordering::SeqCst);
        if generation != self.regenerate_all_seen {
            self.regenerate_all_seen = generation;
            self.common
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            self.service_frame(azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // ── Phase 1: Process injected events ─────────────────
        let mut events_need_redraw = false;
        // The strongest ProcessEventResult of this drain — decides whether
        // the frame below may rebuild the DOM or must keep it (see
        // service_frame).
        let mut events_result = azul_core::events::ProcessEventResult::DoNothing;
        while let Some(event) = self.poll_event() {
            match event {
                HeadlessEvent::Close => {
                    // The window manager's close (the title-bar X) is a
                    // REQUEST: the protocol runs, so the app's CloseRequested
                    // callbacks ("Save changes?") can veto it. It used to
                    // close outright.
                    let outcome = self.run_close_protocol("headless.close");
                    if outcome.confirmed {
                        self.close();
                    } else {
                        events_result = events_result.max(outcome.result);
                        if !matches!(
                            outcome.result,
                            azul_core::events::ProcessEventResult::DoNothing
                        ) {
                            events_need_redraw = true;
                        }
                    }
                }
                HeadlessEvent::FileHover { x, y, paths } => {
                    // MWA-A4: same ingress the OS backends perform —
                    // position + hit test + hovered-file into the manager,
                    // then an event pass (dispatches HoveredFile).
                    use azul_core::window::CursorPosition;
                    self.snapshot_window_state_baseline("headless.run.file_hover");
                    let pos = LogicalPosition { x, y };
                    self.common.mouse_state_mut().cursor_position =
                        CursorPosition::InWindow(pos);
                    self.update_hit_test_at(pos);
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        // MWA-B7: full multi-file list, like the OS shells.
                        lw.file_drop_manager
                            .set_hovered_files(paths.into_iter().map(Into::into).collect());
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                }
                HeadlessEvent::FileDrop { x, y, paths } => {
                    use azul_core::window::CursorPosition;
                    self.snapshot_window_state_baseline("headless.run.file_drop");
                    let pos = LogicalPosition { x, y };
                    self.common.mouse_state_mut().cursor_position =
                        CursorPosition::InWindow(pos);
                    self.update_hit_test_at(pos);
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.file_drop_manager
                            .set_dropped_files(paths.into_iter().map(Into::into).collect());
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                    // Post-pass cleanup, as the OS backends do it: the drop
                    // is a one-shot (the drop itself ended the hover).
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.file_drop_manager.set_dropped_file(None);
                    }
                }
                HeadlessEvent::FileHoverCancel => {
                    self.snapshot_window_state_baseline("headless.run.file_hover_cancel");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        // Some→None flags the cancel; the pass dispatches
                        // HoveredFileCancelled, then we clear the flag.
                        lw.file_drop_manager.set_hovered_file(None);
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.file_drop_manager.clear_hover_cancelled();
                    }
                }
                HeadlessEvent::ScrollPhased {
                    delta_x,
                    delta_y,
                    source,
                } => {
                    self.snapshot_window_state_baseline("headless.run.scroll_phased");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.scroll_manager.note_scroll_phase(source);
                        lw.scroll_manager.pending_wheel_event = Some(LogicalPosition {
                            x: delta_x,
                            y: delta_y,
                        });
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    events_need_redraw = true;
                }
                HeadlessEvent::Pen {
                    x,
                    y,
                    pressure,
                    tilt_x,
                    tilt_y,
                    in_contact,
                    is_eraser,
                } => {
                    self.snapshot_window_state_baseline("headless.run.pen");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.gesture_drag_manager.update_pen_state_full(
                            LogicalPosition { x, y },
                            pressure,
                            (tilt_x, tilt_y),
                            in_contact,
                            is_eraser,
                            false,
                            1,
                            0.0,
                            0.0,
                            0,
                        );
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    events_need_redraw = true;
                }
                HeadlessEvent::PenBarrel { squeeze } => {
                    self.snapshot_window_state_baseline("headless.run.pen_barrel");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.gesture_drag_manager.note_pen_barrel_gesture(squeeze);
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::Gesture { gesture } => {
                    self.snapshot_window_state_baseline("headless.run.gesture");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.gesture_drag_manager.inject_native_gesture(gesture);
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    events_need_redraw = true;
                }
                HeadlessEvent::Gamepad { state } => {
                    self.snapshot_window_state_baseline("headless.run.gamepad");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.gamepad_manager.set_state(state);
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::Sensor { reading } => {
                    self.snapshot_window_state_baseline("headless.run.sensor");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.sensor_manager.set_reading(reading);
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::Composition { text, commit } => {
                    self.snapshot_window_state_baseline("headless.run.composition");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        if commit {
                            lw.text_edit_manager.commit_composition(text.clone());
                        } else {
                            let caret = text.len() as i32;
                            lw.text_edit_manager.set_preedit(text.clone(), caret, caret);
                        }
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    events_need_redraw = true;
                }
                HeadlessEvent::Hotplug {
                    is_monitor,
                    connected,
                } => {
                    self.snapshot_window_state_baseline("headless.run.hotplug");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        if is_monitor {
                            lw.device_event_manager.note_monitor(connected);
                        } else {
                            lw.device_event_manager.note_device(connected);
                        }
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::RawMotion { dx, dy } => {
                    self.snapshot_window_state_baseline("headless.run.raw_motion");
                    // The real backends drop raw motion unless the pointer
                    // is locked, so the harness applies the same gate —
                    // a test that forgets to lock should see nothing,
                    // exactly as the app would.
                    let locked = self
                        .common
                        .current_window_state()
                        .mouse_state
                        .is_cursor_locked;
                    if locked {
                        if let Some(lw) = self.common.layout_window.as_mut() {
                            lw.device_event_manager.note_raw_motion(dx, dy, 1);
                        }
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::Dial { delta_rad } => {
                    self.snapshot_window_state_baseline("headless.run.dial");
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.gesture_drag_manager.update_dial_state(
                            azul_layout::managers::gesture::DialState {
                                device_id: 1,
                                delta_rad,
                                detent_count: 0.0,
                                pressed: false,
                                contact_position: azul_core::geom::OptionLogicalPosition::None,
                            },
                        );
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::Modifiers {
                    shift,
                    ctrl,
                    alt,
                    meta,
                    caps_lock,
                    num_lock,
                } => {
                    self.snapshot_window_state_baseline("headless.run.modifiers");
                    {
                        let ks = self.common.keyboard_state_mut();
                        ks.modifiers = azul_core::events::KeyModifiers {
                            shift,
                            ctrl,
                            alt,
                            meta,
                        };
                        ks.locks = azul_core::window::KeyLocks {
                            caps_lock,
                            num_lock,
                            scroll_lock: false,
                        };
                    }
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                }
                HeadlessEvent::MouseMove { x, y } => {
                    use azul_core::window::CursorPosition;
                    self.snapshot_window_state_baseline("headless.run.mouse_move");
                    let pos = LogicalPosition { x, y };
                    self.common.mouse_state_mut().cursor_position =
                        CursorPosition::InWindow(pos);
                    // MWA-C-scroll: a held scrollbar thumb takes the motion
                    // (the press router's other half, the same shared
                    // helper every desktop backend calls). It records the
                    // cursor and swallows the delta, so it does not surface
                    // as a MouseMove event later.
                    let thumb_drag = PlatformWindow::route_pointer_move(
                        self,
                        pos,
                        "headless.mouse_move.scrollbar_drag",
                    );
                    if let Some(r) = thumb_drag {
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    } else {
                        self.update_hit_test_at(pos);
                        record_headless_input(self, false, false); // MWA-A4
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                }
                HeadlessEvent::MouseDown { button } => {
                    self.snapshot_window_state_baseline("headless.run.mouse_down");
                    // MWA-C-scroll: the press router first, scrollbar then
                    // content (the same shared helper every desktop
                    // backend calls). A press a scrollbar takes is
                    // recorded and swallowed: it must not surface as a
                    // MouseDown event later.
                    let press_at = self
                        .common
                        .current_window_state()
                        .mouse_state
                        .cursor_position
                        .get_position();
                    let routed = match press_at {
                        Some(p) => PlatformWindow::route_pointer_press(
                            self,
                            p,
                            button,
                            "headless.mouse_down.scrollbar_click",
                        ),
                        None => None,
                    };
                    if let Some(r) = routed {
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    } else {
                        match button {
                            azul_core::events::MouseButton::Left => {
                                self.common.mouse_state_mut().left_down = true;
                            }
                            azul_core::events::MouseButton::Right => {
                                self.common.mouse_state_mut().right_down = true;
                            }
                            azul_core::events::MouseButton::Middle => {
                                self.common.mouse_state_mut().middle_down = true;
                            }
                            _ => {}
                        }
                        record_headless_input(self, true, false); // MWA-A4
                        let r = self.process_window_events(0);
                        events_result = events_result.max(r);
                        if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                            events_need_redraw = true;
                        }
                    }
                }
                HeadlessEvent::MouseUp { button } => {
                    self.snapshot_window_state_baseline("headless.run.mouse_up");
                    // MWA-C-scroll: the primary release lets go of a held
                    // thumb and clears the button its press latched (the
                    // shared helper). The button event is not stopped
                    // (`scrollbar_stops_the_button_event`): the pass below
                    // still runs, and finds no delta left to turn into a
                    // MouseUp.
                    let release_at = self
                        .common
                        .current_window_state()
                        .mouse_state
                        .cursor_position
                        .get_position();
                    if let Some(p) = release_at {
                        if PlatformWindow::end_scrollbar_drag(
                            self,
                            p,
                            button,
                            "headless.mouse_up.scrollbar_drag",
                        )
                        .is_some()
                        {
                            events_need_redraw = true;
                        }
                    }
                    match button {
                        azul_core::events::MouseButton::Left => {
                            self.common.mouse_state_mut().left_down = false;
                        }
                        azul_core::events::MouseButton::Right => {
                            self.common.mouse_state_mut().right_down = false;
                        }
                        azul_core::events::MouseButton::Middle => {
                            self.common.mouse_state_mut().middle_down = false;
                        }
                        _ => {}
                    }
                    record_headless_input(self, false, true); // MWA-A4
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                }
                HeadlessEvent::KeyDown { virtual_keycode } => {
                    self.snapshot_window_state_baseline("headless.run.key_down");
                    self.common.keyboard_state_mut().current_virtual_keycode =
                        azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                    self.common
                        .keyboard_state_mut()
                        .pressed_virtual_keycodes
                        .insert_hm_item(virtual_keycode);
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                }
                HeadlessEvent::KeyUp { virtual_keycode } => {
                    self.snapshot_window_state_baseline("headless.run.key_up");
                    self.common.keyboard_state_mut().current_virtual_keycode =
                        azul_core::window::OptionVirtualKeyCode::None;
                    self.common
                        .keyboard_state_mut()
                        .pressed_virtual_keycodes
                        .remove_hm_item(&virtual_keycode);
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                }
                HeadlessEvent::TextInput { text } => {
                    // This arm used to be an empty stub, which silently
                    // swallowed injected text (and made
                    // `synthesize_character_input` a no-op end to end).
                    self.snapshot_window_state_baseline("headless.run.text_input");
                    let r = self.apply_text_input_event(&text);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                }
                HeadlessEvent::Resize { width, height } => {
                    self.snapshot_window_state_baseline("headless.run.resize");
                    self.common
                        .update_window_state(event::WindowStateSource::Os, |ws| {
                            ws.size.dimensions.width = width;
                            ws.size.dimensions.height = height;
                        });
                    // Tag the upcoming regenerate_layout with the REAL
                    // reason, same as `simulate_resize()` — the two
                    // headless resize entry points used to disagree
                    // (this one left the implicit RefreshDom), so the
                    // user's LayoutCallback saw a phantom non-resize
                    // relayout depending on which API drove the resize.
                    self.common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
                    // Same shape as the ten sibling arms: run the pass so
                    // the size diff dispatches `WindowResize` — the one
                    // backend CI runs used to be the one backend that
                    // never fired it (the F4 class), and the un-passed
                    // delta tripped the AZ_VALIDATE assertion at the next
                    // `process_timers_and_threads()`.
                    let r = self.process_window_events(0);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                    events_need_redraw = true;
                }
                HeadlessEvent::Scroll { delta_x, delta_y } => {
                    let r = self.apply_wheel_scroll_event(delta_x, delta_y);
                    events_result = events_result.max(r);
                    if !matches!(r, azul_core::events::ProcessEventResult::DoNothing) {
                        events_need_redraw = true;
                    }
                }
            }
        }
        if events_result == azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows {
            self.request_regeneration_all_windows();
        }
        // MWA-C-virtual_view: drain queued VirtualView re-invocations
        // FIRST so their queue-time reasons (EdgeScrolled/DomRecreated)
        // reach the user callback — headless previously relied solely on
        // the full regenerate below, which resets invocation flags and
        // re-invokes everything as InitialRender (queue never drained,
        // reasons untestable in E2E).
        // One drain for every backend (re-invoke in place + CPU hit-tester
        // rebuild). A non-empty queue owes a frame even if a view declined
        // to rebuild, as before.
        let had_virtual_view_updates = self
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| !lw.pending_virtual_view_updates.is_empty());
        self.common.drain_virtual_view_updates();
        if had_virtual_view_updates {
            events_need_redraw = true;
        }

        if events_need_redraw {
            self.service_frame(events_result);
        }

        // ── Phase 1b: Apply queued accessibility actions ─────
        // The same slot `run.rs` gives the four desktop backends: actions
        // arrive off-loop (there, from an accesskit bus; here, from
        // `inject_accessibility_action`) and are drained by the frame pump
        // after input and before timers. Without this call the queue would
        // fill and nothing would ever read it — which is exactly the state
        // headless a11y was in.
        #[cfg(feature = "a11y")]
        self.process_accessibility_actions();

        // ── Phase 1b': Web views ─────────────────────────────
        // The recording backend gets this frame's placements and ops, and
        // what it reported - or a test / the debug server simulated
        // (`simulate_webview_*`) - runs the views' callbacks
        // (`common::webview::pump`).
        let webview_result = self.pump_webviews();
        if !matches!(
            webview_result,
            azul_core::events::ProcessEventResult::DoNothing
        ) {
            self.service_frame(webview_result);
        }

        // The notification mailbox and the global hotkeys belong to the process: the root
        // window pumps them, a child window must not take its parent's deliveries.
        if is_root {
            // ── Phase 1c: Native notifications ───────────────────
            // The slot the desktop loops give their notification pump. Here
            // the backend RECORDS instead of showing (`AZ_BACKEND=headless`,
            // see desktop/notifications), and any event queued into the
            // mailbox - by a test - runs its notification's callback through
            // the same `invoke_menu_callback` path the OS shells use.
            let deliveries = crate::desktop::notifications::pump_notifications();
            if !deliveries.is_empty()
                && crate::desktop::notifications::invoke_deliveries(self, deliveries)
            {
                self.service_frame(
                    azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow,
                );
            }

            // ── Phase 1c: Global hotkeys ─────────────────────────
            // Presses parked by the simulated backend (`simulate`, the
            // AZ_E2E `global_hotkey` op) run their callbacks against this
            // window - the slot the desktop run loops give them, next to the
            // tray pump - and a status change this window's `layout()` read
            // asks for one more pass.
            let hotkey_result = crate::desktop::global_hotkey::pump_headless(self);
            if !matches!(
                hotkey_result,
                azul_core::events::ProcessEventResult::DoNothing
            ) {
                self.service_frame(hotkey_result);
            }
        }

        // ── Phase 2: Tick timers and threads ─────────────────
        // Use the shared PlatformWindow trait method to invoke
        // expired timer callbacks and poll background threads.
        let needs_redraw = self.process_timers_and_threads();

        // In the CPU-only path there is no GPU compositor that can
        // handle scroll-offset-only or repaint-only updates.  Every
        // visual change (including scroll) requires a full display
        // list rebuild, so we re-render on any redraw signal — but
        // the relayout-only request decides WHICH pass runs: an in-place DOM
        // mutation (debug-server DOM ops, restyle, runtime text edit) must
        // re-run layout on the EXISTING StyledDom. Sending it through the
        // full `regenerate_layout()` is not a slower way to get the same
        // answer: that path bails out on `is_layout_equivalent(old, new)`,
        // which after an in-place mutation compares the DOM with itself,
        // reports "unchanged", and skips layout — leaving the old shaped
        // text and geometry on screen forever.
        if needs_redraw {
            // process_timers_and_threads already routed the tier: it
            // raised the regeneration request only for real RefreshDom
            // returns and relayout-only for in-place mutations. Passing
            // ShouldReRenderCurrentWindow here just says "a frame is
            // owed"; service_frame consumes the flags to pick the pass.
            self.service_frame(
                azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow,
            );
        }

        // ── Phase 2b: Honour `flags.close_requested` ─────────
        // `CallbackChange::CloseWindow` — the cross-platform "quit" API a
        // callback or timer uses — does not close anything itself: it sets
        // `flags.close_requested` and relies on the shell's loop to consume
        // it. Every desktop backend does (the Linux run loop's
        // `close_requested() → close()` check, Windows' WM_PAINT/WndProc
        // checks, macOS's sync_window_state) — headless did NOT, so an app
        // whose exit path is `window.close()` from a callback kept its loop
        // alive forever: the flag was set, `DoNothing` came back, and
        // `while self.is_open()` never terminated. With an active timer the
        // loop even kept polling at 60 Hz, which is exactly the
        // "self-test never exits after the last window closes" hang.
        // Checked here — after events (Phase 1), a11y actions (Phase 1b)
        // and timers/threads (Phase 2), the three places a callback can
        // run — so a close requested anywhere this iteration exits before
        // the condvar wait instead of after a wake that may never come.
        //
        // A close the APP raised (`close_window`, the CSD titlebar's close
        // button) is a request like the window manager's: the protocol runs
        // first, against the DOM this iteration's frames built, and a
        // CloseRequested callback that vetoes leaves the flag down.
        if let Some(outcome) = self.confirm_app_close("headless.app_close") {
            if !outcome.confirmed
                && !matches!(
                    outcome.result,
                    azul_core::events::ProcessEventResult::DoNothing
                )
            {
                self.service_frame(outcome.result);
            }
        }
        if self.common.current_window_state().flags.close_requested {
            log_info!(
                LogCategory::EventLoop,
                "[Headless] close_requested by callback — closing window"
            );
            self.close();
        }

        // ── Phase 2c: the frame a raised flag still owes ─────
        // Every desktop loop ends its turn with a gate that reads the
        // window's FLAGS, not its phases' results: X11's render gate,
        // Wayland's frame-callback re-arm, the Windows loop's
        // `regeneration_pending()` sweep (run.rs), macOS's timer tick. This
        // loop serviced a frame only when a phase REPORTED one, so a request
        // raised as a flag alone sat until an unrelated event forced a frame:
        // a screen reader's press (Phase 1b ends in `request_redraw`, which
        // no later phase reads), a resume delivered in a pass whose result
        // its caller drops, a request a frame's own lifecycle callbacks
        // raised mid-flight. `service_frame` picks the pass from the flags
        // (relayout-only first, so nothing is rebuilt that was not asked
        // for). A closed window owes nothing.
        if self.is_open()
            && (self.common.regeneration_pending() || self.common.resize_relayout_pending())
        {
            self.service_frame(azul_core::events::ProcessEventResult::ShouldReRenderCurrentWindow);
        }
    }

    /// Spawns a window for every pending create request and pumps the open child windows
    /// (one turn of each), dropping the closed ones.
    pub fn pump_children(&mut self) {
        while let Some(mut pending_create) = self.pending_window_creates.pop() {
            // An owned window this window opened has it as its owner. The creates its
            // children opened came up below with their owner already set.
            stamp_owner(&mut pending_create, self.serial);
            let owner = crate::desktop::shell2::common::owned_window::owner_of(&pending_create);
            // Every menu window is `azul-menu` (`desktop::menu::show_menu`):
            // one opened while another is open (a submenu, a second
            // drop-down) gets an id of its own - the first free of
            // `azul-menu-2`, `-3`, ... - or the debug server, which routes by
            // id, could only reach the first.
            if pending_create.window_state.flags.window_type
                == azul_core::window::WindowType::Menu
            {
                let free = crate::desktop::shell2::common::free_window_id(
                    pending_create.window_state.window_id.as_str(),
                    |id| {
                        self.children
                            .iter()
                            .any(|c| c.common.current_window_state().window_id.as_str() == id)
                    },
                );
                pending_create.window_state.window_id = free.into();
            }
            log_debug!(
                LogCategory::Window,
                "[Headless] Spawning sub-HeadlessWindow (type: {:?})",
                pending_create.window_state.flags.window_type
            );
            // The window that opens it shares its registered fonts with it.
            let registered_fonts = self
                .common
                .layout_window
                .as_ref()
                .map(|lw| Arc::clone(&lw.font_manager.registered_fonts));
            match HeadlessWindow::new(
                pending_create,
                self.common.app_data.clone(),
                self.common.undo_manager.clone(),
                self.config.clone(),
                self.icon_provider.clone(),
                self.common.fc_cache.clone(),
                self.font_registry.clone(),
            ) {
                Ok(mut child) => {
                    if let Some(registered_fonts) = &registered_fonts {
                        child.share_registered_fonts(registered_fonts);
                    }
                    child.common.owner_window_id = owner;
                    child.regenerate_all = self.regenerate_all.clone();
                    child.regenerate_all_seen = self
                        .regenerate_all
                        .load(std::sync::atomic::Ordering::SeqCst);
                    child.start_as_child();
                    debug_server::register_debug_timer_on_new_window(&mut child);
                    self.children.push(child);
                }
                Err(e) => {
                    log_error!(
                        LogCategory::Window,
                        "[Headless] Failed to create sub-HeadlessWindow: {:?}",
                        e
                    );
                }
            }
        }
        let mut opened_by_children = Vec::new();
        let mut a_menu_closed = false;
        // The threads of a child that closed go on (or wind down) in this window, as every
        // shell hands them to a window that stays open (`common::thread_handover`).
        let mut inherited = Vec::new();
        let mut inherited_keyring_ops = 0_u32;
        let mut closed = Vec::new();
        // With the root closed, its first open child pumps what belongs to the process (the
        // notifications, the global hotkeys), as the root did.
        let mut heir_pending = !self.is_open();
        self.children.retain_mut(|child| {
            let as_root = heir_pending && child.can_be_heir();
            heir_pending &= !as_root;
            child.pump_once(as_root);
            let child_serial = child.serial;
            for mut create in child.pending_window_creates.drain(..) {
                stamp_owner(&mut create, child_serial);
                opened_by_children.push(create);
            }
            let open = child.is_open();
            a_menu_closed |= !open && child.is_menu_window();
            if !open {
                closed.push(child.serial);
                if let Some(lw) = child.common.layout_window.as_mut() {
                    inherited.append(&mut lw.take_threads_for_heir());
                    inherited_keyring_ops += core::mem::take(&mut lw.keyring_manager.in_flight);
                }
            }
            open
        });
        let poll_was_needed = PlatformWindow::thread_poll_needed(self);
        let adopted = self.common.layout_window.as_mut().is_some_and(|lw| {
            lw.keyring_manager.mark_requests_dispatched(inherited_keyring_ops);
            lw.adopt_handed_over_threads(inherited)
        });
        if adopted {
            PlatformWindow::sync_thread_poll_timer(self, poll_was_needed);
        }
        if inherited_keyring_ops > 0 {
            PlatformWindow::sync_capability_pump_timer(self);
        }
        // A menu that closed takes its chain with it (X11's
        // `dismiss_chain_if_menu`): an item picked in a submenu, or an Escape
        // in it, leaves the menu it hangs off too. Every menu of this window
        // is one chain - a menu's own creates (its submenus) come up here.
        if a_menu_closed {
            let _ = self.dismiss_menu_windows();
        }
        // The items picked in those menus run in the windows that OPENED them (this one, or a
        // popup child that opened a menu), as a native menu's item runs: the menu only posted
        // the pick (`desktop::menu::MenuPicks`), so the text it sets, the title, its timers and
        // threads are the opener's, not the closed menu's (AzDrive's Saved searches, its
        // Refine). The frame they owe is the next turn's (`pump_once`'s Phase 2c).
        let mut picked = false;
        for child in &mut self.children {
            if !child.common.menu_picks.is_empty() {
                let _ = PlatformWindow::run_menu_picks(child);
                picked = true;
            }
        }
        if !self.common.menu_picks.is_empty() {
            let _ = PlatformWindow::run_menu_picks(self);
            picked = true;
        }
        if picked {
            self.wake();
        }
        self.pending_window_creates.extend(opened_by_children);
        self.close_owned_by(&closed);
    }

    /// Asks every open child window owned by one of the windows `closed` to close, through its
    /// own close guard (a window with unsaved work can still ask), as the desktop shells close
    /// an owner's owned windows. A window that closes this way takes its own owned windows
    /// with it on a later turn.
    fn close_owned_by(&mut self, closed: &[u64]) {
        if closed.is_empty() {
            return;
        }
        let mut asked = false;
        for child in &mut self.children {
            if child.is_open() && closed.contains(&child.common.owner_window_id) {
                PlatformWindow::request_close_by_app(child);
                asked = true;
            }
        }
        if asked {
            self.wake();
        }
    }

    /// The root window closed while other windows are open: the app goes on until its last
    /// window closes, as on every desktop. Its menus close with it, and so do the windows it
    /// owns (each through its close guard). Its threads and keyring work go on in the first
    /// window that stays open, as a closing child's go on in the root. Returns whether a
    /// window is left.
    fn hand_over_from_closed_root(&mut self) -> bool {
        let _ = self.dismiss_menu_windows();
        self.close_owned_by(&[self.serial]);
        let Some(heir) = self.children.iter_mut().find(|c| c.can_be_heir()) else {
            return false;
        };
        if let Some(lw) = self.common.layout_window.as_mut() {
            let threads = lw.take_threads_for_heir();
            let keyring_ops = core::mem::take(&mut lw.keyring_manager.in_flight);
            let poll_was_needed = PlatformWindow::thread_poll_needed(heir);
            if let Some(heir_lw) = heir.common.layout_window.as_mut() {
                heir_lw.keyring_manager.mark_requests_dispatched(keyring_ops);
                if heir_lw.adopt_handed_over_threads(threads) {
                    PlatformWindow::sync_thread_poll_timer(heir, poll_was_needed);
                }
            }
            if keyring_ops > 0 {
                PlatformWindow::sync_capability_pump_timer(heir);
            }
        }
        true
    }

    /// Is this window a window-based menu: a `WindowType::Menu` window with no
    /// transient mailbox? Every `<transient-window>` popup (a modal, a
    /// popover, a picker) is a Menu-type window too
    /// (`transient::popup_window_state`); it closes through its own policy and
    /// tells its node (`Dismissed`) - the owner closing it as a menu closed an
    /// open modal on any Escape, silently (E2E-C, AzReview). The same rule as
    /// `process_transient_dismissal`'s `is_window_menu`.
    fn is_menu_window(&self) -> bool {
        let state = self.common.current_window_state();
        state.flags.window_type == azul_core::window::WindowType::Menu
            && crate::desktop::shell2::common::transient::mailbox_of(state).is_none()
    }

    /// Whether this window can take over the closed root's process-wide work: an open window
    /// of the app's own, not a menu or a popup (a `Menu`-type window, which closes with the
    /// window it hangs off).
    fn can_be_heir(&self) -> bool {
        self.is_open()
            && self.common.current_window_state().flags.window_type
                != azul_core::window::WindowType::Menu
    }

    /// Whether this window's loop must poll (it has timers or threads in flight): the
    /// parent's wait polls while one of its children does.
    fn wants_polling(&self) -> bool {
        self.thread_poll_timer_running
            || self
                .common
                .layout_window
                .as_ref()
                .is_some_and(|lw| !lw.timers.is_empty() || !lw.threads.is_empty())
    }

    /// A child window's start, as `run()` starts the root: its create callback, then its
    /// first layout.
    fn start_as_child(&mut self) {
        self.invoke_create_callback();
        // Retires the request the window is born with, as `run()`'s initial
        // layout does (see there).
        let initial = self.common.regen_epoch();
        if let Err(e) = self.regenerate_layout() {
            log_warn!(
                LogCategory::Layout,
                "[Headless] WARNING: a child window's initial layout failed: {}",
                e
            );
        }
        self.common.clear_regeneration_unless_reraised(initial);
    }

    /// Poll the next event from the queue.
    pub fn poll_event(&mut self) -> Option<HeadlessEvent> {
        self.event_queue.pop_front()
    }

    /// Check if the window is still "open".
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Close the window.
    pub fn close(&mut self) {
        // The popups this window opened close with it: a popup's own popup is
        // one of the ROOT's children (`pump_children` hoists it), and nothing
        // else would ever tell it.
        PlatformWindow::close_transient_windows(self);
        // WebRender's Renderer must be deinit()'d, not dropped — texture
        // deletion has to happen inside a frame. Never doing so crashed debug
        // builds on close and leaked GPU resources in release.
        self.common.deinit_renderer();
        self.is_open = false;
    }

    /// Stop and join every live `Thread` BEFORE the process ends.
    ///
    /// `std::process::exit` runs no destructors at all, so without this the
    /// workers are simply abandoned — which is what ThreadSanitizer reports as
    /// `thread leak ... in pthread_create`.
    ///
    /// `StoppingWorkers::stop_all` (the app's, on `LayoutWindow::app_services`) tells EVERY worker
    /// `TerminateThread` first and waits for them together on one grace period (detaching a
    /// worker that never answers), then empties the map - clearing the map alone stopped them
    /// one after the other, each destructor waiting for its own worker. Doing this from the
    /// window rather than the loop keeps it correct for every exit path that ends the process
    /// instead of unwinding.
    fn shutdown_threads(&mut self) {
        let Some(lw) = self.get_layout_window_mut() else {
            return;
        };
        if lw.threads.is_empty() {
            return;
        }
        log_info!(
            LogCategory::EventLoop,
            "[Headless] terminating {} background thread(s) before exit",
            lw.threads.len(),
        );
        lw.app_services.stopping_workers.stop_all(&mut lw.threads);
        lw.thread_owners = azul_layout::managers::thread_owner::ThreadOwnerManager::default();
    }

    /// [`Self::shutdown_threads`] for this window and every child window it
    /// pumps: the threads a callback of a dialog started are joined too.
    fn shutdown_all_threads(&mut self) {
        for child in &mut self.children {
            child.shutdown_all_threads();
        }
        self.shutdown_threads();
        // The font registry's scout / builder threads (one registry for the
        // app, shared by every window) are told to stop too; the registry
        // hands out no handles to join, so this is a signal, not a join.
        if let Some(registry) = self.font_registry.as_ref() {
            registry.shutdown();
        }
    }

    // === Layout ===

    /// Regenerate layout and rebuild CPU hit-tester.
    ///
    /// This is the HeadlessWindow equivalent of `MacOSWindow::regenerate_layout()` /
    /// `WinWindow::regenerate_layout()` etc. It calls the shared
    /// `common::layout::regenerate_layout()` (which no longer requires WebRender
    /// types) and then rebuilds the `CpuHitTester` from the new layout results.
    pub fn regenerate_layout_inner(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // Consume the reason tag BEFORE borrowing the layout window: this is
        // the regeneration this window asked for, and the tag travels with
        // the request (see CommonWindowState::request_regeneration).
        let relayout_reason = self.common.take_relayout_reason();

        let borrows = self.common.layout_borrows();
        let layout_window = borrows.layout_window.ok_or("No layout window")?;

        // Collect debug messages if debug server is enabled
        let debug_enabled = debug_server::layout_trace_enabled();
        let mut debug_messages = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        // Call unified regenerate_layout from common module
        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            borrows.app_data,
            borrows.current_window_state,
            borrows.renderer_resources,
            borrows.gl_context_ptr,
            borrows.fc_cache,
            &self.font_registry,
            borrows.system_style,
            &self.icon_provider,
            &mut debug_messages,
            relayout_reason,
        )?;

        // Forward layout debug messages to the debug server's log queue
        if let Some(msgs) = debug_messages {
            for msg in msgs {
                debug_server::log(
                    debug_server::LogLevel::Debug,
                    LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        // Rebuild the SHARED hit-tester that the common event-dispatch path
        // reads (perform_hit_test → update_hit_test_at): pointer events (real
        // or synthetic via the debug server) resolve their target node through
        // `common.cpu_hit_tester`. Without this rebuild that tester stays
        // empty, so every click hit-tests to nothing and widget callbacks (e.g.
        // a button's on_click) never fire. It is the only one: the backend's
        // own `cpu_backend.hit_tester` is read by nothing here, and rebuilding
        // it too walked every laid-out node a second time per pass.
        self.common.rebuild_cpu_hit_tester_after(&result);

        // Drain any lifecycle events produced by reconciliation (Mount/Unmount/
        // Update/Resize) and dispatch them through the normal callback pipeline.
        // Doing this inside regenerate_layout keeps the headless test harness
        // self-contained: callers do not have to remember to pump lifecycle
        // events separately to see `.with_callback(EventFilter::Component(_))`
        // fire.

        // CPU-render the frame (retained compositor handles efficient resize)
        self.paint_cpu_frame();

        // Deliberately NO request_regeneration here. This ran at the end of
        // every rendered frame ("mark that frame needs regeneration"), which
        // turned the headless loop into a perpetual full-DOM-rebuild cycle:
        // each frame re-invoked the user's layout() on the next tick, so every
        // runtime CSS patch (a gallery panel toggled open, a combobox list
        // shown) was silently reverted one frame later, incremental paths were
        // never exercised in E2E, and "did this idle frame do any work?" was
        // unanswerable. A frame is a RESPONSE to a request, never a producer
        // of one — new frames come from real requests (events, timers,
        // request_repaint, request_regeneration by callbacks).

        Ok(result)
    }

    /// Service one owed frame according to the tier a pass reported plus the
    /// pending regeneration / relayout-only requests — the same contract the
    /// four desktop loops implement (X11 `render_and_present`, wayland
    /// `generate_frame_if_needed`, windows `WM_PAINT`, macOS
    /// `build_atomic_txn`): relayout-only is tested FIRST and both flags are
    /// consumed, and the full `regenerate_layout()` — which re-invokes the
    /// user's `layout()` and therefore DISCARDS runtime CSS patches — runs
    /// ONLY when a DOM rebuild was actually requested. Headless used to map
    /// every redraw signal to `regenerate_layout()`, so a `set_css_property`
    /// patch (the gallery panel toggling open) survived exactly one frame.
    fn service_frame(&mut self, tier: azul_core::events::ProcessEventResult) {
        use azul_core::events::ProcessEventResult as R;

        // Mirror the desktop event-arm routing: a regenerate-tier result marks
        // the DOM rebuild; an incremental-relayout result re-runs layout on the
        // existing StyledDom HERE, as X11's `handle_event` / macOS's input arm
        // do, and the frame then takes the relayout-only path, which PAINTS:
        // every relayout-only request means "the layout already ran"
        // (`process_timers_and_threads`, `adopt_system_style` and this arm
        // all lay out before they raise it). Headless used to raise the flag
        // here and lay out in the frame, so a request raised after a layout
        // laid the window out twice - and the second build, with nothing left
        // to patch, replaced the patched display list (e2e/dl-text-patch).
        if tier >= R::ShouldRegenerateDomCurrentWindow {
            self.common
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        } else if tier == R::ShouldIncrementalRelayout {
            let mut debug_messages = None;
            match self.incremental_relayout_dispatching(
                event::IncrementalRelayout::Restyle,
                &mut debug_messages,
            ) {
                Ok(()) => self.common.request_relayout_only(),
                Err(e) => {
                    log_warn!(
                        LogCategory::Layout,
                        "[Headless] incremental relayout failed: {} - falling back to a full \
                         regeneration",
                        e
                    );
                    self.common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                }
            }
        }

        let relayout_only = self.common.take_relayout_only();
        // The resize fast path, exactly as the desktop shells take it (macOS
        // `build_atomic_txn`, X11, Wayland, Windows `WM_PAINT`): a latched
        // resize with no DOM rebuild owed re-lays-out the EXISTING StyledDom
        // with `IncrementalRelayout::Resize` - the retained tree, its warm
        // per-node caches, a patched display list. A full regeneration
        // (boundary crossed) lays out at the new size itself. Folding the
        // latch into the RESTYLE relayout, as this did, laid out every
        // headless resize (debug server, AZ_E2E) on a path no desktop window
        // takes, so resize bugs of the shells never reproduced here.
        let resize_relayout = self.common.take_resize_relayout();
        let regen_requested = self.common.take_regeneration();
        let content_repaint = core::mem::take(&mut self.common.content_repaint_pending);
        // A display list marked dirty (a css-id image registered, a caret
        // blink): rebuilt from the layout as it stands by `repaint_only`, as
        // macOS's `build_atomic_txn` consumes the flag. Headless had no
        // consumer at all, so such a change re-laid-out the unchanged tree,
        // kept the cached list and painted nothing
        // (e2e/op-image-cache-id-repaints).
        let display_list_dirty = self.common.display_list_dirty;

        let (res, what) = if relayout_only {
            // The layout ran where the request was raised (see the top of
            // this fn) and built the display list - patched where it could:
            // paint it. Rebuilding the list for the dirty flag would replace
            // that patch with a full build (macOS clears the flag here too).
            self.common.display_list_dirty = false;
            (self.paint_laid_out(), "relayout")
        } else if regen_requested {
            let result = self.regenerate_layout();
            // An unchanged DOM reuses its layout and its display list - the
            // dirty list still owes its rebuild (macOS: "layout unchanged but
            // display_list_dirty").
            if display_list_dirty
                && matches!(
                    result,
                    Ok(crate::desktop::shell2::common::layout::LayoutRegenerateResult::LayoutUnchanged)
                )
            {
                (self.repaint_only(), "regeneration + display-list rebuild")
            } else {
                // A changed DOM built its list fresh: the flag is spent (as
                // macOS clears it after this branch).
                self.common.display_list_dirty = false;
                (result.map(|_| ()), "regeneration")
            }
        } else if resize_relayout {
            (
                self.relayout_existing_dom(event::IncrementalRelayout::Resize),
                "resize",
            )
        } else if (content_repaint || display_list_dirty)
            && !resize_relayout
            && tier <= R::ShouldUpdateDisplayListCurrentWindow
        {
            // A content change patched the display list in place (a video
            // frame on a visible tile), or marked it for a rebuild, and nothing
            // else asked for more: paint the frame from the layout as it
            // stands, the way the desktop frame paths do - no layout pass;
            // `repaint_only` rebuilds a dirty list first. The display-list
            // diff damages exactly the changed items.
            (self.repaint_only(), "content repaint")
        } else if self
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.pending_css_dirty.is_some())
        {
            // A restyle staged its CSS dirt for a layout pass (a GPU-only
            // restyle reports a plain repaint): re-lay-out the EXISTING
            // StyledDom and render - it never re-invokes the user's layout(),
            // so runtime patches survive.
            (self.relayout_only(), "repaint")
        } else {
            // Pure repaint (a pointer move that changed nothing, a GPU-value
            // frame of a transform tween, request_repaint): paint the layout
            // as it stands, as the desktop frame paths do (macOS: a light
            // transaction of scroll offsets and GPU values). A relayout here
            // laid the whole window out for every pointer event.
            (self.paint_laid_out(), "repaint")
        };
        if let Err(e) = res {
            log_error!(
                LogCategory::Layout,
                "[Headless] Frame service ({}) failed: {}",
                what,
                e
            );
        }
    }

    /// Re-run layout on the EXISTING (already mutated) `StyledDom` and re-render —
    /// the `ShouldIncrementalRelayout` path every other backend implements
    /// (macOS `apply_incremental_relayout_result`, windows/wayland
    /// `request_relayout_only`), and which headless was missing entirely.
    ///
    /// Headless used to answer *every* redraw signal with the full
    /// `regenerate_layout()`. For an in-place DOM mutation that is not just the
    /// slow path, it is the WRONG path: `regenerate_layout` short-circuits on
    /// `is_layout_equivalent(old, new)`, and after an in-place mutation "old" and
    /// "new" are the same DOM — so layout was skipped and the frame kept the
    /// pre-mutation shaped text and geometry forever (the stale screen).
    pub fn relayout_only(&mut self) -> Result<(), String> {
        self.relayout_existing_dom(event::IncrementalRelayout::Restyle)
    }

    /// [`Self::relayout_only`] as `kind` asks: `Restyle` for a restyle /
    /// runtime edit, `Resize` for the latched resize fast path (see
    /// `service_frame`). Both re-lay-out the EXISTING StyledDom and render.
    fn relayout_existing_dom(&mut self, kind: event::IncrementalRelayout) -> Result<(), String> {
        let debug_enabled = debug_server::layout_trace_enabled();
        let mut debug_messages = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        // The common method owns the finalize tail (the CPU hit-tester
        // rebuild) and the trait wrapper delivers the lifecycle events the
        // pass produced — see `PlatformWindow::incremental_relayout_dispatching`.
        self.incremental_relayout_dispatching(kind, &mut debug_messages)?;

        if let Some(msgs) = debug_messages {
            for msg in msgs {
                debug_server::log(
                    debug_server::LogLevel::Debug,
                    LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        // Same finalize tail as regenerate_layout: the CPU frame and its
        // damage (the shared hit tester was rebuilt inside
        // `incremental_relayout`).
        self.paint_cpu_frame();

        // Same as regenerate_layout_inner above: a completed frame must not
        // re-arm regeneration (see the comment there).
        Ok(())
    }

    /// Paint one frame from the layout as it stands - the repaint a content
    /// change owes (a video frame on a tile, whose display-list item was
    /// patched in place), exactly what a desktop frame path does for it: no
    /// layout pass. A display list marked dirty in the same pass (a caret
    /// blink) is regenerated from the existing layout first.
    /// Paint the layout a relayout-only request says already ran (its raiser
    /// laid the existing StyledDom out and rebuilt the shared hit tester):
    /// the CPU frame - the finalize tail of [`Self::relayout_existing_dom`]
    /// without its layout pass.
    fn paint_laid_out(&mut self) -> Result<(), String> {
        self.paint_cpu_frame();
        Ok(())
    }

    fn repaint_only(&mut self) -> Result<(), String> {
        if core::mem::take(&mut self.common.display_list_dirty) {
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.regenerate_display_list_for_dom(azul_core::dom::DomId { inner: 0 });
            }
        }
        self.paint_cpu_frame();
        Ok(())
    }

    /// CPU-render the frame and publish its damage: the tail every headless
    /// frame path shares (full regeneration, relayout, repaint).
    fn paint_cpu_frame(&mut self) {
        #[cfg(feature = "cpurender")]
        {
            let ws = self.common.current_window_state();
            let width = ws.size.dimensions.width;
            let height = ws.size.dimensions.height;
            let dpi = ws.size.dpi as f32 / 96.0;
            // MWA-C-gpu_state/MWA-D: deliberately NO per-frame scrollbar
            // fade refresh here (unlike the interactive backends) — the
            // audit rated the headless relayout-only cache update adequate
            // for snapshot rendering, and the wall-clock fade advancing
            // BETWEEN two renders broke the fast-scroll-vs-full-render
            // pixel-identity golden tests (the two frames must share cache
            // state to be comparable). Content preparation (image callbacks
            // through the chokepoint + journal clock) still runs: a canvas
            // is content, not animation — without it every callback image
            // rendered as the announced grey placeholder on headless.
            //
            // The thumb TRANSFORMS are refreshed all the same: they read no
            // clock (pure function of layout + the current scroll offsets),
            // so they cannot desynchronise two renders, and without them a
            // scroll that changes nothing else leaves every thumb parked at
            // the position of the last full layout — mis-painted, and
            // undamaged, because the display-list items compare equal and
            // only the GPU value diff can raise the bar's rect.
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.prepare_frame_content();
                lw.refresh_scrollbar_transforms();
            }
            if let Some(lw) = self.common.layout_window.as_ref() {
                self.cpu_backend.sync_window_flags(&lw.current_window_state);
                self.cpu_backend.render_frame(
                    lw,
                    &self.common.renderer_resources,
                    width,
                    height,
                    dpi,
                );
            }
            // Publish the damage of the frame we just rendered onto the
            // LayoutWindow, where `CallbackInfo::get_layout_window()` — and
            // therefore an E2E assertion — can actually see it. Without this the
            // damage machinery is invisible from outside the engine.
            let paint = self.cpu_backend.last_frame_damage.clone();
            let present = self.cpu_backend.last_present_damage.clone();
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.record_frame(paint, present);
            }
            // And the damage-driven framebuffer itself, for the E2E
            // `assert_damage_sound` `pixel_identity` check - as the in-process
            // runner publishes it, so a scenario green there is answerable here.
            #[cfg(any(feature = "debug-server", feature = "e2e-scripting"))]
            if self.publish_presented_frame {
                if let (Some(lw), Some(frame)) = (
                    self.common.layout_window.as_ref(),
                    self.cpu_backend.last_frame.as_ref(),
                ) {
                    debug_server::e2e_set_presented_frame(lw, frame);
                }
                // This backend has no renderer either: nothing is registered in
                // `RendererResources`, so the renderer-side resource counters of
                // `assert_resource_counts` (`fonts`, `images`, ...) read what the
                // display lists draw, as in the in-process runner. They were
                // `0 == 0` here too.
                if let Some(lw) = self.common.layout_window.as_ref() {
                    debug_server::e2e_set_virtual_renderer(lw);
                }
            }
        }
    }

    // === Event injection (for tests / debug server) ===

    /// Inject an event into the queue for the next poll cycle.
    ///
    /// Wakes the blocking event loop if it is sleeping on the condvar.
    pub fn inject_event(&mut self, event: HeadlessEvent) {
        self.event_queue.push_back(event);
        self.wake();
    }

    /// Inject multiple events at once.
    pub fn inject_events(&mut self, events: impl IntoIterator<Item = HeadlessEvent>) {
        self.event_queue.extend(events);
        self.wake();
    }

    /// Queue an accessibility action as if a screen reader had requested it.
    ///
    /// This is headless's substitute for AT-SPI `do_action` / UIA `Invoke` /
    /// `NSAccessibility` press: the four desktop backends receive an
    /// `accesskit::ActionRequest` on a bus and decode it to exactly this
    /// triple, then hand it to `process_accessibility_actions()`. There is no
    /// bus here, so the triple is the ingress.
    ///
    /// It exists so accessibility behaviour is *observable* — before it, no
    /// backend could be driven through `LayoutWindow::process_accessibility_action`
    /// from a test, which is why "screen-reader activation invokes no callback"
    /// could ship and stay shipped.
    ///
    /// The action is applied on the next loop iteration (or on the next
    /// explicit `process_accessibility_actions()` call), NOT synchronously —
    /// same as every real adapter, whose actions arrive off-loop and are
    /// drained by the frame pump.
    pub fn inject_accessibility_action(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
        action: azul_core::dom::AccessibilityAction,
    ) {
        self.accessibility_adapter.push(dom_id, node_id, action);
        self.wake();
    }

    /// Tell this window the system light/dark preference changed.
    ///
    /// headless has no compositor to hear it from, so — exactly like
    /// [`HeadlessWindow::inject_accessibility_action`] — the injection IS the
    /// ingress. Without it no test could drive a theme switch on any backend,
    /// and that is why "a runtime dark-mode toggle does nothing" survived on six
    /// of the seven backends: the one backend CI can run had no way to express
    /// the event.
    ///
    /// The real backends' shape is `windows/mod.rs`'s `WM_SETTINGCHANGE |
    /// WM_THEMECHANGED` arm: re-read the system style, update the window state,
    /// pump the events that fall out, then request a regeneration tagged
    /// [`RelayoutReason::ModeChange`]. This does the same, minus the
    /// re-discovery — the caller supplies the theme, since there is no system
    /// setting here to read.
    ///
    /// Returns `false` if the WINDOW's theme did not move, in which case
    /// nothing is dispatched and no frame is requested: the desktop was
    /// already in `theme` (a no-op switch should not cost a relayout, and a
    /// test asserting "N relayouts" should not have to know whether the theme
    /// happened to differ), or the app pins its mode
    /// (`CallbackInfo::set_mode`) - then the desktop's new light / dark is
    /// only remembered, for when the app follows the desktop again.
    pub fn set_system_theme(&mut self, theme: azul_core::window::DarkLightMode) -> bool {
        let Some(theme) = self.common.adopt_desktop_theme(theme) else {
            return false;
        };

        // previous_window_state is what the diff pipeline compares against to
        // decide that a ThemeChanged event fired; without this snapshot the
        // event is never determined and the callbacks never run.
        self.snapshot_window_state_baseline("headless.set_system_theme");
        self.common.write_shown_mode(theme);

        // Same shape as the HeadlessEvent arms in `run()`: pump the events the
        // state change implies and let the result speak; there is no window
        // handle here to route a result to.
        let _ = self.process_window_events(0);

        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::ModeChange);
        self.wake();
        true
    }

    /// Drain queued accessibility actions, apply them, and dispatch the
    /// callbacks they map to.
    ///
    /// Mirrors `Win32Window::process_accessibility_actions` /
    /// `X11Window::process_accessibility_actions` exactly: poll the action
    /// source, route each through `LayoutWindow::process_accessibility_action`,
    /// mark the display list dirty for a non-empty affected set, dispatch the
    /// mapped callbacks and honour the `Update` they return. The shared body
    /// lives in `PlatformWindow::dispatch_accessibility_actions` so all seven
    /// backends cannot drift apart.
    #[cfg(feature = "a11y")]
    pub fn process_accessibility_actions(&mut self) {
        let mut actions = Vec::new();
        while let Some(action) = self.accessibility_adapter.poll_action() {
            actions.push(action);
        }
        if actions.is_empty() {
            return;
        }
        self.dispatch_accessibility_actions(actions);
        // UNCONDITIONAL, exactly like the four desktop backends. Half the
        // actions (Focus, Blur, ScrollUp/Down/Left/Right, SetScrollOffset,
        // ScrollIntoView, SetTextSelection) change manager state and return an
        // EMPTY affected-node map, because they map to no callback. Gating the
        // redraw on that map would mean a screen reader could move focus or
        // scroll the view and the window would never repaint it.
        self.request_redraw();
    }

    /// Simulate a window resize. Updates `current_window_state.size` to the
    /// new logical dimensions and tags the next `regenerate_layout()` call
    /// with `RelayoutReason::Resize` so the user's `LayoutCallback` sees
    /// the size change via `info.relayout_reason()` plus the live
    /// `info.window_size`. The next `regenerate_layout()` call will
    /// re-invoke `layout()` exactly the way the real platform handlers do.
    pub fn simulate_resize(&mut self, width: f32, height: f32) {
        use azul_core::geom::LogicalSize;
        // Contract shape (snapshot → mutate → pass): the size diff dispatches
        // `WindowResize` exactly as a native configure does, and no live delta
        // is left behind for the validation check to flag.
        self.snapshot_window_state_baseline("headless.simulate_resize");
        self.common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize { width, height };
            });
        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        let _ = self.process_window_events(0);
    }

    /// Simulate the window system answering the decoration request with a
    /// different mode than the one the window asked for.
    ///
    /// This is the headless analogue of
    /// `zxdg_toplevel_decoration_v1.configure` reporting `client_side` to a
    /// window that requested `server_side` (KWin does exactly that): the shell
    /// flips the window to frameless + CSD and asks for a regeneration, in
    /// which the titlebar must appear. Same shape as
    /// `wayland::events::toplevel_decoration_configure_handler` — an
    /// OS-sourced flag write (the change is already true of the window, so the
    /// OS-sync baseline advances with it and is never echoed back) followed by
    /// a plain `RefreshDom` regeneration request.
    pub fn simulate_decoration_change(
        &mut self,
        decorations: azul_core::window::WindowDecorations,
        has_decorations: bool,
    ) {
        self.common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.flags.decorations = decorations;
                ws.flags.has_decorations = has_decorations;
            });
        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        self.wake();
    }

    /// Read the queued reason for the next `regenerate_layout()` call.
    /// Useful for asserting in tests that an event handler tagged the
    /// upcoming relayout correctly.
    pub fn pending_relayout_reason(&self) -> azul_core::callbacks::RelayoutReason {
        self.common.regeneration_reason()
    }

    /// A `TextInput` event: the SAME canonical text pipeline the debug server
    /// and the platform IME paths use - record the input against the
    /// focused/editable node, dispatch the synthetic Input events, apply the
    /// changeset. Shared by `run()` and the tests' `step()`, so a scripted
    /// keystroke in a test takes the event loop's path.
    fn apply_text_input_event(&mut self, text: &str) -> azul_core::events::ProcessEventResult {
        self.apply_user_change(&azul_layout::callbacks::CallbackChange::CreateTextInput {
            text: text.to_string().into(),
        })
    }

    /// A wheel `Scroll` event, as `run()` and the tests' `step()` both take it:
    /// record the delta, arm the momentum timer, then run the event pass.
    fn apply_wheel_scroll_event(
        &mut self,
        delta_x: f32,
        delta_y: f32,
    ) -> azul_core::events::ProcessEventResult {
        // Drive the SAME physics-timer scroll path the desktop
        // backends use: record_scroll_from_hit_test queues the
        // delta against the scroll node under the pointer and
        // the SCROLL_MOMENTUM_TIMER applies it over time.
        // delta_x/delta_y are RAW input deltas, same as a platform
        // wheel/axis event — the direction sign (natural-scroll
        // flag) is applied centrally in ScrollManager, not here. A
        // prior MouseMove must have left the hover hit-test over a
        // scrollable node — otherwise this is a no-op (just like
        // wheeling over a non-scrollable area on the desktop).
        let queue = if let Some(lw) = self.common.layout_window.as_mut() {
            let now = azul_core::task::Instant::from(Instant::now());
            match lw.scroll_manager.record_scroll_from_hit_test(
                delta_x,
                delta_y,
                azul_layout::managers::scroll_state::ScrollInputSource::WheelDiscrete,
                // e2e harness scrolls must stay deterministic
                // (velocity model, no wall-clock glide).
                azul_layout::managers::scroll_state::ScrollInputDevice::TestDriver,
                &lw.hover_manager,
                &azul_layout::managers::hover::InputPointId::Mouse,
                now,
            ) {
                Some((_, _, true)) => Some(lw.scroll_manager.get_input_queue()),
                _ => None,
            }
        } else {
            None
        };

        // Start the momentum timer only on the first pending
        // input (subsequent deltas are picked up by the running
        // timer via the shared ScrollInputQueue).
        if let Some(queue) = queue {
            let timer = azul_layout::scroll_timer::create_scroll_physics_timer(
                queue,
                self.common.system_style.scroll_physics.clone(),
                self.common.frame_interval_nanos(),
            );
            self.start_timer(azul_core::task::SCROLL_MOMENTUM_TIMER_ID.id, timer);
        }

        // HARNESS PARITY: every native backend runs the event
        // pass right after recording a wheel step (macOS
        // `handle_scroll_wheel` → `process_window_events(0)`,
        // the shared wheel arm). This path recorded and never
        // ran it, so the `Scroll` event never fired here and
        // a defect that only a wheel PASS triggers - the tail
        // revealing the caret on a pass that typed nothing -
        // could not be reproduced headlessly.
        self.snapshot_window_state_baseline("headless.run.scroll");
        self.process_window_events(0)
    }

    /// Convert a `KeyDown` virtual keycode into the locale-independent character
    /// fallback (delegating to [`VirtualKeyCode::get_lowercase`]) and, if a
    /// character is available, queue a synthetic `TextInput` event for the next
    /// poll cycle.
    ///
    /// This mirrors what platform IME paths do when no locale-specific composer
    /// is active: latin keys still produce a typed character without going
    /// through a full input-method round-trip.
    pub fn synthesize_character_input(&mut self, vk: VirtualKeyCode) -> Option<char> {
        let c = vk.get_lowercase()?;
        self.inject_event(HeadlessEvent::TextInput {
            text: c.to_string(),
        });
        Some(c)
    }

    /// Replace the active touch point list. Updates `num_touches` to match.
    pub fn inject_touch_points(&mut self, points: impl IntoIterator<Item = TouchPoint>) {
        // Contract shape (snapshot → mutate → pass): the touch_state diff is
        // what dispatches TouchStart/Move/End, same as the native shells.
        self.snapshot_window_state_baseline("headless.inject_touch_points");
        let vec: TouchPointVec = points.into_iter().collect::<Vec<_>>().into();
        let touch_state = self.common.touch_state_mut();
        touch_state.num_touches = vec.len();
        touch_state.touch_points = vec;
        let _ = self.process_window_events(0);
        self.wake();
    }

    /// Set the desired fullscreen-transition style on the current window state
    /// flags. The next request to enter or leave fullscreen will honor this
    /// value (slow vs. fast on macOS).
    ///
    /// On platforms that do not distinguish slow/fast transitions this is a
    /// no-op for animation purposes but still recorded on the window state for
    /// observation.
    pub fn set_fullscreen_mode(&mut self, mode: FullScreenMode) {
        self.common
            .update_window_state(event::WindowStateSource::App, |ws| {
                let flags = &mut ws.flags;
                flags.fullscreen_mode = mode;
                // Fold the request into the current frame state so headless callers
                // can observe the transition without a real OS event loop.
                flags.frame = match mode {
                    FullScreenMode::SlowFullScreen | FullScreenMode::FastFullScreen => {
                        WindowFrame::Fullscreen
                    }
                    FullScreenMode::SlowWindowed | FullScreenMode::FastWindowed => {
                        WindowFrame::Normal
                    }
                };
            });
    }

    /// Returns `true` if every entry of `chord` is currently active in the
    /// window's keyboard state. Use to evaluate registered accelerator
    /// shortcuts (e.g. `[Ctrl, Key(VirtualKeyCode::S)]`) on each key event.
    pub fn matches_accelerator(&self, chord: &[AcceleratorKey]) -> bool {
        self.common
            .current_window_state()
            .keyboard_state
            .matches_accelerator(chord)
    }

    /// Drive a synthetic scroll delta through [`process_system_scroll`] and
    /// return the [`ScrollResult`] for assertion in tests.
    pub fn process_system_scroll(
        &mut self,
        delta: LogicalPosition,
        hit_scrollbar: bool,
    ) -> ScrollResult {
        azul_core::window::process_system_scroll(delta, hit_scrollbar)
    }

    /// Ask for another frame.
    ///
    /// The headless analogue of `XSendEvent(Expose)` / `InvalidateRect` /
    /// `setNeedsDisplay`: there is no compositor that could handle a
    /// repaint-only update in the CPU path (see the note in `run()`), so a
    /// redraw request is a full regeneration request plus a loop wake-up.
    pub fn request_redraw(&mut self) {
        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        self.wake();
    }

    /// Signal the condvar so the blocking loop wakes up.
    fn wake(&self) {
        if let Ok(mut guard) = self.wake_mutex.lock() {
            guard.woken = true;
            self.wake_condvar.notify_one();
        }
    }

    /// Check if any timers are currently active.
    pub fn has_active_timers(&self) -> bool {
        self.common
            .layout_window
            .as_ref()
            .map_or(false, |lw| !lw.timers.is_empty())
    }

    /// Get the number of pending window creation requests.
    pub fn pending_window_count(&self) -> usize {
        self.pending_window_creates.len()
    }

    /// Fire `WindowCreateOptions::create_callback` exactly once, before the
    /// first layout.
    ///
    /// X11/Wayland/Windows/macOS all invoke it after the window exists and
    /// BEFORE the first layout; headless is documented as behavior-parity
    /// ("callbacks fire, timers tick"), so it must too — an app that installs
    /// its startup timer here otherwise idles until killed under
    /// `AZ_BACKEND=headless`. Public so the e2e harness (which drives frames
    /// manually instead of calling the blocking `run()`) can fire it at the
    /// same point in the lifecycle.
    pub fn invoke_create_callback(&mut self) {
        // take, not clone: fires exactly once per window lifetime.
        let taken = core::mem::replace(
            &mut self.create_callback,
            azul_layout::callbacks::OptionCallback::None,
        );
        let Some(mut callback) = taken.into_option() else {
            return;
        };

        let app_data = self.common.app_data.clone();
        let borrows = self.prepare_callback_invocation();
        let mut app_data_ref = app_data.borrow_mut();

        let (changes, _update) = borrows.layout_window.invoke_single_callback(
            &mut callback,
            &mut *app_data_ref,
            &borrows.window_handle,
            borrows.gl_context_ptr,
            borrows.system_style.clone(),
            &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
            borrows.previous_window_state,
            borrows.current_window_state,
            borrows.renderer_resources,
        );

        drop(app_data_ref);
        use crate::desktop::shell2::common::event::PlatformWindow;
        for change in &changes {
            let r = self.apply_user_change(change);
            if r != azul_core::events::ProcessEventResult::DoNothing {
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            }
        }
    }

    // === Blocking event loop ===

    /// Run the headless event loop — **blocks** until the window closes.
    ///
    /// This is the HeadlessWindow equivalent of `NSApplication.run()` / the
    /// Win32 `GetMessage` loop / the X11 `XNextEvent` loop.
    ///
    /// The loop uses a `Condvar` for zero-CPU blocking:
    /// * When timers are active it uses `wait_timeout` (16 ms / 60 Hz) so timers get ticked even
    ///   without external events.
    /// * When no timers are active it calls `wait` (indefinite) — the thread is parked until
    ///   `inject_event()`, `start_timer()`, or another caller invokes `wake()`.
    /// * If nothing can ever wake the loop (no timers, no threads, no debug server) a one-time
    ///   warning is printed to stderr and the loop blocks forever — identical to a desktop window
    ///   nobody interacts with.
    pub fn run(mut self) -> Result<(), WindowError> {
        let debug_enabled = debug_server::is_debug_enabled();
        let start = Instant::now();

        log_info!(
            LogCategory::EventLoop,
            "[Headless] Entering condvar-based blocking event loop (debug={})",
            debug_enabled,
        );

        // -- Invoke create_callback (behavior parity with the OS shells) --
        self.invoke_create_callback();

        // -- Perform initial layout (same as every platform) --
        // It IS the frame the window was born owing
        // (`RegenerationState::pending_initial`), with the create callback's
        // asks already applied: it retires those requests, as every desktop
        // backend's first frame does. Left raised, the first turn's frame
        // gate (`pump_once`, Phase 2c) rebuilt the DOM again for nothing. A
        // request the layout's own lifecycle callbacks raise stays.
        log_debug!(LogCategory::Layout, "[Headless] Performing initial layout");
        let initial = self.common.regen_epoch();
        if let Err(e) = self.regenerate_layout() {
            log_warn!(
                LogCategory::Layout,
                "[Headless] WARNING: Initial layout failed: {}",
                e
            );
        }
        self.common.clear_regeneration_unless_reraised(initial);

        // -- Optional one-shot PNG snapshot --
        // `AZ_HEADLESS_SNAPSHOT_PATH=/tmp/out.png` writes the very
        // first rendered frame as PNG, then closes the window so the
        // process exits with code 0. Enables CI golden-image testing
        // without a full E2E harness: build the app, run with the env
        // var set, diff against a checked-in reference.
        #[cfg(feature = "cpurender")]
        if let Ok(path) = std::env::var("AZ_HEADLESS_SNAPSHOT_PATH") {
            if let Some(ref pixmap) = self.cpu_backend.last_frame {
                match pixmap.encode_png() {
                    Ok(bytes) => match std::fs::write(&path, &bytes) {
                        Ok(()) => log_info!(
                            LogCategory::Rendering,
                            "[Headless] AZ_HEADLESS_SNAPSHOT_PATH: wrote {} bytes to {}",
                            bytes.len(),
                            path,
                        ),
                        Err(e) => {
                            log_error!(LogCategory::Rendering, "[Headless] write({}): {}", path, e)
                        }
                    },
                    Err(e) => log_error!(LogCategory::Rendering, "[Headless] encode_png: {}", e),
                }
            } else {
                log_warn!(
                    LogCategory::Rendering,
                    "[Headless] AZ_HEADLESS_SNAPSHOT_PATH set but no last_frame after initial \
                     layout — ensure the app's layout callback returns a non-empty DOM",
                );
            }
            // Exit cleanly so CI/test scripts get a deterministic
            // process termination after the snapshot is written.
            self.close();
        }

        // -- Exit once a frame has actually rendered --
        // `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1` closes the window as soon as
        // one frame exists, so the process exits 0 having PROVEN it rendered.
        //
        // This comment used to claim "windows, macos, x11 and wayland have
        // honoured this for a while". Only WINDOWS did. macos, x11 and wayland
        // never had it, so there was no non-interactive way to ask a real
        // compositor whether it rendered — the process just kept running and a
        // harness could only time out, which reads the same as a hang. wayland
        // has it now (see linux/wayland/mod.rs, after the surface_committed
        // guard); x11 and macos still do not — #56.
        //
        // headless did not honour it either, and that silently defeated the ASan gate. The CI
        // step sets AZ_BACKEND=headless plus this variable and then runs
        // `timeout 30 ./hello-world-asan || [ $? -eq 124 ]` — so the process
        // ALWAYS ran to the 30s wall, was killed with rc 124, and the `|| [ $? -eq
        // 124 ]` converted that into success. An app that rendered nothing, hung
        // on first layout, or deadlocked was indistinguishable from the healthy
        // path: all rc 124, all green. The gate still caught an ASan abort (rc
        // 134), but it had never once verified a frame rendered, which is
        // precisely what its own env var claims to check.
        //
        // Deliberately AFTER the snapshot block so both can be set together: the
        // snapshot writes the frame, this exits on it.
        if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            #[cfg(feature = "cpurender")]
            let rendered = self.cpu_backend.last_frame.is_some();
            #[cfg(not(feature = "cpurender"))]
            let rendered = false;

            if rendered {
                log_info!(
                    LogCategory::Rendering,
                    "[Headless] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER: a frame rendered, exiting 0",
                );
                self.close();
            } else {
                // Do NOT close. Exiting 0 here would re-create the false green
                // this exists to remove — the caller must be able to tell "no
                // frame" apart from "frame rendered". Staying up means the
                // caller's timeout fires, which is now a real failure signal.
                log_warn!(
                    LogCategory::Rendering,
                    "[Headless] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set but NO frame has rendered \
                     — not exiting, so the caller's timeout reports this as the failure it is",
                );
            }
        }

        let mut warned_no_wake_sources = false;

        // A global-hotkey press simulated from another thread (a test, the
        // e2e runner) signals this loop's condvar instead of waiting for the
        // 60 Hz poll: with the waker attached, `needs_loop_polling` is false.
        if let Some(hotkeys) = self
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.global_hotkeys.shared().clone())
        {
            let condvar = self.wake_condvar.clone();
            let mutex = self.wake_mutex.clone();
            hotkeys.attach_loop_waker(
                Arc::new(move || {
                    if let Ok(mut guard) = mutex.lock() {
                        guard.woken = true;
                        condvar.notify_one();
                    }
                }),
                false,
            );
        }
        // A request the debug server queues wakes this loop too: Phase 2
        // (`process_timers_and_threads`) re-arms the debug poll at the busy
        // rate, so the poll's idle period is a safety net, not a latency.
        // So does an exit request (`common::process_exit`, the AZ_E2E verdict
        // printer), which a script-only build (`e2e-scripting`) makes too.
        #[cfg(any(feature = "debug-server", feature = "e2e-scripting"))]
        {
            let condvar = self.wake_condvar.clone();
            let mutex = self.wake_mutex.clone();
            azul_layout::e2e::add_debug_request_waker(Arc::new(move || {
                if let Ok(mut guard) = mutex.lock() {
                    guard.woken = true;
                    condvar.notify_one();
                }
            }));
        }

        // This loop ends the process when a worker asks (the AZ_E2E verdict
        // printer): on THIS thread, with the windows' threads joined - see
        // `common::process_exit`.
        let exit_request = &crate::desktop::shell2::common::process_exit::EXIT_REQUEST;
        exit_request.loop_takes_requests();

        // The app runs until its LAST window closes, as on every desktop: a window the
        // root opened that it does not own (AzMail's compose window) outlives it.
        let mut root_handed_over = false;
        loop {
            if exit_request.requested().is_some() {
                break;
            }
            if self.is_open() {
                self.pump_once(true);
            }

            // ── Phase 3 + 4: spawn and pump the child windows ─────
            self.pump_children();

            // The root closed: its menus and owned windows close with it, its work goes
            // on in a window that stays. With no window left the loop ends here: the
            // wait below may have no wake source left, and the process would stay up
            // with no window.
            if !self.is_open() {
                if !root_handed_over {
                    root_handed_over = true;
                    if !self.hand_over_from_closed_root() {
                        break;
                    }
                    // The owned windows asked to close run their close protocol now.
                    continue;
                }
                if self.children.is_empty() && self.pending_window_creates.is_empty() {
                    break;
                }
            }
            let root_open = self.is_open();

            // ── Phase 5: Condvar-based wait ──────────────────────
            let has_timers = root_open
                && self
                    .common
                    .layout_window
                    .as_ref()
                    .map_or(false, |lw| !lw.timers.is_empty());
            let root_threads = root_open && self.thread_poll_timer_running;
            // A registered global hotkey on the simulation: a press parked from
            // outside a callback (a test thread) has no way to signal the
            // condvar, so the wait below polls while one is registered.
            let has_hotkeys = crate::desktop::global_hotkey::needs_loop_polling();
            let has_wake_sources = has_timers
                || root_threads
                || has_hotkeys
                || debug_enabled
                || !self.children.is_empty();

            if !has_wake_sources && !warned_no_wake_sources {
                warned_no_wake_sources = true;
                eprintln!(
                    "[azul] HeadlessWindow: no timers, threads, or debug server active. The event \
                     loop will block indefinitely on a condvar (same as a desktop window nobody \
                     interacts with). Set AZ_DEBUG=1 to enable the debug server, or inject events \
                     via inject_event()."
                );
            }

            // How often the loop re-checks while timers or threads are
            // active: once per frame of this window (its monitor's refresh
            // rate, 60 Hz when there is no monitor - the headless default).
            let poll_interval = self.common.frame_interval();

            // Lock, then wait — but only if no wake is already pending.
            let mut guard = self.wake_mutex.lock().unwrap();

            // Threads count as a wake source for the TIMED wait, not just for
            // the no-wake-sources warning above: a background `Thread`'s
            // completion is only ever OBSERVED by polling (`run_all_threads`
            // inside `process_timers_and_threads`) — the worker has no handle
            // to this condvar, so it cannot signal it. With threads in flight
            // and no timers armed, the old `has_timers`-only split blocked
            // indefinitely: the fetch worker finished, its writeback never
            // ran, and the window froze — the headless twin of the X11/Wayland
            // "threads have no fd in the poll set" 16 ms tick, which is
            // exactly how those backends solve the same problem.
            if guard.woken {
                // A wake arrived DURING this iteration's processing — e.g. a
                // timer callback injected an event after Phase 1 had already
                // drained the queue, or requested a redraw after Phase 2. The
                // old sequence cleared `woken` unconditionally and then
                // waited, erasing that wake: the queued work sat there until
                // the next unrelated wake (or forever, with no timer armed).
                // Consume the flag and loop again WITHOUT waiting, so the
                // work the wake announced is serviced now.
                guard.woken = false;
            } else if has_timers
                || root_threads
                || has_hotkeys
                || self.children.iter().any(HeadlessWindow::wants_polling)
            {
                // Threads and simulated hotkeys are only seen by polling: once
                // per frame. Timers alone: until the next one is due, never
                // sooner than a frame - an idle debug poll (2 s) no longer
                // wakes this loop 60 times a second, and anything that has
                // work (a request, a timer change, an injected event)
                // notifies the condvar. A child window with timers or threads
                // (pumped by this loop) keeps the frame poll too.
                let children_poll = self.children.iter().any(HeadlessWindow::wants_polling);
                let wait = if root_threads || has_hotkeys || children_poll {
                    poll_interval
                } else {
                    let get_time = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal()
                        .get_system_time_fn;
                    self.common
                        .layout_window
                        .as_ref()
                        .and_then(|lw| lw.time_until_next_timer_ms(&get_time))
                        .map_or(poll_interval, |ms| {
                            poll_interval.max(std::time::Duration::from_millis(ms))
                        })
                };
                let _r = self
                    .wake_condvar
                    .wait_timeout_while(guard, wait, |ws| !ws.woken);
            } else {
                // No timers → block indefinitely until woken
                let _r = self.wake_condvar.wait_while(guard, |ws| !ws.woken);
            }
        }

        log_info!(
            LogCategory::EventLoop,
            "[Headless] Event loop finished (elapsed: {:.1}s)",
            start.elapsed().as_secs_f64()
        );

        // A worker asked to end the process with a code (the AZ_E2E verdict):
        // whatever the termination behaviour, the process ends here, on the
        // UI thread, after every window's threads are joined.
        if let Some(code) = exit_request.requested() {
            self.shutdown_all_threads();
            crate::desktop::shell2::run::exit_from_ui_thread(code);
        }

        // Handle termination behaviour (same as every platform run())
        match self.config.termination_behavior {
            AppTerminationBehavior::EndProcess => {
                // `process::exit` does NOT run destructors, so every live
                // `Thread` would skip its `Drop` — the one that sends
                // TerminateThread, waits out the grace period and JOINS the
                // worker. The threads simply vanish with the process, and
                // ThreadSanitizer reports exactly that:
                //
                //   SUMMARY: ThreadSanitizer: thread leak in pthread_create
                //
                // The async example makes it visible because it spawns one
                // worker per visible map tile and the harness exits after a
                // single frame, with all of them still in flight. Dropping the
                // registry here runs those destructors while the process is
                // still alive.
                self.shutdown_all_threads();
                // Through the run module's exit: the debug server's thread
                // stops first and an instrumented build's profile is written.
                crate::desktop::shell2::run::exit_from_ui_thread(0);
            }
            AppTerminationBehavior::ReturnToMain => { /* return normally */ }
            AppTerminationBehavior::RunForever => { /* all windows closed */ }
        }

        Ok(())
    }
}

// === PlatformWindow Trait Implementation ===

impl PlatformWindow for HeadlessWindow {
    /// Headless has no window manager to hand a drag to.
    fn handle_begin_interactive_move(&mut self) {}

    /// Every OTHER window of this app (the root and its children share the generation) rebuilds
    /// at its next turn ([`HeadlessWindow::pump_once`]); this one is rebuilt by the result that
    /// asked.
    fn request_regeneration_all_windows(&mut self) {
        self.regenerate_all_seen = self
            .regenerate_all
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
    }

    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // The single pass. The bounded lifecycle loop lives in the trait
        // default `regenerate_layout`, which is what frame paths call.
        self.regenerate_layout_inner()
    }

    /// The recording backend (`common::webview::HeadlessWebViews`): no
    /// browser - a test or a scenario plays the page's part.
    fn webview_backend(
        &mut self,
    ) -> Option<&mut dyn crate::desktop::shell2::common::webview::WebViewBackend> {
        Some(&mut self.webviews)
    }

    // 28 getter/setter methods generated by macro — identical to all other platforms
    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Unsupported
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let borrows = self.common.layout_borrows();

        event::InvokeSingleCallbackBorrows {
            layout_window: borrows
                .layout_window
                .expect("Layout window must exist for callback invocation"),
            window_handle: RawWindowHandle::Unsupported,
            gl_context_ptr: borrows.gl_context_ptr,
            fc_cache_clone: (**borrows.fc_cache).clone(),
            system_style: borrows.system_style.clone(),
            previous_window_state: borrows.previous_window_state,
            current_window_state: borrows.current_window_state,
            renderer_resources: borrows.renderer_resources,
        }
    }

    // Timer Management — condvar wakes the loop when timers change

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
        self.wake(); // transition condvar from indefinite to timed wait
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    fn start_thread_poll_timer(&mut self) {
        self.thread_poll_timer_running = true;
    }

    fn stop_thread_poll_timer(&mut self) {
        self.thread_poll_timer_running = false;
    }

    fn add_threads(
        &mut self,
        threads: BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
        if !self.thread_poll_timer_running {
            self.start_thread_poll_timer();
        }
    }

    fn remove_threads(&mut self, thread_ids: &BTreeSet<azul_core::task::ThreadId>) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for id in thread_ids {
                drop(lw.remove_thread(id));
            }
            if lw.threads.is_empty() {
                self.stop_thread_poll_timer();
            }
        }
    }

    fn queue_window_create(&mut self, options: WindowCreateOptions) {
        self.pending_window_creates.push(options);
    }

    /// A menu is what the X11 / Wayland fallback makes it: a window of its
    /// own with the menu DOM (`desktop::menu::show_menu`), spawned and pumped
    /// as a child of this window ([`HeadlessWindow::pump_children`]), so the
    /// debug server reaches it by `window_id` (`azul-menu`; see
    /// `list_windows`) and a script can read and click its items. A headless
    /// window sits at the origin of no screen: the parent position is (0, 0).
    /// The item picked in it runs HERE (`menu_picks`, drained by
    /// `pump_children`), as on every backend.
    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
        anchor: Option<azul_core::geom::LogicalRect>,
    ) {
        let options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
            Some(azul_layout::placement::WindowToScreen::new(
                azul_layout::placement::ScreenPoint::new(0.0, 0.0),
                1.0,
            )),
            anchor,
            Some(position),
            None,
            self.common.menu_picks.clone(),
        );
        self.pending_window_creates.push(options);
        self.wake();
    }

    /// The menus this window opened are its menu children - one chain: a
    /// menu's own creates (its submenus) come up to this window
    /// ([`HeadlessWindow::pump_children`]). There is no pointer grab headless
    /// to close them when the user leaves them, so the owner does: all of
    /// them, at once (EVENTS7).
    fn dismiss_menu_windows(&mut self) -> bool {
        let mut any = false;
        for child in &mut self.children {
            if child.is_open() && child.is_menu_window() {
                child.close();
                any = true;
            }
        }
        self.children.retain(HeadlessWindow::is_open);
        any
    }

    /// The popups are this window's children: run the pass of every one the
    /// owner just forwarded a key to, right now, as X11, macOS and Win32 do -
    /// headless makes no popup a key window, so a script's key always lands
    /// in the owner, and nothing else would ever wake the popup to replay it.
    fn deliver_forwarded_keys(&mut self) {
        use azul_core::events::ProcessEventResult as R;
        for child in &mut self.children {
            if !child.is_open()
                || !crate::desktop::shell2::common::transient::has_forwarded_keys(
                    child.common.current_window_state(),
                )
            {
                continue;
            }
            let r = child.process_window_events(0);
            if r == R::ShouldRegenerateDomAllWindows {
                child.request_regeneration_all_windows();
            }
            if r != R::DoNothing {
                child.service_frame(r);
            }
        }
    }

    fn show_tooltip_from_callback(&mut self, _text: &str, _anchor: azul_core::geom::LogicalRect) {
        // No-op — no visual surface to show a tooltip on
    }

    fn hide_tooltip_from_callback(&mut self) {
        // No-op
    }

    fn window_follows_position_changes(&self) -> bool {
        // Nothing moves headless: a tear-off drag tracks the pointer
        // arithmetically, the same regime as a Wayland popup.
        false
    }

    fn sync_window_state(&mut self) {
        // No native window to synchronise, so there is no OS-sync baseline to
        // diff either (`os_synced_state` stays `None`).
    }
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stub() -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let fc_cache = Arc::new(FcFontCache::default());
        let app_data = Arc::new(RefCell::new(RefAny::new(())));
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        HeadlessWindow::new(
            WindowCreateOptions::default(),
            app_data,
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap()
    }

    #[test]
    fn test_stub_window_creation() {
        let window = make_stub();
        assert!(window.is_open());
    }

    // =====================================================================
    // Damage harness — pure-Rust (no X11) simulation of the repaint path.
    //
    // Builds a HeadlessWindow with a controlled layout callback, drives state
    // changes, and captures the calculated FrameDamage + the rendered
    // display-list text. Uses println! to trace the architecture (run with
    // `cargo test -p azul-dll damage_ -- --nocapture`).
    // =====================================================================

    use azul_core::{
        callbacks::{LayoutCallback, LayoutCallbackInfo},
        dom::Dom,
        geom::LogicalSize,
        refany::OptionRefAny,
    };
    use azul_layout::solver3::display_list::DisplayListItem;

    /// Minimal app state the harness layout callback reads.
    #[derive(Debug, Clone)]
    struct UiState {
        label: String,
    }

    /// Layout callback: `<body><div>{label}</div></body>`. The text content is
    /// driven entirely by UiState, so a label change is a pure text-content
    /// change at a stable DOM position — the cross-window stale-text repro,
    /// headless.
    extern "C" fn harness_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let label = data
            .downcast_ref::<UiState>()
            .map(|s| s.label.clone())
            .unwrap_or_default();
        Dom::create_body().with_child(Dom::create_div().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper(label.as_str()),
        ))
    }

    /// One embedded font for the whole harness. Using a bundled font instead of
    /// `FcFontCache::build()` makes glyph metrics DETERMINISTIC (tests never
    /// depend on which fonts the host has installed) and does ZERO disk access —
    /// the system-font scan was both a flakiness source and a contributor to the
    /// build-machine lockup (every test forking a full font enumeration).
    const HARNESS_FONT: &[u8] =
        include_bytes!("../../../../../assets/fonts/InstrumentSerif-Regular.ttf");

    /// Parse [`HARNESS_FONT`] and insert it straight into the window's
    /// `FontManager` so text shapes without any system-font scan.
    ///
    /// Why inject the parsed font rather than register it in the `FcFontCache`
    /// (the obvious approach)? Because a single in-memory font CANNOT serve text
    /// through an otherwise-empty fontconfig cache (the trail, #15):
    /// - generic families ("serif", azul's default) are EXPANDED to a hardcoded OS list ("DejaVu
    ///   Serif", …) and the generic is dropped, so a custom font is never matched by the generic
    ///   name (the `web/eventloop.rs` "serif sans-serif monospace" trick silently does nothing);
    /// - the Unicode-fallback path skips every codepoint < U+0400 (it assumes the CSS fallbacks'
    ///   own glyphs cover Latin — i.e. that real system fonts exist), so ASCII resolves no fallback
    ///   in an empty cache.
    ///
    /// The shaper's last resort, however, is a direct glyph probe over the
    /// LOADED fonts (`split_text_by_font_coverage`'s `.or_else` →
    /// `font.has_glyph`). With an empty `FcFontCache` every char misses
    /// fontconfig and falls through to that probe — so a font present in the
    /// `FontManager` is used by real cmap coverage, no font-family needed on the
    /// DOM. We insert with interior mutability (`insert_font(&self, …)`), so this
    /// runs before the test's first `regenerate_layout`.
    ///
    /// (The underlying gap — a bundled in-memory font can't serve generic
    /// families / Latin via the cache — is a real rust-fontconfig footgun that
    /// also breaks the web/wasm fallback; flagged for an upstream fix.)
    fn inject_harness_font(window: &HeadlessWindow) {
        use azul_layout::text3::default::font_ref_from_bytes;
        let font_ref = match font_ref_from_bytes(HARNESS_FONT, 0, false) {
            Some(f) => f,
            None => return,
        };
        if let Some(lw) = window.common.layout_window.as_ref() {
            lw.font_manager
                .insert_font(rust_fontconfig::FontId::new(), font_ref);
        }
    }

    fn make_window_with(
        state: &Arc<RefCell<RefAny>>,
        cb: azul_core::callbacks::LayoutCallbackType,
    ) -> HeadlessWindow {
        make_window_sized(state, cb, 400.0, 300.0)
    }

    /// [`make_window_with`] at an explicit viewport.
    ///
    /// The 400x300 default is PHONE-sized: a widget with a responsive layout
    /// (the ribbon collapses its tab strip into one mobile tab button)
    /// renders its small-screen variant there, and a test aiming at desktop
    /// chrome finds nodes that were never laid out.
    fn make_window_sized(
        state: &Arc<RefCell<RefAny>>,
        cb: azul_core::callbacks::LayoutCallbackType,
        width: f32,
        height: f32,
    ) -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        // Empty cache → NO system-font scan / disk access. The deterministic
        // embedded font is injected into the FontManager below (see
        // `inject_harness_font` for why the cache route doesn't work).
        let fc_cache = Arc::new(FcFontCache::default());
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb,
            ctx: OptionRefAny::None,
        };
        opts.window_state.size.dimensions = LogicalSize::new(width, height);
        let window = HeadlessWindow::new(
            opts,
            state.clone(),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap();
        inject_harness_font(&window);
        window
    }

    fn make_harness_window(state: &Arc<RefCell<RefAny>>) -> HeadlessWindow {
        make_window_with(state, harness_layout)
    }

    /// Total area of a FrameDamage (None for Full = unbounded, 0.0 for None).
    fn damage_area(d: &FrameDamage) -> Option<f32> {
        match d {
            FrameDamage::None => Some(0.0),
            FrameDamage::Full => None,
            FrameDamage::Rects(rs) => Some(rs.iter().map(|r| r.size.width * r.size.height).sum()),
        }
    }

    /// State + layout for a non-text colored box (isolates the damage system
    /// from text-shaping generation bugs).
    #[derive(Debug, Clone)]
    struct BoxState {
        red: bool,
    }

    extern "C" fn harness_layout_box(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let red = data
            .downcast_ref::<BoxState>()
            .map(|s| s.red)
            .unwrap_or(false);
        let color = if red {
            ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }
        } else {
            ColorU {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            }
        };
        let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
        Dom::create_body().with_child(
            Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(100.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(50.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ),
        )
    }

    /// A ribbon-shaped row: LEFT-anchored group, a flex spacer, then two
    /// RIGHT-anchored groups (they move by the FULL width delta on resize),
    /// with a centered label underneath (moves by HALF the delta) — the
    /// mover-diversity of the live miniword ribbon, in miniature.
    extern "C" fn harness_layout_ribbon(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions as C,
            props::{
                basic::color::ColorU,
                layout::{
                    dimensions::{LayoutHeight, LayoutWidth},
                    flex::{LayoutFlexDirection, LayoutFlexGrow},
                },
                property::CssProperty,
                style::{
                    background::{StyleBackgroundContent, StyleBackgroundContentVec},
                    text::StyleTextAlign,
                },
            },
        };

        let label = data
            .downcast_ref::<UiState>()
            .map(|s| s.label.clone())
            .unwrap_or_default();
        let bg = |r: u8, g: u8, b: u8| -> StyleBackgroundContentVec {
            vec![StyleBackgroundContent::Color(ColorU { r, g, b, a: 255 })].into()
        };
        let group = |color: StyleBackgroundContentVec, text: &str| {
            Dom::create_div()
                .with_css_props(
                    vec![
                        C::simple(CssProperty::width(LayoutWidth::px(90.0))),
                        C::simple(CssProperty::height(LayoutHeight::px(40.0))),
                        C::simple(CssProperty::background_content(color)),
                    ]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    text,
                ))
        };
        let spacer = Dom::create_div().with_css_props(
            vec![C::simple(CssProperty::flex_grow(
                LayoutFlexGrow::const_new(1),
            ))]
            .into(),
        );
        let row = Dom::create_div()
            .with_css_props(
                vec![
                    C::simple(CssProperty::flex_direction(LayoutFlexDirection::Row)),
                    C::simple(CssProperty::height(LayoutHeight::px(44.0))),
                ]
                .into(),
            )
            .with_child(group(bg(200, 60, 60), "Clip"))
            .with_child(spacer)
            .with_child(group(bg(60, 160, 60), "Font"))
            .with_child(group(bg(60, 60, 200), "Layout"));
        let centered = Dom::create_div()
            .with_css_props(vec![C::simple(CssProperty::text_align(StyleTextAlign::Center))].into())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                label.as_str(),
            ));
        Dom::create_body().with_child(row).with_child(centered)
    }

    /// TASK #19 REPRO ATTEMPT: incremental resizes of a multi-mover row must
    /// stay pixel-identical to a fresh render at every step. The live
    /// symptom ("LAYOUT tab flapping, groups merging") is stale bands after
    /// drag steps; if the defect is render-side (damage/blit), this goes red
    /// with first-diff coordinates. If this stays green, the defect is in
    /// the wayland PRESENT path (slots/timing), not the renderer.
    #[test]
    fn ribbon_row_stays_pixel_true_across_incremental_resizes() {
        use azul_core::geom::LogicalSize;
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "centered caption".to_string(),
        })));
        let mut window = make_window_sized(&state, harness_layout_ribbon, 800.0, 240.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let mut prev_w = 800.0f32;
        // Odd step: fractional centered deltas + non-integral mover deltas.
        for w in [793.0f32, 786.0, 779.0, 772.0, 765.0] {
            let full = window.common.request_regeneration_for_resize(
                LogicalSize::new(prev_w, 240.0),
                LogicalSize::new(w, 240.0),
            );
            window
                .common
                .update_window_state(event::WindowStateSource::Os, |ws| {
                    ws.size.dimensions = LogicalSize::new(w, 240.0);
                });
            if full {
                window.regenerate_layout().expect("regen");
            } else {
                let _ = window.common.take_relayout_only();
                window.relayout_only().expect("relayout");
            }
            let incr = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("incremental frame")
                .clone_pixmap();

            // Fresh window at the same size = ground truth.
            let fresh_state = Arc::new(RefCell::new(RefAny::new(UiState {
                label: "centered caption".to_string(),
            })));
            let mut fresh = make_window_sized(&fresh_state, harness_layout_ribbon, w, 240.0);
            fresh.regenerate_layout().expect("fresh layout");
            let full_frame = fresh
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("fresh frame")
                .clone_pixmap();

            assert_eq!(incr.width(), full_frame.width(), "width at {w}");
            let (a, b) = (incr.data(), full_frame.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % incr.width(), px / incr.width()));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step to {w}: incremental frame differs from fresh in {diffs} px,                  first at {first:?} — stale band (the live flapping/merge symptom)"
            );
            prev_w = w;
        }
    }

    /// #19 with the REAL ribbon widget (USER ask: "test the ribbon again
    /// with the new damage-rect op" — this is the headless equivalent of
    /// `assert_damage_sound`'s covers-changes law, applied at every drag
    /// step): sweep the width in small odd steps across the group-tiling
    /// thresholds; the incrementally presented frame must be bit-identical
    /// to a fresh full render at every width. The earlier law used a
    /// synthetic row (the "fixture can't express it" gap) — this one builds
    /// `azul_layout::widgets::ribbon::Ribbon` itself: tabs + labeled groups
    /// + the small-button columns whose group labels visually "merged" live.
    #[test]
    fn real_ribbon_resize_sweep_matches_fresh_at_every_step() {
        use azul_core::geom::LogicalSize;
        use azul_layout::widgets::ribbon::{
            Ribbon, RibbonButton, RibbonColumn, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec,
        };

        fn tabs() -> RibbonTabVec {
            let col = |a: (&str, &str), b: (&str, &str)| {
                RibbonItem::Column(
                    RibbonColumn::new()
                        .with_item(RibbonItem::SmallButton(RibbonButton::new(
                            a.0.into(),
                            a.1.into(),
                        )))
                        .with_item(RibbonItem::SmallButton(RibbonButton::new(
                            b.0.into(),
                            b.1.into(),
                        ))),
                )
            };
            let home =
                RibbonTab::new("HOME".into())
                    .with_group(
                        RibbonGroup::new("Clipboard".into())
                            .with_item(RibbonItem::LargeButton(RibbonButton::new(
                                "content_paste".into(),
                                "Paste".into(),
                            )))
                            .with_item(col(("content_cut", "Cut"), ("content_copy", "Copy"))),
                    )
                    .with_group(
                        RibbonGroup::new("Font".into())
                            .with_item(col(("format_bold", "Bold"), ("format_italic", "Italic")))
                            .with_item(col(
                                ("format_underlined", "Underline"),
                                ("format_color_text", "Color"),
                            )),
                    )
                    .with_group(RibbonGroup::new("Styles".into()).with_item(
                        RibbonItem::LargeButton(RibbonButton::new("style".into(), "Styles".into())),
                    ));
            RibbonTabVec::from_vec(vec![
                home,
                RibbonTab::new("INSERT".into()).with_group(
                    RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                        RibbonButton::new("layers".into(), "Insert".into()),
                    )),
                ),
                RibbonTab::new("LAYOUT".into()).with_group(
                    RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                        RibbonButton::new("layers".into(), "Layout".into()),
                    )),
                ),
            ])
        }
        extern "C" fn layout_real_ribbon(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            Dom::create_body().with_child(tabs_dom())
        }
        fn tabs_dom() -> Dom {
            Ribbon::new(tabs()).with_active_tab(0).dom()
        }

        let state = Arc::new(RefCell::new(RefAny::new(0usize)));
        let mut window = make_window_sized(&state, layout_real_ribbon, 900.0, 260.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let mut prev_w = 900.0f32;
        // Odd 13px steps: fractional group re-tiling + label re-centering at
        // every step, crossing the widths where groups compress.
        let mut w = 887.0f32;
        while w >= 614.0 {
            let full = window.common.request_regeneration_for_resize(
                LogicalSize::new(prev_w, 260.0),
                LogicalSize::new(w, 260.0),
            );
            window
                .common
                .update_window_state(event::WindowStateSource::Os, |ws| {
                    ws.size.dimensions = LogicalSize::new(w, 260.0);
                });
            if full {
                window.regenerate_layout().expect("regen");
            } else {
                let _ = window.common.take_relayout_only();
                window.relayout_only().expect("relayout");
            }
            let incr = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("incremental frame")
                .clone_pixmap();

            let fresh_state = Arc::new(RefCell::new(RefAny::new(0usize)));
            let mut fresh = make_window_sized(&fresh_state, layout_real_ribbon, w, 260.0);
            fresh.regenerate_layout().expect("fresh layout");
            let full_frame = fresh
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("fresh frame")
                .clone_pixmap();

            assert_eq!(incr.width(), full_frame.width(), "width at {w}");
            let (a, b) = (incr.data(), full_frame.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % incr.width(), px / incr.width()));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "REAL ribbon diverges from fresh render at width {w}: {diffs} px, first {first:?} \
                 — the live 'merge' class (patched-DL under-damage)"
            );
            prev_w = w;
            w -= 13.0;
        }
    }

    /// `make_window_sized` with the REAL system font cache instead of the
    /// injected deterministic harness font — for laws that must reproduce
    /// live glyph metrics ("Liberation Sans" fractional advances; the
    /// fit-test epsilon is metric-dependent).
    fn make_window_sized_real_fonts(
        state: &Arc<RefCell<RefAny>>,
        cb: azul_core::callbacks::LayoutCallbackType,
        width: f32,
        height: f32,
    ) -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let fc_cache = Arc::new(FcFontCache::build());
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb,
            ctx: OptionRefAny::None,
        };
        opts.window_state.size.dimensions = LogicalSize::new(width, height);
        HeadlessWindow::new(
            opts,
            state.clone(),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap()
    }

    /// USER 2026-08-12 live report: resizing the azwriter window narrower
    /// made "PAGE LAYOUT" — the only tab label with internal whitespace —
    /// suddenly break onto a second line, and the group captions below
    /// visibly de-centered. Fresh layout at every integer width 600..=1400
    /// is clean (layout/tests/ribbon_tab_whitespace.rs), so this law walks
    /// the INCREMENTAL resize path with azwriter parity (nine tabs,
    /// desktop-only DOM, pinned "Liberation Sans"): 1px steps down and back
    /// up, on the integer AND the half-pixel lattice. At every step, on the
    /// live window's own layout: every tab label keeps one-line height and
    /// every group caption stays centered on its footer.
    #[test]
    fn azwriter_ribbon_resize_sweep_keeps_tabs_one_line_and_captions_centered() {
        use azul_core::{
            dom::{DomId, DomNodeId, NodeId, NodeType},
            geom::LogicalSize,
        };
        use azul_layout::widgets::ribbon::{
            Ribbon, RibbonAppButton, RibbonButton, RibbonColumn, RibbonGroup, RibbonItem,
            RibbonTab, RibbonTabVec,
        };

        const TAB_LABELS: &[&str] = &[
            "HOME",
            "INSERT",
            "DESIGN",
            "PAGE LAYOUT",
            "REFERENCES",
            "MAILINGS",
            "REVIEW",
            "VIEW",
        ];
        const CAPTIONS: &[&str] = &["Clipboard", "Font", "Paragraph", "Styles"];

        extern "C" fn layout_azwriter_ribbon(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::{
                dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
                props::{
                    basic::font::{StyleFontFamily, StyleFontFamilyVec},
                    property::CssProperty,
                },
            };
            let col = |labels: [&str; 3]| {
                RibbonItem::Column(labels.into_iter().fold(RibbonColumn::new(), |c, l| {
                    c.with_item(RibbonItem::SmallButton(RibbonButton::new(
                        "content_cut".into(),
                        l.into(),
                    )))
                }))
            };
            let home =
                RibbonTab::new("HOME".into())
                    .with_group(RibbonGroup::new("Clipboard".into()).with_item(col([
                        "Cut",
                        "Copy",
                        "Format Painter",
                    ])))
                    .with_group(
                        RibbonGroup::new("Font".into()).with_item(col(["Grow", "Shrink", "Clear"])),
                    )
                    .with_group(RibbonGroup::new("Paragraph".into()).with_item(col([
                        "Bullets",
                        "Numbering",
                        "Sort",
                    ])))
                    .with_group(RibbonGroup::new("Styles".into()).with_item(
                        RibbonItem::LargeButton(RibbonButton::new("style".into(), "Styles".into())),
                    ));
            let mut tabs = vec![home];
            for label in [
                "INSERT",
                "DESIGN",
                "PAGE LAYOUT",
                "REFERENCES",
                "MAILINGS",
                "REVIEW",
                "VIEW",
            ] {
                tabs.push(RibbonTab::new(label.into()).with_group(
                    RibbonGroup::new("Preview".into()).with_item(RibbonItem::LargeButton(
                        RibbonButton::new("layers".into(), label.into()),
                    )),
                ));
            }
            let mut ribbon = Ribbon::new(RibbonTabVec::from_vec(tabs))
                .with_app_button(RibbonAppButton::new("FILE".into()));
            // The style bundle's fields are `None` = "no opinion" since the
            // theme migration; the resolver is the theme's answer, and the
            // font is appended to THAT.
            let mut v = ribbon.style.resolved_container_style().as_slice().to_vec();
            v.push(CssPropertyWithConditions::simple(
                CssProperty::const_font_family(StyleFontFamilyVec::from_vec(vec![
                    StyleFontFamily::System("Liberation Sans".into()),
                ])),
            ));
            ribbon.style.container_style =
                azul_css::dynamic_selector::OptionCssPropertyWithConditionsVec::Some(
                    CssPropertyWithConditionsVec::from_vec(v),
                );
            Dom::create_body().with_child(ribbon.dom_desktop())
        }

        fn lw_of(window: &HeadlessWindow) -> &azul_layout::window::LayoutWindow {
            window.common.layout_window.as_ref().expect("layout window")
        }
        // Labels are `<p>`-wrapped text: the text node has no rect of its
        // own; wrap + centering are read off the text3 UnifiedLayout of the
        // wrapping `<p>` and its box rect.
        fn find_text_node(window: &HeadlessWindow, needle: &str) -> Option<usize> {
            let lw = lw_of(window);
            let result = lw.layout_results.get(&DomId::ROOT_ID)?;
            let node_data = result.styled_dom.node_data.as_container();
            (0..node_data.len()).find(|i| {
                matches!(
                    node_data[NodeId::new(*i)].get_node_type(),
                    NodeType::Text(s) if s.as_ref().as_str() == needle
                )
            })
        }
        fn parent_of(window: &HeadlessWindow, child: usize) -> Option<NodeId> {
            let lw = lw_of(window);
            let result = lw.layout_results.get(&DomId::ROOT_ID)?;
            let hier = result.styled_dom.node_hierarchy.as_container();
            hier[NodeId::new(child)].parent_id()
        }
        fn rect_of(window: &HeadlessWindow, i: NodeId) -> Option<azul_core::geom::LogicalRect> {
            let id = DomNodeId {
                dom: DomId::ROOT_ID,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(i)),
            };
            lw_of(window).get_node_layout_rect(id)
        }

        let state = Arc::new(RefCell::new(RefAny::new(0usize)));
        let mut window =
            make_window_sized_real_fonts(&state, layout_azwriter_ribbon, 1000.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Box-level detectors (static screen text retains no UnifiedLayout —
        // §3.2 drops it): a wrapped label doubles its `<p>` height; a
        // de-centered caption is a `<p>` that failed to grow onto its footer.
        // Both detectors are NC-proven in layout/tests/ribbon_tab_whitespace.rs.
        let base_h: Vec<f32> = TAB_LABELS
            .iter()
            .map(|l| {
                let i = find_text_node(&window, l).unwrap_or_else(|| panic!("tab {l} @1000"));
                let p = parent_of(&window, i).expect("tab <p>");
                rect_of(&window, p).expect("tab <p> rect").size.height
            })
            .collect();

        let check = |window: &HeadlessWindow, w: f32| {
            for (k, l) in TAB_LABELS.iter().enumerate() {
                let i = find_text_node(window, l)
                    .unwrap_or_else(|| panic!("tab '{l}' vanished at w={w}"));
                let p = parent_of(window, i).expect("tab <p>");
                let r =
                    rect_of(window, p).unwrap_or_else(|| panic!("tab '{l}' has no box at w={w}"));
                assert!(
                    r.size.height <= base_h[k] * 1.5,
                    "tab '{l}' WRAPPED at w={w}: <p> height {:.2} vs one-line {:.2} — the live \
                     PAGE-LAYOUT symptom (resize-path line break diverges from fresh layout)",
                    r.size.height,
                    base_h[k]
                );
            }
            for l in CAPTIONS {
                let i = find_text_node(window, l)
                    .unwrap_or_else(|| panic!("caption '{l}' vanished at w={w}"));
                let p = parent_of(window, i).expect("caption <p>");
                let pr = rect_of(window, p).expect("caption <p> rect");
                let f = parent_of(window, p.index()).expect("caption footer");
                let fr = rect_of(window, f).expect("footer rect");
                let err = (pr.origin.x + pr.size.width / 2.0) - (fr.origin.x + fr.size.width / 2.0);
                assert!(
                    err.abs() <= 1.0,
                    "caption '{l}' OFF-CENTER by {err:.2}px at w={w} — the live de-centering \
                     symptom"
                );
            }
        };
        check(&window, 1000.0);

        // Four lattices: integer down/up, half-pixel down/up. ~1600 resize
        // steps total, every one through the production resize decision.
        let phases: [(f32, f32, f32); 4] = [
            (999.0, 600.0, -1.0),
            (601.0, 1000.0, 1.0),
            (999.5, 600.5, -1.0),
            (601.5, 999.5, 1.0),
        ];
        let mut prev_w = 1000.0f32;
        for (start, end, step) in phases {
            let mut w = start;
            loop {
                if (step < 0.0 && w < end) || (step > 0.0 && w > end) {
                    break;
                }
                let full = window.common.request_regeneration_for_resize(
                    LogicalSize::new(prev_w, 300.0),
                    LogicalSize::new(w, 300.0),
                );
                window
                    .common
                    .update_window_state(event::WindowStateSource::Os, |ws| {
                        ws.size.dimensions = LogicalSize::new(w, 300.0);
                    });
                if full {
                    window.regenerate_layout().expect("regen");
                } else {
                    let _ = window.common.take_relayout_only();
                    window.relayout_only().expect("relayout");
                }
                check(&window, w);
                prev_w = w;
                w += step;
            }
        }
    }

    /// Overlapping rects must still convert each pixel exactly ONCE. A
    /// shift clip and the repaint strip inside it both arrive in the swizzle
    /// set, and a per-rect loop swapped the overlap twice. That is a no-op,
    /// leaving those pixels in RGBA order on an ARGB8888 buffer, so blue
    /// chrome showed brown for a frame.
    #[test]
    fn overlapping_swizzle_rects_convert_each_pixel_exactly_once() {
        let (w, h) = (10usize, 6usize);
        let stride = w * 4;
        let mut buf = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let o = y * stride + x * 4;
                buf[o] = (10 + x) as u8;
                buf[o + 1] = (100 + y) as u8;
                buf[o + 2] = (200 - x) as u8;
                buf[o + 3] = 255;
            }
        }
        let orig = buf.clone();
        // A big clip, a strip fully inside it, and a rect that half-overlaps.
        let rects = [(0, 0, 6, 4), (1, 1, 3, 1), (4, 2, 5, 3)];
        swizzle_rb_in_rects(&mut buf, stride, h, &rects);
        for y in 0..h {
            for x in 0..w {
                let o = y * stride + x * 4;
                let inside = rects.iter().any(|&(rx, ry, rw, rh)| {
                    (x as i32) >= rx
                        && (x as i32) < rx + rw
                        && (y as i32) >= ry
                        && (y as i32) < ry + rh
                });
                if inside {
                    assert_eq!(buf[o], orig[o + 2], "swapped exactly once at {x},{y}");
                } else {
                    assert_eq!(buf[o], orig[o], "untouched at {x},{y}");
                }
            }
        }
    }

    /// #32: the commit swizzle converts R↔B EXACTLY inside the given rects
    /// and leaves every other pixel untouched. The pattern has R != B at
    /// every pixel, so a no-op swizzle fails the inside assertions and an
    /// overreaching swizzle fails the outside assertions — both failure
    /// directions are expressible by this fixture.
    #[test]
    fn commit_swizzle_converts_rects_exactly_and_nothing_else() {
        let (w, h) = (8usize, 6usize);
        let stride = w * 4;
        let mut buf = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let o = y * stride + x * 4;
                buf[o] = (10 + x) as u8; // R
                buf[o + 1] = (100 + y) as u8; // G
                buf[o + 2] = (200 - x) as u8; // B (never equals R)
                buf[o + 3] = 255;
            }
        }
        let orig = buf.clone();
        // Two interior rects + one deliberately out-of-bounds rect (clamped).
        let rects = [(1, 1, 3, 2), (5, 0, 2, 4), (-2, 4, 4, 50)];
        swizzle_rb_in_rects(&mut buf, stride, h, &rects);
        let inside = |x: usize, y: usize| {
            rects.iter().any(|&(rx, ry, rw, rh)| {
                (x as i32) >= rx && (x as i32) < rx + rw && (y as i32) >= ry && (y as i32) < ry + rh
            })
        };
        for y in 0..h {
            for x in 0..w {
                let o = y * stride + x * 4;
                if inside(x, y) {
                    assert_eq!(buf[o], orig[o + 2], "R<-B at {x},{y}");
                    assert_eq!(buf[o + 2], orig[o], "B<-R at {x},{y}");
                } else {
                    assert_eq!(
                        &buf[o..o + 4],
                        &orig[o..o + 4],
                        "untouched outside at {x},{y}"
                    );
                }
                assert_eq!(buf[o + 1], orig[o + 1], "G untouched at {x},{y}");
                assert_eq!(buf[o + 3], orig[o + 3], "A untouched at {x},{y}");
            }
        }
    }

    /// The family of the face the app ships in
    /// `a_layout_callbacks_snapshot_resolves_the_family_of_the_apps_shipped_face`.
    const SHIPPED_FAMILY: &str = "Azul Snapshot Shipped Face";

    /// What a layout callback saw of the font cache snapshot it made from its info.
    #[derive(Debug, Clone)]
    struct SnapshotSeen {
        /// The registered fonts of the app (its shipped face among them).
        app: Arc<azul_layout::text3::fontreg::RegisteredFonts>,
        /// Whether the snapshot's font manager draws with the registered fonts of the app.
        has_the_apps_fonts: bool,
        /// The ids the family of the shipped face is indexed under in the snapshot's font manager.
        family_ids: Vec<rust_fontconfig::FontId>,
    }

    /// A layout callback that makes a font cache snapshot of its info, as an app that paginates a
    /// document in its callback does (`FontCacheSnapshot::from_layout_info`).
    extern "C" fn layout_with_a_snapshot(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
        let snapshot = azul_layout::resource_handles::FontCacheSnapshot::from_layout_info(&info);
        if let (Some(mut seen), Some(manager)) =
            (data.downcast_mut::<SnapshotSeen>(), snapshot.as_font_manager())
        {
            let has_the_apps_fonts = Arc::ptr_eq(&manager.registered_fonts, &seen.app);
            let norm = rust_fontconfig::utils::normalize_family_name(SHIPPED_FAMILY);
            let family_ids: Vec<rust_fontconfig::FontId> = manager
                .memory_families
                .get(&norm)
                .map(|faces| faces.iter().map(|face| face.font_match.id).collect())
                .unwrap_or_default();
            seen.has_the_apps_fonts = has_the_apps_fonts;
            seen.family_ids = family_ids;
        }
        Dom::create_body()
    }

    /// An app with a shipped face (`AppConfig::add_font`): a layout callback of its window makes
    /// a font cache snapshot from its info - what an app paginates a document with - and the
    /// family of the shipped face resolves in it to the face the window draws with, the very id,
    /// because the window hands the app's registered fonts to its callbacks. A window that has
    /// none of the app's fonts hands none over.
    /// Mutation: `regenerate_layout` leaving `registered_fonts` out of the ref data it builds
    /// fails the first block.
    #[test]
    fn a_layout_callbacks_snapshot_resolves_the_family_of_the_apps_shipped_face() {
        use azul_layout::text3::fontreg::RegisteredFonts;

        let norm = rust_fontconfig::utils::normalize_family_name(SHIPPED_FAMILY);
        // The app's registered fonts, with the face it ships.
        let app = Arc::new(RegisteredFonts::default());
        let face = azul_layout::text3::ui_fonts::bundled_ui_fonts()[0].bytes.clone();
        app.set_app_fonts(&[azul_core::resources::NamedFont::new(
            SHIPPED_FAMILY.into(),
            face.into(),
        )]);

        // One layout of a window, given the app's registered fonts or not: the ids the window
        // itself has the family under, and what its callback saw.
        let layout = |given: Option<&Arc<RegisteredFonts>>| {
            let state = Arc::new(RefCell::new(RefAny::new(SnapshotSeen {
                app: Arc::clone(&app),
                has_the_apps_fonts: false,
                family_ids: Vec::new(),
            })));
            let mut window = make_window_with(&state, layout_with_a_snapshot);
            if let Some(fonts) = given {
                window.share_registered_fonts(fonts);
            }
            window.regenerate_layout().expect("the layout runs");
            let on_screen: Vec<rust_fontconfig::FontId> = window
                .common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.font_manager.memory_families.get(&norm))
                .map(|faces| faces.iter().map(|face| face.font_match.id).collect())
                .unwrap_or_default();
            let (has_the_apps_fonts, in_the_snapshot) = state
                .borrow_mut()
                .downcast_ref::<SnapshotSeen>()
                .map(|seen| (seen.has_the_apps_fonts, seen.family_ids.clone()))
                .expect("the layout callback ran");
            (on_screen, has_the_apps_fonts, in_the_snapshot)
        };

        let (on_screen, has_the_apps_fonts, in_the_snapshot) = layout(Some(&app));
        assert!(!on_screen.is_empty(), "premise: the window draws the family with the shipped face");
        assert!(has_the_apps_fonts, "the snapshot draws with the registered fonts of the app");
        assert_eq!(
            in_the_snapshot, on_screen,
            "the family resolves to the face the window draws with"
        );

        let (on_screen, has_the_apps_fonts, in_the_snapshot) = layout(None);
        assert!(on_screen.is_empty(), "a window that has no fonts of the app has no shipped face");
        assert!(
            !has_the_apps_fonts && in_the_snapshot.is_empty(),
            "and the snapshot of its callback has none either"
        );
    }

    /// State for the harvested-breakpoint pin: counts `layout()` invocations.
    #[derive(Debug, Clone)]
    struct BreakpointState {
        layouts: usize,
    }

    /// The ribbon pattern in miniature: an inline conditional property that
    /// flips the box color at viewport width <= 720 (a threshold that was
    /// NOT on the old hardcoded CSS_BREAKPOINTS guess list).
    extern "C" fn harness_layout_breakpoint(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::{CssPropertyWithConditions, DynamicSelector, MinMaxRange},
            props::{
                basic::color::ColorU,
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        if let Some(mut st) = data.downcast_mut::<BreakpointState>() {
            st.layouts += 1;
        }
        let blue: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        })]
        .into();
        let red: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        })]
        .into();
        let cond_mobile: azul_css::dynamic_selector::DynamicSelectorVec =
            vec![DynamicSelector::ViewportWidth(MinMaxRange {
                min: f32::NAN,
                max: 720.0,
            })]
            .into();
        Dom::create_body().with_child(
            Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(100.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(50.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(blue)),
                    CssPropertyWithConditions::with_conditions(
                        CssProperty::background_content(red),
                        cond_mobile,
                    ),
                ]
                .into(),
            ),
        )
    }

    /// The first non-white Rect color in DOM 0's display list.
    fn first_box_color(window: &HeadlessWindow) -> Option<(u8, u8, u8)> {
        use azul_core::dom::DomId;
        let lw = window.common.layout_window.as_ref()?;
        let dl = &lw.layout_results.get(&DomId { inner: 0 })?.display_list;
        dl.items.iter().find_map(|it| match it {
            DisplayListItem::Rect { color, .. }
                if !(color.r > 240 && color.g > 240 && color.b > 240) =>
            {
                Some((color.r, color.g, color.b))
            }
            _ => None,
        })
    }

    /// THE HARVESTED-BREAKPOINT PIN (task #20). Shrinking across an inline
    /// conditional threshold (the ribbon's 720px pattern) must re-invoke
    /// `layout()` and flip the style; crossing a threshold that exists only
    /// on the OLD hardcoded guess list (768) must NOT — that list fired a
    /// ~66ms full regeneration on every drag across 640/768/1024/...
    #[test]
    fn resize_regenerates_exactly_at_harvested_breakpoints() {
        use azul_core::geom::LogicalSize;
        let state = Arc::new(RefCell::new(RefAny::new(BreakpointState { layouts: 0 })));
        let mut window = make_window_sized(&state, harness_layout_breakpoint, 800.0, 600.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let base = state
            .borrow_mut()
            .downcast_ref::<BreakpointState>()
            .map(|s| s.layouts)
            .unwrap_or(0);
        assert!(base > 0, "the layout callback must have run");
        assert_eq!(
            first_box_color(&window),
            Some((0, 0, 255)),
            "desktop = blue"
        );

        // 800 -> 750 crosses the old guess-list's 768 but NO threshold of
        // THIS dom: stays incremental, layout() NOT re-invoked.
        let full = window.common.request_regeneration_for_resize(
            LogicalSize::new(800.0, 600.0),
            LogicalSize::new(750.0, 600.0),
        );
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize::new(750.0, 600.0);
            });
        assert!(
            !full,
            "768 is not a threshold of this DOM — the hardcoded guess list must not fire"
        );
        // Mirror the shells' dispatch: not-full -> the INCREMENTAL entry
        // (relayout_only), exactly what wayland's resize arm calls.
        let _ = window.common.take_relayout_only();
        window.relayout_only().expect("relayout at 750");
        let after_750 = state
            .borrow_mut()
            .downcast_ref::<BreakpointState>()
            .map(|s| s.layouts)
            .unwrap_or(0);
        assert_eq!(
            after_750, base,
            "no layout() re-invoke for a non-crossing resize"
        );
        assert_eq!(
            first_box_color(&window),
            Some((0, 0, 255)),
            "still blue at 750"
        );

        // 750 -> 600 crosses the harvested 720: full regeneration + flip.
        let full = window.common.request_regeneration_for_resize(
            LogicalSize::new(750.0, 600.0),
            LogicalSize::new(600.0, 600.0),
        );
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize::new(600.0, 600.0);
            });
        window.regenerate_layout().expect("regen at 600");
        assert!(
            full,
            "crossing the harvested 720 must regenerate (shrink side!)"
        );
        let after_600 = state
            .borrow_mut()
            .downcast_ref::<BreakpointState>()
            .map(|s| s.layouts)
            .unwrap_or(0);
        assert!(
            after_600 > after_750,
            "layout() must re-run across the breakpoint"
        );
        assert_eq!(
            first_box_color(&window),
            Some((255, 0, 0)),
            "mobile style must apply after shrinking across 720"
        );

        // And back up: 600 -> 800 crosses 720 the other way.
        let full = window.common.request_regeneration_for_resize(
            LogicalSize::new(600.0, 600.0),
            LogicalSize::new(800.0, 600.0),
        );
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize::new(800.0, 600.0);
            });
        window.regenerate_layout().expect("regen back at 800");
        assert!(full, "growing back across 720 regenerates too");
        assert_eq!(first_box_color(&window), Some((0, 0, 255)), "desktop again");
    }

    fn set_box_red(state: &Arc<RefCell<RefAny>>, red: bool) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<BoxState>();
        if let Some(s) = opt.as_mut() {
            s.red = red;
        }
    }

    /// Glyph count of every Text item in the current display list (DOM 0).
    fn text_glyph_counts(window: &HeadlessWindow) -> Vec<usize> {
        use azul_core::dom::DomId;
        let lw = match window.common.layout_window.as_ref() {
            Some(lw) => lw,
            None => return Vec::new(),
        };
        let dl = match lw.layout_results.get(&DomId { inner: 0 }) {
            Some(r) => &r.display_list,
            None => return Vec::new(),
        };
        dl.items
            .iter()
            .filter_map(|it| match it {
                DisplayListItem::Text { glyphs, .. } => Some(glyphs.len()),
                _ => None,
            })
            .collect()
    }

    fn set_label(state: &Arc<RefCell<RefAny>>, new_label: &str) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<UiState>();
        if let Some(s) = opt.as_mut() {
            s.label = new_label.to_string();
        }
    }

    /// Damage must survive the cases that BREAK a naive item mapping.
    ///
    /// The randomised text sequence covers glyphs moving inside a stable
    /// box. These are the shapes that move the BOX, and each one breaks a
    /// different naive assumption:
    ///
    ///  * **box-shadow** — visual bounds deliberately EXCEED the item box (offset + blur + spread),
    ///    so damage taken from the box alone leaves the shadow's fringe stale.
    ///  * **shrinking** — the vacated area is outside the NEW bounds, so damage must include the
    ///    OLD bounds or the old pixels survive.
    ///  * **anonymous layout boxes** — the div holds inline text, so the layout tree carries boxes
    ///    the DOM never had, and resizing moves them; any NodeId -> item mapping has to cope with
    ///    items that have no DOM identity.
    ///
    /// Each step is compared pixel-for-pixel against a full repaint, the
    /// same ground truth `damage_survives_a_randomised_edit_sequence` uses.
    ///
    /// THE ITEM COUNT IS HELD CONSTANT ON PURPOSE. `compute_display_list_
    /// damage` returns `None` — full repaint — the moment the item count
    /// changes, so a fixture that adds or removes a shadow or a child never
    /// takes the incremental path at all. The first version of this test did
    /// exactly that and passed with damage forcibly cleared, i.e. it proved
    /// nothing. Everything here varies SIZE only, so the frame stays
    /// incremental and the assertion has something to catch.
    #[test]
    fn damage_survives_shadow_shrink_and_anonymous_boxes() {
        #[derive(Debug, Clone)]
        struct ShapeState {
            variant: usize,
        }

        extern "C" fn layout_shapes(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::{
                dynamic_selector::CssPropertyWithConditions as P,
                props::{
                    basic::{color::ColorU, PixelValue, PixelValueNoPercent},
                    layout::dimensions::{LayoutHeight, LayoutWidth},
                    property::CssProperty,
                    style::{
                        background::{StyleBackgroundContent, StyleBackgroundContentVec},
                        box_shadow::{BoxShadowClipMode, StyleBoxShadow},
                    },
                },
            };

            let v = data
                .downcast_ref::<ShapeState>()
                .map(|s| s.variant)
                .unwrap_or(0);
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
                r: 0,
                g: 0,
                b: 200,
                a: 255,
            })]
            .into();

            // Variant drives size, shadow presence and child count together,
            // so consecutive steps exercise several shapes at once.
            let (w, h) = match v % 4 {
                0 => (180.0, 90.0),
                1 => (60.0, 30.0),   // shrink: vacates a large area
                2 => (200.0, 100.0), // grow back
                _ => (120.0, 60.0),
            };
            let mut props = vec![
                P::simple(CssProperty::width(LayoutWidth::px(w))),
                P::simple(CssProperty::height(LayoutHeight::px(h))),
                P::simple(CssProperty::background_content(bg)),
            ];
            {
                let shadow = StyleBoxShadow {
                    offset_x: PixelValueNoPercent {
                        inner: PixelValue::const_px(6),
                    },
                    offset_y: PixelValueNoPercent {
                        inner: PixelValue::const_px(6),
                    },
                    blur_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(8),
                    },
                    spread_radius: PixelValueNoPercent {
                        inner: PixelValue::const_px(4),
                    },
                    clip_mode: BoxShadowClipMode::default(),
                    color: ColorU {
                        r: 0,
                        g: 0,
                        b: 0,
                        a: 200,
                    },
                };
                props.push(P::simple(CssProperty::box_shadow_left(shadow.clone())));
                props.push(P::simple(CssProperty::box_shadow_right(shadow.clone())));
                props.push(P::simple(CssProperty::box_shadow_top(shadow.clone())));
                props.push(P::simple(CssProperty::box_shadow_bottom(shadow)));
            }

            // A constant inline child: it forces the anonymous layout boxes
            // an inline formatting context creates (which have no DOM node),
            // while keeping the display-list item COUNT stable so the frame
            // stays on the incremental path — see the note on the test.
            let div = Dom::create_div().with_css_props(props.into()).with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper("one two three"),
            );
            Dom::create_body().with_child(div)
        }

        let state = Arc::new(RefCell::new(RefAny::new(ShapeState { variant: 0 })));
        let mut window = make_window_with(&state, layout_shapes);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        for step in 1..12usize {
            if let Ok(mut b) = state.try_borrow_mut() {
                if let Some(mut s) = b.downcast_mut::<ShapeState>() {
                    s.variant = step;
                }
            }
            window.regenerate_layout().expect("incremental relayout");
            let incremental = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("frame")
                .clone_pixmap();
            let damage = window.cpu_backend.last_frame_damage.clone();

            let mut fresh = make_window_with(&state, layout_shapes);
            fresh.regenerate_layout().expect("fresh layout");
            let full = fresh
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("fresh")
                .clone_pixmap();

            let (a, b) = (incremental.data(), full.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            let w = incremental.width();
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % w, px / w));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "variant {step} left {diffs} stale pixels, first at {first:?}. Damage did not \
                 cover a shadow fringe, a vacated area, or an anonymous-box change. damage = \
                 {damage:?}"
            );
        }
    }

    /// #27 native backbuffer: `render_frame` with an armed `native_target`
    /// draws EXACTLY the pixels the owned path produces — first full frame
    /// and every incremental follow-up — while retaining nothing: the frame
    /// lives ONLY in the external buffer (`last_frame` stays `None`, the
    /// target is consumed). The external buffer stands in for the Wayland shm
    /// slot; a single buffer trivially satisfies the shell's catch-up
    /// contract (it always holds frame N−1), so what this pins is the engine
    /// half: incremental rendering on top of an externally-owned base is
    /// bit-equal to rendering on the retained pixmap.
    #[test]
    fn native_target_render_matches_owned_and_retains_nothing() {
        #[derive(Debug, Clone)]
        struct NbState {
            variant: usize,
        }

        extern "C" fn layout_nb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::{
                dynamic_selector::CssPropertyWithConditions as P,
                props::{
                    basic::color::ColorU,
                    layout::dimensions::{LayoutHeight, LayoutWidth},
                    property::CssProperty,
                    style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
                },
            };

            let v = data
                .downcast_ref::<NbState>()
                .map(|s| s.variant)
                .unwrap_or(0);
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
                r: 200,
                g: 30,
                b: 30,
                a: 255,
            })]
            .into();
            // Sizes change per step, item count stays stable → the frames
            // after the first take the INCREMENTAL path (asserted below).
            let (w, h) = match v % 3 {
                0 => (180.0, 90.0),
                1 => (70.0, 40.0),
                _ => (220.0, 120.0),
            };
            let div = Dom::create_div()
                .with_css_props(
                    vec![
                        P::simple(CssProperty::width(LayoutWidth::px(w))),
                        P::simple(CssProperty::height(LayoutHeight::px(h))),
                        P::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "native backbuffer",
                ));
            Dom::create_body().with_child(div)
        }

        let state = Arc::new(RefCell::new(RefAny::new(NbState { variant: 0 })));
        let mut nat = make_window_with(&state, layout_nb);
        let mut own = make_window_with(&state, layout_nb);
        nat.regenerate_layout().expect("nat initial");
        own.regenerate_layout().expect("own initial");

        // The "slot": a heap buffer standing in for the shm mapping, seeded
        // with frame 1 so it starts caught-up.
        let seed = nat
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("frame 1")
            .clone_pixmap();
        let (pw, ph) = (seed.width(), seed.height());
        let mut slot = seed.data().to_vec();
        drop(seed);

        let mut saw_incremental = false;
        for step in 1..6usize {
            if let Ok(mut b) = state.try_borrow_mut() {
                if let Some(mut s) = b.downcast_mut::<NbState>() {
                    s.variant = step;
                }
            }
            // Arm: this frame renders DIRECTLY into the external buffer,
            // which holds the frame the previous step rendered into it -
            // which is exactly what makes the incremental path legal.
            nat.cpu_backend.native_target_holds_previous_frame = step > 1;
            nat.cpu_backend.native_target = unsafe {
                azul_layout::cpurender::AzulPixmap::from_external(slot.as_mut_ptr(), pw, ph)
            };
            nat.regenerate_layout().expect("native incremental");
            assert!(
                nat.cpu_backend.rendered_native,
                "step {step}: armed target was not rendered into"
            );
            assert!(
                nat.cpu_backend.last_frame.is_none(),
                "step {step}: native mode retained an owned frame"
            );
            assert!(
                nat.cpu_backend.native_target.is_none(),
                "step {step}: target not consumed"
            );
            let native_damage = nat.cpu_backend.last_frame_damage.clone();
            if matches!(native_damage, FrameDamage::Rects(_)) {
                saw_incremental = true;
            }

            own.regenerate_layout().expect("owned incremental");
            let reference = own
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("owned frame")
                .clone_pixmap();
            let b = reference.data();
            assert_eq!(slot.len(), b.len(), "step {step}: size mismatch");
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            for i in (0..slot.len()).step_by(4) {
                if slot[i] != b[i] || slot[i + 1] != b[i + 1] || slot[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % pw, px / pw));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step {step}: external buffer diverges from the owned render at {diffs} px, first \
                 {first:?} — native damage {native_damage:?}"
            );
        }
        assert!(
            saw_incremental,
            "every step took the full-repaint path — the external-base incremental law was never \
             exercised"
        );
    }

    /// A frame of `w` x `h` RGBA pixels whose bytes encode their position
    /// (R = x, G = y, B = x ^ y, A = 200) - any misplaced or unswapped byte
    /// shows up as a wrong value.
    fn coded_frame(w: usize, h: usize) -> Vec<u8> {
        let mut v = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                v[o] = x as u8;
                v[o + 1] = y as u8;
                v[o + 2] = (x ^ y) as u8;
                v[o + 3] = 200;
            }
        }
        v
    }

    #[test]
    fn a_damage_rect_lands_at_its_place_in_a_padded_buffer_and_nothing_else_is_touched() {
        let (w, h) = (10usize, 6usize);
        let src = coded_frame(w, h);
        let pitch = 64; // 16 px rows for a 10 px frame
        let mut dst = vec![0xEEu8; pitch * h];
        let n = copy_rgba_rects_into(&mut dst, pitch, &src, w * 4, w, h, &[(2, 1, 3, 2)], false);
        assert_eq!(n, 3 * 2 * 4);
        for y in 0..h {
            for x in 0..pitch / 4 {
                let d = &dst[y * pitch + x * 4..y * pitch + x * 4 + 4];
                if (2..5).contains(&x) && (1..3).contains(&y) {
                    let s = &src[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    assert_eq!(d, s, "({x},{y}) not copied verbatim");
                } else {
                    assert_eq!(d, [0xEE; 4], "({x},{y}) outside the rect was written");
                }
            }
        }
    }

    #[test]
    fn a_swapped_copy_writes_b_g_r_a_and_overlapping_rects_do_not_toggle_it_back() {
        let (w, h) = (8usize, 4usize);
        let src = coded_frame(w, h);
        let pitch = w * 4;
        let mut dst = vec![0u8; pitch * h];
        copy_rgba_rects_into(
            &mut dst,
            pitch,
            &src,
            w * 4,
            w,
            h,
            &[(0, 0, 8, 4), (2, 1, 4, 2)],
            true,
        );
        for y in 0..h {
            for x in 0..w {
                let o = (y * w + x) * 4;
                assert_eq!(
                    [dst[o], dst[o + 1], dst[o + 2], dst[o + 3]],
                    [src[o + 2], src[o + 1], src[o], src[o + 3]],
                    "({x},{y})"
                );
            }
        }
    }

    #[test]
    fn rects_reaching_past_the_frame_or_either_buffer_are_clipped_not_a_panic() {
        let (w, h) = (6usize, 5usize);
        let src = coded_frame(w, h);
        let pitch = 32;
        let mut dst = vec![0u8; pitch * h];
        // Past the right edge, past the bottom, fully outside, empty.
        let n = copy_rgba_rects_into(
            &mut dst,
            pitch,
            &src,
            w * 4,
            w,
            h,
            &[(4, 3, 50, 50), (99, 0, 3, 3), (0, 0, 0, 4)],
            false,
        );
        assert_eq!(n, 2 * 2 * 4, "only the 2x2 corner inside the frame is copied");
        // A destination shorter than the frame (a configure race) is clipped
        // by its own size.
        let mut short = vec![0u8; pitch * 2];
        let n = copy_rgba_rects_into(&mut short, pitch, &src, w * 4, w, h, &[(0, 0, 6, 5)], false);
        assert_eq!(n, 6 * 2 * 4);
    }

    /// WAYLAND8: a native target whose rows are PADDED - a Wayland `wl_shm`
    /// slot whose pitch is rounded up to 256 bytes so the compositor's GPU can
    /// sample it in place - is a pixmap `frame width + padding` pixels wide
    /// (`AzulPixmap`'s width is its pitch). `render_frame` must draw into it
    /// instead of refusing it as a size mismatch, and the first `frame width`
    /// pixels of every row must equal the owned render, on the full first
    /// frame and on every incremental frame after it.
    #[test]
    fn a_row_padded_native_target_holds_the_owned_frame_in_every_row() {
        #[derive(Debug, Clone)]
        struct PadState {
            variant: usize,
        }

        extern "C" fn layout_pad(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            use azul_css::{
                dynamic_selector::CssPropertyWithConditions as P,
                props::{
                    basic::color::ColorU,
                    layout::dimensions::{LayoutHeight, LayoutWidth},
                    property::CssProperty,
                    style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
                },
            };

            let v = data
                .downcast_ref::<PadState>()
                .map(|s| s.variant)
                .unwrap_or(0);
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
                r: 30,
                g: 120,
                b: 200,
                a: 255,
            })]
            .into();
            let (w, h) = match v % 3 {
                0 => (160.0, 80.0),
                1 => (60.0, 30.0),
                _ => (210.0, 110.0),
            };
            let div = Dom::create_div()
                .with_css_props(
                    vec![
                        P::simple(CssProperty::width(LayoutWidth::px(w))),
                        P::simple(CssProperty::height(LayoutHeight::px(h))),
                        P::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "padded rows",
                ));
            Dom::create_body().with_child(div)
        }

        // Any padding works; 13 px is deliberately not a "nice" number.
        const PAD: u32 = 13;

        let state = Arc::new(RefCell::new(RefAny::new(PadState { variant: 0 })));
        let mut nat = make_window_with(&state, layout_pad);
        let mut own = make_window_with(&state, layout_pad);
        nat.regenerate_layout().expect("nat initial");
        own.regenerate_layout().expect("own initial");

        // The "slot": frame 1 copied row by row into a buffer whose rows are
        // PAD pixels longer than the frame's.
        let seed = nat
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("frame 1")
            .clone_pixmap();
        let (pw, ph) = (seed.width(), seed.height());
        let pitch = pw + PAD;
        let (row, prow) = (pw as usize * 4, pitch as usize * 4);
        let mut slot = vec![0u8; prow * ph as usize];
        for y in 0..ph as usize {
            slot[y * prow..y * prow + row].copy_from_slice(&seed.data()[y * row..(y + 1) * row]);
        }
        drop(seed);

        let mut saw_incremental = false;
        for step in 1..6usize {
            if let Ok(mut b) = state.try_borrow_mut() {
                if let Some(mut s) = b.downcast_mut::<PadState>() {
                    s.variant = step;
                }
            }
            nat.cpu_backend.native_target_holds_previous_frame = step > 1;
            nat.cpu_backend.native_target_row_padding_px = PAD;
            nat.cpu_backend.native_target = unsafe {
                azul_layout::cpurender::AzulPixmap::from_external(slot.as_mut_ptr(), pitch, ph)
            };
            nat.regenerate_layout().expect("native padded");
            assert!(
                nat.cpu_backend.rendered_native,
                "step {step}: the padded target was refused (owned-path fallback)"
            );
            assert!(
                nat.cpu_backend.native_target.is_none(),
                "step {step}: target not consumed"
            );
            let native_damage = nat.cpu_backend.last_frame_damage.clone();
            if matches!(native_damage, FrameDamage::Rects(_)) {
                saw_incremental = true;
            }

            own.regenerate_layout().expect("owned");
            let reference = own
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("owned frame")
                .clone_pixmap();
            assert_eq!((reference.width(), reference.height()), (pw, ph));
            let b = reference.data();
            let mut diffs = 0usize;
            let mut first: Option<(usize, usize)> = None;
            for y in 0..ph as usize {
                for x in 0..pw as usize {
                    let s = y * prow + x * 4;
                    let r = y * row + x * 4;
                    if slot[s..s + 3] != b[r..r + 3] {
                        diffs += 1;
                        if first.is_none() {
                            first = Some((x, y));
                        }
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step {step}: the padded slot diverges from the owned render at {diffs} px, \
                 first {first:?} - native damage {native_damage:?}"
            );
        }
        assert!(
            saw_incremental,
            "every step took the full-repaint path - the padded incremental law was never \
             exercised"
        );
    }

    /// #28: `LayoutWindow::query_pagination` — the speculative query must
    /// (a) SHARE the window's shaped-text entries by pointer (fork law:
    /// cloning the cache maps bumps Arcs, copies no shaped data — the
    /// memory/speed claim), and (b) agree EXACTLY with the ground-truth
    /// fresh-cache pagination the app side computes today (the correctness
    /// claim: forked caches change nothing about the result).
    #[test]
    fn query_pagination_matches_fresh_and_shares_shaping() {
        fn doc_dom() -> Dom {
            let mut body = Dom::create_body();
            for i in 0..14usize {
                body = body.with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    match i % 3 {
                        0 => "alpha beta gamma delta epsilon zeta eta theta iota kappa",
                        1 => "the quick brown fox jumps over the lazy dog again and again",
                        _ => "lorem ipsum dolor sit amet consectetur adipiscing elit sed do",
                    },
                ));
            }
            body
        }
        extern "C" fn layout_doc(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
            doc_dom()
        }

        let state = Arc::new(RefCell::new(RefAny::new(0usize)));
        let mut window = make_window_with(&state, layout_doc);
        window.regenerate_layout().expect("layout");
        let lw = window.common.layout_window.as_ref().expect("layout window");

        // (a) fork law: every shaped entry is shared, not copied.
        let keys = lw.text_cache.per_item_keys();
        assert!(
            !keys.is_empty(),
            "screen layout shaped nothing — fixture broken"
        );
        let fork = lw.text_cache.fork_shared();
        for k in &keys {
            assert!(
                lw.text_cache.per_item_entry_ptr_eq(&fork, *k),
                "shaped entry {k} was copied, not shared"
            );
        }

        // (b) equivalence with the fresh-cache ground truth.
        use azul_layout::solver3::pagination::FakePageConfig;
        let page_size = azul_core::geom::LogicalSize::new(320.0, 180.0);
        let img_resources = azul_core::resources::ImageCache::default();

        let doc = azul_core::styled_dom::StyledDom::create_from_dom(doc_dom());
        let q = lw
            .query_pagination(&doc, page_size, FakePageConfig::new(), &img_resources)
            .expect("query_pagination");

        let doc2 = azul_core::styled_dom::StyledDom::create_from_dom(doc_dom());
        let mut oracle_cache = azul_layout::Solver3LayoutCache::default();
        let mut oracle_text = azul_layout::TextLayoutCache::new();
        let mut fm = lw.font_manager.clone_shared();
        let loader = azul_layout::text3::default::PathLoader::new();
        let font_loader = |bytes, index| loader.load_font_shared(bytes, index);
        let o = azul_layout::solver3::paged_layout::compute_document_pagination(
            &mut oracle_cache,
            &mut oracle_text,
            azul_layout::paged::FragmentationContext::new_paged(page_size),
            &doc2,
            azul_core::geom::LogicalRect {
                origin: azul_core::geom::LogicalPosition::zero(),
                size: page_size,
            },
            &mut fm,
            &std::collections::BTreeMap::new(),
            &mut None,
            None,
            &azul_core::resources::RendererResources::default(),
            azul_core::resources::IdNamespace(0),
            azul_core::dom::DomId::ROOT_ID,
            font_loader,
            FakePageConfig::new(),
            &img_resources,
            azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        )
        .expect("oracle pagination");

        assert!(
            o.page_count >= 2,
            "fixture fits one page (page_count {}) — the law never bites",
            o.page_count
        );
        assert_eq!(q.page_count, o.page_count, "page counts diverge");
        assert_eq!(q.breaks, o.breaks, "break positions diverge");
        assert_eq!(
            q.total_content_height.to_bits(),
            o.total_content_height.to_bits(),
            "content height diverges ({} vs {})",
            q.total_content_height,
            o.total_content_height
        );
    }

    /// Cache GC must run on IDLE frames and not while the user is typing.
    ///
    /// `GlyphCache::gc()` frees the previous generation — thousands of
    /// vectors. Two ways to get this wrong, and this pins both:
    ///
    ///  * never calling it: the cold generation is held until a rotation overwrites it, so memory
    ///    is retained long after the text changed.
    ///  * calling it every present: `prev` never survives long enough for a rotated-out glyph to be
    ///    promoted back, collapsing the two generations into one and reintroducing the rebuild
    ///    storm the generational scheme exists to prevent.
    #[test]
    fn glyph_cache_gc_runs_on_idle_frames_only() {
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "alpha beta gamma".to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");

        // A frame that CHANGES something must not GC — the caches have to
        // stay warm across an edit.
        set_label(&state, "delta epsilon zeta");
        window.regenerate_layout().expect("changed frame");
        let after_change = window.cpu_backend.glyph_cache.paths_len();
        assert!(
            after_change > 0,
            "the changed frame must have populated the glyph path cache, or this test cannot tell \
             GC from an empty cache"
        );
        assert_ne!(
            window.cpu_backend.last_frame_damage,
            FrameDamage::None,
            "a text change must damage something; if it does not, the idle branch below is being \
             reached for the wrong reason"
        );

        // Force a rotation so there IS a previous generation to collect.
        // Without this the idle frame has nothing to free and the assertion
        // below would pass whether or not GC is wired up — which is exactly
        // how the first version of this test managed to be worthless.
        for i in 0..(8192u64 + 1) {
            let _ = window
                .cpu_backend
                .glyph_cache
                .get_or_build_cells(i, 0, 16, 0.0, 0.0, 1.0, false, 1.0);
        }
        assert!(
            window.cpu_backend.glyph_cache.prev_generation_len() > 0,
            "the fixture failed to rotate a generation, so the GC assertion below would be vacuous"
        );

        // Now an IDLE frame: same content, nothing to paint.
        window.regenerate_layout().expect("idle frame");
        assert_eq!(
            window.cpu_backend.glyph_cache.prev_generation_len(),
            0,
            "an idle frame must collect the previous generation — that is the whole point of \
             doing GC here rather than on a keystroke"
        );
        assert_eq!(
            window.cpu_backend.last_frame_damage,
            FrameDamage::None,
            "an unchanged relayout must be idle, or the GC hook never runs"
        );
        // GC only frees the PREVIOUS generation, so the live entries stay.
        // What must hold is that the cache is still usable and bounded.
        let after_idle = window.cpu_backend.glyph_cache.paths_len();
        assert!(
            after_idle <= after_change,
            "an idle frame must not GROW the glyph cache (was {after_change}, now {after_idle})"
        );

        // And the cache must still serve: another edit reuses it rather than
        // starting from nothing.
        set_label(&state, "delta epsilon zeta eta");
        window.regenerate_layout().expect("second edit");
        assert!(
            window.cpu_backend.glyph_cache.paths_len() > 0,
            "the glyph cache must survive an idle-frame GC — if it is empty here, GC threw away \
             the LIVE generation, not the cold one"
        );
    }

    /// STRESS the damage diff with a long, varied edit sequence.
    ///
    /// `tight_text_damage_leaves_no_stale_pixels` proves one scripted edit
    /// is safe. That is not enough to trust incremental painting: the
    /// failure mode is a damage rect that misses something, and whether it
    /// does depends on the SHAPE of the edit — append, insert in the middle,
    /// delete, replace, shrink to empty, grow across a line break. Each
    /// moves a different set of glyphs.
    ///
    /// This runs 40 such edits from a fixed seed and, after every one,
    /// compares the incrementally-painted frame against a full repaint of
    /// the same content. Any rect the diff failed to emit shows up as a
    /// differing pixel at the step that caused it, so a regression names
    /// its own repro.
    ///
    /// This is the guard the display-list PATCHING work needs before it
    /// lands: patching items in place instead of regenerating them can
    /// desync the list from the layout, and a desync is invisible to every
    /// timing test but shows up here immediately.
    #[test]
    fn damage_survives_a_randomised_edit_sequence() {
        const WORDS: [&str; 6] = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta"];

        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "alpha beta gamma".to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Deterministic LCG — a fixed seed means a failure reproduces
        // exactly, and there is no rand dependency in this crate.
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as usize
        };

        let mut text = String::from("alpha beta gamma");
        for step in 0..40 {
            match next() % 5 {
                0 => text.push_str(WORDS[next() % WORDS.len()]), // append
                1 => {
                    // insert in the middle
                    let at = if text.is_empty() {
                        0
                    } else {
                        next() % text.len()
                    };
                    let at = (0..=at)
                        .rev()
                        .find(|i| text.is_char_boundary(*i))
                        .unwrap_or(0);
                    text.insert_str(at, WORDS[next() % WORDS.len()]);
                }
                2 => {
                    // delete a chunk
                    if !text.is_empty() {
                        let a = next() % text.len();
                        let b = (a + 1 + next() % 5).min(text.len());
                        let a = (0..=a)
                            .rev()
                            .find(|i| text.is_char_boundary(*i))
                            .unwrap_or(0);
                        let b = (b..=text.len())
                            .find(|i| text.is_char_boundary(*i))
                            .unwrap_or(text.len());
                        text.replace_range(a..b, "");
                    }
                }
                3 => text = WORDS[next() % WORDS.len()].to_string(), // replace all
                _ => text.push(' '),                                 // whitespace only
            }

            set_label(&state, &text);
            window.regenerate_layout().expect("incremental relayout");
            let incremental = window
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("frame")
                .clone_pixmap();
            let damage = window.cpu_backend.last_frame_damage.clone();

            // Ground truth: the same content painted from scratch.
            let mut fresh = make_harness_window(&state);
            fresh.regenerate_layout().expect("fresh layout");
            let full = fresh
                .cpu_backend
                .last_frame
                .as_ref()
                .expect("fresh")
                .clone_pixmap();

            let (a, b) = (incremental.data(), full.data());
            let mut diffs = 0usize;
            let mut first: Option<(u32, u32)> = None;
            let w = incremental.width();
            for i in (0..a.len().min(b.len())).step_by(4) {
                if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                    diffs += 1;
                    if first.is_none() {
                        let px = (i / 4) as u32;
                        first = Some((px % w, px / w));
                    }
                }
            }
            assert_eq!(
                diffs, 0,
                "step {step} (text = {text:?}) left {diffs} stale pixels, first at {first:?}. The \
                 damage rects did not cover everything that changed. damage = {damage:?}"
            );
        }
    }

    /// MEASUREMENT: how tight is the damage for a one-character edit?
    ///
    /// The interactive budget is spent here. A `DisplayListItem::Text`
    /// reports its `clip_rect` as its bounds (`display_list.rs` `bounds()`),
    /// and the emission site sets that to the node's whole viewport-sized
    /// content box — so damage for a text change can never be finer than the
    /// enclosing text node, no matter how little of it changed.
    ///
    /// This prints the ratio for a paragraph wide enough to wrap to several
    /// lines, and pins the CURRENT behaviour so the next change to
    /// glyph-run splitting or `visual_bounds()` shows up as a number rather
    /// than a surprise. It deliberately does NOT assert a tight bound yet —
    /// asserting one now would just encode the defect as expected.
    /// NEGATIVE CONTROL for the tightened text damage bounds.
    ///
    /// `DisplayListItem::Text::visual_bounds()` now reports the glyph ink
    /// extent instead of the node's whole clip box, and glyph runs break at
    /// line boundaries. Both make damage rects SMALLER, and a damage rect
    /// that is too small leaves stale pixels on screen — a worse defect
    /// than the coarseness it replaces, and one no timing test can see.
    ///
    /// This edits text and then compares the incrementally-painted frame
    /// against a FULL repaint of the same content, pixel for pixel. Any
    /// region the damage rects failed to cover shows up as a difference.
    #[test]
    fn tight_text_damage_leaves_no_stale_pixels() {
        let long = "the quick brown fox jumps over the lazy dog and keeps on running past the end \
                    of the first line and onto a second";
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: long.to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Edit, rendered INCREMENTALLY through the damage path.
        set_label(&state, &format!("{long} tail"));
        window.regenerate_layout().expect("incremental relayout");
        let incremental = window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("frame")
            .clone_pixmap();
        let damage = window.cpu_backend.last_frame_damage.clone();

        // Same content, but painted from scratch: drop the retained frame so
        // the next render cannot reuse anything.
        let mut fresh_window = make_harness_window(&state);
        fresh_window.regenerate_layout().expect("fresh layout");
        let full = fresh_window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("fresh frame")
            .clone_pixmap();

        assert_eq!(incremental.width(), full.width());
        assert_eq!(incremental.height(), full.height());

        let (a, b) = (incremental.data(), full.data());
        let mut diffs = 0usize;
        let mut first: Option<(u32, u32)> = None;
        let w = incremental.width();
        for i in (0..a.len().min(b.len())).step_by(4) {
            // Anti-aliasing is deterministic here (same glyphs, same
            // positions, same cache), so require an exact match on RGB.
            if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                diffs += 1;
                if first.is_none() {
                    let px = (i / 4) as u32;
                    first = Some((px % w, px / w));
                }
            }
        }
        println!(
            "[harness] stale-pixel check: {diffs} differing px, first at {first:?}, damage = \
             {damage:?}"
        );
        assert_eq!(
            diffs, 0,
            "the incrementally-painted frame differs from a full repaint of the same content in \
             {diffs} pixels (first at {first:?}). The damage rects did not cover everything that \
             changed, so those pixels are STALE on a real screen. damage = {damage:?}"
        );
    }

    #[test]
    fn damage_one_char_edit_reports_its_granularity() {
        let long = "the quick brown fox jumps over the lazy dog and keeps on                     \
                    running past the end of the first line and onto a second";
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: long.to_string(),
        })));
        let mut window = make_harness_window(&state);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // Append ONE character. Everything before it is untouched.
        set_label(&state, &format!("{long}X"));
        window.regenerate_layout().expect("relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!(
            "[harness] glyph runs after edit = {:?}",
            text_glyph_counts(&window)
        );

        let damaged_area: f32 = match &damage {
            FrameDamage::Full => 400.0 * 300.0,
            FrameDamage::None => 0.0,
            FrameDamage::Rects(rs) => rs.iter().map(|r| r.size.width * r.size.height).sum(),
        };
        // The window is 400x300; the text node spans most of its width.
        let window_area = 400.0 * 300.0;
        println!(
            "[harness] one-char edit damage = {damage:?}\n[harness]   damaged {damaged_area:.0} \
             px2 of {window_area:.0} px2 window              = {:.1}%",
            damaged_area / window_area * 100.0
        );

        assert!(
            damaged_area > 0.0,
            "a text change must damage SOMETHING, or the edit never reaches the screen"
        );
        assert!(
            !matches!(damage, FrameDamage::Full),
            "a one-character edit must not escalate to a FULL-window repaint — that is the worst \
             case and means damage tracking gave up entirely"
        );
    }

    #[test]
    fn damage_text_change_repro() {
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "AAA".to_string(),
        })));
        let mut window = make_harness_window(&state);

        // Initial layout — establishes the baseline display list.
        window.regenerate_layout().expect("initial layout");
        let before = text_glyph_counts(&window);
        println!(
            "[harness] initial   : text_glyph_counts={:?} damage={:?}",
            before, window.cpu_backend.last_frame_damage
        );

        // Pure text-content change: "AAA" (3) -> "BBBBBBBB" (8).
        set_label(&state, "BBBBBBBB");
        window.regenerate_layout().expect("relayout after change");
        let after = text_glyph_counts(&window);
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!(
            "[harness] post-change: text_glyph_counts={:?} damage={:?}",
            after, damage
        );

        // Baseline sanity: text shaped at all.
        assert_eq!(
            before,
            vec![3],
            "baseline: expected an initial 3-glyph run (\"AAA\"), got {:?} (no fonts? text not \
             shaping?)",
            before
        );

        // HONEST ASSERTION — reproduces the stale-text bug (#11). The display
        // list MUST reflect the new 8-char label. It currently stays at 3
        // glyphs ("AAA"), so this FAILS until #11 is fixed (do NOT weaken it).
        assert_eq!(
            after,
            vec![8],
            "STALE-TEXT BUG (#11): after changing the label to \"BBBBBBBB\" (8 chars) the display \
             list should contain an 8-glyph text run, but it still has {:?} — the text change \
             never reached the display list. Damage was {:?}, so the diff/regen ran but produced \
             STALE content (display-list generation bug, not a damage bug).",
            after,
            damage
        );
    }

    /// App-side state observed by the create_callback parity test.
    #[derive(Debug, Clone)]
    struct CreateState {
        fired: usize,
    }

    extern "C" fn create_cb_installs_timer(
        mut data: RefAny,
        mut info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use azul_core::callbacks::Update;
        if let Some(mut s) = data.downcast_mut::<CreateState>() {
            s.fired += 1;
        }
        // The miniword symptom: the startup timer installed here never
        // ticked because the callback never ran under AZ_BACKEND=headless.
        info.add_timer(
            azul_core::task::TimerId { id: 777 },
            azul_layout::timer::Timer::default(),
        );
        Update::DoNothing
    }

    #[test]
    fn create_callback_fires_once_with_app_data_and_installs_timers() {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        let state = Arc::new(RefCell::new(RefAny::new(CreateState { fired: 0 })));

        let fc_cache = Arc::new(FcFontCache::default());
        let icon_provider = SharedIconProvider::from_handle(IconProviderHandle::default());
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb: harness_layout,
            ctx: OptionRefAny::None,
        };
        opts.window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
        opts.create_callback =
            azul_layout::callbacks::OptionCallback::Some(azul_layout::callbacks::Callback {
                cb: create_cb_installs_timer,
                ctx: OptionRefAny::None,
            });
        let mut window = HeadlessWindow::new(
            opts,
            state.clone(),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            icon_provider,
            fc_cache,
            None,
        )
        .unwrap();
        inject_harness_font(&window);

        // Negative control: nothing fired at construction time.
        assert!(
            !window.has_active_timers(),
            "no timer may exist before invoke_create_callback"
        );
        assert_eq!(
            state
                .borrow_mut()
                .downcast_ref::<CreateState>()
                .unwrap()
                .fired,
            0
        );

        window.invoke_create_callback();

        assert_eq!(
            state
                .borrow_mut()
                .downcast_ref::<CreateState>()
                .unwrap()
                .fired,
            1,
            "create_callback must run with the APP data (parity with OS shells)"
        );
        assert!(
            window.has_active_timers(),
            "the timer installed by create_callback must land in the layout window (AddTimer \
             change applied)"
        );

        // Exactly once per window lifetime — a second lifecycle pass must
        // not re-fire it (OS shells consume the callback at window creation).
        window.invoke_create_callback();
        assert_eq!(
            state
                .borrow_mut()
                .downcast_ref::<CreateState>()
                .unwrap()
                .fired,
            1,
            "create_callback fired more than once"
        );
    }

    #[test]
    fn damage_noop_relayout_is_clean() {
        let state = Arc::new(RefCell::new(RefAny::new(UiState {
            label: "Hello world".to_string(),
        })));
        let mut window = make_harness_window(&state);

        window.regenerate_layout().expect("initial layout");
        // Relayout AGAIN with the SAME state — nothing changed at all.
        window.regenerate_layout().expect("no-op relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] no-op relayout: damage={:?}", damage);

        // HONEST ASSERTION: relaying out an unchanged DOM must produce NO
        // damage. Anything else is a false-positive (e.g. text re-shaping to
        // glyphs at sub-pixel-different positions each pass), which makes the
        // incremental path repaint the whole frame every time.
        assert_eq!(
            damage,
            FrameDamage::None,
            "NO-OP relayout produced {:?} — an unchanged DOM must yield FrameDamage::None; \
             false-positive damage every frame defeats incremental rendering.",
            damage
        );
    }

    #[test]
    fn damage_box_paint_change_is_local() {
        let state = Arc::new(RefCell::new(RefAny::new(BoxState { red: false })));
        let mut window = make_window_with(&state, harness_layout_box);
        window.regenerate_layout().expect("initial layout");

        // Recolor the 100x50 box blue -> red. Pure paint change, no reflow.
        set_box_red(&state, true);
        window.regenerate_layout().expect("recolor");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] box recolor: damage={:?}", damage);

        // HONEST: recoloring a 100x50 box must damage roughly the box, NOT the
        // whole 400x300 window. This isolates the damage system from text
        // generation — if THIS passes, the damage machinery is sound and the
        // earlier failures are text-specific.
        let window_area = 400.0 * 300.0;
        match damage_area(&damage) {
            Some(a) if a > 0.0 => assert!(
                a < window_area * 0.5,
                "box recolor damage area {} should be ~box-sized (~5000), not near-full-window {} \
                 — damage={:?}",
                a,
                window_area,
                damage
            ),
            other => panic!(
                "box recolor should produce bounded incremental damage, got area={:?} damage={:?}",
                other, damage
            ),
        }
    }

    #[test]
    fn damage_box_noop_clean() {
        let state = Arc::new(RefCell::new(RefAny::new(BoxState { red: false })));
        let mut window = make_window_with(&state, harness_layout_box);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("no-op relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] box no-op: damage={:?}", damage);

        // HONEST + diagnostic: a static colored box (no text) relaid out with
        // no change must be FrameDamage::None. If this is None but the TEXT
        // no-op test reports damage, the false-positive is text-shaping
        // specific (non-deterministic glyphs); if this also reports damage,
        // the false-positive is general.
        assert_eq!(
            damage,
            FrameDamage::None,
            "no-op relayout of a static box must be FrameDamage::None, got {:?}",
            damage
        );
    }

    #[test]
    fn perf_noop_relayout_under_budget() {
        let state = Arc::new(RefCell::new(RefAny::new(BoxState { red: false })));
        let mut window = make_window_with(&state, harness_layout_box);
        window.regenerate_layout().expect("initial layout");

        // Measure in BATCHES and judge the FASTEST one: a wall-clock average
        // under a parallel test run measures the scheduler, not the layout
        // engine (this test flaked at ~3ms/relayout with the suite saturating
        // every core while the isolated cost was a steady ~1ms). The batch
        // MINIMUM is load-immune — a real caching regression raises the
        // minimum too, so the budget still bites.
        let batches: u32 = 5;
        let per_batch: u32 = 40;
        let mut best = std::time::Duration::MAX;
        for _ in 0..batches {
            let start = std::time::Instant::now();
            for _ in 0..per_batch {
                window.regenerate_layout().expect("no-op relayout");
            }
            best = best.min(start.elapsed() / per_batch);
        }
        println!(
            "[perf] {} x {} no-op relayouts: fastest batch per={:?}",
            batches, per_batch, best
        );

        // PERF BUDGET: a no-op relayout of a trivial DOM should be cheap
        // (cache hits, no re-render). If nothing caches and every frame fully
        // re-lays-out + re-renders, this blows past it. A slow UI — especially
        // scrolling at this cost per frame — is unusable.
        //
        // The budget is per-PROFILE because the same work costs ~20x more
        // unoptimized: measured on this suite, an optimized build does a no-op
        // relayout in ~117us, while the unoptimized build CI runs (`cd dll &&
        // cargo test`, dev profile, shared runner) takes ~2.1-2.4ms. A single
        // 2ms budget was therefore calibrated for release and could never pass
        // in CI's dev profile — it failed every run, which is worse than no
        // guard: a red test nobody can green teaches the team to ignore it.
        //
        // Both budgets still bite. The failure mode this guards is an
        // ORDER-OF-MAGNITUDE regression (caching gone → full relayout + render
        // every frame), not a few percent: release has ~17x headroom over the
        // healthy number, debug ~8x.
        let budget = if cfg!(debug_assertions) {
            std::time::Duration::from_millis(20)
        } else {
            std::time::Duration::from_millis(2)
        };
        assert!(
            best < budget,
            "no-op relayout too slow: {:?}/relayout (budget {:?} for this profile, best of {} \
             batches) — incremental caching is not working; this is unusable for scrolling",
            best,
            budget,
            batches
        );
    }

    // --- Reflow / structural tests via a stacked grid of colored boxes ---

    /// A vertical stack of colored boxes; each entry is (width, height). Lets
    /// tests drive size reflow, sibling shifts, and structural add/remove with
    /// one layout callback.
    #[derive(Debug, Clone)]
    struct GridState {
        boxes: Vec<(f32, f32)>,
        /// Index of a box to paint a distinct colour (for local-paint tests).
        highlight: Option<usize>,
    }

    extern "C" fn harness_layout_grid(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let (boxes, highlight) = data
            .downcast_ref::<GridState>()
            .map(|s| (s.boxes.clone(), s.highlight))
            .unwrap_or_default();
        let mut body = Dom::create_body();
        for (i, (w, h)) in boxes.iter().enumerate() {
            let color = if Some(i) == highlight {
                ColorU {
                    r: 30,
                    g: 220,
                    b: 30,
                    a: 255,
                } // highlighted box
            } else if i % 2 == 0 {
                ColorU {
                    r: 220,
                    g: 30,
                    b: 30,
                    a: 255,
                }
            } else {
                ColorU {
                    r: 30,
                    g: 30,
                    b: 220,
                    a: 255,
                }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            body = body.with_child(
                Dom::create_div().with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(*w))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            *h,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                ),
            );
        }
        body
    }

    fn set_grid(state: &Arc<RefCell<RefAny>>, boxes: Vec<(f32, f32)>) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<GridState>();
        if let Some(s) = opt.as_mut() {
            s.boxes = boxes;
        }
    }

    fn set_highlight(state: &Arc<RefCell<RefAny>>, highlight: Option<usize>) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<GridState>();
        if let Some(s) = opt.as_mut() {
            s.highlight = highlight;
        }
    }

    /// Max bottom-edge Y across the damage (Full = +inf, None = 0).
    fn damage_max_y(d: &FrameDamage) -> f32 {
        match d {
            FrameDamage::None => 0.0,
            FrameDamage::Full => f32::INFINITY,
            FrameDamage::Rects(rs) => rs
                .iter()
                .map(|r| r.origin.y + r.size.height)
                .fold(0.0f32, f32::max),
        }
    }

    #[test]
    fn damage_box_size_reflow() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Widen the box 100 -> 200 (same height). Pure size reflow.
        set_grid(&state, vec![(200.0, 50.0)]);
        window.regenerate_layout().expect("reflow");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] size reflow: damage={:?}", damage);

        // HONEST: widening must damage the box region (old∪new ⊇ the 200x50
        // box), bounded — not the whole 400x300 window, not empty.
        let window_area = 400.0 * 300.0;
        match damage_area(&damage) {
            Some(a) if a > 0.0 => assert!(
                a < window_area * 0.5,
                "size reflow damage area {} should be box-sized (~10000), not near-full-window {} \
                 — damage={:?}",
                a,
                window_area,
                damage
            ),
            other => panic!(
                "size reflow should produce bounded incremental damage, got area={:?} damage={:?}",
                other, damage
            ),
        }
    }

    /// A window that gets narrower or shorter keeps its pixels: a shrink
    /// exposes nothing new, so the frame repaints what the relayout changed,
    /// and a mixed resize (wider but shorter) repaints that plus the strip it
    /// gained - each frame equal to a full repaint. Every shrinking step of a
    /// window-edge drag used to repaint the whole window (48 ms of raster at
    /// 1x for AzDrive's window).
    #[test]
    fn a_window_that_shrinks_repaints_only_what_changed() {
        use azul_core::geom::LogicalSize;
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0), (100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_sized(&state, harness_layout_grid, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        for (w, h) in [(380.0f32, 300.0f32), (380.0, 280.0), (395.0, 260.0)] {
            window
                .common
                .update_window_state(event::WindowStateSource::Os, |ws| {
                    ws.size.dimensions = LogicalSize::new(w, h);
                });
            window.regenerate_layout().expect("relayout at the new size");
            let damage = window.cpu_backend.last_frame_damage.clone();
            assert!(
                !damage.is_full(),
                "a resize to {w}x{h} that exposes at most a strip repainted the whole window"
            );
            let (diffs, first) = incremental_vs_full(&mut window);
            assert_eq!(
                diffs, 0,
                "the frame at {w}x{h} differs from a full repaint in {diffs} px, first at {first:?} \
                 (damage {damage:?})"
            );
        }
    }

    #[test]
    fn damage_reflow_shifts_sibling() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0), (100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Grow box1's height 50 -> 100. box2 (below it) shifts DOWN by 50.
        set_grid(&state, vec![(100.0, 100.0), (100.0, 50.0)]);
        window.regenerate_layout().expect("reflow");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] sibling shift: damage={:?}", damage);

        // HONEST: box2 moved from y≈58..108 to y≈108..158. What reaches the
        // screen MUST reach box2's new bottom (~158) — otherwise box2 leaves a
        // ghost at its old position / never shows at its new one. Its pixels
        // may be MOVED there (the layout move is presented as a blit: the
        // PRESENT damage covers it, the paint damage only the strips), so the
        // frame itself must also equal a full repaint.
        let present = window.cpu_backend.last_present_damage.clone();
        let max_y = damage_max_y(&present);
        assert!(
            max_y >= 140.0,
            "reflow-shift present damage must reach the shifted sibling (bottom ~158), got \
             max_y={} present={:?} paint={:?} — box2 would ghost/not show",
            max_y,
            present,
            damage
        );
        let (diffs, first) = incremental_vs_full(&mut window);
        assert_eq!(
            diffs, 0,
            "the reflow frame differs from a full repaint in {diffs} px, first at {first:?} \
             (paint {damage:?}, present {present:?})"
        );
    }

    #[test]
    fn damage_structural_add_covers_new_node() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 50.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Add a second box below the first (structural change).
        set_grid(&state, vec![(100.0, 50.0), (100.0, 50.0)]);
        window.regenerate_layout().expect("add box");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] structural add: damage={:?}", damage);

        // HONEST: a structural change (item count differs) can't be diffed
        // item-by-item, so a conservative FULL repaint is correct (precise
        // layout-level damage is a #10 goal). Either Full, or rects that at
        // least reach the new box (~y 58..108). NOT None — the new box must
        // paint.
        match &damage {
            FrameDamage::Full => {}
            FrameDamage::Rects(_) => {
                let max_y = damage_max_y(&damage);
                assert!(
                    max_y >= 90.0,
                    "structural add must damage the new box (~y 108), got max_y={} damage={:?}",
                    max_y,
                    damage
                );
            }
            FrameDamage::None => {
                panic!("structural add produced NO damage — the new box would never paint")
            }
        }
    }

    // --- Event-driven harness: drive a HeadlessEvent through the same per-event
    // path run() uses, relayout if it requested a redraw, and return the damage
    // produced this step (None if the event caused no visual change). ---
    // --- One physical release = ONE Hover(MouseUp) invocation ------------
    //
    // The regression this pins: `process_window_events` derives events from
    // the previous->current window-state diff but never CONSUMED the delta at
    // the end of a pass. Any later pass with no state change of its own (a
    // redraw tick, a wait_frame pump, the regeneration pass after RefreshDom)
    // re-detected the SAME down->up transition and re-invoked the callback -
    // every toggle widget self-cancelled (the ribbon gallery "More" panel
    // could never open). The wrapper now advances previous_window_state after
    // every completed pass.

    #[derive(Debug, Clone)]
    struct ClickCounterState {
        hits: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn count_mouse_up(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(s) = refany.downcast_ref::<ClickCounterState>() {
            s.hits.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn click_counter_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            events::{EventFilter, HoverEventFilter},
        };
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
            },
        };

        let hits = data
            .downcast_ref::<ClickCounterState>()
            .map(|s| s.hits.clone())
            .unwrap_or_default();
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(
                            400.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            300.0,
                        ))),
                    ]
                    .into(),
                )
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::MouseUp),
                        callback: CoreCallback {
                            cb: count_mouse_up as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(ClickCounterState { hits }),
                    }]
                    .into(),
                ),
        )
    }

    #[test]
    fn a_mouse_release_invokes_its_hover_callback_exactly_once() {
        use azul_core::events::MouseButton;

        let hits = Arc::new(core::sync::atomic::AtomicUsize::new(0));
        let state = Arc::new(RefCell::new(RefAny::new(ClickCounterState {
            hits: hits.clone(),
        })));
        let mut window = make_window_with(&state, click_counter_layout);
        window.regenerate_layout().expect("initial layout");

        step(&mut window, HeadlessEvent::MouseMove { x: 200.0, y: 150.0 });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        assert_eq!(
            hits.load(core::sync::atomic::Ordering::SeqCst),
            1,
            "one release fires the Hover(MouseUp) callback once"
        );

        // The regression trigger: run extra passes WITHOUT any state change -
        // exactly what a redraw tick or a wait_frame pump does. The consumed
        // delta must not re-fire the callback.
        for _ in 0..3 {
            use crate::desktop::shell2::common::event::PlatformWindow;
            let _ = window.process_window_events(0);
        }
        assert_eq!(
            hits.load(core::sync::atomic::Ordering::SeqCst),
            1,
            "a pass without a state change must not re-dispatch the release"
        );

        // A genuine second click still works (the delta is consumed, not lost).
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        assert_eq!(
            hits.load(core::sync::atomic::Ordering::SeqCst),
            2,
            "the second click fires exactly once more"
        );
    }

    // --- Slider drag -----------------------------------------------------
    //
    // REPORTED: "dragging the slider leaves old thumbs on the track". The
    // widget slides its thumb with `set_css_property(thumb, margin-left)` on
    // every pointer move, and the AzWidgets demo's `on_value_change` bumps a
    // RENDERED interactions counter and returns `RefreshDom` — so each move
    // is an in-place relayout followed by a full DOM regeneration, through
    // the same `headless::CpuBackend` every desktop shell presents with.

    #[derive(Debug, Clone)]
    struct SliderUiState {
        slider_value: f32,
        interactions: usize,
    }

    extern "C" fn slider_demo_on_change(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
        _state: azul_layout::widgets::slider::SliderState,
    ) -> azul_core::callbacks::Update {
        if let Some(mut s) = data.downcast_mut::<SliderUiState>() {
            // Exactly what examples/azul-widgets does: count the callback,
            // do NOT store the value, refresh.
            s.interactions += 1;
        }
        azul_core::callbacks::Update::RefreshDom
    }

    extern "C" fn slider_demo_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::refany::OptionRefAny;
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions as C,
            props::{
                layout::spacing::{
                    LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop,
                },
                property::CssProperty,
            },
        };
        use azul_layout::widgets::slider::{Slider, SliderOnValueChangeCallback};

        let (value, interactions) = data
            .downcast_ref::<SliderUiState>()
            .map(|s| (s.slider_value, s.interactions))
            .unwrap_or((0.0, 0));
        let caption = format!("callbacks fired so far: {interactions}");
        Dom::create_body()
            .with_css_props(
                vec![
                    C::simple(CssProperty::const_padding_top(LayoutPaddingTop::const_px(
                        20,
                    ))),
                    C::simple(CssProperty::const_padding_right(
                        LayoutPaddingRight::const_px(20),
                    )),
                    C::simple(CssProperty::const_padding_bottom(
                        LayoutPaddingBottom::const_px(20),
                    )),
                    C::simple(CssProperty::const_padding_left(
                        LayoutPaddingLeft::const_px(20),
                    )),
                ]
                .into(),
            )
            .with_child(Dom::create_div().with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper(caption.as_str()),
            ))
            .with_child(
                Slider::create(value, 0.0, 100.0)
                    .with_on_value_change(
                        data.clone(),
                        SliderOnValueChangeCallback {
                            cb: slider_demo_on_change,
                            ctx: OptionRefAny::None,
                        },
                    )
                    .dom(),
            )
    }

    /// The slider's live `dragging` flag, read from the callback state on the
    /// CURRENT DOM's track node — i.e. whatever the reconciler left there
    /// after the app's last `RefreshDom`. `None` when no track node exists.
    fn slider_dragging(window: &HeadlessWindow) -> Option<bool> {
        use azul_core::dom::{DomId, IdOrClass};
        use azul_layout::widgets::slider::SliderStateWrapper;

        let lw = window.common.layout_window.as_ref()?;
        let dom = lw.layout_results.get(&DomId { inner: 0 })?;
        for data in dom.styled_dom.node_data.as_container().internal.iter() {
            let is_track = data.get_ids_and_classes().iter().any(|c| match c {
                IdOrClass::Class(s) => s.as_str() == "__azul-native-slider",
                IdOrClass::Id(_) => false,
            });
            if !is_track {
                continue;
            }
            let mut state = data.callbacks.as_ref().first()?.refany.clone();
            let w = state.downcast_ref::<SliderStateWrapper>()?;
            return Some(w.dragging);
        }
        None
    }

    /// A full repaint of the window's CURRENT display list by a fresh backend
    /// (no retained pixels, nothing to blit or skip): what the screen must
    /// show, whatever path presented it.
    fn full_repaint_of(window: &mut HeadlessWindow) -> azul_layout::cpurender::AzulPixmap {
        let ws = window.common.current_window_state();
        let (w, h, dpi) = (
            ws.size.dimensions.width,
            ws.size.dimensions.height,
            ws.size.dpi as f32 / 96.0,
        );
        let mut fresh = CpuBackend {
            follow_system_background: false,
            ..CpuBackend::new()
        };
        let lw = window.common.layout_window.as_ref().expect("layout window");
        fresh.render_frame(lw, &window.common.renderer_resources, w, h, dpi);
        fresh
            .last_frame
            .as_ref()
            .expect("full frame")
            .clone_pixmap()
    }

    /// Pixel-diff the window's INCREMENTALLY presented frame against a full
    /// repaint of the SAME display list by a fresh backend (no retained
    /// pixels, nothing to blit or skip). Returns (differing px, first diff).
    fn incremental_vs_full(window: &mut HeadlessWindow) -> (usize, Option<(u32, u32)>) {
        let incremental = window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("incremental frame")
            .clone_pixmap();
        let full = full_repaint_of(window);
        assert_eq!(incremental.width(), full.width());
        assert_eq!(incremental.height(), full.height());
        let (a, b) = (incremental.data(), full.data());
        let mut diffs = 0usize;
        let mut first: Option<(u32, u32)> = None;
        for i in (0..a.len().min(b.len())).step_by(4) {
            if a[i] != b[i] || a[i + 1] != b[i + 1] || a[i + 2] != b[i + 2] {
                diffs += 1;
                if first.is_none() {
                    let px = (i / 4) as u32;
                    first = Some((px % incremental.width(), px / incremental.width()));
                }
            }
        }
        (diffs, first)
    }

    #[test]
    fn dragging_the_slider_leaves_no_thumb_behind() {
        use azul_core::events::MouseButton;

        let state = Arc::new(RefCell::new(RefAny::new(SliderUiState {
            slider_value: 40.0,
            interactions: 0,
        })));
        let mut window = make_window_sized(&state, slider_demo_layout, 400.0, 160.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let thumb = rects_by_class(&window, "__azul-native-slider-thumb");
        assert_eq!(thumb.len(), 1, "one thumb: {thumb:?}");
        let thumb0 = thumb[0];
        let track = rects_by_class(&window, "__azul-native-slider");
        assert_eq!(track.len(), 1, "one track: {track:?}");
        let track = track[0];
        let y = thumb0.origin.y + thumb0.size.height / 2.0;
        let x0 = thumb0.origin.x + thumb0.size.width / 2.0;
        println!("[slider] track={track:?} thumb={thumb0:?}");

        step(&mut window, HeadlessEvent::MouseMove { x: x0, y });
        let press_damage = step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        println!(
            "[slider] press: dragging={:?} damage={press_damage:?} thumb={:?}",
            slider_dragging(&window),
            rects_by_class(&window, "__azul-native-slider-thumb")
        );
        let (d, f) = incremental_vs_full(&mut window);
        assert_eq!(d, 0, "after the press: {d} stale px, first at {f:?}");

        // A drag: the cursor walks right across the rail in steps, like a
        // real pointer does, and every presented frame must match a full
        // repaint of what layout says is on screen.
        let mut prev_thumb = thumb0;
        for (i, dx) in [24.0f32, 48.0, 72.0, 96.0].iter().enumerate() {
            let x = x0 + dx;
            let damage = step(&mut window, HeadlessEvent::MouseMove { x, y });
            let now = rects_by_class(&window, "__azul-native-slider-thumb");
            let fired = state
                .borrow_mut()
                .downcast_ref::<SliderUiState>()
                .map(|s| s.interactions)
                .unwrap_or(0);
            println!(
                "[slider] move {i}: cursor x={x} thumb={now:?} (was {prev_thumb:?}) \
                 callbacks={fired} damage={damage:?} dragging={:?}",
                slider_dragging(&window)
            );
            let (diffs, first) = incremental_vs_full(&mut window);
            assert_eq!(
                diffs, 0,
                "drag step {i} (cursor x={x}): the presented frame differs from a full repaint of \
                 the same display list in {diffs} px, first at {first:?} — a thumb ghost / stale \
                 pixels on a real screen. thumb now {now:?}, before {prev_thumb:?}, damage \
                 {damage:?}"
            );
            if let Some(r) = now.first() {
                prev_thumb = *r;
            }
        }
        // The drag must FOLLOW the pointer across the app's RefreshDom
        // rebuilds, not die after the press. The widget maps the cursor's
        // fraction of the track onto a travel of (track − thumb): for the
        // last in-track cursor x that is where the thumb must sit. Before
        // the slider carried its `dragging` flag across a rebuild (and
        // before a bubbled MouseLeave from the thumb stopped ending the
        // drag), it followed for exactly one move in any app that refreshes
        // on change.
        let last_x = x0 + 96.0;
        let fraction = ((last_x - track.origin.x) / track.size.width).clamp(0.0, 1.0);
        let expected_x =
            track.origin.x + (fraction * (track.size.width - thumb0.size.width)).round();
        assert!(
            (prev_thumb.origin.x - expected_x).abs() <= 1.0,
            "the thumb did not follow the drag: at {prev_thumb:?}, expected x≈{expected_x} for \
             cursor x={last_x} (started at {thumb0:?})"
        );
        assert_eq!(
            slider_dragging(&window),
            Some(true),
            "still dragging inside the track"
        );

        // Leaving the TRACK ends the drag (the widget's rule); the thumb
        // stays where the last in-track move put it, and the frame is still
        // exact.
        let outside_x = track.origin.x + track.size.width + 2.0;
        step(&mut window, HeadlessEvent::MouseMove { x: outside_x, y });
        let (diffs, first) = incremental_vs_full(&mut window);
        assert_eq!(
            diffs, 0,
            "after leaving the track: {diffs} stale px, first at {first:?}"
        );
        assert_eq!(
            slider_dragging(&window),
            Some(false),
            "leaving the track ends the drag"
        );
        let parked = rects_by_class(&window, "__azul-native-slider-thumb");
        assert_eq!(
            parked.first().map(|r| r.origin.x),
            Some(prev_thumb.origin.x)
        );

        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        assert_eq!(slider_dragging(&window), Some(false), "released");
    }

    // --- A text selection survives a relayout ---------------------------
    //
    // REPORTED (demo test 2026-08-21): selecting from the heading into the
    // subtitle "flickers" the subtitle but nothing stays selected. The
    // selection was computed correctly; only the DISPLAY-LIST-ONLY rebuild
    // painted `SelectionRect`s, while the layout path handed
    // `layout_document` an empty selection map — so every relayout (a
    // resize, a restyle, an app's RefreshDom, a css patch) erased the band
    // until the next drag move repainted it, and the first relayout after the
    // release erased it for good. The class: two builders of the same
    // display list fed different inputs. This drags a selection across two
    // paragraphs, forces the LAYOUT path with a resize, and expects the band
    // to still be there.

    extern "C" fn selection_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
            .with_child(
                Dom::create_p_with_text("Azul Widget Showcase")
                    .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("h".into())].into())
                    .with_css("font-size: 24px; margin: 10px;"),
            )
            .with_child(
                Dom::create_p_with_text("Every built-in widget (callbacks fired so far: 0)")
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("sub".into())].into(),
                    )
                    .with_css("font-size: 14px; margin: 10px;"),
            )
    }

    fn selection_rect_count(window: &HeadlessWindow) -> usize {
        use azul_core::dom::DomId;
        let Some(lw) = window.common.layout_window.as_ref() else {
            return 0;
        };
        let Some(lr) = lw.layout_results.get(&DomId { inner: 0 }) else {
            return 0;
        };
        lr.display_list
            .items
            .iter()
            .filter(|it| matches!(it, DisplayListItem::SelectionRect { .. }))
            .count()
    }

    #[test]
    fn a_text_selection_survives_a_relayout() {
        use azul_core::events::MouseButton;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, selection_layout, 500.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let heading = rects_by_class(&window, "h");
        let subtitle = rects_by_class(&window, "sub");
        assert_eq!(heading.len(), 1, "{heading:?}");
        assert_eq!(subtitle.len(), 1, "{subtitle:?}");
        let (h, sub) = (heading[0], subtitle[0]);

        // Press inside the heading's first glyphs, drag into the subtitle.
        let start = (h.origin.x + 6.0, h.origin.y + h.size.height / 2.0);
        let end = (
            sub.origin.x + sub.size.width * 0.5,
            sub.origin.y + sub.size.height / 2.0,
        );
        step(
            &mut window,
            HeadlessEvent::MouseMove {
                x: start.0,
                y: start.1,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        for i in 1..=6 {
            let t = i as f32 / 6.0;
            step(
                &mut window,
                HeadlessEvent::MouseMove {
                    x: start.0 + (end.0 - start.0) * t,
                    y: start.1 + (end.1 - start.1) * t,
                },
            );
        }
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );

        let cross_block = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.text_edit_manager.get_cross_block_selection().cloned());
        assert!(
            cross_block.is_some(),
            "dragging from the heading into the subtitle must produce a cross-block selection"
        );
        let before = selection_rect_count(&window);
        assert!(before > 0, "the selection must be painted after the drag");

        // Force the LAYOUT path: a resize is a relayout with the same DOM.
        window.snapshot_window_state_baseline("headless.test.selection_resize");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 560.0;
            });
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        let _ = window.process_window_events(0);
        window.regenerate_layout().expect("relayout after resize");

        let after = selection_rect_count(&window);
        assert!(
            after > 0,
            "the selection band vanished on relayout ({before} SelectionRect(s) before, {after} \
             after): the layout path must paint the live selection, not an empty map"
        );
        assert!(
            window
                .common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.text_edit_manager.get_cross_block_selection())
                .is_some(),
            "the selection state itself must survive the relayout"
        );
    }

    // --- A relayout-only pass keeps the hit-tester in sync ----------------
    //
    // REPORTED (demo test 2026-08-21): AzWidgets' scroll area stopped reacting
    // to the wheel after a window resize and AzMap's "+" went dead, both
    // "healing" on the next widget click. The class: a RELAYOUT-ONLY pass
    // (the coalesced resize fast path, a restyle, a runtime edit) re-runs
    // layout on the existing StyledDom, but the CPU hit-tester is a CACHE of
    // that layout and was rebuilt only by the full `regenerate_layout()`. On
    // macOS and X11 nothing else rebuilt it, so input over a node that had
    // moved went to whatever used to be there. `CommonWindowState::
    // incremental_relayout` now owns that finalize tail (and the bare layout
    // function is private to `common`); this drives the resize path through
    // it and asks the hit-tester where a right-aligned box went.

    extern "C" fn right_aligned_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
            .with_css(
                "display: flex; flex-direction: row; justify-content: flex-end; width: 100%; \
                 height: 100%; margin: 0;",
            )
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("target".into())].into(),
                    )
                    .with_css(
                        "width: 40px; height: 40px; flex-grow: 0; flex-shrink: 0; background: red;",
                    ),
            )
    }

    /// Does the window's CPU hit-tester report a `.target` node at (x, y)?
    fn cpu_hit_tester_hits_class(window: &HeadlessWindow, class: &str, x: f32, y: f32) -> bool {
        use azul_core::{dom::IdOrClass, geom::LogicalPosition};

        let Some(ht) = window.common.cpu_hit_tester.as_ref() else {
            panic!("the headless window must own a CPU hit-tester");
        };
        let Some(lw) = window.common.layout_window.as_ref() else {
            return false;
        };
        ht.hit_test(LogicalPosition::new(x, y))
            .into_iter()
            .any(|(dom, node)| {
                lw.layout_results
                    .get(&dom)
                    .and_then(|lr| {
                        lr.styled_dom
                            .node_data
                            .as_container()
                            .internal
                            .get(node.index())
                            .map(|data| {
                                data.get_ids_and_classes().iter().any(|c| match c {
                                    IdOrClass::Class(s) => s.as_str() == class,
                                    IdOrClass::Id(_) => false,
                                })
                            })
                    })
                    .unwrap_or(false)
            })
    }

    #[test]
    fn a_relayout_only_pass_rebuilds_the_hit_tester() {
        use crate::desktop::shell2::common::event::{IncrementalRelayout, PlatformWindow};

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, right_aligned_layout, 300.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let before = rects_by_class(&window, "target");
        assert_eq!(before.len(), 1, "{before:?}");
        assert!(
            (before[0].origin.x - 260.0).abs() < 1.0,
            "a 40 px box right-aligned in a 300 px window starts at x = 260: {before:?}"
        );
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 280.0, 20.0),
            "after the full layout the hit-tester must find the box at its position"
        );

        // Widen the window through the RELAYOUT-ONLY path: the coalesced
        // resize fast path, no DOM rebuild — exactly what a live window does
        // on every drag of the window edge.
        window.snapshot_window_state_baseline("headless.test.relayout_only_hit_tester");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 500.0;
            });
        let mut debug_messages = None;
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Resize, &mut debug_messages)
            .expect("resize fast path");

        let after = rects_by_class(&window, "target");
        assert_eq!(after.len(), 1, "{after:?}");
        assert!(
            (after[0].origin.x - 460.0).abs() < 1.0,
            "layout itself must have moved the box to x = 460: {after:?}"
        );
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 480.0, 20.0),
            "STALE HIT-TESTER: layout moved the box to x = 460 but the hit-tester still answers \
             for the 300 px window — the relayout-only path skipped the rebuild"
        );
        assert!(
            !cpu_hit_tester_hits_class(&window, "target", 280.0, 20.0),
            "the box's OLD position must no longer hit it"
        );

        // The restyle flavour takes the same tail.
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 400.0;
            });
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Restyle, &mut debug_messages)
            .expect("restyle relayout");
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 380.0, 20.0),
            "the restyle relayout must rebuild the hit-tester too"
        );
    }

    /// A resize that crosses no breakpoint LATCHES the resize fast path
    /// (`request_regeneration_for_resize`) - the debug server's `resize`, the
    /// simulated OS resize, an app's `set_window_size`. The four desktop
    /// shells answer the latch with `IncrementalRelayout::Resize`
    /// (`resize_only_hint`: the retained tree, its warm per-node caches, a
    /// PATCHED display list - macOS `build_atomic_txn`, X11, Wayland, Windows
    /// `WM_PAINT`). Headless consumed the latch and ran the RESTYLE relayout
    /// (a full reconcile whose clone drops every per-node measurement, and
    /// no display-list patch), so no headless run - no debug-server probe, no
    /// AZ_E2E scenario - ever laid out a resize the way a desktop window
    /// does. AzMeet's "the input forgets to stretch" and "the statistics
    /// break lines when I resize" showed on the Mac and nowhere in the
    /// harness.
    #[test]
    fn a_resize_takes_the_fast_path_the_desktop_shells_take() {
        use azul_core::geom::LogicalSize;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, right_aligned_layout, 300.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        // The rebuild a new window is born owing: the frame path consumes it,
        // the direct calls above do not, and it would win over the resize.
        let _ = window.common.take_regeneration();

        let full = window.common.request_regeneration_for_resize(
            LogicalSize::new(300.0, 100.0),
            LogicalSize::new(500.0, 100.0),
        );
        assert!(!full, "no breakpoint is crossed: the fast path is latched");
        window.snapshot_window_state_baseline("headless.test.resize_fast_path");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions = LogicalSize::new(500.0, 100.0);
            });

        // The frame the latch asked for.
        window.service_frame(azul_core::events::ProcessEventResult::DoNothing);

        let skipped = window
            .common
            .layout_window
            .as_ref()
            .expect("the window has a layout window")
            .layout_cache
            .last_reconcile_was_skipped;
        assert!(
            skipped,
            "the resize must run the shells' fast path (`resize_only_hint`: the retained tree, \
             reconcile skipped), not the restyle relayout"
        );
        let after = rects_by_class(&window, "target");
        assert_eq!(after.len(), 1, "{after:?}");
        assert!(
            (after[0].origin.x - 460.0).abs() < 1.0,
            "the fast path laid the box out at the new width (x = 460): {after:?}"
        );
        assert!(
            cpu_hit_tester_hits_class(&window, "target", 480.0, 20.0),
            "and the frame's hit-tester follows it"
        );
    }

    // --- An unchanged RefreshDom still re-renders VirtualViews -------------
    //
    // REPORTED (AzMap "+" analysis, 2026-08-22): a RefreshDom whose only
    // change lives inside a dataset or a VirtualView's refany rebuilds an
    // IDENTICAL DOM, so regenerate_layout takes an unchanged exit — and the
    // view that renders that data was never re-invoked. The full path
    // re-invokes every view (reset_all_invocation_flags); the two unchanged
    // exits re-invoked none. The class: "the DOM did not change" is not
    // "the data did not change". Both exits now queue every view, and the
    // one frame-path drain re-invokes them in place AND rebuilds the CPU
    // hit-tester (macOS and headless used to skip that rebuild).

    /// What the VirtualView renders from: a model the app mutates in place.
    struct VvCounter {
        value: u32,
    }

    /// The app state: holds the SAME RefAny across builds, the shape that
    /// fingerprints equal (a map's tile cache, a virtual list's rows).
    struct VvAppState {
        content: RefAny,
    }

    static VV_INVOCATIONS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    extern "C" fn counter_view_render(
        data: RefAny,
        info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::geom::{LogicalPosition, LogicalRect};
        VV_INVOCATIONS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut data = data;
        let value = data
            .downcast_ref::<VvCounter>()
            .map(|c| c.value)
            .unwrap_or(u32::MAX);
        let size = info.get_bounds().get_logical_size();
        let rect = LogicalRect::new(LogicalPosition::zero(), size);
        azul_core::callbacks::VirtualViewReturn {
            dom: azul_core::dom::OptionDom::Some(
                Dom::create_div()
                    .with_child(Dom::create_p_with_text(format!("value {value}").as_str())),
            ),
            materialized: rect,
            virtual_rect: rect,
        }
    }

    extern "C" fn counter_view_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let content = data
            .downcast_ref::<VvAppState>()
            .map(|s| s.content.clone())
            .expect("app state");
        Dom::create_body().with_child(
            Dom::create_virtual_view(
                content,
                azul_core::callbacks::VirtualViewCallback::create(counter_view_render),
            )
            .with_css("width: 200px; height: 100px;"),
        )
    }

    /// Every text node in every NESTED dom (a VirtualView's content), debug-formatted.
    fn nested_dom_texts(window: &HeadlessWindow) -> Vec<String> {
        use azul_core::dom::NodeType;
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (dom_id, lr) in &lw.layout_results {
            if dom_id.inner == 0 {
                continue;
            }
            for nd in lr.styled_dom.node_data.as_container().internal.iter() {
                if let NodeType::Text(_) = nd.get_node_type() {
                    out.push(format!("{:?}", nd.get_node_type()));
                }
            }
        }
        out
    }

    fn pending_virtual_view_updates(window: &HeadlessWindow) -> usize {
        window.common.layout_window.as_ref().map_or(0, |lw| {
            lw.pending_virtual_view_updates
                .values()
                .map(|m| m.len())
                .sum()
        })
    }

    /// The nested DOM, node and laid-out width of the text `text` in a
    /// VirtualView's content: the width its paragraph's inline layout has.
    fn nested_text_width(
        window: &HeadlessWindow,
        text: &str,
    ) -> Option<(azul_core::dom::DomId, azul_core::id::NodeId, f32)> {
        use azul_core::{dom::NodeType, id::NodeId};
        let lw = window.common.layout_window.as_ref()?;
        for (dom_id, lr) in &lw.layout_results {
            if dom_id.inner == 0 {
                continue;
            }
            let nodes = lr.styled_dom.node_data.as_container();
            let hierarchy = lr.styled_dom.node_hierarchy.as_container();
            for (i, nd) in nodes.internal.iter().enumerate() {
                let NodeType::Text(t) = nd.get_node_type() else {
                    continue;
                };
                if t.as_ref().as_str() != text {
                    continue;
                }
                let node = NodeId::new(i);
                let parent = hierarchy[node].parent_id()?;
                let index = *lr.layout_tree.dom_to_layout.get(&parent)?.first()?;
                // The cache entry's bounds: the sparse layout is retired once the dense text
                // holds the clusters, and then has no items to measure.
                let width = lr
                    .layout_tree
                    .get_cached_inline_layout_for_node(index.index())?
                    .overflow
                    .unclipped_bounds
                    .width;
                return Some((*dom_id, node, width));
            }
        }
        None
    }

    /// A text the app changes inside a VirtualView's content
    /// (`change_node_text`) is laid out again on the relayout that follows,
    /// though the view itself is kept and its callback does not run.
    #[test]
    fn a_text_change_inside_a_virtual_view_lays_the_view_out_again() {
        use azul_core::styled_dom::NodeHierarchyItemId;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(VvAppState {
            content: RefAny::new(VvCounter { value: 0 }),
        })));
        let mut window = make_window_sized(&state, counter_view_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        window.common.drain_virtual_view_updates();
        let (dom, node, before) =
            nested_text_width(&window, "value 0").expect("harness: the view shows its text");

        let longer = "value 0 and then a much longer label";
        let r = window.apply_user_change(&azul_layout::callbacks::CallbackChange::ChangeNodeText {
            node_id: azul_core::dom::DomNodeId {
                dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(node)),
            },
            text: longer.into(),
        });
        window.service_frame(r);

        let (_, _, after) = nested_text_width(&window, longer)
            .expect("the view's DOM holds the new text");
        assert!(
            after > before + 40.0,
            "the view's paragraph still has its old width {after} (was {before}): the kept view \
             was not laid out again"
        );
    }

    /// A view's re-render (`trigger_virtual_view_rerender`, `trigger_all_virtual_view_rerender`)
    /// queues the view's callback for the frame's drain - every shell drains the queue
    /// before it builds the frame - and asks for a REPAINT: the view's own list is what
    /// changes, not the host's. The tier was `ShouldUpdateDisplayListCurrentWindow`, which
    /// macOS answers by regenerating the root's list from the layout tree before the frame
    /// and translating and sending it: every item of the window, each time a panned map
    /// left its margin.
    #[test]
    fn a_views_re_render_is_a_repaint_and_not_a_rebuild_of_the_hosts_display_list() {
        use azul_core::events::ProcessEventResult;
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(VvAppState {
            content: RefAny::new(VvCounter { value: 0 }),
        })));
        let mut window = make_window_sized(&state, counter_view_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        window.common.drain_virtual_view_updates();
        let (dom_id, node_id) = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.virtual_view_manager.all_view_keys().first().copied())
            .expect("premise: the page holds a view");

        let one = window.apply_user_change(&CallbackChange::UpdateVirtualView { dom_id, node_id });
        assert_eq!(
            one,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "the host's display list does not change: a repaint, not a rebuild"
        );
        assert_eq!(
            pending_virtual_view_updates(&window),
            1,
            "the view is queued for the frame's drain"
        );
        assert!(
            window.common.drain_virtual_view_updates(),
            "and the drain renders it"
        );

        let all = window.apply_user_change(&CallbackChange::UpdateAllVirtualViews);
        assert_eq!(all, ProcessEventResult::ShouldReRenderCurrentWindow);
        assert_eq!(pending_virtual_view_updates(&window), 1);
        assert!(window.common.drain_virtual_view_updates());
    }

    // --- A hover over a label's text is a repaint of the label -------------------------------
    //
    // The map editor's labels: spans with a text, placed absolutely in the child DOM of a
    // `VirtualView`, each with a `:hover { background }` (paint only) and enter / leave
    // callbacks, in a window whose sidebar holds thousands of items. A plain mouse move over
    // them - no button, no pan - cost the headless window a layout pass of the WHOLE window for
    // an enter or a leave (the staged CSS diff of a paint-only restyle was taken for a layout
    // owed), and macOS a rebuild of the ROOT's list: P's editor scenario counted 1.3 layout
    // passes and 0.4-0.7 display-list rebuilds a move, under every panel.

    /// What the labels of the view saw: how often the pointer entered and left one.
    #[derive(Clone)]
    struct LabelHovers {
        enters: Arc<std::sync::atomic::AtomicUsize>,
        leaves: Arc<std::sync::atomic::AtomicUsize>,
        /// What the labels' `:hover` rule declares.
        hover: &'static str,
    }

    /// The hover of a label that changes PAINT only: a background.
    const LABEL_HOVER_PAINT: &str = "background: rgba(66, 134, 244, 0.35);";

    extern "C" fn label_entered(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(h) = data.downcast_ref::<LabelHovers>() {
            h.enters.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn label_left(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(h) = data.downcast_ref::<LabelHovers>() {
            h.leaves.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    /// Six rows of twelve labels, 36 px apart, 40 px between the rows: the label of row `r`,
    /// column `c` is in the view at (10 + 36 c, 20 + 40 r), 12 px high.
    extern "C" fn labels_view_render(
        mut data: RefAny,
        info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::{
            events::{EventFilter, HoverEventFilter},
            geom::{LogicalPosition, LogicalRect},
        };
        let hovers = data
            .downcast_ref::<LabelHovers>()
            .map(|h| h.clone())
            .expect("the view's data");
        let mut labels = Vec::new();
        for row in 0..6 {
            for col in 0..12 {
                let css = format!(
                    "position: absolute; left: {}px; top: {}px; transform-origin: 0px 9px; \
                     transform: rotate(0deg); font-size: 12px; line-height: 1; \
                     letter-spacing: 0px; color: #222222; font-family: sans-serif; \
                     white-space: pre; cursor: pointer; text-shadow: 0px 0px 0px 1px #ffffff; \
                     :hover {{ {} }}",
                    10 + col * 36,
                    20 + row * 40,
                    hovers.hover,
                );
                labels.push(
                    Dom::create_span_with_text(format!("Ab{row}{col}"))
                        .with_css(&css)
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseEnter),
                            RefAny::new(hovers.clone()),
                            label_entered as usize,
                        )
                        .with_callback(
                            EventFilter::Hover(HoverEventFilter::MouseLeave),
                            RefAny::new(hovers.clone()),
                            label_left as usize,
                        ),
                );
            }
        }
        let rect = LogicalRect::new(LogicalPosition::zero(), info.get_bounds().get_logical_size());
        azul_core::callbacks::VirtualViewReturn {
            dom: azul_core::dom::OptionDom::Some(Dom::create_div().with_children(labels.into())),
            materialized: rect,
            virtual_rect: rect,
        }
    }

    /// A sidebar of fifteen hundred rows (three thousand items in the window's list), the view
    /// of the labels at (100, 0), 500 x 300.
    extern "C" fn labels_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let content = data
            .downcast_ref::<VvAppState>()
            .map(|s| s.content.clone())
            .expect("app state");
        let mut sidebar =
            Dom::create_div().with_css("width: 100px; height: 300px; overflow: hidden;");
        for i in 0..1_500 {
            sidebar = sidebar.with_child(Dom::create_div().with_css(if i % 2 == 0 {
                "height: 3px; background: #dde6f5; border-bottom: 1px solid #888888;"
            } else {
                "height: 3px; background: #f5e6dd; border-bottom: 1px solid #888888;"
            }));
        }
        Dom::create_body()
            .with_css("margin: 0px; background: #ffffff;")
            .with_child(sidebar)
            .with_child(
                Dom::create_virtual_view(
                    content,
                    azul_core::callbacks::VirtualViewCallback::create(labels_view_render),
                )
                .with_css("position: absolute; left: 100px; top: 0px; width: 500px; height: 300px;"),
            )
    }

    fn labels_window() -> (HeadlessWindow, LabelHovers) {
        labels_window_hovering(LABEL_HOVER_PAINT)
    }

    /// [`labels_window`] with the labels' `:hover` rule declaring `hover`.
    fn labels_window_hovering(hover: &'static str) -> (HeadlessWindow, LabelHovers) {
        use std::sync::atomic::AtomicUsize;

        let hovers = LabelHovers {
            enters: Arc::new(AtomicUsize::new(0)),
            leaves: Arc::new(AtomicUsize::new(0)),
            hover,
        };
        let state = Arc::new(RefCell::new(RefAny::new(VvAppState {
            content: RefAny::new(hovers.clone()),
        })));
        let mut window = make_window_sized(&state, labels_layout, 600.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        window.common.drain_virtual_view_updates();
        // The request the layouts above left is served by the first frame: not the pointer's.
        let _ = window.common.take_regeneration();
        (window, hovers)
    }

    /// The window's list and the view's, as weak references: they name the lists by their
    /// allocation, and a weak reference keeps the allocation from being given to a list built
    /// later (twenty rebuilds one after the other can hand the first one's address back).
    fn lists_of(
        window: &HeadlessWindow,
    ) -> (
        std::sync::Weak<azul_layout::solver3::display_list::DisplayList>,
        std::sync::Weak<azul_layout::solver3::display_list::DisplayList>,
    ) {
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let weak = |dom: azul_core::dom::DomId| {
            std::sync::Arc::downgrade(&lw.layout_results[&dom].display_list)
        };
        let child = *lw
            .layout_results
            .keys()
            .find(|dom| dom.inner != 0)
            .expect("the view's DOM");
        (weak(azul_core::dom::DomId::ROOT_ID), weak(child))
    }

    /// How many pixels of the box `(x, y, w, h)` of the last frame are the hover colour over
    /// white (`rgba(66, 134, 244, 0.35)`: a blue 60 above the red).
    fn bluish_pixels(window: &HeadlessWindow, (x, y, w, h): (u32, u32, u32, u32)) -> usize {
        let frame = window
            .cpu_backend
            .last_frame
            .as_ref()
            .expect("the window has drawn a frame");
        let mut n = 0;
        for py in y..y + h {
            for px in x..x + w {
                let i = ((py * frame.width() + px) * 4) as usize;
                let d = frame.data();
                if d[i + 2] > d[i].saturating_add(40) {
                    n += 1;
                }
            }
        }
        n
    }

    /// 30 plain moves of 12 px along the first row of labels: the pointer enters and leaves
    /// ten of them. No layout, no DOM, no rebuild of the window's list; the view's list is
    /// what is rebuilt, at most once a move.
    ///
    /// With a state colour bound when a state first PAINTS it (A3-window-50: the decoration
    /// patch shows the first enter of each label); the binding from the build of the list,
    /// which is the default, is the test below.
    #[test]
    fn a_mouse_move_over_the_text_of_labels_costs_no_layout_and_no_rebuild_of_the_windows_list() {
        azul_layout::window::set_state_colors_from_the_build_on_this_thread(Some(false));
        let swept = std::panic::catch_unwind(|| a_sweep_over_the_labels(false));
        azul_layout::window::set_state_colors_from_the_build_on_this_thread(None);
        if let Err(panic) = swept {
            std::panic::resume_unwind(panic);
        }
    }

    /// Audit A4-23, the eager binding (A3-window-55): the same sweep changes NO list. Every
    /// label's background is bound to a colour key by the build of the view's list, so each
    /// enter and each leave is that key's value: no layout, no build, no patch, and the
    /// view's list is the allocation it was (to the GPU renderer a changed list is a
    /// translation of every item of it and a scene build).
    /// Mutation: `LayoutWindow::bind_state_colors` given no nodes (the first enter of each
    /// label paints a run: the view's list is another list, `dl_node_patches` > 0).
    #[test]
    fn a_mouse_move_over_the_text_of_labels_changes_no_list() {
        a_sweep_over_the_labels(true);
    }

    /// The sweep of the two tests above; `by_value`: the labels' hover colour is a value
    /// from the first enter (the view's list stays), else the first enter of a label is a
    /// run painted into the view's list.
    fn a_sweep_over_the_labels(by_value: bool) {
        use std::sync::atomic::Ordering;

        let (mut window, hovers) = labels_window();
        // The pointer arrives over empty ground; the baseline is taken from there.
        step(&mut window, HeadlessEvent::MouseMove { x: 300.0, y: 290.0 });
        let (root_before, child_before) = lists_of(&window);
        window
            .common
            .layout_window
            .as_mut()
            .expect("a layout window")
            .frame_report
            .reset_counters();

        let moves = 30;
        for i in 0..moves {
            step(
                &mut window,
                HeadlessEvent::MouseMove {
                    x: 112.0 + 12.0 * i as f32,
                    y: 26.0,
                },
            );
        }

        let enters = hovers.enters.load(Ordering::SeqCst);
        let leaves = hovers.leaves.load(Ordering::SeqCst);
        assert!(
            enters >= 8 && leaves >= 8,
            "premise: the pointer went over the labels ({enters} entered, {leaves} left)"
        );
        let report = window
            .common
            .layout_window
            .as_ref()
            .expect("a layout window")
            .frame_report
            .clone();
        assert_eq!(
            report.layout_passes, 0,
            "a hover that changes a colour lays nothing out: {report:?}"
        );
        assert_eq!(report.dom_regenerations, 0, "{report:?}");
        let (root_after, child_after) = lists_of(&window);
        assert!(
            std::ptr::eq(root_after.as_ptr(), root_before.as_ptr()),
            "the window's list (thousands of items) was not rebuilt for a label's hover"
        );
        if by_value {
            assert!(
                std::ptr::eq(child_after.as_ptr(), child_before.as_ptr()),
                "the view's list is the list it was: a hover colour is the value of a key"
            );
            assert_eq!(
                (report.dl_rebuilds, report.dl_node_patches),
                (0, 0),
                "no list was built and none was patched"
            );
            return;
        }
        assert!(
            !std::ptr::eq(child_after.as_ptr(), child_before.as_ptr()),
            "the view's list is what holds the label: it is another list now"
        );
        // A3-window-07: a label's hover background is painted into the list the view has
        // (`LayoutWindow::patch_restyled_decorations`), which builds no list; a restyle the
        // patch cannot show still builds it. Either way a move costs one of the two at most.
        assert!(
            (1..=moves).contains(&((report.dl_rebuilds + report.dl_node_patches) as usize)),
            "one rebuild or one patch of the view's list a move at most, got {} + {}",
            report.dl_rebuilds,
            report.dl_node_patches
        );
    }

    /// The label under the pointer is painted in its hover colour in the frame, and not any
    /// more once the pointer has left it: the rebuilt list of the view is what the frame draws.
    #[test]
    fn the_label_under_the_pointer_is_drawn_with_its_hover_background() {
        let (mut window, _hovers) = labels_window();
        step(&mut window, HeadlessEvent::MouseMove { x: 300.0, y: 290.0 });
        // The first label of the first row: (10, 20) in the view, at (110, 20) in the window.
        let label = (110, 20, 24, 12);
        // The glyphs' own colour fringes count too: the plain label is the baseline.
        let plain = bluish_pixels(&window, label);

        step(&mut window, HeadlessEvent::MouseMove { x: 114.0, y: 26.0 });
        let hovered = bluish_pixels(&window, label);
        assert!(
            hovered > plain + 30,
            "the hovered label is drawn on its background ({hovered} bluish pixels, {plain} plain)"
        );

        step(&mut window, HeadlessEvent::MouseMove { x: 300.0, y: 290.0 });
        assert_eq!(
            bluish_pixels(&window, label),
            plain,
            "and is plain again once the pointer has left it"
        );
    }

    /// How many `SelectionRect` items the view's list holds: the highlight of a selection in it.
    fn view_selection_rects(window: &HeadlessWindow) -> usize {
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let child = *lw
            .layout_results
            .keys()
            .find(|dom| dom.inner != 0)
            .expect("the view's DOM");
        lw.layout_results[&child]
            .display_list
            .items
            .iter()
            .filter(|item| {
                matches!(item, azul_layout::solver3::display_list::DisplayListItem::SelectionRect { .. })
            })
            .count()
    }

    /// A press on the text of a label places a collapsed cursor in text nobody can edit: it
    /// paints nothing and changes nothing on screen, so no list is rebuilt - not the view's
    /// (thousands of items in the editor) and not the window's. The press rebuilt the list of
    /// the view where the cursor was placed, and the display-list tier it was answered with
    /// made the frame rebuild the window's own on top: two rebuilds a press.
    #[test]
    fn a_press_on_the_text_of_a_label_rebuilds_no_display_list() {
        use azul_core::{events::MouseButton, selection::Selection};

        let (mut window, _hovers) = labels_window();
        // The first label of the first row: (10, 20) in the view, at (110, 20) in the window.
        step(&mut window, HeadlessEvent::MouseMove { x: 114.0, y: 26.0 });
        let (root_before, child_before) = lists_of(&window);
        window
            .common
            .layout_window
            .as_mut()
            .expect("a layout window")
            .frame_report
            .reset_counters();

        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });

        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let session = lw
            .text_edit_manager
            .multi_cursor
            .as_ref()
            .expect("premise: the press opened a selection session in the label's text");
        assert_ne!(session.block.dom().inner, 0, "premise: in the view's DOM, not the window's");
        assert!(
            session.selections.iter().all(|s| matches!(s.selection, Selection::Cursor(_))),
            "premise: a collapsed cursor"
        );
        let report = lw.frame_report.clone();
        let (root_after, child_after) = lists_of(&window);
        assert!(
            std::ptr::eq(root_after.as_ptr(), root_before.as_ptr()),
            "the window's list was rebuilt (the shell asked for it): {report:?}"
        );
        assert!(
            std::ptr::eq(child_after.as_ptr(), child_before.as_ptr()),
            "the view's list was rebuilt (the press put its cursor there): {report:?}"
        );
        assert_eq!(
            report.dl_rebuilds, 0,
            "a cursor in text nobody can edit paints nothing: no list is rebuilt: {report:?}"
        );
        assert_eq!(report.layout_passes, 0, "{report:?}");
        assert!(!lw.text_edit_manager.display_list_dirty, "no repaint is owed");
    }

    /// The press is answered with a repaint, not the display-list tier: the layout window rebuilt
    /// the lists the press changed itself, and the tier makes macOS rebuild the window's own on top
    /// (the headless frame does not, so the tier is asked of the system change directly).
    #[test]
    fn a_text_selection_press_asks_for_a_repaint_and_not_for_a_rebuild() {
        use azul_core::events::{ProcessEventResult, SystemChange};

        use crate::desktop::shell2::common::event::PlatformWindow;

        let (mut window, _hovers) = labels_window();
        step(&mut window, HeadlessEvent::MouseMove { x: 114.0, y: 26.0 });
        let pressed = window.apply_system_change(&SystemChange::TextSelectionClick {
            position: LogicalPosition { x: 114.0, y: 26.0 },
            timestamp: azul_core::task::Instant::now(),
        });
        assert_eq!(
            pressed,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "a press that placed a cursor is presented, not answered with a rebuild of the window's list"
        );
    }

    /// A drag over the text of a label selects it: the highlight is in the view's list - the
    /// DOM the selection is in - and the window's own list is not rebuilt for it (the
    /// display-list tier the drag was answered with rebuilt it on every move).
    #[test]
    fn a_drag_over_the_text_of_a_label_paints_its_highlight_in_the_views_list_alone() {
        use azul_core::events::MouseButton;

        let (mut window, _hovers) = labels_window();
        step(&mut window, HeadlessEvent::MouseMove { x: 112.0, y: 26.0 });
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        let (root_before, _) = lists_of(&window);
        assert_eq!(view_selection_rects(&window), 0, "premise: nothing is selected yet");
        window
            .common
            .layout_window
            .as_mut()
            .expect("a layout window")
            .frame_report
            .reset_counters();

        for x in [118.0f32, 124.0, 130.0, 136.0] {
            step(&mut window, HeadlessEvent::MouseMove { x, y: 26.0 });
        }

        assert!(
            view_selection_rects(&window) > 0,
            "the dragged range is painted in the view's list"
        );
        let (root_after, _) = lists_of(&window);
        assert!(
            std::ptr::eq(root_after.as_ptr(), root_before.as_ptr()),
            "the window's list (the selection is in the view's DOM) was not rebuilt"
        );
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
    }

    /// The caret of an editable field is painted by a press, as ever - the press that paints
    /// nothing is the one in text nobody can edit.
    #[test]
    fn a_press_in_an_editable_field_still_paints_its_caret() {
        use azul_core::events::MouseButton;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, filled_text_input_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let containers = rects_by_class(&window, "__azul-native-text-input-container");
        let c = containers[0];
        step(
            &mut window,
            HeadlessEvent::MouseMove {
                x: c.origin.x + c.size.width * 0.45,
                y: c.origin.y + c.size.height * 0.5,
            },
        );
        window
            .common
            .layout_window
            .as_mut()
            .expect("a layout window")
            .frame_report
            .reset_counters();
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });

        assert!(
            !caret_items(&window).is_empty(),
            "the press placed a caret and the list the frame shows paints it (no layout between)"
        );
    }

    /// 30 plain moves along the first row of labels (the pointer enters and leaves ten of them)
    /// with labels whose `:hover` rule declares `hover`: no layout, no rebuild of any list, no
    /// diff left staged, and both lists the allocations they were.
    fn assert_a_hover_rebuilds_nothing(hover: &'static str) {
        let (mut window, hovers) = labels_window_hovering(hover);
        step(&mut window, HeadlessEvent::MouseMove { x: 300.0, y: 290.0 });
        let (root_before, child_before) = lists_of(&window);
        window
            .common
            .layout_window
            .as_mut()
            .expect("a layout window")
            .frame_report
            .reset_counters();

        let moves = 30;
        for i in 0..moves {
            step(
                &mut window,
                HeadlessEvent::MouseMove {
                    x: 112.0 + 12.0 * i as f32,
                    y: 26.0,
                },
            );
        }
        let enters = hovers.enters.load(std::sync::atomic::Ordering::SeqCst);
        assert!(enters >= 8, "premise: the pointer went over the labels ({enters} entered)");

        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let report = lw.frame_report.clone();
        assert_eq!(report.layout_passes, 0, "`{hover}` lays nothing out: {report:?}");
        assert_eq!(report.dom_regenerations, 0, "{report:?}");
        assert_eq!(report.dl_rebuilds, 0, "`{hover}` rebuilds no list: {report:?}");
        assert!(
            lw.pending_css_dirty.is_none(),
            "the diff the restyle staged is served: {:?}",
            lw.pending_css_dirty
        );
        let (root_after, child_after) = lists_of(&window);
        assert!(std::ptr::eq(root_after.as_ptr(), root_before.as_ptr()), "the window's list");
        assert!(std::ptr::eq(child_after.as_ptr(), child_before.as_ptr()), "the view's list");
    }

    /// A hover that moves a `transform` (or an `opacity`) of a node the list already binds by
    /// key is a value: the new matrix is published under the key, no list is rebuilt and nothing
    /// is laid out. The restyle STAGES its css diff (`pending_css_dirty`: scope `None`) before it
    /// classifies the change, and the GPU-only answer left the entry standing - the headless
    /// window took any staged diff for a layout owed (`service_frame`) and laid the whole window
    /// out for every enter and leave.
    #[test]
    fn a_hover_that_moves_a_transform_costs_no_layout_and_no_rebuild() {
        assert_a_hover_rebuilds_nothing("transform: rotate(8deg);");
    }

    /// A hover that only changes the pointer's shape (`cursor`) changes nothing the frame
    /// draws: the shape is read by the OS, no display list holds it. The restyle asked for the
    /// rebuild of the DOM's list all the same (a paint-scope change, as any other property that
    /// is not layout), once per enter and per leave.
    #[test]
    fn a_hover_that_only_changes_the_pointer_shape_rebuilds_no_list() {
        assert_a_hover_rebuilds_nothing("cursor: text;");
    }

    #[test]
    fn an_unchanged_refresh_dom_still_reinvokes_virtual_views() {
        use std::sync::atomic::Ordering;

        use azul_core::geom::LogicalPosition;

        use crate::desktop::shell2::common::{
            event::PlatformWindow, layout::LayoutRegenerateResult,
        };

        let state = Arc::new(RefCell::new(RefAny::new(VvAppState {
            content: RefAny::new(VvCounter { value: 0 }),
        })));
        let mut window = make_window_sized(&state, counter_view_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        // Settle the queue the identical second build just raised, so the
        // count below starts from a drained state.
        window.common.drain_virtual_view_updates();
        assert!(
            nested_dom_texts(&window)
                .iter()
                .any(|t| t.contains("value 0")),
            "the view must have rendered the initial model: {:?}",
            nested_dom_texts(&window)
        );
        let invocations_before = VV_INVOCATIONS.load(Ordering::SeqCst);
        assert!(invocations_before >= 1);

        // The app mutates its model IN PLACE and asks for a RefreshDom. The
        // rebuilt DOM is identical (same RefAny, same callback, same CSS).
        {
            let mut g = state.borrow_mut();
            let r: &mut RefAny = &mut g;
            let mut app = r.downcast_mut::<VvAppState>().expect("app state");
            let mut counter = app.content.downcast_mut::<VvCounter>().expect("counter");
            counter.value = 1;
        }
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout().expect("refresh");
        assert!(
            matches!(result, LayoutRegenerateResult::LayoutUnchanged),
            "this test exercises the UNCHANGED exit — an identical rebuild must take it"
        );
        assert!(
            pending_virtual_view_updates(&window) > 0,
            "an unchanged RefreshDom must queue the VirtualViews for a re-invoke: the DOM did not \
             change, the data behind the view did"
        );

        // The frame path drains the queue before it paints.
        assert!(
            window.common.drain_virtual_view_updates(),
            "the drain must report that a view was rebuilt"
        );
        assert_eq!(
            pending_virtual_view_updates(&window),
            0,
            "the drain empties the queue"
        );
        assert!(
            VV_INVOCATIONS.load(Ordering::SeqCst) > invocations_before,
            "the view's callback must run again after the RefreshDom"
        );
        let texts = nested_dom_texts(&window);
        assert!(
            texts.iter().any(|t| t.contains("value 1")),
            "the view must now render the mutated model, not last frame's: {texts:?}"
        );

        // The hit-tester was rebuilt by the drain and still resolves the view's
        // host node (the nested content carries no hit-test tags of its own).
        let ht = window
            .common
            .cpu_hit_tester
            .as_ref()
            .expect("headless owns a CPU hit-tester");
        let hits = ht.hit_test(LogicalPosition::new(100.0, 50.0));
        assert!(
            hits.iter()
                .any(|(dom, node)| dom.inner == 0 && node.index() == 1),
            "after the drain the hit-tester must still resolve the VirtualView host: {hits:?}"
        );
    }

    // --- An identical rebuild renders its views from the FRESH payload --------
    //
    // CODESCROLL13 (2026-10-08): when the fresh build fingerprints equal, the
    // pre-cascade skip keeps the retained StyledDom and moves the fresh
    // build's event callbacks, image callbacks and datasets onto it - but not
    // its VirtualViews' callback and refany, which the fingerprint does not
    // read. The retained view kept LAST build's refany. A widget that makes
    // one RefAny per build and hands clones of it to its handlers and to its
    // VirtualView (the code view: its view state) was split in two after any
    // identical rebuild: the handlers wrote the fresh allocation, the view
    // kept rendering the old one - a scroll was stored and nothing moved. The
    // equivalence exit has always transferred the view's refany.

    /// The app state: the model's value, handed to a FRESH RefAny per build.
    struct VvFreshState {
        value: u32,
    }

    extern "C" fn fresh_view_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let value = data
            .downcast_ref::<VvFreshState>()
            .map(|s| s.value)
            .expect("app state");
        Dom::create_body().with_child(
            Dom::create_virtual_view(
                RefAny::new(VvCounter { value }),
                azul_core::callbacks::VirtualViewCallback::create(counter_view_render),
            )
            .with_css("width: 200px; height: 100px;"),
        )
    }

    #[test]
    fn an_identical_rebuild_renders_its_virtual_views_from_the_fresh_payload() {
        use crate::desktop::shell2::common::{
            event::PlatformWindow, layout::LayoutRegenerateResult,
        };

        let state = Arc::new(RefCell::new(RefAny::new(VvFreshState { value: 0 })));
        let mut window = make_window_sized(&state, fresh_view_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        window.common.drain_virtual_view_updates();
        assert!(
            nested_dom_texts(&window)
                .iter()
                .any(|t| t.contains("value 0")),
            "the view must have rendered the initial model: {:?}",
            nested_dom_texts(&window)
        );

        // The model moves. The rebuilt DOM has the same shape and style: only
        // the VirtualView's refany - a fresh allocation - carries the change.
        {
            let mut g = state.borrow_mut();
            let r: &mut RefAny = &mut g;
            let mut app = r.downcast_mut::<VvFreshState>().expect("app state");
            app.value = 1;
        }
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout().expect("refresh");
        assert!(
            matches!(result, LayoutRegenerateResult::LayoutUnchanged),
            "this test exercises an UNCHANGED exit - an identical rebuild must take it"
        );
        window.common.drain_virtual_view_updates();
        let texts = nested_dom_texts(&window);
        assert!(
            texts.iter().any(|t| t.contains("value 1")),
            "the view renders the payload of the build on screen, not the one before it: {texts:?}"
        );
    }

    // --- A native pinch reaches the callbacks of its own pass --------------
    //
    // REPORTED (AzMap, 2026-08-21): a trackpad pinch over the map did nothing.
    // The class: per-pass input that callbacks read LIVE from a manager
    // (the wheel delta via get_scroll_delta, the injected native gesture via
    // get_pinch) must be cleared AFTER dispatch. The native gesture was
    // cleared with the other manager flags during determination, so the
    // PinchIn/PinchOut event it produced was dispatched to a callback that
    // read `None`. The clear now sits next to the wheel delta's, after
    // dispatch; this injects a magnify the way macOS does and checks what
    // the callback saw — and that it does not re-fire on the next pass.

    #[derive(Debug, Clone)]
    struct PinchLog {
        /// (callbacks invoked, callbacks that saw a pinch, last scale × 1000)
        seen: Arc<core::sync::atomic::AtomicUsize>,
        invoked: Arc<core::sync::atomic::AtomicUsize>,
        scale_milli: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn log_pinch(
        mut refany: RefAny,
        info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use core::sync::atomic::Ordering;
        if let Some(log) = refany.downcast_ref::<PinchLog>() {
            log.invoked.fetch_add(1, Ordering::SeqCst);
            if let Some(p) = info.get_pinch().into_option() {
                log.seen.fetch_add(1, Ordering::SeqCst);
                log.scale_milli
                    .store((p.scale * 1000.0).round() as usize, Ordering::SeqCst);
            }
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn pinch_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            events::{EventFilter, HoverEventFilter},
        };
        let log = data
            .downcast_ref::<PinchLog>()
            .map(|l| l.clone())
            .expect("pinch log");
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css("width: 300px; height: 200px;")
                .with_callbacks(
                    vec![
                        CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::PinchOut),
                            callback: CoreCallback {
                                cb: log_pinch as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(log.clone()),
                        },
                        CoreCallbackData {
                            event: EventFilter::Hover(HoverEventFilter::PinchIn),
                            callback: CoreCallback {
                                cb: log_pinch as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(log),
                        },
                    ]
                    .into(),
                ),
        )
    }

    #[test]
    fn a_native_pinch_is_visible_to_the_callbacks_of_its_own_pass() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use azul_layout::managers::gesture::{DetectedPinch, NativeGestureEvent};

        use crate::desktop::shell2::common::event::PlatformWindow;

        let log = PinchLog {
            seen: Arc::new(AtomicUsize::new(0)),
            invoked: Arc::new(AtomicUsize::new(0)),
            scale_milli: Arc::new(AtomicUsize::new(0)),
        };
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, pinch_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");

        // Hover the box (PinchIn/PinchOut target the hovered node), then inject
        // a magnify exactly like macOS's magnify handler does, and run a pass.
        step(&mut window, HeadlessEvent::MouseMove { x: 150.0, y: 100.0 });
        window
            .common
            .layout_window
            .as_mut()
            .expect("layout window")
            .gesture_drag_manager
            .inject_native_gesture(NativeGestureEvent::Pinch(DetectedPinch {
                scale: 1.5,
                center: azul_core::geom::LogicalPosition::new(150.0, 100.0),
                initial_distance: 100.0,
                current_distance: 150.0,
                duration_ms: 0,
                began: true,
                ended: false,
            }));
        window.snapshot_window_state_baseline("headless.test.magnify");
        let _ = window.process_window_events(0);

        assert_eq!(
            log.invoked.load(Ordering::SeqCst),
            1,
            "the injected magnify must dispatch exactly one PinchOut callback"
        );
        assert_eq!(
            log.seen.load(Ordering::SeqCst),
            1,
            "the callback must be able to READ the pinch it was dispatched for: clearing the \
             native gesture before dispatch hands it `None`"
        );
        assert_eq!(log.scale_milli.load(Ordering::SeqCst), 1500);

        // The gesture is consumed by its pass: a later pass with nothing new
        // must not re-fire it.
        window.snapshot_window_state_baseline("headless.test.magnify_idle");
        let _ = window.process_window_events(0);
        assert_eq!(
            log.invoked.load(Ordering::SeqCst),
            1,
            "an ended pinch must not re-fire on the next pass"
        );
    }

    // --- A screen reader's default action clicks the node ------------------
    //
    // FOUND (AzCalendar's E2E, 2026-09-30): `accessibility_action default` on a
    // Button did nothing. `LayoutWindow::process_accessibility_action` answers
    // `AccessibilityAction::Default` with the node's `Click` filter (activation,
    // the filter every widget listens on), but
    // `PlatformWindow::dispatch_accessibility_events` only turned MouseUp /
    // MouseDown filters into events and skipped the rest, so no event was
    // dispatched: VoiceOver's / NVDA's "press" reached no callback on any
    // backend. This drives the headless ingress every backend shares.

    #[derive(Debug, Clone)]
    struct ClickLog {
        clicks: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn log_click(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(log) = refany.downcast_ref::<ClickLog>() {
            log.clicks
                .fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn click_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            events::{EventFilter, HoverEventFilter},
        };
        let log = data
            .downcast_ref::<ClickLog>()
            .map(|l| l.clone())
            .expect("click log");
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css("width: 120px; height: 40px;")
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: log_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(log),
                    }]
                    .into(),
                ),
        )
    }

    #[cfg(feature = "a11y")]
    #[test]
    fn a_screen_readers_default_action_runs_the_nodes_click_callback_once() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        let log = ClickLog {
            clicks: Arc::new(AtomicUsize::new(0)),
        };
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, click_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");

        window.inject_accessibility_action(
            azul_core::dom::DomId::ROOT_ID,
            azul_core::id::NodeId::new(1),
            azul_core::dom::AccessibilityAction::Default,
        );
        window.process_accessibility_actions();

        assert_eq!(
            log.clicks.load(Ordering::SeqCst),
            1,
            "the default action (a screen reader's press) must run the node's Click callback \
             exactly once"
        );
    }

    // --- NodeResized fires when a node's box changes ----------------------
    //
    // REPORTED (AzPaint): "do we have a working 'node was resized' event?"
    // `ComponentEventFilter::NodeResized` is public API (the video widget
    // resizes its decoder target on it) and had NEVER fired in a running
    // app: its only emitter compared layout maps production passed EMPTY,
    // ran before the new tree was solved, and was not reached by a window
    // resize at all. The class: a lifecycle event derived from layout must
    // be derived from the SOLVED layout, after every pass — and the relayout
    // paths must deliver it. This resizes through the fast path, through a
    // no-op restyle, and through a full rebuild.

    #[derive(Debug, Clone)]
    struct ResizeLog {
        hits: Arc<core::sync::atomic::AtomicUsize>,
        last_width: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn on_node_resized(
        mut refany: RefAny,
        info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use core::sync::atomic::Ordering;
        if let Some(log) = refany.downcast_ref::<ResizeLog>() {
            log.hits.fetch_add(1, Ordering::SeqCst);
            if let Some(size) = info.get_node_size(info.get_hit_node()) {
                log.last_width
                    .store(size.width.round() as usize, Ordering::SeqCst);
            }
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn node_resized_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{ComponentEventFilter, EventFilter},
        };
        let log = data
            .downcast_ref::<ResizeLog>()
            .map(|l| l.clone())
            .expect("resize log");
        Dom::create_body()
            .with_css("display: flex; flex-direction: row; width: 100%; height: 100%; margin: 0;")
            .with_child(
                Dom::create_div()
                    .with_css("width: 100px; height: 50px; flex-grow: 0; flex-shrink: 0;"),
            )
            .with_child(
                Dom::create_div()
                    .with_css("flex-grow: 1; height: 50px;")
                    .with_callbacks(
                        vec![CoreCallbackData {
                            event: EventFilter::Component(ComponentEventFilter::NodeResized),
                            callback: CoreCallback {
                                cb: on_node_resized as usize,
                                ctx: azul_core::refany::OptionRefAny::None,
                            },
                            refany: RefAny::new(log),
                        }]
                        .into(),
                    ),
            )
    }

    #[test]
    fn node_resized_fires_after_a_relayout() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use crate::desktop::shell2::common::event::{IncrementalRelayout, PlatformWindow};

        let log = ResizeLog {
            hits: Arc::new(AtomicUsize::new(0)),
            last_width: Arc::new(AtomicUsize::new(0)),
        };
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, node_resized_layout, 400.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        assert_eq!(
            log.hits.load(Ordering::SeqCst),
            0,
            "a mount is not a resize"
        );

        // Widen through the RESIZE FAST PATH (no rebuild): 400 → 600 px grows
        // the flex child from 300 to 500 px.
        window.snapshot_window_state_baseline("headless.test.node_resized");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 600.0;
            });
        let mut debug_messages = None;
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Resize, &mut debug_messages)
            .expect("resize fast path");
        assert_eq!(
            log.hits.load(Ordering::SeqCst),
            1,
            "NodeResized must fire once for the child whose box grew (and not for the fixed-width \
             sibling)"
        );
        assert_eq!(
            log.last_width.load(Ordering::SeqCst),
            500,
            "the callback sees the NEW size"
        );

        // A relayout that changes nothing is not a resize.
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Restyle, &mut debug_messages)
            .expect("restyle");
        assert_eq!(
            log.hits.load(Ordering::SeqCst),
            1,
            "an unchanged box must not re-fire"
        );

        // A FULL regeneration at yet another size fires too: the node keeps
        // its identity across the rebuild, so its baseline follows it.
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 500.0;
            });
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        window.regenerate_layout().expect("full regeneration");
        assert_eq!(
            log.hits.load(Ordering::SeqCst),
            2,
            "the full path delivers NodeResized as well"
        );
        assert_eq!(log.last_width.load(Ordering::SeqCst), 400);
    }

    // --- A capture tile tells its worker its DEVICE size ---------------------
    //
    // REPORTED (AzMeet CPU, 2026-08-22): the camera captured at the device's
    // 1080p default for a 300x200 tile, and nothing ever told the worker the
    // tile's size. The class: a capture widget's size must come from LAYOUT
    // (device pixels: the laid-out size times the hidpi factor), at mount and
    // on every resize, so the worker captures at the covering size and cuts
    // the preview to exactly the tile. This drives a CameraWidget (no backend
    // in the test binary -> the test pattern, through the same loop) through
    // mount + a resize + a DPI change and reads the size the widget recorded
    // for its worker.

    extern "C" fn camera_tile_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::camera::CameraWidget;
        Dom::create_body()
            .with_css("display: flex; flex-direction: row; width: 100%; height: 100%;")
            .with_child(
                CameraWidget::create(azul_core::camera::CameraConfig::default())
                    .dom()
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("cam".into())].into(),
                    )
                    .with_css("width: 50%; height: 90px; flex-grow: 0; flex-shrink: 0;"),
            )
    }

    /// The `(preview, has_thread)` the camera widget's state holds.
    fn camera_state_of(window: &HeadlessWindow) -> (Option<(u32, u32)>, bool) {
        use azul_core::dom::DomId;
        use azul_layout::widgets::camera::CameraWidgetState;
        let Some(lw) = window.common.layout_window.as_ref() else {
            panic!("layout window");
        };
        let Some(dom) = lw.layout_results.get(&DomId { inner: 0 }) else {
            panic!("root dom");
        };
        for data in dom.styled_dom.node_data.as_container().internal.iter() {
            // `get_dataset` hands out `Option<&RefAny>`; clone the RefAny
            // itself (not the reference) to downcast it.
            let Some(ds) = data.get_dataset() else {
                continue;
            };
            let mut ds: RefAny = RefAny::clone(ds);
            // Copy the two fields out so the `Ref` guard is dropped before
            // `ds` goes out of scope (E0597 otherwise: the guard's destructor
            // runs after the local it borrows).
            let found = ds
                .downcast_ref::<CameraWidgetState>()
                .map(|s| (s.preview, s.thread_id.is_some()));
            if let Some(found) = found {
                return found;
            }
        }
        panic!("no CameraWidgetState in the DOM");
    }

    #[test]
    fn a_capture_tile_reports_its_device_size_to_its_worker() {
        use crate::desktop::shell2::common::event::{IncrementalRelayout, PlatformWindow};

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, camera_tile_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let tile = rects_by_class(&window, "cam");
        assert_eq!(tile.len(), 1, "{tile:?}");
        assert!(
            (tile[0].size.width - 200.0).abs() < 1.0,
            "50% of 400: {tile:?}"
        );

        let (preview, has_thread) = camera_state_of(&window);
        assert!(has_thread, "AfterMount starts the capture worker");
        assert_eq!(
            preview,
            Some((200, 90)),
            "AT MOUNT the widget already knows its laid-out size (AfterMount has the hit node), \
             so the camera is opened at the tile's size, not a 1080p default"
        );

        // Widen the window through the resize fast path: the tile grows to
        // 300 px and NodeResized must hand the worker the new size. Each
        // mutation follows the shells' snapshot -> mutate -> PASS contract
        // (the pass consumes the size delta and advances the event baseline;
        // AZ_VALIDATE panics on the next snapshot otherwise), and the frame
        // loop's reaction — the resize fast path — is then driven explicitly.
        let mut debug_messages = None;
        window.snapshot_window_state_baseline("headless.test.capture_tile_size");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 600.0;
            });
        let _ = window.process_window_events(0);
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Resize, &mut debug_messages)
            .expect("resize fast path");
        assert_eq!(
            camera_state_of(&window).0,
            Some((300, 90)),
            "NodeResized re-targets the worker"
        );

        // A Retina display: the same 300x90 LOGICAL tile is 600x180 DEVICE
        // pixels — the size the preview must be cut at (logical px undersized
        // a Retina preview by 2x before).
        window.snapshot_window_state_baseline("headless.test.capture_tile_dpi");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dpi = 192;
            });
        let _ = window.process_window_events(0);
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
        // A DPI change is a FULL regeneration, and `regenerate_layout` runs
        // the lifecycle loop itself: the NodeResized the DPI change queues
        // (`resize_watch_dpi`) is dispatched before it returns.
        window.regenerate_layout().expect("full regeneration at 2x");
        assert_eq!(
            camera_state_of(&window).0,
            Some((600, 180)),
            "the worker is told DEVICE pixels: logical 300x90 at 2x is 600x180"
        );

        // And the resize FAST PATH still works after that full regeneration
        // (a DPI change clears the caches; the next OS resize must not fail
        // on the rebuilt tree): 700 wide -> a 350 px tile -> 700x180 device.
        window.snapshot_window_state_baseline("headless.test.capture_tile_size_2x");
        window
            .common
            .update_window_state(event::WindowStateSource::Os, |ws| {
                ws.size.dimensions.width = 700.0;
            });
        let _ = window.process_window_events(0);
        window
            .incremental_relayout_dispatching(IncrementalRelayout::Resize, &mut debug_messages)
            .expect("resize fast path at 2x");
        assert_eq!(
            camera_state_of(&window).0,
            Some((700, 180)),
            "a resize after the DPI change re-targets the worker in device pixels"
        );
    }

    // --- Menu accelerators fire on every backend ---------------------------
    //
    // REPORTED (AzPaint, 2026-08-21): the File menu's Ctrl+O / Ctrl+S chords
    // did nothing outside the macOS menu bar — a menu item's accelerator was
    // display-only everywhere else. The class: the chord -> item lookup and
    // the item's invocation lived in AppKit alone. `PlatformWindow::
    // dispatch_menu_accelerators` now runs in every event pass (the root
    // menu bar unless the backend's bar is native, then the focused node's
    // context menus), with `azul_core::menu::accelerator_matches` as the one
    // chord rule. Headless has no native menus, so this exercises exactly the
    // shared path Windows / X11 / Wayland use.

    struct AccelLog {
        saved: Arc<core::sync::atomic::AtomicUsize>,
        deleted: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn accel_save(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(s) = data.downcast_ref::<AccelLog>() {
            s.saved.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }
    extern "C" fn accel_delete(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(s) = data.downcast_ref::<AccelLog>() {
            s.deleted.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn accel_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::CoreCallback,
            dom::{IdOrClass, TabIndex},
            menu::{Menu, MenuItem, StringMenuItem},
            window::{VirtualKeyCode as K, VirtualKeyCodeCombo, VirtualKeyCodeVec},
        };

        let log = data.downcast_ref::<AccelLog>().map(|l| AccelLog {
            saved: l.saved.clone(),
            deleted: l.deleted.clone(),
        });
        let Some(log) = log else {
            return Dom::create_body();
        };
        let state = RefAny::new(log);
        let chord = |keys: &[K]| VirtualKeyCodeCombo {
            keys: VirtualKeyCodeVec::from_vec(keys.to_vec()),
        };
        let item = |label: &str,
                    cb: extern "C" fn(
            RefAny,
            azul_layout::callbacks::CallbackInfo,
        ) -> azul_core::callbacks::Update,
                    keys: &[K]| {
            let mut it = StringMenuItem::create(label.into()).with_callback(
                state.clone(),
                CoreCallback {
                    cb: cb as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
            );
            it.accelerator = Some(chord(keys)).into();
            MenuItem::String(it)
        };
        // File > Save (primary+S) in the MENU BAR; Delete (Del) in the
        // CONTEXT MENU of a focusable box.
        let file = StringMenuItem::create("File".into())
            .with_children(vec![item("Save", accel_save, &[K::LWin, K::S])].into());
        let menu_bar = Menu::create(vec![MenuItem::String(file)].into());
        let context = Menu::create(vec![item("Delete", accel_delete, &[K::Delete])].into());
        Dom::create_body()
            .with_menu_bar(menu_bar)
            .with_css("width: 100%; height: 100%;")
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![IdOrClass::Class("box".into())].into())
                    .with_tab_index(TabIndex::Auto)
                    .with_context_menu(context)
                    .with_css("width: 100px; height: 60px; background: #ddd;"),
            )
    }

    #[test]
    fn a_menu_accelerator_runs_the_items_callback_once_per_chord_on_the_shared_path() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use azul_core::{events::MouseButton, window::VirtualKeyCode as K};

        let log = AccelLog {
            saved: Arc::new(AtomicUsize::new(0)),
            deleted: Arc::new(AtomicUsize::new(0)),
        };
        let (saved, deleted) = (log.saved.clone(), log.deleted.clone());
        let state = Arc::new(RefCell::new(RefAny::new(log)));
        let mut window = make_window_sized(&state, accel_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // The platform's primary modifier: Cmd on macOS, Ctrl elsewhere — the
        // `[LWin, S]` combo means exactly that on each (one definition).
        let primary = if cfg!(target_os = "macos") {
            K::LWin
        } else {
            K::LControl
        };

        // Ctrl+S -> Save, once.
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: primary,
            },
        );
        assert_eq!(
            saved.load(Ordering::SeqCst),
            0,
            "a lone modifier is not a chord"
        );
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::S,
            },
        );
        assert_eq!(
            saved.load(Ordering::SeqCst),
            1,
            "Ctrl+S runs File > Save through the shared dispatch"
        );
        // A pass without a new key-down (a redraw, a timer tick) must not re-fire.
        window.regenerate_layout().expect("an unrelated pass");
        assert_eq!(
            saved.load(Ordering::SeqCst),
            1,
            "the chord fires on the key-down, not on every pass"
        );
        // Release + press again = a second chord.
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::S,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::S,
            },
        );
        assert_eq!(saved.load(Ordering::SeqCst), 2);
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::S,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: primary,
            },
        );
        // S alone is typing, not a chord; Shift+Ctrl+S is a DIFFERENT chord.
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::S,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::S,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::LShift,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: primary,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::S,
            },
        );
        assert_eq!(
            saved.load(Ordering::SeqCst),
            2,
            "exact modifiers: Ctrl+Shift+S is not Ctrl+S"
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::S,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: primary,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::LShift,
            },
        );

        // The context menu's Delete fires only while its node is focused.
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::Delete,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::Delete,
            },
        );
        assert_eq!(
            deleted.load(Ordering::SeqCst),
            0,
            "nothing focused: no context menu applies"
        );
        let boxes = rects_by_class(&window, "box");
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        let (x, y) = (boxes[0].origin.x + 10.0, boxes[0].origin.y + 10.0);
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyDown {
                virtual_keycode: K::Delete,
            },
        );
        step(
            &mut window,
            HeadlessEvent::KeyUp {
                virtual_keycode: K::Delete,
            },
        );
        assert_eq!(
            deleted.load(Ordering::SeqCst),
            1,
            "the focused node's context-menu accelerator runs its callback"
        );
        assert_eq!(saved.load(Ordering::SeqCst), 2, "…and nothing else fired");
    }

    // --- A fired menu chord is the menu's: the DOM does not see its key ---
    //
    // AppKit runs a menu item's key equivalent INSTEAD of delivering the
    // key-down to the view, and Win32's TranslateAccelerator turns it into
    // a WM_COMMAND: the key never reaches the window's key handlers. The
    // shared dispatch ran the item and then dispatched the key-down to the
    // DOM as well, so an app that binds Mod+Z in its menu bar AND in its key
    // handler undid twice on Windows / Linux / headless and once on macOS
    // (AzPhoto's E2E, 2026-10-06: one Cmd+Z, History 3 -> 2 -> 1).

    struct ChordLog {
        saved: Arc<core::sync::atomic::AtomicUsize>,
        keys_seen: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn chord_save(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(s) = data.downcast_ref::<ChordLog>() {
            s.saved.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn chord_key_seen(
        mut data: RefAny,
        info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        let key = info
            .get_current_keyboard_state()
            .current_virtual_keycode
            .into_option();
        if key == Some(azul_core::window::VirtualKeyCode::S) {
            if let Some(s) = data.downcast_ref::<ChordLog>() {
                s.keys_seen.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
            }
        }
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn chord_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            events::{EventFilter, WindowEventFilter},
            menu::{Menu, MenuItem, StringMenuItem},
            window::{VirtualKeyCode as K, VirtualKeyCodeCombo, VirtualKeyCodeVec},
        };

        let log = data.downcast_ref::<ChordLog>().map(|l| ChordLog {
            saved: l.saved.clone(),
            keys_seen: l.keys_seen.clone(),
        });
        let Some(log) = log else {
            return Dom::create_body();
        };
        let state = RefAny::new(log);
        let mut save = StringMenuItem::create("Save".into()).with_callback(
            state.clone(),
            CoreCallback {
                cb: chord_save as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
        );
        save.accelerator = Some(VirtualKeyCodeCombo {
            keys: VirtualKeyCodeVec::from_vec(vec![K::LWin, K::S]),
        })
        .into();
        let file = StringMenuItem::create("File".into())
            .with_children(vec![MenuItem::String(save)].into());
        Dom::create_body()
            .with_menu_bar(Menu::create(vec![MenuItem::String(file)].into()))
            .with_css("width: 100%; height: 100%;")
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Window(WindowEventFilter::VirtualKeyDown),
                    callback: CoreCallback {
                        cb: chord_key_seen as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: state,
                }]
                .into(),
            )
    }

    #[test]
    fn a_fired_menu_chord_does_not_reach_the_windows_key_handlers() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use azul_core::window::VirtualKeyCode as K;

        let log = ChordLog {
            saved: Arc::new(AtomicUsize::new(0)),
            keys_seen: Arc::new(AtomicUsize::new(0)),
        };
        let (saved, keys_seen) = (log.saved.clone(), log.keys_seen.clone());
        let state = Arc::new(RefCell::new(RefAny::new(log)));
        let mut window = make_window_sized(&state, chord_layout, 300.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let primary = if cfg!(target_os = "macos") {
            K::LWin
        } else {
            K::LControl
        };

        // S alone: no chord, the key handler sees it (the premise).
        step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: K::S });
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: K::S });
        assert_eq!(keys_seen.load(Ordering::SeqCst), 1, "premise: a plain S reaches the handler");

        // Mod+S: the menu's Save runs, the key handler does not see the S.
        step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: primary });
        step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: K::S });
        assert_eq!(saved.load(Ordering::SeqCst), 1, "premise: Mod+S runs File > Save");
        assert_eq!(
            keys_seen.load(Ordering::SeqCst),
            1,
            "the chord the menu took never reached the window's key handler"
        );
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: K::S });
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: primary });
    }

    // --- An empty, focused editable shows a caret --------------------------
    //
    // REPORTED (AzWidgets, 2026-08-21): "TextInput not working" — clicking
    // the field hid the placeholder and painted no caret, so the focused
    // field was a blank box. The class: the caret builder anchored the caret
    // to a text cluster, and an editable with NO text has none; with the
    // placeholder hidden on focus there was nothing to see at all.
    //
    // The FIX was the caret, not the prompt: an empty editable now gets the
    // strut caret (content-box origin, one line tall), which is what marks the
    // focused field. The prompt stays HIDDEN on focus — the 2026-08-31 ruling
    // recorded on `focus_writes_no_placeholder_css_at_all`, and the rule
    // TextArea always had. This clicks the real widget and looks for the caret
    // item.

    extern "C" fn empty_text_input_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::text_input::TextInput;
        Dom::create_body().with_css("padding: 20px;").with_child(
            TextInput::create()
                .with_placeholder("Type something...".into())
                .dom(),
        )
    }

    fn caret_items(window: &HeadlessWindow) -> Vec<azul_core::geom::LogicalRect> {
        use azul_core::dom::DomId;
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(lr) = lw.layout_results.get(&DomId { inner: 0 }) else {
            return Vec::new();
        };
        lr.display_list
            .items
            .iter()
            .filter_map(|it| match it {
                DisplayListItem::CursorRect { bounds, .. } => Some(*bounds.inner()),
                _ => None,
            })
            .collect()
    }

    extern "C" fn filled_text_input_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::text_input::TextInput;
        Dom::create_body().with_css("padding: 20px;").with_child(
            TextInput::create()
                .with_text("Hello wonderful world".into())
                .dom(),
        )
    }

    /// Clicking into text puts the caret WHERE YOU CLICKED.
    ///
    /// `process_mouse_click_for_selection` is what resolves a click position to
    /// a text cluster (and what does double-click word / triple-click line
    /// select). It is reached only through `SystemChange::TextSelectionClick`,
    /// so if nothing constructs that change the caret can only ever land at the
    /// start of the field — which looks like "clicking does nothing" once the
    /// field has more than a few characters.
    #[test]
    fn clicking_into_text_places_the_caret_at_the_click() {
        use azul_core::events::MouseButton;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, filled_text_input_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let containers = rects_by_class(&window, "__azul-native-text-input-container");
        assert_eq!(containers.len(), 1, "{containers:?}");
        let c = containers[0];

        // Well inside the text, not at its start: a caret that ignores the
        // click position lands at the content-box origin instead.
        let x = c.origin.x + c.size.width * 0.45;
        let y = c.origin.y + c.size.height * 0.5;
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        window.regenerate_layout().expect("layout after click");

        let carets = caret_items(&window);
        assert!(!carets.is_empty(), "clicking into text must paint a caret");
        let caret = carets[carets.len() - 1];
        assert!(
            caret.origin.x > c.origin.x + 20.0,
            "the caret must land AT THE CLICK ({x}), not at the start of the field: caret \
             {caret:?} in {c:?}"
        );
        assert!(
            caret.origin.x <= x + 20.0,
            "...and not past it: caret {caret:?} for a click at {x}"
        );
    }

    extern "C" fn krug_text_input_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::text_input::TextInput;
        Dom::create_body()
            .with_css("padding: 20px;")
            .with_child(TextInput::create().with_text("krug".into()).dom())
    }

    /// One Shift+Left from the end of a field selects ONE character. E2E-A (2026-10-06): it
    /// selected two in AzContacts' search field (range 4..2 on "krug").
    #[test]
    fn one_shift_left_selects_one_character() {
        use azul_core::events::MouseButton;
        use VirtualKeyCode as K;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, krug_text_input_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let containers = rects_by_class(&window, "__azul-native-text-input-container");
        assert_eq!(containers.len(), 1, "{containers:?}");
        let c = containers[0];
        let (x, y) = (c.origin.x + c.size.width - 4.0, c.origin.y + c.size.height * 0.5);
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
        for key in [K::End] {
            step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: key });
            step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: key });
        }
        let caret = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.document_caret())
            .expect("premise: a caret in the field");
        assert_eq!(caret.text_byte, 4, "premise: the caret at the end of \"krug\"");

        // The chord as ONE state change, the way a script's `key_down` op (and a backend
        // that reports a modifier with its key) delivers it: Shift and Left pressed together.
        window.snapshot_window_state_baseline("test.shift_left");
        {
            let keyboard = window.common.keyboard_state_mut();
            keyboard.pressed_virtual_keycodes.insert_hm_item(K::LShift);
            keyboard.pressed_virtual_keycodes.insert_hm_item(K::Left);
            keyboard.current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::Some(K::Left);
        }
        let tier = window.process_window_events(0);
        if tier > azul_core::events::ProcessEventResult::DoNothing {
            window.service_frame(tier);
        }
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: K::Left });
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: K::LShift });

        let spans = window
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.document_selection_spans())
            .unwrap_or_default();
        let selected: Vec<(u32, u32)> =
            spans.iter().map(|s| (s.start_byte, s.end_byte)).collect();
        assert_eq!(selected, vec![(3, 4)], "one Shift+Left selects \"g\" alone");
    }

    /// The debug server's `key_down` op: Shift and Left pressed in ONE window-state change,
    /// pushed from a callback (`modify_window_state`).
    extern "C" fn shift_left_timer(
        _data: RefAny,
        mut info: azul_layout::timer::TimerCallbackInfo,
    ) -> azul_core::callbacks::TimerCallbackReturn {
        use VirtualKeyCode as K;
        let mut state = info.callback_info.get_current_window_state().clone();
        state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(K::LShift);
        state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(K::Left);
        state.keyboard_state.current_virtual_keycode =
            azul_core::window::OptionVirtualKeyCode::Some(K::Left);
        state.keyboard_state.sync_modifiers();
        info.callback_info.modify_window_state(state);
        azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
    }

    /// [`one_shift_left_selects_one_character`] through a callback's window-state change - the
    /// route a script's `key_down {"shift": true}` takes, where the two characters were
    /// selected.
    #[test]
    fn one_shift_left_pushed_by_a_callback_selects_one_character() {
        use azul_core::events::MouseButton;
        use VirtualKeyCode as K;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, krug_text_input_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let c = rects_by_class(&window, "__azul-native-text-input-container")[0];
        let (x, y) = (c.origin.x + c.size.width - 4.0, c.origin.y + c.size.height * 0.5);
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: K::End });
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: K::End });
        assert_eq!(
            window.common.layout_window.as_ref().and_then(|lw| lw.document_caret()).map(|c| c.text_byte),
            Some(4),
            "premise: the caret at the end of \"krug\""
        );

        let get_time = azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        };
        window.start_timer(
            azul_core::task::TimerId::unique().id,
            azul_layout::timer::Timer::create(
                RefAny::new(()),
                shift_left_timer as azul_layout::timer::TimerCallbackType,
                get_time,
            ),
        );
        for _ in 0..3 {
            window.pump_once(true);
        }
        let selected: Vec<(u32, u32)> = window
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.document_selection_spans())
            .unwrap_or_default()
            .iter()
            .map(|s| (s.start_byte, s.end_byte))
            .collect();
        assert_eq!(selected, vec![(3, 4)], "one Shift+Left selects \"g\" alone");
    }

    /// AzContacts' search field: the hook keeps the query, the field is rebuilt from it.
    struct Query {
        text: String,
    }

    extern "C" fn keep_query(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
        field: azul_layout::widgets::text_input::TextInputState,
    ) -> azul_layout::widgets::text_input::OnTextInputReturn {
        if let Some(mut q) = data.downcast_mut::<Query>() {
            q.text = field.get_text();
        }
        azul_layout::widgets::text_input::OnTextInputReturn {
            update: azul_core::callbacks::Update::RefreshDom,
            valid: azul_layout::widgets::text_input::TextInputValid::Yes,
        }
    }

    extern "C" fn search_field_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::text_input::{TextInput, TextInputOnTextInputCallbackType};
        let text = data.downcast_ref::<Query>().map(|q| q.text.clone()).unwrap_or_default();
        Dom::create_body().with_css("padding: 20px;").with_child(
            TextInput::create_search()
                .with_text(text.as_str().into())
                .with_on_text_input(data.clone(), keep_query as TextInputOnTextInputCallbackType)
                .dom(),
        )
    }

    /// [`one_shift_left_pushed_by_a_callback_selects_one_character`] in a search field whose
    /// text was TYPED and whose hook rebuilds it from the app's query - AzContacts' list
    /// search, where the two characters were selected.
    #[test]
    fn one_shift_left_in_a_typed_search_field_selects_one_character() {
        use azul_core::events::MouseButton;
        use VirtualKeyCode as K;

        let state = Arc::new(RefCell::new(RefAny::new(Query { text: String::new() })));
        let mut window = make_window_sized(&state, search_field_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let c = rects_by_class(&window, "__azul-native-text-input-container")[0];
        let (x, y) = (c.origin.x + 10.0, c.origin.y + c.size.height * 0.5);
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        step(&mut window, HeadlessEvent::MouseDown { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::MouseUp { button: MouseButton::Left });
        step(&mut window, HeadlessEvent::TextInput { text: "krug".to_string() });
        for _ in 0..2 {
            if window.common.take_regeneration() {
                window.regenerate_layout().expect("the rebuild");
            }
        }
        step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: K::End });
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: K::End });
        assert_eq!(
            window.common.layout_window.as_ref().and_then(|lw| lw.document_caret()).map(|c| c.text_byte),
            Some(4),
            "premise: the caret at the end of the typed \"krug\""
        );
        let get_time = azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        };
        window.start_timer(
            azul_core::task::TimerId::unique().id,
            azul_layout::timer::Timer::create(
                RefAny::new(()),
                shift_left_timer as azul_layout::timer::TimerCallbackType,
                get_time,
            ),
        );
        for _ in 0..3 {
            window.pump_once(true);
        }
        let selected: Vec<(u32, u32)> = window
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.document_selection_spans())
            .unwrap_or_default()
            .iter()
            .map(|s| (s.start_byte, s.end_byte))
            .collect();
        assert_eq!(selected, vec![(3, 4)], "one Shift+Left selects \"g\" alone");
    }

    #[test]
    fn an_empty_focused_text_input_shows_a_caret() {
        use azul_core::events::MouseButton;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, empty_text_input_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        assert!(caret_items(&window).is_empty(), "no caret before focus");

        let containers = rects_by_class(&window, "__azul-native-text-input-container");
        assert_eq!(containers.len(), 1, "{containers:?}");
        let c = containers[0];

        // Click into the empty field.
        let (x, y) = (
            c.origin.x + c.size.width * 0.5,
            c.origin.y + c.size.height * 0.5,
        );
        step(&mut window, HeadlessEvent::MouseMove { x, y });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );

        // THE FIELD IS NOT BLANK ON FOCUS. Before, focus hid the placeholder
        // and the empty editable painted no caret, so a focused empty field
        // was a blank box — "the TextInput is not working". The placeholder
        // now stays until the first character (it hides on insert, not on
        // focus), the field keeps a line, and the caret is PAINTED: an empty
        // IFC inside an editing host keeps a strut line box (fc.rs
        // `editing_host_strut_height`), which is the line the caret stands
        // on and the height the value `<p>` keeps.
        assert!(
            c.size.height > 12.0,
            "an empty field keeps a line, not just its chrome: {c:?}"
        );
        // The PROMPT IS HIDDEN ON FOCUS, and that is the current rule — see the
        // 2026-08-31 ruling recorded on `focus_writes_no_placeholder_css_at_all`
        // in widgets/text_input.rs: "focusing an empty field HIDES its
        // placeholder - the rule TextArea always had. The empty-editable strut
        // caret marks the focused field, so the old blank-field concern is
        // gone." The engine implements exactly that
        // (`maybe_paint_placeholder_prompt` paints for an "EMPTY, unfocused"
        // host and returns early on `is_focus_within_or_above`).
        //
        // This assertion used to demand the opposite — the 2026-08-21 reading,
        // from before the strut caret existed — and it looked for a
        // `__azul-native-text-input-placeholder` NODE, which the
        // placeholder-as-engine-attribute refactor deleted. That class now
        // appears nowhere else in the workspace, so the check could only ever
        // find an empty list, whatever the engine painted.
        //
        // What makes the field non-blank is the CARET, asserted below; the
        // whole point of the report this test came from.
        assert!(
            dl_texts(&window).is_empty(),
            "a focused empty field paints no prompt text (2026-08-31 ruling): {:?}",
            dl_texts(&window),
        );
        let carets = caret_items(&window);
        assert!(
            !carets.is_empty(),
            "NO CARET: a focused EMPTY editable must paint the strut caret (the empty value <p> \
             keeps a strut line box inside the editing host)"
        );
        let caret = carets[carets.len() - 1];
        assert!(
            caret.size.height >= 8.0 && caret.size.height <= 40.0,
            "the strut caret is one line tall: {caret:?} in {c:?}"
        );
        assert!(
            caret.origin.x >= c.origin.x - 0.5 && caret.origin.x <= c.origin.x + c.size.width,
            "the caret stands inside the field: {caret:?} in {c:?}"
        );
        assert!(
            caret.origin.y >= c.origin.y - 0.5
                && caret.origin.y + caret.size.height <= c.origin.y + c.size.height + 0.5,
            "…vertically too: {caret:?} in {c:?}"
        );
    }

    // --- A flex item is sized by one authority ------------------------------
    //
    // REPORTED (AzWidgets, 2026-08-21): "multi-line text area bleeding into
    // Slider". The textarea is a flex item with `min-height: 64px`; its
    // container reserved a slot from the item's content (~36 px) while the
    // item itself, re-laid-out as a STANDALONE layout root on every
    // RefreshDom (its fresh dataset allocation fingerprinted as a layout
    // change), applied its min-height — 64 px painted into a 36 px slot,
    // over the widget beneath. Two fixes, one test: the dataset is no longer
    // layout, and a dirty flex item promotes its container to the layout
    // root. The invariant is the class: a flex item's painted box never
    // overlaps the sibling its container placed after it — on the first
    // layout and after a rebuild.

    extern "C" fn text_area_over_next_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::text_area::TextArea;
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; padding: 20px;")
            .with_child(
                Dom::create_div()
                    .with_css("display: flex; flex-direction: column; margin-bottom: 16px;")
                    .with_child(
                        TextArea::create()
                            .with_placeholder("Multi line text area".into())
                            .dom(),
                    ),
            )
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("next".into())].into(),
                    )
                    .with_css("height: 24px;"),
            )
    }

    fn assert_text_area_does_not_overlap_its_next_sibling(window: &HeadlessWindow, when: &str) {
        let area = rects_by_class(window, "__azul-native-text-area-container");
        let next = rects_by_class(window, "next");
        assert_eq!(area.len(), 1, "{when}: {area:?}");
        assert_eq!(next.len(), 1, "{when}: {next:?}");
        let (a, n) = (area[0], next[0]);
        assert!(
            a.size.height >= 63.5,
            "{when}: the textarea's painted box honours its min-height: {a:?}"
        );
        assert!(
            n.origin.y >= a.origin.y + a.size.height - 0.5,
            "{when}: the textarea's painted box ({a:?}) overlaps the sibling its flex container \
             placed after it ({n:?}) — the item was sized by two authorities"
        );
    }

    #[test]
    fn a_flex_items_painted_box_never_overlaps_the_sibling_its_container_placed() {
        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, text_area_over_next_layout, 400.0, 400.0);
        window.regenerate_layout().expect("initial layout");
        assert_text_area_does_not_overlap_its_next_sibling(&window, "first layout");

        // Every RefreshDom rebuilds the widget with a FRESH dataset allocation;
        // that must not make the item a standalone layout root.
        for pass in 0..3 {
            window
                .common
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
            window.regenerate_layout().expect("refresh");
            assert_text_area_does_not_overlap_its_next_sibling(
                &window,
                &format!("after RefreshDom #{pass}"),
            );
        }
    }

    // --- The caret reveal does not fight the wheel ---------------------------
    //
    // REPORTED (AzWidgets, macOS, 2026-09-26/28): type several lines into a
    // TextArea, then scroll up inside it - the view JITTERS back to the caret.
    // The shell's post-callback tail revealed the caret on EVERY pass that was
    // not `prevent_default`ed (`ApplyPendingTextInput` is pushed
    // unconditionally), whether or not an edit landed: every wheel NSEvent,
    // every momentum event and every mouse move. See
    // scripts/TEXT_SCROLL_VS_CARET_REVEAL_ARCHITECTURE_2026_09_26.md §1 and
    // §8 step 1.

    extern "C" fn lone_text_area_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::text_area::TextArea;
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; padding: 20px;")
            .with_child(TextArea::create().dom())
    }

    /// The scroll box the editing session's caret lives in (self-inclusive),
    /// and its vertical offset.
    fn caret_scroll_box(window: &HeadlessWindow) -> Option<(azul_core::dom::DomNodeId, f32)> {
        let lw = window.common.layout_window.as_ref()?;
        let anchor = lw
            .text_edit_manager
            .multi_cursor
            .as_ref()?
            .block
            .container_dom_node();
        let scroller = lw.find_scrollable_ancestor(anchor)?;
        let node = scroller.node.into_crate_internal()?;
        let y = lw.scroll_manager.get_current_offset(scroller.dom, node)?.y;
        Some((scroller, y))
    }

    /// One typed line through the IME / debug-server path (`CreateTextInput`)
    /// and the frame its result asks for.
    fn type_line(window: &mut HeadlessWindow, text: &str) {
        use crate::desktop::shell2::common::event::PlatformWindow;
        window.snapshot_window_state_baseline("headless.test.type_line");
        let r = window.apply_user_change(
            &azul_layout::callbacks::CallbackChange::CreateTextInput { text: text.into() },
        );
        window.service_frame(r);
    }

    #[test]
    fn a_pass_that_lands_no_edit_leaves_the_text_area_where_the_wheel_put_it() {
        use azul_layout::managers::scroll_state::{ScrollInputDevice, ScrollInputSource};

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, lone_text_area_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");

        let area = rects_by_class(&window, "__azul-native-text-area-container");
        assert_eq!(area.len(), 1, "harness: one TextArea in the window: {area:?}");
        let cx = area[0].origin.x + area[0].size.width / 2.0;
        let cy = area[0].origin.y + area[0].size.height / 2.0;
        click_at(&mut window, cx, cy);

        for _ in 0..15 {
            type_line(&mut window, "line\n");
        }
        let (scroller, y_typed) = caret_scroll_box(&window)
            .expect("harness: 15 typed lines overflow the 64px TextArea, which scrolls");
        assert!(
            y_typed > 30.0,
            "harness: the caret reveal followed the typing down, offset {y_typed}"
        );
        let scroller_node = scroller
            .node
            .into_crate_internal()
            .expect("the scroll box is a real node");

        // The hover hit test over the TextArea as it is NOW.
        step(&mut window, HeadlessEvent::MouseMove { x: cx, y: cy + 1.0 });

        // THE WHEEL, the way every backend records it, then the physics
        // timer's commit of 30px up.
        let wanted = y_typed - 30.0;
        if let Some(lw) = window.common.layout_window.as_mut() {
            let now = azul_core::task::Instant::from(Instant::now());
            let _ = lw.scroll_manager.record_scroll_from_hit_test(
                0.0,
                -30.0,
                ScrollInputSource::WheelDiscrete,
                ScrollInputDevice::MouseWheel,
                &lw.hover_manager,
                &azul_layout::managers::hover::InputPointId::Mouse,
                now.clone(),
            );
            lw.scroll_manager.set_scroll_position(
                scroller.dom,
                scroller_node,
                LogicalPosition { x: 0.0, y: wanted },
                now,
            );
        }
        // The pass the macOS wheel handler runs after recording
        // (`handle_scroll_wheel` → `process_window_events(0)`), then a 1px
        // pointer move - neither lands an edit.
        window.snapshot_window_state_baseline("headless.test.wheel_pass");
        let r = window.process_window_events(0);
        if r > azul_core::events::ProcessEventResult::DoNothing {
            window.service_frame(r);
        }
        step(&mut window, HeadlessEvent::MouseMove { x: cx + 1.0, y: cy + 1.0 });

        let now_y = caret_scroll_box(&window).map_or(f32::NAN, |(_, y)| y);
        assert!(
            (now_y - wanted).abs() < 0.5,
            "THE BUG: the wheel put the TextArea at y={wanted}, and a pass that typed nothing \
             hauled it back to y={now_y} (the caret-at-bottom reveal was at y={y_typed})"
        );
        let lw = window.common.layout_window.as_ref().expect("layout window");
        assert!(
            !lw.scroll_manager.reveal_may_move_view(),
            "after the wheel no reveal may move the view until the user types or moves the caret"
        );
        let queued = lw.scroll_manager.scroll_input_queue.take_all();
        assert!(
            !queued
                .iter()
                .any(|input| input.source == ScrollInputSource::AnimateTo),
            "no caret glide may be queued against the wheel: {queued:?}"
        );

        // CONTROL (green before and after): the next keystroke is the last
        // action, and it reveals the caret again.
        type_line(&mut window, "x");
        let back_y = caret_scroll_box(&window).map_or(f32::NAN, |(_, y)| y);
        assert!(
            back_y > wanted + 10.0,
            "a keystroke after the wheel must reveal the caret again: offset {back_y}, wheel left \
             it at {wanted}"
        );
    }

    // --- The patched display list equals the wholesale build -----------------
    //
    // REPORTED (AzWidgets, 2026-08-21): "the placeholder text jumps when I
    // hover over the slider". Two builders produce a frame over the same
    // tree: a PATCHED build (the previous list spliced, each node's items
    // translated by its position delta — every structure-preserved
    // RefreshDom) and a WHOLESALE build (a paint-only restyle such as a
    // `:hover` re-emits everything from the layout results). They disagreed,
    // so the SAME layout painted text in two places depending on which
    // builder ran last. The class invariant is that the two builders agree
    // item for item; `LayoutWindow::verify_patched_display_list` is the
    // checker (also behind `AZ_PATCH_VERIFY=1` for every layout pass), and
    // this drives the report's exact scene — TextArea with a placeholder
    // (an absolutely positioned box whose interior is re-run every pass)
    // plus a Slider — through the passes the report named.

    extern "C" fn text_area_and_slider_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::{slider::Slider, text_area::TextArea};
        let slider_value = data
            .downcast_ref::<SliderUiState>()
            .map_or(40.0, |s| s.slider_value);
        let labelled = |label: &str, class: &str, widget: Dom| {
            Dom::create_div()
                .with_css("display: flex; flex-direction: column; margin-bottom: 16px;")
                .with_child(
                    Dom::create_span_with_text(label)
                        .with_ids_and_classes(
                            vec![azul_core::dom::IdOrClass::Class(class.into())].into(),
                        )
                        .with_css("font-size: 13px; margin-bottom: 4px;"),
                )
                .with_child(widget)
        };
        Dom::create_body()
            .with_css("display: flex; flex-direction: column; padding: 20px;")
            .with_child(labelled(
                "TextArea",
                "cap-ta",
                TextArea::create()
                    .with_placeholder("Multi line text area".into())
                    .dom(),
            ))
            .with_child(labelled(
                "Slider",
                "cap-slider",
                Slider::create(slider_value, 0.0, 100.0)
                    .with_on_value_change(
                        data.clone(),
                        azul_layout::widgets::slider::SliderOnValueChangeCallback {
                            cb: slider_demo_on_change,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                    )
                    .dom(),
            ))
    }

    /// The rects the report watched: textarea box, its placeholder, the
    /// caption under it (the slider's label).
    fn watched_rects(
        window: &HeadlessWindow,
        when: &str,
    ) -> (azul_core::geom::LogicalRect, azul_core::geom::LogicalRect) {
        // The textarea's PROMPT is deliberately not watched here. It used to
        // be the third rect, found by class — but the
        // placeholder-as-engine-attribute refactor deleted that node, and
        // `__azul-native-text-area-placeholder` now appears nowhere in the
        // workspace, so the lookup could only ever return `[]`.
        //
        // Nothing is lost: the prompt is PAINTED by the engine, and
        // `assert_builders_agree` compares the WHOLE display list between the
        // patched and wholesale builders — so the prompt's own glyphs are
        // checked directly, which is stronger than watching a box that used to
        // contain them.
        let ta = rects_by_class(window, "__azul-native-text-area-container");
        let cap = rects_by_class(window, "cap-slider");
        assert_eq!(ta.len(), 1, "{when}: {ta:?}");
        assert_eq!(cap.len(), 1, "{when}: {cap:?}");
        (ta[0], cap[0])
    }

    fn assert_builders_agree(window: &mut HeadlessWindow, when: &str) {
        let lw = window.common.layout_window.as_mut().expect("layout window");
        let was_patched = lw.layout_cache.last_build_was_patched;
        let mismatches = lw.verify_patched_display_list(azul_core::dom::DomId::ROOT_ID);
        assert!(
            mismatches.is_empty(),
            "{when} (patched build = {was_patched}): the PATCHED display list differs from the \
             WHOLESALE build of the same layout results — two builders, two pictures:\n{}",
            mismatches.join("\n")
        );
    }

    #[test]
    fn the_patched_display_list_equals_the_wholesale_build_for_the_widgets_scene() {
        use azul_core::events::MouseButton;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(SliderUiState {
            slider_value: 40.0,
            interactions: 0,
        })));
        let mut window = make_window_sized(&state, text_area_and_slider_layout, 400.0, 400.0);
        window.regenerate_layout().expect("initial layout");
        let first = watched_rects(&window, "first layout");
        // The second pass is the structure-preserved one: a PATCHED build.
        window.regenerate_layout().expect("settle");
        let second = watched_rects(&window, "settle");
        assert_eq!(
            first, second,
            "pass 1 and pass 2 must place the textarea and the caption identically"
        );
        assert_builders_agree(&mut window, "after the settle pass");

        // Hover the slider thumb (a paint-only restyle: wholesale re-emit),
        // then a RefreshDom rebuild under that hover (a patched build).
        let thumbs = rects_by_class(&window, "__azul-native-slider-thumb");
        assert_eq!(thumbs.len(), 1, "{thumbs:?}");
        let t = thumbs[0];
        step(
            &mut window,
            HeadlessEvent::MouseMove {
                x: t.origin.x + t.size.width / 2.0,
                y: t.origin.y + t.size.height / 2.0,
            },
        );
        let hovered = watched_rects(&window, "hovering the thumb");
        assert_eq!(
            second, hovered,
            "a hover on the slider must not move the textarea or the caption"
        );
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        window.regenerate_layout().expect("RefreshDom under hover");
        assert_eq!(second, watched_rects(&window, "RefreshDom under hover"));
        assert_builders_agree(&mut window, "after a RefreshDom under the thumb hover");

        // Drag the slider (the widget's own RefreshDom + dataset merge), then
        // hover the textarea itself (its :hover border) — every frame agrees.
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseMove {
                x: t.origin.x + 40.0,
                y: t.origin.y + t.size.height / 2.0,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        assert_eq!(second, watched_rects(&window, "after the drag"));
        assert_builders_agree(&mut window, "after the slider drag");
        let ta = second.0;
        step(
            &mut window,
            HeadlessEvent::MouseMove {
                x: ta.origin.x + 10.0,
                y: ta.origin.y + 10.0,
            },
        );
        assert_eq!(second, watched_rects(&window, "hovering the textarea"));
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        window
            .regenerate_layout()
            .expect("RefreshDom under the textarea hover");
        assert_eq!(
            second,
            watched_rects(&window, "RefreshDom under the textarea hover")
        );
        assert_builders_agree(&mut window, "after a RefreshDom under the textarea hover");
    }

    // --- The pressed node gets its release ------------------------------------
    //
    // REPORTED (demo test 2026-08-21, the whole "stuck input" family): a
    // slider drag that died off the thumb, a map pan cut at a tile edge, a
    // paint stroke that kept painting on plain moves. Events are derived
    // from window-state diffs and targeted at whatever is under the pointer
    // NOW, so a widget that latched on MouseDown only saw its MouseUp if the
    // pointer was still over it. Press-target capture: the pressed node gets
    // the release wherever the pointer is — and its ancestors do not see it
    // twice.

    #[derive(Debug, Clone)]
    struct PressLog {
        a_down: Arc<core::sync::atomic::AtomicUsize>,
        a_up: Arc<core::sync::atomic::AtomicUsize>,
        b_up: Arc<core::sync::atomic::AtomicUsize>,
        body_up: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn press_log_bump(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(counter) = refany.downcast_ref::<Arc<core::sync::atomic::AtomicUsize>>() {
            counter.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    fn counting_callback(
        filter: azul_core::events::HoverEventFilter,
        counter: &Arc<core::sync::atomic::AtomicUsize>,
    ) -> azul_core::callbacks::CoreCallbackData {
        use azul_core::callbacks::{CoreCallback, CoreCallbackData};
        CoreCallbackData {
            event: azul_core::events::EventFilter::Hover(filter),
            callback: CoreCallback {
                cb: press_log_bump as usize,
                ctx: azul_core::refany::OptionRefAny::None,
            },
            refany: RefAny::new(counter.clone()),
        }
    }

    extern "C" fn press_capture_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::events::HoverEventFilter;
        let log = data
            .downcast_ref::<PressLog>()
            .map(|l| l.clone())
            .expect("press log");
        Dom::create_body()
            .with_css("display: flex; flex-direction: row; width: 100%; height: 100%;")
            .with_callbacks(vec![counting_callback(HoverEventFilter::MouseUp, &log.body_up)].into())
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("a".into())].into())
                    .with_css("width: 100px; height: 100px;")
                    .with_callbacks(
                        vec![
                            counting_callback(HoverEventFilter::MouseDown, &log.a_down),
                            counting_callback(HoverEventFilter::MouseUp, &log.a_up),
                        ]
                        .into(),
                    ),
            )
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class("b".into())].into())
                    .with_css("width: 100px; height: 100px;")
                    .with_callbacks(
                        vec![counting_callback(HoverEventFilter::MouseUp, &log.b_up)].into(),
                    ),
            )
    }

    #[test]
    fn the_pressed_node_gets_its_release_wherever_the_pointer_let_go() {
        use core::sync::atomic::{AtomicUsize, Ordering};

        use azul_core::events::MouseButton;

        let log = PressLog {
            a_down: Arc::new(AtomicUsize::new(0)),
            a_up: Arc::new(AtomicUsize::new(0)),
            b_up: Arc::new(AtomicUsize::new(0)),
            body_up: Arc::new(AtomicUsize::new(0)),
        };
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, press_capture_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");

        // Press on A, drag onto B, release there.
        step(&mut window, HeadlessEvent::MouseMove { x: 50.0, y: 50.0 });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        assert_eq!(log.a_down.load(Ordering::SeqCst), 1);
        step(&mut window, HeadlessEvent::MouseMove { x: 150.0, y: 50.0 });
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );

        assert_eq!(
            log.a_up.load(Ordering::SeqCst),
            1,
            "the node that was PRESSED must get the release although the pointer let go over B"
        );
        assert_eq!(
            log.b_up.load(Ordering::SeqCst),
            1,
            "the hovered node's release is unchanged"
        );
        assert_eq!(
            log.body_up.load(Ordering::SeqCst),
            1,
            "the shared ancestor sees the release ONCE — the captured release is at-target-only"
        );

        // Press and release on the same node: exactly one release, as before.
        step(&mut window, HeadlessEvent::MouseMove { x: 50.0, y: 50.0 });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            &mut window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );
        assert_eq!(log.a_up.load(Ordering::SeqCst), 2);
        assert_eq!(log.body_up.load(Ordering::SeqCst), 2);
    }

    // --- Indicator marks are centred ------------------------------------
    //
    // REPORTED (demo test 2026-08-21): "CheckBox not centered" — the 8 px mark
    // sat in the top-left of the box. The class: an indicator widget (check
    // box, radio dot, switch knob) positions its mark with its CONTAINER's
    // layout, and a container that is not a centring flex box parks the mark
    // at its content origin. This lays the indicator widgets out for real
    // and measures, so a container style edit that drops the centring fails
    // here rather than in a screenshot.

    extern "C" fn indicator_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::{check_box::CheckBox, radio_group::RadioGroup, switch::Switch};
        let options: azul_css::StringVec =
            vec![azul_css::AzString::from("a"), azul_css::AzString::from("b")].into();
        Dom::create_body()
            .with_child(CheckBox::create(true).dom())
            .with_child(RadioGroup::create(options).dom())
            .with_child(Switch::create(true).dom())
    }

    #[test]
    fn indicator_marks_are_centred_in_their_boxes() {
        fn centre(r: &azul_core::geom::LogicalRect) -> (f32, f32) {
            (
                r.origin.x + r.size.width / 2.0,
                r.origin.y + r.size.height / 2.0,
            )
        }
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, indicator_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        // (widget, container class, mark class, horizontally centred too?)
        // A switch knob sits at one END of its track by design; only its
        // vertical centring is a layout invariant.
        for (widget, container, mark, both_axes) in [
            (
                "CheckBox",
                "__azul-native-checkbox-container",
                "__azul-native-checkbox-content",
                true,
            ),
            (
                "RadioGroup",
                "__azul-native-radio-group-circle",
                "__azul-native-radio-group-dot",
                true,
            ),
            (
                "Switch",
                "__azul-native-switch",
                "__azul-native-switch-knob",
                false,
            ),
        ] {
            let boxes = rects_by_class(&window, container);
            let marks = rects_by_class(&window, mark);
            assert!(
                !boxes.is_empty() && boxes.len() == marks.len(),
                "{widget}: {} boxes vs {} marks ({boxes:?} / {marks:?})",
                boxes.len(),
                marks.len()
            );
            for (b, m) in boxes.iter().zip(marks.iter()) {
                let (bx, by) = centre(b);
                let (mx, my) = centre(m);
                assert!(
                    (by - my).abs() <= 0.5,
                    "{widget}: mark {m:?} is not vertically centred in its box {b:?}"
                );
                if both_axes {
                    assert!(
                        (bx - mx).abs() <= 0.5,
                        "{widget}: mark {m:?} is not horizontally centred in its box {b:?}"
                    );
                }
            }
        }
    }

    // --- The CSS animation driver's frame rate ----------------------------
    //
    // REPORTED: "the toggle animates smooth on macOS but not on wayland or
    // x11". The switch's knob (its `transform`) and track glide through a
    // declared 150 ms `animation`, which the CPU renderer advances with the CSS animation
    // driver (`CSS_ANIMATION_TIMER_ID`, 16 ms). Its callback is an inert
    // marker, and `invoke_expired_timers` counts it as fired whenever it is
    // REGISTERED. macOS and Windows reach `process_timers_and_threads` only
    // when an OS timer fires; X11 and Wayland reach it on every pass through
    // their loops as well, and there the knob stepped several times a frame.

    extern "C" fn switch_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::switch::Switch;
        Dom::create_body().with_child(Switch::create(false).dom())
    }

    /// The first laid-out node carrying `class`.
    fn node_with_class(window: &HeadlessWindow, class: &str) -> Option<azul_core::dom::NodeId> {
        use azul_core::dom::{DomId, IdOrClass, NodeId};

        let lw = window.common.layout_window.as_ref()?;
        let dom = lw.layout_results.get(&DomId::ROOT_ID)?;
        dom.styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|data| {
                data.get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class))
            })
            .map(NodeId::new)
    }

    /// A switch whose knob was just toggled on - the write the switch's click
    /// handler makes - with the driver armed, on a FROZEN test clock that the
    /// caller resets.
    fn toggled_switch_window() -> HeadlessWindow {
        use azul_core::dom::DomId;
        use azul_css::props::{
            basic::PixelValue,
            property::CssProperty,
            style::{StyleTransform, StyleTransformVec},
        };
        use azul_layout::overlay::ContentChange;

        azul_core::task::reset_test_clock();
        azul_core::task::freeze_test_clock();
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_with(&state, switch_layout);
        window.regenerate_layout().expect("initial layout");
        let knob =
            node_with_class(&window, "__azul-native-switch-knob").expect("the switch has a knob");
        if let Some(lw) = window.common.layout_window.as_mut() {
            let _ = lw.apply_content_change(ContentChange::NodeCss {
                dom_id: DomId::ROOT_ID,
                node_id: knob,
                props: vec![CssProperty::const_transform(StyleTransformVec::from_vec(
                    vec![StyleTransform::TranslateX(PixelValue::const_px(16))],
                ))],
                override_only: false,
            });
        }
        window.arm_animation_drivers_if_needed();
        window
    }

    /// How far the knob's glide has come, 0..=1 (`None` once it settled).
    fn knob_glide_progress(window: &HeadlessWindow) -> Option<f32> {
        use azul_css::props::property::CssPropertyType;

        window
            .common
            .layout_window
            .as_ref()?
            .css_transitions
            .iter()
            .find(|tr| tr.prop_type == CssPropertyType::Transform)
            .map(|tr| tr.t)
    }

    #[test]
    fn css_driver_steps_once_per_timer_period_not_per_pass() {
        use azul_core::task::CSS_ANIMATION_TIMER_ID;

        let mut window = toggled_switch_window();
        let armed = window
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.timers.contains_key(&CSS_ANIMATION_TIMER_ID));

        let _ = azul_core::task::advance_test_clock_ms(16);
        let first = window.process_timers_and_threads();
        let progress = knob_glide_progress(&window);
        // X11 and Wayland run the pass again in the same loop turn.
        let again = window.process_timers_and_threads();
        let progress_again = knob_glide_progress(&window);
        azul_core::task::reset_test_clock();

        assert!(
            armed,
            "harness: the toggle seeds a glide and arms the driver"
        );
        assert!(
            first,
            "harness: the driver's first frame steps the knob and owes a redraw"
        );
        assert!(
            !again,
            "a second pass in the same instant is not a frame: it must neither step the glide \
             again nor owe a relayout and a redraw (progress {progress:?} -> {progress_again:?})"
        );
    }

    /// The other half of the driver's frame rule: a LATE wake must not cost
    /// the next frame. An OS timer keeps its schedule, so a wake delayed by a
    /// slow frame is followed by an on-time one only a few ms later - an
    /// `NSTimer` does exactly that behind a slow render - and both are frames.
    #[test]
    fn css_driver_steps_on_an_on_time_wake_after_a_late_one() {
        // The driver paces at the window's monitor rate: 60 Hz here, one
        // period = 16.67 ms, so its wakes are due at 16.7, 33.3 and 50 ms.
        let mut window = toggled_switch_window();
        let _ = azul_core::task::advance_test_clock_ms(17);
        let first = window.process_timers_and_threads();
        // The 33 ms wake comes 10 ms late (43 ms) ...
        let _ = azul_core::task::advance_test_clock_ms(26);
        let late = window.process_timers_and_threads();
        let progress_late = knob_glide_progress(&window);
        // ... and the 50 ms one on time, 8 ms after it: less than half a
        // period, but in a later period of the driver's schedule.
        let _ = azul_core::task::advance_test_clock_ms(8);
        let on_time = window.process_timers_and_threads();
        let progress_on_time = knob_glide_progress(&window);
        azul_core::task::reset_test_clock();

        assert!(
            first && late,
            "harness: the first wake and the late one both step the knob"
        );
        assert!(
            on_time,
            "the on-time wake after a late one is a frame of its own and must step the knob \
             (progress {progress_late:?} -> {progress_on_time:?})"
        );
    }

    /// A DOM rebuild is not animation time.
    ///
    /// In a real app the switch's toggle returns `RefreshDom`, and X11 runs
    /// the timer pass at the top of its loop turn, BEFORE the frame path
    /// rebuilds the DOM: the driver's first frame lands ahead of the rebuild,
    /// the whole rebuild then falls inside the step of the second one, and the
    /// knob jumps a large part of its glide at once. The rebuild's duration is
    /// modelled by advancing the frozen clock ahead of it.
    #[test]
    fn a_dom_rebuild_between_two_driver_frames_is_not_animation_time() {
        let mut window = toggled_switch_window();
        let _ = azul_core::task::advance_test_clock_ms(16);
        let first = window.process_timers_and_threads();
        let after_first = knob_glide_progress(&window);
        // The toggle's rebuild runs next and takes 60 ms ...
        let _ = azul_core::task::advance_test_clock_ms(60);
        let rebuilt = window.regenerate_layout().is_ok();
        // ... and the driver's next frame follows one period later.
        let _ = azul_core::task::advance_test_clock_ms(16);
        let second = window.process_timers_and_threads();
        let after_second = knob_glide_progress(&window);
        azul_core::task::reset_test_clock();

        assert!(
            first && rebuilt && second,
            "harness: two driver frames with a rebuild between them (first {first}, rebuilt \
             {rebuilt}, second {second})"
        );
        let (Some(before), Some(after)) = (after_first, after_second) else {
            panic!(
                "harness: the glide is in flight across the rebuild ({after_first:?} -> \
                 {after_second:?})"
            );
        };
        // A frame is 1/9 of the knob's 150 ms glide. The frame after the
        // rebuild may take the rebuild's last frame and its own, 2/9 - not
        // the 76 ms that passed since the first one.
        assert!(
            after - before <= 2.0 / 9.0 + 0.01,
            "the frame after a 60 ms rebuild advanced the glide from {before} to {after}, i.e. \
             by {} of it; at most two frames' worth (0.22) may land in one step",
            after - before
        );
    }

    // --- The long-press wake-up -------------------------------------------
    //
    // A motionless press produces no input events, so a MouseDown arms a
    // one-shot marker timer (`LONG_PRESS_TIMER_ID`, threshold + 15 ms) whose
    // expiry runs the event pass that evaluates `detect_long_press` (MWA-B12).
    // The marker was built with an INTERVAL and no delay, and `Timer::invoke`
    // admits a timer that never ran at once unless it has a delay - so the
    // first timer pass of any kind spent it, and `invoke_expired_timers` ran
    // the long-press pass merely because the marker was registered. X11 and
    // Wayland run a timer pass at the top of every loop turn: there the
    // wake-up was spent on the turn right after the press, long before the
    // threshold, and nothing woke the loop when the threshold came.

    #[derive(Debug, Clone)]
    struct LongPressLog {
        hits: Arc<core::sync::atomic::AtomicUsize>,
    }

    extern "C" fn count_long_press(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        if let Some(log) = refany.downcast_ref::<LongPressLog>() {
            log.hits.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }
        azul_core::callbacks::Update::DoNothing
    }

    /// A 300x200 box that counts the `LongPress` events it receives.
    extern "C" fn long_press_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            events::{EventFilter, HoverEventFilter},
        };
        let log = data
            .downcast_ref::<LongPressLog>()
            .map(|l| l.clone())
            .expect("long-press log");
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css("width: 300px; height: 200px;")
                .with_callbacks(
                    vec![CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::LongPress),
                        callback: CoreCallback {
                            cb: count_long_press as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: RefAny::new(log),
                    }]
                    .into(),
                ),
        )
    }

    fn long_press_log() -> LongPressLog {
        LongPressLog {
            hits: Arc::new(core::sync::atomic::AtomicUsize::new(0)),
        }
    }

    fn long_press_wake_armed(window: &HeadlessWindow) -> bool {
        window
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.timers.contains_key(&azul_core::task::LONG_PRESS_TIMER_ID))
    }

    #[test]
    fn a_timer_pass_before_the_threshold_does_not_spend_the_long_press_wake() {
        use azul_core::events::MouseButton;

        // Frozen from before the press, so the wake-up's own schedule is a
        // pure function of the ticks below, whatever this machine's speed.
        azul_core::task::reset_test_clock();
        azul_core::task::freeze_test_clock();
        let state = Arc::new(RefCell::new(RefAny::new(long_press_log())));
        let mut window = make_window_sized(&state, long_press_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        step(&mut window, HeadlessEvent::MouseMove { x: 150.0, y: 100.0 });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        let armed_at_press = long_press_wake_armed(&window);

        // The pass X11 and Wayland run on the very next loop turn ...
        let _ = window.process_timers_and_threads();
        let armed_after_same_instant = long_press_wake_armed(&window);
        // ... and one a few ms later (another timer, a thread tick).
        let _ = azul_core::task::advance_test_clock_ms(5);
        let _ = window.process_timers_and_threads();
        let armed_after_5ms = long_press_wake_armed(&window);
        azul_core::task::reset_test_clock();

        assert!(
            armed_at_press,
            "harness: a press arms the long-press wake-up"
        );
        assert!(
            armed_after_same_instant && armed_after_5ms,
            "a timer pass 0 ms and 5 ms into a 500 ms press must leave the long-press wake-up \
             armed for its threshold; still armed after the same-instant pass: \
             {armed_after_same_instant}, after the +5 ms pass: {armed_after_5ms}"
        );
    }

    /// A press held perfectly still fires `LongPress` once the wake-up comes,
    /// and only once.
    ///
    /// The hold is REAL time, on a short threshold: every gesture sample is
    /// stamped by the wall clock, not the test clock.
    #[test]
    fn a_motionless_press_fires_long_press_once_past_its_threshold() {
        use core::sync::atomic::Ordering;

        use azul_core::events::MouseButton;

        azul_core::task::reset_test_clock();
        let log = long_press_log();
        let state = Arc::new(RefCell::new(RefAny::new(log.clone())));
        let mut window = make_window_sized(&state, long_press_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        // 30 ms, so the wake-up is due 45 ms after the press.
        if let Some(lw) = window.common.layout_window.as_mut() {
            lw.gesture_drag_manager.config.long_press_time_threshold_ms = 30;
        }
        step(&mut window, HeadlessEvent::MouseMove { x: 150.0, y: 100.0 });
        step(
            &mut window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        let at_press = log.hits.load(Ordering::SeqCst);

        // Perfectly still, past the threshold and the wake-up's margin.
        std::thread::sleep(std::time::Duration::from_millis(80));
        let _ = window.process_timers_and_threads();
        let after_wake = log.hits.load(Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let _ = window.process_timers_and_threads();
        let later = log.hits.load(Ordering::SeqCst);

        assert_eq!(at_press, 0, "harness: a fresh press is not a long press");
        assert_eq!(
            after_wake, 1,
            "a press held still for 80 ms past a 30 ms threshold must fire LongPress when the \
             wake-up runs its pass"
        );
        assert_eq!(later, 1, "a delivered long press must not fire again");
    }

    // --- Ribbon tab switching -------------------------------------------
    //
    // REPORTED: "clicking on various tabs causes repaint / damage rect
    // issues, i.e. it's as if the damage rects have a NodeID issue and take
    // the damage rect from a different node (anonymous nodes?)".
    //
    // A tab click is the hardest case a real app hands the incremental
    // path: the callback returns RefreshDom, the content band is REPLACED
    // by a different tab's groups (a different node COUNT, so NodeIds
    // shift), and two tab headers swap their active/inactive style.
    //
    // The harness puts a large, deliberately UNCHANGING document block
    // below the ribbon as a control surface.

    #[derive(Debug, Clone)]
    struct RibbonUiState {
        active_tab: usize,
        /// Give every tab header its own border colour (`RibbonTab::style`).
        /// Off for the damage tests so the strip stays uniform; on for the
        /// test that pins what the aid exposed.
        bordered: bool,
    }

    /// Desktop viewport: at 400x300 the ribbon renders its MOBILE variant,
    /// which has no tab strip to click.
    const RIBBON_W: f32 = 1000.0;
    const RIBBON_H: f32 = 600.0;

    extern "C" fn on_ribbon_tab(
        mut refany: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
        idx: usize,
    ) -> azul_core::callbacks::Update {
        if let Some(mut s) = refany.downcast_mut::<RibbonUiState>() {
            s.active_tab = idx;
        }
        azul_core::callbacks::Update::RefreshDom
    }

    extern "C" fn ribbon_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
            props::{
                basic::color::ColorU,
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
                style::{
                    background::{StyleBackgroundContent, StyleBackgroundContentVec},
                    border::{
                        BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth,
                        LayoutBorderRightWidth, LayoutBorderTopWidth, StyleBorderBottomColor,
                        StyleBorderBottomStyle, StyleBorderLeftColor, StyleBorderLeftStyle,
                        StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
                        StyleBorderTopStyle,
                    },
                },
            },
            AzString,
        };
        use azul_layout::widgets::ribbon::{
            Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec,
        };

        let (active, bordered) = data
            .downcast_ref::<RibbonUiState>()
            .map(|s| (s.active_tab, s.bordered))
            .unwrap_or((0, false));

        let tab_border = |r: u8, g: u8, b: u8| {
            if !bordered {
                return CssPropertyWithConditionsVec::from_vec(Vec::new());
            }
            let c = ColorU { r, g, b, a: 255 };
            CssPropertyWithConditionsVec::from_vec(vec![
                CssPropertyWithConditions::simple(CssProperty::border_top_style(
                    StyleBorderTopStyle {
                        inner: BorderStyle::Solid,
                    },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_right_style(
                    StyleBorderRightStyle {
                        inner: BorderStyle::Solid,
                    },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_bottom_style(
                    StyleBorderBottomStyle {
                        inner: BorderStyle::Solid,
                    },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_left_style(
                    StyleBorderLeftStyle {
                        inner: BorderStyle::Solid,
                    },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_top_width(
                    LayoutBorderTopWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_right_width(
                    LayoutBorderRightWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_bottom_width(
                    LayoutBorderBottomWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_left_width(
                    LayoutBorderLeftWidth::const_px(2),
                )),
                CssPropertyWithConditions::simple(CssProperty::border_top_color(
                    StyleBorderTopColor { inner: c },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_right_color(
                    StyleBorderRightColor { inner: c },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_bottom_color(
                    StyleBorderBottomColor { inner: c },
                )),
                CssPropertyWithConditions::simple(CssProperty::border_left_color(
                    StyleBorderLeftColor { inner: c },
                )),
            ])
        };

        // Different group counts and label lengths per tab, so the replaced
        // subtree differs in node count AND painted extent - a rect carried
        // over from the previous tab cannot happen to be the right size.
        let tab = |label: &str, groups: usize, btns: usize| {
            let mut t = RibbonTab::new(AzString::from(label));
            for g in 0..groups {
                let mut grp = RibbonGroup::new(AzString::from(format!("{label}-G{g}")));
                for b in 0..btns {
                    grp.add_item(RibbonItem::LargeButton(RibbonButton::new(
                        AzString::from(""),
                        AzString::from(format!("{label}{g}{b}")),
                    )));
                }
                t.add_group(grp);
            }
            t
        };

        // Each tab gets a DIFFERENT border colour (RibbonTab::style, the
        // per-tab hook). This is the debug aid the reported artifact needed:
        // with the strip painted in one colour, a rect attributed to the
        // wrong tab is invisible, and telling two tabs apart in a frame dump
        // meant reading their labels.
        let ribbon = Ribbon::new(RibbonTabVec::from_vec(vec![
            tab("HOME", 3, 2).with_style(tab_border(255, 0, 0)),
            tab("INSERT", 1, 4).with_style(tab_border(0, 160, 0)),
            tab("DESIGN", 2, 1).with_style(tab_border(0, 0, 255)),
        ]))
        .with_active_tab(active)
        .with_on_tab_click(
            data.clone(),
            on_ribbon_tab as azul_layout::widgets::ribbon::RibbonOnTabClickCallbackType,
        );

        let doc = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(RIBBON_W))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(RIBBON_H))),
                CssPropertyWithConditions::simple(CssProperty::background_content(
                    StyleBackgroundContentVec::from(vec![StyleBackgroundContent::Color(ColorU {
                        r: 0,
                        g: 128,
                        b: 0,
                        a: 255,
                    })]),
                )),
            ]
            .into(),
        );

        Dom::create_body().with_child(ribbon.dom()).with_child(doc)
    }

    /// On-screen rects of every laid-out node carrying `class`, left to right.
    fn rects_by_class(window: &HeadlessWindow, class: &str) -> Vec<azul_core::geom::LogicalRect> {
        use azul_core::{
            dom::{DomId, DomNodeId, IdOrClass},
            geom::LogicalRect,
            styled_dom::NodeHierarchyItemId,
        };

        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(dom) = lw.layout_results.get(&DomId { inner: 0 }) else {
            return Vec::new();
        };
        let mut found: Vec<LogicalRect> = Vec::new();
        for (node_id, data) in dom
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .enumerate()
        {
            let matches = data.get_ids_and_classes().iter().any(|c| match c {
                IdOrClass::Class(s) => s.as_str() == class,
                IdOrClass::Id(_) => false,
            });
            if !matches {
                continue;
            }
            let dnid = DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::from_crate_internal(Some(azul_core::dom::NodeId::new(
                    node_id,
                ))),
            };
            // A tab header has a MouseUp callback, so it has a tag and a
            // HitTestArea; the ribbon root does not, so fall back to the
            // layout box. A node in the DOM but NOT in the layout (the
            // mobile variant on a desktop viewport) yields neither.
            let bounds = lw.get_node_hit_test_bounds(dnid).or_else(|| {
                Some(LogicalRect {
                    origin: lw.get_node_position(dnid)?,
                    size: lw.get_node_size(dnid)?,
                })
            });
            if let Some(r) = bounds {
                found.push(r);
            }
        }
        found.sort_by(|a, b| a.origin.x.total_cmp(&b.origin.x));
        found
    }

    fn tab_header_centre(window: &HeadlessWindow, idx: usize) -> Option<(f32, f32)> {
        let r = *rects_by_class(window, "__azul-native-ribbon-tab").get(idx)?;
        Some((
            r.origin.x + r.size.width / 2.0,
            r.origin.y + r.size.height / 2.0,
        ))
    }

    /// Bottom edge of the ribbon as LAID OUT; below it is the document area.
    /// Measured, not hard-coded: a constant that drifts past the real edge
    /// turns the control surface into a no-op.
    fn ribbon_bottom(window: &HeadlessWindow) -> f32 {
        rects_by_class(window, "__azul-native-ribbon")
            .iter()
            .map(|r| r.origin.y + r.size.height)
            .fold(0.0_f32, f32::max)
    }

    /// The app-side tab index (`RefAny::downcast_ref` needs `&mut self`).
    fn active_tab_of(state: &Arc<RefCell<RefAny>>) -> Option<usize> {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        r.downcast_ref::<RibbonUiState>().map(|s| s.active_tab)
    }

    fn click_at(window: &mut HeadlessWindow, x: f32, y: f32) -> FrameDamage {
        use azul_core::events::MouseButton;
        step(window, HeadlessEvent::MouseMove { x, y });
        step(
            window,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        step(
            window,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        )
    }

    /// Text items of a window's display list, as debug strings.
    fn dl_texts(window: &HeadlessWindow) -> Vec<String> {
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(r) = lw.layout_results.get(&azul_core::dom::DomId { inner: 0 }) else {
            return Vec::new();
        };
        r.display_list
            .items
            .iter()
            .filter_map(|i| match i {
                DisplayListItem::Text { .. } => Some(format!("{i:?}")),
                _ => None,
            })
            .collect()
    }

    /// Build the pair the tab-switch tests compare: a window that reached
    /// `active_tab = 1` by CLICKING, and a fresh one that started there.
    fn switched_and_fresh() -> (HeadlessWindow, HeadlessWindow, f32, f32, FrameDamage) {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 0,
            bordered: false,
        })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let Some((x, y)) = tab_header_centre(&window, 1) else {
            panic!("no ribbon tab headers in the layout - the harness built the wrong DOM");
        };
        let damage = click_at(&mut window, x, y);
        window.regenerate_layout().expect("post-click relayout");
        assert_eq!(
            active_tab_of(&state),
            Some(1),
            "the click at ({x}, {y}) did not reach the tab header, so every comparison below \
             would pass for the wrong reason"
        );

        // The same state painted from scratch. The cursor has to sit where
        // it sits in the window under test: the tab under the pointer draws
        // in its hover style, so a fresh window with the mouse elsewhere is
        // a DIFFERENT picture and the comparison would report hover as
        // staleness.
        let fresh_state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 1,
            bordered: false,
        })));
        let mut fresh = make_window_sized(&fresh_state, ribbon_layout, RIBBON_W, RIBBON_H);
        fresh.regenerate_layout().expect("fresh layout");
        step(&mut fresh, HeadlessEvent::MouseMove { x, y });
        fresh
            .regenerate_layout()
            .expect("fresh layout under the cursor");

        (window, fresh, x, y, damage)
    }

    /// A tab click must not leave the layout tree describing a DOM that no
    /// longer exists.
    ///
    /// This is the regression for the reconciliation identity bug: switching
    /// from a 3-group tab to a 1-group tab SHRINKS the DOM, reconciliation
    /// falls back to positional matching for the children whose ids moved,
    /// and `clone_node_from_old` used to carry the OLD node's `dom_node_id`
    /// into the new tree. `assert_dom_ids_are_in_range` (solver3::cache)
    /// fires on the resulting tree; before the fix this panicked with
    /// "index out of bounds: the len is 37 but the index is 52" inside
    /// `compute_counters`.
    ///
    /// NEGATIVE CONTROL: reverting `clone_node_from_old` to copy
    /// `old_node.dom_node_id` makes this panic - verified.
    #[test]
    fn switching_ribbon_tabs_keeps_the_layout_tree_addressing_the_live_dom() {
        let (window, _fresh, _x, _y, _damage) = switched_and_fresh();

        // Switch on to a tab that GROWS the tree again, then back: both
        // directions of the size change go through the positional fallback.
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 1,
            bordered: false,
        })));
        let mut w2 = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        w2.regenerate_layout().expect("layout");
        for target in [0usize, 2, 1, 0] {
            let Some((x, y)) = tab_header_centre(&w2, target) else {
                panic!("tab {target} has no header");
            };
            click_at(&mut w2, x, y);
            w2.regenerate_layout().expect("relayout after tab switch");
            assert_eq!(active_tab_of(&state), Some(target));
        }

        // The frames exist and are the right size - a tree that survived the
        // assertion must still paint.
        for (name, w) in [("clicked", &window), ("cycled", &w2)] {
            let pm = w
                .cpu_backend
                .last_frame
                .as_ref()
                .unwrap_or_else(|| panic!("{name} window produced no frame"))
                .clone_pixmap();
            assert_eq!(
                (pm.width(), pm.height()),
                (RIBBON_W as u32, RIBBON_H as u32)
            );
        }
    }

    /// FIXED (was: KNOWN GAP) - a deactivated tab used to keep the text
    /// colour it resolved while it was active.
    ///
    /// After clicking tab 1, tab 0's label painted the accent ACTIVE colour
    /// where a fresh render paints #444444. The cascade was right in both
    /// windows; the stale value was baked into the cached glyph runs
    /// (`StyleProperties::layout_hash` deliberately excludes colour, so a
    /// colour-only change never re-lays the IFC owner). The `<p>` label
    /// wrapper widened the hole from "sometimes" to "always": the IFC owner
    /// became the un-restyled `<p>`, so nothing ever re-laid it. FIX: the
    /// display-list builder re-resolves each run's colour from the CURRENT
    /// cascade via `CompactGlyphRun::source_node_id` at build time
    /// (solver3/display_list.rs, the live_color read before push_text_run);
    /// the baked colour only serves runs without a source node.
    #[test]
    fn clicking_a_ribbon_tab_re_resolves_the_deactivated_tabs_text_colour() {
        let (window, fresh, x, y, _damage) = switched_and_fresh();
        let (a, b) = (dl_texts(&window), dl_texts(&fresh));
        assert_eq!(a.len(), b.len(), "same state must emit the same Text items");
        for (i, (ta, tb)) in a.iter().zip(b.iter()).enumerate() {
            assert_eq!(
                ta, tb,
                "Text[{i}] differs after a tab click at ({x}, {y}); the clicked window painted a \
                 colour it resolved before the switch"
            );
        }
    }

    /// KNOWN GAP - a tab click repaints the whole window.
    ///
    /// `compute_display_list_damage` (cpurender::compositor) returns None -
    /// meaning FULL damage - as soon as the two lists differ in item COUNT,
    /// which every tab switch does. The rects are therefore not wrong, they
    /// are absent; this test pins the behaviour the queued display-list
    /// patching work (NodeId -> item mapping) is meant to deliver.
    #[test]
    #[ignore = "known gap: a structural change falls back to FULL damage"]
    fn ribbon_tab_click_does_not_damage_the_document_below() {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 0,
            bordered: false,
        })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let band = ribbon_bottom(&window);
        assert!(
            band > 0.0 && band < RIBBON_H,
            "the ribbon must occupy a band at the top; measured bottom = {band}"
        );

        let Some((x, y)) = tab_header_centre(&window, 1) else {
            panic!("no ribbon tab headers in the layout");
        };
        click_at(&mut window, x, y);
        window.regenerate_layout().expect("post-click relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        assert_eq!(active_tab_of(&state), Some(1), "the click missed the tab");

        match &damage {
            FrameDamage::Full => panic!(
                "a ribbon tab click reported FULL damage. Only the tab strip and the group band \
                 changed."
            ),
            FrameDamage::None => panic!(
                "a ribbon tab click reported NO damage, but the visible tab changed - the new \
                 frame would never reach the screen"
            ),
            FrameDamage::Rects(rects) => {
                let intruders: Vec<_> = rects
                    .iter()
                    .filter(|r| r.origin.y + r.size.height > band)
                    .collect();
                assert!(
                    intruders.is_empty(),
                    "a tab click damaged {} rect(s) reaching below the ribbon's own bottom edge \
                     (y = {band}), into the document area that did not change: {intruders:?}",
                    intruders.len()
                );
            }
        }
    }

    /// Two nodes with the SAME text must not share one shaped entry's
    /// colour or its identity.
    ///
    /// `shape_bidi_runs_with_per_item_cache` keys on `layout_hash`, which
    /// excludes paint-only properties by design so a recolour can reuse the
    /// shaping. The cached clusters, though, carry a whole
    /// `Arc<StyleProperties>` per glyph AND a `source_node_id`, and the
    /// display list reads its text colour and its `source_node_index` —
    /// which is what damage attributes rects by — straight out of them. Two
    /// labels reading "Label" at the same size therefore collided: the
    /// second took the first's colour and reported the first's node.
    ///
    /// NEGATIVE CONTROL: restoring the plain
    /// `shaped.extend(cached.clusters.iter().cloned())` makes both
    /// assertions fail — verified.
    extern "C" fn twin_label_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{basic::color::ColorU, property::CssProperty, style::text::StyleTextColor},
        };

        let label = |r: u8, g: u8, b: u8| {
            Dom::create_div()
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::text_color(
                        StyleTextColor {
                            inner: ColorU { r, g, b, a: 255 },
                        },
                    ))]
                    .into(),
                )
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    "Label",
                ))
        };
        Dom::create_body()
            .with_child(label(255, 0, 0))
            .with_child(label(0, 0, 255))
    }

    #[test]
    fn two_nodes_with_the_same_text_keep_their_own_colour_and_identity() {
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, twin_label_layout, 400.0, 300.0);
        window.regenerate_layout().expect("layout");

        let texts = dl_texts(&window);
        assert_eq!(texts.len(), 2, "one Text item per label: {texts:?}");

        let red = texts
            .iter()
            .filter(|t| t.contains("r: 255, g: 0, b: 0"))
            .count();
        let blue = texts
            .iter()
            .filter(|t| t.contains("r: 0, g: 0, b: 255"))
            .count();
        assert_eq!(
            (red, blue),
            (1, 1),
            "each label keeps its own colour; the shaping cache is keyed on layout_hash, which \
             excludes colour, so a shared entry would paint both in whichever colour shaped \
             first. items = {texts:?}"
        );

        let mut sources: Vec<&str> = texts
            .iter()
            .filter_map(|t| t.split("source_node_index: ").nth(1))
            .collect();
        sources.sort_unstable();
        sources.dedup();
        assert_eq!(
            sources.len(),
            2,
            "the two labels must report DIFFERENT source nodes - damage attributes rects by \
             source_node_index, so a shared one repaints the wrong node. items = {texts:?}"
        );
    }

    /// Border items of the display list, as debug strings.
    fn dl_borders(window: &HeadlessWindow) -> Vec<String> {
        let Some(lw) = window.common.layout_window.as_ref() else {
            return Vec::new();
        };
        let Some(r) = lw.layout_results.get(&azul_core::dom::DomId { inner: 0 }) else {
            return Vec::new();
        };
        r.display_list
            .items
            .iter()
            .filter_map(|i| match i {
                DisplayListItem::Border { .. } => Some(format!("{i:?}")),
                _ => None,
            })
            .collect()
    }

    /// `RibbonTab::style` reaches the rendered tab header.
    ///
    /// `RibbonStyle` describes the tab STRIP, so before this there was no
    /// way to tint one tab: every header shared `tab_style` /
    /// `tab_active_style`. The per-tab hook is what makes a frame dump
    /// legible when a rect is attributed to the wrong tab.
    ///
    /// NEGATIVE CONTROL: dropping the `tab.style` append in
    /// `Ribbon::dom()` leaves all three colours absent — verified.
    #[test]
    fn a_per_tab_style_reaches_that_tabs_header_and_no_other() {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 0,
            bordered: true,
        })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("layout");

        let borders = dl_borders(&window);
        for (name, needle) in [
            ("HOME red", "#ff0000ff"),
            ("INSERT green", "#00a000ff"),
            ("DESIGN blue", "#0000ffff"),
        ] {
            let n = borders.iter().filter(|b| b.contains(needle)).count();
            assert_eq!(
                n, 1,
                "expected exactly one border carrying the {name} tab colour, found {n}. Per-tab \
                 style is not reaching the header (or is reaching more than one)."
            );
        }
    }

    /// A tab switch must not move the tabs that did not change.
    ///
    /// Clicking tab 1 used to leave tab 2 — the tab whose state never
    /// changed — drawing its label 1.5px lower than a fresh render of the
    /// same state (baseline 34.6 against 33.1), with an IDENTICAL header
    /// box of 66x26 @ (126, 16). Only the CONTENT moved.
    ///
    /// First fixed (deac0bebb) by making `clone_node_from_old` drop the
    /// node's taffy measurement cache: tab 2's final layout was served from
    /// that cache after a MEASURE of the tab had re-placed its label. The
    /// root cause - a memoised final layout whose side effects a measure
    /// had overwritten - is guarded where the memo is used since c60844cab
    /// (`NodeCache::final_layout_current`), and the clone keeps its
    /// measurements again (LAYOUTPERF8: dropping them made every relayout
    /// lay out every flex item of the window). This test is the guard that
    /// a kept measurement never shifts the tabs that did not change.
    ///
    /// Found by the per-tab border aid (`RibbonTab::style`), which is why
    /// this runs with `bordered: true`: without borders the stale
    /// measurement happens to agree.
    #[test]
    fn switching_tabs_does_not_shift_the_other_tabs_text() {
        let state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 0,
            bordered: true,
        })));
        let mut window = make_window_sized(&state, ribbon_layout, RIBBON_W, RIBBON_H);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let Some((x, y)) = tab_header_centre(&window, 1) else {
            panic!("no ribbon tab headers");
        };
        click_at(&mut window, x, y);
        window.regenerate_layout().expect("post-click relayout");

        let fresh_state = Arc::new(RefCell::new(RefAny::new(RibbonUiState {
            active_tab: 1,
            bordered: true,
        })));
        let mut fresh = make_window_sized(&fresh_state, ribbon_layout, RIBBON_W, RIBBON_H);
        fresh.regenerate_layout().expect("fresh layout");
        step(&mut fresh, HeadlessEvent::MouseMove { x, y });
        fresh
            .regenerate_layout()
            .expect("fresh layout under the cursor");

        assert_eq!(
            dl_texts(&window),
            dl_texts(&fresh),
            "a tab switch must leave the untouched tabs where a fresh render puts them"
        );
    }

    fn step(window: &mut HeadlessWindow, event: HeadlessEvent) -> FrameDamage {
        use azul_core::{
            events::{MouseButton, ProcessEventResult},
            window::CursorPosition,
        };

        use crate::desktop::shell2::common::event::PlatformWindow;

        window.snapshot_window_state_baseline("headless.test.step");
        let mut tier = ProcessEventResult::DoNothing;
        match event {
            HeadlessEvent::MouseMove { x, y } => {
                let pos = LogicalPosition { x, y };
                window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
                // MWA-C-scroll: mirrors `run()`'s MouseMove arm — a held thumb
                // takes the motion and the shared helper swallows it.
                let thumb_drag = PlatformWindow::route_pointer_move(
                    window,
                    pos,
                    "headless.test.step.scrollbar_drag",
                );
                if let Some(r) = thumb_drag {
                    tier = tier.max_self(r);
                } else {
                    window.update_hit_test_at(pos);
                    record_headless_input(window, false, false); // MWA-A4
                    tier = tier.max_self(window.process_window_events(0));
                }
            }
            HeadlessEvent::MouseDown { button } => {
                // MWA-C-scroll: mirrors `run()`'s MouseDown arm — the press
                // router first, scrollbar then content.
                let press_at = window
                    .common
                    .current_window_state()
                    .mouse_state
                    .cursor_position
                    .get_position();
                let routed = match press_at {
                    Some(p) => PlatformWindow::route_pointer_press(
                        window,
                        p,
                        button,
                        "headless.test.step.scrollbar_click",
                    ),
                    None => None,
                };
                if let Some(r) = routed {
                    tier = tier.max_self(r);
                } else {
                    match button {
                        MouseButton::Left => window.common.mouse_state_mut().left_down = true,
                        MouseButton::Right => window.common.mouse_state_mut().right_down = true,
                        MouseButton::Middle => window.common.mouse_state_mut().middle_down = true,
                        _ => {}
                    }
                    record_headless_input(window, true, false); // MWA-A4
                    tier = tier.max_self(window.process_window_events(0));
                }
            }
            HeadlessEvent::MouseUp { button } => {
                // MWA-C-scroll: mirrors `run()`'s MouseUp arm — the primary
                // release lets go of a held thumb; the pass below still runs.
                let release_at = window
                    .common
                    .current_window_state()
                    .mouse_state
                    .cursor_position
                    .get_position();
                let ended_scrollbar_drag = release_at.is_some_and(|p| {
                    PlatformWindow::end_scrollbar_drag(
                        &mut *window,
                        p,
                        button,
                        "headless.test.step.scrollbar_drag_end",
                    )
                    .is_some()
                });
                if ended_scrollbar_drag {
                    tier = tier.max_self(ProcessEventResult::ShouldIncrementalRelayout);
                }
                match button {
                    MouseButton::Left => window.common.mouse_state_mut().left_down = false,
                    MouseButton::Right => window.common.mouse_state_mut().right_down = false,
                    MouseButton::Middle => window.common.mouse_state_mut().middle_down = false,
                    _ => {}
                }
                record_headless_input(window, false, true); // MWA-A4
                tier = tier.max_self(window.process_window_events(0));
            }
            HeadlessEvent::KeyDown { virtual_keycode } => {
                window.common.keyboard_state_mut().current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::Some(virtual_keycode);
                window
                    .common
                    .keyboard_state_mut()
                    .pressed_virtual_keycodes
                    .insert_hm_item(virtual_keycode);
                tier = tier.max_self(window.process_window_events(0));
            }
            HeadlessEvent::KeyUp { virtual_keycode } => {
                window.common.keyboard_state_mut().current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::None;
                window
                    .common
                    .keyboard_state_mut()
                    .pressed_virtual_keycodes
                    .remove_hm_item(&virtual_keycode);
                tier = tier.max_self(window.process_window_events(0));
            }
            // Mirrors `run()`'s TextInput and Scroll arms (the same shared
            // methods): a scripted keystroke goes through the text pipeline,
            // a wheel notch is queued against the scroll node under the
            // pointer and arms the momentum timer that applies it. Both used
            // to fall into `_ => {}`.
            HeadlessEvent::TextInput { text } => {
                tier = tier.max_self(window.apply_text_input_event(&text));
            }
            HeadlessEvent::Scroll { delta_x, delta_y } => {
                tier = tier.max_self(window.apply_wheel_scroll_event(delta_x, delta_y));
            }
            _ => {}
        }
        if tier > ProcessEventResult::DoNothing {
            window.service_frame(tier);
            window.cpu_backend.last_frame_damage.clone()
        } else {
            FrameDamage::None
        }
    }

    /// `body > div[contenteditable] > "abc"`.
    extern "C" fn harness_layout_editable(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_contenteditable(true)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("abc")),
        )
    }

    /// The test `step()` drives a `TextInput` the way `run()` does - through
    /// the one text pipeline (`CreateTextInput`: record, dispatch, apply). It
    /// used to fall into `_ => {}`: a scripted keystroke changed nothing, and a
    /// test typing through `step()` could only ever pass by accident.
    #[test]
    fn a_stepped_text_input_types_into_the_focused_field() {
        use azul_core::{
            dom::{DomId, DomNodeId, NodeId},
            selection::{CursorAffinity, GraphemeClusterId, TextCursor},
            styled_dom::NodeHierarchyItemId,
        };

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_with(&state, harness_layout_editable);
        window.regenerate_layout().expect("initial layout");
        let dom = DomId { inner: 0 };
        let field = NodeId::new(1);
        {
            let lw = window
                .common
                .layout_window
                .as_mut()
                .expect("the window has a layout");
            lw.focus_manager.set_focused_node(Some(DomNodeId {
                dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(field)),
            }));
            assert!(
                lw.start_editing_at(
                    TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: 0,
                            start_byte_in_run: 0,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                    dom,
                    field,
                    0,
                ),
                "premise: a session opens in the field"
            );
        }

        step(
            &mut window,
            HeadlessEvent::TextInput {
                text: "x".to_string(),
            },
        );

        let text = window.common.layout_window.as_ref().map(|lw| {
            let content = lw.get_text_before_textinput(dom, field);
            lw.extract_text_from_inline_content(&content)
        });
        assert!(
            text.as_deref()
                .is_some_and(|t| t.len() == 4 && t.contains('x')),
            "the keystroke is typed into \"abc\": {text:?}"
        );
    }

    /// [`notes_editor_layout`]'s app: the editor state the change callback keeps.
    struct NotesEditor {
        editor: azul_layout::widgets::rich_text_editor::RichTextEditorState,
    }

    extern "C" fn keep_editor_state(
        mut data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
        state: azul_layout::widgets::rich_text_editor::RichTextEditorState,
    ) -> azul_core::callbacks::Update {
        if let Some(mut app) = data.downcast_mut::<NotesEditor>() {
            app.editor = state;
        }
        azul_core::callbacks::Update::RefreshDom
    }

    /// `body > RichTextEditor` over the app's editor state - AzNotes' note body.
    extern "C" fn notes_editor_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_layout::widgets::rich_text_editor::{
            RichTextEditor, RichTextEditorOnChangeCallbackType,
        };
        let state = data
            .downcast_ref::<NotesEditor>()
            .map(|app| app.editor.clone())
            .expect("the app's state");
        Dom::create_body().with_child(
            RichTextEditor::create(state)
                .with_on_change(data.clone(), keep_editor_state as RichTextEditorOnChangeCallbackType)
                .dom(),
        )
    }

    /// Enter on an empty list item turns it into a paragraph and the editor rebuilds: the
    /// caret must stand in that paragraph, so the next keystroke lands there. The item's node
    /// was replaced by the paragraph's, the caret on it went with it, and typing went nowhere
    /// until something else placed a caret - AzNotes dropped "[ ] call the bakery" typed
    /// after leaving a shopping list (E2E sweep, 2026-10-06).
    #[test]
    fn typing_after_enter_on_an_empty_list_item_lands_in_the_new_paragraph() {
        use azul_core::{
            dom::{DomId, DomNodeId, NodeId},
            selection::{CursorAffinity, GraphemeClusterId, TextCursor},
            styled_dom::NodeHierarchyItemId,
        };
        use azul_layout::widgets::rich_text::{RichBlock, RichBlockKind, RichTextDoc};

        let doc = RichTextDoc::from_blocks(vec![
            RichBlock::text(RichBlockKind::Bullet(0), "milk"),
            RichBlock::text(RichBlockKind::Bullet(0), "eggs"),
            RichBlock::text(RichBlockKind::Bullet(0), ""),
        ]);
        let editor = azul_layout::widgets::rich_text_editor::RichTextEditorState::create(doc);
        let host_id = editor.host_id.as_str().to_string();
        let state = Arc::new(RefCell::new(RefAny::new(NotesEditor { editor })));
        let mut window = make_window_with(&state, notes_editor_layout);
        window.regenerate_layout().expect("the editor's first layout");
        let _ = window.common.take_regeneration();

        let dom = DomId::ROOT_ID;
        {
            let lw = window.common.layout_window.as_mut().expect("a layout window");
            let styled = &lw.layout_results[&dom].styled_dom;
            let host = styled
                .node_data
                .as_container()
                .internal
                .iter()
                .position(|n| n.has_id(&host_id))
                .map(NodeId::new)
                .expect("premise: the editing host");
            let hierarchy = styled.node_hierarchy.as_container();
            let mut block = hierarchy.get(host).and_then(|h| h.first_child_id(host));
            for _ in 0..2 {
                block = block.and_then(|b| hierarchy.get(b).and_then(|h| h.next_sibling_id()));
            }
            let empty_item = block.expect("premise: the third block");
            lw.focus_manager.set_focused_node(Some(DomNodeId {
                dom,
                node: NodeHierarchyItemId::from_crate_internal(Some(host)),
            }));
            assert!(
                lw.start_editing_at(
                    TextCursor {
                        cluster_id: GraphemeClusterId {
                            source_run: 0,
                            start_byte_in_run: 0,
                        },
                        affinity: CursorAffinity::Leading,
                    },
                    dom,
                    empty_item,
                    0,
                ),
                "premise: a caret in the empty item"
            );
        }

        step(&mut window, HeadlessEvent::KeyDown { virtual_keycode: VirtualKeyCode::Return });
        step(&mut window, HeadlessEvent::KeyUp { virtual_keycode: VirtualKeyCode::Return });
        if window.common.take_regeneration() {
            window.regenerate_layout().expect("the rebuild");
        }
        let kinds = state
            .borrow_mut()
            .downcast_ref::<NotesEditor>()
            .map(|app| {
                app.editor
                    .doc
                    .blocks
                    .as_ref()
                    .iter()
                    .map(|b| b.kind.clone())
                    .collect::<Vec<_>>()
            })
            .expect("the app's state");
        assert_eq!(
            kinds.last(),
            Some(&RichBlockKind::Paragraph),
            "premise: Enter left the list: {kinds:?}"
        );

        let caret = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.document_caret())
            .expect("a caret after the rebuild");
        step(&mut window, HeadlessEvent::TextInput { text: "x".to_string() });
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let paragraph = caret.node.node.into_crate_internal().expect("the caret's block");
        let text = lw.extract_text_from_inline_content(&lw.get_text_before_textinput(dom, paragraph));
        assert_eq!(
            text, "x",
            "the keystroke typed after leaving the list is in the new paragraph"
        );
    }

    /// `step()` drives a wheel `Scroll` the way `run()` does: the delta is
    /// queued against the scroll node under the pointer and the momentum timer
    /// applies it. It used to drop the event, so no test could scroll through
    /// `step()`.
    #[test]
    fn a_stepped_wheel_scrolls_the_box_under_the_pointer() {
        use azul_core::{dom::DomId, task::SCROLL_MOMENTUM_TIMER_ID};

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        let node = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().next().copied())
            .expect("premise: the 200x100 box is a scroll frame");

        // Over the box (body margin 8px), then a wheel notch towards the user.
        step(&mut window, HeadlessEvent::MouseMove { x: 50.0, y: 50.0 });
        step(
            &mut window,
            HeadlessEvent::Scroll {
                delta_x: 0.0,
                delta_y: -120.0,
            },
        );
        assert!(
            window
                .common
                .layout_window
                .as_ref()
                .is_some_and(|lw| lw.timers.contains_key(&SCROLL_MOMENTUM_TIMER_ID)),
            "the wheel arms the momentum timer that applies it"
        );

        let offset = |w: &HeadlessWindow| {
            w.common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.scroll_manager.get_current_offset(dom_id(), node))
                .map_or(0.0, |p| p.y)
        };
        fn dom_id() -> DomId {
            DomId { inner: 0 }
        }
        let mut moved = offset(&window) != 0.0;
        for _ in 0..60 {
            if moved {
                break;
            }
            let _ = azul_core::task::advance_test_clock_ms(16);
            let _ = window.process_timers_and_threads();
            moved = offset(&window) != 0.0;
        }
        azul_core::task::reset_test_clock();
        assert!(moved, "the wheel scrolls the box under the pointer");
    }

    #[test]
    fn damage_mouse_move_no_change_is_clean() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(200.0, 100.0)],
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Move the mouse over a static colored box (no :hover rule, no callback).
        let d1 = step(&mut window, HeadlessEvent::MouseMove { x: 50.0, y: 50.0 });
        let d2 = step(&mut window, HeadlessEvent::MouseMove { x: 90.0, y: 70.0 });
        println!("[harness] mouse moves: d1={:?} d2={:?}", d1, d2);

        // HONEST: moving the mouse over static content with no hover styling and
        // no callbacks must NOT repaint. Otherwise every pointer move repaints
        // the frame — unusable (esp. with the cursor moving constantly).
        assert_eq!(
            d1,
            FrameDamage::None,
            "mouse move over static content produced damage {:?}",
            d1
        );
        assert_eq!(
            d2,
            FrameDamage::None,
            "second mouse move over static content produced damage {:?}",
            d2
        );
    }

    #[test]
    fn damage_single_paint_in_large_grid_is_local() {
        let n = 30usize;
        let boxes: Vec<(f32, f32)> = (0..n).map(|_| (100.0, 20.0)).collect();
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes,
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_grid);
        window.regenerate_layout().expect("initial layout");

        // Recolor exactly ONE box in a 30-box grid. The window is 300 px tall and the boxes 20 px:
        // it shows the first 15, so the box is index 7 (y 140..160). Box 15, the middle of the grid,
        // starts at y = 300, below the frame: the damage of a CPU frame lies inside the frame, so a
        // change there is no damage at all (the test used to pass on the unclipped rect of a box
        // nobody could see).
        set_highlight(&state, Some(7));
        window.regenerate_layout().expect("highlight one box");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!(
            "[harness] single paint in {}-box grid: damage={:?}",
            n, damage
        );

        // HONEST + perf-critical: changing ONE box's colour must damage ~one box
        // (100x20 = 2000 px²), NOT the whole grid (600px tall) or the window.
        // Over-damaging on every small change makes a large UI unusable — this is
        // the core "damage must be incremental at scale" invariant.
        let window_area = 400.0 * 300.0;
        match damage_area(&damage) {
            Some(a) if a > 0.0 => assert!(
                a < window_area * 0.2,
                "single-box recolor in a {}-box grid damaged area {} — should be ~one box (~2000 \
                 px²), not the whole grid/window. Damage is not incremental at scale. damage={:?}",
                n,
                a,
                damage
            ),
            other => panic!(
                "single-box recolor should produce small local damage, got area={:?} damage={:?}",
                other, damage
            ),
        }
    }

    // --- Scroll: the make-or-break perf case ---

    #[derive(Debug, Clone)]
    struct ScrollTestState {
        n_items: usize,
    }

    /// A 200x100 `overflow:scroll` container holding `n_items` 30px-tall rows
    /// (so n_items > ~3 overflows and makes it scrollable).
    extern "C" fn harness_layout_scroll(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::{
                    dimensions::{LayoutHeight, LayoutWidth},
                    overflow::LayoutOverflow,
                },
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let n = data
            .downcast_ref::<ScrollTestState>()
            .map(|s| s.n_items)
            .unwrap_or(0);
        let mut container = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        );
        for i in 0..n {
            let color = if i % 2 == 0 {
                ColorU {
                    r: 200,
                    g: 60,
                    b: 60,
                    a: 255,
                }
            } else {
                ColorU {
                    r: 60,
                    g: 60,
                    b: 200,
                    a: 255,
                }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            container = container.with_child(
                Dom::create_div().with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(
                            180.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            30.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                ),
            );
        }
        Dom::create_body().with_child(container)
    }

    /// A 200x100 `overflow:scroll` container (BOTH axes) holding `n_items` rows
    /// that are WIDER than the viewport (400px) and 30px tall — so the frame is
    /// scrollable diagonally (mobile pan). Rows alternate colour every 30px so a
    /// vertical scroll is visible at a fixed pixel.
    extern "C" fn harness_layout_scroll_2d(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::{
                    dimensions::{LayoutHeight, LayoutWidth},
                    overflow::LayoutOverflow,
                },
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let n = data
            .downcast_ref::<ScrollTestState>()
            .map(|s| s.n_items)
            .unwrap_or(0);
        let mut container = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_x(LayoutOverflow::Scroll)),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        );
        for i in 0..n {
            let color = if i % 2 == 0 {
                ColorU {
                    r: 200,
                    g: 60,
                    b: 60,
                    a: 255,
                }
            } else {
                ColorU {
                    r: 60,
                    g: 60,
                    b: 200,
                    a: 255,
                }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            container = container.with_child(
                Dom::create_div().with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(
                            400.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            30.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                ),
            );
        }
        Dom::create_body().with_child(container)
    }

    /// Grid harness variant with an opaque dark BODY BACKGROUND. The bg rect
    /// spans the whole window, so it intersects every damage rect — exactly
    /// the ingredient that triggered the union-clip overpaint bug (an item
    /// intersecting several disjoint damage rects repainted across their
    /// whole union, erasing the untouched content in between).
    extern "C" fn harness_layout_grid_on_bg(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let (boxes, highlight) = data
            .downcast_ref::<GridState>()
            .map(|s| (s.boxes.clone(), s.highlight))
            .unwrap_or_default();
        let body_bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(ColorU {
            r: 40,
            g: 40,
            b: 40,
            a: 255,
        })]
        .into();
        let mut body = Dom::create_body().with_css_props(
            vec![CssPropertyWithConditions::simple(
                CssProperty::background_content(body_bg),
            )]
            .into(),
        );
        for (i, (w, h)) in boxes.iter().enumerate() {
            let color = if Some(i) == highlight {
                ColorU {
                    r: 30,
                    g: 220,
                    b: 30,
                    a: 255,
                }
            } else if i % 2 == 0 {
                ColorU {
                    r: 220,
                    g: 30,
                    b: 30,
                    a: 255,
                }
            } else {
                ColorU {
                    r: 30,
                    g: 30,
                    b: 220,
                    a: 255,
                }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            body = body.with_child(
                Dom::create_div().with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(*w))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            *h,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                ),
            );
        }
        body
    }

    /// REGRESSION (union-clip overpaint): two boxes far apart change color in
    /// one frame → two DISJOINT damage rects. The full-window background
    /// intersects BOTH. The old union-clip renderer repainted the background
    /// across the whole union while the unchanged boxes in between were
    /// filtered out → they were ERASED to background color on the first
    /// incremental frame. Per-rect passes must leave them untouched.
    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_disjoint_rects_do_not_erase_content_between() {
        let state = Arc::new(RefCell::new(RefAny::new(GridState {
            boxes: vec![(100.0, 20.0); 5],
            highlight: Some(0),
        })));
        let mut window = make_window_with(&state, harness_layout_grid_on_bg);
        window.regenerate_layout().expect("initial layout");

        // Box centers: body content starts at (8, 8); box i spans y 8+i*20.
        let box2_px = (58u32, 58u32); // center of box 2 (unchanged, red)
        let before = sample_px(&window, box2_px.0, box2_px.1).expect("sample before");
        assert_eq!(
            before,
            [220, 30, 30, 255],
            "box2 should start red (harness sanity)"
        );

        // Flip the highlight from box0 to box4: box0 green→red AND box4
        // blue→green — two changed items at opposite ends, disjoint rects.
        set_highlight(&state, Some(4));
        window.regenerate_layout().expect("incremental relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] disjoint-change damage = {:?}", damage);

        let after = sample_px(&window, box2_px.0, box2_px.1).expect("sample after");
        assert_eq!(
            after,
            [220, 30, 30, 255],
            "box2 (unchanged, BETWEEN the two damage rects) was overwritten — an item \
             intersecting several disjoint damage rects must not repaint across their union (it \
             erases skipped neighbours); damage={:?}",
            damage
        );
        // And the actually-changed boxes must have their new colors.
        let box0 = sample_px(&window, 58, 18).expect("box0");
        let box4 = sample_px(&window, 58, 98).expect("box4");
        assert_eq!(box0, [220, 30, 30, 255], "box0 should now be red");
        assert_eq!(box4, [30, 220, 30, 255], "box4 should now be green");
    }

    /// Scroll harness variant where one row's color is state-driven, so a test
    /// can change content INSIDE an already-scrolled frame.
    #[derive(Debug, Clone)]
    struct ScrollHighlightState {
        n_items: usize,
        highlight: Option<usize>,
    }

    extern "C" fn harness_layout_scroll_highlight(
        mut data: RefAny,
        _info: LayoutCallbackInfo,
    ) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::{
                    dimensions::{LayoutHeight, LayoutWidth},
                    overflow::LayoutOverflow,
                },
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let (n, highlight) = data
            .downcast_ref::<ScrollHighlightState>()
            .map(|s| (s.n_items, s.highlight))
            .unwrap_or((0, None));
        let mut container = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(200.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(100.0))),
                CssPropertyWithConditions::simple(CssProperty::overflow_y(LayoutOverflow::Scroll)),
            ]
            .into(),
        );
        for i in 0..n {
            let color = if Some(i) == highlight {
                ColorU {
                    r: 30,
                    g: 220,
                    b: 30,
                    a: 255,
                }
            } else if i % 2 == 0 {
                ColorU {
                    r: 200,
                    g: 60,
                    b: 60,
                    a: 255,
                }
            } else {
                ColorU {
                    r: 60,
                    g: 60,
                    b: 200,
                    a: 255,
                }
            };
            let bg: StyleBackgroundContentVec = vec![StyleBackgroundContent::Color(color)].into();
            container = container.with_child(
                Dom::create_div().with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(
                            180.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            30.0,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                ),
            );
        }
        Dom::create_body().with_child(container)
    }

    fn set_scroll_highlight(state: &Arc<RefCell<RefAny>>, highlight: Option<usize>) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<ScrollHighlightState>();
        if let Some(s) = opt.as_mut() {
            s.highlight = highlight;
        }
    }

    /// Scroll the (single) scroll frame of `window` to vertical offset `dy`.
    #[cfg(feature = "cpurender")]
    fn scroll_frame_to(window: &mut HeadlessWindow, dy: f32) {
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.layout_cache
                    .scroll_id_to_node_id
                    .values()
                    .next()
                    .copied()
            })
            .expect("no scroll frame registered");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, dy),
                size: LogicalSize::new(200.0, 600.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
    }

    /// REGRESSION (content-space damage in scrolled frames): change a row's
    /// color while the frame is scrolled. The damage diff used to emit the
    /// item's CONTENT-space bounds, so the repaint landed a scroll-offset too
    /// low and the changed row stayed visually stale on screen.
    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_change_inside_scrolled_frame_repaints_at_viewport_position() {
        let state = Arc::new(RefCell::new(RefAny::new(ScrollHighlightState {
            n_items: 20,
            highlight: None,
        })));
        let mut window = make_window_with(&state, harness_layout_scroll_highlight);
        window.regenerate_layout().expect("initial layout");

        // Scroll down 30px and render (row 1's content span y 30..60 is now
        // on screen at viewport y 8..38; parent content starts at y=8).
        scroll_frame_to(&mut window, 30.0);
        window.regenerate_layout().expect("scroll relayout");
        println!(
            "[harness] post-scroll damage = {:?} px(50,20)={:?} px(50,75)={:?}",
            window.cpu_backend.last_frame_damage,
            sample_px(&window, 50, 20),
            sample_px(&window, 50, 75),
        );

        let probe = (50u32, 20u32); // inside row 1's on-screen span (content y=42)
        let before = sample_px(&window, probe.0, probe.1).expect("sample before");
        assert_eq!(before, [60, 60, 200, 255], "row1 starts blue (sanity)");

        // Change row 1 to green while scrolled.
        set_scroll_highlight(&state, Some(1));
        window.regenerate_layout().expect("highlight relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] scrolled-change damage = {:?}", damage);

        let after = sample_px(&window, probe.0, probe.1).expect("sample after");
        assert_eq!(
            after,
            [30, 220, 30, 255],
            "row 1 changed color while the frame was scrolled but its ON-SCREEN pixels did not \
             update — the damage diff must project item bounds through the scroll offset \
             (content-space damage repaints the wrong band); damage={:?}",
            damage
        );
    }

    // --- A VirtualView scrolled into sight repaints where it is shown -------
    //
    // REPORTED (AzWidgets on macOS, 2026-09-28): the Video card showed Big Buck
    // Bunny's first frame, and after a press on play no later frame ever
    // reached the screen. The frames did reach the widget: each one is written
    // into the widget's state and its `VirtualView` re-renders in place
    // (`video_writeback` -> `trigger_all_virtual_view_rerender`). What never
    // happened was the REPAINT. A view that re-renders in place changes only
    // its child display list, so its one source of damage is the child-list
    // diff (`compute_virtual_view_damage`), and that damaged the view's box at
    // its CONTENT position. Inside a scrolled box that is a scroll offset below
    // where the view is painted. The Video card sits far down the scrolled
    // page, so every frame repainted a band nowhere near it. The poster got
    // through only because scrolling the card into sight repaints it.
    //
    // The class is the one `damage_change_inside_scrolled_frame_repaints_at_viewport_position`
    // pins for the parent list, in the one damage source it does not reach.

    #[cfg(feature = "cpurender")]
    const SWATCH_RED: [u8; 4] = [220, 30, 30, 255];
    #[cfg(feature = "cpurender")]
    const SWATCH_GREEN: [u8; 4] = [30, 220, 30, 255];
    #[cfg(feature = "cpurender")]
    const SWATCH_BLUE: [u8; 4] = [30, 30, 220, 255];
    #[cfg(feature = "cpurender")]
    const SWATCH_YELLOW: [u8; 4] = [220, 220, 30, 255];

    /// What a swatch view paints: one colour, changed in place the way a
    /// video frame is.
    #[cfg(feature = "cpurender")]
    struct VvSwatch {
        rgba: [u8; 4],
    }

    /// The app state: the swatch's `RefAny`, the same one across builds.
    #[cfg(feature = "cpurender")]
    struct ScrolledSwatchPage {
        swatch: RefAny,
    }

    /// The app state of the video twin: the decoded frames, as the `RefAny`
    /// that `VideoWidget::with_frames` replays.
    #[cfg(feature = "cpurender")]
    struct ScrolledVideoPage {
        frames: RefAny,
    }

    /// Fills the view with the swatch's colour.
    #[cfg(feature = "cpurender")]
    extern "C" fn swatch_view_render(
        data: RefAny,
        info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::geom::{LogicalPosition, LogicalRect};
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let mut data = data;
        let [r, g, b, a] = data
            .downcast_ref::<VvSwatch>()
            .map_or([0, 0, 0, 255], |s| s.rgba);
        let size = info.get_bounds().get_logical_size();
        let rect = LogicalRect::new(LogicalPosition::zero(), size);
        let bg: StyleBackgroundContentVec =
            vec![StyleBackgroundContent::Color(ColorU { r, g, b, a })].into();
        azul_core::callbacks::VirtualViewReturn {
            dom: azul_core::dom::OptionDom::Some(
                Dom::create_div().with_css_props(
                    vec![
                        CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(
                            size.width,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                            size.height,
                        ))),
                        CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                    ]
                    .into(),
                ),
            ),
            materialized: rect,
            virtual_rect: rect,
        }
    }

    /// `<body>` with a 200x100 box scrolling a 600px column: 300px of spacer,
    /// then `view` at 180x60, then 240px of spacer. The view's box is at
    /// content y 308..368 (the body's 8px margin first); scrolled by 300 it is
    /// on screen at y 8..68.
    #[cfg(feature = "cpurender")]
    fn scrolled_page_around(view: Dom) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                layout::{
                    dimensions::{LayoutHeight, LayoutWidth},
                    overflow::LayoutOverflow,
                },
                property::CssProperty,
            },
        };

        fn sized(width: f32, height: f32) -> Vec<CssPropertyWithConditions> {
            vec![
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(width))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(height))),
            ]
        }
        let mut scroller = sized(200.0, 100.0);
        scroller.push(CssPropertyWithConditions::simple(CssProperty::overflow_y(
            LayoutOverflow::Scroll,
        )));
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css_props(scroller.into())
                .with_child(Dom::create_div().with_css_props(sized(180.0, 300.0).into()))
                .with_child(view.with_css_props(sized(180.0, 60.0).into()))
                .with_child(Dom::create_div().with_css_props(sized(180.0, 240.0).into())),
        )
    }

    #[cfg(feature = "cpurender")]
    extern "C" fn scrolled_swatch_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let swatch = data
            .downcast_ref::<ScrolledSwatchPage>()
            .map(|page| page.swatch.clone())
            .expect("page state");
        scrolled_page_around(Dom::create_virtual_view(
            swatch,
            azul_core::callbacks::VirtualViewCallback::create(swatch_view_render),
        ))
    }

    #[cfg(feature = "cpurender")]
    extern "C" fn scrolled_video_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::video::{VideoConfig, VideoSource};
        use azul_layout::widgets::video::VideoWidget;

        let frames = data
            .downcast_ref::<ScrolledVideoPage>()
            .map(|page| page.frames.clone())
            .expect("page state");
        scrolled_page_around(
            VideoWidget::create(VideoConfig::new(VideoSource::Bytes(
                Vec::<u8>::new().into(),
            )))
            .with_frames(frames)
            .dom(),
        )
    }

    /// Change the swatch's colour in place: the dataset changes, the DOM does
    /// not.
    #[cfg(feature = "cpurender")]
    fn set_swatch(swatch: &RefAny, rgba: [u8; 4]) {
        let mut handle = swatch.clone();
        if let Some(mut s) = handle.downcast_mut::<VvSwatch>() {
            s.rgba = rgba;
        };
    }

    /// A 4x4 video frame of one colour.
    #[cfg(feature = "cpurender")]
    fn solid_video_frame(rgba: [u8; 4]) -> azul_core::video::VideoFrame {
        let mut bytes: Vec<u8> = Vec::with_capacity(4 * 4 * 4);
        for _ in 0..16 {
            bytes.extend_from_slice(&rgba);
        }
        azul_core::video::VideoFrame::new(4, 4, bytes.into())
    }

    /// Which colour of `palette` a painted pixel is, allowing for the image
    /// scaler's rounding.
    #[cfg(feature = "cpurender")]
    fn palette_index(px: [u8; 4], palette: &[[u8; 4]]) -> Option<usize> {
        palette
            .iter()
            .position(|c| c.iter().zip(px.iter()).all(|(a, b)| a.abs_diff(*b) <= 12))
    }

    /// Scroll the OUTERMOST scroll frame (the lowest node id) of `window` to
    /// vertical offset `dy`. [`scroll_frame_to`] takes whichever frame the map
    /// hands out first, which is only right while there is one.
    #[cfg(feature = "cpurender")]
    fn scroll_outermost_frame_to(window: &mut HeadlessWindow, dy: f32) {
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_cache.scroll_id_to_node_id.values().copied().min())
            .expect("no scroll frame registered");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, dy),
                size: LogicalSize::new(200.0, 600.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
    }

    /// Paint one frame from the layout as it stands - what a desktop
    /// backend's frame path does after it drained the queued view re-renders:
    /// no relayout, no rebuild, only `render_frame` and its damage.
    #[cfg(feature = "cpurender")]
    fn paint_frame(window: &mut HeadlessWindow) {
        let (width, height, dpi) = {
            let ws = window.common.current_window_state();
            (
                ws.size.dimensions.width,
                ws.size.dimensions.height,
                ws.size.dpi as f32 / 96.0,
            )
        };
        if let Some(lw) = window.common.layout_window.as_ref() {
            let _painted = window.cpu_backend.render_frame(
                lw,
                &window.common.renderer_resources,
                width,
                height,
                dpi,
            );
        }
    }

    // --- A value written INSIDE a view is painted by the next CPU frame ---------------------
    //
    // The map editor zooms its map with `set_css_property(transform)` on the picture of the view:
    // a node of the view's child DOM that carries an identity transform at build time. The value
    // lands in the CHILD's GPU value cache - no list, no layout, no render of the view - and
    // every shell's CPU frame (this backend, headless and the desktop shells alike) read the
    // root's cache alone: the picture stayed unscaled until the view rendered again.
    //
    // The reference is hand-computed: a 40 x 30 picture at (40, 30) of the view's content,
    // scaled by two about its centre and moved by (40, 20), is the 80 x 60 box at (60, 35).

    /// The view's child: a red picture that carries an identity transform at build time and a
    /// blue veil at opacity 0.8.
    #[cfg(feature = "cpurender")]
    extern "C" fn zoom_view_render(
        _data: RefAny,
        info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::geom::{LogicalPosition, LogicalRect};
        let page = Dom::create_div()
            .with_child(Dom::create_div().with_class("picture".into()).with_css(
                "position: absolute; left: 40px; top: 30px; width: 40px; height: 30px; \
                 background: #ff0000; transform: translate(0px, 0px) scale(1);",
            ))
            .with_child(Dom::create_div().with_class("veil".into()).with_css(
                "position: absolute; left: 220px; top: 120px; width: 50px; height: 50px; \
                 background: #0000ff; opacity: 0.8;",
            ));
        let rect = LogicalRect::new(LogicalPosition::zero(), info.get_bounds().get_logical_size());
        azul_core::callbacks::VirtualViewReturn::with_dom(page, rect, rect)
    }

    /// The window: the view at (50, 40), 300 x 200.
    #[cfg(feature = "cpurender")]
    extern "C" fn zoom_page_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_virtual_view(
                RefAny::new(()),
                azul_core::callbacks::VirtualViewCallback::create(zoom_view_render),
            )
            .with_css("position: absolute; left: 50px; top: 40px; width: 300px; height: 200px;"),
        )
    }

    /// The page laid out and painted once, the view's DOM with its picture and veil.
    #[cfg(feature = "cpurender")]
    fn zoom_window() -> (
        HeadlessWindow,
        azul_core::dom::DomId,
        azul_core::dom::NodeId,
        azul_core::dom::NodeId,
    ) {
        use azul_core::dom::NodeId;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, zoom_page_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        window.common.drain_virtual_view_updates();
        let _ = window.common.take_regeneration();
        paint_frame(&mut window);
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let child = *lw
            .layout_results
            .keys()
            .find(|dom| dom.inner != 0)
            .expect("the view's DOM");
        let nodes = lw.layout_results[&child].styled_dom.node_data.as_container();
        let with_class = |class: &str| -> NodeId {
            (0..nodes.len())
                .map(NodeId::new)
                .find(|n| nodes[*n].has_class(class))
                .unwrap_or_else(|| panic!("a node of class {class}"))
        };
        let (picture, veil) = (with_class("picture"), with_class("veil"));
        (window, child, picture, veil)
    }

    /// What `CallbackInfo::set_css_property` does, and the frame the shell makes for its result.
    #[cfg(feature = "cpurender")]
    fn write_in_view(
        window: &mut HeadlessWindow,
        child: azul_core::dom::DomId,
        node: azul_core::dom::NodeId,
        prop: azul_css::props::property::CssProperty,
    ) {
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let result = window.apply_user_change(&CallbackChange::ChangeNodeCssProperties {
            dom_id: child,
            node_id: node,
            properties: vec![prop].into(),
        });
        window.service_frame(result);
    }

    /// `translate(tx, ty) scale(s)`: the editor's zoom.
    #[cfg(feature = "cpurender")]
    fn zoom_css(tx: f32, ty: f32, s: f32) -> azul_css::props::property::CssProperty {
        use azul_css::props::{
            basic::{FloatValue, PixelValue},
            style::{StyleTransform, StyleTransformScale2D, StyleTransformTranslate2D},
        };
        azul_css::props::property::CssProperty::transform(
            vec![
                StyleTransform::Translate(StyleTransformTranslate2D {
                    x: PixelValue::px(tx),
                    y: PixelValue::px(ty),
                }),
                StyleTransform::Scale(StyleTransformScale2D {
                    x: FloatValue::new(s),
                    y: FloatValue::new(s),
                }),
            ]
            .into(),
        )
    }

    /// The inclusive pixel box of the red pixels of the window's last frame.
    #[cfg(feature = "cpurender")]
    fn red_box_of(window: &HeadlessWindow) -> Option<(u32, u32, u32, u32)> {
        let frame = window.cpu_backend.last_frame.as_ref()?;
        let mut found: Option<(u32, u32, u32, u32)> = None;
        for y in 0..frame.height() {
            for x in 0..frame.width() {
                let p = pixel_of(frame, x, y)?;
                // Red by a clear margin over the other channels: the edge pixels of a scaled
                // picture, where its surface is resampled, count.
                if p[0] > 150 && p[0].saturating_sub(p[1].max(p[2])) > 40 {
                    found = Some(match found {
                        None => (x, y, x, y),
                        Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
                    });
                }
            }
        }
        found
    }

    /// A transform value written to a node inside a view moves the picture in the very next CPU
    /// frame - which is the frame of every shell, since they all paint with this backend - with
    /// no layout and no rebuild of any list; a second write replaces the first, and the frame is
    /// the one a full repaint paints.
    #[test]
    #[cfg(feature = "cpurender")]
    fn a_transform_written_inside_a_view_is_painted_by_the_next_cpu_frame() {
        let (mut window, child, picture, _veil) = zoom_window();
        // The view's corner is at (50, 40): the picture is at (90, 70), 40 x 30.
        assert_eq!(red_box_of(&window), Some((90, 70, 129, 99)), "harness: the picture unscaled");
        window
            .common
            .layout_window
            .as_mut()
            .expect("a layout window")
            .frame_report
            .reset_counters();

        write_in_view(&mut window, child, picture, zoom_css(40.0, 20.0, 2.0));
        assert_eq!(
            red_box_of(&window),
            Some((110, 75, 189, 134)),
            "translate(40px, 20px) scale(2) about the centre: 80 x 60 at (60, 35) of the content"
        );
        let (differing, first) = incremental_vs_full(&mut window);
        assert_eq!(differing, 0, "the incremental frame is a full paint's, first diff {first:?}");

        write_in_view(&mut window, child, picture, zoom_css(100.0, 50.0, 2.0));
        assert_eq!(
            red_box_of(&window),
            Some((170, 105, 249, 164)),
            "a second write replaces the first: 80 x 60 at (120, 65) of the content"
        );
        let (differing, first) = incremental_vs_full(&mut window);
        assert_eq!(differing, 0, "after the second write, first diff {first:?}");

        write_in_view(&mut window, child, picture, zoom_css(0.0, 0.0, 1.0));
        assert_eq!(red_box_of(&window), Some((90, 70, 129, 99)), "back at the identity");

        let report = window
            .common
            .layout_window
            .as_ref()
            .expect("a layout window")
            .frame_report
            .clone();
        assert_eq!(report.layout_passes, 0, "no layout: {report:?}");
        assert_eq!(report.dl_rebuilds, 0, "no display list rebuilt: {report:?}");
        assert_eq!(report.dom_regenerations, 0, "no DOM regenerated: {report:?}");
    }

    /// The opacity twin: a node inside a view fades by a value.
    #[test]
    #[cfg(feature = "cpurender")]
    fn an_opacity_written_inside_a_view_is_painted_by_the_next_cpu_frame() {
        use azul_css::props::{
            basic::PercentageValue, property::CssProperty, style::StyleOpacity,
        };
        let opacity = |percent: f32| {
            CssProperty::opacity(StyleOpacity {
                inner: PercentageValue::new(percent),
            })
        };
        let (mut window, child, _picture, veil) = zoom_window();
        // The veil's centre: (50 + 245, 40 + 145).
        let centre = |w: &HeadlessWindow| sample_px(w, 295, 185).map(|p| [p[0], p[1], p[2]]);
        let near = |got: Option<[u8; 3]>, want: [u8; 3]| {
            got.is_some_and(|g| g.iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 4))
        };
        assert!(near(centre(&window), [51, 51, 255]), "harness: 0.8 blue over white");

        write_in_view(&mut window, child, veil, opacity(40.0));
        assert!(near(centre(&window), [153, 153, 255]), "0.4 blue over white: {:?}", centre(&window));
        let (differing, first) = incremental_vs_full(&mut window);
        assert_eq!(differing, 0, "first diff {first:?}");

        write_in_view(&mut window, child, veil, opacity(60.0));
        assert!(
            near(centre(&window), [102, 102, 255]),
            "replaced, not stacked: 0.6 blue over white: {:?}",
            centre(&window)
        );
        let (differing, first) = incremental_vs_full(&mut window);
        assert_eq!(differing, 0, "first diff {first:?}");
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn a_virtual_view_scrolled_into_sight_repaints_where_it_is_shown() {
        let swatch = RefAny::new(VvSwatch { rgba: SWATCH_RED });
        let state = Arc::new(RefCell::new(RefAny::new(ScrolledSwatchPage {
            swatch: swatch.clone(),
        })));
        let mut window = make_window_with(&state, scrolled_swatch_layout);
        window.regenerate_layout().expect("initial layout");
        scroll_outermost_frame_to(&mut window, 300.0);
        window.regenerate_layout().expect("scroll relayout");

        let probe = (50u32, 30u32);
        assert_eq!(
            sample_px(&window, probe.0, probe.1),
            Some(SWATCH_RED),
            "harness: scrolling the view into sight paints it"
        );

        // One frame of new content: the data behind the view changes in
        // place, the view is queued for an in-place re-render (what the video
        // writeback's `trigger_all_virtual_view_rerender` does), and the frame
        // path drains the queue and paints. No relayout.
        set_swatch(&swatch, SWATCH_GREEN);
        window
            .common
            .layout_window
            .as_mut()
            .expect("layout window")
            .queue_all_virtual_view_reinvoke();
        assert!(
            window.common.drain_virtual_view_updates(),
            "harness: the view must re-render"
        );
        paint_frame(&mut window);

        let damage = window.cpu_backend.last_frame_damage.clone();
        assert_eq!(
            sample_px(&window, probe.0, probe.1),
            Some(SWATCH_GREEN),
            "the view re-rendered but its pixels on screen kept the old content: a view inside \
             a scrolled box must be damaged where it is PAINTED (its box minus the scroll \
             offset), not at its content position; damage={damage:?}"
        );
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn a_video_scrolled_into_sight_shows_its_later_frames_too() {
        const PALETTE: [[u8; 4]; 4] = [SWATCH_RED, SWATCH_GREEN, SWATCH_BLUE, SWATCH_YELLOW];

        let frames: Vec<azul_core::video::VideoFrame> =
            PALETTE.iter().map(|c| solid_video_frame(*c)).collect();
        let state = Arc::new(RefCell::new(RefAny::new(ScrolledVideoPage {
            frames: RefAny::new(frames),
        })));
        let mut window = make_window_with(&state, scrolled_video_layout);
        // The first layout mounts the widget, and mounting starts its replay
        // worker: a real thread that writes one frame back about every 33 ms,
        // through the same `video_writeback` the streaming decoder uses.
        window.regenerate_layout().expect("initial layout");
        scroll_outermost_frame_to(&mut window, 300.0);
        window.regenerate_layout().expect("scroll relayout");

        let probe = (50u32, 30u32);
        let mut shown: Vec<usize> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while shown.len() < 3 && std::time::Instant::now() < deadline {
            // One turn of a desktop frame loop: deliver the worker's
            // writebacks (each stores its frame and queues the view's
            // re-render), drain the queue, paint.
            let _ = window.process_timers_and_threads();
            let _ = window.common.drain_virtual_view_updates();
            paint_frame(&mut window);
            if let Some(i) =
                sample_px(&window, probe.0, probe.1).and_then(|px| palette_index(px, &PALETTE))
            {
                if !shown.contains(&i) {
                    shown.push(i);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            shown.len() >= 3,
            "the video played for up to 5 s, and its box on screen showed {} of its 4 frame \
             colours ({shown:?}): every frame reaches the widget and re-renders its view, so a \
             view in a scrolled box must be repainted where it is shown, not a scroll offset \
             below it",
            shown.len()
        );
    }

    /// Three pages of one app: two paragraphs; a 50px box above them (they
    /// move down: a FLIP in flight); then three boxes and no paragraph at
    /// all (nothing of the second page survives but the first box).
    struct FlipPage {
        stage: u8,
    }

    extern "C" fn flip_page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
            },
        };

        fn sized(dom: Dom, width: f32, height: f32) -> Dom {
            dom.with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(width))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(
                        height,
                    ))),
                ]
                .into(),
            )
        }

        let stage = data.downcast_ref::<FlipPage>().map_or(0, |p| p.stage);
        let mut body = Dom::create_body();
        if stage >= 1 {
            body = body.with_child(sized(Dom::create_div(), 40.0, 50.0));
        }
        if stage <= 1 {
            // The paragraphs declare their move (moves are opt-in): pushed down
            // by the box, they slide - a FLIP in flight.
            let moving = |text: &str| {
                Dom::create_p_with_text(text)
                    .with_css("width: 40px; height: 20px; animation: move 300ms ease-out;")
            };
            body = body.with_child(moving("guard")).with_child(moving("line"));
        } else {
            body = body
                .with_child(sized(Dom::create_div_with_text("guard"), 40.0, 20.0))
                .with_child(sized(Dom::create_div_with_text("line"), 40.0, 20.0));
        }
        body
    }

    fn set_flip_stage(state: &Arc<RefCell<RefAny>>, stage: u8) {
        let mut g = state.borrow_mut();
        let r: &mut RefAny = &mut g;
        let mut opt = r.downcast_mut::<FlipPage>();
        if let Some(s) = opt.as_mut() {
            s.stage = stage;
        }
    }

    /// How many reference frames the root display list opens.
    fn dl_reference_frames(window: &HeadlessWindow) -> usize {
        window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_results.get(&azul_core::dom::DomId::ROOT_ID))
            .map_or(0, |r| {
                r.display_list
                    .items
                    .iter()
                    .filter(|i| matches!(i, DisplayListItem::PushReferenceFrame { .. }))
                    .count()
            })
    }

    /// The nodes whose animation transform the GPU value cache holds.
    fn animated_nodes(window: &HeadlessWindow) -> Vec<azul_core::dom::NodeId> {
        let mut nodes: Vec<azul_core::dom::NodeId> = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.gpu_state_manager.get_cache(azul_core::dom::DomId::ROOT_ID))
            .map_or(Vec::new(), |c| c.anim_transform_keys.keys().copied().collect());
        nodes.sort();
        nodes
    }

    /// A FLIP in flight writes its node's transform into the GPU value
    /// cache under the node's id, and a rebuild renumbers the arena. The
    /// cache is remapped like every node-keyed store, but its ANIMATION
    /// channel was left behind under the old ids: the unrelated node that
    /// inherited such an id was wrapped in a reference frame carrying a
    /// stranger's transform. That is how a document mounted over AzWidgets
    /// at 760x400 lost its first line: the `<p>` took node 4 from a widget
    /// mid-move and was painted where that widget had been, off-screen.
    #[test]
    fn a_node_that_inherits_the_id_of_an_animated_node_is_not_painted_through_its_transform() {
        let state = Arc::new(RefCell::new(RefAny::new(FlipPage { stage: 0 })));
        let mut window = make_window_with(&state, flip_page_layout);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        assert_eq!(
            animated_nodes(&window),
            Vec::new(),
            "premise: nothing animates on the first page"
        );

        // The box pushes both paragraphs down by 50px: a FLIP for each,
        // sampled at frame 0 into the cache under their ids.
        set_flip_stage(&state, 1);
        window.regenerate_layout().expect("the paragraphs move");
        let moving = animated_nodes(&window);
        assert!(
            !moving.is_empty(),
            "premise: the moved paragraphs animate (a FLIP is in flight)"
        );
        // The pass that seeds a FLIP builds its display list BEFORE
        // `finish_reconciliation` mints the movers' keys (the "Last" rects
        // must be solved first), so the reference frames show up on the
        // NEXT list build - the one every following frame paints from.
        if let Some(lw) = window.common.layout_window.as_mut() {
            lw.regenerate_display_list_for_dom(azul_core::dom::DomId::ROOT_ID);
        }
        // A mover is painted through a reference frame. (`moving` may also
        // hold the paragraphs' text children - every matched node whose
        // rect moved is seeded - and a text node opens no frame of its own,
        // so the count is not compared to `moving.len()`.)
        assert!(
            dl_reference_frames(&window) > 0,
            "premise: the movers are painted through reference frames once the list is \
             rebuilt with their keys (movers {moving:?})"
        );

        // The paragraphs go; boxes take their ids. Nothing on this page
        // animates (enters are opt-in), so nothing is painted through a
        // transform.
        set_flip_stage(&state, 2);
        window.regenerate_layout().expect("the paragraphs are replaced");
        assert_eq!(
            animated_nodes(&window),
            Vec::new(),
            "the cache keeps no animation transform for a node that is gone: the boxes that \
             inherited the paragraphs' ids ({moving:?}) would be painted through it"
        );
        assert_eq!(
            dl_reference_frames(&window),
            0,
            "a page whose nodes are all new opens no reference frame"
        );
    }

    /// Six 40px stripes of six colours in a row: 240px of content for the
    /// 120px box that shows it, so the view scrolls by up to 120px and no
    /// offset in that range looks like another.
    #[cfg(feature = "cpurender")]
    const STRIPE_COLOURS: [[u8; 4]; 6] = [
        [220, 40, 40, 255],
        [40, 160, 60, 255],
        [40, 80, 220, 255],
        [230, 200, 30, 255],
        [180, 40, 200, 255],
        [30, 200, 210, 255],
    ];

    #[cfg(feature = "cpurender")]
    extern "C" fn stripes_view_render(
        _data: RefAny,
        _info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        stripes_view_return(None)
    }

    /// The stripes view's content, its third stripe painted `tint` when there is one.
    #[cfg(feature = "cpurender")]
    fn stripes_view_return(tint: Option<[u8; 4]>) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                basic::color::ColorU,
                layout::{
                    dimensions::{LayoutHeight, LayoutWidth},
                    display::LayoutDisplay,
                    flex::LayoutFlexDirection,
                },
                property::CssProperty,
                style::background::{StyleBackgroundContent, StyleBackgroundContentVec},
            },
        };

        let row = Dom::create_div().with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::display(LayoutDisplay::Flex)),
                CssPropertyWithConditions::simple(CssProperty::flex_direction(
                    LayoutFlexDirection::Row,
                )),
                CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(240.0))),
                CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(60.0))),
            ]
            .into(),
        );
        let row = STRIPE_COLOURS.iter().enumerate().fold(row, |row, (k, [r, g, b, a])| {
            let [r, g, b, a] = match tint {
                Some(t) if k == 2 => t,
                _ => [*r, *g, *b, *a],
            };
            let bg: StyleBackgroundContentVec =
                vec![StyleBackgroundContent::Color(ColorU { r, g, b, a })].into();
            row.with_child(Dom::create_div().with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(40.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(60.0))),
                    CssPropertyWithConditions::simple(CssProperty::background_content(bg)),
                ]
                .into(),
            ))
        });
        let all = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(240.0, 60.0));
        azul_core::callbacks::VirtualViewReturn {
            dom: azul_core::dom::OptionDom::Some(row),
            materialized: all,
            virtual_rect: all,
        }
    }

    /// The tint the stripes view of [`tinted_stripes_page_layout`] paints its third stripe
    /// with: a dataset the test changes in place.
    #[cfg(feature = "cpurender")]
    struct StripeTint {
        rgba: Option<[u8; 4]>,
    }

    /// The app state of the tinted page: the tint's `RefAny`, the same one across builds.
    #[cfg(feature = "cpurender")]
    struct TintedStripesPage {
        tint: RefAny,
    }

    #[cfg(feature = "cpurender")]
    extern "C" fn tinted_stripes_view_render(
        mut data: RefAny,
        _info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        let tint = data.downcast_ref::<StripeTint>().and_then(|t| t.rgba);
        stripes_view_return(tint)
    }

    /// `<body>` with the stripes view in a 120x60 box (at 8,8: the body's
    /// margin).
    #[cfg(feature = "cpurender")]
    extern "C" fn stripes_page_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        stripes_page(
            RefAny::new(()),
            azul_core::callbacks::VirtualViewCallback::create(stripes_view_render),
        )
    }

    /// [`stripes_page_layout`] with the view's dataset the tint of the page's state.
    #[cfg(feature = "cpurender")]
    extern "C" fn tinted_stripes_page_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let tint = data
            .downcast_ref::<TintedStripesPage>()
            .map(|page| page.tint.clone())
            .expect("page state");
        stripes_page(
            tint,
            azul_core::callbacks::VirtualViewCallback::create(tinted_stripes_view_render),
        )
    }

    #[cfg(feature = "cpurender")]
    fn stripes_page(view_data: RefAny, callback: azul_core::callbacks::VirtualViewCallback) -> Dom {
        use azul_css::{
            dynamic_selector::CssPropertyWithConditions,
            props::{
                layout::dimensions::{LayoutHeight, LayoutWidth},
                property::CssProperty,
            },
        };

        Dom::create_body().with_child(
            Dom::create_virtual_view(view_data, callback).with_css_props(
                vec![
                    CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(120.0))),
                    CssPropertyWithConditions::simple(CssProperty::height(LayoutHeight::px(60.0))),
                ]
                .into(),
            ),
        )
    }

    /// A view glided by fractions of a pixel (a wheel's smooth scroll: 9.3 px,
    /// 8.9 px, ...) is drawn on the device pixel grid, so each frame moves its
    /// pixels and repaints the strip that came in - not the whole view - and
    /// equals a full repaint.
    #[test]
    #[cfg(feature = "cpurender")]
    fn a_view_glided_by_fractions_of_a_pixel_moves_its_pixels() {
        use azul_core::{
            dom::{DomId, NodeId},
            events::ProcessEventResult,
            styled_dom::NodeHierarchyItemId,
        };
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, stripes_page_layout, 200.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        let view = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.virtual_view_manager
                    .get_all_virtual_view_infos()
                    .first()
                    .map(|info| NodeId::new(info.parent_node_id))
            })
            .expect("premise: the page holds one VirtualView");
        for x in [9.3f32, 18.6, 27.4, 36.9] {
            let tier = PlatformWindow::apply_user_change(
                &mut window,
                &CallbackChange::ScrollTo {
                    dom_id: DomId::ROOT_ID,
                    node_id: NodeHierarchyItemId::from_crate_internal(Some(view)),
                    position: LogicalPosition::new(x, 0.0),
                    unclamped: false,
                },
            );
            if tier > ProcessEventResult::DoNothing {
                window.service_frame(tier);
            }
            let damage = window.cpu_backend.last_frame_damage.clone();
            let area: f32 = match &damage {
                FrameDamage::Rects(rs) => rs.iter().map(|r| r.size.width * r.size.height).sum(),
                FrameDamage::Full => f32::INFINITY,
                FrameDamage::None => 0.0,
            };
            assert!(
                area < 120.0 * 60.0 * 0.5,
                "scrolled to x={x}: the frame repainted {area} px^2 of the view - a glide moves \
                 its pixels (damage {damage:?})"
            );
            let (diffs, first) = incremental_vs_full(&mut window);
            assert_eq!(diffs, 0, "x={x}: {diffs} px differ from a full repaint, first at {first:?}");
        }
    }

    /// A `VirtualView` has no scroll frame: its scroll is the `content_offset`
    /// of its display-list item, which the lightweight scroll path re-points
    /// (`patch_virtual_view_content_offset`) and every relayout recomputes.
    /// The CPU backend presents the frame from the display-list diff, and
    /// that diff must see the view move - it compared two views by child,
    /// bounds and clip alone, so a scrolled view produced no damage and the
    /// content stayed put while the scrollbar moved (AzReview's sheet strip
    /// on the Mac: "only the bottom 8px still update"). Many steps, forward
    /// and back: every presented frame equals a full repaint.
    #[test]
    #[cfg(feature = "cpurender")]
    fn a_virtual_view_scrolled_on_the_lightweight_path_paints_where_its_content_moved() {
        use azul_core::{
            dom::{DomId, NodeId},
            events::ProcessEventResult,
            styled_dom::NodeHierarchyItemId,
        };
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, stripes_page_layout, 200.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();

        let view = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.virtual_view_manager
                    .get_all_virtual_view_infos()
                    .first()
                    .map(|info| NodeId::new(info.parent_node_id))
            })
            .expect("premise: the page holds one VirtualView");
        let scroll_x = |window: &HeadlessWindow| {
            window
                .common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.scroll_manager.get_current_offset(DomId::ROOT_ID, view))
                .map_or(0.0, |p| p.x)
        };
        // 2px into the view's box (at 8,8), mid-height: inside stripe
        // `(x + 2) / 40` of the child at scroll offset x.
        let probe = (10u32, 38u32);
        let (d, f) = incremental_vs_full(&mut window);
        assert_eq!(
            d, 0,
            "premise: the settled frame equals a full repaint ({d} px differ, first at {f:?})"
        );

        for (i, x) in [8.0f32, 24.0, 40.0, 57.0, 80.0, 120.0, 96.0, 30.0, 0.0, 119.0]
            .iter()
            .enumerate()
        {
            // The lightweight scroll path: what the physics timer and
            // `CallbackInfo::scroll_to` push, serviced like any change.
            let tier = PlatformWindow::apply_user_change(
                &mut window,
                &CallbackChange::ScrollTo {
                    dom_id: DomId::ROOT_ID,
                    node_id: NodeHierarchyItemId::from_crate_internal(Some(view)),
                    position: LogicalPosition::new(*x, 0.0),
                    unclamped: false,
                },
            );
            if tier > ProcessEventResult::DoNothing {
                window.service_frame(tier);
            }
            let at = scroll_x(&window);
            assert!(
                (at - x).abs() < 0.01,
                "premise: step {i} scrolled the view to x={x}, it is at {at}"
            );
            let stripe = ((x + 2.0) / 40.0) as usize;
            let full = full_repaint_of(&mut window);
            assert_eq!(
                pixel_of(&full, probe.0, probe.1),
                Some(STRIPE_COLOURS[stripe]),
                "premise: a full repaint at offset {x} shows stripe {stripe} at the view's left \
                 edge"
            );
            let (diffs, first) = incremental_vs_full(&mut window);
            assert_eq!(
                diffs, 0,
                "step {i} (view scrolled to x={x}): the presented frame differs from a full \
                 repaint of the same display list in {diffs} px, first at {first:?} - the \
                 view's content did not move with its scroll (frame damage {:?})",
                window.cpu_backend.last_frame_damage
            );
        }
    }

    /// A view that renders AGAIN while it is scrolled - a tile that arrives, a frame of a
    /// video, a caret that goes on - is repainted where its scrolled content is. The page's
    /// list does not change (the view's item is the same), so the frame finds the change by
    /// diffing the view's own lists, and the rects it finds are in the list's 0-relative
    /// space: the view paints it at its box's corner plus the item's `content_offset`
    /// (`window origin - scroll offset`). The damage was placed at the corner MINUS the
    /// offset: right while the view stood at its origin, a mirror image away from the changed
    /// pixels once it was scrolled (a stripe that turned black stayed green on screen).
    #[test]
    #[cfg(feature = "cpurender")]
    fn a_view_that_renders_in_place_while_scrolled_is_repainted_where_its_content_is() {
        use azul_core::{
            dom::{DomId, NodeId},
            events::ProcessEventResult,
            styled_dom::NodeHierarchyItemId,
        };
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let tint = RefAny::new(StripeTint { rgba: None });
        let state = Arc::new(RefCell::new(RefAny::new(TintedStripesPage { tint: tint.clone() })));
        let mut window = make_window_sized(&state, tinted_stripes_page_layout, 200.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        window.common.drain_virtual_view_updates();
        let view = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.virtual_view_manager
                    .get_all_virtual_view_infos()
                    .first()
                    .map(|info| NodeId::new(info.parent_node_id))
            })
            .expect("premise: the page holds one VirtualView");

        // Scrolled by 57 px: the third stripe (content x 80..120) is at x 31..71 of the box,
        // which starts at x = 8; (50, 38) is inside it.
        let probe = (50u32, 38u32);
        let tier = PlatformWindow::apply_user_change(
            &mut window,
            &CallbackChange::ScrollTo {
                dom_id: DomId::ROOT_ID,
                node_id: NodeHierarchyItemId::from_crate_internal(Some(view)),
                position: LogicalPosition::new(57.0, 0.0),
                unclamped: false,
            },
        );
        if tier > ProcessEventResult::DoNothing {
            window.service_frame(tier);
        }
        assert_eq!(
            sample_px(&window, probe.0, probe.1),
            Some(STRIPE_COLOURS[2]),
            "premise: the scrolled view shows its third stripe at the probe"
        );
        let (d, f) = incremental_vs_full(&mut window);
        assert_eq!(d, 0, "premise: the settled frame is a full repaint ({d} px, first at {f:?})");

        // The data behind the view changes in place, the view is queued for a re-render and
        // the frame path drains the queue and paints.
        if let Some(mut t) = tint.clone().downcast_mut::<StripeTint>() {
            t.rgba = Some([0, 0, 0, 255]);
        }
        window
            .common
            .layout_window
            .as_mut()
            .expect("layout window")
            .queue_all_virtual_view_reinvoke();
        assert!(
            window.common.drain_virtual_view_updates(),
            "harness: the view must render again"
        );
        paint_frame(&mut window);

        assert_eq!(
            sample_px(&window, probe.0, probe.1),
            Some([0, 0, 0, 255]),
            "the stripe turned black in the view's list and stayed on screen as it was: the \
             damage of the view's change must be where the scrolled view shows it (damage {:?})",
            window.cpu_backend.last_frame_damage
        );
        let (d, f) = incremental_vs_full(&mut window);
        assert_eq!(d, 0, "{d} px differ from a full repaint, first at {f:?}");
    }

    /// REGRESSION (swallowed sub-pixel scrolling): high-resolution trackpads
    /// deliver deltas well under a device pixel per frame. The scroll baseline
    /// used to advance every frame even when the delta was dropped as
    /// sub-threshold, so the deficit never accumulated — slow scrolling froze
    /// the content entirely. The baseline must stay at the last PAINTED offset
    /// so tiny deltas accumulate until they cross a device pixel.
    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_subpixel_scroll_accumulates() {
        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");

        // Three 0.2px scroll steps. Each individual delta is sub-threshold;
        // cumulatively they cross half a device pixel at 0.6.
        scroll_frame_to(&mut window, 0.2);
        window.regenerate_layout().expect("step 1");
        let d1 = window.cpu_backend.last_frame_damage.clone();
        scroll_frame_to(&mut window, 0.4);
        window.regenerate_layout().expect("step 2");
        let d2 = window.cpu_backend.last_frame_damage.clone();
        scroll_frame_to(&mut window, 0.6);
        window.regenerate_layout().expect("step 3");
        let d3 = window.cpu_backend.last_frame_damage.clone();

        println!(
            "[harness] subpixel damage steps = {:?} / {:?} / {:?}",
            d1, d2, d3
        );
        // A sub-device-pixel scroll must repaint NOTHING — not the content
        // (the frame builder's half-device-pixel threshold drops the shift)
        // and not the scrollbar either (`quantize_thumb_offset` rounds the
        // thumb's GPU value, so a ~0.03 px thumb move is not a value change).
        //
        // This used to be asserted on "the content area, x < 200" — a filter
        // that silently meant "everything but the scrollbar" on macOS ONLY,
        // where the overlay bar hangs off the container's right edge at
        // x=200..208. Windows and Linux reserve a 12 px gutter INSIDE the
        // container, putting the bar at x=196..208, so the filter classified
        // the bar as content and the law read as a content repaint that never
        // happened. Neither platform needs an exemption now, so the honest
        // assertion is the strong one: no damage at all.
        let damaged = |d: &FrameDamage| -> bool { *d != FrameDamage::None };
        assert!(
            !damaged(&d1),
            "0.2px scroll must not repaint ANYTHING: the content shift is below the \
             half-device-pixel threshold and the thumb moves ~0.03px, which quantises to no move \
             at all; got {:?}",
            d1
        );
        assert!(
            !damaged(&d2),
            "0.4px cumulative must not repaint anything yet; got {:?}",
            d2
        );
        assert!(
            damaged(&d3),
            "0.6px CUMULATIVE scroll crossed half a device pixel and must repaint content — if \
             the damage is empty the baseline advanced on skipped frames and slow trackpad \
             scrolling is swallowed forever; got {:?}",
            d3
        );
    }

    /// REGRESSION (idle skip with scrollbars): a no-op relayout of a window
    /// WITH a scrollbar must reach `FrameDamage::None`. ScrollBarStyled used
    /// to fall into `is_visually_equal`'s `_ => false` catch-all, so every
    /// scrollbar'd window re-damaged its bar every frame — the skip path was
    /// unreachable and idle windows re-rendered + re-presented forever (the
    /// thumb position now flows through the GPU-value damage channel instead).
    /// Scrolling must MOVE the pixels it already has, not repaint them.
    ///
    /// `render_frame` memmoves the still-visible part of a scrolled frame
    /// inside the retained pixmap (`scroll_shift_region`) and repaints only
    /// the strip that scrolled into view — but ONLY when
    /// `scroll_fast_path_eligible` says the content opaquely covers the clip
    /// at both the old and new offsets. When it says no, the else-branch
    /// pushes the WHOLE clip as damage and every scrolled pixel is
    /// re-rasterized.
    ///
    /// Nothing pinned which of those two happens, so the fast path could
    /// silently stop firing — turning every pan into a full repaint of the
    /// scrolled area — without a single test noticing. This asserts the
    /// painted area stays far below the clip area.
    #[test]
    fn damage_scroll_takes_the_memmove_fast_path() {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        // Second render so the next one is incremental (the first frame is a
        // full repaint by definition, and the fast path only runs then).
        window.regenerate_layout().expect("settle");

        // The scroll clip from `harness_layout_scroll` / `scroll_frame_to`.
        let clip = LogicalRect {
            origin: LogicalPosition::new(8.0, 8.0),
            size: LogicalSize::new(200.0, 100.0),
        };
        let clip_area = clip.size.width * clip.size.height;

        scroll_frame_to(&mut window, 30.0);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] scroll-30 PAINT damage = {damage:?}");

        // Area actually re-rasterized inside the scroll clip.
        let painted: f32 = match &damage {
            FrameDamage::Full => clip_area,
            FrameDamage::None => 0.0,
            FrameDamage::Rects(rs) => rs
                .iter()
                .filter_map(|r| {
                    let x0 = r.origin.x.max(clip.origin.x);
                    let y0 = r.origin.y.max(clip.origin.y);
                    let x1 = (r.origin.x + r.size.width).min(clip.origin.x + clip.size.width);
                    let y1 = (r.origin.y + r.size.height).min(clip.origin.y + clip.size.height);
                    if x1 > x0 && y1 > y0 {
                        Some((x1 - x0) * (y1 - y0))
                    } else {
                        None
                    }
                })
                .sum(),
        };

        // A 30px scroll of a 100px-tall clip exposes a 30px strip, so ~30% of
        // the clip plus the scrollbar column. The threshold is deliberately
        // loose — the point is to catch "the whole clip got repainted"
        // (100%), which is what an ineligible fast path produces.
        let ratio = painted / clip_area;
        println!("[harness] painted {painted:.0}px2 of clip {clip_area:.0}px2 = {ratio:.2}");
        assert!(
            ratio < 0.6,
            "scrolling 30px of a 100px clip repainted {:.0}% of it. The memmove fast path did not \
             fire, so every pan re-rasterizes the whole scrolled area instead of shifting the \
             pixels it already has. damage = {damage:?}",
            ratio * 100.0
        );
        assert!(
            painted > 0.0,
            "scrolling repainted NOTHING — the newly exposed strip must still be painted, or \
             scrolled-in content is stale pixels"
        );
    }

    /// A scroller whose bar is hidden, so a scroll step that moves no whole
    /// device pixel changes nothing at all on screen.
    extern "C" fn harness_layout_scroll_no_bar(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let mut container = Dom::create_div().with_css(
            "width: 200px; height: 100px; overflow-y: scroll; scrollbar-width: none;",
        );
        for i in 0..20 {
            let bg = if i % 2 == 0 { "#c83c3c" } else { "#3c78c8" };
            container = container.with_child(
                Dom::create_div()
                    .with_css(&format!("height: 20px; background-color: {bg};")),
            );
        }
        Dom::create_body().with_child(container)
    }

    /// THE WRITES-WITHIN-DAMAGE LAW for a frame that has nothing to repaint.
    ///
    /// A scroll step smaller than half a device pixel shifts no pixel and
    /// damages nothing. The incremental renderer then fell into its FULL
    /// repaint branch (the branch is chosen by "damage is non-empty", not by
    /// "this frame is incremental") while still reporting empty damage. On a
    /// native ARGB8888 commit-swizzle pool that wrote every pixel of the
    /// compositor's buffer in renderer byte order and nothing converted them:
    /// the whole window turned R<->B swapped the next time that buffer was
    /// shown (blue UI turning orange after scrolling, KDE Wayland). On every
    /// other target it was a wasted full repaint per smooth-scroll tail frame.
    #[test]
    #[cfg(feature = "cpurender")]
    fn a_frame_with_nothing_to_repaint_writes_no_pixel_outside_its_damage() {
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_with(&state, harness_layout_scroll_no_bar);
        window.regenerate_layout().expect("initial layout");
        // 10.6 rounds to 11 device pixels; so does 11.2. The step between them
        // is larger than half a pixel (the fast path takes it) yet shifts by
        // round(11.2) - round(10.6) = 0 pixels: nothing moves, nothing is
        // exposed - a frame with nothing to repaint.
        scroll_frame_to(&mut window, 10.6);
        window.regenerate_layout().expect("scroll to 10.6");

        const MARK: [u8; 4] = [1, 2, 3, 4];
        {
            let frame = window.cpu_backend.last_frame.as_mut().expect("retained frame");
            for px in frame.data_mut().chunks_exact_mut(4) {
                px.copy_from_slice(&MARK);
            }
        }
        scroll_frame_to(&mut window, 11.2);
        window.regenerate_layout().expect("zero-pixel scroll step");

        let damage = window.cpu_backend.last_frame_damage.clone();
        assert!(
            matches!(&damage, FrameDamage::Rects(rs) if rs.is_empty()),
            "premise: the step must be an incremental frame with nothing to repaint, got \
             {damage:?}"
        );
        let frame = window.cpu_backend.last_frame.as_ref().expect("retained frame");
        let written = frame.data().chunks_exact(4).filter(|px| *px != MARK).count();
        assert_eq!(
            written, 0,
            "a frame that reports no damage wrote {written} pixels (a full repaint)"
        );
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn damage_idle_scrollbar_window_skips() {
        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        // Second render, nothing changed at all.
        window.regenerate_layout().expect("no-op relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        assert_eq!(
            damage,
            FrameDamage::None,
            "an idle window with a scrollbar must skip (FrameDamage::None);              non-None \
             means the scrollbar (or another item) produces false              per-frame damage \
             and idle windows burn CPU forever"
        );
        // And scrolling must still damage the bar (thumb moved → GPU value
        // diff) — the equality arm must not have frozen the thumb.
        scroll_frame_to(&mut window, 30.0);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        // The bar's own rect, read off the display list rather than
        // re-derived from per-OS constants: the platform decides the
        // scrollbar width (8px overlay where the UA sheet says `thin`, 12px
        // where it says `auto`) AND whether it reserves a gutter, so the only
        // honest source for "where is the bar" is where layout put it. A
        // hard-coded x also made this law vacuous on overlay platforms, where
        // the bar sits inside the scroll clip and the exposed-strip damage
        // covers those columns whether or not the thumb was damaged at all.
        use azul_core::dom::ScrollbarOrientation;
        let bar = {
            let lw = window.common.layout_window.as_ref().expect("layout window");
            let r = lw
                .layout_results
                .get(&azul_core::dom::DomId::ROOT_ID)
                .expect("root layout result");
            r.display_list
                .items
                .iter()
                .find_map(|it| match it {
                    DisplayListItem::ScrollBarStyled { info }
                        if info.orientation == ScrollbarOrientation::Vertical =>
                    {
                        Some(info.bounds.0)
                    }
                    _ => None,
                })
                .expect("the scrollable container must paint a vertical scrollbar")
        };
        match &damage {
            FrameDamage::Rects(rs) => {
                // The thumb moved (GPU value cache), so the bar's rect must be
                // covered by the damage — by a single rect, since that is how
                // the value diff raises it.
                let covers = |r: &azul_core::geom::LogicalRect| {
                    r.origin.x <= bar.origin.x + 0.5
                        && r.origin.y <= bar.origin.y + 0.5
                        && r.origin.x + r.size.width >= bar.origin.x + bar.size.width - 0.5
                        && r.origin.y + r.size.height >= bar.origin.y + bar.size.height - 0.5
                };
                assert!(
                    rs.iter().any(covers),
                    "scroll must damage the scrollbar {bar:?} (the thumb moved via the GPU value \
                     cache, and the display-list items compare equal, so nothing else can raise \
                     it); got {rs:?}"
                );
            }
            other => panic!("scroll should be incremental, got {:?}", other),
        }
    }

    // --- The press router: scripted and physical presses, one arbitration ---

    /// A 200x100 box with a CLASSIC vertical scrollbar (reserved, always
    /// shown) over 20 rows of 30px. Classic on purpose: whether an overlay
    /// bar or a `scrollbar-width: none` bar exists at all is the
    /// scrollbar-presence question, not the press router's.
    extern "C" fn harness_layout_classic_scrollbar(
        _data: RefAny,
        _info: LayoutCallbackInfo,
    ) -> Dom {
        let mut container = Dom::create_div().with_css(
            "width: 200px; height: 100px; overflow-y: scroll; \
             -azul-scrollbar-visibility: always; scrollbar-width: auto;",
        );
        for i in 0..20 {
            let bg = if i % 2 == 0 { "#c83c3c" } else { "#3c78c8" };
            container = container.with_child(
                Dom::create_div().with_css(&format!("height: 30px; background-color: {bg};")),
            );
        }
        Dom::create_body().with_child(container)
    }

    /// The classic box's scroll node and the window point at the centre of
    /// its vertical thumb, read off the bar the scroll manager built.
    fn classic_thumb(
        window: &HeadlessWindow,
    ) -> (azul_core::dom::DomId, azul_core::dom::NodeId, LogicalPosition) {
        use azul_core::dom::ScrollbarOrientation;
        use azul_layout::managers::scroll_state::ScrollbarComponent;

        let sm = &window
            .common
            .layout_window
            .as_ref()
            .expect("layout window")
            .scroll_manager;
        let (dom, node) = sm
            .state_keys()
            .into_iter()
            .find(|&(d, n)| {
                sm.get_scrollbar_state(d, n, ScrollbarOrientation::Vertical)
                    .is_some()
            })
            .expect("the overflowing box must carry a vertical scrollbar");
        let bar = sm
            .get_scrollbar_state(dom, node, ScrollbarOrientation::Vertical)
            .expect("found above");
        let at = LogicalPosition::new(
            bar.track_rect.origin.x + bar.track_rect.size.width / 2.0,
            bar.track_rect.origin.y + bar.button_size + bar.thumb_offset + bar.thumb_length / 2.0,
        );
        assert!(
            matches!(
                sm.hit_test_scrollbars(at).map(|h| h.component),
                Some(ScrollbarComponent::Thumb)
            ),
            "premise: {at:?} is on the thumb of the classic bar {bar:?}"
        );
        (dom, node, at)
    }

    /// Push the primary pointer the way a SCRIPTED op does: the
    /// `mouse_move` / `mouse_down` / `mouse_up` ops all end in
    /// `modify_window_state`, i.e. a whole-state push through
    /// `CallbackChange::ModifyWindowState`, serviced like any callback's
    /// change.
    fn push_pointer(window: &mut HeadlessWindow, at: LogicalPosition, left_down: bool) {
        use azul_core::{events::ProcessEventResult, window::CursorPosition};
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let mut state = window.get_current_window_state().clone();
        state.mouse_state.cursor_position = CursorPosition::InWindow(at);
        state.mouse_state.left_down = left_down;
        let tier = PlatformWindow::apply_user_change(
            window,
            &CallbackChange::ModifyWindowState { state },
        );
        if tier > ProcessEventResult::DoNothing {
            window.service_frame(tier);
        }
    }

    fn scroll_y_of(
        window: &HeadlessWindow,
        dom: azul_core::dom::DomId,
        node: azul_core::dom::NodeId,
    ) -> f32 {
        window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.scroll_manager.get_current_offset(dom, node))
            .map_or(0.0, |o| o.y)
    }

    /// ONE press router: a SCRIPTED press on a scrollbar thumb must reach the
    /// same arbitration as a physical one - scrollbar first, then content -
    /// and the same drag must scroll the box the same.
    ///
    /// The physical press is asked "is this a scrollbar?" by the backend
    /// before anything else. The scripted one (`DebugEvent::MouseDown` ->
    /// `modify_window_state` -> the `ModifyWindowState` arm) went straight to
    /// the event pass and became a `MouseDown` on the box. So an AZ_E2E
    /// script against the real app could never reproduce a press a
    /// scrollbar took on the device.
    #[test]
    fn a_scripted_press_on_a_scrollbar_thumb_drags_it_like_a_physical_press() {
        use azul_core::events::MouseButton;

        use crate::desktop::shell2::common::event::PlatformWindow;

        const DRAG_PX: f32 = 20.0;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut physical = make_window_with(&state, harness_layout_classic_scrollbar);
        physical.regenerate_layout().expect("initial layout");
        let mut scripted = make_window_with(&state, harness_layout_classic_scrollbar);
        scripted.regenerate_layout().expect("initial layout");

        let (dom, node, at) = classic_thumb(&physical);
        assert_eq!(
            classic_thumb(&scripted),
            (dom, node, at),
            "premise: two identical windows lay the bar out identically"
        );
        let to = LogicalPosition::new(at.x, at.y + DRAG_PX);

        // PHYSICAL: the headless backend's native ingress, which asks the
        // scrollbars first like every desktop backend.
        step(&mut physical, HeadlessEvent::MouseMove { x: at.x, y: at.y });
        step(
            &mut physical,
            HeadlessEvent::MouseDown {
                button: MouseButton::Left,
            },
        );
        let physical_held = physical.get_scrollbar_drag_state().is_some();
        step(&mut physical, HeadlessEvent::MouseMove { x: to.x, y: to.y });
        let physical_scroll = scroll_y_of(&physical, dom, node);
        step(
            &mut physical,
            HeadlessEvent::MouseUp {
                button: MouseButton::Left,
            },
        );

        // SCRIPTED: the same gesture as the ops a script sends.
        push_pointer(&mut scripted, at, false);
        push_pointer(&mut scripted, at, true);
        let scripted_held = scripted.get_scrollbar_drag_state().is_some();
        push_pointer(&mut scripted, to, true);
        let scripted_scroll = scroll_y_of(&scripted, dom, node);
        push_pointer(&mut scripted, to, false);

        assert!(physical_held, "premise: a physical press on the thumb grabs it");
        assert!(
            physical_scroll > 0.0,
            "premise: dragging the grabbed thumb {DRAG_PX}px scrolls the box"
        );
        assert!(
            scripted_held,
            "a scripted press on the thumb must grab it too: the same press router, scrollbar \
             first (it went to the content under the bar)"
        );
        assert!(
            (scripted_scroll - physical_scroll).abs() < 0.01,
            "the same drag must scroll the box the same: physical {physical_scroll}, scripted \
             {scripted_scroll}"
        );
        for (name, window) in [("physical", &physical), ("scripted", &scripted)] {
            assert!(
                window.get_scrollbar_drag_state().is_none(),
                "{name}: the release lets go of the thumb"
            );
            assert!(
                !window.get_current_window_state().mouse_state.left_down,
                "{name}: the release clears the button the press latched"
            );
        }
    }

    /// Sample the RGBA of the last rendered frame at physical pixel (x, y).
    #[cfg(feature = "cpurender")]
    fn sample_px(window: &HeadlessWindow, x: u32, y: u32) -> Option<[u8; 4]> {
        pixel_of(window.cpu_backend.last_frame.as_ref()?, x, y)
    }

    /// One pixel of `pm`, RGBA.
    #[cfg(feature = "cpurender")]
    fn pixel_of(pm: &azul_layout::cpurender::AzulPixmap, x: u32, y: u32) -> Option<[u8; 4]> {
        let (w, h) = (pm.width(), pm.height());
        if x >= w || y >= h {
            return None;
        }
        let d = pm.data();
        let i = ((y * w + x) * 4) as usize;
        if i + 4 > d.len() {
            return None;
        }
        Some([d[i], d[i + 1], d[i + 2], d[i + 3]])
    }

    /// Write the window's last frame to `/tmp/<name>.png` for visual inspection.
    /// Best-effort: silently does nothing if there's no frame / encode fails.
    #[cfg(feature = "cpurender")]
    fn save_frame_png(window: &HeadlessWindow, name: &str) {
        if let Some(pm) = window.cpu_backend.last_frame.as_ref() {
            if let Ok(bytes) = pm.encode_png() {
                let _ = std::fs::write(format!("/tmp/{}.png", name), bytes);
            }
        }
    }

    /// Count pixels (and the max per-channel delta) that differ between two
    /// pixmaps. (usize::MAX, 255) if the dimensions differ.
    #[cfg(feature = "cpurender")]
    fn pixmap_diff(
        pa: &azul_layout::cpurender::AzulPixmap,
        pb: &azul_layout::cpurender::AzulPixmap,
    ) -> (usize, u8) {
        if pa.width() != pb.width() || pa.height() != pb.height() {
            return (usize::MAX, 255);
        }
        let (da, db) = (pa.data(), pb.data());
        let mut diff_px = 0usize;
        let mut max_d = 0u8;
        for (ca, cb) in da.chunks_exact(4).zip(db.chunks_exact(4)) {
            let d = (0..4)
                .map(|k| (ca[k] as i16 - cb[k] as i16).unsigned_abs() as u8)
                .max()
                .unwrap_or(0);
            if d > 0 {
                diff_px += 1;
                max_d = max_d.max(d);
            }
        }
        (diff_px, max_d)
    }

    /// Render the window's CURRENT state as a full, offset-aware frame using the
    /// offset-applying rasteriser (`render_display_list_damaged` over the whole
    /// viewport) — the trustworthy "what it should look like" reference, independent
    /// of the incremental and compositor paths.
    #[cfg(feature = "cpurender")]
    fn offset_aware_reference(w: &mut HeadlessWindow) -> azul_layout::cpurender::AzulPixmap {
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
        };
        let (pw, ph) = w
            .cpu_backend
            .last_frame
            .as_ref()
            .map(|p| (p.width(), p.height()))
            .unwrap_or((1, 1));
        let dpi = {
            let ws = w.common.current_window_state();
            ws.size.dpi as f32 / 96.0
        };
        let mut reference = azul_layout::cpurender::AzulPixmap::new(pw, ph).expect("ref pixmap");
        reference.fill(255, 255, 255, 255);
        let lw = w.common.layout_window.as_ref().unwrap();
        let dom = DomId { inner: 0 };
        let result = lw.layout_results.get(&dom).unwrap();
        let offsets = lw
            .scroll_manager
            .build_scroll_offset_map(dom, &result.scroll_id_to_node_id);
        // The GPU value cache is part of the frame's state, not decoration:
        // scrollbar thumb positions and fade opacities live ONLY there (the
        // display list carries the keys). A reference built without them
        // paints every thumb at its display-list-baked initial position, so
        // it would call a correctly-moved thumb a diff.
        let (gpu_transforms, gpu_opacities) =
            azul_layout::cpurender::extract_gpu_values(lw.gpu_state_manager.get_cache(dom), dom);
        let mut rs = azul_layout::cpurender::CpuRenderState::new(offsets)
            .with_system_style(lw.system_style.clone());
        rs.transforms = gpu_transforms;
        rs.opacities = gpu_opacities;
        let full_clip = LogicalRect {
            origin: LogicalPosition::new(0.0, 0.0),
            size: LogicalSize::new(pw as f32 / dpi, ph as f32 / dpi),
        };
        let _ = azul_layout::cpurender::render_display_list_damaged(
            &result.display_list,
            &mut reference,
            dpi,
            &w.common.renderer_resources,
            &lw.font_manager,
            &mut w.cpu_backend.glyph_cache,
            &rs,
            &[full_clip],
        );
        reference
    }

    /// Render a scrolled state TWO ways — the incremental fast path (memmove +
    /// strip) and a forced full re-render at the same offset — and assert they're
    /// pixel-identical. This is the rigorous proof that the scroll-shift fast path
    /// is correct. Saves `/tmp/<tag>_fast.png` and `/tmp/<tag>_full.png` for the
    /// human to eyeball, and returns the fast-path damage.
    #[cfg(feature = "cpurender")]
    fn assert_fast_matches_full_scroll(
        cb: azul_core::callbacks::LayoutCallbackType,
        dx: f32,
        dy: f32,
        tag: &str,
    ) -> FrameDamage {
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut w = make_window_with(&state, cb);
        w.regenerate_layout().expect("initial layout");
        let node = w
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.layout_cache
                    .scroll_id_to_node_id
                    .values()
                    .next()
                    .copied()
            })
            .expect("scroll frame should exist");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(dx, dy),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        w.common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node, sp);
        // Incremental fast path.
        w.regenerate_layout().expect("scroll (fast)");
        let damage = w.cpu_backend.last_frame_damage.clone();
        save_frame_png(&w, &format!("{}_fast", tag));
        let fast = w
            .cpu_backend
            .last_frame
            .as_ref()
            .map(|p| p.clone_pixmap())
            .expect("fast frame");

        // Correct reference: a FULL offset-aware render of the whole viewport via
        // the offset-applying rasteriser.
        let full = offset_aware_reference(&mut w);
        if let Ok(bytes) = full.encode_png() {
            let _ = std::fs::write(format!("/tmp/{}_full.png", tag), bytes);
        }

        let (diff_px, max_d) = pixmap_diff(&fast, &full);
        println!(
            "[harness] {tag}: fast-vs-full diff_px={diff_px} max_delta={max_d} (PNGs in \
             /tmp/{tag}_*.png)"
        );
        assert_eq!(
            diff_px, 0,
            "{tag}: fast-path scroll is NOT pixel-identical to a full re-render ({diff_px} px \
             differ, max channel delta {max_d}). The memmove produced a wrong frame — see \
             /tmp/{tag}_fast.png vs /tmp/{tag}_full.png",
        );
        damage
    }

    #[test]
    fn scroll_moves_content_not_just_scrollbar() {
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 20 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        #[cfg(feature = "cpurender")]
        let before_px = sample_px(&window, 50, 20);

        // Find the scroll frame's node (overflow:scroll should register one).
        let (n_scroll_nodes, scroll_node) = window
            .common
            .layout_window
            .as_ref()
            .map(|lw| {
                (
                    lw.layout_cache.scroll_id_to_node_id.len(),
                    lw.layout_cache
                        .scroll_id_to_node_id
                        .values()
                        .next()
                        .copied(),
                )
            })
            .unwrap_or((0, None));
        println!("[harness] scroll frames registered = {}", n_scroll_nodes);
        let node_id = match scroll_node {
            Some(n) => n,
            None => panic!(
                "overflow:scroll created NO scroll frame (scroll_id_to_node_id empty) — content \
                 {}px in a 100px container should be scrollable",
                20 * 30
            ),
        };

        // Scroll down by 30px (one row).
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(200.0, 600.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        println!("[harness] scroll damage = {:?}", damage);

        // HONEST: scrolling must move the CONTENT, not just the scrollbar. The
        // rows alternate colour every 30px, so a 30px scroll swaps the colour at
        // a fixed viewport pixel. If it's unchanged, the content is FROZEN on
        // scroll — only the scrollbar moved (damage was scrollbar-only). The
        // scroll_layer shift is dead code and content items don't shift in the DL
        // (§0.6). A weak "damage != None / bounded" assertion would FAKE-PASS on
        // the scrollbar alone — so we check the rendered pixels directly.
        #[cfg(feature = "cpurender")]
        {
            let after_px = sample_px(&window, 50, 20);
            println!(
                "[harness] content px @ (50,20): before={:?} after={:?}",
                before_px, after_px
            );
            assert!(
                before_px.is_some() && after_px.is_some(),
                "no rendered pixmap to sample (before={:?} after={:?})",
                before_px,
                after_px
            );
            assert_ne!(
                before_px, after_px,
                "scroll did NOT change the content at (50,20) — content is FROZEN on scroll; only \
                 the scrollbar moved (damage={:?}). scroll_layer is dead code (§0.6) and content \
                 items don't shift in the display list.",
                damage
            );
        }
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn scroll_present_damage_larger_than_paint_damage() {
        // The render-vs-present split: scrolling PAINTS a thin strip but PRESENTS
        // the whole clip (the pixels moved on screen). Paint damage must stay a
        // strip; present damage must cover the full clip.
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.layout_cache
                    .scroll_id_to_node_id
                    .values()
                    .next()
                    .copied()
            })
            .expect("scroll frame should exist");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll");

        let paint = damage_area(&window.cpu_backend.last_frame_damage);
        let present = damage_area(&window.cpu_backend.last_present_damage);
        println!("[harness] paint={:?} present={:?}", paint, present);
        let (paint, present) = (
            paint.expect("paint finite"),
            present.expect("present finite"),
        );
        // Paint stays a strip; present covers the ~188x100 clip; present > paint.
        assert!(
            paint <= 10_000.0,
            "paint damage should be a strip, got {paint}px"
        );
        assert!(
            present >= 18_000.0,
            "present damage should cover the full ~188x100 clip, got {present}px"
        );
        assert!(
            present > paint,
            "present ({present}) must exceed paint ({paint})"
        );
    }

    #[test]
    fn scroll_repaint_pixels_is_strip() {
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut window = make_window_with(&state, harness_layout_scroll);
        window.regenerate_layout().expect("initial layout");
        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.layout_cache
                    .scroll_id_to_node_id
                    .values()
                    .next()
                    .copied()
            })
            .expect("scroll frame should exist");

        // Scroll down 30px (one row) in the 200x100 viewport.
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(200.0, 3000.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        let pixels = damage_area(&damage);
        println!(
            "[harness] scroll repaint pixels = {:?} damage = {:?}",
            pixels, damage
        );

        // HONEST perf metric — count pixels REPAINTED, not m×n (the whole
        // viewport). Scrolling a 200x100 viewport by 30px should repaint ~a 30px
        // content strip + the scrollbar (~30*188 + 12*100 ≈ 6.8k px), NOT the full
        // viewport (~188*100 + 12*100 ≈ 20k px). Wall-time is noisy and dominated
        // by relayout (which real scroll skips); the repainted-pixel count is the
        // deterministic signal. Currently a full-viewport re-render (scroll_layer
        // pixel-shift unwired) → FAILS here until #14 cuts the paint to a strip.
        match pixels {
            Some(px) => assert!(
                px <= 10_000.0,
                "scroll repainted {} px — should be a ~30px strip + scrollbar (~6.8k px), not the \
                 full viewport (~20k px = m×n). Wire scroll_layer pixel-shift (#14). damage={:?}",
                px,
                damage
            ),
            None => panic!(
                "scroll produced Full damage — worse than full-viewport. damage={:?}",
                damage
            ),
        }
    }

    #[test]
    fn scroll_diagonal_pan_two_strips() {
        // #16 mobile pan: a DIAGONAL scroll (both axes in one frame) must repaint
        // an L-shape (a bottom strip + a right strip), not the whole viewport and
        // not fall back to a full-clip repaint. Exercises the single-pass 2-D
        // shift end-to-end through render_frame.
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut window = make_window_with(&state, harness_layout_scroll_2d);
        window.regenerate_layout().expect("initial layout");
        #[cfg(feature = "cpurender")]
        let before_px = sample_px(&window, 50, 20);
        let node_id = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.layout_cache
                    .scroll_id_to_node_id
                    .values()
                    .next()
                    .copied()
            })
            .expect("2-axis scroll frame should exist");

        // Pan down-right: 20px right + 30px down (one row).
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(20.0, 30.0),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        window
            .common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node_id, sp);
        window.regenerate_layout().expect("scroll relayout");
        let damage = window.cpu_backend.last_frame_damage.clone();
        let pixels = damage_area(&damage);
        println!(
            "[harness] diagonal pan pixels = {:?} damage = {:?}",
            pixels, damage
        );

        // Perf: the L-shape (two thin strips + scrollbars) must stay well under a
        // full re-render. A diagonal that fell back to a full-clip repaint (the
        // pre-#16 behaviour) would land near the ~17-20k full viewport.
        match pixels {
            Some(px) => assert!(
                px > 0.0 && px <= 12_000.0,
                "diagonal pan repainted {} px — expected a thin L-shape (two strips + \
                 scrollbars), not a full-clip repaint. damage={:?}",
                px,
                damage
            ),
            None => panic!("diagonal pan produced Full damage. damage={:?}", damage),
        }

        // The damage must contain at least TWO content strips (one per axis) — a
        // single strip would mean only one axis actually scrolled.
        if let FrameDamage::Rects(rs) = &damage {
            let content_strips = rs
                .iter()
                .filter(|r| r.size.width > 20.0 && r.size.height > 20.0)
                .count();
            assert!(
                content_strips >= 2,
                "diagonal pan must expose TWO content strips (bottom + right), got {} sizeable \
                 rects in {:?}",
                content_strips,
                damage
            );
        }

        // Correctness: content actually moved (the row colour at a fixed pixel
        // flips on the 30px vertical component of the pan).
        #[cfg(feature = "cpurender")]
        {
            let after_px = sample_px(&window, 50, 20);
            assert!(before_px.is_some() && after_px.is_some());
            assert_ne!(
                before_px, after_px,
                "diagonal pan did not move content at (50,20) — before={:?} after={:?}",
                before_px, after_px
            );
        }
    }

    // #21: PNG visual tests. The fast path (memmove + strip repaint) must produce
    // a frame byte-identical to a full re-render at the same offset. These render
    // both ways, assert pixel-equality, and drop PNGs in /tmp for eyeballing.
    #[test]
    #[cfg(feature = "cpurender")]
    fn png_scroll_vertical_fast_matches_full_render() {
        let damage =
            assert_fast_matches_full_scroll(harness_layout_scroll, 0.0, 30.0, "scroll_vert");
        // It really took the fast path (a strip), not a full clip repaint.
        match damage_area(&damage) {
            Some(px) => assert!(
                px <= 10_000.0,
                "vertical scroll should be a thin strip via the fast path, got {px}px {:?}",
                damage
            ),
            None => panic!("vertical scroll produced Full damage: {:?}", damage),
        }
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn png_scroll_diagonal_fast_matches_full_render() {
        let damage =
            assert_fast_matches_full_scroll(harness_layout_scroll_2d, 20.0, 30.0, "scroll_diag");
        match damage_area(&damage) {
            Some(px) => assert!(
                px <= 12_000.0,
                "diagonal pan should be two strips via the fast path, got {px}px {:?}",
                damage
            ),
            None => panic!("diagonal pan produced Full damage: {:?}", damage),
        }
    }

    #[test]
    #[cfg(feature = "cpurender")]
    fn png_scroll_compositor_full_render_applies_offset() {
        // #18 fix: the COMPOSITOR full-render path (render_layers) must apply scroll
        // offsets. It used to render with an empty offset map → a full repaint while
        // scrolled drew content at offset 0. Force the compositor full path at a
        // 30px scroll and assert it matches the offset-aware reference.
        use azul_core::{
            dom::DomId,
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            hit_test::ScrollPosition,
        };

        let state = Arc::new(RefCell::new(RefAny::new(ScrollTestState { n_items: 100 })));
        let mut w = make_window_with(&state, harness_layout_scroll);
        w.regenerate_layout().expect("initial");
        let node = w
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| {
                lw.layout_cache
                    .scroll_id_to_node_id
                    .values()
                    .next()
                    .copied()
            })
            .expect("scroll frame");
        let sp = ScrollPosition {
            parent_rect: LogicalRect {
                origin: LogicalPosition::new(8.0, 8.0),
                size: LogicalSize::new(200.0, 100.0),
            },
            children_rect: LogicalRect {
                origin: LogicalPosition::new(0.0, 30.0),
                size: LogicalSize::new(400.0, 3000.0),
            },
        };
        w.common
            .layout_window
            .as_mut()
            .unwrap()
            .set_scroll_position(DomId { inner: 0 }, node, sp);
        w.regenerate_layout().expect("scroll (incremental)");
        // Force the FULL (compositor) path for the next frame at the same offset.
        w.cpu_backend.previous_display_list = None;
        w.regenerate_layout().expect("compositor full");
        save_frame_png(&w, "scroll_compositor_full");
        let comp = w
            .cpu_backend
            .last_frame
            .as_ref()
            .map(|p| p.clone_pixmap())
            .expect("compositor frame");
        let reference = offset_aware_reference(&mut w);
        let (diff_px, max_d) = pixmap_diff(&comp, &reference);
        println!(
            "[harness] compositor-full vs offset-aware reference: diff_px={diff_px} max={max_d}"
        );
        // Allow a tiny tolerance for AA/compositing path differences; the pre-fix
        // bug was a whole-viewport mismatch (~18k px, full row phase wrong).
        assert!(
            diff_px < 200,
            "compositor full-render does not apply the scroll offset (diff {diff_px}px, max delta \
             {max_d}) — see /tmp/scroll_compositor_full.png",
        );
    }

    #[test]
    fn test_stub_window_close() {
        let mut window = make_stub();
        window.close();
        assert!(!window.is_open());
    }

    #[test]
    fn test_stub_event_injection() {
        let mut window = make_stub();

        assert!(window.poll_event().is_none());

        window.inject_event(HeadlessEvent::MouseMove { x: 100.0, y: 200.0 });
        window.inject_event(HeadlessEvent::Close);

        assert!(matches!(
            window.poll_event(),
            Some(HeadlessEvent::MouseMove { .. })
        ));
        assert!(matches!(window.poll_event(), Some(HeadlessEvent::Close)));
        assert!(window.poll_event().is_none());
    }

    #[test]
    fn test_stub_timer_management() {
        let mut window = make_stub();
        assert!(!window.has_active_timers());

        let get_time = azul_core::task::GetSystemTimeCallback {
            cb: azul_core::task::get_system_time_libstd,
        };
        let timer = azul_layout::timer::Timer::create(
            RefAny::new(()),
            test_timer_callback as azul_layout::timer::TimerCallbackType,
            get_time,
        );
        window.start_timer(1, timer);
        assert!(window.has_active_timers());

        window.stop_timer(1);
        assert!(!window.has_active_timers());
    }

    #[test]
    fn test_stub_window_create_queue() {
        let mut window = make_stub();
        assert_eq!(window.pending_window_count(), 0);

        window.queue_window_create(WindowCreateOptions::default());
        assert_eq!(window.pending_window_count(), 1);
    }

    #[test]
    fn test_cpu_backend_creation() {
        let backend = CpuBackend::new();
        let results = backend
            .hit_tester
            .hit_test(azul_core::geom::LogicalPosition { x: 0.0, y: 0.0 });
        assert!(results.is_empty());
    }

    extern "C" fn test_timer_callback(
        _data: RefAny,
        _info: azul_layout::timer::TimerCallbackInfo,
    ) -> azul_core::callbacks::TimerCallbackReturn {
        azul_core::callbacks::TimerCallbackReturn::terminate_unchanged()
    }

    // Idle-CPU laws: animation culling, frame requests, idle timers
    // (`tests/idle_cpu.rs`).
    mod idle_cpu;

    // The AZ_E2E / AZ_DEBUG host: what a script can see and drive through
    // this backend (`tests/e2e_host.rs`, HEADLESS6).
    mod e2e_host;

    // A permission-bearing node subscribes under its own node id
    // (`tests/permission_probe.rs`, TEXT7).
    mod permission_probe;

    // A native command runs as the keystroke it stands for
    // (`tests/shortcut_keys.rs`, EVENTS7).
    mod shortcut_keys;

    // Space / Enter press a focused control as a pointer does
    // (`tests/keyboard_press.rs`).
    mod keyboard_press;

    // A request's resume rebuilds the windows that show its answer
    // (`tests/request_resumes.rs`, HEADLESSRESUME15). Its presses are a
    // screen reader's.
    #[cfg(feature = "a11y")]
    mod request_resumes;

    // A `<webview>` on the headless backend: created and placed through the
    // web view pump, its sign-in redirect caught by the app's callback
    // (`tests/webview_sign_in.rs`, WEBVIEW17).
    mod webview_sign_in;

    // Stopping a background thread never waits for it on the UI thread, and its last
    // write-back still runs (`tests/thread_stop.rs`).
    mod thread_stop;

    // A node with `autofocus` takes the focus when it mounts, also when its window opens
    // (`tests/autofocus.rs`).
    mod autofocus;

    // A click that rebuilds the page lays it out once (`tests/click_cost.rs`).
    mod click_cost;

    // A file drop is one event, a drag move one hover, however many passes they cost
    // (`tests/file_drop.rs`).
    mod file_drop;

    // A layout pass rebuilds the CPU hit tester once (`tests/one_hit_tester.rs`).
    mod one_hit_tester;

    // A rebuild that only adds or changes a context menu opens the new menu
    // (`tests/context_menu_rebuild.rs`).
    mod context_menu_rebuild;

    // A context menu's item counts into the model a rebuild's merge kept
    // (`tests/dataset_merge_repoint.rs`).
    mod dataset_merge_repoint;

    // Every holder of a node's data is re-pointed by a rebuild's merge
    // (`tests/every_holder_of_a_nodes_data_is_repointed.rs`).
    mod every_holder_of_a_nodes_data_is_repointed;

    // --- Video tiles: a new frame is an image CONTENT update ---------------
    //
    // A camera / decoder frame reaches its tile as `ChangeNodeImage` (the
    // capture writeback, AzMeet's `show_picture`). The tile's box does not
    // move and its display-list item only swaps its `ImageRef`, patched in
    // place: nothing may re-run layout or rebuild the display list, and the
    // damage is the tile's rect. A tile nobody can see asks for no frame.

    /// A 120x80 frame of one colour, the tile's own size.
    fn tile_frame(rgba: [u8; 4]) -> azul_core::resources::ImageRef {
        use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};
        ImageRef::new_rawimage(RawImage {
            pixels: RawImageData::U8(rgba.repeat(120 * 80).into()),
            width: 120,
            height: 80,
            premultiplied_alpha: true,
            data_format: RawImageFormat::RGBA8,
            tag: Vec::new().into(),
        })
        .expect("a well-formed frame")
    }

    /// A 120x80 image tile with class `class`.
    fn video_tile(class: &str) -> Dom {
        Dom::create_image(tile_frame([10, 20, 30, 255]))
            .with_ids_and_classes(vec![azul_core::dom::IdOrClass::Class(class.into())].into())
            .with_css("display: block; width: 120px; height: 80px; margin-bottom: 40px;")
    }

    /// Two tiles, `tile-a` above `tile-b`, 40 px apart; `tile-far` 1000 px
    /// further down, below the bottom of any window the tests open.
    extern "C" fn video_tiles_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
            .with_css("margin: 0;")
            .with_child(video_tile("tile-a"))
            .with_child(video_tile("tile-b"))
            .with_child(Dom::create_div().with_css("height: 1000px;"))
            .with_child(video_tile("tile-far"))
    }

    /// A 200x100 scroll box holding a 150 px spacer and then `tile-scrolled`:
    /// the tile (y 150..230) is inside the 300 px window but below the fold
    /// of its box.
    extern "C" fn scrolled_tile_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 100px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 150px;"))
                .with_child(video_tile("tile-scrolled")),
        )
    }

    /// The DOM node carrying `class` in the root DOM.
    fn node_of_class(window: &HeadlessWindow, class: &str) -> azul_core::dom::NodeId {
        use azul_core::dom::{DomId, IdOrClass};
        let lw = window.common.layout_window.as_ref().expect("layout window");
        let dom = lw
            .layout_results
            .get(&DomId { inner: 0 })
            .expect("root dom");
        let index = dom
            .styled_dom
            .node_data
            .as_container()
            .internal
            .iter()
            .position(|d| {
                d.get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class))
            })
            .expect("a node with the class");
        azul_core::dom::NodeId::new(index)
    }

    /// What a capture writeback does with a new frame: `change_node_image`
    /// on the tile. Returns the tier the pass reports.
    fn push_tile_frame(
        window: &mut HeadlessWindow,
        class: &str,
        rgba: [u8; 4],
    ) -> azul_core::events::ProcessEventResult {
        use azul_layout::callbacks::CallbackChange;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let node_id = node_of_class(window, class);
        PlatformWindow::apply_user_change(
            window,
            &CallbackChange::ChangeNodeImage {
                dom_id: azul_core::dom::DomId { inner: 0 },
                node_id,
                image: tile_frame(rgba),
                update_type: azul_core::resources::UpdateImageType::Content,
                dirty_rect: None,
            },
        )
    }

    /// A settled window over `layout`, with its frame counters zeroed.
    fn settled_tile_window(
        layout: azul_core::callbacks::LayoutCallbackType,
    ) -> HeadlessWindow {
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        // A new window is born owing a rebuild. The frame path consumes that
        // request; the direct calls above do not, so the first frame under
        // test would rebuild the DOM for it.
        let _ = window.common.take_regeneration();
        window
            .common
            .layout_window
            .as_mut()
            .expect("layout window")
            .frame_report
            .reset_counters();
        window
    }

    fn frame_report(window: &HeadlessWindow) -> azul_layout::window::FrameReport {
        window
            .common
            .layout_window
            .as_ref()
            .expect("layout window")
            .frame_report
            .clone()
    }

    /// The bounding box of a frame's damage rects (`None` for no damage or a
    /// full repaint).
    fn damage_bounds(d: &FrameDamage) -> Option<azul_core::geom::LogicalRect> {
        let FrameDamage::Rects(rects) = d else {
            return None;
        };
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for r in rects {
            x0 = x0.min(r.origin.x);
            y0 = y0.min(r.origin.y);
            x1 = x1.max(r.origin.x + r.size.width);
            y1 = y1.max(r.origin.y + r.size.height);
        }
        (x1 > x0 && y1 > y0).then(|| {
            azul_core::geom::LogicalRect::new(
                azul_core::geom::LogicalPosition::new(x0, y0),
                azul_core::geom::LogicalSize::new(x1 - x0, y1 - y0),
            )
        })
    }

    fn assert_rect_close(
        got: azul_core::geom::LogicalRect,
        want: azul_core::geom::LogicalRect,
        what: &str,
    ) {
        let close = |a: f32, b: f32| (a - b).abs() <= 1.0;
        assert!(
            close(got.origin.x, want.origin.x)
                && close(got.origin.y, want.origin.y)
                && close(got.size.width, want.size.width)
                && close(got.size.height, want.size.height),
            "{what}: damage {got:?}, tile {want:?}"
        );
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_video_frame_on_a_tile_runs_no_layout_and_rebuilds_no_display_list() {
        use azul_core::events::ProcessEventResult;

        let mut window = settled_tile_window(video_tiles_layout);
        let tier = push_tile_frame(&mut window, "tile-a", [200, 40, 40, 255]);
        assert_eq!(
            tier,
            ProcessEventResult::ShouldReRenderCurrentWindow,
            "a same-size frame is a repaint, nothing more"
        );
        assert!(
            !window.common.display_list_dirty,
            "the display list was patched in place: a dirty flag makes the next frame rebuild \
             it from the layout tree (and send webrender a full transaction)"
        );
        window.service_frame(tier);
        let report = frame_report(&window);
        assert_eq!(report.layout_passes, 0, "a video frame re-ran layout");
        assert_eq!(report.dl_rebuilds, 0, "a video frame rebuilt the display list");
        assert_eq!(report.frames_since_reset, 1, "and one frame was painted");
    }

    /// A frame owed for nothing but a repaint - a pointer move that changed
    /// nothing, a GPU-value frame of a transform tween, an app's
    /// `request_repaint` - paints the layout as it stands, as the desktop
    /// frame paths do (macOS: a light transaction, scroll offsets and GPU
    /// values only). Laying the window out for it made every pointer event
    /// in a headless window a whole-window relayout: ~17 ms each on
    /// AzWidgets, two or three per click.
    #[cfg(feature = "cpurender")]
    #[test]
    fn a_repaint_with_nothing_changed_runs_no_layout() {
        use azul_core::events::ProcessEventResult;

        let mut window = settled_tile_window(video_tiles_layout);
        window.service_frame(ProcessEventResult::ShouldReRenderCurrentWindow);
        let report = frame_report(&window);
        assert_eq!(report.layout_passes, 0, "a repaint re-ran layout");
        assert_eq!(report.dl_rebuilds, 0, "a repaint rebuilt the display list");
        assert_eq!(report.frames_since_reset, 1, "and one frame was painted");
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_video_frame_damages_exactly_its_tile() {
        let mut window = settled_tile_window(video_tiles_layout);
        let tile = rects_by_class(&window, "tile-a")[0];
        let tier = push_tile_frame(&mut window, "tile-a", [200, 40, 40, 255]);
        window.service_frame(tier);
        let damage = window.cpu_backend.last_frame_damage.clone();
        let bounds = damage_bounds(&damage)
            .unwrap_or_else(|| panic!("a tile frame damages a rect, not {damage:?}"));
        assert_rect_close(bounds, tile, "one tile's frame");
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn two_tiles_updating_in_one_tick_damage_two_rects_not_the_window() {
        let mut window = settled_tile_window(video_tiles_layout);
        let a = rects_by_class(&window, "tile-a")[0];
        let b = rects_by_class(&window, "tile-b")[0];
        let tier = push_tile_frame(&mut window, "tile-a", [200, 40, 40, 255])
            .max(push_tile_frame(&mut window, "tile-b", [40, 200, 40, 255]));
        window.service_frame(tier);
        let FrameDamage::Rects(rects) = window.cpu_backend.last_frame_damage.clone() else {
            panic!("two tiles damage two rects, not the whole window");
        };
        assert_eq!(rects.len(), 2, "one rect per tile: {rects:?}");
        let mut got: Vec<azul_core::geom::LogicalRect> = rects.clone();
        got.sort_by(|p, q| p.origin.y.total_cmp(&q.origin.y));
        assert_rect_close(got[0], a, "the upper tile");
        assert_rect_close(got[1], b, "the lower tile");
        let report = frame_report(&window);
        assert_eq!(report.layout_passes, 0);
        assert_eq!(report.dl_rebuilds, 0);
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_video_frame_on_a_tile_below_the_window_requests_no_frame() {
        use azul_core::events::ProcessEventResult;

        let mut window = settled_tile_window(video_tiles_layout);
        let tier = push_tile_frame(&mut window, "tile-far", [200, 40, 40, 255]);
        assert_eq!(
            tier,
            ProcessEventResult::DoNothing,
            "a tile 1000 px below a 300 px window shows nothing: no frame"
        );
        // The frame is kept: scrolled into view, the tile shows it.
        let node = node_of_class(&window, "tile-far");
        let lw = window.common.layout_window.as_ref().expect("layout window");
        assert!(
            lw.content_overlay
                .image_for_node(azul_core::dom::DomId { inner: 0 }, node)
                .is_some(),
            "the hidden tile still holds its newest frame"
        );
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_video_frame_on_a_tile_scrolled_out_of_its_box_requests_no_frame() {
        use azul_core::events::ProcessEventResult;

        let mut window = settled_tile_window(scrolled_tile_layout);
        let tier = push_tile_frame(&mut window, "tile-scrolled", [200, 40, 40, 255]);
        assert_eq!(
            tier,
            ProcessEventResult::DoNothing,
            "a tile below the fold of its 100 px scroll box shows nothing: no frame"
        );
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn a_video_frame_on_a_minimized_window_requests_no_frame() {
        use azul_core::events::ProcessEventResult;

        let mut window = settled_tile_window(video_tiles_layout);
        window
            .common
            .layout_window
            .as_mut()
            .expect("layout window")
            .current_window_state
            .flags
            .frame = azul_core::window::WindowFrame::Minimized;
        let tier = push_tile_frame(&mut window, "tile-a", [200, 40, 40, 255]);
        assert_eq!(tier, ProcessEventResult::DoNothing);
    }

    // --- The worker of a node that unmounts is told to stop ----------------
    //
    // PR #476 ledger, engine backlog 4: video decode / camera / screen
    // capture / microphone workers kept running after their node left the
    // DOM - `run_all_threads` only ever reaped FINISHED threads, and a capture
    // or decode loop never finishes. The class: a worker one of a node's
    // lifecycle callbacks started belongs to that node; when the node
    // unmounts the worker is told to stop, and once it has, the next frame
    // removes it from the window (and the thread poll stops with it).

    #[derive(Debug, Default)]
    struct WorkerProbe {
        started: core::sync::atomic::AtomicBool,
        told_to_stop: core::sync::atomic::AtomicBool,
    }

    #[derive(Debug, Clone)]
    struct WorkerHost {
        show: bool,
        probe: Arc<WorkerProbe>,
    }

    #[derive(Debug, Clone)]
    struct WorkerNode {
        probe: Arc<WorkerProbe>,
    }

    /// A capture-style loop: runs until it is told to stop. Bounded (4 s),
    /// so an engine that never tells it fails the test instead of leaking it.
    extern "C" fn probe_worker(
        mut init: RefAny,
        _sender: azul_layout::thread::ThreadSender,
        recv: azul_core::task::ThreadReceiver,
    ) {
        use core::sync::atomic::Ordering;
        use std::time::{Duration, Instant};

        use azul_core::task::ThreadSendMsg;

        let Some(probe) = init.downcast_ref::<WorkerNode>().map(|n| n.probe.clone()) else {
            return;
        };
        probe.started.store(true, Ordering::SeqCst);
        // The window's messages - a `Tick` per frame, then `TerminateThread` -
        // come down the channel inside the receiver: wait on it up to the
        // deadline, instead of polling it every 2 ms.
        let Ok(inner) = recv.ptr.lock() else {
            return;
        };
        let deadline = Instant::now() + Duration::from_secs(4);
        loop {
            match inner.ptr.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(ThreadSendMsg::TerminateThread) => {
                    probe.told_to_stop.store(true, Ordering::SeqCst);
                    return;
                }
                // A frame's `Tick`: keep waiting.
                Ok(_) => {}
                // The deadline passed, or the window dropped the thread untold.
                Err(_) => return,
            }
        }
    }

    /// The node's `AfterMount`: start its worker, like every capture widget.
    extern "C" fn start_probe_worker(
        mut data: RefAny,
        mut info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use azul_layout::thread::{Thread, ThreadCallback};

        let Some(node) = data.downcast_ref::<WorkerNode>().map(|n| (*n).clone()) else {
            return azul_core::callbacks::Update::DoNothing;
        };
        let thread = Thread::create(
            RefAny::new(node),
            data.clone(),
            ThreadCallback::new(probe_worker),
        );
        info.add_thread(azul_core::task::ThreadId::unique(), thread);
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn worker_host_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{ComponentEventFilter, EventFilter},
        };
        let host = data
            .downcast_ref::<WorkerHost>()
            .map(|h| (*h).clone())
            .expect("worker host");
        let body = Dom::create_body().with_child(Dom::create_div().with_css("height: 10px;"));
        if !host.show {
            return body;
        }
        body.with_child(
            Dom::create_div().with_css("height: 20px;").with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Component(ComponentEventFilter::AfterMount),
                    callback: CoreCallback {
                        cb: start_probe_worker as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(WorkerNode {
                        probe: host.probe.clone(),
                    }),
                }]
                .into(),
            ),
        )
    }

    fn window_thread_ids(window: &HeadlessWindow) -> Vec<azul_core::task::ThreadId> {
        window
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.threads.keys().copied().collect())
            .unwrap_or_default()
    }

    #[test]
    fn the_worker_of_a_node_that_unmounts_is_told_to_stop_and_is_gone_after_a_frame() {
        use core::sync::atomic::Ordering;
        use std::time::{Duration, Instant};

        use crate::desktop::shell2::common::event::PlatformWindow;

        let probe = Arc::new(WorkerProbe::default());
        let state = Arc::new(RefCell::new(RefAny::new(WorkerHost {
            show: true,
            probe: probe.clone(),
        })));
        let mut window = make_window_sized(&state, worker_host_layout, 200.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        let started = window_thread_ids(&window);
        assert_eq!(started.len(), 1, "AfterMount starts the node's worker");
        let tid = started[0];
        assert!(
            window
                .common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.thread_owners.owner(&tid))
                .is_some(),
            "a worker the node's own AfterMount started belongs to that node"
        );
        assert!(window.thread_poll_timer_running);

        // The next DOM drops the node.
        {
            let mut g = state.borrow_mut();
            let r: &mut RefAny = &mut g;
            if let Some(mut host) = r.downcast_mut::<WorkerHost>() {
                host.show = false;
            };
        }
        window.regenerate_layout().expect("the node unmounts");

        let deadline = Instant::now() + Duration::from_secs(2);
        while !probe.told_to_stop.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(
            probe.told_to_stop.load(Ordering::SeqCst),
            "the unmount must tell the node's worker to stop (it ran on: started = {})",
            probe.started.load(Ordering::SeqCst)
        );

        // Once the worker has returned, ONE frame of thread polling removes it.
        let finished = |w: &HeadlessWindow| {
            w.common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.threads.get(&tid))
                .is_none_or(|t| t.ptr.lock().map_or(true, |inner| inner.is_finished()))
        };
        while !finished(&window) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(finished(&window), "the worker returns once it is told to stop");
        let _ = window.invoke_thread_callbacks();
        assert!(
            window_thread_ids(&window).is_empty(),
            "the stopped worker of the unmounted node is gone after a frame"
        );
        assert_eq!(
            window
                .common
                .layout_window
                .as_ref()
                .map(|lw| lw.thread_owners.orphan_count()),
            Some(0),
            "the retired orphan is forgotten"
        );
        assert!(
            !window.thread_poll_timer_running,
            "no thread left, so the thread poll stops (an idle window polls nothing)"
        );
    }

    // --- The timer a node started on mount stops when the node unmounts ----
    //
    // THREADS8: the map's tile sweep (`map_on_after_mount`) is a 250 ms timer
    // that never ends itself. A map that left the DOM kept waking the app four
    // times a second and kept its tile cache alive; a remount added a second
    // sweep. Same rule as the node's workers (`managers::thread_owner`): a
    // timer one of a node's lifecycle callbacks started belongs to the node.

    /// The id the node's `AfterMount` starts its timer under.
    const NODE_SWEEP_TIMER: usize = 0x7_3EE9;
    /// A timer of the app's, started outside any node's lifecycle.
    const APP_TIMER: usize = 0x7_A990;

    #[derive(Debug, Clone)]
    struct SweepHost {
        show: bool,
    }

    extern "C" fn sweep_tick(
        _data: RefAny,
        _info: azul_layout::timer::TimerCallbackInfo,
    ) -> azul_core::callbacks::TimerCallbackReturn {
        azul_core::callbacks::TimerCallbackReturn {
            should_update: azul_core::callbacks::Update::DoNothing,
            should_terminate: azul_core::task::TerminateTimer::Continue,
        }
    }

    fn sweep_timer(
        get_system_time_fn: azul_core::task::GetSystemTimeCallback,
    ) -> azul_layout::timer::Timer {
        use azul_core::task::{Duration, SystemTimeDiff};
        azul_layout::timer::Timer::create(
            RefAny::new(()),
            azul_layout::timer::TimerCallback::create(sweep_tick),
            get_system_time_fn,
        )
        .with_interval(Duration::System(SystemTimeDiff::from_millis(250)))
    }

    /// The node's `AfterMount`: start its periodic sweep, like the map does.
    extern "C" fn start_node_sweep(
        _data: RefAny,
        mut info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        let timer = sweep_timer(info.get_system_time_fn());
        info.add_timer(
            azul_core::task::TimerId {
                id: NODE_SWEEP_TIMER,
            },
            timer,
        );
        azul_core::callbacks::Update::DoNothing
    }

    extern "C" fn sweep_host_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{ComponentEventFilter, EventFilter},
        };
        let show = data.downcast_ref::<SweepHost>().is_some_and(|h| h.show);
        let body = Dom::create_body().with_child(Dom::create_div().with_css("height: 10px;"));
        if !show {
            return body;
        }
        body.with_child(
            Dom::create_div().with_css("height: 20px;").with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Component(ComponentEventFilter::AfterMount),
                    callback: CoreCallback {
                        cb: start_node_sweep as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                    refany: RefAny::new(()),
                }]
                .into(),
            ),
        )
    }

    fn has_timer(window: &HeadlessWindow, id: usize) -> bool {
        window
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.timers.contains_key(&azul_core::task::TimerId { id }))
    }

    #[test]
    fn the_timer_a_node_started_on_mount_stops_when_the_node_unmounts() {
        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(SweepHost { show: true })));
        let mut window = make_window_sized(&state, sweep_host_layout, 200.0, 100.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");

        assert!(
            has_timer(&window, NODE_SWEEP_TIMER),
            "AfterMount starts the node's timer"
        );
        let sweep_id = azul_core::task::TimerId {
            id: NODE_SWEEP_TIMER,
        };
        assert!(
            window
                .common
                .layout_window
                .as_ref()
                .and_then(|lw| lw.thread_owners.timer_owner(&sweep_id))
                .is_some(),
            "a timer the node's own AfterMount started belongs to that node"
        );
        // The app's own timer, started outside the node's lifecycle.
        let get_time =
            azul_layout::callbacks::ExternalSystemCallbacks::rust_internal().get_system_time_fn;
        window.start_timer(APP_TIMER, sweep_timer(get_time));

        // The next DOM drops the node.
        {
            let mut g = state.borrow_mut();
            let r: &mut RefAny = &mut g;
            if let Some(mut host) = r.downcast_mut::<SweepHost>() {
                host.show = false;
            };
        }
        window.regenerate_layout().expect("the node unmounts");

        assert!(
            !has_timer(&window, NODE_SWEEP_TIMER),
            "the timer of the unmounted node is stopped with it"
        );
        assert!(
            has_timer(&window, APP_TIMER),
            "the app's timer is not the node's to stop"
        );
        assert_eq!(
            window
                .common
                .layout_window
                .as_mut()
                .map(|lw| lw.thread_owners.take_timers_to_stop()),
            Some(Vec::new()),
            "the shell has stopped the unmounted node's timer (nothing left to stop)"
        );
    }

    /// `body(0) > div[contenteditable](1) > [p(2) > "Re"(3), p(4) > "Ben"(5)]`: a reply's
    /// editor, the same DOM on every build.
    extern "C" fn reply_editor_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body().with_child(
            Dom::create_div()
                .with_contenteditable(true)
                .with_child(Dom::create_p_with_text("Re"))
                .with_child(Dom::create_p_with_text("Ben")),
        )
    }

    /// An app that resets its editor's content (`CallbackInfo::reset_editor_content`, caret at
    /// the start) and rebuilds the SAME DOM gets its caret: the reset's caret is placed by the
    /// tail of a full layout, and an identical rebuild takes the shell's pre-cascade "layout
    /// unchanged" exit, which lays nothing out - the caret waited for some later full layout,
    /// and the focus the app gave the editor in the same callback seeded it at the END of the
    /// text meanwhile. AzMail's reply: the caret stood after the quote's last line, and the
    /// typed answer went under the quote (E2E-A, 2026-10-06).
    #[test]
    fn a_reset_editor_gets_its_caret_when_the_app_rebuilds_the_same_dom() {
        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_with(&state, reply_editor_layout);
        window
            .regenerate_layout()
            .expect("the editor's first layout");
        let _ = window.common.take_regeneration();
        let host = azul_core::dom::DomNodeId {
            dom: azul_core::dom::DomId::ROOT_ID,
            node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                azul_core::id::NodeId::new(1),
            )),
        };
        assert!(
            window
                .common
                .layout_window
                .as_mut()
                .expect("a layout window")
                .reset_editor_content(host, false),
            "premise: the host is a live editor"
        );
        window.regenerate_layout().expect("the identical rebuild");
        let _ = window.common.take_regeneration();

        let caret = window
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.document_caret())
            .expect("the reset placed the editor's caret");
        assert_eq!(
            caret.node.node.into_crate_internal(),
            Some(azul_core::id::NodeId::new(2)),
            "the caret is in the first block"
        );
        assert_eq!(caret.text_byte, 0, "at its start");
    }

    // --- A3-window-46: an identical rebuild under the pointer runs no cascade ----------------
    //
    // The pre-cascade skip keeps the styled DOM of the last pass, which was cascaded under the
    // states it holds. It wrote the same states again and ran the whole cascade whenever ANY
    // state was raised and a rule names a state left of its subject (`.btn:hover .ink`): every
    // `RefreshDom` with the pointer over a button, for the DOM it already had.

    extern "C" fn a3w_hover_noop(
        _data: RefAny,
        _info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        azul_core::callbacks::Update::DoNothing
    }

    /// A button at (0, 0), 120 x 60, whose hover shows in its child: the ink (20 x 20 at the
    /// button's top left) is green at rest and red while the BUTTON is hovered.
    extern "C" fn a3w_hover_left_of_subject_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::events::{EventFilter, HoverEventFilter};
        Dom::create_body()
            .with_css("margin: 0px;")
            .with_component_css(azul_css::css::Css::from_string(
                ".a3w-btn { width: 120px; height: 60px; } .a3w-ink { width: 20px; height: 20px; \
                 background: #00ff00; } .a3w-btn:hover .a3w-ink { background: #ff0000; }"
                    .into(),
            ))
            .with_child(
                Dom::create_div()
                    .with_class("a3w-btn".into())
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::MouseEnter),
                        RefAny::new(()),
                        a3w_hover_noop as usize,
                    )
                    .with_child(Dom::create_div().with_class("a3w-ink".into())),
            )
    }

    /// The solid background the ink of [`a3w_hover_left_of_subject_layout`] resolves to now.
    fn a3w_ink_colour(window: &HeadlessWindow) -> Option<azul_css::props::basic::ColorU> {
        let ink = node_with_class(window, "a3w-ink")?;
        let lw = window.common.layout_window.as_ref()?;
        let styled = &lw.layout_results.get(&azul_core::dom::DomId::ROOT_ID)?.styled_dom;
        let state = styled.styled_nodes.as_container()[ink].styled_node_state;
        azul_layout::solver3::getters::get_background_contents(styled, ink, &state)
            .into_iter()
            .find_map(|layer| match layer {
                azul_css::props::style::StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            })
    }

    const A3W_RED: azul_css::props::basic::ColorU = azul_css::props::basic::ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    };

    #[test]
    fn an_identical_rebuild_under_a_hovered_node_runs_no_cascade() {
        use crate::desktop::shell2::common::{
            event::PlatformWindow, layout::LayoutRegenerateResult,
        };

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, a3w_hover_left_of_subject_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        // Over the button, beside the ink.
        step(&mut window, HeadlessEvent::MouseMove { x: 80.0, y: 40.0 });
        assert_eq!(
            a3w_ink_colour(&window),
            Some(A3W_RED),
            "premise: the button is hovered and the rule left of the ink holds"
        );

        let cascades = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout().expect("the identical rebuild");
        assert!(
            matches!(result, LayoutRegenerateResult::LayoutUnchanged),
            "premise: an identical rebuild takes the unchanged exit"
        );
        assert_eq!(
            azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - cascades,
            0,
            "no state moved: the DOM that is kept is styled for the states it holds"
        );
        assert_eq!(a3w_ink_colour(&window), Some(A3W_RED), "and still shows the hover");
    }

    /// The other direction: a hover the managers hold and the kept DOM does not (the pointer
    /// came over the button and the rebuild runs before any restyle) is cascaded by the rebuild.
    #[test]
    fn a_hover_that_comes_with_an_identical_rebuild_is_cascaded() {
        use azul_core::window::CursorPosition;

        use crate::desktop::shell2::common::event::PlatformWindow;

        let state = Arc::new(RefCell::new(RefAny::new(())));
        let mut window = make_window_sized(&state, a3w_hover_left_of_subject_layout, 400.0, 200.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        assert_ne!(a3w_ink_colour(&window), Some(A3W_RED), "premise: at rest the ink is green");

        // The hit test alone: the hover manager knows the button, no event pass restyled it.
        let pos = LogicalPosition { x: 80.0, y: 40.0 };
        window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
        window.update_hit_test_at(pos);
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        window.regenerate_layout().expect("the identical rebuild");
        assert_eq!(
            a3w_ink_colour(&window),
            Some(A3W_RED),
            "the state came with the rebuild: the rule left of the ink is matched for it"
        );
    }

    // --- A3-window-47: diff first, step 1 - a rebuild that changed only texts -----------------
    //
    // The fingerprints of a rebuild name the nodes that differ before anything is styled. When
    // they are texts whose characters changed, the styled DOM is kept, the texts are written
    // onto it and the solver is told which nodes moved (`AZ_DIFF_FIRST`, on by default). The
    // full path - cascade, reconciliation, equivalence walk - is what it must agree with.

    /// The app state of [`a3w_texts_layout`]: one text per row, whether every third row is
    /// wide, whether row 1 declares a move, whether the text of row 2 has a handler.
    struct A3wTexts {
        texts: Vec<String>,
        wide: bool,
        mover: bool,
        handler: bool,
    }

    /// Rows of one cell each: `body > .a3w-row > span.a3w-cell > text`.
    extern "C" fn a3w_texts_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::events::{EventFilter, HoverEventFilter};
        let (texts, wide, mover, handler) = data
            .downcast_ref::<A3wTexts>()
            .map(|s| (s.texts.clone(), s.wide, s.mover, s.handler))
            .expect("app state");
        let mut body = Dom::create_body()
            .with_css("margin: 0px; font-size: 14px;")
            .with_component_css(azul_css::css::Css::from_string(
                ".a3w-row { padding: 2px; background: #eeeeee; } .a3w-wide { padding: 6px; \
                 background: #dddddd; } .a3w-cell { color: #222222; }"
                    .into(),
            ));
        for (i, text) in texts.iter().enumerate() {
            let mut row = Dom::create_div().with_class("a3w-row".into());
            if wide && i % 3 == 0 {
                row = row.with_class("a3w-wide".into());
            }
            if mover && i == 1 {
                row = row.with_css("animation: move 200ms ease-out;");
            }
            let mut text_node =
                Dom::create_text_do_not_use_without_block_level_wrapper(text.clone());
            if handler && i == 2 {
                text_node = text_node.with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseEnter),
                    RefAny::new(()),
                    a3w_hover_noop as usize,
                );
            }
            body = body.with_child(row.with_child(
                Dom::create_span().with_class("a3w-cell".into()).with_child(text_node),
            ));
        }
        body
    }

    fn a3w_texts_state(rows: usize) -> Arc<RefCell<RefAny>> {
        Arc::new(RefCell::new(RefAny::new(A3wTexts {
            texts: (0..rows).map(|i| format!("row {i}")).collect(),
            wide: false,
            mover: false,
            handler: false,
        })))
    }

    fn a3w_texts_window(state: &Arc<RefCell<RefAny>>) -> HeadlessWindow {
        let mut window = make_window_sized(state, a3w_texts_layout, 400.0, 600.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        window
    }

    fn a3w_change(state: &Arc<RefCell<RefAny>>, change: impl FnOnce(&mut A3wTexts)) {
        let mut guard = state.borrow_mut();
        let any: &mut RefAny = &mut guard;
        let mut app = any.downcast_mut::<A3wTexts>().expect("app state");
        change(&mut *app);
    }

    /// One `RefreshDom` of `window`, down the diff-first path or the full one.
    fn a3w_rebuild(window: &mut HeadlessWindow, diff_first: bool) {
        use crate::desktop::shell2::common::event::PlatformWindow;

        azul_layout::window::set_diff_first_on_this_thread(Some(diff_first));
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout();
        azul_layout::window::set_diff_first_on_this_thread(None);
        result.expect("the rebuild");
        let _ = window.common.take_regeneration();
    }

    /// What a rebuild leaves, line by line, for two windows that were handed the same builds:
    /// every node (what it is, its classes, its state, its box) and every item of the display
    /// list. A hit area is written by its bounds: a DOM that was styled again has new tag ids.
    /// A node's classes are read one by one (`IdOrClass`).
    fn a3w_dump(window: &HeadlessWindow) -> Vec<String> {
        use azul_core::{
            dom::{DomId, DomNodeId},
            styled_dom::NodeHierarchyItemId,
        };

        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let result = &lw.layout_results[&DomId::ROOT_ID];
        let styled = &result.styled_dom;
        let mut lines = Vec::new();
        for (index, node) in styled.node_data.as_ref().iter().enumerate() {
            let id = DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(azul_core::dom::NodeId::new(
                    index,
                ))),
            };
            lines.push(format!(
                "node {index}: {:?} {:?} state {:?} at {:?} size {:?} in {:?}",
                node.get_node_type(),
                node.get_ids_and_classes()
                    .iter()
                    .map(|class| format!("{class:?}"))
                    .collect::<Vec<_>>(),
                styled.styled_nodes.as_ref()[index],
                lw.get_node_position(id),
                lw.get_node_size(id),
                styled.node_hierarchy.as_ref()[index],
            ));
        }
        for (index, item) in result.display_list.items.iter().enumerate() {
            // A text by all but its font's hash: each window registers the harness font
            // under an id of its own.
            lines.push(match item {
                DisplayListItem::HitTestArea { bounds, .. } => {
                    format!("item {index}: hit area {bounds:?}")
                }
                DisplayListItem::Text {
                    glyphs,
                    font_size_px,
                    color,
                    clip_rect,
                    line_box,
                    source_node_index,
                    ..
                } => format!(
                    "item {index}: text {glyphs:?} {font_size_px} {color:?} {clip_rect:?} \
                     {line_box:?} {source_node_index:?}"
                ),
                DisplayListItem::TextLayout {
                    bounds,
                    font_size_px,
                    color,
                    ..
                } => format!("item {index}: text layout {bounds:?} {font_size_px} {color:?}"),
                other => format!("item {index}: {other:?}"),
            });
        }
        lines
    }

    /// Random rebuilds of one page in two windows, one down the diff-first path and one down
    /// the full path: after every rebuild both hold the same nodes, states, boxes and display
    /// list. Six of eight steps change one to three texts (to a word, to nothing, to what it
    /// was); the others flip a class or add or take a row, which both windows take in full.
    #[test]
    fn a_rebuild_down_the_diff_first_path_leaves_what_the_full_path_leaves() {
        const WORDS: [&str; 8] = ["", "a", "41", "42", "Tuebingen", "one two", "x y z", "row"];
        for seed in [1u64, 7, 20261121] {
            let state_first = a3w_texts_state(12);
            let state_full = a3w_texts_state(12);
            let mut first = a3w_texts_window(&state_first);
            let mut full = a3w_texts_window(&state_full);
            let mut random = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = |below: u64| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((random >> 33) % below) as usize
            };
            for step in 0..40 {
                let kind = next(8);
                let picks = [next(64), next(64), next(64), next(8), next(8), next(8)];
                let count = 1 + next(3);
                let change = |app: &mut A3wTexts| match kind {
                    6 => app.wide = !app.wide,
                    7 => {
                        if app.texts.len() > 8 && picks[0] % 2 == 0 {
                            let _ = app.texts.pop();
                        } else {
                            app.texts.push(format!("new {}", picks[0]));
                        }
                    }
                    _ => {
                        for i in 0..count {
                            let row = picks[i] % app.texts.len();
                            app.texts[row] = WORDS[picks[3 + i]].to_string();
                        }
                    }
                };
                a3w_change(&state_first, change);
                a3w_change(&state_full, change);
                a3w_rebuild(&mut first, true);
                a3w_rebuild(&mut full, false);
                let (left, right) = (a3w_dump(&first), a3w_dump(&full));
                let differs = left
                    .iter()
                    .zip(right.iter())
                    .position(|(a, b)| a != b)
                    .or((left.len() != right.len()).then_some(left.len().min(right.len())));
                assert!(
                    differs.is_none(),
                    "seed {seed}, step {step} (kind {kind}): line {differs:?} differs\n \
                     diff-first: {:?}\n full:       {:?}",
                    differs.and_then(|line| left.get(line)),
                    differs.and_then(|line| right.get(line)),
                );
            }
        }
    }

    /// What one changed text costs down the diff-first path: `(cascades, nodes the solver
    /// built fresh, nodes it sized again)` for a page of `rows` rows.
    fn a3w_cost_of_one_text(rows: usize) -> (usize, usize, usize) {
        let state = a3w_texts_state(rows);
        let mut window = a3w_texts_window(&state);
        a3w_change(&state, |app| app.texts[rows / 2] = "another text".to_string());
        let cascades = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
        a3w_rebuild(&mut window, true);
        let cascades = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - cascades;
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        (
            cascades,
            lw.layout_cache.last_reconcile_fresh,
            lw.layout_cache.last_intrinsic_dirty,
        )
    }

    /// One changed text styles nothing and lays out a number of boxes that does not grow with
    /// the page: at 60 rows and at 240.
    #[test]
    fn one_changed_text_styles_no_node_and_lays_out_its_own_boxes() {
        let (cascades_small, fresh_small, sized_small) = a3w_cost_of_one_text(60);
        let (cascades_large, fresh_large, sized_large) = a3w_cost_of_one_text(240);
        assert_eq!((cascades_small, cascades_large), (0, 0), "the cascade is kept");
        assert!(fresh_small >= 1, "premise: the solver saw the text ({fresh_small} fresh)");
        assert!(
            fresh_large <= fresh_small && sized_large <= sized_small,
            "four times the rows, the same boxes: fresh {fresh_small} -> {fresh_large}, sized \
             {sized_small} -> {sized_large}"
        );
        assert!(fresh_small <= 8, "the text and what holds its line: {fresh_small} fresh");
    }

    /// What the path does not do keeps it off: a node that declares a move is slid by the
    /// reconciliation, a text with a handler is told by it. Both rebuilds take the full path
    /// (they cascade), and so does every rebuild while the switch is off.
    #[test]
    fn a_changed_text_takes_the_full_path_where_the_reconciliation_has_work() {
        let cascades_of = |prepare: fn(&mut A3wTexts), diff_first: bool| {
            let state = a3w_texts_state(12);
            a3w_change(&state, prepare);
            let mut window = a3w_texts_window(&state);
            a3w_change(&state, |app| app.texts[2] = "another text".to_string());
            let before = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
            // The step `text` alone: with the later steps on, a text with a handler is a
            // node changed where it stands (`nodes`), and a page with a mover is reconciled
            // before it is styled or not at all (`tree`) - other exits, other tests.
            let steps: &[azul_layout::window::DiffFirstStep] = if diff_first {
                &[azul_layout::window::DiffFirstStep::Text]
            } else {
                &[]
            };
            df_rebuild(&mut window, steps);
            azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - before
        };
        assert_eq!(cascades_of(|_| {}, true), 0, "premise: a plain text change keeps the cascade");
        assert!(cascades_of(|app| app.mover = true, true) >= 1, "a declared move: the full path");
        assert!(cascades_of(|app| app.handler = true, true) >= 1, "a handler: the full path");
        assert!(cascades_of(|_| {}, false) >= 1, "the switch off: the full path");
    }

    // --- diff first, step `nodes`: a rebuild that changed nodes where they stand -------------
    //
    // The fingerprints say the tree and the sheets are what they were and name the nodes whose
    // own data moved (a class, an id, an inline style, a text). The styled DOM is kept, the
    // nodes are written onto it and it is styled again where it stands; the full path - a new
    // styled DOM, the reconciliation, the equivalence walk - is what it must agree with.

    /// One row of [`df_layout`]: its text, which `df-k<kind>` class it wears, which of
    /// [`DF_PLACES`] is its inline style, its id (0: none), and how many `div.df-extra`
    /// boxes follow its cell (a subtree that comes and goes inside the row).
    #[derive(Clone)]
    struct DfRow {
        text: String,
        kind: usize,
        place: usize,
        id: usize,
        extra: usize,
    }

    /// The app state of [`df_layout`]: the rows, whether the list is `df-dense`, and whether
    /// the sheet has its sibling rule (with it a changed row reaches the rows after it).
    struct DfPage {
        rows: Vec<DfRow>,
        dense: bool,
        siblings: bool,
    }

    /// The inline styles a row can have. The walks pick among the first four; the fifth (a
    /// declared `animation` over every property) and the sixth (a list item: the row gets
    /// a `::marker` box, so every box after it has another layout index under the same
    /// node id) are set by the tests that are about them.
    const DF_PLACES: [&str; 6] = [
        "",
        "position: relative; left: 4px;",
        "position: relative; left: 9px;",
        "width: 150px;",
        "animation: all 200ms ease-out;",
        "display: list-item;",
    ];

    /// `body > .df-list[.df-dense] > .df-row.df-k<n>[#df-id<n>] > span.df-cell > text`, under a
    /// sheet with a rule per class, a descendant rule (`.df-dense .df-row`), an inherited
    /// value (`.df-k2`), an id rule, a hover rule and - with `siblings` - a sibling rule
    /// (`.df-k1 + .df-row`). A row's id 2 styles nothing of the row itself: it colours the
    /// `div.df-extra` boxes inside the row (a descendant rule, paint only).
    extern "C" fn df_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let (rows, dense, siblings) = data
            .downcast_ref::<DfPage>()
            .map(|page| (page.rows.clone(), page.dense, page.siblings))
            .expect("app state");
        let mut list = Dom::create_div().with_class("df-list".into());
        if dense {
            list = list.with_class("df-dense".into());
        }
        for row in &rows {
            let mut node = Dom::create_div()
                .with_class("df-row".into())
                .with_class(format!("df-k{}", row.kind).into());
            if row.id != 0 {
                node = node.with_id(format!("df-id{}", row.id).into());
            }
            if !DF_PLACES[row.place].is_empty() {
                node = node.with_css(DF_PLACES[row.place]);
            }
            let mut node = node.with_child(
                Dom::create_span().with_class("df-cell".into()).with_child(
                    Dom::create_text_do_not_use_without_block_level_wrapper(row.text.clone()),
                ),
            );
            for n in 0..row.extra {
                node = node.with_child(
                    Dom::create_div().with_class("df-extra".into()).with_child(
                        Dom::create_text_do_not_use_without_block_level_wrapper(format!("x{n}")),
                    ),
                );
            }
            list = list.with_child(node);
        }
        let mut sheet = String::from(
            ".df-list { padding: 2px; } .df-row { padding: 4px; background: #eeeeee; } \
             .df-dense .df-row { padding: 1px; } .df-k1 { background: #ddddff; width: 200px; } \
             .df-k2 { color: #aa0000; font-size: 18px; } .df-k3 { margin-top: 3px; } \
             #df-id1 { background: #00ff00; } .df-row:hover { background: #ffff00; color: \
             #0000aa; } .df-extra { height: 6px; background: #cccccc; } .df-k2 .df-extra { \
             height: 9px; } #df-id2 .df-extra { background: #99cc99; }",
        );
        if siblings {
            sheet.push_str(" .df-k1 + .df-row { margin-left: 6px; }");
        }
        Dom::create_body()
            .with_css("margin: 0px; font-size: 14px;")
            .with_component_css(azul_css::css::Css::from_string(sheet.into()))
            .with_child(list)
    }

    fn df_state(rows: usize) -> Arc<RefCell<RefAny>> {
        df_state_with(rows, true)
    }

    fn df_state_with(rows: usize, siblings: bool) -> Arc<RefCell<RefAny>> {
        Arc::new(RefCell::new(RefAny::new(DfPage {
            rows: (0..rows)
                .map(|i| DfRow {
                    text: format!("row {i}"),
                    kind: i % 4,
                    place: 0,
                    id: 0,
                    extra: 0,
                })
                .collect(),
            dense: false,
            siblings,
        })))
    }

    fn df_window(state: &Arc<RefCell<RefAny>>) -> HeadlessWindow {
        let mut window = make_window_sized(state, df_layout, 400.0, 600.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        window
    }

    fn df_change(state: &Arc<RefCell<RefAny>>, change: impl FnOnce(&mut DfPage)) {
        let mut guard = state.borrow_mut();
        let any: &mut RefAny = &mut guard;
        let mut page = any.downcast_mut::<DfPage>().expect("app state");
        change(&mut *page);
    }

    /// One `RefreshDom` of `window` with exactly `steps` of the diff-first pipeline on.
    fn df_rebuild(window: &mut HeadlessWindow, steps: &[azul_layout::window::DiffFirstStep]) {
        use crate::desktop::shell2::common::event::PlatformWindow;

        azul_layout::window::set_diff_first_steps_on_this_thread(Some(steps));
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout();
        azul_layout::window::set_diff_first_steps_on_this_thread(None);
        result.expect("the rebuild");
        let _ = window.common.take_regeneration();
    }

    /// [`a3w_dump`], and of every node what its cascade resolves: each property that has a
    /// value in the state the node is in, its compact entries, and the hit-test tags.
    fn df_dump(window: &HeadlessWindow) -> Vec<String> {
        use azul_core::dom::{DomId, NodeId};

        let mut lines = a3w_dump(window);
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let styled = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
        let cache = styled.get_css_property_cache();
        for (index, node) in styled.node_data.as_ref().iter().enumerate() {
            let state = styled.styled_nodes.as_ref()[index].styled_node_state;
            let id = NodeId::new(index);
            let style: Vec<String> = azul_css::props::property::CssPropertyType::ALL
                .iter()
                .filter_map(|ty| cache.get_property(node, &id, &state, ty))
                .map(|property| format!("{property:?}"))
                .collect();
            lines.push(format!("style {index}: {style:?}"));
            if let Some(compact) = cache.compact_cache.as_ref() {
                lines.push(format!(
                    "compact {index}: {:?} {:?} {:?} {:?}",
                    compact.tier1_enums[index],
                    compact.tier2_dims[index],
                    compact.tier2_cold[index],
                    compact.tier2b_text[index],
                ));
            }
        }
        lines.push(format!("tags: {:?}", styled.tag_ids_to_node_ids));
        lines
    }

    /// The step `kind` of the random walk of [`df_walk`] applied to `page`. Nodes changed
    /// where they stand: texts (0, 1), the class of a row (2, 3), the list's class (5), a
    /// row's id (6), several rows at once (7), nothing (9). The TREE or a sheet changed
    /// (the step `tree`; without it both windows take the full path): a row's own sheet
    /// (4: `Dom::with_css`), a row added or taken (8), a row moved (10), a subtree inside a
    /// row that comes or goes (11), a row added, another taken and a third restyled at
    /// once (12), the first and the last row exchanged (13).
    fn df_mutate(page: &mut DfPage, kind: usize, picks: [usize; 6]) {
        const WORDS: [&str; 8] = ["", "a", "41", "42", "Tuebingen", "one two", "x y z", "row"];
        let rows = page.rows.len();
        match kind {
            0 | 1 => {
                for i in 0..=(picks[5] % 3) {
                    page.rows[picks[i] % rows].text = WORDS[picks[3 + i] % 8].to_string();
                }
            }
            2 | 3 => page.rows[picks[0] % rows].kind = picks[3] % 4,
            4 => page.rows[picks[0] % rows].place = picks[3] % 4,
            5 => page.dense = !page.dense,
            6 => page.rows[picks[0] % rows].id = picks[3] % 3,
            7 => {
                page.rows[picks[0] % rows].text = WORDS[picks[3] % 8].to_string();
                page.rows[picks[1] % rows].kind = picks[4] % 4;
                page.rows[picks[2] % rows].place = picks[5] % 4;
            }
            8 => {
                if rows > 8 && picks[0] % 2 == 0 {
                    let _ = page.rows.remove(picks[1] % rows);
                } else {
                    page.rows.insert(
                        picks[1] % (rows + 1),
                        DfRow {
                            text: format!("new {}", picks[0]),
                            kind: picks[3] % 4,
                            place: picks[4] % 4,
                            id: 0,
                            extra: 0,
                        },
                    );
                }
            }
            10 => {
                let row = page.rows.remove(picks[0] % rows);
                page.rows.insert(picks[1] % rows, row);
            }
            11 => {
                let at = picks[0] % rows;
                page.rows[at].extra = (page.rows[at].extra + 1 + picks[3] % 2) % 3;
            }
            12 => {
                page.rows[picks[2] % rows].kind = picks[5] % 4;
                page.rows.insert(
                    picks[1] % (rows + 1),
                    DfRow {
                        text: format!("came {}", picks[0]),
                        kind: picks[3] % 4,
                        place: 0,
                        id: 0,
                        extra: picks[4] % 2,
                    },
                );
                if page.rows.len() > 9 {
                    let _ = page.rows.remove(picks[0] % page.rows.len());
                }
            }
            13 => page.rows.swap(0, rows - 1),
            _ => {}
        }
    }

    /// Random rebuilds of one page in two windows, one with `steps` of the diff-first pipeline
    /// on and one down the full path: after every rebuild both hold the same nodes, states,
    /// resolved styles, compact entries, tags, boxes and display list. With `pointer`, the
    /// pointer is put over a row first and moved to another one every few steps, in both;
    /// `siblings` is whether the page's sheet has its sibling rule. Answers how many of the
    /// rebuilds were reconciled before they were styled and kept rows (the step `tree`).
    fn df_walk(
        steps: &[azul_layout::window::DiffFirstStep],
        pointer: bool,
        siblings: bool,
    ) -> usize {
        let kept_rows_before = azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread();
        for seed in [1u64, 7, 20261121] {
            let state_first = df_state_with(12, siblings);
            let state_full = df_state_with(12, siblings);
            let mut first = df_window(&state_first);
            let mut full = df_window(&state_full);
            let mut random = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = |below: u64| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((random >> 33) % below) as usize
            };
            for step_index in 0..48 {
                if pointer && step_index % 5 == 0 {
                    let y = 10.0 + 30.0 * next(8) as f32;
                    for window in [&mut first, &mut full] {
                        let _ = step(window, HeadlessEvent::MouseMove { x: 40.0, y });
                    }
                }
                let kind = next(14);
                let picks = [next(64), next(64), next(64), next(64), next(64), next(64)];
                df_change(&state_first, |page| df_mutate(page, kind, picks));
                df_change(&state_full, |page| df_mutate(page, kind, picks));
                df_rebuild(&mut first, steps);
                df_rebuild(&mut full, &[]);
                let (left, right) = (df_dump(&first), df_dump(&full));
                let differs = left
                    .iter()
                    .zip(right.iter())
                    .position(|(a, b)| a != b)
                    .or((left.len() != right.len()).then_some(left.len().min(right.len())));
                assert!(
                    differs.is_none(),
                    "seed {seed}, step {step_index} (kind {kind}, picks {picks:?}, steps \
                     {steps:?}, siblings {siblings}): line {differs:?} differs\n diff-first: \
                     {:?}\n full:       {:?}",
                    differs.and_then(|line| left.get(line)),
                    differs.and_then(|line| right.get(line)),
                );
            }
        }
        azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread() - kept_rows_before
    }

    /// How to read a failure: the message names the seed, the step and its kind
    /// ([`df_mutate`]) and the first differing line. A `node` / `item` line is the layout or
    /// the list, a `style` / `compact` line the cascade, `tags` the hit-test tags. A
    /// difference at a step whose kind changes the tree (4, 8, 10-13) is not this path:
    /// without the step `tree` both windows take the full path there.
    #[test]
    fn a_rebuild_that_changed_nodes_leaves_what_the_full_path_leaves() {
        use azul_layout::window::DiffFirstStep;
        let _ = df_walk(&[DiffFirstStep::Text, DiffFirstStep::Nodes], false, true);
    }

    /// The walks with the step `tree` (and the three before it): a rebuild that adds, takes,
    /// moves or exchanges rows, grows or shrinks a subtree inside a row or changes a row's
    /// own sheet is reconciled before it is styled, keeps the rows of the nodes that are
    /// what they were, and leaves what the full path leaves - nodes, states, resolved
    /// styles, compact entries, tags, boxes and display list. A difference at a step of
    /// kind 4, 8 or 10-13 is this path; a `style` / `compact` line is the cascade that kept
    /// rows (`StyledDom::cascade_reusing`), a `node` / `item` line the layout or the list.
    #[test]
    fn a_rebuild_that_changed_the_tree_leaves_what_the_full_path_leaves() {
        use azul_layout::window::DiffFirstStep::{Nodes, Style, Text, Tree};
        let kept = df_walk(&[Text, Nodes, Style, Tree], false, false);
        assert!(kept >= 20, "premise: rebuilds that change the tree keep rows ({kept} did)");
        let _ = df_walk(&[Text, Nodes, Style, Tree], false, true);
        // The step alone: every rebuild the fingerprints do not call identical takes it
        // (a changed text, a changed class too).
        let alone = df_walk(&[Tree], false, false);
        assert!(alone >= 20, "premise: the step alone keeps rows too ({alone} rebuilds did)");
    }

    /// The same with the pointer over a row, moved every few steps: the hover follows the
    /// row through the reconciliation BEFORE the cascade, which runs once under it. The
    /// hover rule hands the row's text a colour (`.df-row:hover { color }`).
    #[test]
    fn a_rebuild_that_changed_the_tree_under_the_pointer_leaves_what_the_full_path_leaves() {
        use azul_layout::window::DiffFirstStep::{Nodes, Style, Text, Tree};
        let _ = df_walk(&[Text, Nodes, Style, Tree], true, false);
        let _ = df_walk(&[Text, Nodes, Style, Tree], true, true);
    }

    /// `(whole cascades, nodes the cascade ran for, boxes the solver built fresh, rebuilds
    /// that kept rows)` when `added` rows come at the end of a page of `rows` rows (3 nodes
    /// a row, 2 above them; no sibling rule), with `steps` on.
    fn df_cost_of_rows_that_come(
        rows: usize,
        added: usize,
        steps: &[azul_layout::window::DiffFirstStep],
    ) -> (usize, usize, usize, usize) {
        let state = df_state_with(rows, false);
        let mut window = df_window(&state);
        df_change(&state, |page| {
            for i in 0..added {
                page.rows.push(DfRow {
                    text: format!("came {i}"),
                    kind: i % 4,
                    place: 0,
                    id: 0,
                    extra: 0,
                });
            }
        });
        let whole = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
        let nodes = azul_core::styled_dom::nodes_cascaded_on_this_thread();
        let kept = azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread();
        df_rebuild(&mut window, steps);
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        (
            azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - whole,
            azul_core::styled_dom::nodes_cascaded_on_this_thread() - nodes,
            lw.layout_cache.last_reconcile_fresh,
            azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread() - kept,
        )
    }

    /// Rows that come are styled and laid out, and no other node: 40 rows are 120 nodes, in
    /// a page of 300 rows and in one of 1,200. The cascade's count is exact (a row that
    /// comes at the end leaves every other node its data, its ancestors, its state and -
    /// the sheet has no sibling and no structural rule - all a rule can read of its place);
    /// the boxes built fresh do not grow with the page (the rows before keep their ids, so
    /// the solver keeps their boxes). Down the full path the same rebuild cascades every
    /// node, once.
    #[test]
    fn rows_that_come_are_styled_and_laid_out_and_no_other_node() {
        use azul_layout::window::DiffFirstStep;

        let (whole_small, nodes_small, fresh_small, kept_small) =
            df_cost_of_rows_that_come(300, 40, &DiffFirstStep::ALL);
        let (whole_large, nodes_large, fresh_large, kept_large) =
            df_cost_of_rows_that_come(1200, 40, &DiffFirstStep::ALL);
        assert_eq!((kept_small, kept_large), (1, 1), "reconciled first, rows kept");
        assert_eq!((whole_small, whole_large), (0, 0), "no cascade over the whole DOM");
        assert_eq!((nodes_small, nodes_large), (120, 120), "the rows that came: 3 nodes each");
        assert!(fresh_small >= 40, "premise: the solver built the new rows ({fresh_small})");
        assert!(
            fresh_large <= fresh_small,
            "four times the rows, the same boxes: fresh {fresh_small} -> {fresh_large}"
        );

        // The reference: the full path styles the whole page, in one cascade (the shell does
        // not build the compact cache a second time for a tree out of one cascade).
        let (whole_full, nodes_full, _, kept_full) = df_cost_of_rows_that_come(300, 40, &[]);
        assert_eq!(kept_full, 0, "every step off: the full path");
        assert_eq!(whole_full, 1, "one cascade, one compact cache");
        assert_eq!(nodes_full, 2 + 340 * 3, "over every node");
    }

    /// The case of the trace: about 500 nodes appear among about 3,500 (167 rows after
    /// 1,166). The cascade runs for the 501 nodes that came.
    #[test]
    fn five_hundred_nodes_among_3500_are_styled_alone() {
        use azul_layout::window::DiffFirstStep;

        let (whole, nodes, fresh, kept) = df_cost_of_rows_that_come(1166, 167, &DiffFirstStep::ALL);
        assert_eq!(kept, 1, "reconciled first, rows kept");
        assert_eq!(whole, 0, "no cascade over the whole DOM");
        assert_eq!(nodes, 501, "167 rows of 3 nodes");
        // The solver: the rows that came (a box per row at least, three nodes per row at
        // most, and what the reconcile builds around them), not the 3,500 nodes before.
        assert!(
            (167..=501 + 64).contains(&fresh),
            "the solver builds the boxes of the rows that came: {fresh} fresh"
        );
    }

    /// The same walks with the step `style`: the cascade runs for the nodes a change reaches
    /// (a row and what is below it; with the sibling rule, every row, which is the whole
    /// cascade again), every other node keeps its rows.
    #[test]
    fn a_rebuild_that_styled_its_changed_nodes_leaves_what_the_full_path_leaves() {
        use azul_layout::window::DiffFirstStep;
        let _ = df_walk(&DiffFirstStep::ALL, false, false);
        let _ = df_walk(&DiffFirstStep::ALL, false, true);
    }

    #[test]
    fn a_rebuild_that_styled_its_changed_nodes_under_the_pointer_leaves_what_the_full_path_leaves()
    {
        use azul_layout::window::DiffFirstStep;
        let _ = df_walk(&DiffFirstStep::ALL, true, false);
    }

    /// The same walk with the pointer over a row (a `:hover` rule holds for it), moved every
    /// few steps: the kept DOM holds the states the full path derives again.
    #[test]
    fn a_rebuild_that_changed_nodes_under_the_pointer_leaves_what_the_full_path_leaves() {
        use azul_layout::window::DiffFirstStep;
        let _ = df_walk(&[DiffFirstStep::Text, DiffFirstStep::Nodes], true, true);
    }

    /// What one changed class costs with every step on, in a page of `rows` rows without the
    /// sibling rule: `(whole cascades, nodes the cascade ran for, boxes built fresh)`.
    fn df_cost_of_a_styled_class(rows: usize) -> (usize, usize, usize) {
        let state = df_state_with(rows, false);
        let mut window = df_window(&state);
        df_change(&state, |page| {
            let middle = page.rows.len() / 2;
            page.rows[middle].kind = (page.rows[middle].kind + 2) % 4;
        });
        let whole = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
        let nodes = azul_core::styled_dom::nodes_cascaded_on_this_thread();
        df_rebuild(&mut window, &azul_layout::window::DiffFirstStep::ALL);
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        (
            azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - whole,
            azul_core::styled_dom::nodes_cascaded_on_this_thread() - nodes,
            lw.layout_cache.last_reconcile_fresh,
        )
    }

    /// One changed class styles the subtree of its node - the row, its cell and its text: 3
    /// nodes - in a page of 60 rows and in one of 244, with no pass over the whole DOM.
    #[test]
    fn one_changed_class_styles_its_row_and_no_other_node() {
        let (whole_small, nodes_small, fresh_small) = df_cost_of_a_styled_class(60);
        let (whole_large, nodes_large, fresh_large) = df_cost_of_a_styled_class(244);
        assert_eq!((whole_small, whole_large), (0, 0), "no cascade over the whole DOM");
        assert_eq!((nodes_small, nodes_large), (3, 3), "the row, its cell and its text");
        assert!(
            fresh_large <= fresh_small,
            "four times the rows, the same boxes: fresh {fresh_small} -> {fresh_large}"
        );
    }

    // --- diff first, step `wlist`: the window's display list after a rebuild that kept its DOM --
    //
    // Exit 1.46 styles the nodes a rebuild's changes reach and moves the cascade epoch, the
    // solver's only proof that no cascade ran: the window's whole list was emitted. Under
    // `wlist` the shell hands the pass the reached nodes (`solver3::cache::CascadedSince`,
    // as a view's re-render does under `list`) and the pass copies every other box's run.

    /// Every step but `wlist`, but `tlist`, but `list`: the state before each.
    const DF_BEFORE_WLIST: [azul_layout::window::DiffFirstStep; 7] = {
        use azul_layout::window::DiffFirstStep::{List, Nodes, Style, Text, Tree, TreeList, View};
        [Text, Nodes, Style, Tree, View, List, TreeList]
    };
    const DF_BEFORE_TLIST: [azul_layout::window::DiffFirstStep; 7] = {
        use azul_layout::window::DiffFirstStep::{List, Nodes, Style, Text, Tree, View, WindowList};
        [Text, Nodes, Style, Tree, View, List, WindowList]
    };
    const DF_BEFORE_LIST: [azul_layout::window::DiffFirstStep; 7] = {
        use azul_layout::window::DiffFirstStep::{Nodes, Style, Text, Tree, TreeList, View, WindowList};
        [Text, Nodes, Style, Tree, View, WindowList, TreeList]
    };

    /// What the display-list patching of `window`'s documents did so far
    /// (`LayoutWindow::display_list_patch_counters`: this window's own, no other window's).
    fn df_patch_counters(
        window: &HeadlessWindow,
    ) -> azul_layout::solver3::cache::DisplayListPatchCounters {
        window
            .common
            .layout_window
            .as_ref()
            .expect("a layout window")
            .display_list_patch_counters()
    }

    /// `(lists patched after a named cascade, lists patched across a changed tree, runs
    /// copied, paint calls of patched builds)` of `window` for one rebuild of it after
    /// `change`, with `steps` on.
    fn df_list_cost(
        window: &mut HeadlessWindow,
        state: &Arc<RefCell<RefAny>>,
        steps: &[azul_layout::window::DiffFirstStep],
        change: impl FnOnce(&mut DfPage),
    ) -> (usize, usize, usize, usize) {
        df_change(state, change);
        let before = df_patch_counters(window);
        df_rebuild(window, steps);
        let after = df_patch_counters(window);
        (
            after.lists_patched_after_a_named_cascade - before.lists_patched_after_a_named_cascade,
            after.lists_patched_across_trees - before.lists_patched_across_trees,
            after.cached_runs_copied - before.cached_runs_copied,
            after.runs_painted_by_patched_builds - before.runs_painted_by_patched_builds,
        )
    }

    /// The lines of a dump with the number of every `Key { id: N }` masked, and nothing else
    /// of the item (an item with a key that is missing or one too many still differs).
    /// A scrollbar's `opacity_key` and `thumb_transform_key` are ids of the process-wide
    /// `PROPERTY_KEY_COUNTER` (core `resources.rs`): stable for a window, different between
    /// two windows by construction. Measured (the verifier's run, 2026-11-21): the window
    /// rebuilt in full holds the same key before and after its rebuild (`OpacityKey { id:
    /// 184 }` twice) where the patched window holds 122; with the ids masked the two dumps
    /// have 0 differing lines of 852, 3,428, 972, 4,772 and 17,372.
    fn df_mask_key_ids(lines: Vec<String>) -> Vec<String> {
        const KEY: &str = "Key { id: ";
        lines
            .into_iter()
            .map(|line| {
                let mut masked = String::with_capacity(line.len());
                let mut rest = line.as_str();
                while let Some(at) = rest.find(KEY) {
                    let end = at + KEY.len();
                    masked.push_str(&rest[..end]);
                    masked.push('_');
                    rest = rest[end..].trim_start_matches(|c: char| c.is_ascii_digit());
                }
                masked.push_str(rest);
                masked
            })
            .collect()
    }

    /// What a display list holds beside its items, one line per item and one per list
    /// table: everything a copied run brings along that the item's own line in
    /// [`a3w_dump`] does not print. Per item: the DOM node it is attributed to
    /// (`node_mapping`), the layout box and phase that emitted it (`layout_node_mapping`: an
    /// index of the tree the list was built for), a hit area's tag (the DOM node's index +
    /// 1), a text's proven background, its ink and its selection colours, whether a rect is
    /// bound to a colour key. Per list: the carets, the ranges of fixed boxes, the forced
    /// breaks, the boxes a tween frame translated. (A text's `source_node_index`, the other
    /// index of the tree, is in the item's line.)
    fn df_list_tables(list: &azul_layout::solver3::display_list::DisplayList) -> Vec<String> {
        let mut lines: Vec<String> = list
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let tag = match item {
                    DisplayListItem::HitTestArea { tag, .. } => Some(*tag),
                    _ => None,
                };
                format!(
                    "tables {i}: node {:?} box {:?} tag {tag:?} background {:?} ink {:?} \
                     selection {:?} bound {}",
                    list.node_mapping.get(i).copied().flatten(),
                    list.layout_node_mapping.get(i).copied().flatten(),
                    list.uniform_text_bgs.get(i).copied().flatten(),
                    list.text_ink.get(i).copied().flatten(),
                    list.text_selection_colors.get(i).copied().flatten(),
                    list.color_binding(i).is_some(),
                )
            })
            .collect();
        lines.push(format!("carets: {:?}", list.carets));
        lines.push(format!("fixed ranges: {:?}", list.fixed_position_item_ranges));
        lines.push(format!("forced breaks: {:?}", list.forced_page_breaks));
        lines.push(format!("translated: {:?}", list.translated_since_painted));
        lines
    }

    /// Two windows of the page `page` makes, rebuilt after the same `change`: one with every
    /// step, one with the steps `reference` (every step but the one under test: the state
    /// before it, the list emitted in full). Answers the costs of the first
    /// ([`df_list_cost`]) after asserting that both windows hold the same nodes, styles,
    /// boxes and display list, line for line ([`df_dump`], the ids of the windows' own keys
    /// masked: [`df_mask_key_ids`]), and the same tables beside the list's items
    /// ([`df_list_tables`]).
    fn df_list_against_full(
        page: impl Fn() -> Arc<RefCell<RefAny>>,
        change: impl Fn(&mut DfPage),
        reference: &[azul_layout::window::DiffFirstStep],
        what: &str,
    ) -> (usize, usize, usize, usize) {
        let (state_list, state_full) = (page(), page());
        let mut with_list = df_window(&state_list);
        let mut in_full = df_window(&state_full);
        let cost = df_list_cost(
            &mut with_list,
            &state_list,
            &azul_layout::window::DiffFirstStep::ALL,
            &change,
        );
        let before = df_list_cost(&mut in_full, &state_full, reference, &change);
        assert_eq!(
            (before.0, before.1),
            (0, 0),
            "{what}: premise: without the step no list is patched for the cascade's names"
        );
        let dump = |window: &HeadlessWindow| {
            let mut lines = df_mask_key_ids(df_dump(window));
            let lw = window.common.layout_window.as_ref().expect("a layout window");
            lines.extend(df_list_tables(
                &lw.layout_results[&azul_core::dom::DomId::ROOT_ID].display_list,
            ));
            lines
        };
        let (left, right) = (dump(&with_list), dump(&in_full));
        let differs = left
            .iter()
            .zip(right.iter())
            .position(|(a, b)| a != b)
            .or((left.len() != right.len()).then_some(left.len().min(right.len())));
        assert!(
            differs.is_none(),
            "{what}: line {differs:?} of the window differs\n patched: {:?}\n in full: {:?}",
            differs.and_then(|line| left.get(line)),
            differs.and_then(|line| right.get(line)),
        );
        cost
    }

    /// One row of the window changes its class (a background and a width: no other row
    /// moves) and the others stay: the window's list is PATCHED - the runs of that row are
    /// painted, every other row's are copied - in a page of 60 rows and in one of 244, and
    /// the list is the one a full emission makes.
    ///
    /// Mutation: the shell's hand-over dropped (`cascaded_since_list` not set on exit 1.46,
    /// or its epochs not the list's and the DOM's): no list is patched, 0 runs are copied.
    #[test]
    fn one_changed_class_paints_its_row_and_copies_the_windows_list() {
        // Row 40 wears `df-k0` (40 % 4) at both sizes and takes `df-k1`.
        let restyle_one = |page: &mut DfPage| page.rows[40].kind = 1;
        let mut painted_at = Vec::new();
        for rows in [60_usize, 244] {
            let (patched, _, copied, painted) = df_list_against_full(
                || df_state_with(rows, false),
                restyle_one,
                &DF_BEFORE_WLIST,
                "one row restyled",
            );
            assert_eq!(patched, 1, "{rows} rows: the window's list is patched");
            // A row paints a background and a line of text.
            assert!(
                copied >= rows - 1,
                "{rows} rows: the runs of the rows that stayed are copied ({copied})"
            );
            assert!(
                (1..=12).contains(&painted),
                "{rows} rows: the runs of one row are painted, {painted} were"
            );
            painted_at.push(painted);
        }
        assert_eq!(
            painted_at[0], painted_at[1],
            "what is painted does not grow with the page"
        );
    }

    /// The mutant of the step: a node the cascade reached whose run is copied anyway. Every
    /// row holds a `div.df-extra`; one row takes the id 2, which styles nothing of the row
    /// and colours the box inside it through a descendant rule. The box's node data is what
    /// it was and it stands where it stood, so the reconcile keeps it clean, and nothing but
    /// the cascade - the CSS diff of the rebuild and the nodes it names - says it paints
    /// another colour. The list is a full emission's.
    ///
    /// Mutation: the re-emit set of the named cascade left empty (`css_dirty_reemit` in
    /// `layout_document`'s patch gate; in the early exit of a clean reconcile, `reemit`):
    /// the box is copied with its old colour and the two windows differ at its item.
    #[test]
    fn a_box_the_windows_cascade_reached_is_painted_though_it_is_clean() {
        let page = || {
            let state = df_state_with(40, false);
            df_change(&state, |page| {
                for row in &mut page.rows {
                    row.extra = 1;
                }
            });
            state
        };
        let (patched, across, copied, painted) = df_list_against_full(
            page,
            |page: &mut DfPage| page.rows[20].id = 2,
            &DF_BEFORE_WLIST,
            "a box recoloured by its row's id",
        );
        // Measured (the verifier's run, 2026-11-21): (patched, across) = (0, 1), 235 runs
        // copied, 10 painted; the patch gate (`AZ_PATCH_DEBUG`) reads `preserved=false`,
        // `fresh={122}` of 242 boxes. On this page every row holds an anonymous box (its
        // inline cell beside the block `div.df-extra`), and the layout reconcile carries an
        // anonymous box without counting it as reused or fresh: `reused + fresh == nodes`
        // does not hold, so no pass of this page is a same-shaped tree's and the pass is not
        // the named cascade's arm (the step `wlist`): the arm of a changed tree (`tlist`)
        // takes it, with the same names. (A row that takes the class of a descendant rule
        // instead of the id gives the same (0, 1): it is the page, not the id.) The mutant
        // of THAT arm is its `boxes_painted_for(&new_tree, named)` left out of the re-emit
        // set. Either arm patches the list; one of them must. The named arm's own re-emit
        // set has this test through a view (`a_node_the_cascade_reached_is_painted_though_its_box_is_clean`),
        // and the window's hand-over to it has `one_changed_class_paints_its_row_and_copies_the_windows_list`.
        assert_eq!(
            patched + across,
            1,
            "premise: the window's list is patched, by one of the two arms ({patched}, {across})"
        );
        assert!(copied >= 39, "premise: the other rows are copied ({copied} runs)");
        assert!(painted >= 1, "the box is painted ({painted} paint calls)");
    }

    // --- diff first, step `tlist`: the display list across a changed tree (diff-first-07) ------
    //
    // A pass whose layout tree changed its node count emitted its whole list: the patch
    // keys the runs of the list before by layout index. Under `tlist` the runs follow the
    // boxes the reconcile carried over (`PatchState::build_across_trees`), and exit 1.47
    // names the nodes it cascaded as exit 1.46 does.

    /// Rows that come at the end of the window's page: the rows before keep their node ids
    /// and their boxes, and their runs are COPIED from the list of the tree before; the
    /// rows that came are painted. 40 rows after 300 and after 1,200: what is painted does
    /// not grow with the page, and the list is a full emission's.
    ///
    /// Mutation: the arm of the changed tree off, or its map empty (no run of the list
    /// before follows its box): no list is patched across the trees and 0 runs are copied;
    /// a box that came left out of the re-emit set: the new rows are missing from the list
    /// and the two windows differ.
    #[test]
    fn rows_that_come_are_painted_and_the_rows_before_are_copied() {
        let come = |page: &mut DfPage| {
            for i in 0..40 {
                page.rows.push(DfRow {
                    text: format!("came {i}"),
                    kind: i % 4,
                    place: 0,
                    id: 0,
                    extra: 0,
                });
            }
        };
        let mut painted_at = Vec::new();
        for rows in [300_usize, 1200] {
            let (_, across, copied, painted) = df_list_against_full(
                || df_state_with(rows, false),
                come,
                &DF_BEFORE_TLIST,
                "40 rows came at the end",
            );
            assert_eq!(across, 1, "{rows} rows: the list is patched across the changed tree");
            assert!(
                copied >= rows,
                "{rows} rows: the runs of the rows before are copied ({copied})"
            );
            assert!(
                painted >= 40 && painted <= 40 * 8 + 16,
                "{rows} rows: the 40 rows that came are painted ({painted} paint calls)"
            );
            painted_at.push(painted);
        }
        assert_eq!(
            painted_at[0], painted_at[1],
            "what is painted does not grow with the page"
        );
    }

    /// The map itself: a box that keeps its node id and gets ANOTHER layout index. Row 10
    /// of 60 becomes a list item (its own sheet changes: exit 1.47): it gets a `::marker`
    /// box, so every box after it stands one index further on in the layout tree, under
    /// the node id it had. The rows after it stay where they are and are copied - each from
    /// the run of the index it HAD.
    ///
    /// Mutation: the list before read by the new index (`build_across_trees` without
    /// `new_of_old`): at each index after the marker stands another box than the one whose
    /// run is asked for, of another size, and the rows after row 10 are painted - fewer
    /// than 55 runs are copied.
    #[test]
    fn a_box_that_got_another_layout_index_is_copied_from_the_run_it_had() {
        use azul_core::dom::{DomId, DomNodeId, NodeId};
        use azul_core::styled_dom::NodeHierarchyItemId;

        // `body 0 > list 1 > row i at 2 + 3 i (its cell, its text)`: where row 30 stands.
        let row_30 = |window: &HeadlessWindow| {
            let lw = window.common.layout_window.as_ref().expect("a layout window");
            lw.get_node_position(DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2 + 3 * 30))),
            })
        };
        let state = df_state_with(60, false);
        let mut window = df_window(&state);
        let stood = row_30(&window);
        let (_, across, copied, _) = df_list_cost(
            &mut window,
            &state,
            &azul_layout::window::DiffFirstStep::ALL,
            |page: &mut DfPage| page.rows[10].place = 5,
        );
        assert_eq!(across, 1, "premise: the marker changes the tree, and the list is patched across it");
        assert!(stood.is_some(), "premise: row 30 has a place");
        assert_eq!(
            row_30(&window),
            stood,
            "premise: the rows after the list item did not move"
        );
        assert!(
            copied >= 55,
            "the rows before and after the list item are copied ({copied} runs)"
        );
        // And the list is the one a full emission makes.
        let (_, across, _, _) = df_list_against_full(
            || df_state_with(60, false),
            |page: &mut DfPage| page.rows[10].place = 5,
            &DF_BEFORE_TLIST,
            "a row became a list item",
        );
        assert_eq!(across, 1);

        // Rows that come in the MIDDLE: the rows before them keep their node ids and their
        // boxes' indices and are copied; every row after them is another DOM node id (and
        // another index) and is painted. The comparison holds every item and every table
        // beside the items ([`df_list_tables`]): nothing of the tree before is left in a
        // copied run.
        let (_, across, copied, _) = df_list_against_full(
            || df_state_with(60, false),
            |page: &mut DfPage| {
                for i in 0..5 {
                    page.rows.insert(
                        30,
                        DfRow {
                            text: format!("between {i}"),
                            kind: i % 4,
                            place: 0,
                            id: 0,
                            extra: 0,
                        },
                    );
                }
            },
            &DF_BEFORE_TLIST,
            "5 rows came in the middle",
        );
        assert_eq!(across, 1, "premise: the list is patched across the changed tree");
        assert!(copied >= 30, "the rows before the new ones are copied ({copied} runs)");

        // The same on a page whose rows each hold an anonymous box (the inline cell beside a
        // block `div.df-extra`): a box without a DOM node, whose run holds the cell's text
        // under the text's DOM node id. After the new rows every such id is another one.
        let with_boxes = || {
            let state = df_state_with(60, false);
            df_change(&state, |page| {
                for row in &mut page.rows {
                    row.extra = 1;
                }
            });
            state
        };
        let (_, across, copied, _) = df_list_against_full(
            with_boxes,
            |page: &mut DfPage| {
                for i in 0..5 {
                    page.rows.insert(
                        30,
                        DfRow {
                            text: format!("between {i}"),
                            kind: i % 4,
                            place: 0,
                            id: 0,
                            extra: 1,
                        },
                    );
                }
            },
            &DF_BEFORE_TLIST,
            "5 rows came in the middle of rows with anonymous boxes",
        );
        assert_eq!(across, 1, "premise: the list is patched across the changed tree");
        assert!(copied >= 30, "the rows before the new ones are copied ({copied} runs)");
    }

    /// The state of [`dfi_layout`]: how many boxes the inline-block of the line holds.
    struct DfiLine(usize);

    /// `body > div.dfi-line > text "one ", span.dfi-box > (span > "a") [, span.dfi-out ..],
    /// text " two"`, and a second line below. The box is an inline-block of a fixed size and
    /// a `span.dfi-out` is absolutely placed and takes no room: a box that comes inside the
    /// inline-block moves nothing of the line, and gives the text after it another DOM node
    /// id.
    extern "C" fn dfi_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let inside = data
            .downcast_ref::<DfiLine>()
            .map(|line| line.0)
            .expect("app state");
        let text = |s: &str| Dom::create_text_do_not_use_without_block_level_wrapper(s.to_string());
        let mut boxed = Dom::create_span()
            .with_class("dfi-box".into())
            .with_child(Dom::create_span().with_child(text("a")));
        for _ in 1..inside {
            boxed = boxed.with_child(Dom::create_span().with_class("dfi-out".into()));
        }
        Dom::create_body()
            .with_css("margin: 0px; font-size: 14px;")
            .with_component_css(azul_css::css::Css::from_string(
                ".dfi-line { padding: 4px; background: #eeeeee; } .dfi-box { display: \
                 inline-block; position: relative; width: 40px; height: 14px; background: \
                 #ddddff; } .dfi-out { position: absolute; left: 0px; top: 0px; width: 0px; \
                 height: 0px; }"
                    .to_string()
                    .into(),
            ))
            .with_child(
                Dom::create_div()
                    .with_class("dfi-line".into())
                    .with_child(text("one "))
                    .with_child(boxed)
                    .with_child(text(" two")),
            )
            .with_child(
                Dom::create_div()
                    .with_class("dfi-line".into())
                    .with_child(text("three")),
            )
    }

    /// An item of a run copied across a changed tree is attributed to the DOM node it has
    /// NOW (`node_mapping`). A line holds a text, an inline-block and a second text; a box
    /// comes inside the inline-block and takes no room: the line keeps its DOM node id, its
    /// size and its lines, and the second text is another DOM node id than it was. The run
    /// of the line holds the glyphs of both texts under their texts' ids, so it must be
    /// painted, not copied: every item and every table of the patched window is the
    /// window's that was rebuilt in full ([`df_list_tables`]: `node_mapping` among them).
    ///
    /// What paints the line here (the verifier's run, 2026-11-21; `AZ_PATCH_DEBUG`:
    /// `reflowed_ifcs={1, 8} fresh={6, 7, 8, 9}`): the line's inline context is laid out
    /// again when a box comes inside its inline-block, and a context that was laid out
    /// re-emits. So this test is green with and without the rule of `layout_document`'s arm
    /// of a changed tree that was written for the case ("a carried box whose DOM node id
    /// changed re-emits the root of its inline context"): it guards the property on this
    /// page, it is NOT that rule's mutant test. A page on which the root is carried without
    /// being laid out again was not found.
    #[test]
    fn a_text_whose_node_id_moved_is_not_copied_under_the_id_it_had() {
        let window_of = |inside: usize| {
            let state = Arc::new(RefCell::new(RefAny::new(DfiLine(inside))));
            let mut window = make_window_sized(&state, dfi_layout, 400.0, 600.0);
            window.regenerate_layout().expect("initial layout");
            window.regenerate_layout().expect("settle");
            let _ = window.common.take_regeneration();
            (state, window)
        };
        let grow = |state: &Arc<RefCell<RefAny>>| {
            let mut guard = state.borrow_mut();
            let any: &mut RefAny = &mut guard;
            let mut line = any.downcast_mut::<DfiLine>().expect("app state");
            line.0 = 2;
        };
        let dump = |window: &HeadlessWindow| {
            let mut lines = df_mask_key_ids(a3w_dump(window));
            let lw = window.common.layout_window.as_ref().expect("a layout window");
            lines.extend(df_list_tables(
                &lw.layout_results[&azul_core::dom::DomId::ROOT_ID].display_list,
            ));
            lines
        };
        let (state_list, mut with_list) = window_of(1);
        let (state_full, mut in_full) = window_of(1);
        let stood = dump(&with_list);
        grow(&state_list);
        grow(&state_full);
        let before = df_patch_counters(&with_list);
        df_rebuild(&mut with_list, &azul_layout::window::DiffFirstStep::ALL);
        let after = df_patch_counters(&with_list);
        let (across, copied) = (
            after.lists_patched_across_trees - before.lists_patched_across_trees,
            after.cached_runs_copied - before.cached_runs_copied,
        );
        df_rebuild(&mut in_full, &DF_BEFORE_TLIST);
        assert_eq!(across, 1, "premise: the list is patched across the changed tree");
        assert!(copied >= 1, "premise: the second line is copied ({copied} runs)");
        let (left, right) = (dump(&with_list), dump(&in_full));
        // What moved between the two trees, for the reader of a failure: the lines of the
        // dump that are not what they were.
        let moved = stood.iter().zip(right.iter()).filter(|(a, b)| a != b).count();
        assert!(moved >= 1, "premise: the box that came changed the window");
        let differs = left
            .iter()
            .zip(right.iter())
            .position(|(a, b)| a != b)
            .or((left.len() != right.len()).then_some(left.len().min(right.len())));
        assert!(
            differs.is_none(),
            "line {differs:?} of the window differs ({across} lists patched across, {copied} \
             runs copied)\n patched: {:?}\n in full: {:?}",
            differs.and_then(|line| left.get(line)),
            differs.and_then(|line| right.get(line)),
        );
    }

    /// The two cheapest rebuilds, counted: an identical one styles no node, runs no layout
    /// pass and keeps the display list where it lies (the renderers skip a list by its
    /// identity); one changed text styles no node either.
    #[test]
    fn an_identical_rebuild_and_a_changed_text_style_no_node() {
        use azul_core::dom::DomId;

        use crate::desktop::shell2::common::{
            event::PlatformWindow, layout::LayoutRegenerateResult,
        };

        let first_item = |window: &HeadlessWindow| {
            let lw = window.common.layout_window.as_ref().expect("a layout window");
            lw.layout_results[&DomId::ROOT_ID]
                .display_list
                .items
                .iter()
                .next()
                .map(|item| core::ptr::from_ref(item) as usize)
        };
        let state = df_state_with(60, false);
        let mut window = df_window(&state);

        let (nodes, list) = (
            azul_core::styled_dom::nodes_cascaded_on_this_thread(),
            first_item(&window),
        );
        assert!(list.is_some(), "premise: the page has a display list");
        let passes = window
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.frame_report.layout_passes);
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout().expect("the identical rebuild");
        let _ = window.common.take_regeneration();
        assert!(matches!(result, LayoutRegenerateResult::LayoutUnchanged));
        assert_eq!(
            azul_core::styled_dom::nodes_cascaded_on_this_thread() - nodes,
            0,
            "an identical rebuild styles no node"
        );
        assert_eq!(
            window.common.layout_window.as_ref().map(|lw| lw.frame_report.layout_passes),
            passes,
            "and lays nothing out"
        );
        assert_eq!(first_item(&window), list, "and its display list is the list it had");

        df_change(&state, |page| page.rows[30].text = "another text".to_string());
        let nodes = azul_core::styled_dom::nodes_cascaded_on_this_thread();
        df_rebuild(&mut window, &azul_layout::window::DiffFirstStep::ALL);
        assert_eq!(
            azul_core::styled_dom::nodes_cascaded_on_this_thread() - nodes,
            0,
            "a changed text styles no node"
        );
        // ... and the solver was handed the diff: nobody wrote this DOM since its last pass
        // (`dom_written_since_its_pass`), so it took over the fingerprints of the nodes the
        // rebuild did not change instead of hashing all 182 again.
        let skips = window
            .common
            .layout_window
            .as_ref()
            .map_or(0, |lw| lw.layout_cache.last_fingerprint_skips);
        assert!(skips >= 150, "the solver hashed the changed text, not the page: {skips} skips");
    }

    /// How often [`dfb_layout`] ran (one test builds with it).
    static DFB_LAYOUTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// [`df_layout`] through `azul_core::dom::DomBuilder`: the same page, node for node and
    /// sheet for sheet, handed over as a `Dom` that carries its tree flat.
    extern "C" fn dfb_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::dom::{DomBuilder, NodeType};

        DFB_LAYOUTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let (rows, dense, siblings) = data
            .downcast_ref::<DfPage>()
            .map(|page| (page.rows.clone(), page.dense, page.siblings))
            .expect("app state");
        let mut sheet = String::from(
            ".df-list { padding: 2px; } .df-row { padding: 4px; background: #eeeeee; } \
             .df-dense .df-row { padding: 1px; } .df-k1 { background: #ddddff; width: 200px; } \
             .df-k2 { color: #aa0000; font-size: 18px; } .df-k3 { margin-top: 3px; } \
             #df-id1 { background: #00ff00; } .df-row:hover { background: #ffff00; color: \
             #0000aa; } .df-extra { height: 6px; background: #cccccc; } .df-k2 .df-extra { \
             height: 9px; }",
        );
        if siblings {
            sheet.push_str(" .df-k1 + .df-row { margin-left: 6px; }");
        }
        let mut b = DomBuilder::new();
        b.open(NodeType::Body);
        b.set_css("margin: 0px; font-size: 14px;");
        b.add_component_css(azul_css::css::Css::from_string(sheet.into()));
        b.open(NodeType::Div);
        b.add_class_str("df-list");
        if dense {
            b.add_class_str("df-dense");
        }
        for row in &rows {
            b.open(NodeType::Div);
            b.add_class_str("df-row");
            b.add_class_str(&format!("df-k{}", row.kind));
            if row.id != 0 {
                b.add_id_str(&format!("df-id{}", row.id));
            }
            if !DF_PLACES[row.place].is_empty() {
                b.set_css(DF_PLACES[row.place]);
            }
            b.open(NodeType::Span);
            b.add_class_str("df-cell");
            b.add_text_str(&row.text);
            b.close();
            for n in 0..row.extra {
                b.open(NodeType::Div);
                b.add_class_str("df-extra");
                b.add_text_str(&format!("x{n}"));
                b.close();
            }
            b.close();
        }
        b.close();
        b.finish()
    }

    /// A window whose layout callback builds through `DomBuilder` takes the exits a window
    /// with the same page built as a tree takes, and leaves what it leaves; and a rebuild
    /// that built what the build before built is read as it was handed over: its
    /// fingerprints come from the builder's tiers and its flat tree never becomes a tree
    /// of `Dom`s (`azul_core::dom::flat_expansions_on_this_thread`: 0).
    ///
    /// Mutations this fails on: `regenerate_layout` calling `resolve_form_controls` (it
    /// expands) or `fingerprint_dom` (it reads a carrying `Dom` as one node: the identical
    /// rebuild is "changed"); `fingerprint_built_dom_against` expanding whatever the
    /// build before was; `fingerprint_built_dom_against` leaving a CHANGED flat DOM flat
    /// (the changed text is not adopted: the dumps differ); a tier of the builder that is
    /// not `fingerprint_dom`'s (the retained arena is not the shape of the fingerprints).
    #[test]
    fn a_builder_made_window_dom_rebuilt_unchanged_is_never_expanded() {
        use std::sync::atomic::Ordering::Relaxed;

        use azul_core::{
            callbacks::RelayoutReason, dom::flat_expansions_on_this_thread as expansions,
            styled_dom::nodes_cascaded_on_this_thread as cascaded,
        };
        use azul_layout::window::{kept_dom_rebuilds_on_this_thread as kept, DiffFirstStep};

        use crate::desktop::shell2::common::{
            event::PlatformWindow, layout::LayoutRegenerateResult,
        };

        let flat_state = df_state_with(60, false);
        let tree_state = df_state_with(60, false);
        // Two windows: each has its own scrollbar keys (ids of a process-wide counter), so
        // the dumps are compared with those ids masked ([`df_mask_key_ids`]; the page
        // overflows: 60 rows are 1,429 px in a window of 600).
        let dump = |window: &HeadlessWindow| df_mask_key_ids(df_dump(window));

        let before = expansions();
        let mut flat = make_window_sized(&flat_state, dfb_layout, 400.0, 600.0);
        flat.regenerate_layout().expect("initial layout");
        assert!(
            expansions() > before,
            "premise: the first build arrives flat (there is no build before it to equal) \
             and is styled as the tree it stands for"
        );
        flat.regenerate_layout().expect("settle");
        let _ = flat.common.take_regeneration();
        let mut tree = df_window(&tree_state);
        assert_eq!(dump(&flat), dump(&tree), "the first build, built either way");

        // The same page again: read flat, kept, never a tree.
        let unchanged = |flat: &mut HeadlessWindow, what: &str| {
            let (calls, trees, nodes) = (DFB_LAYOUTS.load(Relaxed), expansions(), cascaded());
            flat.common.request_regeneration(RelayoutReason::RefreshDom);
            let result = flat.regenerate_layout().expect("the identical rebuild");
            let _ = flat.common.take_regeneration();
            assert_eq!(DFB_LAYOUTS.load(Relaxed) - calls, 1, "{what}: the callback built it");
            assert!(
                matches!(result, LayoutRegenerateResult::LayoutUnchanged),
                "{what}: the identical-rebuild exit"
            );
            assert_eq!(expansions() - trees, 0, "{what}: the flat DOM stayed flat");
            assert_eq!(cascaded() - nodes, 0, "{what}: no node styled");
        };
        unchanged(&mut flat, "the first identical rebuild");
        unchanged(&mut flat, "the second");
        assert_eq!(dump(&flat), dump(&tree), "after two identical rebuilds");

        // One changed text: the texts exit for both; the flat DOM becomes its tree once.
        for state in [&flat_state, &tree_state] {
            df_change(state, |page| page.rows[30].text = "another text".to_string());
        }
        let (trees, nodes) = (expansions(), cascaded());
        df_rebuild(&mut flat, &DiffFirstStep::ALL);
        assert_eq!(cascaded() - nodes, 0, "a changed text styles no node, built either way");
        assert_eq!(expansions() - trees, 1, "a rebuild that changed something reads the tree");
        df_rebuild(&mut tree, &DiffFirstStep::ALL);
        assert_eq!(dump(&flat), dump(&tree), "after a changed text");

        // One changed class: the same exit for both (the styled DOM kept, or not).
        for state in [&flat_state, &tree_state] {
            df_change(state, |page| page.rows[7].kind = (page.rows[7].kind + 1) % 4);
        }
        let before = kept();
        df_rebuild(&mut flat, &DiffFirstStep::ALL);
        let flat_kept = kept() - before;
        let before = kept();
        df_rebuild(&mut tree, &DiffFirstStep::ALL);
        assert_eq!(flat_kept, kept() - before, "a changed class takes the same exit");
        assert_eq!(dump(&flat), dump(&tree), "after a changed class");

        // A row added (the tree changed), then the same page again.
        for state in [&flat_state, &tree_state] {
            df_change(state, |page| {
                let mut row = page.rows[3].clone();
                row.text = "a new row".to_string();
                page.rows.insert(10, row);
            });
        }
        df_rebuild(&mut flat, &DiffFirstStep::ALL);
        df_rebuild(&mut tree, &DiffFirstStep::ALL);
        assert_eq!(dump(&flat), dump(&tree), "after a row was added");
        unchanged(&mut flat, "identical again, after the tree changed");
        assert_eq!(dump(&flat), dump(&tree), "at the end");
    }

    /// `(rebuilds that kept the styled DOM, cascades, boxes the solver built fresh)` for one
    /// rebuild of a page of `rows` rows after `change`, with `steps` on.
    fn df_cost(
        rows: usize,
        steps: &[azul_layout::window::DiffFirstStep],
        change: impl FnOnce(&mut DfPage),
    ) -> (usize, usize, usize) {
        let state = df_state(rows);
        let mut window = df_window(&state);
        df_change(&state, change);
        let kept = azul_layout::window::kept_dom_rebuilds_on_this_thread();
        let cascades = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
        df_rebuild(&mut window, steps);
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        (
            azul_layout::window::kept_dom_rebuilds_on_this_thread() - kept,
            azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - cascades,
            lw.layout_cache.last_reconcile_fresh,
        )
    }

    /// One changed class keeps the styled DOM (one restyle, no second cascade) and the boxes
    /// the solver builds fresh do not grow with the page; with the step off, and for a row
    /// that came, the rebuild takes the full path.
    #[test]
    fn one_changed_class_keeps_the_styled_dom_and_lays_out_its_own_boxes() {
        use azul_layout::window::DiffFirstStep::{Nodes, Text};

        let class = |page: &mut DfPage| {
            let middle = page.rows.len() / 2;
            page.rows[middle].kind = (page.rows[middle].kind + 2) % 4;
        };
        let (kept_small, cascades_small, fresh_small) = df_cost(60, &[Text, Nodes], class);
        // 244 rows: its middle row wears the class the middle row of 60 wears.
        let (kept_large, cascades_large, fresh_large) = df_cost(244, &[Text, Nodes], class);
        assert_eq!((kept_small, kept_large), (1, 1), "the styled DOM is kept");
        assert_eq!((cascades_small, cascades_large), (1, 1), "one restyle, nothing twice");
        assert!(fresh_small >= 1, "premise: the solver saw the row ({fresh_small} fresh)");
        assert!(
            fresh_large <= fresh_small,
            "four times the rows, the same boxes: fresh {fresh_small} -> {fresh_large}"
        );

        assert_eq!(df_cost(60, &[Text], class).0, 0, "the step off: the full path");
        assert_eq!(df_cost(60, &[], class).0, 0, "every step off: the full path");
        let came = |page: &mut DfPage| {
            let row = page.rows[0].clone();
            page.rows.push(row);
        };
        assert_eq!(df_cost(60, &[Text, Nodes], came).0, 0, "a row came: another tree");
    }

    /// The names of `AZ_DIFF_FIRST`: `0` is no step, a list exactly its steps, anything else
    /// every step.
    #[test]
    fn the_steps_of_the_switch_are_named_one_by_one() {
        use azul_layout::window::{diff_first_step, diff_first_steps_of, DiffFirstStep};

        let on = |value: &str| -> Vec<&'static str> {
            let bits = diff_first_steps_of(value);
            DiffFirstStep::ALL
                .into_iter()
                .enumerate()
                .filter(|(index, _)| bits & (1 << index) != 0)
                .map(|(_, step)| step.name())
                .collect()
        };
        assert_eq!(on("0"), Vec::<&str>::new());
        let every = vec!["text", "nodes", "style", "tree", "view", "list", "wlist", "tlist"];
        assert_eq!(on("1"), every);
        assert_eq!(on(""), every);
        assert_eq!(on("wlist,list"), vec!["list", "wlist"]);
        assert_eq!(on("tlist"), vec!["tlist"]);
        assert_eq!(
            on("text,nodes,style,tree,view"),
            vec!["text", "nodes", "style", "tree", "view"],
            "the state before `list`"
        );
        assert_eq!(on("list"), vec!["list"]);
        assert_eq!(on("tree"), vec!["tree"]);
        assert_eq!(on("text,nodes,style"), vec!["text", "nodes", "style"], "the state before `tree`");
        assert_eq!(on("text"), vec!["text"]);
        assert_eq!(on("nodes, style"), vec!["nodes", "style"]);
        assert_eq!(on("text,bogus"), vec!["text"], "an unknown name beside a known one is skipped");

        azul_layout::window::set_diff_first_steps_on_this_thread(Some(&[DiffFirstStep::Nodes]));
        let chosen = (
            diff_first_step(DiffFirstStep::Text),
            diff_first_step(DiffFirstStep::Nodes),
            azul_layout::window::diff_first_enabled(),
        );
        azul_layout::window::set_diff_first_steps_on_this_thread(None);
        assert_eq!(chosen, (false, true, true));
    }

    // --- diff-first-10: a full rebuild under a hovered node with an inherited `:hover` value ---
    //
    // The full path cascades the new DOM at rest and raises the states afterwards. A node in
    // a state hands its children the inheritable values the state declares; the children of a
    // DOM cascaded at rest held the resting ones until the pointer left and came back.

    /// The app state of [`df10_layout`]: how many rows the page has.
    struct Df10Rows(usize);

    /// `body > .df10-row > span.df10-cell > text`, each row 40 px high; a hovered row's
    /// `color` (inherited by the cell and the text) is red, a resting one's black.
    extern "C" fn df10_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::events::{EventFilter, HoverEventFilter};
        let rows = data
            .downcast_ref::<Df10Rows>()
            .map(|rows| rows.0)
            .expect("app state");
        let mut body = Dom::create_body()
            .with_css("margin: 0px; font-size: 14px;")
            .with_component_css(azul_css::css::Css::from_string(
                ".df10-row { height: 40px; color: #000000; } .df10-row:hover { color: #ff0000; }"
                    .into(),
            ));
        for i in 0..rows {
            body = body.with_child(
                Dom::create_div()
                    .with_class("df10-row".into())
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::MouseEnter),
                        RefAny::new(()),
                        a3w_hover_noop as usize,
                    )
                    .with_child(Dom::create_span().with_class("df10-cell".into()).with_child(
                        Dom::create_text_do_not_use_without_block_level_wrapper(format!("row {i}")),
                    )),
            );
        }
        body
    }

    /// The colour the first text of the page is painted in (the text of the first row).
    fn df10_first_text_colour(window: &HeadlessWindow) -> Option<String> {
        let lw = window.common.layout_window.as_ref()?;
        let result = lw.layout_results.get(&azul_core::dom::DomId::ROOT_ID)?;
        result.display_list.items.iter().find_map(|item| match item {
            DisplayListItem::Text { color, .. } | DisplayListItem::TextLayout { color, .. } => {
                Some(format!("{color:?}"))
            }
            _ => None,
        })
    }

    /// A rebuild that changes the tree (a row comes at the end) while the pointer rests on
    /// the first row: the text in that row keeps the row's hover colour. Down the full path
    /// (`AZ_DIFF_FIRST=0`), which is the reference of every equality walk.
    #[test]
    fn a_full_rebuild_keeps_the_hover_colour_the_hovered_row_hands_its_text() {
        let state = Arc::new(RefCell::new(RefAny::new(Df10Rows(3))));
        let mut window = make_window_sized(&state, df10_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        let _ = window.common.take_regeneration();
        let resting = df10_first_text_colour(&window);
        assert!(resting.is_some(), "premise: the first row paints a text");

        // Over the first row.
        step(&mut window, HeadlessEvent::MouseMove { x: 200.0, y: 20.0 });
        let hovered = df10_first_text_colour(&window);
        assert_ne!(hovered, resting, "premise: the hover reaches the text in the row");

        {
            let mut guard = state.borrow_mut();
            let any: &mut RefAny = &mut guard;
            let mut rows = any.downcast_mut::<Df10Rows>().expect("app state");
            rows.0 = 4;
        }
        df_rebuild(&mut window, &[]);
        assert_eq!(
            df10_first_text_colour(&window),
            hovered,
            "the row is still hovered after the rebuild: its text shows the colour it hands down"
        );
    }

    // --- The gate `animation`, narrowed to the nodes a rebuild can restyle ----------------------

    /// What the window's transitions are, as text: node, property, from, to.
    fn df_transitions(window: &HeadlessWindow) -> Vec<String> {
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let mut out: Vec<String> = lw
            .css_transitions
            .iter()
            .map(|t| format!("{:?} {:?}: {:?} -> {:?}", t.node, t.prop_type, t.from, t.to))
            .collect();
        out.sort();
        out
    }

    /// A page of 12 rows whose row 2 declares `animation: all 200ms`, after `change`, rebuilt
    /// with `steps`: `(rebuilds that kept the styled DOM, rebuilds reconciled first that kept
    /// rows, the transitions that run afterwards)`.
    fn df_cost_beside_an_animated_row(
        steps: &[azul_layout::window::DiffFirstStep],
        change: impl FnOnce(&mut DfPage),
    ) -> (usize, usize, Vec<String>) {
        let state = df_state_with(12, false);
        df_change(&state, |page| page.rows[2].place = 4);
        let mut window = df_window(&state);
        df_change(&state, change);
        let kept = azul_layout::window::kept_dom_rebuilds_on_this_thread();
        let tree = azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread();
        df_rebuild(&mut window, steps);
        (
            azul_layout::window::kept_dom_rebuilds_on_this_thread() - kept,
            azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread() - tree,
            df_transitions(&window),
        )
    }

    /// A node that declares an `animation` somewhere in the window does not send every
    /// rebuild down the full path: a class that changes on ANOTHER row keeps the styled DOM
    /// (exit 1.46), a row that comes is reconciled first and keeps rows (exit 1.47), and no
    /// transition starts, as down the full path. When the ANIMATED row's own class changes,
    /// exit 1.46 leaves it alone (its property changes become transitions), exit 1.47 takes
    /// it and starts the transitions the full path starts.
    #[test]
    fn an_animated_node_elsewhere_does_not_send_a_rebuild_down_the_full_path() {
        use azul_layout::window::DiffFirstStep::{Nodes, Style, Text, Tree};
        let all = [Text, Nodes, Style, Tree];

        let other_class = |page: &mut DfPage| page.rows[7].kind = (page.rows[7].kind + 1) % 4;
        let (kept, tree, transitions) = df_cost_beside_an_animated_row(&all, other_class);
        assert_eq!((kept, tree), (1, 0), "another row's class: the styled DOM is kept");
        assert!(transitions.is_empty(), "and nothing animates: {transitions:?}");

        let row_comes = |page: &mut DfPage| {
            let row = page.rows[0].clone();
            page.rows.push(row);
        };
        let (kept, tree, transitions) = df_cost_beside_an_animated_row(&all, row_comes);
        assert_eq!((kept, tree), (0, 1), "a row comes: reconciled first, rows kept");
        assert!(transitions.is_empty(), "and nothing animates: {transitions:?}");

        // Row 2 is `df-k2` (a colour, a font size); `df-k1` gives it a background and a width.
        let its_own_class = |page: &mut DfPage| page.rows[2].kind = 1;
        let (kept, tree, transitions) = df_cost_beside_an_animated_row(&all, its_own_class);
        assert_eq!((kept, tree), (0, 1), "the animated row's class: not exit 1.46, exit 1.47");
        let (_, _, reference) = df_cost_beside_an_animated_row(&[], its_own_class);
        assert!(!reference.is_empty(), "premise: the full path starts transitions for it");
        assert_eq!(transitions, reference, "the same transitions as down the full path");
    }

    // --- A style a callback wrote, and the rebuild it asked for (a slider's drag) -------------
    //
    // `set_css_property` from a callback writes the node's own style on the KEPT arena, and
    // the shell lays out once, in the rebuild the callback asked for. The exits that keep the
    // styled DOM compare the app's two BUILDS: for them the written node is "what it was",
    // and telling the solver so (`dom_diff_clean`) made it take over the fingerprint from
    // before the write - the written style was never laid out, from the first rebuild on
    // that came without a state change (`dragging_the_slider_leaves_no_thumb_behind`).

    /// The app state of [`dfw_layout`]: how often the pad's handler ran.
    struct DfwMoves(usize);

    /// The pad's handler: slides the pad's first child with `set_css_property(margin-left)`,
    /// 10 px per call, counts the call and asks for a rebuild - what a slider does on every
    /// move of a drag in an app that refreshes on change. The app does not store the margin:
    /// the DOM it builds next differs from the one before in the caption's text alone.
    extern "C" fn dfw_on_move(
        mut data: RefAny,
        mut info: azul_layout::callbacks::CallbackInfo,
    ) -> azul_core::callbacks::Update {
        use azul_css::props::{layout::LayoutMarginLeft, property::CssProperty};
        let moves = match data.downcast_mut::<DfwMoves>() {
            Some(mut state) => {
                state.0 += 1;
                state.0
            }
            None => return azul_core::callbacks::Update::DoNothing,
        };
        let pad = info.get_hit_node();
        if let Some(knob) = info.get_first_child(pad) {
            info.set_css_property(
                knob,
                CssProperty::const_margin_left(LayoutMarginLeft::const_px(10 * moves as isize)),
            );
        }
        azul_core::callbacks::Update::RefreshDom
    }

    /// `body > (div > "moves: N"), (div.dfw-pad[MouseMove] > div.dfw-knob)`: the caption is
    /// 20 px high whatever it says, the pad 300 x 100 below it, the knob 10 x 10 at the pad's
    /// top left.
    extern "C" fn dfw_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        use azul_core::events::{EventFilter, HoverEventFilter};
        let moves = data
            .downcast_ref::<DfwMoves>()
            .map(|state| state.0)
            .expect("app state");
        Dom::create_body()
            .with_css("margin: 0px; font-size: 14px;")
            .with_child(Dom::create_div().with_css("height: 20px;").with_child(
                Dom::create_text_do_not_use_without_block_level_wrapper(format!("moves: {moves}")),
            ))
            .with_child(
                Dom::create_div()
                    .with_class("dfw-pad".into())
                    .with_css("width: 300px; height: 100px; background: #dddddd;")
                    .with_callback(
                        EventFilter::Hover(HoverEventFilter::MouseMove),
                        data.clone(),
                        dfw_on_move as usize,
                    )
                    .with_child(
                        Dom::create_div()
                            .with_class("dfw-knob".into())
                            .with_css("width: 10px; height: 10px; background: #333333;"),
                    ),
            )
    }

    /// The pointer walks along the bottom of the pad (away from the knob: after it entered,
    /// no node's state changes); every move slides the knob by a write from the handler and
    /// rebuilds a DOM in which only the caption's text is new. After every move the knob
    /// stands where the handler put it - down the full path, with the step `text` alone, and
    /// with every step.
    #[test]
    fn a_style_a_callback_wrote_is_laid_out_by_the_rebuild_it_asked_for() {
        use azul_layout::window::DiffFirstStep;

        let knob_x = |window: &HeadlessWindow| {
            rects_by_class(window, "dfw-knob").first().map(|r| r.origin.x)
        };
        let all = DiffFirstStep::ALL;
        let paths: [&[DiffFirstStep]; 3] = [&[], &[DiffFirstStep::Text], &all];
        for steps in paths {
            azul_layout::window::set_diff_first_steps_on_this_thread(Some(steps));
            let state = Arc::new(RefCell::new(RefAny::new(DfwMoves(0))));
            let mut window = make_window_sized(&state, dfw_layout, 400.0, 200.0);
            window.regenerate_layout().expect("initial layout");
            window.regenerate_layout().expect("settle");
            let _ = window.common.take_regeneration();
            let start = knob_x(&window).expect("the knob is laid out");
            let mut calls_per_move = Vec::new();
            let mut before = 0;
            for (i, x) in [100.0_f32, 120.0, 140.0, 160.0, 180.0].into_iter().enumerate() {
                step(&mut window, HeadlessEvent::MouseMove { x, y: 100.0 });
                let moves = state
                    .borrow_mut()
                    .downcast_ref::<DfwMoves>()
                    .map(|state| state.0)
                    .unwrap_or(0);
                calls_per_move.push(moves - before);
                before = moves;
                assert_eq!(
                    knob_x(&window),
                    Some(start + 10.0 * moves as f32),
                    "steps {steps:?}, move {i}: the handler ran {moves} times and wrote a \
                     margin of {} px, the knob is not there",
                    10 * moves
                );
            }
            azul_layout::window::set_diff_first_steps_on_this_thread(None);
            assert!(
                calls_per_move[2..].iter().all(|calls| *calls >= 1),
                "premise (steps {steps:?}): the handler runs on every move inside the pad \
                 ({calls_per_move:?})"
            );
        }
    }

    // --- diff first, step `view`: a VirtualView's document that renders again ----------------
    //
    // A map view after a pan: its labels are keyed boxes, absolutely positioned by their own
    // style (`left`, `top`), under a sheet of classes. Most persist and only moved; some
    // enter and some leave. The view's callback returns a new DOM every time; the step
    // pairs its nodes with the render before FIRST and runs the cascade for the labels that
    // entered, where the full path styles every node.

    /// One label of [`dfv_render`]: its key, where it stands, which `dfv-k<kind>` it wears.
    #[derive(Clone)]
    struct DfvLabel {
        key: u64,
        left: f32,
        top: f32,
        kind: usize,
    }

    /// The app state of [`dfv_layout`] and the data of the view it builds.
    #[derive(Clone)]
    struct DfvLabels(Vec<DfvLabel>);

    /// `div.dfv-picture > (div.dfv-label.dfv-k<n>[key][left, top] > text)*` under a sheet
    /// with a rule per class (one of them inherited by the text). A label of kind 4 or 5
    /// (no walk makes one) also holds a `div.dfv-badge`, which the label's class colours
    /// through a descendant rule: the label's own style is the same under both.
    extern "C" fn dfv_render(
        data: RefAny,
        info: azul_core::callbacks::VirtualViewCallbackInfo,
    ) -> azul_core::callbacks::VirtualViewReturn {
        use azul_core::geom::{LogicalPosition, LogicalRect};
        let mut data = data;
        let labels = data
            .downcast_ref::<DfvLabels>()
            .map(|labels| labels.0.clone())
            .unwrap_or_default();
        let mut picture = Dom::create_div()
            .with_class("dfv-picture".into())
            .with_component_css(azul_css::css::Css::from_string(
                ".dfv-picture { position: relative; width: 100%; height: 100%; font-size: 10px; } \
                 .dfv-label { position: absolute; padding: 1px; background: #eeeeee; } \
                 .dfv-k1 { color: #aa0000; } .dfv-k2 { font-size: 12px; background: #ddddff; } \
                 .dfv-k3 { padding: 3px; } \
                 .dfv-badge { position: absolute; left: 0px; top: 0px; width: 4px; height: 4px; } \
                 .dfv-k4 .dfv-badge { background: #00aa00; } \
                 .dfv-k5 .dfv-badge { background: #aa00aa; }"
                    .into(),
            ));
        for label in &labels {
            let mut node = Dom::create_div()
                .with_class("dfv-label".into())
                .with_class(format!("dfv-k{}", label.kind).into())
                .with_key(label.key)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(format!(
                    "L{}",
                    label.key
                )));
            if label.kind >= 4 {
                node = node.with_child(Dom::create_div().with_class("dfv-badge".into()));
            }
            // The label's OWN style (not a sheet hung on it), as the map view sets it.
            node.root
                .set_css(&format!("left: {}px; top: {}px;", label.left, label.top));
            picture = picture.with_child(node);
        }
        let size = info.get_bounds().get_logical_size();
        let rect = LogicalRect::new(LogicalPosition::zero(), size);
        azul_core::callbacks::VirtualViewReturn {
            dom: azul_core::dom::OptionDom::Some(picture),
            materialized: rect,
            virtual_rect: rect,
        }
    }

    extern "C" fn dfv_layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        let labels = data
            .downcast_ref::<DfvLabels>()
            .map(|labels| DfvLabels(labels.0.clone()))
            .expect("app state");
        Dom::create_body().with_css("margin: 0px;").with_child(
            Dom::create_virtual_view(
                RefAny::new(labels),
                azul_core::callbacks::VirtualViewCallback::create(dfv_render),
            )
            .with_css("width: 380px; height: 280px;"),
        )
    }

    /// `count` labels on a grid, keys `first_key..`.
    fn dfv_labels(first_key: u64, count: usize) -> Vec<DfvLabel> {
        (0..count)
            .map(|i| DfvLabel {
                key: first_key + i as u64,
                left: (i % 20) as f32 * 18.0,
                top: (i / 20) as f32 * 9.0,
                kind: i % 4,
            })
            .collect()
    }

    fn dfv_window(state: &Arc<RefCell<RefAny>>) -> HeadlessWindow {
        let mut window = make_window_sized(state, dfv_layout, 400.0, 300.0);
        window.regenerate_layout().expect("initial layout");
        window.regenerate_layout().expect("settle");
        window.common.drain_virtual_view_updates();
        let _ = window.common.take_regeneration();
        window
    }

    fn dfv_change(state: &Arc<RefCell<RefAny>>, change: impl FnOnce(&mut Vec<DfvLabel>)) {
        let mut guard = state.borrow_mut();
        let any: &mut RefAny = &mut guard;
        let mut labels = any.downcast_mut::<DfvLabels>().expect("app state");
        change(&mut labels.0);
    }

    /// The view renders again from the app's state, with exactly `steps` of the diff-first
    /// pipeline on: a rebuild of the window's DOM (identical: the view is the same node)
    /// queues the view, the drain renders it.
    fn dfv_rerender(window: &mut HeadlessWindow, steps: &[azul_layout::window::DiffFirstStep]) {
        use crate::desktop::shell2::common::event::PlatformWindow;

        azul_layout::window::set_diff_first_steps_on_this_thread(Some(steps));
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        let result = window.regenerate_layout();
        window.common.drain_virtual_view_updates();
        azul_layout::window::set_diff_first_steps_on_this_thread(None);
        result.expect("the rebuild");
        let _ = window.common.take_regeneration();
    }

    /// The view's document: its `DomId`.
    fn dfv_child(window: &HeadlessWindow) -> Option<azul_core::dom::DomId> {
        let lw = window.common.layout_window.as_ref()?;
        lw.layout_results.keys().copied().find(|dom| dom.inner != 0)
    }

    /// What a render of the view leaves, line by line, as [`df_dump`] writes a window's DOM:
    /// every node of the view's document (what it is, its classes, its state, its box, its
    /// resolved properties, its compact entries), the tags, and every item of its list.
    fn dfv_dump(window: &HeadlessWindow) -> Vec<String> {
        use azul_core::{
            dom::{DomNodeId, NodeId},
            styled_dom::NodeHierarchyItemId,
        };

        let lw = window.common.layout_window.as_ref().expect("a layout window");
        let Some(child) = dfv_child(window) else {
            return vec!["no view document".to_string()];
        };
        let result = &lw.layout_results[&child];
        let styled = &result.styled_dom;
        let cache = styled.get_css_property_cache();
        let mut lines = Vec::new();
        for (index, node) in styled.node_data.as_ref().iter().enumerate() {
            let id = NodeId::new(index);
            let dom_node = DomNodeId {
                dom: child,
                node: NodeHierarchyItemId::from_crate_internal(Some(id)),
            };
            let state = styled.styled_nodes.as_ref()[index].styled_node_state;
            lines.push(format!(
                "node {index}: {:?} {:?} state {state:?} at {:?} size {:?} in {:?}",
                node.get_node_type(),
                node.get_ids_and_classes()
                    .iter()
                    .map(|class| format!("{class:?}"))
                    .collect::<Vec<_>>(),
                lw.get_node_position(dom_node),
                lw.get_node_size(dom_node),
                styled.node_hierarchy.as_ref()[index],
            ));
            let style: Vec<String> = azul_css::props::property::CssPropertyType::ALL
                .iter()
                .filter_map(|ty| cache.get_property(node, &id, &state, ty))
                .map(|property| format!("{property:?}"))
                .collect();
            lines.push(format!("style {index}: {style:?}"));
            if let Some(compact) = cache.compact_cache.as_ref() {
                lines.push(format!(
                    "compact {index}: {:?} {:?} {:?} {:?}",
                    compact.tier1_enums[index],
                    compact.tier2_dims[index],
                    compact.tier2_cold[index],
                    compact.tier2b_text[index],
                ));
            }
        }
        lines.push(format!("tags: {:?}", styled.tag_ids_to_node_ids));
        for (index, item) in result.display_list.items.iter().enumerate() {
            lines.push(match item {
                DisplayListItem::HitTestArea { bounds, .. } => {
                    format!("item {index}: hit area {bounds:?}")
                }
                DisplayListItem::Text {
                    glyphs,
                    font_size_px,
                    color,
                    clip_rect,
                    line_box,
                    source_node_index,
                    ..
                } => format!(
                    "item {index}: text {glyphs:?} {font_size_px} {color:?} {clip_rect:?} \
                     {line_box:?} {source_node_index:?}"
                ),
                DisplayListItem::TextLayout {
                    bounds,
                    font_size_px,
                    color,
                    ..
                } => format!("item {index}: text layout {bounds:?} {font_size_px} {color:?}"),
                other => format!("item {index}: {other:?}"),
            });
        }
        lines
    }

    /// The step `kind` of the random walk of a view's labels: every label moves (0, 1: a
    /// pan), some leave and as many enter anywhere in the list (2, 3), a pan with labels
    /// leaving at the front and entering at the end (4), a label changes its class (5),
    /// the list is reversed (6: keyed labels in another order), nothing (7).
    fn dfv_mutate(labels: &mut Vec<DfvLabel>, next_key: &mut u64, kind: usize, picks: [usize; 6]) {
        let pan = |labels: &mut Vec<DfvLabel>, dx: f32, dy: f32| {
            for label in labels.iter_mut() {
                label.left += dx;
                label.top += dy;
            }
        };
        let mut enter = |labels: &mut Vec<DfvLabel>, at: usize, pick: usize| {
            let key = *next_key;
            *next_key += 1;
            let at = at.min(labels.len());
            labels.insert(
                at,
                DfvLabel {
                    key,
                    left: (pick % 300) as f32,
                    top: (pick % 7) as f32 * 30.0,
                    kind: pick % 4,
                },
            );
        };
        match kind {
            0 | 1 => pan(labels, picks[0] as f32 - 32.0, picks[1] as f32 * 0.5 - 16.0),
            2 | 3 => {
                for i in 0..=(picks[5] % 3) {
                    if labels.len() > 4 {
                        let gone = picks[i] % labels.len();
                        let _ = labels.remove(gone);
                    }
                    let at = picks[3 + i % 2] % (labels.len() + 1);
                    enter(labels, at, picks[i]);
                }
            }
            4 => {
                pan(labels, -(picks[0] as f32), 3.0);
                for i in 0..2 {
                    if labels.len() > 4 {
                        let _ = labels.remove(0);
                    }
                    let end = labels.len();
                    enter(labels, end, picks[1 + i]);
                }
            }
            5 => {
                let at = picks[0] % labels.len();
                labels[at].kind = picks[1] % 4;
            }
            6 => labels.reverse(),
            _ => {}
        }
    }

    /// Random re-renders of one view in two windows, one with `steps` on and one down the
    /// full path: after every render the view's document holds the same nodes, resolved
    /// styles, compact entries, tags, boxes and display list in both. Answers how many of
    /// the renders kept rows of the render before.
    fn dfv_walk(steps: &[azul_layout::window::DiffFirstStep]) -> usize {
        let kept_before = azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread();
        for seed in [1u64, 7, 20261121] {
            let state_first = Arc::new(RefCell::new(RefAny::new(DfvLabels(dfv_labels(1, 24)))));
            let state_full = Arc::new(RefCell::new(RefAny::new(DfvLabels(dfv_labels(1, 24)))));
            let mut first = dfv_window(&state_first);
            let mut full = dfv_window(&state_full);
            let (mut key_first, mut key_full) = (1000_u64, 1000_u64);
            let mut random = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
            let mut next = |below: u64| {
                random = random
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                ((random >> 33) % below) as usize
            };
            for step_index in 0..32 {
                let kind = next(8);
                let picks = [next(64), next(64), next(64), next(64), next(64), next(64)];
                dfv_change(&state_first, |labels| dfv_mutate(labels, &mut key_first, kind, picks));
                dfv_change(&state_full, |labels| dfv_mutate(labels, &mut key_full, kind, picks));
                dfv_rerender(&mut first, steps);
                dfv_rerender(&mut full, &[]);
                let (left, right) = (dfv_dump(&first), dfv_dump(&full));
                let differs = left
                    .iter()
                    .zip(right.iter())
                    .position(|(a, b)| a != b)
                    .or((left.len() != right.len()).then_some(left.len().min(right.len())));
                assert!(
                    differs.is_none(),
                    "seed {seed}, step {step_index} (kind {kind}, picks {picks:?}, steps \
                     {steps:?}): line {differs:?} differs\n diff-first: {:?}\n full:       {:?}",
                    differs.and_then(|line| left.get(line)),
                    differs.and_then(|line| right.get(line)),
                );
            }
        }
        azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread() - kept_before
    }

    /// How to read a failure: the message names the seed, the step and its kind
    /// ([`dfv_mutate`]) and the first differing line of the view's document. A `style` /
    /// `compact` line is the cascade that kept rows (`StyledDom::cascade_reusing`; for a
    /// label that only moved: the compact entries built again from its own style), a `node`
    /// / `item` line the layout or the list.
    #[test]
    fn a_view_rendered_again_leaves_what_the_full_path_leaves() {
        use azul_layout::window::DiffFirstStep;
        let kept = dfv_walk(&[DiffFirstStep::View]);
        assert!(kept >= 30, "premise: the renders keep rows of the render before ({kept} did)");
        let _ = dfv_walk(&DiffFirstStep::ALL);
    }

    /// `(whole cascades, nodes the cascade ran for, boxes the view's solver built fresh,
    /// renders that kept rows)` for one re-render of a view of `count` labels after `change`,
    /// with `steps` on.
    fn dfv_cost(
        count: usize,
        steps: &[azul_layout::window::DiffFirstStep],
        change: impl FnOnce(&mut Vec<DfvLabel>),
    ) -> (usize, usize, usize, usize) {
        let state = Arc::new(RefCell::new(RefAny::new(DfvLabels(dfv_labels(1, count)))));
        let mut window = dfv_window(&state);
        dfv_change(&state, change);
        let whole = azul_core::styled_dom::compact_cache_rebuilds_on_this_thread();
        let nodes = azul_core::styled_dom::nodes_cascaded_on_this_thread();
        let kept = azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread();
        dfv_rerender(&mut window, steps);
        let child = dfv_child(&window).expect("the view's document");
        let lw = window.common.layout_window.as_ref().expect("a layout window");
        (
            azul_core::styled_dom::compact_cache_rebuilds_on_this_thread() - whole,
            azul_core::styled_dom::nodes_cascaded_on_this_thread() - nodes,
            lw.child_layout_caches
                .get(&child)
                .map_or(usize::MAX, |cache| cache.last_reconcile_fresh),
            azul_layout::window::tree_rebuilds_that_kept_rows_on_this_thread() - kept,
        )
    }

    /// A pan: every label moves, none enters or leaves. The view's document is KEPT (the
    /// same tree, the same keys: nothing is flattened or paired). The cascade runs for no
    /// node (a label's own `left` / `top` live in its compact entries alone, which are built again)
    /// and the solver builds no box fresh (it places each label again and keeps what is in
    /// it) - in a view of 100 labels and in one of 400. Down the full path every node of
    /// the view is cascaded.
    #[test]
    fn a_view_whose_labels_all_moved_styles_no_node_and_keeps_every_box() {
        use azul_layout::window::DiffFirstStep::View;
        let pan = |labels: &mut Vec<DfvLabel>| {
            for label in labels.iter_mut() {
                label.left += 7.0;
                label.top += 3.0;
            }
        };
        for count in [100, 400] {
            let (whole, nodes, fresh, kept) = dfv_cost(count, &[View], pan);
            assert_eq!(kept, 1, "{count} labels: rows kept");
            assert_eq!((whole, nodes), (0, 0), "{count} labels: no node is cascaded");
            assert_eq!(fresh, 0, "{count} labels: every label keeps its box");
        }
        let (whole, nodes, _, kept) = dfv_cost(100, &[], pan);
        assert_eq!((kept, whole), (0, 1), "the full path: one whole cascade");
        assert_eq!(nodes, 1 + 2 * 100, "over the picture and every label with its text");
    }

    /// One label of a view changes its class, the others stay: the view's document is kept
    /// and the cascade runs for that label and its text - 2 nodes - in a view of 100 labels
    /// and in one of 400, and the solver builds the boxes of that label, not the view's.
    #[test]
    fn one_changed_label_of_a_view_styles_that_label() {
        use azul_layout::window::DiffFirstStep::View;
        let restyle_one = |labels: &mut Vec<DfvLabel>| {
            let at = labels.len() / 2;
            labels[at].kind = (labels[at].kind + 1) % 4;
        };
        for count in [100, 400] {
            let (whole, nodes, fresh, kept) = dfv_cost(count, &[View], restyle_one);
            assert_eq!(kept, 1, "{count} labels: the document is kept");
            assert_eq!((whole, nodes), (0, 2), "{count} labels: the label and its text");
            assert!(
                (1..=6).contains(&fresh),
                "{count} labels: the boxes of one label are built, {fresh} were"
            );
        }
    }

    // --- diff first, step `list`: the display list of a view that renders again ---------------
    //
    // The step `view` keeps the rows of the nodes that are what they were and moves the
    // cascade epoch, which is the solver's only proof that no cascade ran: the view's whole
    // list was emitted. Under `list` the view hands the pass the nodes it cascaded
    // (`solver3::cache::CascadedSince`), and the pass copies the runs of every box that is
    // what it was, where it was.

    /// `(lists patched after a named cascade, lists patched across a changed tree, runs
    /// copied, paint calls of patched builds)` of `window` for one re-render of it after
    /// `change`, with `steps` on.
    fn dfv_list_cost(
        window: &mut HeadlessWindow,
        state: &Arc<RefCell<RefAny>>,
        steps: &[azul_layout::window::DiffFirstStep],
        change: impl FnOnce(&mut Vec<DfvLabel>),
    ) -> (usize, usize, usize, usize) {
        dfv_change(state, change);
        let before = df_patch_counters(window);
        dfv_rerender(window, steps);
        let after = df_patch_counters(window);
        (
            after.lists_patched_after_a_named_cascade - before.lists_patched_after_a_named_cascade,
            after.lists_patched_across_trees - before.lists_patched_across_trees,
            after.cached_runs_copied - before.cached_runs_copied,
            after.runs_painted_by_patched_builds - before.runs_painted_by_patched_builds,
        )
    }

    /// Two windows of the labels `labels`, re-rendered after the same `change`: one with
    /// every step, one with the steps `reference` (every step but the one under test: the
    /// state before it, the view's list emitted in full). Answers the costs of the first
    /// ([`dfv_list_cost`]) after asserting that both views hold the same document and the
    /// same list, line for line ([`dfv_dump`]: every item of the list with every field).
    fn dfv_list_against_full(
        labels: Vec<DfvLabel>,
        change: impl Fn(&mut Vec<DfvLabel>),
        reference: &[azul_layout::window::DiffFirstStep],
        what: &str,
    ) -> (usize, usize, usize, usize) {
        let state_list = Arc::new(RefCell::new(RefAny::new(DfvLabels(labels.clone()))));
        let state_full = Arc::new(RefCell::new(RefAny::new(DfvLabels(labels))));
        let mut with_list = dfv_window(&state_list);
        let mut in_full = dfv_window(&state_full);
        let cost = dfv_list_cost(
            &mut with_list,
            &state_list,
            &azul_layout::window::DiffFirstStep::ALL,
            &change,
        );
        let before = dfv_list_cost(&mut in_full, &state_full, reference, &change);
        assert_eq!(
            (before.0, before.1),
            (0, 0),
            "{what}: premise: without the step no list is patched for the cascade's names"
        );
        let dump = |window: &HeadlessWindow| {
            let mut lines = dfv_dump(window);
            if let Some(child) = dfv_child(window) {
                let lw = window.common.layout_window.as_ref().expect("a layout window");
                lines.extend(df_list_tables(&lw.layout_results[&child].display_list));
            }
            lines
        };
        let (left, right) = (dump(&with_list), dump(&in_full));
        let differs = left
            .iter()
            .zip(right.iter())
            .position(|(a, b)| a != b)
            .or((left.len() != right.len()).then_some(left.len().min(right.len())));
        assert!(
            differs.is_none(),
            "{what}: line {differs:?} of the view differs\n patched: {:?}\n in full: {:?}",
            differs.and_then(|line| left.get(line)),
            differs.and_then(|line| right.get(line)),
        );
        cost
    }

    /// One label of a view changes its class and the others stay: the view's list is
    /// PATCHED - the runs of that label are painted, every other label's are copied - in a
    /// view of 100 labels and in one of 400, and the list is the one a full emission makes.
    /// (A view that renders the DOM it rendered before builds no list at all: its document
    /// is taken back as it lies, `reclaim_unchanged_child`.)
    ///
    /// Mutation: the hand-over dropped (`cascaded_since_list` never set, or the epochs it
    /// carries not the list's and the DOM's): no list is patched, 0 runs are copied.
    #[test]
    fn one_changed_label_of_a_view_paints_that_label_and_copies_the_rest() {
        // The same label at both sizes (kind 1 to 2: a colour for a font size and a
        // background), so that the two views paint the same.
        let restyle_one = |labels: &mut Vec<DfvLabel>| {
            labels[41].kind = (labels[41].kind + 1) % 4;
        };
        let mut painted_at = Vec::new();
        for count in [100_usize, 400] {
            let (patched, _, copied, painted) = dfv_list_against_full(
                dfv_labels(1, count),
                restyle_one,
                &DF_BEFORE_LIST,
                "one label restyled",
            );
            assert_eq!(patched, 1, "{count} labels: the view's list is patched");
            // A label paints a background and a line of text: two runs each.
            assert!(
                copied >= count - 1,
                "{count} labels: the runs of the labels that stayed are copied ({copied})"
            );
            assert!(
                (1..=12).contains(&painted),
                "{count} labels: the runs of one label are painted, {painted} were"
            );
            painted_at.push(painted);
        }
        assert_eq!(
            painted_at[0], painted_at[1],
            "what is painted does not grow with the view"
        );
    }

    /// 5 % of the labels of a view are restyled where they stand: 5 % are painted and the
    /// rest copied, at 100 labels and at 400, and the list is a full emission's.
    #[test]
    fn a_view_that_restyled_5_percent_of_its_labels_paints_5_percent() {
        let restyle_some = |labels: &mut Vec<DfvLabel>| {
            for label in labels.iter_mut().step_by(20) {
                label.kind = (label.kind + 1) % 4;
            }
        };
        for count in [100_usize, 400] {
            let restyled = count / 20;
            let (patched, _, copied, painted) = dfv_list_against_full(
                dfv_labels(1, count),
                restyle_some,
                &DF_BEFORE_LIST,
                "5 % restyled",
            );
            assert_eq!(patched, 1, "{count} labels: the view's list is patched");
            assert!(
                copied >= count - restyled,
                "{count} labels: the labels that stayed are copied ({copied} runs)"
            );
            assert!(
                painted >= restyled && painted <= 6 * restyled + 8,
                "{count} labels: the runs of {restyled} labels are painted, {painted} were"
            );
        }
    }

    /// Every label of a view moves (a pan that gives each new insets): every label is
    /// painted - a moved box is painted anew by any pass that is no tween frame - and the
    /// list is a full emission's. The patch gains nothing here, and must cost nothing
    /// wrong: what it copies is the few runs that did not move (the picture's own).
    #[test]
    fn a_view_whose_labels_all_moved_paints_every_label() {
        let pan = |labels: &mut Vec<DfvLabel>| {
            for label in labels.iter_mut() {
                label.left += 7.0;
                label.top += 3.0;
            }
        };
        for count in [100_usize, 400] {
            let (patched, _, copied, painted) = dfv_list_against_full(
                dfv_labels(1, count),
                pan,
                &DF_BEFORE_LIST,
                "every label moved",
            );
            assert_eq!(patched, 1, "{count} labels: the list is patched all the same");
            assert!(
                painted >= count,
                "{count} labels: every label is painted ({painted} paint calls)"
            );
            assert!(
                copied <= 8,
                "{count} labels: no label's run is copied ({copied} runs were)"
            );
        }
    }

    /// The mutant of the step: a node the cascade ran for whose run is copied anyway. Every
    /// label holds a badge that its label's class colours through a descendant rule; one
    /// label changes its class. The label's own style does not move and its box stands
    /// where it stood; the badge's node data is what it was, so the reconcile keeps its box
    /// clean, and nothing but the cascade - the CSS diff of the re-render and the nodes it
    /// names - says the badge paints another colour. The list is a full emission's: the
    /// badge has the new colour.
    ///
    /// Mutation: the boxes the cascade reached left out of the re-emit set (in
    /// `layout_document`, `css_dirty_reemit` empty in the arm of the named cascade): the
    /// badge's run is copied with the old colour and the two views differ at its item.
    /// (Either half alone - the css dirt, or the named nodes' boxes - re-emits the badge;
    /// the named nodes are the superset that holds where no css diff was made.)
    #[test]
    fn a_node_the_cascade_reached_is_painted_though_its_box_is_clean() {
        let mut labels = dfv_labels(1, 40);
        for label in &mut labels {
            label.kind = 4;
        }
        let recolour_one = |labels: &mut Vec<DfvLabel>| labels[20].kind = 5;
        let (patched, _, copied, painted) = dfv_list_against_full(
            labels,
            recolour_one,
            &DF_BEFORE_LIST,
            "a badge recoloured by its label's class",
        );
        assert_eq!(patched, 1, "premise: the view's list is patched");
        assert!(copied >= 39, "premise: the other labels are copied ({copied} runs)");
        assert!(painted >= 1, "the badge is painted ({painted} paint calls)");
    }

    /// Labels that enter at the end of a view's list while the others stay where they are:
    /// the view's tree changed, and its list is patched ACROSS the trees (the step `tlist`):
    /// the labels that stayed are copied, the ones that entered painted - 5 of 100 and 20
    /// of 400 - and the list is a full emission's.
    ///
    /// Mutation: the arm of the changed tree off: no list is patched across the trees and
    /// 0 runs are copied.
    #[test]
    fn labels_that_enter_a_view_are_painted_and_the_others_copied() {
        for count in [100_usize, 400] {
            let entered = count / 20;
            let (_, across, copied, painted) = dfv_list_against_full(
                dfv_labels(1, count),
                move |labels: &mut Vec<DfvLabel>| labels.extend(dfv_labels(50_000, entered)),
                &DF_BEFORE_TLIST,
                "labels entered at the end",
            );
            assert_eq!(across, 1, "{count} labels: the list is patched across the changed tree");
            assert!(
                copied >= count,
                "{count} labels: the labels that stayed are copied ({copied} runs)"
            );
            assert!(
                painted >= entered && painted <= 8 * entered + 12,
                "{count} labels: the {entered} labels that entered are painted ({painted})"
            );
        }
    }

    /// A settle: every label moves and 5 % of them are exchanged for new ones. The cascade
    /// runs for the labels that entered (a box and its text each): 10 nodes of 201 for 100
    /// labels, 40 of 801 for 400 - 5 %, at both sizes. With the exchange at the end of the
    /// list (the labels that stay keep their node ids) the solver builds the boxes of the
    /// labels that entered and keeps the others'.
    ///
    /// NOT asserted, because it does not hold yet: where the labels that enter are not as
    /// many as those that leave at the same place of the list, every label after that place
    /// gets another node id, and the solver lays a renumbered box out anew whatever it is
    /// told (`solver3::cache::DomDiff::keeps`; `audit/diff-first.md`, diff-first-06). The
    /// cascade's count does not depend on it.
    #[test]
    fn a_view_that_exchanged_5_percent_of_its_labels_styles_5_percent() {
        use azul_layout::window::DiffFirstStep::View;
        let settle_at_the_end = |labels: &mut Vec<DfvLabel>| {
            let exchanged = labels.len() / 20;
            for label in labels.iter_mut() {
                label.left += 5.0;
                label.top += 2.0;
            }
            let kept = labels.len() - exchanged;
            labels.truncate(kept);
            labels.extend(dfv_labels(50_000, exchanged));
        };
        let settle_in_the_middle = |labels: &mut Vec<DfvLabel>| {
            let exchanged = labels.len() / 20;
            for label in labels.iter_mut() {
                label.left += 5.0;
                label.top += 2.0;
            }
            let from = labels.len() / 3;
            let _ = labels.splice(from..from + exchanged, dfv_labels(50_000, exchanged));
        };
        let (whole_small, nodes_small, fresh_small, kept_small) =
            dfv_cost(100, &[View], settle_at_the_end);
        let (whole_large, nodes_large, fresh_large, kept_large) =
            dfv_cost(400, &[View], settle_at_the_end);
        assert_eq!((kept_small, kept_large), (1, 1), "rows kept");
        assert_eq!((whole_small, whole_large), (0, 0), "no cascade over the whole view");
        assert_eq!((nodes_small, nodes_large), (10, 40), "the labels that entered: 2 nodes each");
        assert!(
            (5..=5 * 3 + 4).contains(&fresh_small) && (20..=20 * 3 + 4).contains(&fresh_large),
            "the solver builds the boxes of the labels that entered: {fresh_small} of 100, \
             {fresh_large} of 400"
        );
        // In the middle of the list: the same cascade.
        let (whole, nodes, _, kept) = dfv_cost(400, &[View], settle_in_the_middle);
        assert_eq!((kept, whole, nodes), (1, 0, 40), "exchanged in the middle of the list");
    }
}

#[cfg(test)]
mod native_backbuffer_reuse_law {
    use super::frame_may_reuse_previous_pixels;

    /// Steady state: same size, same canvas, nothing forced, and the output
    /// buffer holds the previous frame.
    #[test]
    fn a_steady_frame_reuses_the_previous_one() {
        assert!(frame_may_reuse_previous_pixels(
            false, false, false, false, true
        ));
    }

    #[test]
    fn a_shrink_a_recolour_and_a_forced_repaint_do_not() {
        // shrink (resize that did not preserve the pixels)
        assert!(!frame_may_reuse_previous_pixels(
            true, false, false, false, true
        ));
        // the canvas colour changed under the reused pixels
        assert!(!frame_may_reuse_previous_pixels(
            false, false, true, false, true
        ));
        // the shell asked for a full repaint
        assert!(!frame_may_reuse_previous_pixels(
            false, false, false, true, true
        ));
    }

    /// THE LAW (I6): a window GROWN by a resize makes the platform re-create
    /// its backbuffer - `CreateDIBSection` on Windows hands back a ZEROED
    /// DIB. The compositor's own layer pixbuf grew with its pixels intact,
    /// which is what `resize_preserved_pixels` reports, but the buffer the
    /// frame is about to be painted INTO did not: it holds nothing, so the
    /// frame cannot be incremental.
    #[test]
    fn a_grow_into_a_freshly_allocated_backbuffer_is_a_full_repaint() {
        assert!(
            !frame_may_reuse_previous_pixels(true, true, false, false, false),
            "a grow-resize painted only its damage strips into a backbuffer that holds no \
             previous frame"
        );
    }

    /// Same law without a resize: the shell can hand over a fresh buffer at
    /// any time (a native frame drops the retained `last_frame`, so the next
    /// owned frame starts from a blank pixmap).
    #[test]
    fn a_frame_with_no_previous_pixels_anywhere_is_a_full_repaint() {
        assert!(
            !frame_may_reuse_previous_pixels(false, false, false, false, false),
            "an unchanged-size frame painted only its damage strips into a buffer that holds no \
             previous frame"
        );
    }
}

/// A window an app opens from a callback (`CallbackInfo::create_window`: AzMail's compose
/// window, a dialog) is a window of its own in a headless run too: its create callback runs,
/// it is laid out, and its timers - the debug server's among them - and its threads are
/// pumped like the root's. It used to be created and then only polled for a Close: never laid
/// out, its timers and threads never run, so nothing (and no E2E) could reach it.
#[cfg(test)]
mod child_window_tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use azul_core::{
        callbacks::{LayoutCallback, LayoutCallbackInfo, TimerCallbackReturn, Update},
        dom::Dom,
        refany::{OptionRefAny, RefAny},
        task::{TerminateTimer, TimerId},
    };
    use azul_layout::{
        callbacks::{Callback, CallbackInfo},
        timer::{Timer, TimerCallbackInfo},
    };

    use super::*;

    static CHILD_CREATED: AtomicUsize = AtomicUsize::new(0);
    static CHILD_LAYOUTS: AtomicUsize = AtomicUsize::new(0);
    static CHILD_TIMER_RUNS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn root_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
    }

    extern "C" fn child_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        CHILD_LAYOUTS.fetch_add(1, Ordering::SeqCst);
        Dom::create_body().with_child(Dom::create_div())
    }

    extern "C" fn child_timer(_data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
        CHILD_TIMER_RUNS.fetch_add(1, Ordering::SeqCst);
        TimerCallbackReturn::terminate_unchanged()
    }

    /// The child's create callback starts a timer, as AzMail's compose window does to put the
    /// caret into its editor once it is laid out.
    extern "C" fn child_created(data: RefAny, mut info: CallbackInfo) -> Update {
        CHILD_CREATED.fetch_add(1, Ordering::SeqCst);
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(data, child_timer as azul_layout::timer::TimerCallbackType, get_time),
        );
        Update::DoNothing
    }

    fn window(layout: azul_core::callbacks::LayoutCallbackType, id: &str) -> WindowCreateOptions {
        let mut opts = WindowCreateOptions::default();
        opts.window_state.layout_callback = LayoutCallback {
            cb: layout,
            ctx: OptionRefAny::None,
        };
        opts.window_state.window_id = id.into();
        opts.window_state.size.dimensions = azul_core::geom::LogicalSize::new(300.0, 200.0);
        opts
    }

    fn root() -> HeadlessWindow {
        root_with(root_layout)
    }

    fn root_with(layout: azul_core::callbacks::LayoutCallbackType) -> HeadlessWindow {
        use azul_core::icon::{IconProviderHandle, SharedIconProvider};
        HeadlessWindow::new(
            window(layout, "root"),
            Arc::new(RefCell::new(RefAny::new(()))),
            event::SharedUndoManager::new(),
            AppConfig::default(),
            SharedIconProvider::from_handle(IconProviderHandle::default()),
            Arc::new(FcFontCache::default()),
            None,
        )
        .unwrap()
    }

    #[test]
    fn a_window_opened_from_a_callback_is_laid_out_and_runs_its_timers() {
        let mut root = root();
        let mut child = window(child_layout, "child");
        child.create_callback =
            Some(Callback::create(child_created as azul_layout::callbacks::CallbackType)).into();
        root.queue_window_create(child);
        root.pump_children();
        assert_eq!(root.children.len(), 1, "the child window exists");
        assert_eq!(CHILD_CREATED.load(Ordering::SeqCst), 1, "its create callback ran once");
        assert!(CHILD_LAYOUTS.load(Ordering::SeqCst) >= 1, "it was laid out");
        assert!(
            root.children[0]
                .common
                .layout_window
                .as_ref()
                .is_some_and(|lw| !lw.layout_results.is_empty()),
            "it has a layout result"
        );
        for _ in 0..3 {
            root.pump_children();
        }
        assert_eq!(CHILD_TIMER_RUNS.load(Ordering::SeqCst), 1, "its timer ran");
        assert_eq!(CHILD_CREATED.load(Ordering::SeqCst), 1, "created once, pumped many times");
    }

    /// A window opened from a callback draws with the registered fonts of the window that opened
    /// it, as the windows of the other shells draw with the app's through the app's font manager:
    /// a face registered through the one is the other's too. A window built on its own has
    /// registered fonts of its own, and so has one given the app's
    /// (`HeadlessWindow::share_registered_fonts`).
    #[test]
    fn a_window_opened_from_a_callback_shares_the_registered_fonts_of_its_opener() {
        let fonts = |window: &HeadlessWindow| {
            Arc::clone(
                &window
                    .common
                    .layout_window
                    .as_ref()
                    .expect("a layout window")
                    .font_manager
                    .registered_fonts,
            )
        };
        let mut opener = root();
        opener.queue_window_create(window(root_layout, "child"));
        opener.pump_children();
        assert_eq!(opener.children.len(), 1, "the child window exists");
        assert!(
            Arc::ptr_eq(&fonts(&opener), &fonts(&opener.children[0])),
            "the child draws with the registered fonts of its opener"
        );

        let mut alone = root_with(root_layout);
        assert!(
            !Arc::ptr_eq(&fonts(&opener), &fonts(&alone)),
            "a window built on its own has registered fonts of its own"
        );
        alone.share_registered_fonts(&fonts(&opener));
        assert!(
            Arc::ptr_eq(&fonts(&opener), &fonts(&alone)),
            "and takes the ones it is given"
        );
    }

    static ROOT_LAYOUTS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn counting_root_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        ROOT_LAYOUTS.fetch_add(1, Ordering::SeqCst);
        Dom::create_body()
    }

    /// What AzMail's compose window answers when a mail is sent: the main window must show it
    /// in Sent Items.
    extern "C" fn refresh_all_timer(_data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
        TimerCallbackReturn {
            should_update: Update::RefreshDomAllWindows,
            should_terminate: TerminateTimer::Terminate,
        }
    }

    extern "C" fn child_refreshing_everything(data: RefAny, mut info: CallbackInfo) -> Update {
        let get_time = info.get_system_time_fn();
        info.add_timer(
            TimerId::unique(),
            Timer::create(
                data,
                refresh_all_timer as azul_layout::timer::TimerCallbackType,
                get_time,
            ),
        );
        Update::DoNothing
    }

    extern "C" fn quiet_child_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
    }

    /// `Update::RefreshDomAllWindows` from a timer, a thread's write-back or a finished request
    /// rebuilds EVERY window of the app, as it does from an event handler: the shared
    /// `process_timers_and_threads` treated it as a plain `RefreshDom` of the window it ran in.
    #[test]
    fn a_child_timer_answering_refresh_all_windows_rebuilds_the_root() {
        let mut root = root_with(counting_root_layout);
        root.regenerate_layout().expect("the root's first layout");
        let _ = root.common.take_regeneration();
        let before = ROOT_LAYOUTS.load(Ordering::SeqCst);
        let mut child = window(quiet_child_layout, "child");
        child.create_callback = Some(Callback::create(
            child_refreshing_everything as azul_layout::callbacks::CallbackType,
        ))
        .into();
        root.queue_window_create(child);
        for _ in 0..3 {
            root.pump_children();
            root.pump_once(true);
        }
        assert!(
            ROOT_LAYOUTS.load(Ordering::SeqCst) > before,
            "the root was rebuilt for the child's RefreshDomAllWindows"
        );
    }

    /// A `<transient-window>`'s popup, as the owner's sync opens one (a Modal's window, a
    /// Popover's, a combobox's list): a `Menu`-type window whose layout ctx is its mailbox.
    fn transient_popup(owner: &HeadlessWindow) -> (WindowCreateOptions, RefAny) {
        transient_popup_showing(owner, Dom::create_div())
    }

    /// [`transient_popup`] whose mailbox holds `content`: what the owner's subtree put there.
    fn transient_popup_showing(
        owner: &HeadlessWindow,
        content: Dom,
    ) -> (WindowCreateOptions, RefAny) {
        use azul_core::{
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            id::NodeId,
            transient::TransientWindowConfig,
        };
        use azul_layout::transient::{placement_for, transient_dom_id, OpenTransientWindow};
        let open = OpenTransientWindow {
            source_node: NodeId::new(1),
            content_dom: transient_dom_id(0),
            placement: placement_for(
                NodeId::new(1),
                LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(300.0, 200.0)),
                &TransientWindowConfig::opened(),
            ),
            content_size: LogicalSize::new(200.0, 120.0),
            surface: OptionRefAny::None,
            torn: None,
            anchor_override: None,
            attr_torn: false,
        };
        crate::desktop::shell2::common::transient::popup_create_options(
            0,
            owner.common.current_window_state(),
            &open,
            content,
            None,
            false,
            None,
            &[],
        )
    }

    fn child_ids(owner: &HeadlessWindow) -> Vec<String> {
        owner
            .children
            .iter()
            .map(|c| c.common.current_window_state().window_id.as_str().to_string())
            .collect()
    }

    /// A transient popup (a Modal's window, a Popover's, a combobox's list) is a `Menu`-type
    /// window too (`transient::popup_window_state`), but it is no window-based MENU: its node
    /// owns it, and the transient machinery closes it and tells the node (a Modal's on_close).
    /// The owner's light dismissal of its menus - an Escape or a press that reached it - closed
    /// every `Menu`-type child, the transient ones included, and told nobody: AzCalculator's
    /// About box (a Modal) vanished on Escape while the app still held it open, and the Escape
    /// never reached it (E2E-A, 2026-10-06).
    #[test]
    fn the_owner_dismisses_its_menu_windows_but_not_its_transient_popups() {
        let mut root = root();
        let mut menu = window(root_layout, "azul-menu");
        menu.window_state.flags.window_type = azul_core::window::WindowType::Menu;
        let (popup, _mailbox) = transient_popup(&root);
        root.queue_window_create(menu);
        root.queue_window_create(popup);
        root.pump_children();
        let mut ids = child_ids(&root);
        ids.sort();
        assert_eq!(ids, vec!["azul-menu".to_string(), "azul-transient".to_string()]);

        assert!(root.dismiss_menu_windows(), "a window-based menu was open");
        assert_eq!(
            child_ids(&root),
            vec!["azul-transient".to_string()],
            "the menu closed; the transient popup is its node's to close"
        );
        assert!(
            !root.dismiss_menu_windows(),
            "a transient popup alone is no menu to dismiss"
        );
    }

    /// A window that closes takes the popups it opened with it, however it closes: its parent's
    /// rebuild dropped it (a popup's own popup - AzDrive's payment popover over its Add drive
    /// dialog, both gone in one Cancel), its user or its app closed it. A popup opened by a popup
    /// is hoisted into the root window's children, so nothing else ever closes it: the window
    /// that opened it closed without telling its mailbox, and the nested popup stayed open on
    /// its own (the add-drive E2E's step 11, 2026-10-10).
    #[test]
    fn a_closing_window_closes_the_popups_it_opened() {
        use azul_core::{
            geom::{LogicalPosition, LogicalRect, LogicalSize},
            id::NodeId,
            transient::TransientWindowConfig,
        };
        use crate::desktop::shell2::common::transient::{poll_popup, PopupAction};

        let mut root = root();
        root.regenerate_layout().expect("the window's first layout");
        let (popup, mailbox) = transient_popup(&root);
        {
            let lw = root
                .common
                .layout_window
                .as_mut()
                .expect("a laid-out window has its layout window");
            let placement = azul_layout::transient::placement_for(
                NodeId::new(1),
                LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(300.0, 200.0)),
                &TransientWindowConfig::opened(),
            );
            let opened = lw
                .transient_windows
                .reconcile(&[placement], |_, _| Some(LogicalSize::new(200.0, 120.0)))
                .opened;
            assert_eq!(opened.len(), 1, "the window opened a popup");
            lw.transient_windows
                .get_mut(opened[0])
                .expect("the popup is open")
                .surface = OptionRefAny::Some(mailbox);
        }
        assert_eq!(poll_popup(&popup.window_state), PopupAction::Nothing);

        root.close();

        assert_eq!(
            poll_popup(&popup.window_state),
            PopupAction::Close,
            "the popup was told to close with the window that opened it"
        );
        assert!(
            root.common
                .layout_window
                .as_ref()
                .is_some_and(|lw| lw.transient_windows.open_windows().is_empty()),
            "the closed window holds no open popup"
        );
    }

    /// A key that reached the owner while its transient popup holds the keyboard goes into the
    /// popup's mailbox (`forward_keys_to_popup`: headless, like X11, makes no popup a key
    /// window) - and the owner runs the popup's pass right away, as X11, macOS and Win32 do
    /// (`deliver_forwarded_keys`). Headless left the key in the mailbox until some other input
    /// reached the popup, which in a script is never: a Modal never heard the Escape that
    /// closes it (E2E-A, 2026-10-06).
    #[test]
    fn a_key_the_owner_forwards_to_its_popup_is_replayed_by_the_popup_at_once() {
        use azul_core::window::{KeyboardState, OptionVirtualKeyCode};

        use crate::desktop::shell2::common::transient::{
            forward_key, has_forwarded_keys, ForwardedKey,
        };
        let mut root = root();
        let (popup, mailbox) = transient_popup(&root);
        root.queue_window_create(popup);
        root.pump_children();
        assert_eq!(child_ids(&root), vec!["azul-transient".to_string()]);

        let mut keyboard = KeyboardState::default();
        keyboard.current_virtual_keycode = Some(VirtualKeyCode::Escape).into();
        keyboard.pressed_virtual_keycodes = vec![VirtualKeyCode::Escape].into();
        assert!(forward_key(
            &mailbox,
            ForwardedKey {
                keyboard,
                previous_key: OptionVirtualKeyCode::None,
                text: None,
            },
        ));
        assert!(has_forwarded_keys(root.children[0].common.current_window_state()));

        root.deliver_forwarded_keys();
        assert!(
            root.children
                .iter()
                .all(|c| !has_forwarded_keys(c.common.current_window_state())),
            "the popup replayed the forwarded key in the owner's pass"
        );
    }

    const PAY_PAGE: &str = "https://pay.example/fields#cs=secret";

    static OWNER_LAYOUTS: AtomicUsize = AtomicUsize::new(0);
    static PAGE_OWNER_LAYOUTS: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn counting_owner_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        OWNER_LAYOUTS.fetch_add(1, Ordering::SeqCst);
        Dom::create_body()
    }

    extern "C" fn counting_page_owner_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        PAGE_OWNER_LAYOUTS.fetch_add(1, Ordering::SeqCst);
        Dom::create_body()
    }

    /// What AzDrive's payment popover answers when its card page says which card is typed (a
    /// navigation to `_bridge/brand?v=visa`, cancelled): the state the owner's layout reads
    /// changed - the popup's content is built there.
    extern "C" fn page_said_the_brand(_data: RefAny, mut info: CallbackInfo) -> Update {
        info.prevent_default();
        Update::RefreshDom
    }

    /// The same page in a window of its own, whose answer is for every window (a sign-in that
    /// the main window shows the account of).
    extern "C" fn page_signed_in(_data: RefAny, mut info: CallbackInfo) -> Update {
        info.prevent_default();
        Update::RefreshDomAllWindows
    }

    fn page_with(callback: azul_layout::callbacks::CallbackType) -> Dom {
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_webview(azul_css::AzString::from(PAY_PAGE))
                .with_css("display: block; width: 200px; height: 100px;")
                .with_callback(
                    azul_core::events::EventFilter::Component(
                        azul_core::events::ComponentEventFilter::WebViewNavigationRequested,
                    ),
                    RefAny::new(()),
                    callback as usize,
                ),
        )
    }

    extern "C" fn page_window_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
        page_with(page_signed_in)
    }

    /// The page in `window` navigates to `url`, as its script would; the callbacks run at the
    /// window's next turn.
    fn page_navigates(window: &mut HeadlessWindow, url: &str) {
        let lw = window
            .common
            .layout_window
            .as_mut()
            .expect("a laid-out window has its layout window");
        let id = lw
            .webviews
            .views()
            .first()
            .map(|view| view.id)
            .expect("the window shows the page");
        lw.webviews
            .push_simulated(azul_layout::managers::webview::WebViewReport {
                id,
                request: 0,
                event: azul_core::webview::WebViewEvent::NavigationRequested(
                    azul_core::webview::WebViewNavigation {
                        url: azul_css::AzString::from(url),
                        is_redirect: false,
                    },
                ),
            });
    }

    /// A popup only mirrors its owner's subtree: when a callback of a web view in the popup asks
    /// for a rebuild, the OWNER must be rebuilt, or the popup lays out the old content again.
    /// The event pass does this for a popup's clicks and keys; a web view's report went through
    /// the web view pump, which rebuilt the popup alone - AzDrive's card artwork never showed
    /// VISA once nothing else woke the owner every frame.
    #[test]
    fn a_popups_web_view_callback_asking_for_a_rebuild_rebuilds_its_owner() {
        let mut root = root_with(counting_owner_layout);
        root.regenerate_layout().expect("the owner's first layout");
        let _ = root.common.take_regeneration();
        let (popup, _mailbox) = transient_popup_showing(&root, page_with(page_said_the_brand));
        root.queue_window_create(popup);
        for _ in 0..2 {
            root.pump_children();
            root.pump_once(true);
        }
        let before = OWNER_LAYOUTS.load(Ordering::SeqCst);
        page_navigates(&mut root.children[0], "https://pay.example/_bridge/brand?v=visa");
        for _ in 0..3 {
            root.pump_children();
            root.pump_once(true);
        }
        let navigation = root.children[0]
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.webviews.views().first())
            .and_then(|view| view.navigations.last())
            .map(|n| n.allowed);
        assert_eq!(navigation, Some(false), "the popup's callback heard the page and cancelled");
        assert!(
            OWNER_LAYOUTS.load(Ordering::SeqCst) > before,
            "the owner was rebuilt for its popup's web view callback"
        );
    }

    /// A web view callback's `RefreshDomAllWindows` rebuilds every window of the app, as it does
    /// from an event handler or a timer: the web view pump took it for a rebuild of its own
    /// window only.
    #[test]
    fn a_web_view_callback_answering_refresh_all_windows_rebuilds_the_root() {
        let mut root = root_with(counting_page_owner_layout);
        root.regenerate_layout().expect("the root's first layout");
        let _ = root.common.take_regeneration();
        root.queue_window_create(window(page_window_layout, "sign-in"));
        for _ in 0..2 {
            root.pump_children();
            root.pump_once(true);
        }
        let before = PAGE_OWNER_LAYOUTS.load(Ordering::SeqCst);
        page_navigates(&mut root.children[0], "https://pay.example/_bridge/result?v=succeeded");
        for _ in 0..3 {
            root.pump_children();
            root.pump_once(true);
        }
        assert!(
            PAGE_OWNER_LAYOUTS.load(Ordering::SeqCst) > before,
            "the root was rebuilt for the web view's RefreshDomAllWindows"
        );
    }

    /// A window the app opened as OWNED (an Options window) closes when its owner closes, as
    /// on every desktop: headless windows have no platform id, so an owned window named no
    /// owner and stayed open after the window that opened it was gone.
    #[test]
    fn an_owned_window_closes_with_the_window_that_opened_it() {
        let mut root = root();
        root.queue_window_create(window(root_layout, "editor"));
        root.pump_children();
        let mut options = window(root_layout, "options");
        options.owned = true;
        root.children[0].queue_window_create(options);
        root.pump_children();
        root.pump_children();
        let mut ids = child_ids(&root);
        ids.sort();
        assert_eq!(ids, vec!["editor".to_string(), "options".to_string()]);

        let editor = root
            .children
            .iter_mut()
            .find(|c| c.common.current_window_state().window_id.as_str() == "editor")
            .unwrap();
        editor.close();
        for _ in 0..3 {
            root.pump_children();
        }
        assert!(child_ids(&root).is_empty(), "the owned window closed with its owner");
    }

    /// The app runs until its last window closes: a window the root opened without owning it
    /// (AzMail's compose window) stays when the root closes, and the root's owned windows and
    /// menus close with it. Headless ended the app with its first window.
    #[test]
    fn a_window_the_root_does_not_own_outlives_it() {
        let mut root = root();
        root.queue_window_create(window(root_layout, "compose"));
        let mut options = window(root_layout, "options");
        options.owned = true;
        root.queue_window_create(options);
        let mut menu = window(root_layout, "azul-menu");
        menu.window_state.flags.window_type = azul_core::window::WindowType::Menu;
        root.queue_window_create(menu);
        root.pump_children();
        assert_eq!(child_ids(&root).len(), 3);

        root.close();
        assert!(root.hand_over_from_closed_root(), "the compose window is left");
        for _ in 0..3 {
            root.pump_children();
        }
        assert_eq!(child_ids(&root), vec!["compose".to_string()]);

        root.children[0].close();
        root.pump_children();
        assert!(root.children.is_empty());
        assert!(!root.hand_over_from_closed_root(), "no window is left");
    }
}

