/// Extract the inner type from a generic like Vec<T> or Option<T>
/// Assumes type_str has no spaces (call normalize_generic_type first)
pub fn extract_generic_type(type_str: &str, generic_name: &str) -> Option<String> {
    let prefix = format!("{}<", generic_name);

    if type_str.starts_with(&prefix) && type_str.ends_with('>') {
        let start = prefix.len();
        let end = type_str.len() - 1;
        if start < end {
            return Some(type_str[start..end].to_string());
        }
    }

    None
}

/// Split generic arguments by comma at the top level
pub fn split_generic_args(args: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut depth = 0;

    for ch in args.chars() {
        match ch {
            '<' | '(' | '[' => {
                depth += 1;
                current.push(ch);
            }
            '>' | ')' | ']' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                result.push(current.trim().to_string());
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }

    result
}

/// Get the normalized type name from GenericTypeInfo
pub fn get_generic_type_name(info: &GenericTypeInfo) -> String {
    match info {
        GenericTypeInfo::Vec {
            normalized_name, ..
        } => normalized_name.clone(),
        GenericTypeInfo::Option {
            normalized_name, ..
        } => normalized_name.clone(),
        GenericTypeInfo::Result {
            normalized_name, ..
        } => normalized_name.clone(),
    }
}

/// Information about a detected generic type
#[derive(Debug, Clone)]
pub enum GenericTypeInfo {
    Vec {
        inner_type: String,
        normalized_name: String,
    },
    Option {
        inner_type: String,
        normalized_name: String,
    },
    Result {
        ok_type: String,
        err_type: String,
        normalized_name: String,
    },
}

/// Clean up type strings by removing unnecessary parentheses, extracting last path segment,
/// and normalizing generic types
pub fn clean_type_string(type_str: &str) -> String {
    // First, remove spaces around :: (syn's token stream adds these)
    let type_str = type_str.replace(" :: ", "::");

    // If it's a single type wrapped in parentheses, remove them
    // e.g., "(usize)" -> "usize"
    let trimmed = type_str.trim();
    let unwrapped = if trimmed.starts_with('(') && trimmed.ends_with(')') {
        let inner = &trimmed[1..trimmed.len() - 1];
        // Check if there's only one type (no comma outside of nested structures)
        let mut depth = 0;
        let mut has_comma_at_top_level = false;
        for ch in inner.chars() {
            match ch {
                '<' | '(' | '[' => depth += 1,
                '>' | ')' | ']' => depth -= 1,
                ',' if depth == 0 => {
                    has_comma_at_top_level = true;
                    break;
                }
                _ => {}
            }
        }
        if !has_comma_at_top_level {
            inner
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    // Normalize raw pointers: *const Foo -> *const c_void, *mut Foo -> *mut c_void
    // Only c_void can take raw pointers in the C ABI
    let normalized_pointers = normalize_raw_pointers(unwrapped);

    // Normalize generic types (Vec<T> -> TVec, Option<T> -> OptionT, Result<T,E> -> ResultTE)
    // Also handles Option<Box<T>> -> *const c_void
    let (normalized, generic_info) = normalize_generic_type(&normalized_pointers);

    // If normalization changed the type, return the normalized version
    if normalized != normalized_pointers {
        return normalized;
    }

    // If it's a tracked generic type, we already normalized it
    if generic_info.is_some() {
        return normalized;
    }

    // Extract the last segment of the path (after the last ::)
    // e.g., "crate::thread::CreateThreadCallback" -> "CreateThreadCallback"
    let result = if let Some(last_segment_pos) = normalized_pointers.rfind("::") {
        normalized_pointers[last_segment_pos + 2..].to_string()
    } else {
        normalized_pointers.to_string()
    };

    // Final cleanup: normalize spacing in arrays, pointers, and references
    // [u8 ; 4] -> [u8; 4]
    // * mut c_void -> *mut c_void
    // * const c_void -> *const c_void
    // & mut T -> &mut T
    // & T -> &T
    normalize_spacing(&result)
}

/// Normalize generic type names for FFI compatibility
/// Examples:
///   Option<Box<T>> -> *const c_void (opaque)
///   Option<T> -> OptionT
///   Vec<T> -> TVec
///   Result<T, E> -> ResultTE
///   Box<T> -> *const c_void (opaque)
pub fn normalize_generic_type(type_str: &str) -> (String, Option<GenericTypeInfo>) {
    // Remove all spaces first (syn returns types with spaces like "Option < Box < T > >")
    let no_spaces = type_str.replace(" ", "");
    let trimmed = no_spaces.trim();

    // Special case: Option<Box<T>> -> *const c_void (opaque Rust types in FFI)
    if let Some(inner) = extract_generic_type(trimmed, "Option") {
        if let Some(_box_inner) = extract_generic_type(&inner, "Box") {
            // Option<Box<T>> is opaque in the FFI API
            return ("*const c_void".to_string(), None);
        }

        // Not Option<Box<T>>, so normalize normally
        // Recursively normalize the inner type
        let (inner_normalized, _) = normalize_generic_type(&inner);
        let inner_clean = inner_normalized.replace(" ", "");
        // Use canonicalize_option_type_name for correct casing (OptionU8, not Optionu8)
        let normalized = canonicalize_option_type_name(&inner_clean);
        return (
            normalized.clone(),
            Some(GenericTypeInfo::Option {
                inner_type: inner_clean,
                normalized_name: normalized,
            }),
        );
    }

    // Check for Vec<T>
    if let Some(inner) = extract_generic_type(trimmed, "Vec") {
        // Recursively normalize the inner type
        let (inner_normalized, _) = normalize_generic_type(&inner);
        let inner_clean = inner_normalized.replace(" ", "");
        let normalized = format!("{}Vec", inner_clean);
        return (
            normalized.clone(),
            Some(GenericTypeInfo::Vec {
                inner_type: inner_clean,
                normalized_name: normalized,
            }),
        );
    }

    // Check for Box<T> - always convert to *const c_void (opaque in FFI)
    // Box<T> types cannot be represented in C ABI, so we treat them as opaque pointers
    if let Some(_inner) = extract_generic_type(trimmed, "Box") {
        return ("*const c_void".to_string(), None);
    }

    // Check for Result<T, E>
    if let Some(inner) = extract_generic_type(trimmed, "Result") {
        // Parse Result<T, E> - split by comma at top level
        let parts = split_generic_args(&inner);
        if parts.len() == 2 {
            // Recursively normalize both types
            let (ok_normalized, _) = normalize_generic_type(&parts[0]);
            let (err_normalized, _) = normalize_generic_type(&parts[1]);
            let ok_type = ok_normalized.replace(" ", "");
            let err_type = err_normalized.replace(" ", "");
            let normalized = format!("Result{}{}", ok_type, err_type);
            return (
                normalized.clone(),
                Some(GenericTypeInfo::Result {
                    ok_type,
                    err_type,
                    normalized_name: normalized,
                }),
            );
        }
    }

    // Don't normalize Az prefix here - it should only be done for types from azul_dll
    // and that's handled separately in workspace.rs with path context
    (type_str.to_string(), None)
}

/// Remove the "Az" prefix from a type name if present.
/// This is needed because some types in the source code already have the "Az" prefix
/// (e.g., `AzDuration` in azul_dll), but when stored in api.json they should not have it
/// since the code generator will add the prefix when generating FFI code.
///
/// IMPORTANT: This should ONLY be called for types from azul_dll module, not for types
/// like AzString from azul_core which intentionally have "Az" in their name.
pub fn normalize_az_prefix(type_name: &str) -> String {
    // Only strip "Az" prefix if the remaining part starts with uppercase
    // This prevents stripping from types like "Azure" where "Az" is part of the name
    if type_name.starts_with("Az") && type_name.len() > 2 {
        let rest = &type_name[2..];
        if rest
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
        {
            return rest.to_string();
        }
    }
    type_name.to_string()
}

/// Check if a type name has an "Az" prefix that should be normalized.
/// Returns true if the type name starts with "Az" followed by an uppercase letter.
/// This should ALWAYS be normalized regardless of the source crate, because
/// the code generator adds the "Az" prefix when generating FFI code.
pub fn should_normalize_az_prefix(type_name: &str) -> bool {
    if type_name.starts_with("Az") && type_name.len() > 2 {
        let rest = &type_name[2..];
        return rest
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false);
    }
    false
}

/// Extract the base type and reference kind from a type string
/// Returns (base_type, ref_kind) where:
/// - "*const T" -> ("T", RefKind::ConstPtr)
/// - "*mut T" -> ("T", RefKind::MutPtr)
/// - "&T" -> ("T", RefKind::Ref)
/// - "&mut T" -> ("T", RefKind::RefMut)
/// - "Box<T>" -> ("T", RefKind::Boxed)
/// - "Option<Box<T>>" -> ("T", RefKind::OptionBoxed)
/// - "T" -> ("T", RefKind::Value)
pub fn extract_type_and_ref_kind(type_str: &str) -> (String, crate::api::RefKind) {
    use crate::api::RefKind;

    // Normalize spacing first
    let normalized = type_str.replace(" ", "");
    let trimmed = normalized.trim();

    // Check for raw pointers
    if trimmed.starts_with("*const") {
        let inner = trimmed[6..].trim_start();
        return (inner.to_string(), RefKind::ConstPtr);
    }
    if trimmed.starts_with("*mut") {
        let inner = trimmed[4..].trim_start();
        return (inner.to_string(), RefKind::MutPtr);
    }

    // Check for references
    if trimmed.starts_with("&mut") {
        let inner = trimmed[4..].trim_start();
        return (inner.to_string(), RefKind::RefMut);
    }
    if trimmed.starts_with('&') {
        let inner = trimmed[1..].trim_start();
        return (inner.to_string(), RefKind::Ref);
    }

    // Check for Option<Box<T>>
    if let Some(inner) = extract_generic_type(trimmed, "Option") {
        if let Some(box_inner) = extract_generic_type(&inner, "Box") {
            return (box_inner, RefKind::OptionBoxed);
        }
    }

    // Check for Box<T>
    if let Some(inner) = extract_generic_type(trimmed, "Box") {
        return (inner, RefKind::Boxed);
    }

    // No wrapper - it's a value type
    (trimmed.to_string(), RefKind::Value)
}

/// Normalize raw pointer types to c_void
/// Only c_void can take raw pointers in the C ABI
/// Examples:
///   *const Foo -> *const c_void
///   *mut Bar -> *mut c_void
///   *const c_void -> *const c_void (unchanged)
pub fn normalize_raw_pointers(type_str: &str) -> String {
    let trimmed = type_str.trim();

    // Normalize spacing in raw pointer syntax: "* const " or "* mut " -> "*const " or "*mut "
    let normalized_spacing = trimmed
        .replace("* const ", "*const ")
        .replace("* mut ", "*mut ");

    // Check for *const Type (but not *const c_void)
    if normalized_spacing.starts_with("*const ") {
        let rest = &normalized_spacing[7..]; // Skip "*const "
        if rest.trim() != "c_void" {
            return "*const c_void".to_string();
        }
    }

    // Check for *mut Type (but not *mut c_void)
    if normalized_spacing.starts_with("*mut ") {
        let rest = &normalized_spacing[5..]; // Skip "*mut "
        if rest.trim() != "c_void" {
            return "*mut c_void".to_string();
        }
    }

    normalized_spacing
}

/// Normalize spacing in type strings
pub fn normalize_spacing(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    let mut prev_was_space = false;

    while let Some(ch) = chars.next() {
        match ch {
            // Remove spaces before semicolons in arrays: [u8 ; 4] -> [u8; 4]
            ' ' if chars.peek() == Some(&';') => {
                continue;
            }
            // Remove spaces after semicolons in arrays: [u8; 4] -> [u8; 4]
            ';' => {
                result.push(ch);
                if chars.peek() == Some(&' ') {
                    chars.next(); // skip the space
                }
                prev_was_space = false;
            }
            // Normalize pointer/reference spacing: * mut -> *mut, * const -> *const
            '*' => {
                result.push(ch);
                // Skip spaces after *
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                prev_was_space = false;
            }
            '&' => {
                result.push(ch);
                // Skip spaces after &
                while chars.peek() == Some(&' ') {
                    chars.next();
                }
                prev_was_space = false;
            }
            // Remove multiple consecutive spaces
            ' ' if prev_was_space => {
                continue;
            }
            ' ' => {
                result.push(ch);
                prev_was_space = true;
            }
            _ => {
                result.push(ch);
                prev_was_space = false;
            }
        }
    }

    result
}

/// Extract doc comments from attributes as a multi-line array
/// Each line of documentation becomes a separate element in the vector
pub fn extract_doc_comments(attrs: &[syn::Attribute]) -> Option<Vec<String>> {
    let doc_lines: Vec<String> = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| {
            if let syn::Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let syn::Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value().trim().to_string());
                    }
                }
            }
            None
        })
        // Filter out empty lines
        .filter(|s| !s.is_empty())
        .collect();

    if doc_lines.is_empty() {
        None
    } else {
        Some(doc_lines)
    }
}

