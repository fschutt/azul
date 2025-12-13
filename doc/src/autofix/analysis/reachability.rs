//! Reachability analysis for unused type detection
//!
//! This module determines which types are actually used in the API
//! and which can be removed.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::autofix::types::{ParsedType, TypeParser};

/// Result of reachability analysis
#[derive(Debug)]
pub struct ReachabilityAnalysis {
    /// Types that are reachable from public API
    pub reachable: HashSet<String>,
    /// Types that are defined but not reachable
    pub unreachable: HashSet<String>,
    /// Entry points (public functions and their types)
    pub entry_points: HashSet<String>,
}

/// Find all unused types in the API
///
/// A type is "used" if:
/// 1. It appears in a public function signature (parameter or return type)
/// 2. It's a field type of a used struct
/// 3. It's a variant type of a used enum
///
/// This uses BFS to find all transitively reachable types.
pub fn find_unused_types(api: &serde_json::Value) -> ReachabilityAnalysis {
    let parser = TypeParser::new();

    // Step 1: Build type dependency graph
    let mut type_deps: HashMap<String, HashSet<String>> = HashMap::new();
    let mut all_types: HashSet<String> = HashSet::new();

    if let Some(classes) = api.get("classes").and_then(|v| v.as_object()) {
        for (class_name, class_def) in classes {
            all_types.insert(class_name.clone());

            let deps = extract_dependencies(&parser, class_def);
            type_deps.insert(class_name.clone(), deps);
        }
    }

    // Step 2: Find entry points (types used in public function signatures)
    let entry_points = find_entry_points(api, &parser);

    // Step 3: BFS from entry points to find all reachable types
    let reachable = bfs_reachability(&entry_points, &type_deps);

    // Step 4: Unreachable = All - Reachable
    let unreachable: HashSet<String> = all_types.difference(&reachable).cloned().collect();

    ReachabilityAnalysis {
        reachable,
        unreachable,
        entry_points,
    }
}

/// Extract type dependencies from a class definition
fn extract_dependencies(parser: &TypeParser, class_def: &serde_json::Value) -> HashSet<String> {
    let mut deps = HashSet::new();

    // From struct fields
    if let Some(fields) = class_def.get("struct_fields").and_then(|v| v.as_object()) {
        for (_name, type_val) in fields {
            if let Some(type_str) = type_val.as_str() {
                extract_types_from_string(parser, type_str, &mut deps);
            }
        }
    }

    // From enum variants
    if let Some(variants) = class_def.get("enum_fields").and_then(|v| v.as_object()) {
        for (_name, type_val) in variants {
            if let Some(type_str) = type_val.as_str() {
                if !type_str.is_empty() {
                    extract_types_from_string(parser, type_str, &mut deps);
                }
            }
        }
    }

    // From constructors
    if let Some(ctors) = class_def.get("constructors").and_then(|v| v.as_object()) {
        for (_name, ctor_def) in ctors {
            extract_fn_types(parser, ctor_def, &mut deps);
        }
    }

    // From functions
    if let Some(funcs) = class_def.get("functions").and_then(|v| v.as_object()) {
        for (_name, fn_def) in funcs {
            extract_fn_types(parser, fn_def, &mut deps);
        }
    }

    deps
}

/// Extract types from a function definition (return + args)
fn extract_fn_types(parser: &TypeParser, fn_def: &serde_json::Value, out: &mut HashSet<String>) {
    // Return type
    if let Some(ret) = fn_def.get("returns").and_then(|v| v.as_str()) {
        extract_types_from_string(parser, ret, out);
    }

    // Arguments - fn_args MUST be an array of objects
    if let Some(fn_args) = fn_def.get("fn_args") {
        if let Some(args_array) = fn_args.as_array() {
            // Correct format: array of objects
            for arg in args_array {
                if let Some(obj) = arg.as_object() {
                    for (key, val) in obj {
                        // Skip self (borrow mode), doc, type metadata
                        if key == "self" || key == "doc" || key == "type" {
                            continue;
                        }
                        if let Some(type_str) = val.as_str() {
                            extract_types_from_string(parser, type_str, out);
                        }
                    }
                }
            }
        } else if let Some(args_obj) = fn_args.as_object() {
            // Legacy format: flat object (deprecated, order not preserved!)
            eprintln!(
                "WARNING: fn_args is a flat object instead of array - argument order may be lost!"
            );
            for (key, val) in args_obj {
                if key == "self" || key == "doc" || key == "type" {
                    continue;
                }
                if let Some(type_str) = val.as_str() {
                    extract_types_from_string(parser, type_str, out);
                }
            }
        }
    }
}

