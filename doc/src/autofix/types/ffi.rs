//! FFI safety checking for types
//!
//! This module determines whether types are FFI-safe for use across C boundaries.

use super::parser::ParsedType;

/// Result of an FFI safety check
#[derive(Debug, Clone)]
pub struct FfiSafetyCheck {
    pub is_safe: bool,
    pub reason: Option<String>,
}

impl FfiSafetyCheck {
    pub fn safe() -> Self {
        Self { is_safe: true, reason: None }
    }

    pub fn unsafe_with_reason(reason: impl Into<String>) -> Self {
        Self { is_safe: false, reason: Some(reason.into()) }
    }
}

/// Check if a parsed type is FFI-safe
///
/// Rules:
/// - Primitives are safe
/// - Raw pointers are safe (caller must ensure validity)
/// - References are NOT safe across FFI
/// - Vec, Box, etc. are NOT safe (need repr(C) wrappers)
/// - User-defined types are assumed safe if they use repr(C)
pub fn is_ffi_safe(ty: &ParsedType) -> FfiSafetyCheck {
    match ty {
        ParsedType::Primitive(_) => FfiSafetyCheck::safe(),

        ParsedType::Pointer { inner, .. } => {
            // Pointers are FFI-safe, but check inner type
            let inner_check = is_ffi_safe(inner);
            if inner_check.is_safe {
                FfiSafetyCheck::safe()
            } else {
                FfiSafetyCheck::unsafe_with_reason(
                    format!("pointer to non-FFI-safe type: {:?}", inner_check.reason)
                )
            }
        }

        ParsedType::Reference { .. } => {
            FfiSafetyCheck::unsafe_with_reason("references are not FFI-safe, use raw pointers")
        }

        ParsedType::Generic { outer, args } => {
            // Check if it's a known FFI-safe wrapper
            if is_ffi_safe_generic(outer) {
                // Check that inner args are also safe
                for arg in args {
                    let arg_check = is_ffi_safe(arg);
                    if !arg_check.is_safe {
                        return FfiSafetyCheck::unsafe_with_reason(
                            format!("{}<...> with non-FFI-safe inner type", outer)
                        );
                    }
                }
                FfiSafetyCheck::safe()
            } else {
                FfiSafetyCheck::unsafe_with_reason(
                    format!("generic type {} is not FFI-safe without wrapper", outer)
                )
            }
        }

        ParsedType::Tuple(elems) => {
            if elems.is_empty() {
                // Unit tuple is safe
                FfiSafetyCheck::safe()
            } else {
                // Tuples need #[repr(C)] wrapper structs
                FfiSafetyCheck::unsafe_with_reason("tuples are not FFI-safe, use repr(C) struct")
            }
        }

        ParsedType::UserDefined(_) => {
            // We assume user-defined types are designed to be FFI-safe
            // The actual check would require inspecting the type definition
            FfiSafetyCheck::safe()
        }

        ParsedType::FnPointer { .. } => {
            // extern "C" fn pointers are FFI-safe
            FfiSafetyCheck::safe()
        }

        ParsedType::Invalid(reason) => {
            FfiSafetyCheck::unsafe_with_reason(format!("invalid type: {}", reason))
        }
    }
}

/// Check if a generic type name has FFI-safe wrappers in azul
fn is_ffi_safe_generic(name: &str) -> bool {
    // These are azul's FFI-safe wrapper types
    matches!(name,
        // Option wrapper (OptionX types in azul)
        "OptionI8" | "OptionU8" | "OptionI16" | "OptionU16" |
        "OptionI32" | "OptionU32" | "OptionI64" | "OptionU64" |
        "OptionF32" | "OptionF64" | "OptionUsize" | "OptionIsize" |
        // Vec wrappers
        "AzString" | "StringVec" | "U8Vec" | "U16Vec" | "U32Vec" |
        // Result wrappers
        "ResultXmlXmlError" |
        // Other known FFI wrappers
        "AzRefAny" | "RefAny"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autofix::types::TypeParser;

    #[test]
    fn test_primitive_ffi_safe() {
        let parser = TypeParser::new();
        let ty = parser.parse("u32");
        assert!(is_ffi_safe(&ty).is_safe);
    }

    #[test]
    fn test_reference_not_ffi_safe() {
        let parser = TypeParser::new();
        let ty = parser.parse("&CssProperty");
        assert!(!is_ffi_safe(&ty).is_safe);
    }

    #[test]
    fn test_pointer_ffi_safe() {
        let parser = TypeParser::new();
        let ty = parser.parse("*const CssProperty");
        assert!(is_ffi_safe(&ty).is_safe);
    }
}
