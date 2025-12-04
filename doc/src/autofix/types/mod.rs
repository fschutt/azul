//! Type parsing and representation
//!
//! This module provides unified type parsing with clear semantics.
//! All type-related operations should go through this module.

pub mod parser;
pub mod ffi;
pub mod borrow;

pub use parser::{ParsedType, TypeParser, ParseError};
pub use ffi::{is_ffi_safe, FfiSafetyCheck};
pub use borrow::{BorrowMode, SelfParam, FnArg};
