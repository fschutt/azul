//! GUI-free demonstration of the `Transpiler` trait.
//!
//! Run with the stub (default web backend, no remill toolchain):
//!     cargo run --release -p azul-dll --no-default-features \
//!         --features "web link-static" --example transpile_fn
//!
//! Run with the remill-backed transpiler (requires the
//! `third_party/remill-rs` submodule and LLVM toolchain):
//!     cargo run --release -p azul-dll --no-default-features \
//!         --features "web-transpiler link-static" --example transpile_fn
//!
//! The example deliberately avoids any window / event-loop types so the
//! transpile step can be lifted out of the web.md flow at whatever phase
//! (build-time, first-request, lazy-per-callback) the caller chooses.

use azul::web::transpiler::{default_transpiler, Transpiler};

/// Trivial leaf function — the minimum-viable lift target.
///
/// Marked `#[inline(never)]` so the symbol is preserved in the running
/// binary (a real lift uses `dladdr` to resolve `fn_addr` to this symbol).
#[inline(never)]
#[no_mangle]
pub extern "C" fn azul_demo_add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let t = default_transpiler();
    println!("Using transpiler: {}", t.name());
    println!("Available:        {}", t.is_available());

    let fn_addr = azul_demo_add as usize;
    let fn_size: usize = 64;

    match t.lift_function("azul_demo_add", fn_addr, fn_size) {
        Ok(module) => {
            println!(
                "Lifted azul_demo_add → {} WASM bytes (content hash {})",
                module.bytes.len(),
                module.content_hash
            );
            println!("Exports: {:?}", module.exports);
            println!("Imports from azul-mini: {:?}", module.imports_from_mini);
        }
        Err(e) => {
            println!("Lift failed (expected with stub transpiler): {}", e);
            println!(
                "Native call still works: azul_demo_add(2, 3) = {}",
                azul_demo_add(2, 3)
            );
        }
    }
}
