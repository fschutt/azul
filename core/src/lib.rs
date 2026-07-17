//! Shared datatypes for azul-* crates
//!
//! `azul-core` provides the platform-independent core types used throughout
//! the Azul toolkit. Key modules include [`dom`] for DOM construction,
//! [`callbacks`] for event callback types, [`styled_dom`] for the CSSOM,
//! and [`window`] for OS windowing abstractions.
//!
//! This crate depends on [`azul_css`] for CSS property definitions and is
//! consumed by `azul-layout`, `azul-dll`, and the platform shell crates.
//! It supports `no_std` environments via `#![cfg_attr(not(feature = "std"), no_std)]`.

#![cfg_attr(not(feature = "std"), no_std)]
// Lint policy: deny correctness/safety issues, warn on style
#![deny(unused_must_use)]
#![warn(clippy::all)]
// Extreme-lint lockdown: all clippy groups plus opt-in rustc lints, enforced as
// -D warnings on library code by the CI clippy job. Test builds are exempt via
// cfg(not(test)) below since the set is high-noise and low-value on unit and
// generated tests; clippy::all correctness still applies to test code.
// (clippy::restriction wholesale + unused_results + box_pointers deliberately
// omitted — contradictory / overwhelmingly noisy by design.)
#![cfg_attr(not(test), warn(
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo,
    // missing_docs,  // TODO(docs): re-enable as a dedicated final docs pass; disabled
    //                // for now so the cleanup focuses on code-quality lints, not doc debt.
    missing_debug_implementations,
    missing_copy_implementations,
    unreachable_pub,
    unused_qualifications,
    unused_lifetimes,
    unused_import_braces,
    unused_macro_rules,
    unused_crate_dependencies,
    meta_variable_misuse,
    trivial_casts,
    trivial_numeric_casts,
    elided_lifetimes_in_paths,
    single_use_lifetimes,
    variant_size_differences,
    non_ascii_idents,
    unsafe_op_in_unsafe_fn,
    let_underscore_drop,
))]
// `multiple_crate_versions` (implied by clippy::cargo) flags transitive
// dependency-version dups that cannot be resolved in azul's own source:
// `syn` 1.0.x ↔ 2.0.x (the proc-macro ecosystem is mid-migration; both are
// pulled in transitively). Documented allow — re-audit when the dep tree aligns.
#![allow(clippy::multiple_crate_versions)]
#![allow(
    clippy::non_canonical_partial_ord_impl,
    clippy::legacy_numeric_constants,
    clippy::should_implement_trait,
    clippy::result_unit_err,
    clippy::ptr_as_ptr,
    clippy::too_many_arguments,
    clippy::type_complexity,
    unused_imports,
    unused_variables,
    unused_mut,
    unused_parens,
    dead_code,
    unused_doc_comments,
    unused_assignments,                    // compact_cache_builder incremental updates
    unexpected_cfgs,
    unpredictable_function_pointer_comparisons, // intentional in dom callback comparison
    improper_ctypes_definitions,           // xml component fns use Rust fn pointers internally
    static_mut_refs,                       // TODO: migrate to OnceLock for Rust 2024
)]

// `extern crate` + `#[macro_use]` required for `no_std` support:
// makes `core` and `alloc` macros available without `use` imports.
#[macro_use]
extern crate core;
#[macro_use]
extern crate alloc;
#[macro_use]
extern crate azul_css;

/// Internal macros for `Vec`, `Option`, and callback boilerplate.
///
#[macro_use]
pub mod macros;
/// Debug logging system with category filtering.
#[macro_use]
pub mod debug;
/// SQL database POD types — `DbValue` + `DbRows` (engine-agnostic). The
/// `Db` handle + SQLite engine live in `azul_dll` behind `db-sqlite`.
pub mod db;
/// Unified `AZ_PROFILE` gate for memory and CPU profiling instrumentation.
pub mod profile;
/// `no_std`-friendly synchronization primitives.
///
/// In `std` builds these re-export the matching `std::sync` types. In
/// `no_std` builds they provide minimal spinlock-backed equivalents
/// implementing only the API surface azul-core relies on.
pub mod sync {
    #[cfg(feature = "std")]
    pub use std::sync::OnceLock;

    #[cfg(not(feature = "std"))]
    pub use self::nostd::OnceLock;

