//! PHP extension entry point (feature `php-extension`).
//!
//! Built as a Zend-engine native extension via the `ext-php-rs` crate.
//! Unlike the standard php-ffi path (which rejects closure-to-fnpointer
//! by design), the Zend engine supports calling back into PHP from a C
//! function pointer pinned at extension-load time. This unblocks the
//! host-invoker pattern that all the other managed-FFI hosts use.
//!
//! ## Building
//!
//! Required environment on macOS aarch64:
//!
//! ```sh
//! LIBCLANG_PATH=/Library/Developer/CommandLineTools/usr/lib \
//! DYLD_FALLBACK_LIBRARY_PATH=$LIBCLANG_PATH \
//! RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup" \
//!   cargo build --release -p azul-dll --features php-extension
//! ```
//!
//! * `LIBCLANG_PATH` lets ext-php-rs's build script find libclang.
//! * `DYLD_FALLBACK_LIBRARY_PATH` lets dyld resolve `@rpath/libclang.dylib`
//!   for the build-script binary at runtime.
//! * `RUSTFLAGS="-C link-arg=-undefined -C link-arg=dynamic_lookup"`
//!   defers `zend_*` symbol resolution to extension-load time.
//!
//! Linux equivalent: swap the macOS dynamic_lookup flag for
//! `RUSTFLAGS="-C link-arg=-Wl,--unresolved-symbols=ignore-in-object-files"`.
//!
//! Load:
//!
//! ```sh
//! php -d extension=target/release/libazul.dylib hello-world-ext.php
//! ```
//!
//! ## Surface
//!
//! Phase 47 hand-coded the host-invoker primitives directly in this
//! file. Phase 48 moves them into a generated `target/codegen/php_api.rs`
//! produced by `doc/src/codegen/v2/lang_php_ext.rs`. The generator's
//! input is the same CodegenIR every other lang_* module consumes, so
//! adding a callback kind to `HOST_INVOKER_KINDS` lights up a matching
//! `azul_register_<kind>_callback` PHP function automatically.
//!
//! Regenerate with:
//!
//! ```sh
//! cd doc && cargo run --release -- codegen all
//! ```

// libazul C-ABI host-invoker symbols live in the same .dylib (we ARE
// libazul under feature `php-extension`) — they're exported by
// target/codegen/dll_api_internal.rs via `#[no_mangle] extern "C"`.
// Cross-call by referring to them as extern "C" symbols without a
// `#[link]` directive (no separate link target).
extern "C" {
    fn AzApp_setHostHandleReleaser(
        releaser: Option<unsafe extern "C" fn(id: u64)>,
    );
}

// Pull in the generated bindings. The generator emits ext-php-rs
// macro calls qualified through `::ext_php_rs::...` paths, so no
// `use ext_php_rs::prelude::*` is required at the include site.
include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../target/codegen/php_api.rs"
));
