//! Window state types for azul-layout
//!
//! These types are defined here (rather than azul-core) because CallbackInfo
//! needs to reference them, and CallbackInfo must live in azul-layout (since
//! it requires LayoutWindow).

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallback,
    dom::DomId,
    window::{
        DebugState, ImePosition, KeyboardState, Monitor, MouseState, PlatformSpecificOptions,
        RendererOptions, TouchState, WindowFlags, WindowPosition, WindowSize, WindowTheme,
    },
};
use azul_css::{
    corety::OptionU32, impl_option, impl_option_inner, impl_vec, impl_vec_clone, impl_vec_debug,
    impl_vec_mut, impl_vec_partialeq, props::basic::OptionColorU, AzString,
};

use crate::callbacks::OptionCallback;

/// Options for creating a new window
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct WindowCreateOptions {
    /// Initial state for the new window
    pub window_state: FullWindowState,
    /// Optional callback invoked after the window is created
    pub create_callback: OptionCallback,
    /// Optional renderer configuration (e.g., `VSync`, SRGB)
    pub renderer: azul_core::window::OptionRendererOptions,
    /// Optional window theme override (light/dark)
    pub theme: azul_core::window::OptionWindowTheme,
    /// If true, the window is resized to fit its content after the first layout
    pub size_to_content: bool,
    /// If true, enables hot-reloading of CSS and resources
    pub hot_reload: bool,
    /// Parent window's platform id (the window-registry key: X Window id on X11,
    /// `wl_surface` ptr on Wayland, HWND on Windows, `NSWindow` ptr on macOS), or 0
    /// for a top-level window with no parent. Child windows (menus, dropdowns,
    /// dialogs) set this so the backend can position them relative to the parent
    /// and, on X11, reuse the parent's display connection for the single shared
    /// event pump. 0 = no parent.
    pub parent_window_id: u64,
}

impl Default for WindowCreateOptions {
    fn default() -> Self {
        Self {
            window_state: FullWindowState::default(),
            create_callback: OptionCallback::None,
            renderer: azul_core::window::OptionRendererOptions::None,
            theme: azul_core::window::OptionWindowTheme::None,
            size_to_content: false,
            hot_reload: false,
            parent_window_id: 0,
        }
    }
}

impl WindowCreateOptions {
    /// Create a new `WindowCreateOptions` with a layout callback
    pub fn create(layout_callback: impl Into<LayoutCallback>) -> Self {
        let mut options = Self::default();
        options.window_state.layout_callback = layout_callback.into();
        options
    }
}

impl_option!(
    WindowCreateOptions,
    OptionWindowCreateOptions,
    copy = false,
    [Debug, Clone, PartialEq]
);
impl_vec!(
    WindowCreateOptions,
    WindowCreateOptionsVec,
    WindowCreateOptionsVecDestructor,
    WindowCreateOptionsVecDestructorType,
    WindowCreateOptionsVecSlice,
    OptionWindowCreateOptions
);
impl_vec_clone!(
    WindowCreateOptions,
    WindowCreateOptionsVec,
    WindowCreateOptionsVecDestructor
);
impl_vec_partialeq!(WindowCreateOptions, WindowCreateOptionsVec);
impl_vec_debug!(WindowCreateOptions, WindowCreateOptionsVec);
impl_vec_mut!(WindowCreateOptions, WindowCreateOptionsVec);

/// Full window state including internal fields not exposed to callbacks
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct FullWindowState {
    /// Platform-specific window options
    pub platform_specific_options: PlatformSpecificOptions,
    /// Current keyboard state (pressed keys, modifiers)
    pub keyboard_state: KeyboardState,
    /// Semantic window identifier for multi-window debugging.
    /// Can be set by the user to identify specific windows (e.g., "main", "settings", "popup-1")
    pub window_id: AzString,
    /// Window title bar text
    pub title: AzString,
    /// Optional callback invoked when the user requests the window to close
    pub close_callback: OptionCallback,
    /// Callback that returns the DOM for this window
    pub layout_callback: LayoutCallback,
    /// Window position on screen
    pub position: WindowPosition,
    /// Current touch/gesture input state
    pub touch_state: TouchState,
    /// Window dimensions (logical and physical)
    pub size: WindowSize,
    /// Window flags (minimized, maximized, fullscreen, etc.)
    pub flags: WindowFlags,
    /// Current mouse cursor state (position, buttons)
    pub mouse_state: MouseState,
    /// Active window theme (light/dark)
    pub theme: WindowTheme,
    /// Position of the IME candidate window
    pub ime_position: ImePosition,
    /// GPU renderer options (`VSync`, SRGB, hardware acceleration)
    pub renderer_options: RendererOptions,
    /// Monitor ID (not the full Monitor struct - just the identifier)
    pub monitor_id: OptionU32,
    /// Debug visualization state (layout borders, repaints, etc.)
    pub debug_state: DebugState,
    /// Window background color. If None, uses system window background color.
    pub background_color: OptionColorU,
    /// Whether this window currently has input focus
    pub window_focused: bool,
    /// Active route match (pattern + extracted parameters).
    /// Set by `CallbackInfo::switch_route()` or by the web server on URL match.
    /// Layout callbacks read this via `LayoutCallbackInfo::get_route_param()`.
    pub active_route: azul_core::resources::OptionRouteMatch,
    /// Every pointer seat OTHER than the primary (9b-ii), sorted by `seat_id`.
    /// APPENDED for ABI stability.
    ///
    /// `mouse_state` above IS the primary seat, and is deliberately not
    /// duplicated in here: two copies of one cursor are a desync waiting to
    /// happen, and every existing reader of "the mouse" keeps reading the
    /// primary. Empty on any platform that presents one cursor - Windows,
    /// macOS, Android, iOS - which is why nothing there changes. See
    /// [`azul_core::window::PointerSeat`] for why the key is a SEAT and not a
    /// device. Read and write through [`Self::pointer_seat`] /
    /// [`Self::pointer_seat_mut`], which fold the primary in.
    pub pointer_seats: azul_core::window::PointerSeatVec,
    /// The NON-primary seats' keyboards (9b-ii-a-i), the per-seat twin of
    /// `pointer_seats`; `keyboard_state` is the primary seat's and is never
    /// duplicated here. Read through [`Self::keyboard_seat`].
    pub keyboard_seats: azul_core::window::KeyboardSeatVec,
}

