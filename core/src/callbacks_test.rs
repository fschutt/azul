#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    clippy::unusual_byte_groupings
)]
mod autotest_generated {
    use alloc::string::String;

    use super::*;
    use crate::{
        events::HoverEventFilter,
        resources::{RawImageFormat, RouteMatch},
        window::StringPairVec,
    };

    // ---- helpers -----------------------------------------------------------

    fn s(v: &str) -> AzString {
        AzString::from(String::from(v))
    }

    fn win(width: f32, height: f32, dpi: u32) -> WindowSize {
        WindowSize {
            dimensions: LogicalSize::new(width, height),
            dpi,
            min_dimensions: None.into(),
            max_dimensions: None.into(),
        }
    }

    /// Owns everything a `LayoutCallbackInfoRefData` borrows, so that the raw
    /// pointer `LayoutCallbackInfo` launders to `'static` always points at
    /// live memory for the duration of a test.
    struct Fixture {
        fonts: FcFontCache,
        images: ImageCache,
        style: Arc<SystemStyle>,
        gl: OptionGlContextPtr,
        route: Option<RouteMatch>,
    }

    impl Fixture {
        fn new() -> Self {
            Self {
                fonts: FcFontCache::default(),
                images: ImageCache::default(),
                style: Arc::new(SystemStyle::default()),
                gl: OptionGlContextPtr::None,
                route: None,
            }
        }

        fn with_route(route: RouteMatch) -> Self {
            let mut f = Self::new();
            f.route = Some(route);
            f
        }