    #[cfg(not(feature = "std"))]
    mod nostd {
        use core::cell::UnsafeCell;
        use core::sync::atomic::{AtomicU8, Ordering};

        const UNINIT: u8 = 0;
        const BUSY: u8 = 1;
        const READY: u8 = 2;

        /// Minimal `no_std` `OnceLock` mirroring the slice of the std API used
        /// by azul-core (`new`, `get`, `get_or_init`).
        pub struct OnceLock<T> {
            state: AtomicU8,
            value: UnsafeCell<Option<T>>,
        }

        unsafe impl<T: Send + Sync> Sync for OnceLock<T> {}
        unsafe impl<T: Send> Send for OnceLock<T> {}

        impl<T> OnceLock<T> {
            pub const fn new() -> Self {
                OnceLock {
                    state: AtomicU8::new(UNINIT),
                    value: UnsafeCell::new(None),
                }
            }

            pub fn get(&self) -> Option<&T> {
                if self.state.load(Ordering::Acquire) == READY {
                    unsafe { (*self.value.get()).as_ref() }
                } else {
                    None
                }
            }

            pub fn get_or_init<F: FnOnce() -> T>(&self, f: F) -> &T {
                if let Some(v) = self.get() {
                    return v;
                }
                // Contend for the right to initialize.
                while self
                    .state
                    .compare_exchange(UNINIT, BUSY, Ordering::Acquire, Ordering::Acquire)
                    .is_err()
                {
                    if self.state.load(Ordering::Acquire) == READY {
                        return self.get().expect("OnceLock ready");
                    }
                    core::hint::spin_loop();
                }
                unsafe {
                    *self.value.get() = Some(f());
                }
                self.state.store(READY, Ordering::Release);
                self.get().expect("OnceLock initialized")
            }
        }

        impl<T> Default for OnceLock<T> {
            fn default() -> Self {
                Self::new()
            }
        }

        impl<T: core::fmt::Debug> core::fmt::Debug for OnceLock<T> {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple("OnceLock").field(&self.get()).finish()
            }
        }

        impl<T: Clone> Clone for OnceLock<T> {
            fn clone(&self) -> Self {
                let new = OnceLock::new();
                if let Some(v) = self.get() {
                    let _ = new.get_or_init(|| v.clone());
                }
                new
            }
        }

        impl<T: PartialEq> PartialEq for OnceLock<T> {
            fn eq(&self, other: &Self) -> bool {
                self.get() == other.get()
            }
        }
    }
}

/// `no_std`-friendly default hasher used for change-detection hashing.
///
/// In `std` builds this re-exports `std::hash::DefaultHasher` so behaviour
/// is unchanged. In `no_std` builds it provides a small deterministic
/// `FxHasher`-style hasher implementing `core::hash::Hasher`. The values are
/// only required to be stable within a single process run (they back diffing /
/// change detection), not to match `std`'s `SipHash` output.
pub mod hash {
    #[cfg(feature = "std")]
    pub use std::hash::DefaultHasher;

    #[cfg(not(feature = "std"))]
    pub use self::nostd::DefaultHasher;

    #[cfg(not(feature = "std"))]
    mod nostd {
        use core::hash::Hasher;

        const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
        const ROTATE: u32 = 5;

        /// FxHasher-style `no_std` hasher. Not DoS-resistant; used purely for
        /// in-process change detection.
        #[derive(Default)]
        pub struct DefaultHasher {
            hash: u64,
        }

        impl DefaultHasher {
            pub fn new() -> Self {
                DefaultHasher { hash: 0 }
            }

            #[inline]
            fn add(&mut self, word: u64) {
                self.hash = (self.hash.rotate_left(ROTATE) ^ word).wrapping_mul(SEED);
            }
        }

        impl Hasher for DefaultHasher {
            #[inline]
            fn finish(&self) -> u64 {
                self.hash
            }

            #[inline]
            fn write(&mut self, bytes: &[u8]) {
                for chunk in bytes.chunks(8) {
                    let mut buf = [0u8; 8];
                    buf[..chunk.len()].copy_from_slice(chunk);
                    self.add(u64::from_le_bytes(buf));
                }
            }

