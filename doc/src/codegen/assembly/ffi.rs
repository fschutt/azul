//! FFI layer assembly
//!
//! Assembles the complete FFI layer (dll_api.rs) for inclusion in azul-dll.
//! This delegates to the existing memtest::generate_dll_api which is battle-tested.
//!
//! The blocks in `blocks/` provide reusable components that will gradually
//! replace the monolithic generation as we refactor.

use std::path::Path;

use crate::api::ApiData;
use crate::codegen::memtest;

pub type Result<T> = std::result::Result<T, String>;

/// Generate the complete FFI layer (dll_api.rs)
///
/// This currently delegates to the existing `memtest::generate_dll_api`
/// which is well-tested and handles all the edge cases correctly.
///
/// Future work will gradually replace internals with blocks from `blocks/`
/// that can be configured via `CodegenConfig`.
pub fn generate_ffi_layer(api_data: &ApiData, output_path: &Path) -> Result<()> {
    println!("  [FFI] Generating FFI layer via memtest::generate_dll_api...");
    
    memtest::generate_dll_api(api_data, output_path)
        .map_err(|e| format!("Failed to generate FFI layer: {}", e))?;
    
    println!("[OK] Generated FFI layer at: {}", output_path.display());
    Ok(())
}