impl FullWindowState {
    /// The state of pointer seat `seat_id`: the primary for
    /// [`PRIMARY_POINTER_SEAT`](azul_core::window::PRIMARY_POINTER_SEAT),
    /// otherwise the matching entry of `pointer_seats`, or `None` for a seat
    /// this window has never seen.
    #[must_use]
    pub fn pointer_seat(&self, seat_id: u64) -> Option<&MouseState> {
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            return Some(&self.mouse_state);
        }
        self.pointer_seats
            .as_ref()
            .iter()
            .find(|s| s.seat_id == seat_id)
            .map(|s| &s.state)
    }

    /// Mutable access to seat `seat_id`, CREATING it (default state) when it
    /// is new - the platform handlers call this the moment a second master
    /// pointer moves, and "first seen" is the only registration a seat gets.
    /// Keeps `pointer_seats` sorted so equality and the event diff are
    /// order-independent.
    pub fn pointer_seat_mut(&mut self, seat_id: u64) -> &mut MouseState {
        use azul_core::window::{PointerSeat, PointerSeatVec};
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            return &mut self.mouse_state;
        }
        let idx = match self
            .pointer_seats
            .as_ref()
            .binary_search_by_key(&seat_id, |s| s.seat_id)
        {
            Ok(i) => i,
            Err(i) => {
                let mut v = self.pointer_seats.clone().into_library_owned_vec();
                v.insert(
                    i,
                    PointerSeat {
                        seat_id,
                        state: MouseState::default(),
                    },
                );
                self.pointer_seats = PointerSeatVec::from_vec(v);
                i
            }
        };
        &mut self.pointer_seats.as_mut()[idx].state
    }

    /// Forget a non-primary seat (the master pointer was removed, the
    /// `wl_seat` went away). The event diff then releases whatever it held,
    /// because a seat present in the previous state and absent in the current
    /// one is diffed against a default state. The primary cannot be removed;
    /// returns whether anything went.
    pub fn remove_pointer_seat(&mut self, seat_id: u64) -> bool {
        use azul_core::window::PointerSeatVec;
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            return false;
        }
        let before = self.pointer_seats.len();
        if !self.pointer_seats.as_ref().iter().any(|s| s.seat_id == seat_id) {
            return false;
        }
        let mut v = self.pointer_seats.clone().into_library_owned_vec();
        v.retain(|s| s.seat_id != seat_id);
        self.pointer_seats = PointerSeatVec::from_vec(v);
        self.pointer_seats.len() != before
    }

    /// Every seat, primary first.
    pub fn pointer_seats_with_primary(&self) -> impl Iterator<Item = (u64, &MouseState)> {
        core::iter::once((azul_core::window::PRIMARY_POINTER_SEAT, &self.mouse_state)).chain(
            self.pointer_seats
                .as_ref()
                .iter()
                .map(|s| (s.seat_id, &s.state)),
        )
    }

    /// One seat's keyboard (9b-ii-a-i): the primary's own for seat 0.
    #[must_use]
    pub fn keyboard_seat(&self, seat_id: u64) -> Option<&azul_core::window::KeyboardState> {
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            return Some(&self.keyboard_state);
        }
        self.keyboard_seats
            .as_ref()
            .iter()
            .find(|s| s.seat_id == seat_id)
            .map(|s| &s.state)
    }

    /// One seat's keyboard for writing, created (sorted by id) on first use.
    pub fn keyboard_seat_mut(&mut self, seat_id: u64) -> &mut azul_core::window::KeyboardState {
        use azul_core::window::{KeyboardSeat, KeyboardSeatVec, KeyboardState};
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            return &mut self.keyboard_state;
        }
        let idx = match self
            .keyboard_seats
            .as_ref()
            .binary_search_by_key(&seat_id, |s| s.seat_id)
        {
            Ok(i) => i,
            Err(i) => {
                let mut v = self.keyboard_seats.clone().into_library_owned_vec();
                v.insert(
                    i,
                    KeyboardSeat {
                        seat_id,
                        state: KeyboardState::default(),
                    },
                );
                self.keyboard_seats = KeyboardSeatVec::from_vec(v);
                i
            }
        };
        &mut self.keyboard_seats.as_mut()[idx].state
    }

    /// Drop a seat's keyboard (the seat went away). Never the primary's.
    pub fn remove_keyboard_seat(&mut self, seat_id: u64) -> bool {
        use azul_core::window::KeyboardSeatVec;
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            return false;
        }
        if !self.keyboard_seats.as_ref().iter().any(|s| s.seat_id == seat_id) {
            return false;
        }
        let mut v = self.keyboard_seats.clone().into_library_owned_vec();
        v.retain(|s| s.seat_id != seat_id);
        self.keyboard_seats = KeyboardSeatVec::from_vec(v);
        true
    }

    /// Every keyboard, the primary's first.
    pub fn keyboard_seats_with_primary(
        &self,
    ) -> impl Iterator<Item = (u64, &azul_core::window::KeyboardState)> {
        core::iter::once((azul_core::window::PRIMARY_POINTER_SEAT, &self.keyboard_state)).chain(
            self.keyboard_seats
                .as_ref()
                .iter()
                .map(|s| (s.seat_id, &s.state)),
        )
    }
}

impl_option!(
    FullWindowState,
    OptionFullWindowState,
    copy = false,
    [Debug, Clone, PartialEq]
);

