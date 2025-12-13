//! Type parsing and representation
//!
//! This module provides unified type parsing with clear semantics.
//! All type-related operations should go through this module.

pub mod borrow;
pub mod ffi;
pub mod parser;
pub mod ref_kind;

pub use borrow::{BorrowMode, FnArg, SelfParam};
pub use ffi::{is_ffi_safe, FfiSafetyCheck};
pub use parser::{ParseError, ParsedType, TypeParser};
pub use ref_kind::RefKind;