        fn ref_data(&self) -> LayoutCallbackInfoRefData<'_> {
            static EN_US: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            let locale = EN_US.get_or_init(|| AzString::from("en-US"));
            static NO_WINDOW_ID: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            LayoutCallbackInfoRefData {
                locale,
                accessed_locale: core::cell::Cell::new(false),
                accessed_text_direction: core::cell::Cell::new(false),
                text_direction: crate::callbacks::TextDirection::LeftToRight,
                image_cache: &self.images,
                gl_context: &self.gl,
                system_fonts: &self.fonts,
                system_style: self.style.clone(),
                active_route: self.route.as_ref(),
                monitors: crate::window::MonitorVec::from_const_slice(&[]),
                safe_area: azul_css::system::SafeAreaInsets::default(),
                global_hotkeys: crate::global_hotkey::GlobalHotkeyInfoVec::from_const_slice(&[]),
                window_id: NO_WINDOW_ID.get_or_init(|| AzString::from("")),
            }
        }
    }

    /// #28 (d): `get_max_monitor_size` returns the LARGEST monitor by area
    /// (the safe "how much could possibly be visible" bound for first
    /// layout) and `None` on an empty snapshot (headless/web).
    #[test]
    fn max_monitor_size_is_largest_by_area_or_none() {
        use azul_css::props::basic::LayoutSize;

        use crate::window::{Monitor, MonitorVec};

        let fixture = Fixture::new();
        let mut rd = fixture.ref_data();
        rd.monitors = MonitorVec::from_vec(Vec::from([
            Monitor {
                size: LayoutSize::new(1920, 1080),
                ..Monitor::default()
            },
            Monitor {
                size: LayoutSize::new(2560, 1440),
                ..Monitor::default()
            },
            Monitor {
                size: LayoutSize::new(800, 600),
                ..Monitor::default()
            },
        ]));
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);
        let max: Option<LayoutSize> = info.get_max_monitor_size().into();
        assert_eq!(max, Some(LayoutSize::new(2560, 1440)));
        assert_eq!(info.get_monitors().len(), 3);

        let rd2 = fixture.ref_data(); // empty snapshot
        let info2 = LayoutCallbackInfo::new(&rd2, WindowSize::default(), DarkLightMode::Light);
        let none: Option<LayoutSize> = info2.get_max_monitor_size().into();
        assert_eq!(none, None);
    }

    /// `/user/:id` with a plain and a non-ASCII parameter.
    fn user_route() -> RouteMatch {
        RouteMatch {
            pattern: s("/user/:id"),
            params: StringPairVec::from_vec(Vec::from([
                AzStringPair {
                    key: s("id"),
                    value: s("42"),
                },
                AzStringPair {
                    key: s("\u{1F600}"),
                    value: s("emoji"),
                },
            ])),
        }
    }

    fn vv_info<'a>(
        fonts: &'a FcFontCache,
        images: &'a ImageCache,
        bounds: HidpiAdjustedBounds,
    ) -> VirtualViewCallbackInfo {
        VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::InitialRender,
            fonts,
            images,
            DarkLightMode::Light,
            crate::window::WindowFrame::Normal,
            bounds,
            // materialized: a window at y=2 covering 100x200 of the document
            LogicalRect::new(
                LogicalPosition::new(1.0, 2.0),
                LogicalSize::new(100.0, 200.0),
            ),
            // virtual_rect: the whole document
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(1000.0, 2000.0)),
            // where the user is looking
            LogicalPosition::new(3.0, 4.0),
        )
    }

    fn bounds_1x1() -> HidpiAdjustedBounds {
        HidpiAdjustedBounds::from_bounds(LayoutSize::new(1, 1), DpiScaleFactor::new(1.0))
    }

    // ---- Update::max_self --------------------------------------------------

    const ALL_UPDATES: [Update; 3] = [
        Update::DoNothing,
        Update::RefreshDom,
        Update::RefreshDomAllWindows,
    ];

    /// `max_self` must be exactly the `Ord`-max of the lattice, for every one
    /// of the 3x3 combinations (this is the whole contract, so check it
    /// exhaustively rather than sampling).
    #[test]
    fn update_max_self_is_exhaustively_ord_max() {
        for a in ALL_UPDATES {
            for b in ALL_UPDATES {
                let mut got = a;
                got.max_self(b);
                assert_eq!(
                    got,
                    core::cmp::max(a, b),
                    "max_self({a:?}, {b:?}) disagrees with Ord::max"
                );
            }
        }
    }

    #[test]
    fn update_max_self_is_idempotent_and_monotone() {
        for a in ALL_UPDATES {
            // idempotent: x.max(x) == x
            let mut got = a;
            got.max_self(a);
            assert_eq!(got, a);

            // absorbing top element: nothing can lower RefreshDomAllWindows
            let mut top = Update::RefreshDomAllWindows;
            top.max_self(a);
            assert_eq!(top, Update::RefreshDomAllWindows);

            // monotone: max_self never decreases self
            let mut m = a;
            m.max_self(Update::DoNothing);
            assert!(m >= a);
        }
    }

    /// Applying the same set of updates in any order must converge to the same
    /// value (commutativity/associativity of the fold), since callbacks fold
    /// their `Update`s in nondeterministic order.
    #[test]
    fn update_max_self_fold_is_order_independent() {
        for a in ALL_UPDATES {
            for b in ALL_UPDATES {
                for c in ALL_UPDATES {
                    let mut fwd = a;
                    fwd.max_self(b);
                    fwd.max_self(c);

                    let mut rev = c;
                    rev.max_self(b);
                    rev.max_self(a);

                    assert_eq!(fwd, rev, "fold of {a:?},{b:?},{c:?} is order-dependent");
                }
            }
        }
    }

    // ---- LayoutCallback / default_layout_callback ---------------------------

    static ALT_LAYOUT_CALLS: AtomicUsize = AtomicUsize::new(0);

    // NOTE: the body must differ from `default_layout_callback`'s, otherwise
    // identical-code-folding may merge the two symbols and the pointer
    // inequality assertion below would compare equal addresses.
    extern "C" fn alt_layout_callback(_: RefAny, _: LayoutCallbackInfo) -> Dom {
        ALT_LAYOUT_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_body()
    }

    #[test]
    fn default_layout_callback_returns_body_and_does_not_panic() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(0.0, 0.0, 0), DarkLightMode::Dark);

        // extreme arg: zero-sized window, zero DPI, empty caches
        let dom = default_layout_callback(RefAny::new(0u32), info);
        assert_eq!(dom, Dom::create_body());
    }

    #[test]
    fn layout_callback_create_stores_the_given_fn_and_null_ctx() {
        let from_default = LayoutCallback::create(default_layout_callback as LayoutCallbackType);
        assert!(
            from_default.ctx.is_none(),
            "native-Rust create() must leave the FFI ctx empty"
        );
        assert_eq!(from_default, LayoutCallback::default());

        // create() must actually store its argument, not silently fall back
        // to the default callback.
        let from_alt = LayoutCallback::create(alt_layout_callback as LayoutCallbackType);
        assert!(from_alt.ctx.is_none());
        assert_ne!(
            from_alt, from_default,
            "create() ignored its argument (or the two fns were ICF-folded)"
        );

        // the stored pointer is callable and is the one we passed in
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let before = ALT_LAYOUT_CALLS.load(AtomicOrdering::SeqCst);
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);
        let _ = (from_alt.cb)(RefAny::new(()), info);
        assert_eq!(ALT_LAYOUT_CALLS.load(AtomicOrdering::SeqCst), before + 1);
    }

    // ---- VirtualViewCallback ------------------------------------------------

    extern "C" fn vv_keep_current_cb(
        _: RefAny,
        info: VirtualViewCallbackInfo,
    ) -> VirtualViewReturn {
        VirtualViewReturn::keep_current(info.materialized, info.virtual_rect)
    }

    #[test]
    fn virtual_view_callback_create_round_trips_through_the_fn_ptr() {
        let cb = VirtualViewCallback::create(vv_keep_current_cb as VirtualViewCallbackType);
        assert!(cb.ctx.is_none());

        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let info = vv_info(&fonts, &images, bounds_1x1());

        let ret = (cb.cb)(RefAny::new(0u8), info);
        assert!(ret.dom.is_none());
        assert_eq!(ret.materialized.size, LogicalSize::new(100.0, 200.0));
        assert_eq!(ret.materialized.origin, LogicalPosition::new(1.0, 2.0));
        assert_eq!(ret.virtual_rect.size, LogicalSize::new(1000.0, 2000.0));
        assert_eq!(ret.virtual_rect.origin, LogicalPosition::zero());
    }

    // ---- VirtualViewCallbackInfo -------------------------------------------

    #[test]
    fn virtual_view_callback_info_new_holds_its_fields() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let bounds =
            HidpiAdjustedBounds::from_bounds(LayoutSize::new(800, 600), DpiScaleFactor::new(2.0));
        let info = vv_info(&fonts, &images, bounds);

        assert_eq!(info.reason, VirtualViewCallbackReason::InitialRender);
        assert_eq!(info.window_mode, DarkLightMode::Light);
        assert_eq!(
            info.get_bounds().get_logical_size(),
            LogicalSize::new(800.0, 600.0)
        );
        assert_eq!(
            info.get_bounds().get_hidpi_factor(),
            DpiScaleFactor::new(2.0)
        );
        assert_eq!(info.materialized.size, LogicalSize::new(100.0, 200.0));

        // the raw pointers must alias the borrows we handed in
        assert!(core::ptr::eq(info.internal_get_system_fonts(), &fonts));
        assert!(core::ptr::eq(info.internal_get_image_cache(), &images));

        // FFI ctx starts empty and the measure hook starts absent
        assert!(info.get_ctx().is_none());
        assert_eq!(
            info.measure_dom(Dom::create_body(), LogicalSize::new(10.0, 10.0)),
            LogicalSize::zero()
        );

        // clone must not disturb any of that
        let cloned = info;
        assert_eq!(cloned.reason, info.reason);
        assert!(core::ptr::eq(cloned.internal_get_system_fonts(), &fonts));
        assert!(cloned.get_ctx().is_none());
    }

    #[test]
    fn virtual_view_callback_info_new_survives_nan_and_infinite_geometry() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let info = VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
            &fonts,
            &images,
            DarkLightMode::Dark,
            crate::window::WindowFrame::Normal,
            HidpiAdjustedBounds::from_bounds(
                LayoutSize::new(isize::MAX, isize::MIN),
                DpiScaleFactor::new(f32::NAN),
            ),
            LogicalRect::new(
                LogicalPosition::new(f32::NEG_INFINITY, f32::MAX),
                LogicalSize::new(f32::NAN, f32::INFINITY),
            ),
            LogicalRect::new(
                LogicalPosition::new(-0.0, f32::EPSILON),
                LogicalSize::new(f32::MIN, 0.0),
            ),
            LogicalPosition::new(-0.0, f32::EPSILON),
        );

        // extreme values are stored verbatim, not silently clamped
        assert!(info.materialized.size.width.is_nan());
        assert!(info.materialized.size.height.is_infinite());
        assert!(
            info.materialized.origin.x.is_infinite()
                && info.materialized.origin.x.is_sign_negative()
        );
        assert_eq!(info.virtual_rect.size.width, f32::MIN);
        assert_eq!(
            info.reason,
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom)
        );

        // and none of the getters panic on that instance
        assert!(info.get_ctx().is_none());
        assert!(info.get_bounds().get_logical_size().width > 0.0);
    }

    #[test]
    fn virtual_view_callback_info_get_ctx_clones_without_double_free() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        // null callable_ptr -> None (the native-Rust path)
        assert!(info.get_ctx().is_none());

        let callable = OptionRefAny::Some(RefAny::new(0xDEAD_BEEF_u32));
        info.set_callable_ptr(&callable);

        // repeated get_ctx() must hand out independent clones; dropping them
        // all must not corrupt the original RefAny's refcount.
        for _ in 0..64 {
            let got = info.get_ctx();
            assert!(got.is_some());
            drop(got);
        }

        let mut got = info.get_ctx();
        match got {
            OptionRefAny::Some(ref mut r) => {
                let inner = r.downcast_ref::<u32>().expect("ctx should hold a u32");
                assert_eq!(*inner, 0xDEAD_BEEF_u32);
            }
            OptionRefAny::None => panic!("callable_ptr was set, get_ctx() returned None"),
        }
        drop(got);

        // the original is still alive and intact after all those clones dropped
        let mut orig = callable;
        match orig {
            OptionRefAny::Some(ref mut r) => {
                assert_eq!(*r.downcast_ref::<u32>().unwrap(), 0xDEAD_BEEF_u32);
            }
            OptionRefAny::None => panic!("original callable was consumed"),
        }
    }

    // ---- measure_dom --------------------------------------------------------

    static MEASURE_CALLS: AtomicUsize = AtomicUsize::new(0);
    /// The `MeasureDomMode` the trampoline saw last (0 = Extent, 1 = ShrinkToFit).
    static LAST_MEASURE_MODE: AtomicUsize = AtomicUsize::new(usize::MAX);
    /// Held by every test that calls the trampoline: they share its statics, and the
    /// test harness runs them in parallel - another test's call between a call and the
    /// read of `LAST_MEASURE_MODE` overwrote the mode it read.
    static MEASURE_HOOK_TESTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// [`MEASURE_HOOK_TESTS`], held for the rest of the calling test (a test that
    /// panicked while holding it poisons nothing the next one needs).
    fn measure_hook_tests() -> std::sync::MutexGuard<'static, ()> {
        MEASURE_HOOK_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Test trampoline. Per the `MeasureDomFn` contract the `Dom` is passed by
    /// pointer and **consumed** (moved out) here.
    extern "C" fn test_measure_dom_fn(
        ctx: *mut c_void,
        dom: *mut Dom,
        available: LogicalSize,
        mode: MeasureDomMode,
    ) -> LogicalSize {
        MEASURE_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        LAST_MEASURE_MODE.store(
            match mode {
                MeasureDomMode::Extent => 0,
                MeasureDomMode::ShrinkToFit => 1,
            },
            AtomicOrdering::SeqCst,
        );
        // SAFETY: `measure_dom` always passes a valid, owned-but-ManuallyDrop
        // Dom; taking it by value here is exactly the documented contract.
        let dom = unsafe { core::ptr::read(dom) };
        drop(dom);
        if !ctx.is_null() {
            // SAFETY: the only caller below passes a `&mut u32`.
            unsafe {
                *ctx.cast::<u32>() = 0xABCD;
            }
        }
        LogicalSize::new(available.width * 2.0, available.height / 2.0)
    }

    #[test]
    fn measure_dom_without_hook_returns_zero_for_every_input() {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let info = vv_info(&fonts, &images, bounds_1x1());

        // zero / negative / NaN / infinite / huge available sizes must all take
        // the null-hook early-out without panicking (and must drop the Dom).
        for available in [
            LogicalSize::zero(),
            LogicalSize::new(-1.0, -1.0),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY),
            LogicalSize::new(f32::MAX, f32::MIN),
            LogicalSize::new(1.0, 1_000_000.0),
        ] {
            assert_eq!(
                info.measure_dom(Dom::create_body(), available),
                LogicalSize::zero()
            );
        }
    }

    #[test]
    fn measure_dom_with_hook_forwards_ctx_and_available_and_consumes_the_dom() {
        let _serial = measure_hook_tests();
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        let mut ctx_val: u32 = 0;
        info.set_measure_dom_fn(
            test_measure_dom_fn,
            core::ptr::from_mut(&mut ctx_val).cast::<c_void>(),
        );

        // NOTE: `>` not `== before + 1` - other tests share this static and
        // run in parallel, so only monotonicity is safe to assert here.
        let before = MEASURE_CALLS.load(AtomicOrdering::SeqCst);
        let out = info.measure_dom(Dom::create_body(), LogicalSize::new(100.0, 40.0));

        assert!(MEASURE_CALLS.load(AtomicOrdering::SeqCst) > before);
        assert_eq!(out, LogicalSize::new(200.0, 20.0));
        assert_eq!(ctx_val, 0xABCD, "measure ctx pointer was not forwarded");

        // the documented virtual-scroll sizing idiom: measure at a huge height
        let natural = info.measure_dom(Dom::create_body(), LogicalSize::new(320.0, 1_000_000.0));
        assert_eq!(natural, LogicalSize::new(640.0, 500_000.0));

        // NaN / infinite constraints reach the hook unmodified and come back
        // as NaN/inf rather than panicking or being clamped
        let nan = info.measure_dom(Dom::create_body(), LogicalSize::new(f32::NAN, 4.0));
        assert!(nan.width.is_nan());
        assert_eq!(nan.height, 2.0);

        let inf = info.measure_dom(Dom::create_body(), LogicalSize::new(f32::INFINITY, 4.0));
        assert!(inf.width.is_infinite());
    }

    #[test]
    fn measure_dom_shrink_to_fit_reaches_the_same_hook_in_its_own_mode() {
        let _serial = measure_hook_tests();
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        // No hook: zero, like `measure_dom`.
        assert_eq!(
            info.measure_dom_shrink_to_fit(Dom::create_body(), LogicalSize::new(50.0, 50.0)),
            LogicalSize::zero()
        );

        info.set_measure_dom_fn(test_measure_dom_fn, core::ptr::null_mut());
        // The two entry points share one hook and differ only in the mode
        // they pass; a mode-blind trampoline would answer both the same way.
        // (Other tests share the statics, so each assertion reads its own
        // store back immediately.)
        let fit = info.measure_dom_shrink_to_fit(Dom::create_body(), LogicalSize::new(3.0, 8.0));
        assert_eq!(fit, LogicalSize::new(6.0, 4.0));
        assert_eq!(LAST_MEASURE_MODE.load(AtomicOrdering::SeqCst), 1);
        let extent = info.measure_dom(Dom::create_body(), LogicalSize::new(3.0, 8.0));
        assert_eq!(extent, fit);
        assert_eq!(LAST_MEASURE_MODE.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn measure_dom_hook_can_be_replaced_and_last_writer_wins() {
        let _serial = measure_hook_tests();
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let mut info = vv_info(&fonts, &images, bounds_1x1());

        info.set_measure_dom_fn(test_measure_dom_fn, core::ptr::null_mut());
        // null ctx must be tolerated by the trampoline contract
        let first = info.measure_dom(Dom::create_body(), LogicalSize::new(2.0, 8.0));
        assert_eq!(first, LogicalSize::new(4.0, 4.0));

        let mut ctx_val: u32 = 0;
        info.set_measure_dom_fn(
            test_measure_dom_fn,
            core::ptr::from_mut(&mut ctx_val).cast::<c_void>(),
        );
        let second = info.measure_dom(Dom::create_body(), LogicalSize::new(2.0, 8.0));
        assert_eq!(second, first);
        assert_eq!(ctx_val, 0xABCD);
    }

    // ---- VirtualViewReturn --------------------------------------------------

    #[test]
    fn virtual_view_return_with_dom_and_keep_current_hold_their_fields() {
        // the window the callback rendered: 30px tall, sitting 300px down the document
        let mat = LogicalRect::new(
            LogicalPosition::new(0.0, 300.0),
            LogicalSize::new(600.0, 30.0),
        );
        // the whole document estimate
        let virt = LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(600.0, 30_000.0));

        let with = VirtualViewReturn::with_dom(Dom::create_body(), mat, virt);
        assert!(with.dom.is_some(), "with_dom must produce OptionDom::Some");
        assert_eq!(with.materialized, mat);
        assert_eq!(with.virtual_rect, virt);
        assert_eq!(with.dom, OptionDom::Some(Dom::create_body()));

        let keep = VirtualViewReturn::keep_current(mat, virt);
        assert!(
            keep.dom.is_none(),
            "keep_current must produce OptionDom::None"
        );
        assert_eq!(keep.materialized, mat);
        assert_eq!(keep.virtual_rect, virt);

        // the two constructors differ *only* in the dom field
        assert_ne!(with, keep);

        // default is the "keep everything, render nothing" zero value
        let d = VirtualViewReturn::default();
        assert_eq!(
            d,
            VirtualViewReturn::keep_current(LogicalRect::zero(), LogicalRect::zero())
        );
    }

    #[test]
    fn virtual_view_return_keep_current_passes_extreme_values_through_unclamped() {
        // zero
        let z = VirtualViewReturn::keep_current(LogicalRect::zero(), LogicalRect::zero());
        assert_eq!(z.materialized.size, LogicalSize::zero());
        assert_eq!(z.virtual_rect.size, LogicalSize::zero());

        // negative + f32 limits: stored verbatim (no saturation, no panic)
        let n = VirtualViewReturn::keep_current(
            LogicalRect::new(
                LogicalPosition::new(f32::MIN, f32::MAX),
                LogicalSize::new(-1.0, -0.0),
            ),
            LogicalRect::new(
                LogicalPosition::new(-f32::EPSILON, 0.0),
                LogicalSize::new(f32::MAX, f32::MIN_POSITIVE),
            ),
        );
        assert_eq!(n.materialized.size.width, -1.0);
        assert_eq!(n.materialized.origin.x, f32::MIN);
        assert_eq!(n.materialized.origin.y, f32::MAX);
        assert_eq!(n.virtual_rect.size.width, f32::MAX);
        assert_eq!(n.virtual_rect.size.height, f32::MIN_POSITIVE);

        // NaN / inf: stored verbatim; NaN makes the struct unequal to itself
        // under PartialEq, so probe the fields directly.
        let x = VirtualViewReturn::keep_current(
            LogicalRect::new(
                LogicalPosition::new(f32::NEG_INFINITY, f32::NAN),
                LogicalSize::new(f32::NAN, f32::INFINITY),
            ),
            LogicalRect::new(
                LogicalPosition::new(f32::NAN, f32::NEG_INFINITY),
                LogicalSize::new(f32::INFINITY, f32::NAN),
            ),
        );
        assert!(x.materialized.size.width.is_nan());
        assert!(
            x.materialized.size.height.is_infinite()
                && x.materialized.size.height.is_sign_positive()
        );
        assert!(
            x.materialized.origin.x.is_infinite() && x.materialized.origin.x.is_sign_negative()
        );
        assert!(x.materialized.origin.y.is_nan());
        assert!(x.virtual_rect.origin.y.is_infinite());
        assert!(x.dom.is_none());
    }

    // ---- TimerCallbackReturn ------------------------------------------------

    #[test]
    fn timer_callback_return_constructors_match_their_documented_flags() {
        let c = TimerCallbackReturn::continue_unchanged();
        assert_eq!(c.should_update, Update::DoNothing);
        assert_eq!(c.should_terminate, TerminateTimer::Continue);

        let cr = TimerCallbackReturn::continue_and_refresh_dom();
        assert_eq!(cr.should_update, Update::RefreshDom);
        assert_eq!(cr.should_terminate, TerminateTimer::Continue);

        let t = TimerCallbackReturn::terminate_unchanged();
        assert_eq!(t.should_update, Update::DoNothing);
        assert_eq!(t.should_terminate, TerminateTimer::Terminate);

        let tr = TimerCallbackReturn::terminate_and_refresh_dom();
        assert_eq!(tr.should_update, Update::RefreshDom);
        assert_eq!(tr.should_terminate, TerminateTimer::Terminate);

        // all four are distinct - no constructor is a copy-paste of another
        let all = [c, cr, t, tr];
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate() {
                assert_eq!(i == j, a == b, "constructors {i} and {j} collide");
            }
        }

        // Default is documented as "continue, no update"
        assert_eq!(TimerCallbackReturn::default(), c);
    }

    #[test]
    fn timer_callback_return_create_round_trips_every_flag_combination() {
        for u in ALL_UPDATES {
            for t in [TerminateTimer::Continue, TerminateTimer::Terminate] {
                let r = TimerCallbackReturn::create(u, t);
                assert_eq!(r.should_update, u);
                assert_eq!(r.should_terminate, t);
            }
        }

        // the named constructors agree with the generic one
        assert_eq!(
            TimerCallbackReturn::create(Update::DoNothing, TerminateTimer::Continue),
            TimerCallbackReturn::continue_unchanged()
        );
        assert_eq!(
            TimerCallbackReturn::create(Update::RefreshDom, TerminateTimer::Terminate),
            TimerCallbackReturn::terminate_and_refresh_dom()
        );

        // RefreshDomAllWindows is reachable through create() even though no
        // named constructor exposes it
        let all_windows =
            TimerCallbackReturn::create(Update::RefreshDomAllWindows, TerminateTimer::Terminate);
        assert_eq!(all_windows.should_update, Update::RefreshDomAllWindows);
    }

    // ---- LayoutCallbackInfo: construction + getters --------------------------

    #[test]
    fn layout_callback_info_new_defaults_to_initial_reason_and_holds_fields() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(1280.0, 720.0, 192), DarkLightMode::Dark);

        assert_eq!(info.relayout_reason(), RelayoutReason::Initial);
        assert_eq!(info.mode, DarkLightMode::Dark);
        assert_eq!(info.get_window_width(), 1280.0);
        assert_eq!(info.get_window_height(), 720.0);
        assert_eq!(info.get_dpi_factor(), 2.0);
        assert!(info.get_ctx().is_none());

        // the borrowed resources are reachable through the laundered pointer
        assert!(core::ptr::eq(info.internal_get_image_cache(), &fx.images));
        assert!(core::ptr::eq(info.internal_get_system_fonts(), &fx.fonts));
        assert!(core::ptr::eq(info.internal_get_gl_context(), &fx.gl));
        assert!(info.get_gl_context().is_none());
    }

    #[test]
    fn layout_callback_info_new_with_reason_round_trips_every_reason() {
        let fx = Fixture::new();
        let rd = fx.ref_data();

        for reason in [
            RelayoutReason::Initial,
            RelayoutReason::RefreshDom,
            RelayoutReason::Resize,
            RelayoutReason::ModeChange,
            RelayoutReason::RouteChange,
            RelayoutReason::Other,
            RelayoutReason::ThemeChange,
        ] {
            let info = LayoutCallbackInfo::new_with_reason(
                &rd,
                WindowSize::default(),
                DarkLightMode::Light,
                reason,
            );
            assert_eq!(info.relayout_reason(), reason);
            // clone must preserve it
            assert_eq!(info.clone().relayout_reason(), reason);
        }

        assert_eq!(RelayoutReason::default(), RelayoutReason::Initial);
    }

    #[test]
    fn layout_callback_info_get_system_style_shares_the_arc() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        let a = info.get_system_style();
        let b = info.get_system_style();
        // it is a clone of the *same* Arc, not a fresh deep copy
        assert!(Arc::ptr_eq(&a, &b));
        assert!(Arc::ptr_eq(&a, &fx.style));

        // repeated cloning must not leak/underflow the refcount
        let before = Arc::strong_count(&fx.style);
        for _ in 0..128 {
            drop(info.get_system_style());
        }
        assert_eq!(Arc::strong_count(&fx.style), before);
    }

    #[test]
    fn layout_callback_info_get_ctx_is_none_until_set_then_clones_safely() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let mut info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        assert!(info.get_ctx().is_none(), "native path must have a null ctx");

        let callable = OptionRefAny::Some(RefAny::new(7u64));
        info.set_callable_ptr(&callable);

        for _ in 0..64 {
            assert!(info.get_ctx().is_some());
        }

        let mut got = info.get_ctx();
        match got {
            OptionRefAny::Some(ref mut r) => assert_eq!(*r.downcast_ref::<u64>().unwrap(), 7),
            OptionRefAny::None => panic!("get_ctx() lost the callable"),
        }
        drop(got);

        // a clone of the info keeps pointing at the same callable
        let cloned = info;
        assert!(cloned.get_ctx().is_some());
    }

    #[test]
    fn layout_callback_info_get_system_fonts_is_empty_for_an_empty_cache() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        // an empty FcFontCache must yield an empty list, not panic
        let fonts: Vec<AzStringPair> = info.get_system_fonts();
        assert!(fonts.is_empty());
        // and be stable across calls
        assert_eq!(info.get_system_fonts().len(), fonts.len());
    }

    // ---- LayoutCallbackInfo::get_image --------------------------------------

    #[test]
    fn get_image_returns_none_for_missing_empty_and_hostile_ids() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        assert!(info.get_image(&s("")).is_none());
        assert!(info.get_image(&s("   ")).is_none());
        assert!(info.get_image(&s("nope")).is_none());
        assert!(info.get_image(&s("\u{1F600}\u{0301}")).is_none());
        assert!(info.get_image(&s("\0")).is_none());
        assert!(info.get_image(&s(&"x".repeat(100_000))).is_none());
    }

    #[test]
    fn get_image_finds_an_inserted_id_and_is_exact_match() {
        let mut fx = Fixture::new();
        fx.images.add_css_image_id(
            s("logo"),
            ImageRef::null_image(2, 2, RawImageFormat::RGBA8, Vec::new()),
        );
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        assert!(info.get_image(&s("logo")).is_some(), "positive control");

        // lookup is exact: no trimming, no case folding, no prefix matching
        assert!(info.get_image(&s("Logo")).is_none());
        assert!(info.get_image(&s(" logo")).is_none());
        assert!(info.get_image(&s("logo ")).is_none());
        assert!(info.get_image(&s("log")).is_none());
        assert!(info.get_image(&s("logos")).is_none());
    }

    // ---- LayoutCallbackInfo::get_active_route / get_route_param -------------

    #[test]
    fn get_route_param_returns_none_when_no_route_is_active() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        assert!(info.get_active_route().is_none());

        // every hostile key must take the `?` early-out, never panic
        for key in ["", " ", "\t\n", "id", "\u{1F600}", "\0", "../../etc/passwd"] {
            assert!(info.get_route_param(key).is_none(), "key {key:?}");
        }
    }

    #[test]
    fn get_route_param_valid_minimal_and_unicode_positive_controls() {
        let fx = Fixture::with_route(user_route());
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        let route = info.get_active_route().expect("route was configured");
        assert_eq!(route.pattern.as_str(), "/user/:id");

        // positive control
        assert_eq!(info.get_route_param("id").map(AzString::as_str), Some("42"));
        // multibyte key round-trips
        assert_eq!(
            info.get_route_param("\u{1F600}").map(AzString::as_str),
            Some("emoji")
        );
    }

    #[test]
    fn get_route_param_rejects_malformed_keys_without_trimming_or_folding() {
        let fx = Fixture::with_route(user_route());
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        // empty / whitespace-only
        assert!(info.get_route_param("").is_none());
        assert!(info.get_route_param("   ").is_none());
        assert!(info.get_route_param("\t\n").is_none());

        // leading/trailing junk is NOT trimmed, and lookup is case-sensitive
        assert!(info.get_route_param(" id").is_none());
        assert!(info.get_route_param("id ").is_none());
        assert!(info.get_route_param("  id  ").is_none());
        assert!(info.get_route_param("id;garbage").is_none());
        assert!(info.get_route_param("ID").is_none());
        assert!(info.get_route_param("Id").is_none());

        // no prefix / substring matching
        assert!(info.get_route_param("i").is_none());
        assert!(info.get_route_param("idd").is_none());

        // garbage bytes, NUL, control chars
        assert!(info.get_route_param("\0").is_none());
        assert!(info.get_route_param("id\0").is_none());
        assert!(info.get_route_param("\u{7F}\u{1}\u{2}").is_none());

        // boundary numeric strings
        for key in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "18446744073709551616",
            "NaN",
            "inf",
            "-inf",
            "1e400",
            "0.0000000000000000001",
        ] {
            assert!(info.get_route_param(key).is_none(), "key {key:?}");
        }

        // non-ASCII that is *not* a param, incl. combining marks
        assert!(info.get_route_param("i\u{0301}d").is_none());
        assert!(info.get_route_param("\u{1F600}\u{1F600}").is_none());
    }

    #[test]
    fn get_route_param_handles_pathological_key_sizes_and_nesting() {
        let fx = Fixture::with_route(user_route());
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        // extremely long key: must return None quickly, not hang or overflow
        let huge = "x".repeat(1_000_000);
        assert!(info.get_route_param(&huge).is_none());

        // a long key that *shares a prefix* with a real param
        let long_id = alloc::format!("id{}", "0".repeat(1_000_000));
        assert!(info.get_route_param(&long_id).is_none());

        // deeply nested brackets: the lookup is a flat scan, so this must not
        // recurse or stack-overflow
        let nested = "[".repeat(10_000) + &"]".repeat(10_000);
        assert!(info.get_route_param(&nested).is_none());
    }

    #[test]
    fn get_route_param_preserves_huge_and_unicode_values() {
        let big = "v".repeat(200_000);
        let route = RouteMatch {
            pattern: s("/blob/:data"),
            params: StringPairVec::from_vec(Vec::from([AzStringPair {
                key: s("data"),
                value: s(&big),
            }])),
        };
        let fx = Fixture::with_route(route);
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);

        let got = info.get_route_param("data").expect("param exists");
        assert_eq!(got.as_str().len(), 200_000);
    }

    // ---- LayoutCallbackInfo: responsive predicates ---------------------------

    #[test]
    fn window_predicates_obey_trichotomy_and_the_between_identity() {
        let fx = Fixture::new();
        let rd = fx.ref_data();

        let probes = [
            0.0f32,
            -0.0,
            1.0,
            -1.0,
            640.0,
            f32::MIN,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];

        for &dim in &probes {
            let info = LayoutCallbackInfo::new(&rd, win(dim, dim, 96), DarkLightMode::Light);

            for &px in &probes {
                let lt = info.window_width_less_than(px);
                let gt = info.window_width_greater_than(px);
                let eq = info.get_window_width() == px;

                // exactly one of <, >, == holds for non-NaN operands
                assert_eq!(
                    u8::from(lt) + u8::from(gt) + u8::from(eq),
                    1,
                    "trichotomy broken for width {dim} vs {px}"
                );

                // height predicates mirror the width ones on a square window
                assert_eq!(info.window_height_less_than(px), lt);
                assert_eq!(info.window_height_greater_than(px), gt);

                for &px2 in &probes {
                    // between(a, b) == !(w < a) && !(w > b)
                    assert_eq!(
                        info.window_width_between(px, px2),
                        !info.window_width_less_than(px) && !info.window_width_greater_than(px2),
                        "between identity broken for width {dim} in [{px}, {px2}]"
                    );
                    assert_eq!(
                        info.window_height_between(px, px2),
                        info.window_width_between(px, px2)
                    );
                }
            }
        }
    }

    #[test]
    fn window_predicates_with_inverted_and_degenerate_ranges() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(640.0, 480.0, 96), DarkLightMode::Light);

        // inverted range is always empty
        assert!(!info.window_width_between(1000.0, 100.0));
        assert!(!info.window_height_between(1000.0, 100.0));

        // degenerate (min == max) range is inclusive on both ends
        assert!(info.window_width_between(640.0, 640.0));
        assert!(info.window_height_between(480.0, 480.0));
        assert!(!info.window_width_between(639.9, 639.95));

        // inclusive boundaries
        assert!(info.window_width_between(640.0, 1000.0));
        assert!(info.window_width_between(0.0, 640.0));

        // strictness at the exact boundary
        assert!(!info.window_width_less_than(640.0));
        assert!(!info.window_width_greater_than(640.0));
        assert!(info.window_width_less_than(640.001));
        assert!(info.window_width_greater_than(639.999));

        // the widest possible range contains a finite width
        assert!(info.window_width_between(f32::NEG_INFINITY, f32::INFINITY));
    }

    #[test]
    fn window_predicates_are_all_false_for_nan_probes() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info = LayoutCallbackInfo::new(&rd, win(640.0, 480.0, 96), DarkLightMode::Light);

        // every comparison against NaN is false - no panic, no accidental `true`
        assert!(!info.window_width_less_than(f32::NAN));
        assert!(!info.window_width_greater_than(f32::NAN));
        assert!(!info.window_width_between(f32::NAN, f32::NAN));
        assert!(!info.window_width_between(f32::NAN, 10_000.0));
        assert!(!info.window_width_between(0.0, f32::NAN));

        assert!(!info.window_height_less_than(f32::NAN));
        assert!(!info.window_height_greater_than(f32::NAN));
        assert!(!info.window_height_between(f32::NAN, f32::NAN));
        assert!(!info.window_height_between(f32::NAN, 10_000.0));
        assert!(!info.window_height_between(0.0, f32::NAN));
    }

    #[test]
    fn window_predicates_are_all_false_for_a_nan_sized_window() {
        let fx = Fixture::new();
        let rd = fx.ref_data();
        let info =
            LayoutCallbackInfo::new(&rd, win(f32::NAN, f32::NAN, 96), DarkLightMode::Light);

        assert!(info.get_window_width().is_nan());
        assert!(info.get_window_height().is_nan());

        // a NaN window is neither smaller, larger, nor within any range
        for px in [0.0f32, 640.0, f32::MAX, f32::INFINITY, f32::NEG_INFINITY] {
            assert!(!info.window_width_less_than(px));
            assert!(!info.window_width_greater_than(px));
            assert!(!info.window_height_less_than(px));
            assert!(!info.window_height_greater_than(px));
            assert!(!info.window_width_between(f32::NEG_INFINITY, px));
            assert!(!info.window_height_between(px, f32::INFINITY));
        }
    }

    #[test]
    fn get_dpi_factor_at_zero_and_u32_limits() {
        let fx = Fixture::new();
        let rd = fx.ref_data();

        // 96 DPI is the 1.0 baseline
        let base = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 96), DarkLightMode::Light);
        assert_eq!(base.get_dpi_factor(), 1.0);

        let hidpi = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 192), DarkLightMode::Light);
        assert_eq!(hidpi.get_dpi_factor(), 2.0);

        // dpi = 0 must not divide-by-zero-panic; it yields 0.0
        let zero = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 0), DarkLightMode::Light);
        assert_eq!(zero.get_dpi_factor(), 0.0);

        // u32::MAX must not overflow the f32 cast - it stays finite
        let max = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, u32::MAX), DarkLightMode::Light);
        let f = max.get_dpi_factor();
        assert!(f.is_finite() && f > 0.0, "dpi factor {f} is not finite");
        assert_eq!(f, (u32::MAX as f32) / 96.0);

        // dpi = 1 rounds to a tiny-but-positive factor rather than 0
        let one = LayoutCallbackInfo::new(&rd, win(1.0, 1.0, 1), DarkLightMode::Light);
        assert!(one.get_dpi_factor() > 0.0);
    }

    // ---- HidpiAdjustedBounds -------------------------------------------------

    #[test]
    fn hidpi_adjusted_bounds_from_bounds_holds_its_fields() {
        let b =
            HidpiAdjustedBounds::from_bounds(LayoutSize::new(800, 600), DpiScaleFactor::new(1.5));
        assert_eq!(b.get_logical_size(), LogicalSize::new(800.0, 600.0));
        assert_eq!(b.get_hidpi_factor(), DpiScaleFactor::new(1.5));
        assert_eq!(b.logical_size, b.get_logical_size());
        assert_eq!(b.hidpi_factor, b.get_hidpi_factor());

        let p = b.get_physical_size();
        assert_eq!(p.width, 1200);
        assert_eq!(p.height, 900);
    }

    #[test]
    fn hidpi_adjusted_bounds_at_zero() {
        let b = HidpiAdjustedBounds::from_bounds(LayoutSize::new(0, 0), DpiScaleFactor::new(1.0));
        assert_eq!(b.get_logical_size(), LogicalSize::zero());
        let p = b.get_physical_size();
        assert_eq!(p.width, 0);
        assert_eq!(p.height, 0);

        // a zero scale factor collapses any size to 0x0 without panicking
        let z =
            HidpiAdjustedBounds::from_bounds(LayoutSize::new(1920, 1080), DpiScaleFactor::new(0.0));
        let zp = z.get_physical_size();
        assert_eq!(zp.width, 0);
        assert_eq!(zp.height, 0);
    }

    /// `get_physical_size` funnels through `roundf(x) as u32`, which is a
    /// *saturating* float->int cast in Rust: negatives clamp to 0, huge values
    /// clamp to u32::MAX, NaN becomes 0. Pin that down so a future refactor to
    /// an unchecked cast (UB) or a panicking one is caught.
    #[test]
    fn hidpi_adjusted_bounds_physical_size_saturates_on_negative_input() {
        let b =
            HidpiAdjustedBounds::from_bounds(LayoutSize::new(-100, -50), DpiScaleFactor::new(1.0));
        assert_eq!(b.get_logical_size(), LogicalSize::new(-100.0, -50.0));

        let p = b.get_physical_size();
        assert_eq!(
            p.width, 0,
            "negative logical width must clamp to 0, not wrap"
        );
        assert_eq!(
            p.height, 0,
            "negative logical height must clamp to 0, not wrap"
        );

        // negative scale factor on a positive size clamps the same way
        let neg_scale =
            HidpiAdjustedBounds::from_bounds(LayoutSize::new(100, 100), DpiScaleFactor::new(-2.0));
        let np = neg_scale.get_physical_size();
        assert_eq!(np.width, 0);
        assert_eq!(np.height, 0);
    }

    #[test]
    fn hidpi_adjusted_bounds_physical_size_saturates_at_the_upper_limit() {
        // isize::MAX logical px * 1.0 overflows u32 -> must saturate, not wrap
        let b = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(isize::MAX, isize::MAX),
            DpiScaleFactor::new(1.0),
        );
        let p = b.get_physical_size();
        assert_eq!(p.width, u32::MAX);
        assert_eq!(p.height, u32::MAX);

        // isize::MIN saturates downwards to 0
        let min = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(isize::MIN, isize::MIN),
            DpiScaleFactor::new(1.0),
        );
        let mp = min.get_physical_size();
        assert_eq!(mp.width, 0);
        assert_eq!(mp.height, 0);

        // a huge scale factor on a modest size also saturates
        let huge_scale = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(1000, 1000),
            DpiScaleFactor::new(f32::MAX),
        );
        let hp = huge_scale.get_physical_size();
        assert_eq!(hp.width, u32::MAX);
        assert_eq!(hp.height, u32::MAX);
    }

    /// `DpiScaleFactor` stores its f32 in a fixed-point `isize` (x1000), so
    /// NaN quantizes to 0 and +/-inf quantize to the isize limits. Assert the
    /// *observable* consequence rather than a panic.
    #[test]
    fn hidpi_adjusted_bounds_physical_size_with_nan_and_infinite_scale() {
        let nan = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(f32::NAN),
        );
        // NaN -> fixed-point 0 -> 0.0 scale -> 0x0 physical
        assert_eq!(nan.get_hidpi_factor().inner.get(), 0.0);
        let np = nan.get_physical_size();
        assert_eq!(np.width, 0);
        assert_eq!(np.height, 0);

        let inf = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(f32::INFINITY),
        );
        // +inf -> saturated fixed-point -> huge (but finite) scale
        assert!(inf.get_hidpi_factor().inner.get().is_finite());
        let ip = inf.get_physical_size();
        assert_eq!(ip.width, u32::MAX);
        assert_eq!(ip.height, u32::MAX);

        let neg_inf = HidpiAdjustedBounds::from_bounds(
            LayoutSize::new(100, 100),
            DpiScaleFactor::new(f32::NEG_INFINITY),
        );
        let nip = neg_inf.get_physical_size();
        assert_eq!(nip.width, 0);
        assert_eq!(nip.height, 0);
    }

    #[test]
    fn hidpi_adjusted_bounds_physical_size_rounds_to_nearest() {
        // 0.5px rounds away from zero (libm::roundf), not truncates
        let b = HidpiAdjustedBounds::from_bounds(LayoutSize::new(3, 3), DpiScaleFactor::new(1.5));
        let p = b.get_physical_size();
        assert_eq!(p.width, 5, "3 * 1.5 = 4.5 must round to 5");
        assert_eq!(p.height, 5);

        // idempotent: repeated calls give the same answer
        let p2 = b.get_physical_size();
        assert_eq!(p.width, p2.width);
        assert_eq!(p.height, p2.height);
    }

    // ---- CoreCallbackDataVec -------------------------------------------------

    fn cb_data(cb: usize) -> CoreCallbackData {
        CoreCallbackData {
            event: EventFilter::Hover(HoverEventFilter::MouseOver),
            callback: CoreCallback::from(cb),
            refany: RefAny::new(cb),
        }
    }

    #[test]
    fn core_callback_data_vec_as_container_on_empty_vecs_does_not_panic() {
        // both the const-empty and the heap-empty representation must produce
        // a valid (length-0) container - a null-ptr slice here would be UB
        let empty = CoreCallbackDataVec::new();
        assert_eq!(empty.as_container().len(), 0);
        assert!(empty.as_container().internal.is_empty());

        let from_empty_vec = CoreCallbackDataVec::from_vec(Vec::new());
        assert_eq!(from_empty_vec.as_container().len(), 0);

        let mut mut_empty = CoreCallbackDataVec::from_vec(Vec::new());
        assert!(mut_empty.as_container_mut().internal.is_empty());
    }

    #[test]
    fn core_callback_data_vec_as_container_matches_the_backing_vec() {
        let v = CoreCallbackDataVec::from_vec(Vec::from([cb_data(1), cb_data(2), cb_data(3)]));

        let c = v.as_container();
        assert_eq!(c.len(), 3);
        assert_eq!(c.len(), v.len());
        assert_eq!(c.internal[0].callback.cb, 1);
        assert_eq!(c.internal[2].callback.cb, 3);

        // the container borrows - it does not copy
        assert!(core::ptr::eq(c.internal.as_ptr(), v.as_slice().as_ptr()));
    }

    #[test]
    fn core_callback_data_vec_as_container_mut_writes_through() {
        let mut v = CoreCallbackDataVec::from_vec(Vec::from([cb_data(1), cb_data(2)]));

        {
            let mut c = v.as_container_mut();
            assert_eq!(c.internal.len(), 2);
            c.internal[0].callback.cb = 99;
            c.internal[1].event = EventFilter::Hover(HoverEventFilter::MouseDown);
        }

        // mutations are visible through the immutable container
        let c = v.as_container();
        assert_eq!(c.internal[0].callback.cb, 99);
        assert_eq!(
            c.internal[1].event,
            EventFilter::Hover(HoverEventFilter::MouseDown)
        );
        assert_eq!(c.len(), 2);
    }
}

