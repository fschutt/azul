//! Reference kinds for struct fields and pointers
//!
//! This module provides type-safe representations for pointer/reference types
//! used in struct fields.

use std::fmt;
use serde::{Deserialize, Serialize, Deserializer, Serializer};
use serde::de::{self, Visitor};

/// Reference kind for struct fields
///
/// Represents how a type is referenced in a struct field:
/// - `constptr`: `*const T` - const raw pointer
/// - `mutptr`: `*mut T` - mutable raw pointer  
/// - `value`: `T` - direct value (default, not serialized)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RefKind {
    /// `*const T` - const raw pointer
    ConstPtr,
    /// `*mut T` - mutable raw pointer
    MutPtr,
    /// `&T` - immutable reference
    Ref,
    /// `&mut T` - mutable reference
    RefMut,
    /// `Box<T>` - owned heap pointer
    Boxed,
    /// `Option<Box<T>>` - optional owned heap pointer
    OptionBoxed,
    /// `T` (by value) - this is the default and won't be serialized
    #[default]
    Value,
}

impl RefKind {
    /// Convert to the string representation used in api.json
    pub fn as_str(&self) -> &'static str {
        match self {
            RefKind::ConstPtr => "constptr",
            RefKind::MutPtr => "mutptr",
            RefKind::Ref => "ref",
            RefKind::RefMut => "refmut",
            RefKind::Boxed => "boxed",
            RefKind::OptionBoxed => "optionboxed",
            RefKind::Value => "value",
        }
    }
    
    /// Parse from the string representation used in api.json
    pub fn parse(s: &str) -> Option<RefKind> {
        match s {
            "constptr" => Some(RefKind::ConstPtr),
            "mutptr" => Some(RefKind::MutPtr),
            "ref" => Some(RefKind::Ref),
            "refmut" => Some(RefKind::RefMut),
            "boxed" => Some(RefKind::Boxed),
            "optionboxed" => Some(RefKind::OptionBoxed),
            "value" => Some(RefKind::Value),
            _ => None,
        }
    }
    
    /// Returns true if this ref kind represents an opaque pointer type
    /// that should not be recursed through for type resolution
    pub fn is_opaque_pointer(&self) -> bool {
        matches!(self, RefKind::ConstPtr | RefKind::MutPtr | RefKind::Boxed | RefKind::OptionBoxed)
    }
    
    /// Returns true if this is the default value (Value)
    pub fn is_default(&self) -> bool {
        matches!(self, RefKind::Value)
    }
    
    /// Convert to Rust syntax prefix
    pub fn to_rust_prefix(&self) -> &'static str {
        match self {
            RefKind::ConstPtr => "*const ",
            RefKind::MutPtr => "*mut ",
            RefKind::Ref => "&",
            RefKind::RefMut => "&mut ",
            RefKind::Boxed => "Box<",
            RefKind::OptionBoxed => "Option<Box<",
            RefKind::Value => "",
        }
    }
    
    /// Convert to Rust syntax suffix (for closing brackets)
    pub fn to_rust_suffix(&self) -> &'static str {
        match self {
            RefKind::Boxed => ">",
            RefKind::OptionBoxed => ">>",
            _ => "",
        }
    }
}

impl fmt::Display for RefKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for RefKind {
    type Err = ParseRefKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or(ParseRefKindError(s.to_string()))
    }
}

#[derive(Debug, Clone)]
pub struct ParseRefKindError(pub String);

impl fmt::Display for ParseRefKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid ref_kind: '{}' (expected 'constptr', 'mutptr', 'ref', 'refmut', 'boxed', 'optionboxed', or 'value')", self.0)
    }
}

impl std::error::Error for ParseRefKindError {}

// Serde serialization - writes as string, skips "value" (default)
impl Serialize for RefKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

// Serde deserialization
impl<'de> Deserialize<'de> for RefKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct RefKindVisitor;

        impl<'de> Visitor<'de> for RefKindVisitor {
            type Value = RefKind;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("one of: \"constptr\", \"mutptr\", \"ref\", \"refmut\", \"boxed\", \"optionboxed\", \"value\"")
            }

            fn visit_str<E>(self, value: &str) -> Result<RefKind, E>
            where
                E: de::Error,
            {
                RefKind::parse(value).ok_or_else(|| {
                    de::Error::custom(format!(
                        "invalid ref_kind '{}', expected one of: constptr, mutptr, ref, refmut, boxed, optionboxed, value",
                        value
                    ))
                })
            }
        }

        deserializer.deserialize_str(RefKindVisitor)
    }
}

/// Helper function for serde skip_serializing_if
pub fn is_value(kind: &RefKind) -> bool {
    kind.is_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ref_kind_parse() {
        assert_eq!(RefKind::parse("constptr"), Some(RefKind::ConstPtr));
        assert_eq!(RefKind::parse("mutptr"), Some(RefKind::MutPtr));
        assert_eq!(RefKind::parse("ref"), Some(RefKind::Ref));
        assert_eq!(RefKind::parse("refmut"), Some(RefKind::RefMut));
        assert_eq!(RefKind::parse("boxed"), Some(RefKind::Boxed));
        assert_eq!(RefKind::parse("optionboxed"), Some(RefKind::OptionBoxed));
        assert_eq!(RefKind::parse("value"), Some(RefKind::Value));
        assert_eq!(RefKind::parse("invalid"), None);
    }

    #[test]
    fn test_ref_kind_serde() {
        // Test serialization
        let kind = RefKind::ConstPtr;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"constptr\"");

        // Test deserialization
        let parsed: RefKind = serde_json::from_str("\"mutptr\"").unwrap();
        assert_eq!(parsed, RefKind::MutPtr);

        // Test default value
        let value: RefKind = serde_json::from_str("\"value\"").unwrap();
        assert_eq!(value, RefKind::Value);
        assert!(value.is_default());
    }

    #[test]
    fn test_rust_syntax() {
        assert_eq!(RefKind::ConstPtr.to_rust_prefix(), "*const ");
        assert_eq!(RefKind::MutPtr.to_rust_prefix(), "*mut ");
        assert_eq!(RefKind::Ref.to_rust_prefix(), "&");
        assert_eq!(RefKind::RefMut.to_rust_prefix(), "&mut ");
        assert_eq!(RefKind::Boxed.to_rust_prefix(), "Box<");
        assert_eq!(RefKind::OptionBoxed.to_rust_prefix(), "Option<Box<");
        assert_eq!(RefKind::Value.to_rust_prefix(), "");
    }
}
