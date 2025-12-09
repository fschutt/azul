//! Struct, enum, and typedef generation block
//!
//! Re-exports the struct generation functionality from the existing struct_gen module.
//! This wrapper allows gradual migration to the block-based architecture.

pub use crate::codegen::struct_gen::{
    generate_structs, GenerateConfig, StructMetadata,
};