            #[inline]
            fn write_u8(&mut self, i: u8) {
                self.add(i as u64);
            }
            #[inline]
            fn write_u64(&mut self, i: u64) {
                self.add(i);
            }
            #[inline]
            fn write_usize(&mut self, i: usize) {
                self.add(i as u64);
            }
        }
    }
}
/// Callback types: layout, event, timer, thread, and focus handling.
#[macro_use]
pub mod callbacks;
/// Host-language callback invoker registry.
///
/// The C-ABI surface managed-FFI bindings (Lua, Ruby, …) use to register one
/// per-kind invoker + a single shared releaser, so callbacks can be created via
/// `_createFromHostHandle` without the host having to generate trampolines for
/// struct-by-value signatures their FFI library can't handle.
#[macro_use]
pub mod host_invoker;
/// Accessibility types for screen-reader integration (AccessKit).
pub mod a11y;
/// Audio POD types — `AudioConfig` (stream format) + `AudioFrame` (interleaved
/// f32 samples).
///
/// The unit captured from the mic, played back, and (P8) shared
/// over UDP. Backend (rodio / cpal / AVAudioEngine / AAudio) lives dll-side.
pub mod audio;
/// Biometric-auth POD types — `BiometricKind` + `BiometricResult` + `BiometricPrompt`.
///
/// Stateful manager lives in `azul_layout::managers::biometric`.
pub mod biometric;
/// Camera-capture POD types — `CaptureStreamId` + `CameraConfig` +
/// `CameraFacing` + `StreamState` + … .
///
/// The stateful `CameraStream` /
/// `CameraManager` (which own the shared `ImageRef` texture) live in
/// `azul_layout::managers::camera`.
pub mod camera;
/// Converts `CssPropertyCache` into compact three-tier numeric cache.
pub mod compact;
/// Linear-time DOM diffing for incremental updates.
pub mod diff;
/// DOM construction: `Dom`, `NodeData`, `NodeType`, and the CSS-in-Rust API.
pub mod dom;
/// Drag context for text selection, scrollbar, node, and window drags.
pub mod drag;
/// Event filtering: mouse, keyboard, window, and synthetic events.
pub mod events;
/// Gamepad POD types — `GamepadId` + `GamepadButton` + `GamepadAxis` +
/// `GamepadState`.
///
/// Stateful manager lives in `azul_layout::managers::gamepad`.
pub mod gamepad;
/// Geolocation POD types — `LocationFix` + `GeolocationProbeConfig`.
///
/// Stateful manager lives in `azul_layout::managers::geolocation`.
pub mod geolocation;
/// Logical and physical coordinate types (`LogicalSize`, `PhysicalPosition`, etc.).
pub mod geom;
/// OpenGL context wrappers, shader compilation, and texture cache.
///
pub mod gl;
/// FXAA (Fast Approximate Anti-Aliasing) shader.
pub mod gl_fxaa;
/// OpenGL constants (GL 1.1 through GL 4.x).
pub mod glconst;
/// GPU value cache for CSS transforms and opacity.
pub mod gpu;
/// Hit-test results (which DOM nodes are under the cursor) + the type-safe
/// hit-test tag system for compositor integration (merged from `hit_test_tag`).
///
pub mod hit_test;
/// Icon provider system for loading icons from fonts, images, or zip packs.
pub mod icon;
/// Arena-based node tree storage and hierarchy management.
pub mod id;
/// JSON value types for the C API (no serde dependency).
pub mod json;
/// System-keyring POD types — `KeyringRequest` + `KeyringResult`.
///
/// Stateful manager lives in `azul_layout::managers::keyring`.
pub mod keyring;
/// Menu system: context menus, dropdown menus, and menu bars.
pub mod menu;
/// Paged-media primitives: the `FragmentationContext` (continuous vs. paged) and
/// `PageMargins`. The pagination/slicing logic lives in `azul_layout::solver3`.
pub mod paged;
/// SVG `d=""` path data parser.
pub mod path_parser;
/// CSS property cache for efficient per-node style resolution.
pub mod prop_cache;
/// Type-erased, ref-counted smart pointer with runtime borrow checking.
pub mod refany;
/// Resource management: font/image loading, caching, and garbage collection.
pub mod resources;
/// Screen-capture POD types — `ScreenCaptureSource` + `ScreenCaptureConfig`.
///
/// Symmetric to the camera surface (a "dumb widget" in
/// `azul_layout::widgets::screencap`); reuses `camera`'s capture status types.
pub mod screencap;
/// Text selection and cursor positioning for inline content.
pub mod selection;
/// Motion-sensor POD types — `SensorKind` + `SensorReading`.
///
/// Stateful manager lives in `azul_layout::managers::sensors`.
pub mod sensors;
/// CSS cascade: selector matching, specificity, and property inheritance.
pub mod style;
/// `StyledDom` — the result of applying CSS to a DOM tree (the CSSOM).
pub mod styled_dom;
/// SVG rendering, path tessellation, and geometric operations.
pub mod svg;
/// Timer, thread, and async task management.
pub mod task;
/// 3D transform matrix computation for CSS transforms.
pub mod transform;
/// Built-in user-agent default stylesheet.
pub mod ua_css;
/// Default font/text constants and small geometry helpers for layout.
pub mod ui_solver;
/// URL POD type (`Url`/`UrlParseError`); parsing gated behind the `url` feature.
pub mod url;
/// Video-playback POD types — `VideoConfig` (source URL + autoplay/loop).
///
/// Same "dumb widget" architecture (`azul_layout::widgets::video`); decoded
/// via vk-video into the shared GL texture.
pub mod video;
/// Window configuration, input state, and platform-specific options.
pub mod window;
/// XML and XHTML parsing for declarative UI definitions.
pub mod xml;