// C-ABI vec so that `CallbackInfo::queue_window_state_sequence()` can be
// exposed over FFI (a `Vec<FullWindowState>` is not repr(C)).
impl_vec!(
    FullWindowState,
    FullWindowStateVec,
    FullWindowStateVecDestructor,
    FullWindowStateVecDestructorType,
    FullWindowStateVecSlice,
    OptionFullWindowState
);
impl_vec_clone!(
    FullWindowState,
    FullWindowStateVec,
    FullWindowStateVecDestructor
);
impl_vec_partialeq!(FullWindowState, FullWindowStateVec);
impl_vec_debug!(FullWindowState, FullWindowStateVec);

impl Default for FullWindowState {
    fn default() -> Self {
        Self {
            platform_specific_options: PlatformSpecificOptions::default(),
            keyboard_state: KeyboardState::default(),
            window_id: AzString::from_const_str("azul-window"),
            title: AzString::from_const_str("Azul Window"),
            close_callback: OptionCallback::None,
            layout_callback: LayoutCallback::default(),
            position: WindowPosition::default(),
            touch_state: TouchState::default(),
            size: WindowSize::default(),
            flags: WindowFlags::default(),
            mouse_state: MouseState::default(),
            pointer_seats: azul_core::window::PointerSeatVec::from_const_slice(&[]),
            keyboard_seats: azul_core::window::KeyboardSeatVec::from_const_slice(&[]),
            theme: WindowTheme::default(),
            ime_position: ImePosition::default(),
            renderer_options: RendererOptions::default(),
            monitor_id: OptionU32::None,
            debug_state: DebugState::default(),
            background_color: OptionColorU::None,
            window_focused: true,
            active_route: azul_core::resources::OptionRouteMatch::None,
        }
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_core::{
        callbacks::{LayoutCallbackInfo, LayoutCallbackType, Update},
        dom::Dom,
        geom::{LogicalSize, PhysicalPositionI32},
        refany::RefAny,
        resources::{OptionRouteMatch, RouteMatch},
        window::{AzStringPair, OptionWindowTheme, StringPairVec},
    };
    use azul_css::props::basic::ColorU;

    use super::*;
    use crate::callbacks::{Callback, CallbackInfo};

    // ------------------------------------------------------------------
    // Pointer seats (9b-ii)
    // ------------------------------------------------------------------

    #[test]
    fn seat_zero_is_the_primary_and_never_an_entry() {
        use azul_core::window::PRIMARY_POINTER_SEAT;
        let mut s = FullWindowState::default();
        s.pointer_seat_mut(PRIMARY_POINTER_SEAT).left_down = true;
        assert!(s.mouse_state.left_down, "seat 0 IS mouse_state");
        assert!(s.pointer_seats.is_empty(), "and is never duplicated into the vec");
        assert!(!s.remove_pointer_seat(PRIMARY_POINTER_SEAT));
        assert_eq!(s.pointer_seat(PRIMARY_POINTER_SEAT).map(|m| m.left_down), Some(true));
    }

    #[test]
    fn seats_are_created_on_first_touch_and_kept_sorted() {
        let mut s = FullWindowState::default();
        assert!(s.pointer_seat(7).is_none());
        s.pointer_seat_mut(7).right_down = true;
        s.pointer_seat_mut(3).left_down = true;
        s.pointer_seat_mut(7).middle_down = true;
        let ids: Vec<u64> = s.pointer_seats.as_ref().iter().map(|p| p.seat_id).collect();
        assert_eq!(ids, vec![3, 7], "sorted, and 7 was not re-created");
        let seven = s.pointer_seat(7).unwrap();
        assert!(seven.right_down && seven.middle_down);
        assert!(!s.mouse_state.right_down, "the primary is untouched");
        let all: Vec<u64> = s.pointer_seats_with_primary().map(|(id, _)| id).collect();
        assert_eq!(all, vec![0, 3, 7]);
    }

    #[test]
    fn a_removed_seat_is_gone_and_equality_sees_it() {
        let mut a = FullWindowState::default();
        a.pointer_seat_mut(5).left_down = true;
        let b = FullWindowState::default();
        assert_ne!(a, b, "a second cursor holding a button is a state delta");
        assert!(a.remove_pointer_seat(5));
        assert!(!a.remove_pointer_seat(5));
        assert_eq!(a, b);
    }

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    // The four layout callbacks below deliberately have DIFFERENT bodies:
    // identical-body `extern "C"` functions can be folded onto a single symbol
    // by the linker (identical code folding), which would silently make two
    // "distinct" callbacks compare equal. Every assertion here is still written
    // so it holds either way — the pointer that goes in is the pointer that
    // must come out — but distinct bodies keep the tests meaningful.

    extern "C" fn cb_alpha(_: RefAny, _: LayoutCallbackInfo) -> Dom {
        Dom::create_body()
    }

    extern "C" fn cb_beta(_: RefAny, _: LayoutCallbackInfo) -> Dom {
        Dom::create_text_do_not_use_without_block_level_wrapper("beta")
    }

    extern "C" fn cb_gamma(_: RefAny, _: LayoutCallbackInfo) -> Dom {
        Dom::create_text_do_not_use_without_block_level_wrapper("gamma-gamma-gamma")
    }

    /// Stored by `create` but never invoked by it — invoking it fails the test.
    extern "C" fn cb_never_called(_: RefAny, _: LayoutCallbackInfo) -> Dom {
        unreachable!("WindowCreateOptions::create must not invoke the layout callback")
    }

    extern "C" fn close_cb(_: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn ptr_of(f: LayoutCallbackType) -> usize {
        f as usize
    }

    fn stored_cb(o: &WindowCreateOptions) -> usize {
        o.window_state.layout_callback.cb as usize
    }

    /// `FullWindowState::default()` with one mutation applied. Written as a
    /// helper so the tests do not trip `field_reassign_with_default`.
    fn state_with(mutate: impl FnOnce(&mut FullWindowState)) -> FullWindowState {
        let mut s = FullWindowState::default();
        mutate(&mut s);
        s
    }

    fn options_with(mutate: impl FnOnce(&mut WindowCreateOptions)) -> WindowCreateOptions {
        let mut o = WindowCreateOptions::default();
        mutate(&mut o);
        o
    }

    fn opts_with_id(id: u64) -> WindowCreateOptions {
        let mut o = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
        o.parent_window_id = id;
        o
    }

    fn ids_of(v: &WindowCreateOptionsVec) -> Vec<u64> {
        v.as_slice().iter().map(|o| o.parent_window_id).collect()
    }

    /// Strings that break naive string handling: empty, interior NUL, astral
    /// planes, ZWJ sequences, RTL, combining marks, bidi/zero-width controls,
    /// fullwidth forms, and a 128 KiB blob.
    fn nasty_strings() -> Vec<String> {
        vec![
            String::new(),
            "\0".to_string(),
            "a\0b\0".to_string(),
            "🦀🐉👨‍👩‍👧‍👦".to_string(),
            "مرحبا بالعالم".to_string(),
            "e\u{0301}\u{0301}\u{0301}".to_string(),
            "\u{200B}\u{FEFF}\u{202E}drowssap".to_string(),
            "ｆｕｌｌｗｉｄｔｈ".to_string(),
            "ß".repeat(65_536),
        ]
    }

    // ------------------------------------------------------------------
    // WindowCreateOptions::create
    // ------------------------------------------------------------------

    #[test]
    fn create_stores_the_callback_and_leaves_every_other_field_at_default() {
        let opts = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
        let def = WindowCreateOptions::default();

        assert_eq!(stored_cb(&opts), ptr_of(cb_alpha));
        assert!(opts.window_state.layout_callback.ctx.is_none());

        assert_eq!(opts.create_callback, def.create_callback);
        assert_eq!(opts.renderer, def.renderer);
        assert_eq!(opts.theme, def.theme);
        assert_eq!(opts.size_to_content, def.size_to_content);
        assert_eq!(opts.hot_reload, def.hot_reload);
        assert_eq!(opts.parent_window_id, def.parent_window_id);
        assert_eq!(opts.window_state.title, def.window_state.title);
        assert_eq!(opts.window_state.window_id, def.window_state.window_id);
        assert_eq!(
            opts.window_state.close_callback,
            def.window_state.close_callback
        );
        assert_eq!(
            opts.window_state.window_focused,
            def.window_state.window_focused
        );
    }

    #[test]
    fn create_records_exactly_the_pointer_it_was_given() {
        let all: [LayoutCallbackType; 4] = [cb_alpha, cb_beta, cb_gamma, cb_never_called];
        for cb in all.iter().copied() {
            let opts = WindowCreateOptions::create(cb);
            assert_eq!(stored_cb(&opts), cb as usize);
        }
    }

    #[test]
    fn create_does_not_invoke_the_stored_callback() {
        // `cb_never_called` panics if it ever runs; reaching the assert proves
        // `create` only *stores* the pointer.
        let opts = WindowCreateOptions::create(cb_never_called as LayoutCallbackType);
        assert_eq!(stored_cb(&opts), ptr_of(cb_never_called));
    }

    #[test]
    fn create_with_the_default_callback_equals_default_options() {
        let opts = WindowCreateOptions::create(LayoutCallback::default());
        assert_eq!(opts, WindowCreateOptions::default());
    }

    #[test]
    fn create_accepts_a_layout_callback_value_and_preserves_its_ctx() {
        // The `impl Into<LayoutCallback>` arg may already carry an FFI ctx
        // payload; `create` must not drop it.
        let cb = LayoutCallback {
            cb: cb_beta,
            ctx: Some(RefAny::new(vec![0xAAu8; 4096])).into(),
        };
        let opts = WindowCreateOptions::create(cb);
        assert_eq!(stored_cb(&opts), ptr_of(cb_beta));
        assert!(opts.window_state.layout_callback.ctx.is_some());

        // Same, routed through `LayoutCallback::create`.
        let opts =
            WindowCreateOptions::create(LayoutCallback::create(cb_gamma as LayoutCallbackType));
        assert_eq!(stored_cb(&opts), ptr_of(cb_gamma));
        assert!(opts.window_state.layout_callback.ctx.is_none());
    }

    #[test]
    fn create_is_deterministic_and_independent_across_calls() {
        let first = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
        for _ in 0..500 {
            let next = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
            assert_eq!(next, first);
        }

        // Mutating one result must not disturb a later one.
        let mut a = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
        a.parent_window_id = u64::MAX;
        a.window_state.title = AzString::from("mutated");
        let b = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
        assert_eq!(b, first);
        assert_ne!(a, b);
    }

    // ------------------------------------------------------------------
    // Default invariants
    // ------------------------------------------------------------------

    #[test]
    fn full_window_state_default_invariants() {
        let s = FullWindowState::default();

        assert_eq!(s.window_id.as_str(), "azul-window");
        assert_eq!(s.title.as_str(), "Azul Window");
        assert!(s.window_focused);
        assert!(s.monitor_id.is_none());
        assert!(s.background_color.is_none());
        assert!(s.active_route.is_none());
        assert_eq!(s.close_callback, OptionCallback::None);
        assert_eq!(s.theme, WindowTheme::default());
        assert_eq!(s.position, WindowPosition::default());
        assert_eq!(s.ime_position, ImePosition::default());

        // Default is a pure value: stable, self-equal, and clone-equal.
        assert_eq!(s, FullWindowState::default());
        assert_eq!(s.clone(), s);
    }

    #[test]
    fn window_create_options_default_invariants() {
        let o = WindowCreateOptions::default();

        assert_eq!(o.parent_window_id, 0, "0 means 'no parent window'");
        assert!(!o.size_to_content);
        assert!(!o.hot_reload);
        assert!(o.renderer.is_none());
        assert!(o.theme.is_none());
        assert_eq!(o.create_callback, OptionCallback::None);
        assert_eq!(o.window_state, FullWindowState::default());

        assert_eq!(o, WindowCreateOptions::default());
        assert_eq!(o.clone(), o);
    }

    // ------------------------------------------------------------------
    // Equality / predicate invariants
    // ------------------------------------------------------------------

    #[test]
    fn mutating_any_scalar_field_breaks_equality() {
        let base = WindowCreateOptions::default();

        assert_ne!(options_with(|o| o.parent_window_id = u64::MAX), base);
        assert_ne!(options_with(|o| o.parent_window_id = 1), base);
        assert_ne!(options_with(|o| o.size_to_content = true), base);
        assert_ne!(options_with(|o| o.hot_reload = true), base);
        assert_ne!(
            options_with(|o| o.theme = OptionWindowTheme::Some(WindowTheme::DarkMode)),
            base
        );
        assert_ne!(
            options_with(|o| o.create_callback =
                OptionCallback::Some(Callback::from(close_cb as crate::callbacks::CallbackType))),
            base
        );
        assert_ne!(
            options_with(|o| o.window_state.window_focused = false),
            base
        );
        assert_ne!(
            options_with(|o| o.window_state.monitor_id = OptionU32::Some(u32::MAX)),
            base
        );
        assert_ne!(
            options_with(|o| o.window_state.monitor_id = OptionU32::Some(0)),
            base
        );
        assert_ne!(
            options_with(
                |o| o.window_state.background_color = OptionColorU::Some(ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 0,
                })
            ),
            base
        );
        assert_ne!(
            options_with(|o| o.window_state.title = AzString::from("")),
            base
        );
    }

    #[test]
    #[allow(clippy::eq_op)] // `a == a` IS the reflexivity check being asserted
    fn equality_is_reflexive_symmetric_and_transitive_for_extreme_values() {
        let a = options_with(|o| {
            o.parent_window_id = u64::MAX;
            o.window_state.title = AzString::from("🦀\0🦀");
            o.window_state.size.dimensions = LogicalSize::new(f32::NAN, -0.0);
        });
        let b = a.clone();
        let c = b.clone();

        assert_eq!(a, a); // reflexive, even with a NaN inside
        assert_eq!(a, b);
        assert_eq!(b, a); // symmetric
        assert_eq!(b, c);
        assert_eq!(a, c); // transitive
        assert_ne!(a, WindowCreateOptions::default());
    }

    #[test]
    fn callback_ctx_is_deliberately_not_part_of_equality() {
        // `impl_callback!` compares callbacks by function pointer only, so two
        // otherwise-identical states that differ ONLY in the FFI ctx payload
        // compare equal. Pinned so a change to that rule is loud.
        let a = WindowCreateOptions::create(cb_alpha as LayoutCallbackType);
        let mut b = a.clone();
        b.window_state.layout_callback.ctx = Some(RefAny::new(0xDEAD_BEEF_u32)).into();

        assert!(a.window_state.layout_callback.ctx.is_none());
        assert!(b.window_state.layout_callback.ctx.is_some());
        assert_eq!(a, b);
    }

    // ------------------------------------------------------------------
    // Numeric limits / NaN / saturation
    // ------------------------------------------------------------------

    #[test]
    fn nan_dimensions_stay_reflexive_and_do_not_alias_the_origin() {
        let nan = state_with(|s| s.size.dimensions = LogicalSize::new(f32::NAN, f32::NAN));
        let zero = state_with(|s| s.size.dimensions = LogicalSize::new(0.0, 0.0));

        // `LogicalSize`'s PartialEq quantizes, mapping NaN to a fixed sentinel,
        // so equality stays reflexive (unlike raw f32) and NaN != origin.
        assert_eq!(nan, nan.clone());
        assert_ne!(nan, zero);
        assert_ne!(nan, FullWindowState::default());
    }

    #[test]
    fn out_of_range_dimensions_saturate_rather_than_wrap() {
        // Quantization multiplies by 1000 and saturates the f32->i64 cast, so
        // every coordinate at or beyond the overflow point collapses onto the
        // same bound. Pinned as behaviour, not as a claim that it is ideal.
        let inf = state_with(|s| {
            s.size.dimensions = LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY);
        });
        let max = state_with(|s| s.size.dimensions = LogicalSize::new(f32::MAX, -f32::MAX));

        assert_eq!(inf, inf.clone());
        assert_eq!(max, max.clone());
        assert_eq!(
            inf, max,
            "f32::MAX * 1000 overflows to inf, so both saturate"
        );

        // Tiny magnitudes quantize to 0 and are therefore indistinguishable
        // from the origin — no panic, no wraparound.
        let tiny = state_with(|s| {
            s.size.dimensions = LogicalSize::new(f32::MIN_POSITIVE, -f32::MIN_POSITIVE);
        });
        assert_eq!(
            tiny,
            state_with(|s| s.size.dimensions = LogicalSize::new(0.0, -0.0))
        );
    }

