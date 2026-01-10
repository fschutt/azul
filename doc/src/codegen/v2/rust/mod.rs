//! Rust code generators
//!
//! This module contains separate generators for different Rust binding styles:
//! - `static_binding`: Generates standalone Rust code with native implementations
//! - `dynamic_binding`: Generates Rust code that calls extern "C" functions from a DLL
//!
//! Both generators share common code for type generation via the `shared` module.

pub mod dynamic_binding;
pub mod shared;
pub mod static_binding;

pub use dynamic_binding::RustDynamicGenerator;
pub use static_binding::RustStaticGenerator;