/// Tests for the recorded window-size queries — the responsive helpers
/// (`window_width_less_than` & co.) every `layout()` should branch on, and the
/// mechanism that lets a resize skip `layout()` when no recorded answer flips.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod size_query_tests {
    use super::*;
    use crate::geom::LogicalSize;

    fn win(width: f32, height: f32) -> WindowSize {
        WindowSize {
            dimensions: LogicalSize::new(width, height),
            ..WindowSize::default()
        }
    }

    fn info_at(rd: &LayoutCallbackInfoRefData<'_>, w: f32, h: f32) -> LayoutCallbackInfo {
        LayoutCallbackInfo::new(rd, win(w, h), DarkLightMode::Light)
    }

    fn drain() -> (alloc::vec::Vec<SizeQuery>, bool) {
        take_recorded_size_queries()
    }

    /// Build the minimal ref-data a `LayoutCallbackInfo` needs. The queries
    /// only read `window_size`, so everything else can be empty.
    struct Rd {
        image_cache: crate::resources::ImageCache,
        gl: crate::gl::OptionGlContextPtr,
        fonts: rust_fontconfig::FcFontCache,
        style: alloc::sync::Arc<azul_css::system::SystemStyle>,
    }
    impl Rd {
        fn new() -> Self {
            Self {
                image_cache: crate::resources::ImageCache::default(),
                gl: crate::gl::OptionGlContextPtr::None,
                fonts: rust_fontconfig::FcFontCache::default(),
                style: alloc::sync::Arc::new(azul_css::system::SystemStyle::default()),
            }
        }
        fn ref_data(&self) -> LayoutCallbackInfoRefData<'_> {
            static EN_US: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            let locale = EN_US.get_or_init(|| AzString::from("en-US"));
            static NO_WINDOW_ID: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            LayoutCallbackInfoRefData {
                locale,
                accessed_locale: core::cell::Cell::new(false),
                accessed_text_direction: core::cell::Cell::new(false),
                text_direction: crate::callbacks::TextDirection::LeftToRight,
                image_cache: &self.image_cache,
                gl_context: &self.gl,
                system_fonts: &self.fonts,
                system_style: self.style.clone(),
                active_route: None,
                monitors: crate::window::MonitorVec::from_const_slice(&[]),
                safe_area: azul_css::system::SafeAreaInsets::default(),
                global_hotkeys: crate::global_hotkey::GlobalHotkeyInfoVec::from_const_slice(&[]),
                window_id: NO_WINDOW_ID.get_or_init(|| AzString::from("")),
            }
        }
    }

    #[test]
    fn every_responsive_helper_records_with_its_exact_operator() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = drain();

        let info = info_at(&rd, 800.0, 600.0);
        assert!(
            !info.window_width_less_than(800.0),
            "strict <: boundary is false"
        );
        assert!(
            !info.window_width_greater_than(800.0),
            "strict >: boundary is false"
        );
        assert!(
            info.window_width_between(800.0, 1024.0),
            "between is inclusive"
        );
        assert!(info.window_height_less_than(601.0));
        assert!(!info.window_height_greater_than(600.0));
        assert!(info.window_height_between(0.0, 600.0));

        let (recorded, overflowed) = drain();
        // between records BOTH of its bounds, so 4 single-bound calls + 2
        // between calls = 8 queries.
        assert_eq!(
            recorded.len(),
            8,
            "every call recorded; between records two bounds"
        );
        assert!(!overflowed);
        assert_eq!(recorded[0].op, SizeQueryOp::LessThan);
        assert_eq!(recorded[1].op, SizeQueryOp::GreaterThan);
        assert_eq!(recorded[2].op, SizeQueryOp::GreaterOrEqual);
        assert_eq!(recorded[3].op, SizeQueryOp::LessOrEqual);
    }

    #[test]
    fn flips_at_detects_exactly_the_crossings() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = drain();

        let info = info_at(&rd, 800.0, 600.0);
        let mobile = info.window_width_less_than(640.0); // false at 800
        assert!(!mobile);
        let (recorded, _) = drain();
        let q = recorded[0];

        // Shrinking within the desktop range does not flip…
        assert!(!q.flips_at(LogicalSize::new(700.0, 600.0)));
        assert!(
            !q.flips_at(LogicalSize::new(640.0, 600.0)),
            "strict <: 640 is still false"
        );
        // …crossing the queried threshold does…
        assert!(q.flips_at(LogicalSize::new(639.9, 600.0)));
        assert!(q.flips_at(LogicalSize::new(320.0, 600.0)));
        // …and the other axis is irrelevant to a width query.
        assert!(!q.flips_at(LogicalSize::new(700.0, 10.0)));
    }

    /// `between` must flip on BOTH of its bounds, inclusively — the reason
    /// [`SizeQueryOp`] has four exact operators instead of a bool.
    #[test]
    fn between_flips_on_either_bound_with_inclusive_semantics() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = drain();

        let info = info_at(&rd, 800.0, 600.0);
        assert!(info.window_width_between(768.0, 1024.0));
        let (recorded, _) = drain();
        let lower = recorded[0];
        let upper = recorded[1];

        assert!(
            !lower.flips_at(LogicalSize::new(768.0, 600.0)),
            ">= 768: boundary holds"
        );
        assert!(lower.flips_at(LogicalSize::new(767.9, 600.0)));
        assert!(
            !upper.flips_at(LogicalSize::new(1024.0, 600.0)),
            "<= 1024: boundary holds"
        );
        assert!(upper.flips_at(LogicalSize::new(1024.1, 600.0)));
    }

    #[test]
    fn drain_resets_the_recording() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = drain();

        let info = info_at(&rd, 1024.0, 768.0);
        let _ = info.window_width_greater_than(640.0);
        let (first, _) = drain();
        assert_eq!(first.len(), 1);
        let (second, overflowed) = drain();
        assert!(second.is_empty(), "drain must reset");
        assert!(!overflowed);
    }

    #[test]
    fn overflow_latches_and_reports_rather_than_dropping_silently() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = drain();

        let info = info_at(&rd, 1024.0, 768.0);
        for i in 0..(size_query_recorder::SIZE_QUERY_CAP + 10) {
            let _ = info.window_width_greater_than(i as f32);
        }
        let (recorded, overflowed) = drain();
        assert_eq!(recorded.len(), size_query_recorder::SIZE_QUERY_CAP);
        assert!(
            overflowed,
            "past the cap the drain MUST say the list is incomplete — silence here is a resize \
             skipping a layout() that would have branched"
        );
        // And the latch itself resets with the drain.
        let (_, overflowed2) = drain();
        assert!(!overflowed2);
    }
}

