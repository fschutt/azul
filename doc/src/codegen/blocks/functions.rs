//! FFI function generation block
//!
//! Re-exports the function generation functionality from the existing modules.

pub use crate::codegen::func_gen::{build_functions_map, build_functions_map_ext, FunctionInfo};
pub use crate::codegen::memtest::{
    generate_transmuted_fn_body, parse_fn_args, parse_arg_type,
};
