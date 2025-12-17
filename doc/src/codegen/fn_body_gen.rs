//! Function body generation for C-ABI wrapper functions
//!
//! This module handles the generation of function bodies that:
//! 1. Transmute input arguments from Az-prefixed types to core types
//! 2. Execute the original fn_body from api.json
//! 3. Transmute the result back to Az-prefixed types
//!
//! The key insight is that api.json fn_body values are written against the
//! original core types (e.g., `dom.set_children(children)`), but the generated
//! C-ABI functions receive Az-prefixed types (e.g., `AzDom`, `AzDomVec`).

use std::collections::BTreeMap;

/// Information about a function argument
#[derive(Debug, Clone, PartialEq)]
pub struct FnArg {
    pub name: String,
    /// The Az-prefixed type as it appears in the C-ABI signature
    pub az_type: String,
    /// The core type that the fn_body expects
    pub core_type: String,
    /// Whether this is a reference (&T)
    pub is_ref: bool,
    /// Whether this is a mutable reference (&mut T)
    pub is_mut_ref: bool,
    /// Whether this is the self parameter
    pub is_self: bool,
}

/// Information needed to generate a function body
#[derive(Debug, Clone)]
pub struct FnBodyContext {
    /// The original fn_body from api.json
    pub fn_body: String,
    /// Function arguments with type information
    pub args: Vec<FnArg>,
    /// The Az-prefixed return type (empty string for void)
    pub az_return_type: String,
    /// The core return type (empty string for void)
    pub core_return_type: String,
    /// Whether this is a constructor (no self parameter)
    pub is_constructor: bool,
}

/// Generate a complete function body with transmutations
///
/// # Example output for a method:
/// ```ignore
/// {
///     // Transmute arguments from Az types to core types
///     let dom: &azul_core::dom::Dom = unsafe { core::mem::transmute(dom) };
///     let children: azul_core::dom::DomVec = unsafe { core::mem::transmute(children) };
///     
///     // Execute the original fn_body
///     let __result: azul_core::dom::Dom = {
///         let mut dom = dom.swap_with_default();
///         dom.set_children(children);
///         dom
///     };
///     
///     // Transmute result back to Az type
///     unsafe { core::mem::transmute(__result) }
/// }
/// ```
pub fn generate_fn_body(ctx: &FnBodyContext) -> String {
    let mut lines = Vec::new();

    // 1. Generate transmutations for each argument
    for arg in &ctx.args {
        if arg.is_self {
            // Self parameter needs special handling based on mutability
            if arg.is_mut_ref {
                // &mut self - use swap_with_default pattern
                // The fn_body should call .swap_with_default() itself
                lines.push(format!(
                    "let {name}: &mut {core_type} = unsafe {{ core::mem::transmute({name}) }};",
                    name = arg.name,
                    core_type = arg.core_type
                ));
            } else if arg.is_ref {
                // &self - simple transmute
                lines.push(format!(
                    "let {name}: &{core_type} = unsafe {{ core::mem::transmute({name}) }};",
                    name = arg.name,
                    core_type = arg.core_type
                ));
            } else {
                // self by value - transmute owned
                lines.push(format!(
                    "let {name}: {core_type} = unsafe {{ core::mem::transmute({name}) }};",
                    name = arg.name,
                    core_type = arg.core_type
                ));
            }
        } else {
            // Regular argument
            if arg.is_mut_ref {
                lines.push(format!(
                    "let {name}: &mut {core_type} = unsafe {{ core::mem::transmute({name}) }};",
                    name = arg.name,
                    core_type = arg.core_type
                ));
            } else if arg.is_ref {
                lines.push(format!(
                    "let {name}: &{core_type} = unsafe {{ core::mem::transmute({name}) }};",
                    name = arg.name,
                    core_type = arg.core_type
                ));
            } else {
                lines.push(format!(
                    "let {name}: {core_type} = unsafe {{ core::mem::transmute({name}) }};",
                    name = arg.name,
                    core_type = arg.core_type
                ));
            }
        }
    }

    // 2. Execute fn_body and handle return value
    if ctx.az_return_type.is_empty() {
        // Void return - just execute
        if ctx.fn_body.contains(';') {
            // Multiple statements
            lines.push(ctx.fn_body.clone());
        } else {
            // Single expression - discard result
            lines.push(format!("let _: () = {};", ctx.fn_body));
        }
    } else {
        // Has return value - capture and transmute
        if ctx.fn_body.contains(';') {
            // Multiple statements - wrap in block
            lines.push(format!(
                "let __result: {} = {{ {} }};",
                ctx.core_return_type, ctx.fn_body
            ));
        } else {
            // Single expression
            lines.push(format!(
                "let __result: {} = {};",
                ctx.core_return_type, ctx.fn_body
            ));
        }

        // Transmute result back
        lines.push(format!(
            "unsafe {{ core::mem::transmute::<{}, {}>(__result) }}",
            ctx.core_return_type, ctx.az_return_type
        ));
    }

    // Wrap everything in a block
    format!("{{\n    {}\n}}", lines.join("\n    "))
}