/// Tests for the declared system-style dependencies — the seam that decides
/// whether an appearance change costs the app a full `Update::RefreshDom` or
/// only a restyle.
#[cfg(test)]
mod system_style_dependency_tests {
    use azul_css::{
        props::basic::color::ColorU,
        system::{SystemStyle, DarkLightMode},
    };

    use super::*;
    use crate::geom::LogicalSize;

    fn style() -> SystemStyle {
        SystemStyle::default()
    }

    fn c(r: u8, g: u8, b: u8) -> azul_css::props::basic::color::OptionColorU {
        Some(ColorU { r, g, b, a: 255 }).into()
    }

    struct Rd {
        image_cache: crate::resources::ImageCache,
        gl: crate::gl::OptionGlContextPtr,
        fonts: rust_fontconfig::FcFontCache,
        style: alloc::sync::Arc<SystemStyle>,
    }
    impl Rd {
        fn new() -> Self {
            Self {
                image_cache: crate::resources::ImageCache::default(),
                gl: crate::gl::OptionGlContextPtr::None,
                fonts: rust_fontconfig::FcFontCache::default(),
                style: alloc::sync::Arc::new(style()),
            }
        }
        fn ref_data(&self) -> LayoutCallbackInfoRefData<'_> {
            static EN_US: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            let locale = EN_US.get_or_init(|| AzString::from("en-US"));
            static NO_WINDOW_ID: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            LayoutCallbackInfoRefData {
                locale,
                accessed_locale: core::cell::Cell::new(false),
                accessed_text_direction: core::cell::Cell::new(false),
                text_direction: crate::callbacks::TextDirection::LeftToRight,
                image_cache: &self.image_cache,
                gl_context: &self.gl,
                system_fonts: &self.fonts,
                system_style: alloc::sync::Arc::clone(&self.style),
                active_route: None,
                monitors: crate::window::MonitorVec::from_const_slice(&[]),
                safe_area: azul_css::system::SafeAreaInsets::default(),
                global_hotkeys: crate::global_hotkey::GlobalHotkeyInfoVec::from_const_slice(&[]),
                window_id: NO_WINDOW_ID.get_or_init(|| AzString::from("")),
            }
        }
    }

    fn info(rd: &LayoutCallbackInfoRefData<'_>) -> LayoutCallbackInfo {
        LayoutCallbackInfo::new(
            rd,
            WindowSize {
                dimensions: LogicalSize::new(800.0, 600.0),
                ..WindowSize::default()
            },
            DarkLightMode::Light,
        )
    }

    /// The whole point: a callback that only mirrors light/dark keeps its DOM
    /// across a change that leaves the polarity alone, and loses it when the
    /// polarity flips. Both directions asserted — a `dom_depends_on_change`
    /// hardwired to `false` would pass the first half alone.
    #[test]
    fn a_theme_only_dependency_ignores_a_palette_move_but_not_a_polarity_flip() {
        let mut deps = SystemStyleDependencies::empty();
        deps.insert(SystemStyleDependency::Theme);

        let old = style();

        // Same polarity, different accent: this app cannot have built a
        // different DOM, so it must not be rebuilt.
        let mut recoloured = style();
        recoloured.colors.accent = c(61, 174, 233);
        assert_ne!(old.colors, recoloured.colors, "the fixture must differ");
        assert!(!deps.dom_depends_on_change(&old, &recoloured));

        // Polarity flip: rebuild.
        let mut dark = style();
        dark.mode = DarkLightMode::Dark;
        assert!(deps.dom_depends_on_change(&old, &dark));
    }

    /// An app that paints its own controls from the OS palette IS invalidated
    /// by a light-to-light accent change — the case the theme-only app is not.
    #[test]
    fn a_colors_dependency_catches_a_light_to_light_accent_change() {
        let mut deps = SystemStyleDependencies::empty();
        deps.insert(SystemStyleDependency::Colors);

        let old = style();
        let mut recoloured = style();
        recoloured.colors.accent = c(61, 174, 233);
        assert!(deps.dom_depends_on_change(&old, &recoloured));

        // ... and is NOT invalidated by something outside the palette.
        let mut louder = style();
        louder.audio.event_sounds_enabled = !louder.audio.event_sounds_enabled;
        assert_ne!(old.audio, louder.audio, "the fixture must differ");
        assert!(!deps.dom_depends_on_change(&old, &louder));
    }

    /// Declaring NOTHING is not declaring independence: every callback written
    /// before this API existed lands here, and silently skipping its rebuild
    /// would leave the old palette baked into its DOM.
    #[test]
    fn an_undeclared_callback_is_rebuilt_on_any_change_and_only_on_a_change() {
        let deps = SystemStyleDependencies::empty();
        let old = style();

        let mut moved = style();
        moved.colors.accent = c(61, 174, 233);
        assert!(deps.dom_depends_on_change(&old, &moved));

        // But an EQUAL style is still not a change — a settings broadcast that
        // carries nothing new must stay free.
        assert!(!deps.dom_depends_on_change(&old, &style()));
    }

    /// `Everything` subsumes every facet, which is what makes one widget's
    /// whole-struct read conservative for the entire tree.
    #[test]
    fn everything_contains_every_facet() {
        let mut deps = SystemStyleDependencies::empty();
        deps.insert(SystemStyleDependency::Everything);
        for dep in [
            SystemStyleDependency::Theme,
            SystemStyleDependency::Colors,
            SystemStyleDependency::Fonts,
            SystemStyleDependency::Metrics,
            SystemStyleDependency::Icons,
            SystemStyleDependency::Accessibility,
        ] {
            assert!(deps.contains(dep), "{dep:?} must be covered by Everything");
        }
        // And a narrow declaration does NOT claim its neighbours.
        let mut narrow = SystemStyleDependencies::empty();
        narrow.insert(SystemStyleDependency::Theme);
        assert!(narrow.contains(SystemStyleDependency::Theme));
        assert!(!narrow.contains(SystemStyleDependency::Colors));
    }

    /// The recorder unions what the callback declared, and the drain empties
    /// it — a leak across two `layout()` calls would make the second one
    /// inherit the first's dependencies.
    #[test]
    fn declarations_union_and_the_drain_empties_the_recording() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = take_recorded_style_dependencies();

        let info = info(&rd);
        assert!(
            take_recorded_style_dependencies().is_empty(),
            "constructing the info declares nothing"
        );

        info.depends_on_system_style(SystemStyleDependency::Theme);
        info.depends_on_system_style(SystemStyleDependency::Fonts);
        let declared = take_recorded_style_dependencies();
        assert!(declared.contains(SystemStyleDependency::Theme));
        assert!(declared.contains(SystemStyleDependency::Fonts));
        assert!(!declared.contains(SystemStyleDependency::Colors));

        assert!(
            take_recorded_style_dependencies().is_empty(),
            "the drain must reset — the next layout() starts from nothing"
        );
    }

    /// `get_mode()` is the tracked way to read the light / dark mode; the bare
    /// `theme` FIELD declares nothing, exactly like reading `window_size`
    /// directly.
    #[test]
    fn get_mode_declares_the_mode_and_the_bare_field_declares_nothing() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = take_recorded_style_dependencies();

        let info = info(&rd);
        let _ = info.mode;
        assert!(take_recorded_style_dependencies().is_empty());

        assert_eq!(info.get_mode(), DarkLightMode::Light);
        let declared = take_recorded_style_dependencies();
        assert!(declared.contains(SystemStyleDependency::Theme));
        assert!(!declared.contains(SystemStyleDependency::Colors));
    }

    /// Handing out the whole struct is opaque, so it declares everything —
    /// while the untracked accessor (engine-internal readers, and callbacks
    /// that already said what they read) declares nothing.
    #[test]
    fn get_system_style_declares_everything_and_the_untracked_one_declares_nothing() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let _ = take_recorded_style_dependencies();

        let info = info(&rd);
        let _ = info.get_system_style_untracked();
        assert!(take_recorded_style_dependencies().is_empty());

        let _ = info.get_system_style();
        let declared = take_recorded_style_dependencies();
        assert!(declared.contains(SystemStyleDependency::Everything));
        assert!(declared.contains(SystemStyleDependency::Colors));
    }
}