/// Canonicalize a type name to its Option wrapper type name.
/// 
/// This is the SINGLE SOURCE OF TRUTH for Option type naming.
/// All code that constructs Option type names MUST use this function
/// to ensure consistent naming (OptionU8, not Optionu8).
///
/// Handles:
/// - Primitive types: u8 -> OptionU8, f32 -> OptionF32
/// - Type aliases: GLint -> OptionI32, GLuint -> OptionU32, ScanCode -> OptionU32
/// - Other types: Foo -> OptionFoo
pub fn canonicalize_option_type_name(element_type: &str) -> String {
    // Handle type aliases first - resolve to underlying primitive
    let resolved_type = match element_type {
        "GLint" => "i32",
        "GLuint" => "u32",
        "ScanCode" => "u32",
        "GLboolean" => "u8",
        "GLenum" => "u32",
        "GLsizei" => "i32",
        "GLint64" => "i64",
        "GLfloat" => "f32",
        other => other,
    };
    
    // Canonicalize primitive types to their Option equivalents with proper casing
    match resolved_type {
        "u8" => "OptionU8".to_string(),
        "u16" => "OptionU16".to_string(),
        "u32" => "OptionU32".to_string(),
        "u64" => "OptionU64".to_string(),
        "i8" => "OptionI8".to_string(),
        "i16" => "OptionI16".to_string(),
        "i32" => "OptionI32".to_string(),
        "i64" => "OptionI64".to_string(),
        "f32" => "OptionF32".to_string(),
        "f64" => "OptionF64".to_string(),
        "bool" => "OptionBool".to_string(),
        "usize" => "OptionUsize".to_string(),
        "isize" => "OptionIsize".to_string(),
        other => format!("Option{}", other),
    }
}