/// Ordered map alias used throughout `azul-core`.
///
/// This is backed by `BTreeMap` (not a hash map) because the `core` crate
/// supports `no_std`, where `HashMap` is unavailable. The webrender crates
/// define their own `FastHashMap` using `HashMap` + `FxHasher`.
pub type OrderedMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;

#[cfg(test)]
#[allow(clippy::pedantic, clippy::nursery)]
mod autotest_generated {
    use alloc::{boxed::Box, string::String, vec::Vec};
    use core::{
        cell::Cell,
        hash::{Hash, Hasher},
    };

    use super::{hash::DefaultHasher, sync::OnceLock, FastBTreeSet, OrderedMap};

    // NOTE: `sync::OnceLock` and `hash::DefaultHasher` are *aliases*: with the
    // (default) `std` feature they re-export `std::sync::OnceLock` /
    // `std::hash::DefaultHasher`; without it they resolve to the hand-written
    // `no_std` shims in this file. Tests below are split accordingly:
    //   * un-gated  -> the API contract BOTH impls must satisfy,
    //   * cfg-gated -> behaviour that is specific to one impl.
    // `DefaultHasher::add` is private to the private `hash::nostd` module, so it
    // is not nameable from here; `write_u64` forwards to it 1:1 and is used as
    // the proxy for the numeric/overflow cases.

    // ---------------------------------------------------------------
    // OnceLock — constructor / getter invariants
    // ---------------------------------------------------------------

    #[test]
    fn oncelock_new_is_empty() {
        let cell: OnceLock<u32> = OnceLock::new();
        assert!(cell.get().is_none());
        // getter must stay pure: repeated reads never initialize
        assert!(cell.get().is_none());
    }

    #[test]
    fn oncelock_new_is_usable_in_const_context() {
        static CELL: OnceLock<u64> = OnceLock::new();
        assert!(CELL.get().is_none());
        assert_eq!(*CELL.get_or_init(|| u64::MAX), u64::MAX);
        assert_eq!(CELL.get().copied(), Some(u64::MAX));
    }

    #[test]
    fn oncelock_default_matches_new() {
        let cell: OnceLock<Vec<u8>> = OnceLock::default();
        assert!(cell.get().is_none());
    }

    #[test]
    fn oncelock_get_or_init_runs_closure_exactly_once() {
        let calls = Cell::new(0usize);
        let cell: OnceLock<u32> = OnceLock::new();

        assert_eq!(*cell.get_or_init(|| { calls.set(calls.get() + 1); 7 }), 7);
        // The second/third call must return the FIRST value and never re-run `f`.
        assert_eq!(*cell.get_or_init(|| { calls.set(calls.get() + 1); 9 }), 7);
        assert_eq!(*cell.get_or_init(|| { calls.set(calls.get() + 1); 11 }), 7);
        assert_eq!(calls.get(), 1);
        assert_eq!(cell.get().copied(), Some(7));
    }