/// `LayoutCallbackInfo::add_global_hotkey` & co.: `layout()` DECLARES the
/// global hotkeys the state it is built from wants, through the same
/// thread-local recorder shape as the size queries and the style
/// dependencies - the FFI-frozen, `Copy` info struct does not change.
#[cfg(test)]
mod global_hotkey_recorder_tests {
    use azul_css::{system::SystemStyle, AzString};

    use super::*;
    use crate::{
        global_hotkey::{
            take_recorded_global_hotkeys, GlobalHotkey, GlobalHotkeyError, GlobalHotkeyInfo,
            GlobalHotkeyInfoVec, GlobalHotkeyOwner, GlobalHotkeyStatus, HotkeyModifiers,
            GLOBAL_HOTKEY_DECLARATION_CAP,
        },
        geom::LogicalSize,
        refany::{OptionRefAny, RefAny},
        window::VirtualKeyCode as K,
    };

    struct Rd {
        image_cache: crate::resources::ImageCache,
        gl: crate::gl::OptionGlContextPtr,
        fonts: rust_fontconfig::FcFontCache,
        style: alloc::sync::Arc<SystemStyle>,
        hotkeys: GlobalHotkeyInfoVec,
    }

    impl Rd {
        fn new(hotkeys: Vec<GlobalHotkeyInfo>) -> Self {
            Self {
                image_cache: crate::resources::ImageCache::default(),
                gl: crate::gl::OptionGlContextPtr::None,
                fonts: rust_fontconfig::FcFontCache::default(),
                style: alloc::sync::Arc::new(SystemStyle::default()),
                hotkeys: GlobalHotkeyInfoVec::from_vec(hotkeys),
            }
        }

