pub mod c_api;
pub mod cpp_api;
pub mod fn_body_gen;
pub mod func_gen;
pub mod memtest;
pub mod python_api;
pub mod rust_api;
pub mod rust_dll;
pub mod struct_gen;
pub mod tests;

// Codegen v2: Unified, configuration-driven code generation
// This is the new architecture that will eventually replace the above modules.
// See scripts/CODEGEN_REFACTOR_PROPOSAL.md for details.
pub mod v2;