    #[test]
    fn oncelock_get_and_get_or_init_alias_the_same_storage() {
        let cell: OnceLock<u32> = OnceLock::new();
        let a: *const u32 = cell.get_or_init(|| 1);
        let b: *const u32 = cell.get().expect("initialized");
        let c: *const u32 = cell.get_or_init(|| 2);
        // The value must never be moved/duplicated by a second init attempt.
        assert_eq!(a, b);
        assert_eq!(a, c);
    }

    #[test]
    fn oncelock_holds_zero_sized_type() {
        // ZST: `Option<()>` has no payload bits, so a naive impl can confuse
        // "initialized" with "None".
        let cell: OnceLock<()> = OnceLock::new();
        assert!(cell.get().is_none());
        cell.get_or_init(|| ());
        assert!(cell.get().is_some());
    }

    #[test]
    fn oncelock_holds_large_payload() {
        let cell: OnceLock<Box<[u8]>> = OnceLock::new();
        let v = cell.get_or_init(|| alloc::vec![0xABu8; 1 << 20].into_boxed_slice());
        assert_eq!(v.len(), 1 << 20);
        assert!(v.iter().all(|b| *b == 0xAB));
        assert_eq!(cell.get().map(|b| b.len()), Some(1 << 20));
    }

    #[test]
    fn oncelock_holds_nan_without_eq_confusion() {
        let cell: OnceLock<f64> = OnceLock::new();
        // `NaN != NaN`, so initialization must be tracked by state, not by
        // comparing the payload against a sentinel.
        assert!(cell.get_or_init(|| f64::NAN).is_nan());
        assert!(cell.get().is_some_and(|f| f.is_nan()));
        // A second init must not overwrite the stored NaN with 1.0.
        assert!(cell.get_or_init(|| 1.0).is_nan());
    }

    #[test]
    fn oncelock_clone_copies_state_not_aliases_it() {
        let cell: OnceLock<String> = OnceLock::new();

        let empty = cell.clone();
        assert!(empty.get().is_none());
        // Initializing the source must not retro-fill an earlier clone.
        cell.get_or_init(|| String::from("azul"));
        assert!(empty.get().is_none());

        let full = cell.clone();
        assert_eq!(full.get().map(String::as_str), Some("azul"));
        // Distinct storage: the clone must own its own allocation.
        assert_ne!(
            cell.get().expect("init") as *const String,
            full.get().expect("init") as *const String
        );
    }

    #[test]
    fn oncelock_eq_compares_contents() {
        let a: OnceLock<u32> = OnceLock::new();
        let b: OnceLock<u32> = OnceLock::new();
        assert_eq!(a, b); // both empty

        a.get_or_init(|| 5);
        assert_ne!(a, b); // Some(5) vs None

        b.get_or_init(|| 5);
        assert_eq!(a, b);

        let c: OnceLock<u32> = OnceLock::new();
        c.get_or_init(|| 6);
        assert_ne!(a, c);
    }

    #[cfg(feature = "std")]
    #[test]
    fn oncelock_concurrent_get_or_init_initializes_exactly_once() {
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Barrier,
        };

        const THREADS: usize = 8;

        let cell: OnceLock<usize> = OnceLock::new();
        let inits = AtomicUsize::new(0);
        let gate = Barrier::new(THREADS);

        std::thread::scope(|s| {
            for id in 0..THREADS {
                let (cell, inits, gate) = (&cell, &inits, &gate);
                let _ = s.spawn(move || {
                    gate.wait(); // maximize contention on the CAS
                    let v = *cell.get_or_init(|| {
                        inits.fetch_add(1, Ordering::SeqCst);
                        id
                    });
                    // Every racer must observe the same winner.
                    assert_eq!(v, *cell.get().expect("initialized after get_or_init"));
                    v
                });
            }
        });

