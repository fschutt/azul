//! Helper functions for generating transmute-based function bodies
//!
//! These functions are used by the Rust code generators to create
//! function bodies that transmute between Az-prefixed local types
//! and the actual implementation types.

use std::collections::{BTreeMap, BTreeSet};

/// Parse a function's argument list into (name, type) pairs
///
/// Handles nested generics like `Option<Vec<T>>`
pub fn parse_fn_args(fn_args: &str) -> Vec<(String, String)> {
    if fn_args.trim().is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();

    // Handle nested generics like Option<Vec<T>>
    for ch in fn_args.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    if let Some((name, ty)) = parse_single_arg(&current) {
                        result.push((name, ty));
                    }
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    // Don't forget the last argument
    if !current.trim().is_empty() {
        if let Some((name, ty)) = parse_single_arg(&current) {
            result.push((name, ty));
        }
    }

    result
}

/// Parse a single argument like "dom: &mut AzDom" into ("dom", "&mut AzDom")
fn parse_single_arg(arg: &str) -> Option<(String, String)> {
    let trimmed = arg.trim();
    let colon_pos = trimmed.find(':')?;
    let name = trimmed[..colon_pos].trim().to_string();
    let ty = trimmed[colon_pos + 1..].trim().to_string();
    Some((name, ty))
}

/// Parse a type string to extract reference/mut info and base type
///
/// - `"&mut AzDom"` -> `(true, true, "AzDom")`
/// - `"&AzDom"` -> `(true, false, "AzDom")`
/// - `"AzDom"` -> `(false, false, "AzDom")`
pub fn parse_arg_type(ty: &str) -> (bool, bool, String) {
    let trimmed = ty.trim();

    if trimmed.starts_with("&mut ") {
        (true, true, trimmed[5..].trim().to_string())
    } else if trimmed.starts_with("&") {
        (true, false, trimmed[1..].trim().to_string())
    } else {
        (false, false, trimmed.to_string())
    }
}

