//! Type parser with validation
//!
//! This module handles parsing of type strings from api.json with proper validation.
//! It distinguishes between:
//! - Primitive types (bool, u8, u32, f32, etc.)
//! - Built-in wrapper types (Option, Vec, Result, etc.)
//! - User-defined types (structs, enums from the workspace)
//! - Invalid/bogus type strings ("ref", "value", etc.)

use std::collections::HashSet;

use super::borrow::BorrowMode;

/// A parsed type representation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ParsedType {
    /// Primitive type like u8, i32, f32, bool, ()
    Primitive(String),
    /// Pointer type like *const T or *mut T
    Pointer {
        is_const: bool,
        inner: Box<ParsedType>,
    },
    /// Reference type like &T or &mut T
    Reference {
        is_mut: bool,
        inner: Box<ParsedType>,
    },
    /// Generic type like Option<T>, Vec<T>, Result<T, E>
    Generic {
        outer: String,
        args: Vec<ParsedType>,
    },
    /// Tuple type like (A, B, C)
    Tuple(Vec<ParsedType>),
    /// User-defined type name (struct or enum from workspace)
    UserDefined(String),
    /// Function pointer type like fn(A, B) -> C
    FnPointer {
        args: Vec<ParsedType>,
        ret: Option<Box<ParsedType>>,
    },
    /// Invalid/unparseable type string
    Invalid(String),
}

impl ParsedType {
    /// Returns all user-defined type names referenced by this type
    pub fn collect_user_types(&self, out: &mut HashSet<String>) {
        match self {
            ParsedType::Primitive(_) => {}
            ParsedType::Pointer { inner, .. } => inner.collect_user_types(out),
            ParsedType::Reference { inner, .. } => inner.collect_user_types(out),
            ParsedType::Generic { outer, args } => {
                // The outer name might be a user-defined generic (rare but possible)
                if !is_builtin_generic(outer) {
                    out.insert(outer.clone());
                }
                for arg in args {
                    arg.collect_user_types(out);
                }
            }
            ParsedType::Tuple(elems) => {
                for elem in elems {
                    elem.collect_user_types(out);
                }
            }
            ParsedType::UserDefined(name) => {
                out.insert(name.clone());
            }
            ParsedType::FnPointer { args, ret } => {
                for arg in args {
                    arg.collect_user_types(out);
                }
                if let Some(r) = ret {
                    r.collect_user_types(out);
                }
            }
            ParsedType::Invalid(_) => {}
        }
    }

    /// Returns true if this type is marked as invalid
    pub fn is_invalid(&self) -> bool {
        matches!(self, ParsedType::Invalid(_))
    }

    /// Returns true if this is a primitive type
    pub fn is_primitive(&self) -> bool {
        matches!(self, ParsedType::Primitive(_))
    }
}

/// Error type for parsing failures
#[derive(Debug, Clone)]
pub struct ParseError {
    pub input: String,
    pub reason: String,
}

/// Type parser with validation
pub struct TypeParser {
    /// Known primitive type names
    primitives: HashSet<String>,
    /// Known built-in generic type names
    builtin_generics: HashSet<String>,
}