/// Parse a type string to determine if it's a reference and extract the base type
pub fn parse_type_info(type_str: &str) -> (bool, bool, String) {
    let trimmed = type_str.trim();

    if trimmed.starts_with("&mut ") {
        (true, true, trimmed[5..].to_string())
    } else if trimmed.starts_with("&") {
        (true, false, trimmed[1..].to_string())
    } else {
        (false, false, trimmed.to_string())
    }
}

/// Convert an Az-prefixed type to its core type path
pub fn az_type_to_core_type(az_type: &str, type_map: &BTreeMap<String, String>) -> String {
    // Check if we have a mapping
    if let Some(core_path) = type_map.get(az_type) {
        return core_path.clone();
    }

    // Handle Option types
    if az_type.starts_with("Option") || az_type.starts_with("AzOption") {
        // TODO: Handle option types properly
        return az_type.to_string();
    }

    // Handle Vec types
    if az_type.ends_with("Vec") {
        // TODO: Handle vec types properly
        return az_type.to_string();
    }

    // Primitive types don't need conversion
    if is_primitive_type(az_type) {
        return az_type.to_string();
    }

    // Fallback - return as-is
    az_type.to_string()
}

fn is_primitive_type(t: &str) -> bool {
    matches!(
        t,
        "bool"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "isize"
            | "usize"
            | "c_void"
            | "()"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_type_map() -> BTreeMap<String, String> {
        let mut map = BTreeMap::new();
        map.insert("AzDom".to_string(), "azul_core::dom::Dom".to_string());
        map.insert("AzDomVec".to_string(), "azul_core::dom::DomVec".to_string());
        map.insert(
            "AzNodeData".to_string(),
            "azul_core::dom::NodeData".to_string(),
        );
        map.insert(
            "AzGlContextPtr".to_string(),
            "azul_core::gl::GlContextPtr".to_string(),
        );
        map.insert("AzString".to_string(), "azul_css::AzString".to_string());
        map.insert(
            "AzRawImage".to_string(),
            "azul_core::resources::RawImage".to_string(),
        );
        map
    }

    #[test]
    fn test_simple_method_ref_self() {
        let ctx = FnBodyContext {
            fn_body: "glcontextptr.get_type()".to_string(),
            args: vec![FnArg {
                name: "glcontextptr".to_string(),
                az_type: "AzGlContextPtr".to_string(),
                core_type: "azul_core::gl::GlContextPtr".to_string(),
                is_ref: true,
                is_mut_ref: false,
                is_self: true,
            }],
            az_return_type: "AzGlType".to_string(),
            core_return_type: "azul_core::gl::GlType".to_string(),
            is_constructor: false,
        };

        let result = generate_fn_body(&ctx);

        assert!(result.contains("let glcontextptr: &azul_core::gl::GlContextPtr"));
        assert!(result.contains("core::mem::transmute(glcontextptr)"));
        assert!(result.contains("let __result: azul_core::gl::GlType = glcontextptr.get_type()"));
        assert!(
            result.contains("core::mem::transmute::<azul_core::gl::GlType, AzGlType>(__result)")
        );
    }

    #[test]
    fn test_method_with_args() {
        let ctx = FnBodyContext {
            fn_body: "dom.set_children(children)".to_string(),
            args: vec![
                FnArg {
                    name: "dom".to_string(),
                    az_type: "AzDom".to_string(),
                    core_type: "azul_core::dom::Dom".to_string(),
                    is_ref: true,
                    is_mut_ref: true,
                    is_self: true,
                },
                FnArg {
                    name: "children".to_string(),
                    az_type: "AzDomVec".to_string(),
                    core_type: "azul_core::dom::DomVec".to_string(),
                    is_ref: false,
                    is_mut_ref: false,
                    is_self: false,
                },
            ],
            az_return_type: "".to_string(),
            core_return_type: "".to_string(),
            is_constructor: false,
        };

        let result = generate_fn_body(&ctx);

        assert!(result.contains("let dom: &mut azul_core::dom::Dom"));
        assert!(result.contains("let children: azul_core::dom::DomVec"));
        assert!(result.contains("let _: () = dom.set_children(children)"));
    }

    #[test]
    fn test_constructor() {
        let ctx = FnBodyContext {
            fn_body: "Dom::div()".to_string(),
            args: vec![],
            az_return_type: "AzDom".to_string(),
            core_return_type: "azul_core::dom::Dom".to_string(),
            is_constructor: true,
        };

        let result = generate_fn_body(&ctx);

        assert!(result.contains("let __result: azul_core::dom::Dom = Dom::div()"));
        assert!(result.contains("core::mem::transmute::<azul_core::dom::Dom, AzDom>(__result)"));
    }

    #[test]
    fn test_void_return_with_statements() {
        let ctx = FnBodyContext {
            fn_body: "let mut dom = dom.swap_with_default(); dom.set_children(children);"
                .to_string(),
            args: vec![
                FnArg {
                    name: "dom".to_string(),
                    az_type: "AzDom".to_string(),
                    core_type: "azul_core::dom::Dom".to_string(),
                    is_ref: true,
                    is_mut_ref: true,
                    is_self: true,
                },
                FnArg {
                    name: "children".to_string(),
                    az_type: "AzDomVec".to_string(),
                    core_type: "azul_core::dom::DomVec".to_string(),
                    is_ref: false,
                    is_mut_ref: false,
                    is_self: false,
                },
            ],
            az_return_type: "".to_string(),
            core_return_type: "".to_string(),
            is_constructor: false,
        };

        let result = generate_fn_body(&ctx);

        // Should not wrap in "let _: () =" since it has statements
        assert!(
            result.contains("let mut dom = dom.swap_with_default(); dom.set_children(children);")
        );
        assert!(!result.contains("let _: ()"));
    }

    #[test]
    fn test_return_with_block() {
        let ctx = FnBodyContext {
            fn_body: "let mut dom = dom.swap_with_default(); dom.set_children(children); dom"
                .to_string(),
            args: vec![
                FnArg {
                    name: "dom".to_string(),
                    az_type: "AzDom".to_string(),
                    core_type: "azul_core::dom::Dom".to_string(),
                    is_ref: true,
                    is_mut_ref: true,
                    is_self: true,
                },
                FnArg {
                    name: "children".to_string(),
                    az_type: "AzDomVec".to_string(),
                    core_type: "azul_core::dom::DomVec".to_string(),
                    is_ref: false,
                    is_mut_ref: false,
                    is_self: false,
                },
            ],
            az_return_type: "AzDom".to_string(),
            core_return_type: "azul_core::dom::Dom".to_string(),
            is_constructor: false,
        };

        let result = generate_fn_body(&ctx);

        // Should wrap fn_body in a block for multi-statement returns
        assert!(result.contains("let __result: azul_core::dom::Dom = {"));
        assert!(result.contains("let mut dom = dom.swap_with_default()"));
        assert!(result.contains("core::mem::transmute::<azul_core::dom::Dom, AzDom>(__result)"));
    }

    #[test]
    fn test_parse_type_info() {
        assert_eq!(
            parse_type_info("&AzDom"),
            (true, false, "AzDom".to_string())
        );
        assert_eq!(
            parse_type_info("&mut AzDom"),
            (true, true, "AzDom".to_string())
        );
        assert_eq!(
            parse_type_info("AzDom"),
            (false, false, "AzDom".to_string())
        );
        assert_eq!(parse_type_info("u32"), (false, false, "u32".to_string()));
    }

    #[test]
    fn test_gl_uniform_method() {
        // Example: glcontextptr.uniform_2i(location, v0, v1)
        let ctx = FnBodyContext {
            fn_body: "glcontextptr.uniform_2i(location, v0, v1)".to_string(),
            args: vec![
                FnArg {
                    name: "glcontextptr".to_string(),
                    az_type: "AzGlContextPtr".to_string(),
                    core_type: "azul_core::gl::GlContextPtr".to_string(),
                    is_ref: true,
                    is_mut_ref: false,
                    is_self: true,
                },
                FnArg {
                    name: "location".to_string(),
                    az_type: "i32".to_string(),
                    core_type: "i32".to_string(),
                    is_ref: false,
                    is_mut_ref: false,
                    is_self: false,
                },
                FnArg {
                    name: "v0".to_string(),
                    az_type: "i32".to_string(),
                    core_type: "i32".to_string(),
                    is_ref: false,
                    is_mut_ref: false,
                    is_self: false,
                },
                FnArg {
                    name: "v1".to_string(),
                    az_type: "i32".to_string(),
                    core_type: "i32".to_string(),
                    is_ref: false,
                    is_mut_ref: false,
                    is_self: false,
                },
            ],
            az_return_type: "".to_string(),
            core_return_type: "".to_string(),
            is_constructor: false,
        };

        let result = generate_fn_body(&ctx);

        // Self should be transmuted
        assert!(result.contains("let glcontextptr: &azul_core::gl::GlContextPtr"));
        // Primitives should also be transmuted (even though it's a no-op)
        assert!(result.contains("let location: i32"));
        // The fn_body should be executed
        assert!(result.contains("glcontextptr.uniform_2i(location, v0, v1)"));
    }
}