/// Generate a function body that transmutes between local (Az-prefixed) and external types
///
/// The fn_body in api.json uses unprefixed types like "Dom::create_node(node_type)"
/// We need to:
/// 1. Convert the self parameter from Az-prefixed local type to external type (transmute in)
/// 2. Convert ALL arguments from Az-prefixed local types to external types (transmute in)
/// 3. Call the actual function on the external type
/// 4. Convert the result back to Az-prefixed local type (transmute out)
///
/// Now generates multi-line readable code instead of one giant line.
pub fn generate_transmuted_fn_body(
    fn_body: &str,
    class_name: &str,
    is_constructor: bool,
    return_type: &str,
    prefix: &str,
    type_to_external: &BTreeMap<String, String>,
    fn_args: &str,
    is_for_dll: bool,
    keep_self_name: bool, // If true, use "_self" for self parameter (for PyO3 bindings)
    force_clone_self: bool, // If true, always clone self (for PyO3 methods where API says self by-value)
    skip_args: &BTreeSet<String>, // Arguments to skip (already converted with _ffi suffix)
) -> String {
    let self_var = class_name.to_lowercase();
    let parsed_args = parse_fn_args(fn_args);
    
    // For PyO3 bindings (keep_self_name=true), we need to use "_self" as the transmuted variable
    // because Rust doesn't allow shadowing "self"
    let transmuted_self_var = if keep_self_name { "_self" } else { &self_var };

    // Transform the fn_body:
    // 1. For DLL mode: Replace "azul_dll::" with "crate::" (generated code is included in azul-dll
    //    crate) For memtest mode: Keep "azul_dll::" as is (memtest uses azul_dll as dependency)
    // 2. Replace "self." and "classname." with the appropriate variable name
    // 3. Replace "object." with the appropriate variable name (legacy naming convention)
    // 4. Replace unqualified "TypeName::method(" with fully qualified path
    // 5. Replace standalone variable name (as function argument) with transmuted variable
    let mut fn_body = if is_for_dll {
        fn_body.replace("azul_dll::", "crate::")
    } else {
        fn_body.to_string()
    };

    // Only replace "self." with the transmuted variable name, but keep "classname." as-is
    // since we generate an alias `let classname = _self;` below
    fn_body = fn_body.replace("self.", &format!("{}.", transmuted_self_var));
    fn_body = fn_body.replace("object.", &format!("{}.", transmuted_self_var));

    // For constructors: if fn_body starts with "TypeName::" (no "::" before it),
    // replace with the fully qualified external path
    // E.g., "RefAny::new_c(...)" -> "azul_core::refany::RefAny::new_c(...)"
    if is_constructor {
        // Check if fn_body starts with a type name (uppercase letter followed by ::)
        if let Some(colon_pos) = fn_body.find("::") {
            let potential_type = &fn_body[..colon_pos];
            // Check if it's a simple type name (no :: in it, starts with uppercase)
            if !potential_type.contains("::")
                && potential_type
                    .chars()
                    .next()
                    .map(|c| c.is_uppercase())
                    .unwrap_or(false)
            {
                // Look up the type in type_to_external
                let prefixed_type = format!("{}{}", prefix, potential_type);
                if let Some(external_path) = type_to_external.get(&prefixed_type) {
                    // Replace "TypeName::" with "external::path::TypeName::"
                    let replacement = if is_for_dll {
                        format!("{}::", external_path.replace("azul_dll", "crate"))
                    } else {
                        format!("{}::", external_path)
                    };
                    fn_body = fn_body.replacen(&format!("{}::", potential_type), &replacement, 1);
                }
            }
        }
    }

    let mut lines = Vec::new();
    
    // Track if self is a reference - needed for PyO3 bindings where we need to clone
    // for consuming methods (builder pattern)
    let mut self_is_ref = false;

    // Generate transmutations for ALL arguments on separate lines
    for (arg_name, arg_type) in &parsed_args {
        // Skip arguments that are already converted (have _ffi suffix handled elsewhere)
        if skip_args.contains(arg_name) {
            continue;
        }
        
        let (is_ref, is_mut, base_type) = parse_arg_type(arg_type);
        
        // Track if self is a reference
        if arg_name == "self" {
            self_is_ref = is_ref || is_mut;
        }

        // Get the external type for this argument
        // For DLL mode: Replace azul_dll with crate since generated code is included in azul-dll
        // For memtest mode: Keep azul_dll as is since memtest uses azul_dll as dependency
        let external_type = if is_for_dll {
            type_to_external
                .get(&base_type)
                .map(|s| s.replace("azul_dll", "crate"))
                .unwrap_or_else(|| base_type.clone())
        } else {
            type_to_external
                .get(&base_type)
                .cloned()
                .unwrap_or_else(|| base_type.clone())
        };

        // For PyO3 bindings, use "_self" instead of "self" because Rust doesn't allow shadowing self
        let var_name = if keep_self_name && arg_name == "self" {
            "_self"
        } else {
            arg_name.as_str()
        };

        // Generate transmute line based on reference type
        let transmute_line = if is_mut {
            format!(
                "    let {var_name}: &mut {ext} = core::mem::transmute({arg_name});",
                var_name = var_name,
                arg_name = arg_name,
                ext = external_type
            )
        } else if is_ref {
            format!(
                "    let {var_name}: &{ext} = core::mem::transmute({arg_name});",
                var_name = var_name,
                arg_name = arg_name,
                ext = external_type
            )
        } else {
            format!(
                "    let {var_name}: {ext} = core::mem::transmute({arg_name});",
                var_name = var_name,
                arg_name = arg_name,
                ext = external_type
            )
        };

        lines.push(transmute_line);
    }

    // For PyO3 bindings (keep_self_name=true), generate an alias from the lowercase class name
    // to _self, so that fn_body can use the original variable name (e.g., `instant` for Instant)
    // This avoids having to replace all occurrences of the variable name in fn_body
    // IMPORTANT: If self is a reference AND fn_body uses consuming methods (builder pattern),
    // we need to clone. Builder methods like .with_*() consume self.
    // But for methods that just use references (like encode_bmp()), we should NOT clone.
    // ALSO: If force_clone_self is true, we always clone (for PyO3 methods where API says self by-value)
    if keep_self_name && !is_constructor {
        // Detect if fn_body uses builder pattern (consuming methods)
        // Builder pattern methods typically are: .with_*, .set_*, etc. that return Self
        let uses_builder_pattern = fn_body.contains(&format!("{}.with_", self_var))
            || fn_body.contains(&format!("{}.set_", self_var))
            || fn_body.contains(&format!("{}.add_", self_var))
            || fn_body.contains("object.with_")
            || fn_body.contains("_self.with_");
        
        if force_clone_self || (self_is_ref && uses_builder_pattern) {
            // Clone for consuming methods - the fn_body calls methods like .with_node_type()
            // that take self by value, OR the API expects self by value but PyO3 gives us &self
            // Since fn_body uses _self (after object. -> _self. replacement), clone to _self
            // Use a temporary to avoid "use of moved value" error
            lines.push(format!("    let __cloned = _self.clone();"));
            // Now fn_body replacements: replace "_self." with "__cloned." below
        } else {
            lines.push(format!("    let {} = _self;", self_var));
        }
    }
    
    // If we cloned, replace _self with __cloned in fn_body
    if keep_self_name && !is_constructor {
        let uses_builder_pattern = fn_body.contains(&format!("{}.with_", self_var))
            || fn_body.contains(&format!("{}.set_", self_var))
            || fn_body.contains(&format!("{}.add_", self_var))
            || fn_body.contains("object.with_")
            || fn_body.contains("_self.with_");
        if force_clone_self || (self_is_ref && uses_builder_pattern) {
            fn_body = fn_body.replace("_self.", "__cloned.");
            fn_body = fn_body.replace(&format!("{}.", self_var), "__cloned.");
        }
    }

    // Check if fn_body contains statements (has `;` before the last expression)
    let has_statements = fn_body.contains(';');

    if return_type.is_empty() {
        // Void return - just call the function (side effects only)
        if has_statements {
            lines.push(format!("    {}", fn_body));
        } else {
            lines.push(format!("    let _: () = {};", fn_body));
        }
    } else {
        // Has return type - need to transmute the result
        // For DLL mode: Replace azul_dll with crate since generated code is included in azul-dll
        // For memtest mode: Keep azul_dll as is since memtest uses azul_dll as dependency
        let return_external = if is_for_dll {
            type_to_external
                .get(return_type)
                .map(|s| s.as_str())
                .unwrap_or(return_type)
                .replace("azul_dll", "crate")
        } else {
            type_to_external
                .get(return_type)
                .cloned()
                .unwrap_or_else(|| return_type.to_string())
        };

        if has_statements {
            // fn_body has statements - wrap in block and transmute the final result
            lines.push(format!(
                "    let __result: {} = {{ {} }};",
                return_external, fn_body
            ));
        } else {
            // Simple expression - assign to __result
            lines.push(format!(
                "    let __result: {} = {};",
                return_external, fn_body
            ));
        }

        // Transmute result back to local type
        // The From/Into traits handle conversion between wrapper types
        lines.push(format!(
            "    core::mem::transmute::<{ext}, {local}>(__result)",
            ext = return_external,
            local = return_type
        ));
    }

    // Join with newlines and wrap in block
    format!("{{\n{}\n}}", lines.join("\n"))
}