/// Parse a type string and extract user-defined type names
fn extract_types_from_string(parser: &TypeParser, type_str: &str, out: &mut HashSet<String>) {
    let parsed = parser.parse(type_str);
    parsed.collect_user_types(out);
}

/// Find entry point types (used in public API)
///
/// Entry points are:
/// - Types that are callback types (used by user code)
/// - Types that are return values of "new" constructors
/// - Types that appear in functions without "internal_only" flag
fn find_entry_points(api: &serde_json::Value, parser: &TypeParser) -> HashSet<String> {
    let mut entry_points = HashSet::new();

    // Critical types that are always entry points (used externally)
    let critical_types = [
        // Core UI types
        "Dom",
        "StyledDom",
        "NodeData",
        "DomNodeId",
        // Callbacks
        "Callback",
        "IFrameCallback",
        "RenderImageCallback",
        "TimerCallback",
        "ThreadCallback",
        "WriteBackCallback",
        // Rendering
        "Gl",
        "Texture",
        "RawImage",
        "ImageRef",
        // System interaction
        "Clipboard",
        "SystemClipboard",
        "File",
        // Resources
        "ImageCache",
        "FontCache",
        // Window
        "WindowState",
        "Monitor",
        // CSS
        "Css",
        "CssProperty",
        "CssPropertyValue",
        // Events
        "CallbackInfo",
        "HitTest",
        // Animations
        "Animation",
        "AnimationRepeat",
        // Menus
        "Menu",
        "MenuItem",
        // Widgets
        "Button",
        "CheckBox",
        "TextInput",
        "NumberInput",
        "Slider",
        "Dropdown",
        "ColorInput",
        "ProgressBar",
        "Frame",
        "TabContainer",
        "TabHeader",
    ];

    for ty in &critical_types {
        entry_points.insert(ty.to_string());
    }

    // Add types from public functions
    if let Some(classes) = api.get("classes").and_then(|v| v.as_object()) {
        for (class_name, class_def) in classes {
            // Classes with constructors are entry points
            if class_def
                .get("constructors")
                .and_then(|v| v.as_object())
                .map(|m| !m.is_empty())
                .unwrap_or(false)
            {
                entry_points.insert(class_name.clone());
            }

            // Types used in non-internal functions are entry points
            if let Some(funcs) = class_def.get("functions").and_then(|v| v.as_object()) {
                for (_fn_name, fn_def) in funcs {
                    // Skip internal-only functions
                    if fn_def
                        .get("internal_only")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    // Extract types from this public function
                    if let Some(ret) = fn_def.get("returns").and_then(|v| v.as_str()) {
                        extract_types_from_string(parser, ret, &mut entry_points);
                    }
                }
            }
        }
    }

    entry_points
}

/// BFS to find all types reachable from entry points
fn bfs_reachability(
    entry_points: &HashSet<String>,
    type_deps: &HashMap<String, HashSet<String>>,
) -> HashSet<String> {
    let mut reachable = HashSet::new();
    let mut queue: VecDeque<String> = entry_points.iter().cloned().collect();

    while let Some(type_name) = queue.pop_front() {
        if reachable.contains(&type_name) {
            continue;
        }

        reachable.insert(type_name.clone());

        // Add all dependencies of this type
        if let Some(deps) = type_deps.get(&type_name) {
            for dep in deps {
                if !reachable.contains(dep) {
                    queue.push_back(dep.clone());
                }
            }
        }
    }

    reachable
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_critical_types_always_reachable() {
        let api = json!({
            "classes": {
                "Dom": {
                    "struct_fields": {}
                },
                "Clipboard": {
                    "struct_fields": {}
                },
                "InternalOnlyType": {
                    "struct_fields": {}
                }
            }
        });

        let result = find_unused_types(&api);

        assert!(result.reachable.contains("Dom"));
        assert!(result.reachable.contains("Clipboard"));
        // InternalOnlyType might still be reachable if it has constructors
    }

    #[test]
    fn test_transitive_reachability() {
        let parser = TypeParser::new();
        let mut type_deps = HashMap::new();

        // A -> B -> C
        type_deps.insert("A".to_string(), ["B".to_string()].into_iter().collect());
        type_deps.insert("B".to_string(), ["C".to_string()].into_iter().collect());
        type_deps.insert("C".to_string(), HashSet::new());

        let entry = ["A".to_string()].into_iter().collect();
        let reachable = bfs_reachability(&entry, &type_deps);

        assert!(reachable.contains("A"));
        assert!(reachable.contains("B"));
        assert!(reachable.contains("C"));
    }
}