    #[test]
    fn integer_window_state_limits_survive_clone_and_containers() {
        let extreme = state_with(|s| {
            s.size.dpi = 0; // platform reported no DPI at all
            s.size.min_dimensions = Some(LogicalSize::new(-0.0, 0.0)).into();
            s.size.max_dimensions = Some(LogicalSize::new(f32::INFINITY, f32::INFINITY)).into();
            s.monitor_id = OptionU32::Some(u32::MAX);
            s.position = WindowPosition::Initialized(PhysicalPositionI32 {
                x: i32::MIN,
                y: i32::MAX,
            });
            s.background_color = OptionColorU::Some(ColorU {
                r: u8::MAX,
                g: 0,
                b: u8::MAX,
                a: 0,
            });
        });

        let cloned = extreme.clone();
        assert_eq!(cloned, extreme);
        assert_eq!(cloned.size.dpi, 0);
        assert_eq!(cloned.monitor_id.into_option(), Some(u32::MAX));
        assert_eq!(
            cloned.position,
            WindowPosition::Initialized(PhysicalPositionI32 {
                x: i32::MIN,
                y: i32::MAX,
            })
        );

        let round_tripped: Option<FullWindowState> = OptionFullWindowState::Some(cloned).into();
        assert_eq!(round_tripped, Some(extreme.clone()));

        let v = FullWindowStateVec::from_vec(vec![extreme.clone()]);
        assert_eq!(v.as_slice()[0], extreme);
    }

