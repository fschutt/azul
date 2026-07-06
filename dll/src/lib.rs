//! Azul DLL - C API bindings for the Azul GUI framework
//!
//! This crate provides the C-ABI functions for the Azul library.
//! The API is auto-generated from api.json using `azul-doc codegen all`.
//!
//! To regenerate all bindings:
//!   cd doc && cargo run --release -- codegen all
//!
//! ## Feature Flags
//!
//! ### Codegen Block Features (granular)
//! - `cabi_internal`: Include C-ABI function bodies (transmute-based implementations)
//! - `cabi_export`: Add `#[no_mangle]` to internal functions (for C/C++/Python/dlsym)
//! - `cabi_external`: Include `extern "C"` declarations (for dynamic linking)
//! - `rust_api`: Include public Rust API re-exports (`azul::dom::Dom`, etc.)
//!
//! ### Build Modes (compose the granular features)
//! - `build-dll`: `cabi_export` + `rust_api` + all platform deps
//! - `link-static`: `cabi_internal` + `rust_api` + all platform deps
//! - `link-dynamic`: `cabi_external` + `rust_api` (no internal deps)
//!
//! ### Optional Features
//! - `web`: Enable the web backend (serve the app as HTML over HTTP)
//! - `python-extension`: Build as a Python extension module (PyInit_azul)

// Lint policy: deny correctness/safety issues, warn on style
#![deny(improper_ctypes_definitions)]
#![deny(unused_must_use)]
#![warn(clippy::all)]
// === "extreme lints" lockdown (2026-06-20) — maximal opt-in lint set ===
// All clippy groups + opt-in rustc lints, warn-level so normal builds still
// pass; the CI clippy job runs `-D warnings`, turning every one of these into
// the outstanding-lint-failure report for Monday triage. NOT yet fixed.
#![warn(
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
)]
#![allow(
    clippy::non_canonical_partial_ord_impl,
    clippy::legacy_numeric_constants,
    clippy::should_implement_trait,
    clippy::result_unit_err,
    clippy::ptr_as_ptr,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::needless_return,              // generated FFI code
    clippy::let_and_return,               // generated FFI code
    clippy::manual_map,                   // generated FFI code
    unused_imports,                        // conditional compilation
    unused_variables,                      // platform-gated code
    dead_code,                             // feature-gated code
    unused_mut,
    unused_unsafe,                         // objc2 macro expansions
    non_snake_case,                        // Win32 API naming (DwmSetWindowAttribute etc.)
    unused_doc_comments,                   // doc on macro invocations
    mismatched_lifetime_syntaxes,
    unexpected_cfgs,
    static_mut_refs,                       // TODO: migrate to OnceLock for Rust 2024
    deprecated,                            // objc2 NSOpenGL*, msg_send_id, PanicInfo
)]

// Security rail: a remill/web build (libazulwithremill.so — shipped in the
// azul-web-base Docker image and run in production pods) must NEVER carry the
// in-process HTTP debug server. `debug-server` is what compiles in the AZ_DEBUG
// port + the AZ_E2E runner; combining it with the web transpiler would risk an
// accidental open debug port in a deployed web app. Refuse to build the two
// together so it can't happen by mistake.
#[cfg(all(feature = "web-transpiler", feature = "debug-server"))]
compile_error!(
    "the `debug-server` feature (AZ_DEBUG port + AZ_E2E runner) must not be enabled \
     together with the web transpiler (`web-transpiler`/`web-transpiler-static`): a \
     deployed web build must not expose a debug port. Build libazulwithremill.so \
     without `debug-server`."
);

// ---------------------------------------------------------------------------
// Global allocator selection (mutually exclusive features)
// ---------------------------------------------------------------------------
// The C API boundary means this only affects azul's internal allocations.
// The host application keeps its own allocator unchanged.
#[cfg(feature = "allocator_mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// jemalloc has no Windows-MSVC support, so on Windows the `allocator_jemalloc`
// feature falls through to the system allocator instead of failing — keeps the
// feature cross-platform (runtime-adaptable) rather than a hard build break.
#[cfg(all(feature = "allocator_jemalloc", not(target_os = "windows")))]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[macro_use]
extern crate alloc;