        fn ref_data(&self) -> LayoutCallbackInfoRefData<'_> {
            static EN_US: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            let locale = EN_US.get_or_init(|| AzString::from("en-US"));
            static NO_WINDOW_ID: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            LayoutCallbackInfoRefData {
                locale,
                accessed_locale: core::cell::Cell::new(false),
                accessed_text_direction: core::cell::Cell::new(false),
                text_direction: crate::callbacks::TextDirection::LeftToRight,
                image_cache: &self.image_cache,
                gl_context: &self.gl,
                system_fonts: &self.fonts,
                system_style: alloc::sync::Arc::clone(&self.style),
                active_route: None,
                monitors: crate::window::MonitorVec::from_const_slice(&[]),
                safe_area: azul_css::system::SafeAreaInsets::default(),
                global_hotkeys: self.hotkeys.clone(),
                window_id: NO_WINDOW_ID.get_or_init(|| AzString::from("")),
            }
        }
    }

    fn info(rd: &LayoutCallbackInfoRefData<'_>) -> LayoutCallbackInfo {
        LayoutCallbackInfo::new(
            rd,
            WindowSize {
                dimensions: LogicalSize::new(800.0, 600.0),
                ..WindowSize::default()
            },
            DarkLightMode::Light,
        )
    }

    fn ctrl_alt(key: K) -> GlobalHotkey {
        GlobalHotkey {
            modifiers: HotkeyModifiers {
                ctrl: true,
                alt: true,
                shift: false,
                meta: false,
            },
            key,
        }
    }

    /// A callback the recorder only stores (never invoked here), told apart
    /// by its number.
    fn cb(n: usize) -> CoreCallback {
        CoreCallback {
            cb: n,
            ctx: OptionRefAny::None,
        }
    }

    fn entry(
        hotkey: GlobalHotkey,
        status: GlobalHotkeyStatus,
        owner: GlobalHotkeyOwner,
    ) -> GlobalHotkeyInfo {
        GlobalHotkeyInfo {
            hotkey,
            status,
            trigger: hotkey.to_display_string(),
            owner,
        }
    }

    /// What one `layout()` call declared drains ONCE: a leak into the next
    /// call would keep a hotkey alive that the new state no longer wants.
    #[test]
    fn declarations_drain_once_and_the_next_drain_is_empty() {
        let rd = Rd::new(Vec::new());
        let rd = rd.ref_data();
        let _ = take_recorded_global_hotkeys();

        let info = info(&rd);
        assert!(
            take_recorded_global_hotkeys().declared.is_empty(),
            "constructing the info declares nothing"
        );

        info.add_global_hotkey(ctrl_alt(K::K), RefAny::new(1_u32), cb(1));
        info.add_global_hotkey_with_description(
            ctrl_alt(K::J),
            AzString::from("Bring the app to the front"),
            RefAny::new(2_u32),
            cb(2),
        );
        let recorded = take_recorded_global_hotkeys();
        assert_eq!(recorded.declared.len(), 2);
        assert_eq!(recorded.declared[0].hotkey, ctrl_alt(K::K));
        assert_eq!(recorded.declared[0].description.as_str(), "");
        assert_eq!(recorded.declared[1].hotkey, ctrl_alt(K::J));
        assert_eq!(
            recorded.declared[1].description.as_str(),
            "Bring the app to the front"
        );
        assert_eq!(recorded.declared[1].callback.cb, 2);
        assert!(!recorded.read_status, "declaring is not reading");
        assert!(!recorded.overflowed);

        let again = take_recorded_global_hotkeys();
        assert!(
            again.declared.is_empty(),
            "the drain must reset - the next layout() starts from nothing"
        );
    }

    /// Declaring the same accelerator twice in one pass: the last
    /// declaration wins, as a later `with_callback` would.
    #[test]
    fn the_last_duplicate_wins() {
        let rd = Rd::new(Vec::new());
        let rd = rd.ref_data();
        let _ = take_recorded_global_hotkeys();

        let info = info(&rd);
        info.add_global_hotkey(ctrl_alt(K::K), RefAny::new(1_u32), cb(1));
        info.add_global_hotkey(ctrl_alt(K::J), RefAny::new(2_u32), cb(2));
        info.add_global_hotkey(ctrl_alt(K::K), RefAny::new(3_u32), cb(3));
        let recorded = take_recorded_global_hotkeys();
        assert_eq!(recorded.declared.len(), 2, "one entry per accelerator");
        let k = recorded
            .declared
            .iter()
            .find(|d| d.hotkey == ctrl_alt(K::K))
            .expect("Ctrl+Alt+K is declared");
        assert_eq!(k.callback.cb, 3);
    }

    /// `layout()` reads where a hotkey stood when the pass began (the
    /// snapshot the engine took), and the READ is recorded: a later status
    /// change re-runs this layout once.
    #[test]
    fn a_status_read_answers_from_the_snapshot_and_is_recorded() {
        let rd = Rd::new(vec![
            entry(
                ctrl_alt(K::K),
                GlobalHotkeyStatus::Active,
                GlobalHotkeyOwner::ThisWindow,
            ),
            entry(
                ctrl_alt(K::T),
                GlobalHotkeyStatus::Failed(GlobalHotkeyError::TakenByAnotherApp),
                GlobalHotkeyOwner::OtherWindow,
            ),
        ]);
        let rd = rd.ref_data();
        let _ = take_recorded_global_hotkeys();

        let info = info(&rd);
        assert_eq!(
            info.get_global_hotkey_status(ctrl_alt(K::K)),
            GlobalHotkeyStatus::Active
        );
        assert_eq!(
            info.get_global_hotkey_status(ctrl_alt(K::T)),
            GlobalHotkeyStatus::Failed(GlobalHotkeyError::TakenByAnotherApp)
        );
        assert_eq!(
            info.get_global_hotkey_status(ctrl_alt(K::J)),
            GlobalHotkeyStatus::NotRegistered,
            "an accelerator the snapshot does not list is not registered"
        );
        let recorded = take_recorded_global_hotkeys();
        assert!(recorded.read_status);
        assert!(recorded.declared.is_empty());
    }

    #[test]
    fn the_list_is_the_snapshot_with_its_owners_and_reading_it_is_recorded() {
        let snapshot = vec![
            entry(
                ctrl_alt(K::A),
                GlobalHotkeyStatus::Active,
                GlobalHotkeyOwner::App,
            ),
            entry(
                ctrl_alt(K::B),
                GlobalHotkeyStatus::Pending,
                GlobalHotkeyOwner::ThisWindow,
            ),
        ];
        let rd = Rd::new(snapshot.clone());
        let rd = rd.ref_data();
        let _ = take_recorded_global_hotkeys();

        let info = info(&rd);
        let listed = info.get_global_hotkeys();
        assert_eq!(listed.as_ref(), snapshot.as_slice());
        assert!(take_recorded_global_hotkeys().read_status);
    }

    /// A callback generating accelerators programmatically cannot grow the
    /// recording without bound; the drain says the list is incomplete.
    #[test]
    fn more_than_the_cap_latches_overflowed() {
        const LETTERS: [K; 26] = [
            K::A,
            K::B,
            K::C,
            K::D,
            K::E,
            K::F,
            K::G,
            K::H,
            K::I,
            K::J,
            K::K,
            K::L,
            K::M,
            K::N,
            K::O,
            K::P,
            K::Q,
            K::R,
            K::S,
            K::T,
            K::U,
            K::V,
            K::W,
            K::X,
            K::Y,
            K::Z,
        ];
        let rd = Rd::new(Vec::new());
        let rd = rd.ref_data();
        let _ = take_recorded_global_hotkeys();

        let info = info(&rd);
        for i in 0..(GLOBAL_HOTKEY_DECLARATION_CAP + 10) {
            let bits = i / LETTERS.len();
            let hotkey = GlobalHotkey {
                modifiers: HotkeyModifiers {
                    ctrl: bits & 1 != 0,
                    alt: bits & 2 != 0,
                    shift: bits & 4 != 0,
                    meta: bits & 8 != 0,
                },
                key: LETTERS[i % LETTERS.len()],
            };
            info.add_global_hotkey(hotkey, RefAny::new(i), cb(i + 1));
        }
        let recorded = take_recorded_global_hotkeys();
        assert!(recorded.overflowed);
        assert_eq!(recorded.declared.len(), GLOBAL_HOTKEY_DECLARATION_CAP);
        assert!(
            !take_recorded_global_hotkeys().overflowed,
            "the flag resets with the drain"
        );
    }
}
#[cfg(test)]
mod app_theme_tests {
    //! The app theme a DOM is BUILT for (`@theme(<name>)` blocks select by
    //! it in the cascade; a widget's STRUCTURE branches on it at build time).
    //!
    //! A widget's `dom()` has no `LayoutCallbackInfo`, so the theme is
    //! ambient, the seam shape of the style-dependency recorder: the engine
    //! enters a [`ThemeScope`] with the window's theme around every DOM build
    //! of that window, and outside one the app's published theme applies.
    //!
    //! This binary never PUBLISHES an app theme (`set_app_theme` is
    //! `App::create`'s and `CallbackInfo::set_theme`'s; the dll's headless
    //! test covers it), so outside a scope the answer here is the default.
    use azul_css::{dynamic_selector::DEFAULT_APP_THEME, system::SystemStyle, AzString};