    #[test]
    fn parent_window_id_boundaries_survive_every_round_trip() {
        let ids: [u64; 5] = [0, 1, u64::MAX / 2, u64::MAX - 1, u64::MAX];
        for id in ids.iter().copied() {
            let o = opts_with_id(id);
            assert_eq!(o.parent_window_id, id);
            assert_eq!(o.clone().parent_window_id, id);

            let opt: OptionWindowCreateOptions = Some(o.clone()).into();
            assert_eq!(opt.as_ref().map(|x| x.parent_window_id), Some(id));

            let v = WindowCreateOptionsVec::from_vec(vec![o.clone()]);
            assert_eq!(v.as_slice()[0].parent_window_id, id);

            let out: Vec<WindowCreateOptions> = v.into();
            assert_eq!(out[0], o);
        }
    }

    // ------------------------------------------------------------------
    // Unicode / huge strings
    // ------------------------------------------------------------------

    #[test]
    fn unicode_and_huge_strings_round_trip_through_state_and_containers() {
        for s in nasty_strings() {
            let st = state_with(|w| {
                w.title = AzString::from(s.clone());
                w.window_id = AzString::from(s.clone());
            });

            assert_eq!(st.title.as_str(), s.as_str());
            assert_eq!(st.title.as_str().len(), s.len(), "byte length preserved");
            assert_eq!(st.window_id.as_str(), s.as_str());

            let cloned = st.clone();
            assert_eq!(cloned, st);
            assert_eq!(cloned.title.as_str(), s.as_str());

            // encode == decode through the FFI Option wrapper
            let opt: OptionFullWindowState = Some(st.clone()).into();
            let back: Option<FullWindowState> = opt.into();
            assert_eq!(back.as_ref().map(|x| x.title.as_str()), Some(s.as_str()));

            // ... and through the FFI Vec wrapper
            let v = FullWindowStateVec::from_vec(vec![st.clone(), st.clone()]);
            assert_eq!(v.len(), 2);
            assert_eq!(v.as_slice()[1].title.as_str(), s.as_str());
            assert_eq!(v.clone(), v);

            let out: Vec<FullWindowState> = v.into_library_owned_vec();
            assert_eq!(out.len(), 2);
            assert_eq!(out[0].title.as_str(), s.as_str());
            assert_eq!(out[0], st);
        }
    }

