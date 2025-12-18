// Legacy C and C++ generators - kept for regression testing during V2 migration
// The C header generation has been ported to v2, but cpp_api still depends on c_api helpers
pub mod c_api;
pub mod cpp_api;

// Codegen v2: Unified, configuration-driven code generation
// This is the new architecture that replaces the old rust_api, python_api modules.
// See scripts/CODEGEN_REFACTOR_PROPOSAL.md for details.
pub mod v2;