    use super::*;
    use crate::{
        app_theme::{app_theme, current_theme, ThemeScope},
        geom::LogicalSize,
    };

    struct Rd {
        image_cache: crate::resources::ImageCache,
        gl: crate::gl::OptionGlContextPtr,
        fonts: rust_fontconfig::FcFontCache,
        style: alloc::sync::Arc<SystemStyle>,
    }
    impl Rd {
        fn new() -> Self {
            Self {
                image_cache: crate::resources::ImageCache::default(),
                gl: crate::gl::OptionGlContextPtr::None,
                fonts: rust_fontconfig::FcFontCache::default(),
                style: alloc::sync::Arc::new(SystemStyle::default()),
            }
        }
        fn ref_data(&self) -> LayoutCallbackInfoRefData<'_> {
            static EN_US: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            let locale = EN_US.get_or_init(|| AzString::from("en-US"));
            static NO_WINDOW_ID: std::sync::OnceLock<AzString> = std::sync::OnceLock::new();
            LayoutCallbackInfoRefData {
                locale,
                accessed_locale: core::cell::Cell::new(false),
                accessed_text_direction: core::cell::Cell::new(false),
                text_direction: crate::callbacks::TextDirection::LeftToRight,
                image_cache: &self.image_cache,
                gl_context: &self.gl,
                system_fonts: &self.fonts,
                system_style: alloc::sync::Arc::clone(&self.style),
                active_route: None,
                monitors: crate::window::MonitorVec::from_const_slice(&[]),
                safe_area: azul_css::system::SafeAreaInsets::default(),
                global_hotkeys: crate::global_hotkey::GlobalHotkeyInfoVec::from_const_slice(&[]),
                window_id: NO_WINDOW_ID.get_or_init(|| AzString::from("")),
            }
        }
    }

    fn info(rd: &LayoutCallbackInfoRefData<'_>) -> LayoutCallbackInfo {
        LayoutCallbackInfo::new(
            rd,
            WindowSize {
                dimensions: LogicalSize::new(800.0, 600.0),
                ..WindowSize::default()
            },
            DarkLightMode::Light,
        )
    }

    #[test]
    fn outside_any_dom_build_the_theme_is_the_apps_which_defaults_to_flat() {
        assert_eq!(app_theme().as_str(), DEFAULT_APP_THEME);
        assert_eq!(current_theme().as_str(), "flat");
    }

    #[test]
    fn a_theme_scope_sets_the_theme_a_dom_is_built_for_and_restores_the_outer_one() {
        assert_eq!(current_theme().as_str(), "flat");
        {
            let _flora = ThemeScope::enter(AzString::from("flora"));
            assert_eq!(current_theme().as_str(), "flora");
            {
                let _inner = ThemeScope::enter(AzString::from("monokai"));
                assert_eq!(current_theme().as_str(), "monokai");
            }
            assert_eq!(
                current_theme().as_str(),
                "flora",
                "leaving the inner scope restores the outer one"
            );
        }
        assert_eq!(
            current_theme().as_str(),
            "flat",
            "and leaving the outer one the app's"
        );
    }

    #[test]
    fn a_theme_scope_belongs_to_the_thread_that_builds() {
        let _flora = ThemeScope::enter(AzString::from("flora"));
        let elsewhere = std::thread::spawn(|| current_theme().as_str().to_string())
            .join()
            .expect("the probe thread");
        assert_eq!(
            elsewhere, "flat",
            "another thread's DOM build is not inside this window's scope"
        );
        assert_eq!(current_theme().as_str(), "flora");
    }

    /// `layout()` reads the app theme it builds for through its info
    /// (`get_theme`, a name); the light / dark mode is the other getter
    /// (`get_mode`).
    #[test]
    fn get_theme_answers_the_scope_and_leaves_the_mode_getter_alone() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let info = info(&rd);
        let _ = take_recorded_style_dependencies();

        assert_eq!(info.get_theme().as_str(), "flat");
        {
            let _flora = ThemeScope::enter(AzString::from("flora"));
            assert_eq!(info.get_theme().as_str(), "flora");
        }
        assert!(
            take_recorded_style_dependencies().is_empty(),
            "a theme switch ALWAYS rebuilds the DOM, so reading the name declares nothing"
        );
        assert_eq!(
            info.get_mode(),
            DarkLightMode::Light,
            "the light / dark mode, as before"
        );
    }

    #[test]
    fn a_theme_switch_has_its_own_relayout_reason() {
        let rd = Rd::new();
        let rd = rd.ref_data();
        let info = LayoutCallbackInfo::new_with_reason(
            &rd,
            WindowSize::default(),
            DarkLightMode::Light,
            RelayoutReason::ThemeChange,
        );
        assert_eq!(info.relayout_reason(), RelayoutReason::ThemeChange);
        assert_ne!(
            RelayoutReason::ThemeChange,
            RelayoutReason::ModeChange,
            "ModeChange is the light / dark mode; the app theme is a DOM rebuild of its own"
        );
    }

    /// CAL3: one layout callback serving several editor windows needs to know
    /// which window it builds - `LayoutCallbackInfo` had no window id.
    #[test]
    fn a_layout_callback_knows_which_window_it_builds() {
        let id = AzString::from("azcalendar-editor-2");
        let rd = Rd::new();
        let mut rd = rd.ref_data();
        rd.window_id = &id;
        let info = LayoutCallbackInfo::new(&rd, WindowSize::default(), DarkLightMode::Light);
        assert_eq!(info.get_window_id().as_str(), "azcalendar-editor-2");
    }
}

