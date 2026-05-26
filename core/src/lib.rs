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
    mismatched_lifetime_syntaxes,
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
                OnceLock { state: AtomicU8::new(UNINIT), value: UnsafeCell::new(None) }
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
/// change detection), not to match `std`'s SipHash output.
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
/// Host-language callback invoker registry — the C-ABI surface managed-FFI
/// bindings (Lua, Ruby, …) use to register one per-kind invoker + a single
/// shared releaser, so callbacks can be created via `_createFromHostHandle`
/// without the host having to generate trampolines for struct-by-value
/// signatures their FFI library can't handle.
#[macro_use]
pub mod host_invoker;
/// Accessibility types for screen-reader integration (AccessKit).
pub mod a11y;
/// DOM construction: `Dom`, `NodeData`, `NodeType`, and the CSS-in-Rust API.
pub mod dom;
/// Drag context for text selection, scrollbar, node, and window drags.
pub mod drag;
/// Icon provider system for loading icons from fonts, images, or zip packs.
pub mod icon;
/// Resource management: font/image loading, caching, and garbage collection.
pub mod resources;
/// Text selection and cursor positioning for inline content.
pub mod selection;
/// Motion-sensor POD types — `SensorKind` + `SensorReading`.
/// Stateful manager lives in `azul_layout::managers::sensors`.
pub mod sensors;
/// Linear-time DOM diffing for incremental updates.
pub mod diff;
/// CSS animation and transition configuration.
pub mod animation;
/// Event filtering: mouse, keyboard, window, and synthetic events.
pub mod events;
/// Biometric-auth POD types — `BiometricKind` + `BiometricResult` + `BiometricPrompt`.
/// Stateful manager lives in `azul_layout::managers::biometric`.
pub mod biometric;
/// Geolocation POD types — `LocationFix` + `GeolocationProbeConfig`.
/// Stateful manager lives in `azul_layout::managers::geolocation`.
pub mod geolocation;
/// Gamepad POD types — `GamepadId` + `GamepadButton` + `GamepadAxis` +
/// `GamepadState`. Stateful manager lives in `azul_layout::managers::gamepad`.
pub mod gamepad;
/// Camera-capture POD types — `CaptureStreamId` + `CameraConfig` +
/// `CameraFacing` + `StreamState` + … . The stateful `CameraStream` /
/// `CameraManager` (which own the shared `ImageRef` texture) live in
/// `azul_layout::managers::camera`.
pub mod camera;
/// Screen-capture POD types — `ScreenCaptureSource` + `ScreenCaptureConfig`.
/// Symmetric to the camera surface (a "dumb widget" in
/// `azul_layout::widgets::screencap`); reuses `camera`'s capture status types.
pub mod screencap;
/// Video-playback POD types — `VideoConfig` (source URL + autoplay/loop).
/// Same "dumb widget" architecture (`azul_layout::widgets::video`); decoded
/// via vk-video into the shared GL texture.
pub mod video;
/// Audio POD types — `AudioConfig` (stream format) + `AudioFrame` (interleaved
/// f32 samples). The unit captured from the mic, played back, and (P8) shared
/// over UDP. Backend (rodio / cpal / AVAudioEngine / AAudio) lives dll-side.
pub mod audio;
/// UDP chunked-message framing (P8): split a >MTU payload into sequenced
/// datagrams + reassemble them, tolerating reorder + loss. Pure logic the
/// dll's `Udp` handle builds on; unit-tested here. See `udp_framing.rs`.
pub mod udp_framing;
/// Logical and physical coordinate types (`LogicalSize`, `PhysicalPosition`, etc.).
pub mod geom;
/// OpenGL context wrappers, shader compilation, and texture cache.
pub mod gl;
/// FXAA (Fast Approximate Anti-Aliasing) shader.
pub mod gl_fxaa;
/// OpenGL constants (GL 1.1 through GL 4.x).
pub mod glconst;
/// GPU value cache for CSS transforms and opacity.
pub mod gpu;
/// Hit-test results: which DOM nodes are under the cursor.
pub mod hit_test;
/// Type-safe hit-test tag system for compositor integration.
pub mod hit_test_tag;
/// Arena-based node tree storage and hierarchy management.
pub mod id;
/// System-keyring POD types — `KeyringRequest` + `KeyringResult`.
/// Stateful manager lives in `azul_layout::managers::keyring`.
pub mod keyring;
/// Menu system: context menus, dropdown menus, and menu bars.
pub mod menu;
/// CSS property cache for efficient per-node style resolution.
pub mod prop_cache;
/// Converts `CssPropertyCache` into compact three-tier numeric cache.
pub mod compact_cache_builder;
/// Type-erased, ref-counted smart pointer with runtime borrow checking.
pub mod refany;
/// CSS cascade: selector matching, specificity, and property inheritance.
pub mod style;
/// `StyledDom` — the result of applying CSS to a DOM tree (the CSSOM).
pub mod styled_dom;
/// SVG rendering, path tessellation, and geometric operations.
pub mod svg;
/// SVG `d=""` path data parser.
pub mod svg_path_parser;
/// Timer, thread, and async task management.
pub mod task;
/// 3D transform matrix computation for CSS transforms.
pub mod transform;
/// Built-in user-agent default stylesheet.
pub mod ua_css;
/// Default font/text constants and small geometry helpers for layout.
pub mod ui_solver;
/// Window configuration, input state, and platform-specific options.
pub mod window;
/// XML and XHTML parsing for declarative UI definitions.
pub mod xml;
/// JSON value types for the C API (no serde dependency).
pub mod json;

/// Ordered map alias used throughout `azul-core`.
///
/// This is backed by `BTreeMap` (not a hash map) because the `core` crate
/// supports `no_std`, where `HashMap` is unavailable. The webrender crates
/// define their own `FastHashMap` using `HashMap` + `FxHasher`.
pub type OrderedMap<T, U> = alloc::collections::BTreeMap<T, U>;
pub type FastBTreeSet<T> = alloc::collections::BTreeSet<T>;