impl Default for TypeParser {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeParser {
    pub fn new() -> Self {
        let primitives: HashSet<String> = [
            "bool", "char", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
            "i128", "isize", "f32", "f64", "()", "c_void",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let builtin_generics: HashSet<String> = [
            "Option",
            "Vec",
            "Result",
            "Box",
            "Arc",
            "Rc",
            "RefCell",
            "Cell",
            "Mutex",
            "RwLock",
            "HashMap",
            "HashSet",
            "BTreeMap",
            "BTreeSet",
            "Cow",
            "PhantomData",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        Self {
            primitives,
            builtin_generics,
        }
    }

    /// Check if a string is a borrow keyword (not a type)
    /// Uses the strongly-typed BorrowMode enum
    pub fn is_borrow_keyword(&self, s: &str) -> bool {
        BorrowMode::is_borrow_keyword(s)
    }

    /// Check if a string is a primitive type
    pub fn is_primitive(&self, s: &str) -> bool {
        self.primitives.contains(s)
    }

    /// Parse a type string from api.json
    pub fn parse(&self, input: &str) -> ParsedType {
        let input = input.trim();

        // Empty string is invalid
        if input.is_empty() {
            return ParsedType::Invalid("empty string".to_string());
        }

        // Check for borrow keywords
        if self.is_borrow_keyword(input) {
            return ParsedType::Invalid(format!("borrow keyword '{}' is not a type", input));
        }

        // Check for tuple-like "Type, Type" without parentheses (parser bug in old code)
        if input.contains(',') && !input.starts_with('(') && !input.contains('<') {
            return ParsedType::Invalid(format!(
                "bare comma in type '{}' - likely parser bug",
                input
            ));
        }

        // Parse the actual type
        self.parse_inner(input)
    }

    fn parse_inner(&self, input: &str) -> ParsedType {
        let input = input.trim();

        // Unit type
        if input == "()" {
            return ParsedType::Primitive("()".to_string());
        }

        // Primitive check
        if self.primitives.contains(input) {
            return ParsedType::Primitive(input.to_string());
        }

        // Pointer types
        if input.starts_with("*const ") {
            let inner = &input[7..];
            return ParsedType::Pointer {
                is_const: true,
                inner: Box::new(self.parse_inner(inner)),
            };
        }
        if input.starts_with("*mut ") {
            let inner = &input[5..];
            return ParsedType::Pointer {
                is_const: false,
                inner: Box::new(self.parse_inner(inner)),
            };
        }

        // Reference types
        if input.starts_with("&mut ") {
            let inner = &input[5..];
            return ParsedType::Reference {
                is_mut: true,
                inner: Box::new(self.parse_inner(inner)),
            };
        }
        if input.starts_with('&') {
            let inner = &input[1..];
            return ParsedType::Reference {
                is_mut: false,
                inner: Box::new(self.parse_inner(inner)),
            };
        }

        // Tuple types
        if input.starts_with('(') && input.ends_with(')') {
            let inner = &input[1..input.len() - 1];
            if inner.is_empty() {
                return ParsedType::Primitive("()".to_string());
            }
            let parts = split_at_top_level(inner, ',');
            let parsed: Vec<ParsedType> =
                parts.iter().map(|p| self.parse_inner(p.trim())).collect();
            return ParsedType::Tuple(parsed);
        }

        // Generic types like Option<T>, Vec<T>, etc.
        if let Some(angle_pos) = input.find('<') {
            if input.ends_with('>') {
                let outer = &input[..angle_pos];
                let inner = &input[angle_pos + 1..input.len() - 1];
                let parts = split_at_top_level(inner, ',');
                let args: Vec<ParsedType> =
                    parts.iter().map(|p| self.parse_inner(p.trim())).collect();
                return ParsedType::Generic {
                    outer: outer.to_string(),
                    args,
                };
            }
        }

        // Function pointer types
        if input.starts_with("fn(") {
            return self.parse_fn_pointer(input);
        }
        if input.starts_with("extern \"C\" fn(") {
            return self.parse_fn_pointer(input);
        }

        // Simple user-defined type (no generics, no pointers)
        // Validate it looks like a valid Rust identifier
        if is_valid_type_name(input) {
            return ParsedType::UserDefined(input.to_string());
        }

        ParsedType::Invalid(format!("cannot parse type '{}'", input))
    }

    fn parse_fn_pointer(&self, input: &str) -> ParsedType {
        // Simple parsing for fn(A, B) -> C
        let fn_prefix = if input.starts_with("extern") {
            "extern \"C\" fn("
        } else {
            "fn("
        };

        let rest = &input[fn_prefix.len()..];

        // Find matching paren
        let mut paren_depth = 1;
        let mut args_end = 0;
        for (i, c) in rest.char_indices() {
            match c {
                '(' => paren_depth += 1,
                ')' => {
                    paren_depth -= 1;
                    if paren_depth == 0 {
                        args_end = i;
                        break;
                    }
                }
                _ => {}
            }
        }

        let args_str = &rest[..args_end];
        let after_parens = rest[args_end + 1..].trim();

        let args: Vec<ParsedType> = if args_str.is_empty() {
            vec![]
        } else {
            split_at_top_level(args_str, ',')
                .iter()
                .map(|p| self.parse_inner(p.trim()))
                .collect()
        };

        let ret = if after_parens.starts_with("->") {
            let ret_type = after_parens[2..].trim();
            Some(Box::new(self.parse_inner(ret_type)))
        } else {
            None
        };

        ParsedType::FnPointer { args, ret }
    }
}

/// Check if a type name is a built-in generic
fn is_builtin_generic(name: &str) -> bool {
    matches!(
        name,
        "Option"
            | "Vec"
            | "Result"
            | "Box"
            | "Arc"
            | "Rc"
            | "RefCell"
            | "Cell"
            | "Mutex"
            | "RwLock"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "Cow"
            | "PhantomData"
    )
}

/// Check if a string looks like a valid Rust type name
fn is_valid_type_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }

    // Must start with uppercase letter for user types
    let first = s.chars().next().unwrap();
    if !first.is_ascii_uppercase() && first != '_' {
        return false;
    }

    // Rest must be alphanumeric or underscore
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Split a string at top-level occurrences of delimiter (ignoring nested <> and ())
fn split_at_top_level(s: &str, delim: char) -> Vec<&str> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut angle_depth: i32 = 0;
    let mut paren_depth: i32 = 0;

    for (i, c) in s.char_indices() {
        match c {
            '<' => angle_depth += 1,
            '>' => angle_depth = angle_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            c if c == delim && angle_depth == 0 && paren_depth == 0 => {
                result.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }

    result.push(&s[start..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_primitive() {
        let parser = TypeParser::new();
        assert!(matches!(parser.parse("u32"), ParsedType::Primitive(_)));
        assert!(matches!(parser.parse("bool"), ParsedType::Primitive(_)));
        assert!(matches!(parser.parse("()"), ParsedType::Primitive(_)));
    }

    #[test]
    fn test_parse_borrow_keywords() {
        let parser = TypeParser::new();
        assert!(parser.parse("ref").is_invalid());
        assert!(parser.parse("refmut").is_invalid());
        assert!(parser.parse("value").is_invalid());
    }

    #[test]
    fn test_parse_user_defined() {
        let parser = TypeParser::new();
        match parser.parse("CssProperty") {
            ParsedType::UserDefined(name) => assert_eq!(name, "CssProperty"),
            other => panic!("Expected UserDefined, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_generic() {
        let parser = TypeParser::new();
        match parser.parse("Option<CssProperty>") {
            ParsedType::Generic { outer, args } => {
                assert_eq!(outer, "Option");
                assert_eq!(args.len(), 1);
            }
            other => panic!("Expected Generic, got {:?}", other),
        }
    }

    #[test]
    fn test_collect_user_types() {
        let parser = TypeParser::new();
        let ty = parser.parse("Option<Vec<CssProperty>>");
        let mut types = HashSet::new();
        ty.collect_user_types(&mut types);
        assert!(types.contains("CssProperty"));
        assert!(!types.contains("Option"));
        assert!(!types.contains("Vec"));
    }

    #[test]
    fn test_bare_comma_invalid() {
        let parser = TypeParser::new();
        assert!(parser.parse("String, String").is_invalid());
    }
}