#[cfg(test)]
mod build_window_tests {
    //! The window a DOM is BUILT for: what lets a widget whose structure
    //! depends on the window's width (the ribbon scales a tab that does not
    //! fit down to it, as Office's does) ask that width from its `dom()`,
    //! which has no `LayoutCallbackInfo` - the size twin of the theme scope
    //! above. The engine enters a [`WindowSizeScope`] around `layout()`; a
    //! widget asks [`build_window_width_less_than`], which records its
    //! question with the callback's own (`window_width_less_than`), so a
    //! resize that changes the answer re-runs `layout()` and any other
    //! resize re-flows the DOM as it is.
    use super::*;
    use crate::geom::LogicalSize;

    fn width_query(threshold_px: f32, answer: bool) -> SizeQuery {
        SizeQuery {
            axis: SizeQueryAxis::Width,
            op: SizeQueryOp::LessThan,
            threshold_px,
            answer,
        }
    }

    #[test]
    fn a_widget_built_for_a_window_asks_its_width_through_the_recorded_channel() {
        let _ = take_recorded_size_queries();
        let _window = WindowSizeScope::enter(LogicalSize::new(800.0, 600.0));
        assert_eq!(build_window_width_less_than(1000.0), Some(true));
        assert_eq!(
            build_window_width_less_than(800.0),
            Some(false),
            "strictly narrower, as `window_width_less_than` asks"
        );
        let (queries, overflowed) = take_recorded_size_queries();
        assert!(!overflowed);
        assert_eq!(
            queries,
            vec![width_query(1000.0, true), width_query(800.0, false)],
            "both questions are the build's, for a resize to replay"
        );
        assert!(
            queries[1].flips_at(LogicalSize::new(799.0, 600.0)),
            "a window shrinking below the width the widget asked about rebuilds it"
        );
        assert!(
            !queries[0].flips_at(LogicalSize::new(900.0, 600.0)),
            "one that keeps every answer re-flows the DOM it has"
        );
    }

    #[test]
    fn outside_a_dom_build_there_is_no_window_to_ask() {
        let _ = take_recorded_size_queries();
        assert_eq!(build_window_width_less_than(1000.0), None);
        assert!(
            take_recorded_size_queries().0.is_empty(),
            "and nothing is recorded: no layout() replays it"
        );
    }

    #[test]
    fn a_window_size_scope_restores_the_outer_window_and_belongs_to_its_thread() {
        let _outer = WindowSizeScope::enter(LogicalSize::new(800.0, 600.0));
        {
            let _inner = WindowSizeScope::enter(LogicalSize::new(400.0, 300.0));
            assert_eq!(build_window_width_less_than(500.0), Some(true));
        }
        assert_eq!(
            build_window_width_less_than(500.0),
            Some(false),
            "leaving the inner scope restores the outer window"
        );
        let elsewhere = std::thread::spawn(|| build_window_width_less_than(500.0))
            .join()
            .expect("the probe thread");
        assert_eq!(
            elsewhere, None,
            "another thread's DOM build is not inside this window's scope"
        );
        let _ = take_recorded_size_queries();
    }
}