        assert_eq!(inits.load(Ordering::SeqCst), 1);
        let winner = cell.get().copied().expect("initialized");
        assert!(winner < THREADS);
    }

    // The `std` OnceLock documents that a panicking `f` leaves the cell
    // *uninitialized* (and re-initializable) rather than poisoned.
    //
    // The `no_std` shim in this file does NOT hold this property: it leaves
    // `state == BUSY`, so any later `get_or_init` spins forever. This test is
    // therefore std-gated on purpose — running it under `no_std` would hang the
    // test binary instead of failing it.
    #[cfg(feature = "std")]
    #[test]
    fn oncelock_panicking_initializer_leaves_cell_reusable() {
        let cell: OnceLock<u32> = OnceLock::new();

        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {})); // keep the expected panic quiet
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cell.get_or_init(|| panic!("initializer blew up"));
        }));
        std::panic::set_hook(prev);

        assert!(caught.is_err(), "the panic must propagate to the caller");
        assert!(cell.get().is_none(), "cell must remain uninitialized");
        assert_eq!(*cell.get_or_init(|| 42), 42, "cell must still be usable");
    }

    // ---------------------------------------------------------------
    // DefaultHasher — construction / determinism
    // ---------------------------------------------------------------

    fn hash_bytes(bytes: &[u8]) -> u64 {
        let mut h = DefaultHasher::new();
        h.write(bytes);
        h.finish()
    }

    fn hash_u64(word: u64) -> u64 {
        let mut h = DefaultHasher::new();
        h.write_u64(word);
        h.finish()
    }

    #[test]
    fn hasher_new_and_default_agree_and_are_deterministic() {
        assert_eq!(DefaultHasher::new().finish(), DefaultHasher::new().finish());
        assert_eq!(
            DefaultHasher::new().finish(),
            DefaultHasher::default().finish()
        );
    }

    #[test]
    fn hasher_is_deterministic_within_a_run() {
        assert_eq!(hash_bytes(b"azul"), hash_bytes(b"azul"));
        assert_eq!(hash_u64(0xDEAD_BEEF_CAFE_F00D), hash_u64(0xDEAD_BEEF_CAFE_F00D));
    }

    #[test]
    fn hasher_distinguishes_different_inputs() {
        assert_ne!(hash_bytes(b"a"), hash_bytes(b"b"));
        assert_ne!(hash_u64(0), hash_u64(1));
    }

    #[test]
    fn hasher_is_order_sensitive() {
        let mut a = DefaultHasher::new();
        a.write_u64(1);
        a.write_u64(2);

        let mut b = DefaultHasher::new();
        b.write_u64(2);
        b.write_u64(1);

        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn hasher_finish_does_not_consume_state() {
        let mut h = DefaultHasher::new();
        h.write_u64(7);
        let first = h.finish();
        // `finish` must be a pure read: calling it twice returns the same value.
        assert_eq!(first, h.finish());
        // ...and further writes must keep mutating the same running state.
        h.write_u64(7);
        assert_ne!(first, h.finish());
    }

    // ---------------------------------------------------------------
    // DefaultHasher — numeric limits / overflow (exercises the private `add`
    // via its 1:1 forwarders `write_u64` / `write_usize` / `write_u8`)
    // ---------------------------------------------------------------

    #[test]
    fn hasher_handles_integer_limits_without_panicking() {
        // `add` does a `wrapping_mul`; a debug build must not overflow-panic.
        for word in [
            0u64,
            1,
            u64::MAX,
            u64::MAX - 1,
            i64::MIN as u64, // 0x8000_0000_0000_0000 — "negative" bit pattern
            i64::MAX as u64,
            -1i64 as u64,
            1 << 63,
            usize::MAX as u64,
        ] {
            let h = hash_u64(word);
            // deterministic + no panic; value itself is impl-defined
            assert_eq!(h, hash_u64(word));
        }

        let mut h = DefaultHasher::new();
        h.write_usize(usize::MAX);
        h.write_usize(0);
        h.write_u8(u8::MAX);
        h.write_u8(0);
        let _ = h.finish();
    }

    #[test]
    fn hasher_repeated_max_words_do_not_overflow_panic() {
        // Hammer the wrapping rotate/xor/multiply chain: every iteration
        // overflows u64. Must wrap, never panic (even in a debug profile).
        let mut h = DefaultHasher::new();
        for _ in 0..10_000 {
            h.write_u64(u64::MAX);
        }
        let a = h.finish();

        let mut h2 = DefaultHasher::new();
        for _ in 0..10_000 {
            h2.write_u64(u64::MAX);
        }
        assert_eq!(a, h2.finish(), "overflowing chain must stay deterministic");
    }

    #[test]
    fn hasher_zero_words_are_deterministic() {
        let mut h = DefaultHasher::new();
        for _ in 0..1_000 {
            h.write_u64(0);
        }
        let a = h.finish();

        let mut h2 = DefaultHasher::new();
        for _ in 0..1_000 {
            h2.write_u64(0);
        }
        assert_eq!(a, h2.finish());
    }

    // ---------------------------------------------------------------
    // DefaultHasher — `write` chunking / boundaries / unicode
    // ---------------------------------------------------------------

    #[test]
    fn hasher_write_empty_slice_does_not_panic() {
        let mut h = DefaultHasher::new();
        h.write(&[]);
        h.write(&[]);
        let a = h.finish();

        let mut h2 = DefaultHasher::new();
        h2.write(&[]);
        h2.write(&[]);
        assert_eq!(a, h2.finish());
    }

    #[test]
    fn hasher_write_covers_every_chunk_boundary() {
        // The `no_std` impl walks `chunks(8)` and zero-pads the tail; lengths
        // 0..=24 cover empty, short, exact-multiple and ragged-tail cases.
        let data: Vec<u8> = (0u8..=24).collect();
        for len in 0..=24usize {
            let slice = &data[..len];
            assert_eq!(hash_bytes(slice), hash_bytes(slice), "len {len}");
        }
        // A short slice must not collide with the same slice explicitly padded
        // out past the next 8-byte chunk boundary.
        assert_ne!(hash_bytes(&[1u8]), hash_bytes(&[1u8, 0, 0, 0, 0, 0, 0, 0, 0]));
    }

    // `write` must not swallow a trailing zero byte: `[1]` and `[1, 0]` are
    // different inputs and must hash differently.
    //
    // The `no_std` shim FAILS this: it zero-pads the final `chunks(8)` chunk
    // and mixes in no length, so `[1]` and `[1, 0]` both become the word
    // `0x0000_0000_0000_0001` — a guaranteed collision for every pair of byte
    // strings differing only in trailing zeros. Kept as a live assertion for
    // the (default) `std` build and `ignore`d rather than weakened under
    // `no_std`; see the autotest report.
    #[cfg_attr(
        not(feature = "std"),
        ignore = "no_std DefaultHasher zero-pads without length mixing: hash([1]) == hash([1, 0])"
    )]
    #[test]
    fn hasher_write_does_not_swallow_trailing_zero_bytes() {
        assert_ne!(hash_bytes(&[1u8]), hash_bytes(&[1u8, 0]));
        assert_ne!(hash_bytes(b"az"), hash_bytes(b"az\0"));
        assert_ne!(hash_bytes(&[]), hash_bytes(&[0u8]));
    }

    #[test]
    fn hasher_handles_huge_input() {
        let big: Vec<u8> = (0..(1 << 16)).map(|i| (i % 251) as u8).collect();
        let a = hash_bytes(&big);
        assert_eq!(a, hash_bytes(&big));

        // A single flipped byte in the middle must change the digest.
        let mut flipped = big.clone();
        flipped[1 << 15] ^= 0xFF;
        assert_ne!(a, hash_bytes(&flipped));
    }

    #[test]
    fn hasher_handles_unicode_and_nul_bytes() {
        for s in [
            "",
            "\u{0}",
            "ascii",
            "héllo wörld",
            "日本語テキスト",
            "🦀🔥👨‍👩‍👧‍👦",
            "a\u{0}b",
            "\u{FEFF}bom",
            "\u{10FFFF}",
        ] {
            let mut h = DefaultHasher::new();
            s.hash(&mut h);
            let a = h.finish();

            let mut h2 = DefaultHasher::new();
            s.hash(&mut h2);
            assert_eq!(a, h2.finish(), "unstable hash for {s:?}");
        }

        // Interior NUL must not truncate the input (C-string style bug).
        let mut a = DefaultHasher::new();
        "a\u{0}b".hash(&mut a);
        let mut b = DefaultHasher::new();
        "a".hash(&mut b);
        assert_ne!(a.finish(), b.finish());
    }

    #[test]
    fn hasher_respects_eq_hash_contract_for_std_types() {
        fn digest<T: Hash>(t: &T) -> u64 {
            let mut h = DefaultHasher::new();
            t.hash(&mut h);
            h.finish()
        }

        // Equal values must hash equal.
        assert_eq!(digest(&String::from("x")), digest(&String::from("x")));
        assert_eq!(digest(&alloc::vec![1u64, 2, 3]), digest(&alloc::vec![1u64, 2, 3]));
        assert_eq!(digest(&(1u8, "a")), digest(&(1u8, "a")));

        // Length must be part of the digest: [1,2] vs [1,2,0] must differ...
        assert_ne!(digest(&alloc::vec![1u8, 2]), digest(&alloc::vec![1u8, 2, 0]));
        // ...and prefix-concatenation must not collide ("ab" vs "a"+"b" fields).
        assert_ne!(digest(&("ab", "")), digest(&("a", "b")));
    }

    // ---------------------------------------------------------------
    // `no_std` shim internals: exact FxHasher-style formula of the private
    // `add`, reached through its 1:1 forwarder `write_u64`.
    // ---------------------------------------------------------------

    #[cfg(not(feature = "std"))]
    #[test]
    fn nostd_hasher_add_matches_documented_formula() {
        const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;
        const ROTATE: u32 = 5;

        fn expect(words: &[u64]) -> u64 {
            words
                .iter()
                .fold(0u64, |h, w| (h.rotate_left(ROTATE) ^ w).wrapping_mul(SEED))
        }

        assert_eq!(DefaultHasher::new().finish(), 0, "fresh state must be 0");

        for words in [
            &[0u64][..],
            &[1][..],
            &[u64::MAX][..],
            &[i64::MIN as u64][..],
            &[u64::MAX, u64::MAX, u64::MAX][..],
            &[0, u64::MAX, 0, 1 << 63][..],
        ] {
            let mut h = DefaultHasher::new();
            for w in words {
                h.write_u64(*w);
            }
            assert_eq!(h.finish(), expect(words), "formula drift for {words:?}");
        }
    }

    #[cfg(not(feature = "std"))]
    #[test]
    fn nostd_hasher_zero_is_an_absorbing_state() {
        // Documented FxHasher weakness, asserted so it stays *intentional*:
        // from a zero state, hashing zero words keeps the state at zero
        // ((0.rotate_left(5) ^ 0) * SEED == 0).
        let mut h = DefaultHasher::new();
        for _ in 0..64 {
            h.write_u64(0);
        }
        assert_eq!(h.finish(), 0);

        // Leading zero words are therefore invisible: hash([0, x]) == hash([x]).
        let mut a = DefaultHasher::new();
        a.write_u64(0);
        a.write_u64(0xABCD);
        let mut b = DefaultHasher::new();
        b.write_u64(0xABCD);
        assert_eq!(a.finish(), b.finish());
    }

    #[cfg(not(feature = "std"))]
    #[test]
    fn nostd_hasher_write_empty_slice_is_a_noop() {
        // `chunks(8)` over an empty slice yields nothing, so the state is untouched.
        let mut h = DefaultHasher::new();
        h.write(b"seed");
        let before = h.finish();
        h.write(&[]);
        assert_eq!(h.finish(), before);
    }

    // ---------------------------------------------------------------
    // Public type aliases — ordering / dedup invariants
    // ---------------------------------------------------------------

    #[test]
    fn ordered_map_iterates_in_key_order() {
        let mut m: OrderedMap<i64, &str> = OrderedMap::new();
        for (k, v) in [(i64::MAX, "max"), (0, "zero"), (i64::MIN, "min"), (-1, "neg")] {
            let _ = m.insert(k, v);
        }
        let keys: Vec<i64> = m.keys().copied().collect();
        assert_eq!(keys, alloc::vec![i64::MIN, -1, 0, i64::MAX]);

        // Re-insert must overwrite, not duplicate.
        assert_eq!(m.insert(0, "zero2"), Some("zero"));
        assert_eq!(m.len(), 4);
        assert_eq!(m.get(&0).copied(), Some("zero2"));
    }

    #[test]
    fn fast_btree_set_dedups_and_orders() {
        let mut s: FastBTreeSet<u32> = FastBTreeSet::new();
        assert!(s.insert(u32::MAX));
        assert!(s.insert(0));
        assert!(!s.insert(0), "duplicate insert must report false");
        assert!(s.insert(1));

        assert_eq!(s.len(), 3);
        assert_eq!(s.iter().copied().collect::<Vec<u32>>(), alloc::vec![0, 1, u32::MAX]);
        assert!(s.contains(&u32::MAX));
        assert!(!s.contains(&2));
    }
}