    #[test]
    fn debug_rendering_survives_nasty_strings() {
        for s in nasty_strings().into_iter().filter(|s| s.len() < 4096) {
            let o = options_with(|w| {
                w.window_state.title = AzString::from(s.clone());
                w.parent_window_id = u64::MAX;
            });
            let rendered = format!("{o:?}");
            assert!(rendered.starts_with("WindowCreateOptions"));
            assert!(rendered.contains("18446744073709551615"));
        }
    }

    #[test]
    fn active_route_round_trips_with_unicode_pattern_and_params() {
        let route = RouteMatch {
            pattern: AzString::from("/user/:id/🦀"),
            params: StringPairVec::from_vec(vec![AzStringPair {
                key: AzString::from("id"),
                value: AzString::from("𝟜𝟚"),
            }]),
        };
        let st = state_with(|s| s.active_route = OptionRouteMatch::Some(route.clone()));

        assert!(st.active_route.is_some());
        assert_ne!(st, FullWindowState::default());

        let opt: OptionFullWindowState = Some(st.clone()).into();
        let back = opt.into_option().expect("just constructed as Some");
        assert_eq!(back, st);
        assert_eq!(
            back.active_route.as_ref().map(|r| r.pattern.as_str()),
            Some("/user/:id/🦀")
        );
        assert_eq!(
            back.active_route
                .as_ref()
                .and_then(|r| r.get_param("id"))
                .map(|v| v.as_str()),
            Some("𝟜𝟚")
        );
        assert!(back
            .active_route
            .as_ref()
            .and_then(|r| r.get_param("missing"))
            .is_none());
    }

    // ------------------------------------------------------------------
    // OptionWindowCreateOptions / OptionFullWindowState
    // ------------------------------------------------------------------

    #[test]
    fn option_wrappers_agree_with_std_option() {
        let opts = WindowCreateOptions::create(cb_beta as LayoutCallbackType);

        let none: OptionWindowCreateOptions = Option::<WindowCreateOptions>::None.into();
        assert!(none.is_none());
        assert!(!none.is_some());
        assert!(none.as_ref().is_none());
        assert!(none.as_option().is_none());
        assert_eq!(none, OptionWindowCreateOptions::default());
        let back: Option<WindowCreateOptions> = none.into();
        assert_eq!(back, None);

        let some: OptionWindowCreateOptions = Some(opts.clone()).into();
        assert!(some.is_some());
        assert!(!some.is_none());
        assert_eq!(some.as_ref(), Some(&opts));
        assert_eq!(some.into_option(), Some(opts.clone()));
        assert_eq!(some.clone().map(|o| o.parent_window_id), Some(0));
        assert_eq!(
            some.clone()
                .and_then(|o| o.window_state.title.as_str().chars().next()),
            Some('A')
        );
        let back: Option<WindowCreateOptions> = some.into();
        assert_eq!(back, Some(opts));
    }

    #[test]
    fn option_replace_returns_the_previous_value() {
        let a = opts_with_id(1);
        let b = opts_with_id(2);

        let mut o = OptionWindowCreateOptions::None;
        assert!(o.replace(a.clone()).is_none());
        assert_eq!(o.as_ref(), Some(&a));

        let prev = o.replace(b.clone());
        assert_eq!(prev.as_ref(), Some(&a));
        assert_eq!(o.as_ref(), Some(&b));

        if let Some(inner) = o.as_mut() {
            inner.parent_window_id = u64::MAX;
        }
        assert_eq!(o.as_ref().map(|x| x.parent_window_id), Some(u64::MAX));
        // the value handed back by `replace` is independent of `o`
        assert_eq!(prev.as_ref().map(|x| x.parent_window_id), Some(1));
    }

