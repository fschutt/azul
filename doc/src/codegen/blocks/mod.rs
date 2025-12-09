//! Block-based code generation
//!
//! This module contains individual code generation "blocks" that can be
//! combined to build different outputs (ffi, api, memtest, etc.)
//!
//! Each block is responsible for generating a specific part of the output:
//! - `config`: Shared configuration types
//! - `gl_types`: OpenGL type aliases
//! - `primitive_types`: Primitive type aliases (AzI32, AzU32, etc.)
//! - `structs`: Struct/enum/typedef generation
//! - `functions`: FFI function generation
//! - `trait_impls`: Trait implementations (Debug, Clone, etc.)
//! - `nice_api`: Wrapper API generation (ergonomic Rust API)
//! - `prelude`: Re-export generation

pub mod config;
pub mod gl_types;
pub mod primitive_types;
pub mod structs;
pub mod functions;
pub mod trait_impls;
pub mod nice_api;
pub mod prelude;

pub use config::*;