/// Hint the allocator to return freed pages to the OS.
/// Call after large transient allocations are freed (e.g. after layout).
/// With `allocator_mimalloc`: calls `mi_collect(true)` for aggressive purge.
/// With `allocator_jemalloc`: calls `mallctl("arena.0.purge")`.
/// Without either: platform-specific hint (macOS `malloc_zone_pressure_relief`).
#[cfg(feature = "cabi_export")]
#[no_mangle]
pub extern "C" fn az_purge_allocator() {
    #[cfg(feature = "allocator_mimalloc")]
    {
        extern "C" {
            fn mi_collect(force: bool);
        }
        unsafe { mi_collect(true); }
    }
    #[cfg(feature = "allocator_jemalloc")]
    {
        // jemalloc: purge via the raw mallctl interface
        extern "C" {
            fn mallctl(
                name: *const u8, oldp: *mut core::ffi::c_void, oldlenp: *mut usize,
                newp: *mut core::ffi::c_void, newlen: usize,
            ) -> core::ffi::c_int;
        }
        unsafe {
            mallctl(
                b"arena.0.purge\0".as_ptr(), core::ptr::null_mut(),
                core::ptr::null_mut(), core::ptr::null_mut(), 0,
            );
        }
    }
    #[cfg(not(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc")))]
    {
        #[cfg(target_os = "macos")]
        {
            extern "C" {
                fn malloc_zone_pressure_relief(zone: *mut core::ffi::c_void, goal: usize) -> usize;
            }
            unsafe { malloc_zone_pressure_relief(core::ptr::null_mut(), 0); }
        }
    }
}

// Internal crates - only needed when cabi_internal is enabled
// (pulled in by build-dll and link-static via _internal_deps)
#[cfg(feature = "cabi_internal")]
extern crate azul_core;
#[cfg(feature = "cabi_internal")]
extern crate azul_css;
#[cfg(feature = "cabi_internal")]
extern crate azul_layout;

// Desktop windowing implementation (OpenGL, fonts, event loop, etc.)
// Compiled when internal bindings are available (not for link-dynamic)
#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub mod desktop;

// Target-stable home for the `extra::*` feature handles (audio, video_codec,
// sqlite, pdf, udp). Off-wasm it re-exports the real `desktop::extra::*` types;
// on wasm it provides repr-C-identical stubs so the generated C-ABI bindings
// resolve everywhere. UNGATED on purpose (the path must exist on every target).
pub mod unified;

// Web backend: serve the app as HTML over HTTP (AZ_BACKEND=web://ip:port)
#[cfg(all(
    feature = "web",
    feature = "cabi_internal",
    not(target_arch = "wasm32")
))]
pub mod web;

// =============================================================================
// Internal Bindings: C-ABI function bodies via transmute
// Used by both build-dll (with cabi_export → #[no_mangle]) and
// link-static (without cabi_export → internal only)
// Generated by: cd doc && cargo run --release -- codegen all
// =============================================================================

#[cfg(feature = "cabi_internal")]
mod __ffi_internal {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/dll_api_internal.rs"
    ));
}

#[cfg(feature = "cabi_internal")]
pub use __ffi_internal::__dll_api_inner::dll;

#[cfg(feature = "cabi_internal")]
pub mod ffi {
    pub use crate::__ffi_internal::__dll_api_inner::*;
}

// =============================================================================
// External Bindings: extern "C" declarations for dynamic linking
// Used by link-dynamic (links against pre-built libazul.dylib/so/dll)
// Generated by: cd doc && cargo run --release -- codegen all
// =============================================================================

#[cfg(all(feature = "cabi_external", not(feature = "cabi_internal")))]
mod __ffi_external {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/dll_api_external.rs"
    ));
}

#[cfg(all(feature = "cabi_external", not(feature = "cabi_internal")))]
pub use __ffi_external::__dll_api_inner::dll;

#[cfg(all(feature = "cabi_external", not(feature = "cabi_internal")))]
pub mod ffi {
    pub use crate::__ffi_external::__dll_api_inner::*;
}

// =============================================================================
// Public Rust API: Re-exports without Az prefix
// Generated by: cd doc && cargo run --release -- codegen all
//
// This provides a nice Rust API:
//   use azul::prelude::*;
//   use azul::app::App;
//   use azul::dom::Dom;
// =============================================================================

#[cfg(feature = "rust_api")]
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/codegen/reexports.rs"
));

// =============================================================================
// Python Extension Module
// Generated by: cd doc && cargo run --release -- codegen all
// =============================================================================

#[cfg(feature = "python-extension")]
mod python {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/python_api.rs"
    ));
}

// Re-export the pymodule function at crate root for PyInit_azul to work
#[cfg(feature = "python-extension")]
pub use python::azul;

// Fallback definitions of the `Py*` symbols pyo3's `extension-module` leaves
// undefined, so a single libazul.so serves both `import azul` and C `-lazul`
// (bug B1). MUST be a crate `#[no_mangle]` module — not a linked C archive —
// so rustc exports them GLOBAL; a localized stub is not interposable and crashes
// import. Linux/ELF only. See the module docs for the full rationale.
#[cfg(all(target_os = "linux", feature = "python-extension"))]
mod python_abi3_stubs;

// PHP extension entry point (Zend engine). Loaded via
// `php -d extension=/path/to/libazul.dylib`.
#[cfg(feature = "php-extension")]
pub mod php_extension;

// =============================================================================
// Memory Tests: Size and alignment verification
// Generated by: cd doc && cargo run --release -- codegen all
// Run with: cd dll && cargo test
// =============================================================================

#[cfg(test)]
mod memtest {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../target/codegen/memtest.rs"
    ));
}