    #[test]
    fn option_into_option_hands_back_an_independent_clone() {
        let mut o: OptionFullWindowState = Some(FullWindowState::default()).into();

        let mut taken = o.into_option().expect("constructed as Some");
        taken.title = AzString::from("changed");
        assert_eq!(
            o.as_ref().map(|s| s.title.as_str()),
            Some("Azul Window"),
            "into_option must clone, not alias"
        );

        if let Some(inner) = o.as_mut() {
            inner.title = AzString::from("also changed");
        }
        assert_eq!(taken.title.as_str(), "changed");
    }

    // ------------------------------------------------------------------
    // WindowCreateOptionsVec
    // ------------------------------------------------------------------

    #[test]
    fn vec_round_trips_empty_single_and_large() {
        for n in [0usize, 1, 2, 1000].iter().copied() {
            let src: Vec<WindowCreateOptions> = (0..n as u64).map(opts_with_id).collect();
            let v = WindowCreateOptionsVec::from_vec(src.clone());

            assert_eq!(v.len(), n);
            assert_eq!(v.is_empty(), n == 0);
            assert!(v.capacity() >= v.len());
            assert_eq!(v.as_slice(), src.as_slice());
            assert_eq!(v.iter().count(), n);
            assert_eq!(v.as_c_slice().len(), n);
            assert_eq!(v.as_c_slice().as_slice(), src.as_slice());

            let out: Vec<WindowCreateOptions> = v.into_library_owned_vec();
            assert_eq!(out, src, "encode == decode");
        }
    }

    #[test]
    fn vec_get_is_none_out_of_bounds() {
        let v = WindowCreateOptionsVec::from_vec(vec![opts_with_id(7)]);

        assert_eq!(v.get(0).map(|o| o.parent_window_id), Some(7));
        assert!(v.get(1).is_none());
        assert!(v.get(usize::MAX).is_none());
        assert!(v.c_get(1).is_none());
        assert!(v.c_get(usize::MAX).is_none());
        assert_eq!(
            v.c_get(0).into_option().map(|o| o.parent_window_id),
            Some(7)
        );

        let empty = WindowCreateOptionsVec::new();
        assert!(empty.is_empty());
        assert!(empty.get(0).is_none());
        assert!(empty.get(usize::MAX).is_none());
        assert!(empty.c_get(0).is_none());
        assert_eq!(empty.as_slice(), &[] as &[WindowCreateOptions]);
    }

    #[test]
    fn vec_c_slice_range_clamps_instead_of_panicking() {
        let v = WindowCreateOptionsVec::from_vec((0..4u64).map(opts_with_id).collect());

        assert_eq!(v.as_c_slice_range(0, 4).len(), 4);
        assert_eq!(v.as_c_slice_range(1, 3).len(), 2);
        assert_eq!(v.as_c_slice_range(1, 3).as_slice()[0].parent_window_id, 1);
        assert_eq!(v.as_c_slice_range(3, 1).len(), 0, "inverted range -> empty");
        assert_eq!(v.as_c_slice_range(4, 4).len(), 0);
        assert_eq!(
            v.as_c_slice_range(0, usize::MAX).len(),
            4,
            "end clamps to len"
        );
        assert_eq!(v.as_c_slice_range(2, usize::MAX).as_slice().len(), 2);
        assert_eq!(v.as_c_slice_range(usize::MAX, usize::MAX).len(), 0);
        assert_eq!(v.as_c_slice_range(usize::MAX, 0).len(), 0);

        let empty = WindowCreateOptionsVec::new();
        assert!(empty.as_c_slice_range(0, usize::MAX).is_empty());
        assert!(empty.as_c_slice_range(usize::MAX, usize::MAX).is_empty());
        assert!(empty.as_c_slice().as_slice().is_empty());
    }

    #[test]
    fn vec_insert_and_remove_are_noops_out_of_bounds() {
        let mut v = WindowCreateOptionsVec::from_vec(vec![opts_with_id(1), opts_with_id(2)]);

        v.insert(3, opts_with_id(99)); // index > len
        assert_eq!(ids_of(&v), vec![1, 2]);
        v.insert(usize::MAX, opts_with_id(99));
        assert_eq!(ids_of(&v), vec![1, 2]);

        v.remove(2); // index == len
        assert_eq!(ids_of(&v), vec![1, 2]);
        v.remove(usize::MAX);
        assert_eq!(ids_of(&v), vec![1, 2]);

        v.insert(0, opts_with_id(0)); // prepend
        v.insert(3, opts_with_id(3)); // index == len -> append
        assert_eq!(ids_of(&v), vec![0, 1, 2, 3]);

        v.remove(0);
        assert_eq!(ids_of(&v), vec![1, 2, 3]);
        v.remove(2);
        assert_eq!(ids_of(&v), vec![1, 2]);
    }

    #[test]
    fn vec_push_pop_truncate_retain_keep_len_and_capacity_consistent() {
        let mut v = WindowCreateOptionsVec::new();
        assert!(v.pop().is_none(), "pop on an empty vec must not panic");

        for i in 0..256u64 {
            v.push(opts_with_id(i));
            assert_eq!(v.len(), i as usize + 1);
            assert!(v.capacity() >= v.len());
        }

        assert_eq!(v.get(255).map(|o| o.parent_window_id), Some(255));
        assert_eq!(v.pop().map(|o| o.parent_window_id), Some(255));
        assert_eq!(v.len(), 255);

        v.truncate(1000); // larger than len -> no-op
        assert_eq!(v.len(), 255);

        v.retain(|o| o.parent_window_id % 2 == 0);
        assert_eq!(v.len(), 128);
        assert!(v.as_slice().iter().all(|o| o.parent_window_id % 2 == 0));

        v.truncate(0);
        assert!(v.is_empty());
        assert!(v.pop().is_none());
        assert_eq!(v.as_slice(), &[] as &[WindowCreateOptions]);
    }

    #[test]
    fn vec_append_moves_everything_and_empties_the_source() {
        let mut a = WindowCreateOptionsVec::from_vec(vec![opts_with_id(1)]);
        let mut b = WindowCreateOptionsVec::from_vec(vec![opts_with_id(2), opts_with_id(3)]);

        a.append(&mut b);
        assert_eq!(ids_of(&a), vec![1, 2, 3]);
        assert!(b.is_empty());

        let mut empty = WindowCreateOptionsVec::new();
        a.append(&mut empty); // appending nothing changes nothing
        assert_eq!(ids_of(&a), vec![1, 2, 3]);

        empty.append(&mut a); // appending into a zero-capacity vec
        assert_eq!(ids_of(&empty), vec![1, 2, 3]);
        assert!(a.is_empty());
    }

    #[test]
    fn vec_clone_is_deep_and_growth_does_not_alias_the_original() {
        let original = WindowCreateOptionsVec::from_vec(vec![opts_with_id(1), opts_with_id(2)]);
        let mut cloned = original.clone();

        assert_eq!(cloned, original);
        assert_ne!(
            cloned.as_ptr(),
            original.as_ptr(),
            "a clone must own a separate buffer"
        );

        for i in 0..64u64 {
            cloned.push(opts_with_id(100 + i));
        }
        assert_eq!(cloned.len(), 66);
        assert_eq!(ids_of(&original), vec![1, 2], "original must be untouched");

        for o in cloned.iter_mut() {
            o.hot_reload = true;
        }
        assert!(!original.as_slice()[0].hot_reload);
    }

    #[test]
    fn vec_from_iterator_extend_and_sort_are_consistent() {
        let mut v: WindowCreateOptionsVec = (0..8u64).rev().map(opts_with_id).collect();
        assert_eq!(v.len(), 8);
        assert_eq!(ids_of(&v), vec![7, 6, 5, 4, 3, 2, 1, 0]);

        v.extend((8..12u64).map(opts_with_id));
        assert_eq!(v.len(), 12);

        v.sort_by(|a, b| a.parent_window_id.cmp(&b.parent_window_id));
        assert_eq!(ids_of(&v), (0..12u64).collect::<Vec<_>>());

        let back: Vec<WindowCreateOptions> = v.into();
        assert_eq!(back.len(), 12);
        let again: WindowCreateOptionsVec = back.into();
        assert_eq!(ids_of(&again), (0..12u64).collect::<Vec<_>>());

        let single = WindowCreateOptionsVec::from_item(opts_with_id(42));
        assert_eq!(ids_of(&single), vec![42]);

        let reserved = WindowCreateOptionsVec::with_capacity(0);
        assert!(reserved.is_empty());
    }

    #[test]
    fn vec_equality_and_debug_are_well_behaved() {
        let a = WindowCreateOptionsVec::from_vec(vec![opts_with_id(1)]);
        let b = WindowCreateOptionsVec::from_vec(vec![opts_with_id(1)]);
        let c = WindowCreateOptionsVec::from_vec(vec![opts_with_id(2)]);
        let longer = WindowCreateOptionsVec::from_vec(vec![opts_with_id(1), opts_with_id(1)]);
        let empty = WindowCreateOptionsVec::new();

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, longer, "length is part of equality");
        assert_ne!(a, empty);
        assert_eq!(empty, WindowCreateOptionsVec::default());
        assert_eq!(format!("{empty:?}"), "[]");
        assert!(format!("{a:?}").starts_with('['));
    }

    // ------------------------------------------------------------------
    // FullWindowStateVec (no mutation API — read-only surface)
    // ------------------------------------------------------------------

    #[test]
    fn full_window_state_vec_round_trips_and_clones_deeply() {
        let extreme = state_with(|s| {
            s.title = AzString::from("t".repeat(4096));
            s.size.dimensions = LogicalSize::new(f32::NAN, f32::INFINITY);
            s.monitor_id = OptionU32::Some(u32::MAX);
            s.window_focused = false;
        });

        let v = FullWindowStateVec::from_vec(vec![FullWindowState::default(), extreme.clone()]);
        assert_eq!(v.len(), 2);
        assert!(!v.is_empty());
        assert!(v.get(2).is_none());
        assert!(v.get(usize::MAX).is_none());
        assert!(v.c_get(2).is_none());
        assert_eq!(v.c_get(1).into_option().as_ref(), Some(&extreme));
        assert_eq!(v.iter().count(), 2);
        assert_eq!(v.as_c_slice_range(1, usize::MAX).len(), 1);

        let cloned = v.clone();
        assert_eq!(cloned, v);
        assert_ne!(cloned.as_ptr(), v.as_ptr());

        let out: Vec<FullWindowState> = cloned.into_library_owned_vec();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], FullWindowState::default());
        assert_eq!(out[1], extreme);
        assert_eq!(v.len(), 2, "original survives the clone being consumed");

        let single = FullWindowStateVec::from_item(extreme.clone());
        assert_eq!(single.len(), 1);
        assert_eq!(single.as_slice()[0], extreme);

        let empty = FullWindowStateVec::new();
        assert!(empty.is_empty());
        assert!(empty.get(0).is_none());
        assert_eq!(empty, FullWindowStateVec::default());
        assert_eq!(format!("{empty:?}"), "[]");
    }

    #[test]
    fn state_vec_survives_a_large_batch_of_extreme_states() {
        let src: Vec<FullWindowState> = (0..512u32)
            .map(|i| {
                state_with(|s| {
                    s.monitor_id = OptionU32::Some(i);
                    s.title = AzString::from(format!("win-{i}-🦀"));
                    s.size.dimensions = LogicalSize::new(i as f32, f32::NAN);
                })
            })
            .collect();

        let v = FullWindowStateVec::from_vec(src.clone());
        assert_eq!(v.len(), 512);
        assert_eq!(v.clone(), v);
        assert_eq!(v.as_slice()[511].title.as_str(), "win-511-🦀");

        let out: Vec<FullWindowState> = v.into_library_owned_vec();
        assert_eq!(out, src, "encode == decode for 512 extreme states");
    }
}
