//! IR Builder - Constructs CodegenIR from api.json
//!
//! This module is responsible for parsing the api.json data structure
//! and building a complete Intermediate Representation (IR) that can
//! be consumed by language-specific generators.

use anyhow::Result;
use indexmap::IndexMap;
use std::collections::BTreeMap;

use super::ir::*;
use crate::api::{ApiData, ClassData, EnumVariantData, FieldData, VersionData};
use crate::utils::string::snake_case_to_lower_camel;

// ============================================================================
// IR Builder
// ============================================================================

/// Builder for constructing CodegenIR from api.json data
pub struct IRBuilder<'a> {
    /// Reference to the API data
    version_data: &'a VersionData,

    /// The IR being built
    ir: CodegenIR,
}

impl<'a> IRBuilder<'a> {
    /// Create a new IR builder
    pub fn new(version_data: &'a VersionData) -> Self {
        Self {
            version_data,
            ir: CodegenIR::new(),
        }
    }

    /// Build the complete IR from api.json
    pub fn build(mut self) -> Result<CodegenIR> {
        // Phase 0: Validate api.json for disallowed patterns
        self.validate_api_json()?;

        // Phase 1: Build type lookup tables
        self.build_type_lookups()?;

        // Phase 2: Build struct and enum definitions
        self.build_type_definitions()?;

        // Phase 3: Build callback typedefs
        self.build_callback_typedefs()?;

        // Phase 4: Build type aliases
        self.build_type_aliases()?;

        // Phase 5: Link callback wrapper structs to their callback_typedefs
        // Must happen after both structs and callback_typedefs are built
        self.link_callback_wrappers();

        // Phase 6: Build functions from api.json (constructors, methods)
        self.build_api_functions()?;

        // Phase 6b: Generate enum variant constructors automatically
        self.build_enum_variant_constructors();

        // Phase 7: Generate trait functions (_deepCopy, _delete, _partialEq, etc.)
        self.build_trait_functions()?;

        // Phase 8: Build constants from api.json
        self.build_constants()?;

        // Phase 9: Sort types by dependency depth (topological sort)
        // This ensures types are defined before they are used
        self.sort_types_by_dependencies();

        Ok(self.ir)
    }

    /// Validate api.json for disallowed patterns
    ///
    /// This checks for:
    /// 1. Array types like [T; N] - should be replaced with proper structs
    /// 2. Direct type aliases without generics - should be replaced with actual struct/enum definitions
    /// 3. Non-FFI-safe types (NonZeroUsize, std library types, etc.)
    fn validate_api_json(&self) -> Result<()> {
        let mut errors: Vec<String> = Vec::new();

        // Types that are not FFI-safe and should not appear in the public API
        // These must be checked with careful pattern matching to avoid false positives
        // (e.g., "BoxSizing" should NOT match "Box<T>")

        /// Check if a type string contains any non-FFI-safe type
        /// Returns the problematic type name if found
        fn contains_non_ffi_safe_type(type_str: &str) -> Option<&'static str> {
            // Check for generic types with angle brackets (Box<T>, Arc<T>, etc.)
            const GENERIC_NON_FFI_TYPES: &[&str] = &[
                "Box<",
                "Arc<",
                "Rc<",
                "Mutex<",
                "RwLock<",
                "Cell<",
                "RefCell<",
                "BTreeMap<",
                "HashMap<",
                "HashSet<",
                "BTreeSet<",
                "FastBTreeSet<",
                "FastHashMap<",
                "FastHashSet<",
                "Vec<", // Generic Vec (not our specialized Vec types)
            ];

            for &generic in GENERIC_NON_FFI_TYPES {
                if type_str.contains(generic) {
                    return Some(generic.trim_end_matches('<'));
                }
            }

            // Check for standalone non-FFI types (must be exact word match)
            const STANDALONE_NON_FFI_TYPES: &[&str] = &[
                "NonZeroUsize",
                "NonZeroU8",
                "NonZeroU16",
                "NonZeroU32",
                "NonZeroU64",
                "NonZeroIsize",
                "NonZeroI8",
                "NonZeroI16",
                "NonZeroI32",
                "NonZeroI64",
                "FcFontCache",
                "ImageCache",
                "CallbackInfoRefData",
                "LayoutCallbackInfoRefData",
                "CssPropertyCache",
            ];

            for &standalone in STANDALONE_NON_FFI_TYPES {
                // Check if it's a complete word (not part of another identifier)
                if type_str == standalone {
                    return Some(standalone);
                }
                // Check if it appears as a standalone type (not part of a larger name)
                // by checking for word boundaries
                let patterns = [
                    format!("{}<", standalone),        // Generic usage
                    format!("Option<{}>", standalone), // Inside Option
                    format!("&{}", standalone),        // Reference
                    format!("&mut {}", standalone),    // Mutable reference
                ];
                for pattern in &patterns {
                    if type_str.contains(pattern.as_str()) {
                        return Some(standalone);
                    }
                }
            }

            None
        }

        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Check struct fields for array types and non-FFI-safe types
                if let Some(struct_fields) = &class_data.struct_fields {
                    for field_map in struct_fields {
                        for (field_name, field_data) in field_map {
                            if is_array_type(&field_data.r#type) {
                                errors.push(format!(
                                    "Array type not allowed: {}.{} has type '{}'. \
                                     Use a dedicated struct instead (e.g., PixelValueSize).",
                                    class_name, field_name, field_data.r#type
                                ));
                            }
                            if let Some(bad_type) = contains_non_ffi_safe_type(&field_data.r#type) {
                                errors.push(format!(
                                    "Non-FFI-safe type in struct field: {}.{} uses '{}'. \
                                     Remove this type from api.json or wrap it in an FFI-safe wrapper.",
                                    class_name, field_name, bad_type
                                ));
                            }
                        }
                    }
                }

                // Check enum variant payloads for array types and non-FFI-safe types
                if let Some(enum_fields) = &class_data.enum_fields {
                    for variant_map in enum_fields {
                        for (variant_name, variant_data) in variant_map {
                            if let Some(variant_type) = &variant_data.r#type {
                                if is_array_type(variant_type) {
                                    errors.push(format!(
                                        "Array type not allowed: {}::{} has type '{}'. \
                                         Use a dedicated struct instead.",
                                        class_name, variant_name, variant_type
                                    ));
                                }
                                if let Some(bad_type) = contains_non_ffi_safe_type(variant_type) {
                                    errors.push(format!(
                                        "Non-FFI-safe type in enum variant: {}::{} uses '{}'. \
                                         Remove this type from api.json or wrap it in an FFI-safe wrapper.",
                                        class_name, variant_name, bad_type
                                    ));
                                }
                            }
                        }
                    }
                }

                // Check function arguments and return types for non-FFI-safe types
                if let Some(functions) = &class_data.functions {
                    for (fn_name, fn_data) in functions {
                        // Check arguments
                        for arg in &fn_data.fn_args {
                            for (_arg_name, arg_type) in arg {
                                if let Some(bad_type) = contains_non_ffi_safe_type(arg_type) {
                                    errors.push(format!(
                                        "Non-FFI-safe type in function argument: {}.{}() uses '{}'. \
                                         Remove this function from api.json.",
                                        class_name, fn_name, bad_type
                                    ));
                                }
                            }
                        }
                        // Check return type
                        if let Some(ret_type) = &fn_data.returns {
                            if let Some(bad_type) = contains_non_ffi_safe_type(&ret_type.r#type) {
                                errors.push(format!(
                                    "Non-FFI-safe return type: {}.{}() returns '{}'. \
                                     Remove this function from api.json.",
                                    class_name, fn_name, bad_type
                                ));
                            }
                        }
                    }
                }

                // Check constructors for non-FFI-safe types
                if let Some(constructors) = &class_data.constructors {
                    for (ctor_name, ctor_data) in constructors {
                        for arg in &ctor_data.fn_args {
                            for (_arg_name, arg_type) in arg {
                                if let Some(bad_type) = contains_non_ffi_safe_type(arg_type) {
                                    errors.push(format!(
                                        "Non-FFI-safe type in constructor: {}.{}() uses '{}'. \
                                         Remove this constructor from api.json.",
                                        class_name, ctor_name, bad_type
                                    ));
                                }
                            }
                        }
                    }
                }

                // Check for direct type aliases without generics (not pointing to a generic type)
                // These are problematic for codegen because:
                // 1. They cause type ordering issues (the alias may appear before its target)
                // 2. They don't add semantic meaning (just renaming)
                // 3. They complicate FFI binding generation
                //
                // Allowed exceptions:
                // - Primitive type aliases like `type GLuint = u32` (for C-API compatibility)
                // - Opaque pointers like `type X11Visual = *const c_void` (with ref_kind)
                //
                // NOT allowed (must use newtype struct instead):
                // - `type XmlTagName = String` -> struct XmlTagName { inner: String }
                // - `type XmlAttributeMap = StringPairVec` -> struct XmlAttributeMap { inner: StringPairVec }
                if let Some(type_alias) = &class_data.type_alias {
                    if type_alias.generic_args.is_empty() {
                        let target = &type_alias.target;

                        // Primitives are OK (C-API naming like GLuint, GLint, etc.)
                        let is_primitive_alias = matches!(
                            target.as_str(),
                            "u8" | "u16"
                                | "u32"
                                | "u64"
                                | "usize"
                                | "i8"
                                | "i16"
                                | "i32"
                                | "i64"
                                | "isize"
                                | "f32"
                                | "f64"
                                | "bool"
                                | "char"
                                | "c_void"
                        );

                        // Pointer types are OK (opaque handles like X11Visual, HwndHandle)
                        let is_pointer_alias = matches!(
                            type_alias.ref_kind,
                            crate::api::RefKind::ConstPtr | crate::api::RefKind::MutPtr
                        );

                        if !is_primitive_alias && !is_pointer_alias {
                            errors.push(format!(
                                "Simple type alias not allowed: {} = {}. \n\
                                 Simple type aliases cause codegen issues (type ordering, FFI complexity).\n\
                                 Please convert to a newtype struct instead:\n\
                                 \n\
                                 In Rust source:\n\
                                 ```rust\n\
                                 #[derive(Default, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]\n\
                                 #[repr(C)]\n\
                                 pub struct {} {{\n\
                                     pub inner: {},\n\
                                 }}\n\
                                 \n\
                                 impl From<{}> for {} {{\n\
                                     fn from(v: {}) -> Self {{ Self {{ inner: v }} }}\n\
                                 }}\n\
                                 ```\n\
                                 \n\
                                 In api.json, change from:\n\
                                 ```json\n\
                                 \"{}\": {{ \"type_alias\": {{ \"target\": \"{}\" }} }}\n\
                                 ```\n\
                                 \n\
                                 To:\n\
                                 ```json\n\
                                 \"{}\": {{ \"struct_fields\": [{{ \"inner\": {{ \"type\": \"{}\" }} }}], \"derive\": [...] }}\n\
                                 ```",
                                class_name, target,
                                class_name, target,
                                target, class_name, target,
                                class_name, target,
                                class_name, target
                            ));
                        }
                    }
                }

                // Validate repr annotation consistency
                // Convention: structs → "C", simple enums (no data) → "C", enums with data → "C, u8"
                if let Some(repr) = &class_data.repr {
                    if class_data.struct_fields.is_some() {
                        // Struct: must be repr(C)
                        if repr != "C" && repr != "transparent" {
                            errors.push(format!(
                                "Invalid repr for struct {}: got repr({}), expected repr(C). \
                                 Structs must use #[repr(C)] for FFI safety.",
                                class_name, repr
                            ));
                        }
                    } else if let Some(enum_fields) = &class_data.enum_fields {
                        let has_data = enum_fields.iter()
                            .flat_map(|m| m.values())
                            .any(|v| v.r#type.is_some());

                        if has_data {
                            // Tagged union: must be repr(C, u8)
                            if repr != "C, u8" {
                                errors.push(format!(
                                    "Invalid repr for tagged enum {}: got repr({}), expected repr(C, u8). \
                                     Enums with variant data must use #[repr(C, u8)].",
                                    class_name, repr
                                ));
                            }
                        } else {
                            // Simple enum: must be repr(C)
                            if repr != "C" {
                                errors.push(format!(
                                    "Invalid repr for simple enum {}: got repr({}), expected repr(C). \
                                     Enums without variant data must use #[repr(C)].",
                                    class_name, repr
                                ));
                            }
                        }
                    }
                }

                // Check for reserved function names that conflict with auto-generated trait functions
                const RESERVED_FN_NAMES: &[&str] = &[
                    "hash",
                    "partialEq",
                    "partialCmp",
                    "cmp",
                    "deepCopy",
                    "delete",
                    "eq",
                    "clone",
                    "default",
                    "debug",
                    "display",
                ];

                if let Some(functions) = &class_data.functions {
                    for (fn_name, _fn_data) in functions {
                        if RESERVED_FN_NAMES.contains(&fn_name.as_str()) {
                            errors.push(format!(
                                "Reserved function name not allowed: {}.{}(). \
                                 This name conflicts with auto-generated trait functions. \
                                 Please rename the function (e.g., 'hash' -> 'nodeDataHash').",
                                class_name, fn_name
                            ));
                        }
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "API validation failed with {} error(s):\n  - {}",
                errors.len(),
                errors.join("\n  - ")
            ))
        }
    }

    /// Analyze and sort all types by their dependencies (topological sort)
    ///
    /// This populates the dependency tracking fields in the IR:
    /// - `dependencies`: List of type names this type depends on
    /// - `sort_order`: Position in topological order (lower = earlier)
    /// - `needs_forward_decl`: Whether this type needs a forward declaration
    ///
    /// This is critical for C/C++ code generation where types must be defined
    /// before they are used as field types (not pointers).
    fn sort_types_by_dependencies(&mut self) {
        use std::collections::{BTreeMap, BTreeSet};

        // Build a unified set of all type names (including type aliases with monomorphized defs)
        let all_type_names: BTreeSet<String> = self
            .ir
            .structs
            .iter()
            .map(|s| s.name.clone())
            .chain(self.ir.enums.iter().map(|e| e.name.clone()))
            .chain(self.ir.callback_typedefs.iter().map(|c| c.name.clone()))
            .chain(self.ir.type_aliases.iter().map(|t| t.name.clone()))
            .collect();

        // Build a set of types that CAN have forward declarations in C:
        // - Structs: `struct X;` is valid
        // - Unions (tagged enums with is_union=true): `union X;` is valid
        // Types that CANNOT have forward declarations:
        // - Simple enums: need full definition
        // - Type aliases: need full definition
        let forward_declarable_types: BTreeSet<String> = self
            .ir
            .structs
            .iter()
            .map(|s| s.name.clone())
            .chain(
                self.ir
                    .enums
                    .iter()
                    .filter(|e| e.is_union) // Only tagged unions, not simple enums
                    .map(|e| e.name.clone()),
            )
            .collect();

        // Collect dependencies for all types into a unified map
        // IMPORTANT: Direct field references are always dependencies.
        // Pointer references to forward-declarable types (structs, unions) only need forward decls,
        // BUT pointer references to simple enums/type aliases ARE dependencies
        let mut all_deps: BTreeMap<String, Vec<String>> = BTreeMap::new();
        // Struct dependencies (from field types)
        for struct_def in &self.ir.structs {
            let deps: Vec<String> = struct_def
                .fields
                .iter()
                .filter_map(|field| {
                    let base = self.extract_base_type(&field.type_name);
                    
                    // Skip primitives and unknown types
                    if self.is_primitive_type(&base) || !all_type_names.contains(&base) {
                        return None;
                    }
                    
                    // Check if this is a pointer reference
                    let is_pointer = self.is_pointer_type(&field.type_name) || matches!(
                        field.ref_kind,
                        FieldRefKind::Ptr
                        | FieldRefKind::PtrMut
                        | FieldRefKind::Ref
                        | FieldRefKind::RefMut
                        | FieldRefKind::Boxed
                        | FieldRefKind::OptionBoxed
                    );
                    
                    // For pointer references to structs, forward decl is enough - skip
                    // BUT for pointer references to enums, we need the full type (C limitation)
                    if is_pointer && forward_declarable_types.contains(&base) {
                        return None;
                    }
                    
                    Some(base)
                })
                .collect();
            all_deps.insert(struct_def.name.clone(), deps);
        }

        // Enum dependencies (from variant payloads)
        for enum_def in &self.ir.enums {
            let deps: Vec<String> = enum_def
                .variants
                .iter()
                .flat_map(|variant| {
                    match &variant.kind {
                        EnumVariantKind::Tuple(types) => {
                            types
                                .iter()
                                .filter_map(|(t, _ref_kind)| {
                                    let base = self.extract_base_type(t);
                                    
                                    // Skip primitives and unknown types
                                    if self.is_primitive_type(&base) || !all_type_names.contains(&base) {
                                        return None;
                                    }
                                    
                                    // For pointer refs to structs, skip (forward decl is enough)
                                    // For pointer refs to enums, include (C limitation)
                                    let is_pointer = self.is_pointer_type(t);
                                    if is_pointer && forward_declarable_types.contains(&base) {
                                        return None;
                                    }
                                    
                                    Some(base)
                                })
                                .collect::<Vec<_>>()
                        }
                        EnumVariantKind::Struct(fields) => {
                            fields
                                .iter()
                                .filter_map(|f| {
                                    let base = self.extract_base_type(&f.type_name);
                                    
                                    // Skip primitives and unknown types
                                    if self.is_primitive_type(&base) || !all_type_names.contains(&base) {
                                        return None;
                                    }
                                    
                                    // Check if this is a pointer reference
                                    let is_pointer = self.is_pointer_type(&f.type_name) || matches!(
                                        f.ref_kind,
                                        FieldRefKind::Ptr
                                        | FieldRefKind::PtrMut
                                        | FieldRefKind::Ref
                                        | FieldRefKind::RefMut
                                        | FieldRefKind::Boxed
                                        | FieldRefKind::OptionBoxed
                                    );
                                    
                                    // For pointer refs to structs, skip (forward decl is enough)
                                    // For pointer refs to enums, include (C limitation)
                                    if is_pointer && forward_declarable_types.contains(&base) {
                                        return None;
                                    }
                                    
                                    Some(base)
                                })
                                .collect::<Vec<_>>()
                        }
                        EnumVariantKind::Unit => Vec::new(),
                    }
                })
                .collect();
            all_deps.insert(enum_def.name.clone(), deps);
        }

        // Callback typedef dependencies (from argument types and return type)
        // Note: Callback args are always passed by pointer in C, so no dependencies needed
        for callback in &self.ir.callback_typedefs {
            // Callbacks only need forward declarations for their argument types
            // because arguments are always pointers or primitives in C ABI
            all_deps.insert(callback.name.clone(), Vec::new());
        }

        // Type alias dependencies (from monomorphized variants/fields)
        for type_alias in &self.ir.type_aliases {
            let mut deps: Vec<String> = Vec::new();

            if let Some(ref mono_def) = type_alias.monomorphized_def {
                match &mono_def.kind {
                    MonomorphizedKind::TaggedUnion { variants, .. } => {
                        for variant in variants {
                            if let Some(ref payload_type) = variant.payload_type {
                                let base = self.extract_base_type(payload_type);
                                
                                // Skip primitives and unknown types
                                if self.is_primitive_type(&base) || !all_type_names.contains(&base) {
                                    continue;
                                }
                                
                                // For pointer refs to structs, skip (forward decl is enough)
                                // For pointer refs to enums, include (C limitation)
                                let is_pointer = self.is_pointer_type(payload_type);
                                if is_pointer && forward_declarable_types.contains(&base) {
                                    continue;
                                }
                                
                                deps.push(base);
                            }
                        }
                    }
                    MonomorphizedKind::Struct { fields } => {
                        for field in fields {
                            let base = self.extract_base_type(&field.type_name);
                            
                            // Skip primitives and unknown types
                            if self.is_primitive_type(&base) || !all_type_names.contains(&base) {
                                continue;
                            }
                            
                            // Check if this is a pointer reference
                            let is_pointer = self.is_pointer_type(&field.type_name) || matches!(
                                field.ref_kind,
                                FieldRefKind::Ptr
                                | FieldRefKind::PtrMut
                                | FieldRefKind::Ref
                                | FieldRefKind::RefMut
                                | FieldRefKind::Boxed
                                | FieldRefKind::OptionBoxed
                            );
                            
                            // For pointer refs to structs, skip (forward decl is enough)
                            // For pointer refs to enums, include (C limitation)
                            if is_pointer && forward_declarable_types.contains(&base) {
                                continue;
                            }
                            
                            deps.push(base);
                        }
                    }
                    MonomorphizedKind::SimpleEnum { .. } => {
                        // Simple enums have no type dependencies
                    }
                }
            }

            all_deps.insert(type_alias.name.clone(), deps);
        }

        // Calculate depths using iterative algorithm
        let mut depths: BTreeMap<String, usize> = BTreeMap::new();

        // Initialize: types with no dependencies have depth 0
        for (name, deps) in &all_deps {
            if deps.is_empty() {
                depths.insert(name.clone(), 0);
            }
        }

        // Iteratively resolve depths
        for _ in 0..500 {
            let mut changed = false;

            for (name, deps) in &all_deps {
                if depths.contains_key(name) {
                    continue;
                }

                // Check if all dependencies are resolved
                let all_deps_resolved = deps.iter().all(|dep| depths.contains_key(dep));

                if all_deps_resolved {
                    let max_dep_depth = deps
                        .iter()
                        .filter_map(|dep| depths.get(dep).copied())
                        .max()
                        .unwrap_or(0);
                    depths.insert(name.clone(), max_dep_depth + 1);
                    changed = true;
                }
            }

            if !changed {
                break;
            }
        }

        // Handle circular dependencies: assign incrementally higher depths
        let max_depth = depths.values().copied().max().unwrap_or(0);
        let mut next_depth = max_depth + 1;
        for name in all_deps.keys() {
            if !depths.contains_key(name) {
                depths.insert(name.clone(), next_depth);
                next_depth += 1;
            }
        }

        // Update struct IR with dependencies and sort order
        for struct_def in &mut self.ir.structs {
            struct_def.dependencies = all_deps.get(&struct_def.name).cloned().unwrap_or_default();
            struct_def.sort_order = depths.get(&struct_def.name).copied().unwrap_or(usize::MAX);
            // Structs with circular dependencies need forward declarations
            struct_def.needs_forward_decl = struct_def.sort_order > max_depth;
        }

        // Update enum IR with dependencies and sort order
        for enum_def in &mut self.ir.enums {
            enum_def.dependencies = all_deps.get(&enum_def.name).cloned().unwrap_or_default();
            enum_def.sort_order = depths.get(&enum_def.name).copied().unwrap_or(usize::MAX);
            enum_def.needs_forward_decl = enum_def.sort_order > max_depth;
        }

        // Update callback typedef IR with dependencies and sort order
        for callback in &mut self.ir.callback_typedefs {
            callback.dependencies = all_deps.get(&callback.name).cloned().unwrap_or_default();
            callback.sort_order = depths.get(&callback.name).copied().unwrap_or(usize::MAX);
        }

        // Update type alias IR with dependencies and sort order
        for type_alias in &mut self.ir.type_aliases {
            type_alias.dependencies = all_deps.get(&type_alias.name).cloned().unwrap_or_default();
            type_alias.sort_order = depths.get(&type_alias.name).copied().unwrap_or(usize::MAX);
        }

        // Sort all collections by depth
        self.ir.structs.sort_by_key(|s| s.sort_order);
        self.ir.enums.sort_by_key(|e| e.sort_order);
        self.ir.callback_typedefs.sort_by_key(|c| c.sort_order);
        self.ir.type_aliases.sort_by_key(|t| t.sort_order);
    }

    /// Check if a type name is a primitive type
    fn is_primitive_type(&self, type_name: &str) -> bool {
        matches!(
            type_name,
            "bool"
                | "u8"
                | "u16"
                | "u32"
                | "u64"
                | "usize"
                | "i8"
                | "i16"
                | "i32"
                | "i64"
                | "isize"
                | "f32"
                | "f64"
                | "c_void"
                | "()"
                | "c_int"
                | "c_uint"
                | "c_long"
                | "c_ulong"
                | "c_char"
                | "c_uchar"
        )
    }

    /// Check if a type is accessed through a pointer (only needs forward declaration in C)
    /// Pointer types include: *const T, *mut T, Box<T>, &T, &mut T
    fn is_pointer_type(&self, type_name: &str) -> bool {
        let s = type_name.trim();
        s.starts_with("*const ")
            || s.starts_with("*mut ")
            || s.starts_with("* const ")
            || s.starts_with("* mut ")
            || s.starts_with("Box<")
            || s.starts_with("&mut ")
            || s.starts_with("&")
    }

    /// Extract base type name from a complex type (removes pointers, generics, arrays)
    fn extract_base_type(&self, type_name: &str) -> String {
        let mut s = type_name.trim();

        // Remove pointer prefixes
        s = s.strip_prefix("*const ").unwrap_or(s);
        s = s.strip_prefix("*mut ").unwrap_or(s);
        s = s.strip_prefix("* const ").unwrap_or(s);
        s = s.strip_prefix("* mut ").unwrap_or(s);

        // Remove generic suffix
        if let Some(idx) = s.find('<') {
            s = &s[..idx];
        }

        // Remove array syntax
        if s.starts_with('[') && s.contains(';') {
            if let Some(idx) = s.find(';') {
                s = s[1..idx].trim();
            }
        }

        s.trim().to_string()
    }

    // ========================================================================
    // Phase 1: Type Lookups
    // ========================================================================

    fn build_type_lookups(&mut self) -> Result<()> {
        // TODO: Iterate through all modules and classes
        // TODO: Build type_to_module map
        // TODO: Build type_to_external map from class.external field

        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                self.ir
                    .type_to_module
                    .insert(class_name.clone(), module_name.clone());

                if let Some(ref external) = class_data.external {
                    self.ir
                        .type_to_external
                        .insert(class_name.clone(), external.clone());
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // Phase 2: Type Definitions
    // ========================================================================

    fn build_type_definitions(&mut self) -> Result<()> {
        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Skip callback typedefs (handled separately)
                if class_data.callback_typedef.is_some() {
                    continue;
                }

                // Skip type aliases (handled separately)
                if class_data.type_alias.is_some() {
                    continue;
                }

                if class_data.struct_fields.is_some() {
                    // This is a struct
                    let struct_def = self.build_struct_def(class_name, class_data, module_name)?;
                    self.ir.structs.push(struct_def);
                } else if class_data.enum_fields.is_some() {
                    // This is an enum
                    let enum_def = self.build_enum_def(class_name, class_data, module_name)?;
                    self.ir.enums.push(enum_def);
                }
            }
        }

        Ok(())
    }

    fn build_struct_def(
        &self,
        name: &str,
        class_data: &ClassData,
        module: &str,
    ) -> Result<StructDef> {
        let derives = class_data.derive.clone().unwrap_or_default();
        let custom_impls = class_data.custom_impls.clone().unwrap_or_default();
        let has_explicit_derive = class_data.derive.is_some();
        let has_custom_drop = class_data.has_custom_drop();
        let mut traits =
            TypeTraits::from_derives_and_custom_impls(&derives, &custom_impls, has_custom_drop);

        let fields = self.build_struct_fields(class_data)?;

        // Classify the type category
        let category = classify_struct_type(name, class_data, self.version_data);

        // Destructor and callback types are Copy+Clone (they only contain function pointers)
        if matches!(category, TypeCategory::DestructorOrClone) {
            traits.is_copy = true;
            traits.is_clone = true;
            traits.clone_is_derived = true; // These can be derived
        }

        // Vec types should have Clone (they have a deep_copy function that clones the data)
        // Vec types end with "Vec" but NOT "VecRef"
        // IMPORTANT: Vec Clone CANNOT be derived! The derive(Clone) would do a bitwise copy,
        // copying the pointer without allocating new memory, leading to double-free on drop.
        // Vec types have a custom clone_self() that properly allocates and copies the data.
        // The Clone trait must be implemented via transmute to the real type's .clone() method.
        if name.ends_with("Vec") && !name.ends_with("VecRef") {
            traits.is_clone = true;
            traits.clone_is_derived = false; // Vec Clone MUST NOT be derived - needs custom impl
        }

        Ok(StructDef {
            name: name.to_string(),
            doc: class_data.doc.clone().unwrap_or_default(),
            fields,
            external_path: class_data.external.clone(),
            module: module.to_string(),
            derives,
            has_explicit_derive,
            custom_impls: class_data.custom_impls.clone().unwrap_or_default(),
            is_boxed: class_data.is_boxed_object,
            repr: class_data.repr.clone(),
            // Vec module types are semantically Send+Sync safe (like Rust's Vec<T>)
            is_send_safe: module == "vec",
            generic_params: class_data.generic_params.clone().unwrap_or_default(),
            traits,
            category,
            // Will be populated in link_callback_wrappers phase
            callback_wrapper_info: None,
            // Will be populated in analyze_dependencies phase
            dependencies: Vec::new(),
            sort_order: 0,
            needs_forward_decl: false,
        })
    }

    fn build_struct_fields(&self, class_data: &ClassData) -> Result<Vec<FieldDef>> {
        // TODO: Iterate through class_data.struct_fields
        // TODO: Build FieldDef for each field with proper type analysis

        let mut fields = Vec::new();

        if let Some(ref struct_fields) = class_data.struct_fields {
            for field_map in struct_fields {
                for (field_name, field_data) in field_map {
                    let ref_kind = match &field_data.ref_kind {
                        crate::api::RefKind::Ref => FieldRefKind::Ref,
                        crate::api::RefKind::RefMut => FieldRefKind::RefMut,
                        crate::api::RefKind::ConstPtr => FieldRefKind::Ptr,
                        crate::api::RefKind::MutPtr => FieldRefKind::PtrMut,
                        crate::api::RefKind::Value => FieldRefKind::Owned,
                        crate::api::RefKind::Boxed => FieldRefKind::Boxed,
                        crate::api::RefKind::OptionBoxed => FieldRefKind::OptionBoxed,
                    };

                    fields.push(FieldDef {
                        name: field_name.clone(),
                        type_name: field_data.r#type.clone(),
                        doc: field_data.doc.as_ref().and_then(|d| d.first().cloned()),
                        is_public: true,
                        ref_kind,
                    });
                }
            }
        }

        Ok(fields)
    }

    fn build_enum_def(&self, name: &str, class_data: &ClassData, module: &str) -> Result<EnumDef> {
        let derives = class_data.derive.clone().unwrap_or_default();
        let custom_impls = class_data.custom_impls.clone().unwrap_or_default();
        let has_explicit_derive = class_data.derive.is_some();
        let has_custom_drop = class_data.has_custom_drop();
        let mut traits =
            TypeTraits::from_derives_and_custom_impls(&derives, &custom_impls, has_custom_drop);

        let (variants, is_union) = self.build_enum_variants(class_data)?;

        // Classify the type category
        let category = classify_enum_type(name, class_data, self.version_data);

        // Destructor and callback types are Copy+Clone (they only contain function pointers)
        if matches!(category, TypeCategory::DestructorOrClone) {
            traits.is_copy = true;
            traits.is_clone = true;
            traits.clone_is_derived = true; // These can be derived
        }

        Ok(EnumDef {
            name: name.to_string(),
            doc: class_data.doc.clone().unwrap_or_default(),
            variants,
            external_path: class_data.external.clone(),
            module: module.to_string(),
            derives,
            has_explicit_derive,
            is_union,
            repr: class_data.repr.clone(),
            // Vec module types are semantically Send+Sync safe
            is_send_safe: module == "vec",
            traits,
            generic_params: class_data.generic_params.clone().unwrap_or_default(),
            category,
            // Will be populated in analyze_dependencies phase
            dependencies: Vec::new(),
            sort_order: 0,
            needs_forward_decl: false,
        })
    }

    fn build_enum_variants(&self, class_data: &ClassData) -> Result<(Vec<EnumVariantDef>, bool)> {
        // TODO: Parse enum_fields and determine variant kinds
        // TODO: Return (variants, is_union)

        let mut variants = Vec::new();
        let mut is_union = false;

        if let Some(ref enum_fields) = class_data.enum_fields {
            for variant_map in enum_fields {
                for (variant_name, variant_data) in variant_map {
                    let kind = self.build_variant_kind(variant_data)?;

                    if !matches!(kind, EnumVariantKind::Unit) {
                        is_union = true;
                    }

                    variants.push(EnumVariantDef {
                        name: variant_name.clone(),
                        doc: variant_data.doc.as_ref().and_then(|d| d.first().cloned()),
                        kind,
                    });
                }
            }
        }

        Ok((variants, is_union))
    }

    fn build_variant_kind(&self, variant_data: &EnumVariantData) -> Result<EnumVariantKind> {
        // In api.json, EnumVariantData has an optional `type` field
        // If present, it's a tuple variant with one element
        // If absent, it's a unit variant
        // Note: The current api.json structure doesn't support multi-element tuples
        // or struct variants - those would need schema changes

        if let Some(ref type_name) = variant_data.r#type {
            // Single-element tuple variant, with optional ref_kind
            let ref_kind = match &variant_data.ref_kind {
                crate::api::RefKind::Ref => FieldRefKind::Ref,
                crate::api::RefKind::RefMut => FieldRefKind::RefMut,
                crate::api::RefKind::ConstPtr => FieldRefKind::Ptr,
                crate::api::RefKind::MutPtr => FieldRefKind::PtrMut,
                crate::api::RefKind::Value => FieldRefKind::Owned,
                crate::api::RefKind::Boxed => FieldRefKind::Boxed,
                crate::api::RefKind::OptionBoxed => FieldRefKind::OptionBoxed,
            };
            return Ok(EnumVariantKind::Tuple(vec![(type_name.clone(), ref_kind)]));
        }

        // Unit variant
        Ok(EnumVariantKind::Unit)
    }

    // ========================================================================
    // Phase 3: Callback Typedefs
    // ========================================================================

    fn build_callback_typedefs(&mut self) -> Result<()> {
        // Extract callback_typedef definitions from api.json
        // CallbackDefinition has fn_args: Vec<CallbackArgData> and returns: Option<ReturnTypeData>

        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(ref callback) = class_data.callback_typedef {
                    let args = callback
                        .fn_args
                        .iter()
                        .map(|arg_data| {
                            FunctionArg {
                                name: String::new(), // CallbackArgData doesn't have a name field
                                type_name: arg_data.r#type.clone(),
                                ref_kind: match arg_data.ref_kind {
                                    crate::api::RefKind::Ref => ArgRefKind::Ref,
                                    crate::api::RefKind::RefMut => ArgRefKind::RefMut,
                                    crate::api::RefKind::ConstPtr => ArgRefKind::Ptr,
                                    crate::api::RefKind::MutPtr => ArgRefKind::PtrMut,
                                    _ => ArgRefKind::Owned,
                                },
                                doc: arg_data.doc.as_ref().and_then(|d| d.first().cloned()),
                                callback_info: None, // Callback typedef args don't have nested callbacks
                            }
                        })
                        .collect();

                    let return_type = callback.returns.as_ref().map(|r| r.r#type.clone());

                    self.ir.callback_typedefs.push(CallbackTypedefDef {
                        name: class_name.clone(),
                        args,
                        return_type,
                        doc: class_data.doc.clone().unwrap_or_default(),
                        module: module_name.clone(),
                        external_path: class_data.external.clone(),
                        // Will be populated in analyze_dependencies phase
                        dependencies: Vec::new(),
                        sort_order: 0,
                    });
                }
            }
        }

        Ok(())
    }

    // ========================================================================
    // Phase 4: Type Aliases
    // ========================================================================

    fn build_type_aliases(&mut self) -> Result<()> {
        // Extract type_alias definitions from api.json
        // Only handle non-generic type aliases here.
        // Generic type aliases (like StyleCursorValue = CssPropertyValue<StyleCursor>)
        // are monomorphized and stored in the monomorphized_def field.

        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(ref type_alias) = class_data.type_alias {
                    // Apply ref_kind for pointer types
                    // e.g., c_void with ref_kind=MutPtr -> *mut c_void
                    let target = match &type_alias.ref_kind {
                        crate::api::RefKind::ConstPtr => format!("*const {}", type_alias.target),
                        crate::api::RefKind::MutPtr => format!("*mut {}", type_alias.target),
                        _ => type_alias.target.clone(),
                    };

                    // Build monomorphized definition for generic type aliases
                    let monomorphized_def = if !type_alias.generic_args.is_empty() {
                        self.build_monomorphized_def(&type_alias.target, &type_alias.generic_args)
                    } else {
                        None
                    };

                    // Build traits from derive/custom_impls (same logic as structs/enums)
                    // For type aliases, inherit traits from the target type if not explicitly set
                    let derives = class_data.derive.clone().unwrap_or_default();
                    let custom_impls = class_data.custom_impls.clone().unwrap_or_default();
                    let has_custom_drop = class_data.has_custom_drop();
                    let traits = TypeTraits::from_derives_and_custom_impls(
                        &derives,
                        &custom_impls,
                        has_custom_drop,
                    );

                    self.ir.type_aliases.push(TypeAliasDef {
                        name: class_name.clone(),
                        target,
                        generic_args: type_alias.generic_args.clone(),
                        doc: class_data.doc.clone().unwrap_or_default(),
                        module: module_name.clone(),
                        external_path: class_data.external.clone(),
                        traits,
                        monomorphized_def,
                        dependencies: Vec::new(),
                        sort_order: 0,
                    });
                }
            }
        }

        Ok(())
    }

    /// Build a monomorphized type definition from a generic type alias
    ///
    /// For example, `CaretColorValue = CssPropertyValue<CaretColor>` becomes
    /// a concrete enum with variants Auto, None, Inherit, Initial, Exact(CaretColor)
    fn build_monomorphized_def(
        &self,
        target_type: &str,
        generic_args: &[String],
    ) -> Option<MonomorphizedTypeDef> {
        // Find the target class (e.g., CssPropertyValue)
        let target_class = self
            .version_data
            .api
            .values()
            .find_map(|module| module.classes.get(target_type))?;

        // Check if it's an enum
        if let Some(enum_fields) = &target_class.enum_fields {
            let is_union = self.enum_is_union(enum_fields);

            if is_union {
                // Build tagged union variants
                let variants: Vec<MonomorphizedVariant> = enum_fields
                    .iter()
                    .flat_map(|variant_map| variant_map.iter())
                    .map(|(variant_name, variant_data)| {
                        let payload_type = variant_data.r#type.as_ref().map(|t| {
                            // Substitute generic type parameter with concrete type
                            self.substitute_generic_param(t, generic_args)
                        });
                        let payload_ref_kind = match &variant_data.ref_kind {
                            crate::api::RefKind::Ref => FieldRefKind::Ref,
                            crate::api::RefKind::RefMut => FieldRefKind::RefMut,
                            crate::api::RefKind::ConstPtr => FieldRefKind::Ptr,
                            crate::api::RefKind::MutPtr => FieldRefKind::PtrMut,
                            crate::api::RefKind::Value => FieldRefKind::Owned,
                            crate::api::RefKind::Boxed => FieldRefKind::Boxed,
                            crate::api::RefKind::OptionBoxed => FieldRefKind::OptionBoxed,
                        };
                        MonomorphizedVariant {
                            name: variant_name.clone(),
                            payload_type,
                            payload_ref_kind,
                        }
                    })
                    .collect();

                Some(MonomorphizedTypeDef {
                    kind: MonomorphizedKind::TaggedUnion {
                        repr: target_class.repr.clone(),
                        variants,
                    },
                })
            } else {
                // Simple enum (no data)
                let variants: Vec<String> = enum_fields
                    .iter()
                    .flat_map(|variant_map| variant_map.keys())
                    .cloned()
                    .collect();

                Some(MonomorphizedTypeDef {
                    kind: MonomorphizedKind::SimpleEnum {
                        repr: target_class.repr.clone(),
                        variants,
                    },
                })
            }
        } else if let Some(struct_fields) = &target_class.struct_fields {
            // Build struct fields with substituted types
            let fields: Vec<FieldDef> = struct_fields
                .iter()
                .flat_map(|field_map| field_map.iter())
                .map(|(field_name, field_data)| {
                    let type_name = self.substitute_generic_param(&field_data.r#type, generic_args);
                    FieldDef {
                        name: field_name.clone(),
                        type_name,
                        ref_kind: match &field_data.ref_kind {
                            crate::api::RefKind::Ref => FieldRefKind::Ref,
                            crate::api::RefKind::RefMut => FieldRefKind::RefMut,
                            crate::api::RefKind::ConstPtr => FieldRefKind::Ptr,
                            crate::api::RefKind::MutPtr => FieldRefKind::PtrMut,
                            crate::api::RefKind::Value => FieldRefKind::Owned,
                            crate::api::RefKind::Boxed => FieldRefKind::Boxed,
                            crate::api::RefKind::OptionBoxed => FieldRefKind::OptionBoxed,
                        },
                        doc: field_data.doc.as_ref().and_then(|d| d.first().cloned()),
                        is_public: true,
                    }
                })
                .collect();

            Some(MonomorphizedTypeDef {
                kind: MonomorphizedKind::Struct { fields },
            })
        } else {
            None
        }
    }

    /// Substitute generic type parameters (like "T") with concrete types
    fn substitute_generic_param(&self, type_str: &str, generic_args: &[String]) -> String {
        // Map generic parameter names to their index
        let generic_names = ["T", "U", "V"];

        // Check if the entire type_str is a bare generic parameter
        if let Some(idx) = generic_names.iter().position(|&g| type_str == g) {
            return generic_args
                .get(idx)
                .cloned()
                .unwrap_or_else(|| type_str.to_string());
        }

        // Otherwise, substitute generic params inside compound types like "*const T", "*mut T"
        let mut result = type_str.to_string();
        for (idx, &name) in generic_names.iter().enumerate() {
            if let Some(concrete) = generic_args.get(idx) {
                // Replace the generic name as a whole word (not inside other identifiers)
                // Simple approach: replace " T" suffix or standalone "T" in pointer types
                result = result.replace(&format!(" {}", name), &format!(" {}", concrete));
                // Also handle if T appears at start (unlikely in practice)
                if result == name {
                    result = concrete.clone();
                }
            }
        }
        result
    }

    /// Check if an enum has data in any variant (making it a tagged union)
    fn enum_is_union(&self, enum_fields: &[IndexMap<String, crate::api::EnumVariantData>]) -> bool {
        enum_fields
            .iter()
            .flat_map(|m| m.values())
            .any(|v| v.r#type.is_some())
    }

    // ========================================================================
    // Phase 5: Link Callback Wrappers
    // ========================================================================

    /// Links callback wrapper structs to their callback_typedefs.
    ///
    /// A callback wrapper struct is identified by:
    /// 1. Name ends with "Callback" (but not "CallbackType" or "CallbackInfo")
    /// 2. Has a field with a callback_typedef type (usually named "cb")
    /// 3. Has a "callable" field with type "OptionRefAny"
    ///
    /// For "Core" callbacks like "Callback", the associated typedef is "CallbackType".
    /// For widget callbacks like "ButtonOnClickCallback", it's "ButtonOnClickCallbackType".
    fn link_callback_wrappers(&mut self) {
        // Build a set of callback typedef names for fast lookup
        let callback_typedef_names: std::collections::BTreeSet<String> = self
            .ir
            .callback_typedefs
            .iter()
            .map(|cb| cb.name.clone())
            .collect();

        // Iterate through all structs and find callback wrappers
        for struct_def in &mut self.ir.structs {
            // Quick reject: must end with "Callback" but not "CallbackType" or "CallbackInfo"
            if !struct_def.name.ends_with("Callback") {
                continue;
            }
            if struct_def.name.ends_with("CallbackType")
                || struct_def.name.ends_with("CallbackInfo")
            {
                continue;
            }

            // Find the callback typedef field and the context field
            let mut callback_field: Option<(String, String)> = None; // (field_name, typedef_name)
            let mut context_field_name: Option<String> = None;

            for field in &struct_def.fields {
                // Check if field type is a callback_typedef
                if callback_typedef_names.contains(&field.type_name) {
                    callback_field = Some((field.name.clone(), field.type_name.clone()));
                }

                // Check for "ctx" or "callable" field with type "OptionRefAny"
                // "ctx" is used in api.json for some callbacks, "callable" for others
                if (field.name == "ctx" || field.name == "callable")
                    && field.type_name == "OptionRefAny"
                {
                    context_field_name = Some(field.name.clone());
                }
            }

            // If we found both a callback typedef field and a ctx/callable field, this is a callback wrapper
            if let Some((field_name, typedef_name)) = callback_field {
                if let Some(ctx_name) = context_field_name {
                    struct_def.callback_wrapper_info = Some(CallbackWrapperInfo {
                        callback_typedef_name: typedef_name,
                        callback_field_name: field_name,
                        context_field_name: ctx_name,
                    });
                }
            }
        }
    }

    // ========================================================================
    // Phase 6: API Functions
    // ========================================================================

    fn build_api_functions(&mut self) -> Result<()> {
        // TODO: Extract constructors and functions from api.json
        // TODO: Build FunctionDef for each
        // TODO: Handle fn_body from api.json

        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                // Skip callback typedefs, type aliases
                if class_data.callback_typedef.is_some() || class_data.type_alias.is_some() {
                    continue;
                }

                // Build constructors
                if let Some(ref constructors) = class_data.constructors {
                    for (ctor_name, ctor_data) in constructors {
                        let func = self.build_function_def(
                            class_name,
                            ctor_name,
                            ctor_data,
                            FunctionKind::Constructor,
                        )?;
                        self.ir.functions.push(func);
                    }
                }

                // Build methods
                if let Some(ref functions) = class_data.functions {
                    for (fn_name, fn_data) in functions {
                        let kind = self
                            .determine_method_kind(fn_data)
                            .map_err(|e| anyhow::anyhow!("{}", e))?;
                        let func = self.build_function_def(class_name, fn_name, fn_data, kind)?;
                        self.ir.functions.push(func);
                    }
                }
            }
        }

        Ok(())
    }

    fn build_function_def(
        &self,
        class_name: &str,
        method_name: &str,
        fn_data: &crate::api::FunctionData,
        kind: FunctionKind,
    ) -> Result<FunctionDef> {
        // Parse function arguments from Vec<IndexMap<String, String>>
        // Each IndexMap has one entry: arg_name -> type_name

        // C-ABI function name uses camelCase for method name (e.g., Az{ClassName}_{methodName})
        let c_name = format!(
            "Az{}_{}",
            class_name,
            snake_case_to_lower_camel(method_name)
        );

        let args: Vec<FunctionArg> = fn_data
            .fn_args
            .iter()
            .flat_map(|arg_map| {
                arg_map.iter().map(|(name, type_name)| {
                    // Handle self parameter specially:
                    // In api.json: "self": "ref" | "refmut" | "value"
                    // These become reference types (not pointers!) for C-ABI
                    // The parameter name becomes the lowercase class name (e.g., Dom -> dom)
                    // because fn_body uses that name (e.g., "dom.root.set_node_type(...)")
                    if name == "self" {
                        let (ref_kind, actual_type) = match type_name.as_str() {
                            "ref" => (ArgRefKind::Ref, class_name.to_string()), // &self -> &ClassName
                            "refmut" => (ArgRefKind::RefMut, class_name.to_string()), // &mut self -> &mut ClassName
                            "value" => (ArgRefKind::Owned, class_name.to_string()), // self -> ClassName
                            "mut value" => (ArgRefKind::Owned, class_name.to_string()), // mut self -> ClassName
                            other => (ArgRefKind::Owned, other.to_string()),            // fallback
                        };
                        return FunctionArg {
                            // Use snake_case class name as parameter name
                            // This matches what fn_body expects (e.g., "dom_vec.len()")
                            name: to_snake_case(class_name),
                            type_name: actual_type,
                            ref_kind,
                            doc: None,
                            callback_info: None,
                        };
                    }

                    // For regular arguments, parse the ref_kind from type string
                    let (ref_kind, actual_type) = parse_type_ref_kind(type_name);

                    // Check if this is a callback typedef type
                    let callback_info = self.detect_callback_arg_info(&actual_type);

                    FunctionArg {
                        name: name.clone(),
                        type_name: actual_type,
                        ref_kind,
                        doc: None,
                        callback_info,
                    }
                })
            })
            .collect();

        // Determine return type:
        // - For explicit returns: use the specified type
        // - For constructors: ALWAYS return class type (same as old codegen)
        // - For methods: may return void or a type
        let return_type = if let Some(returns) = fn_data.returns.as_ref() {
            Some(returns.r#type.clone())
        } else if kind == FunctionKind::Constructor {
            // Constructors without explicit return type ALWAYS return the class type
            // This matches the old codegen behavior (rust_dll.rs line 335)
            Some(class_name.to_string())
        } else {
            None
        };

        Ok(FunctionDef {
            c_name,
            class_name: class_name.to_string(),
            method_name: method_name.to_string(),
            kind,
            args,
            return_type,
            fn_body: fn_data.fn_body.clone(),
            doc: fn_data.doc.clone().unwrap_or_default(),
            is_const: fn_data.const_fn,
            is_unsafe: false,
        })
    }

    fn determine_method_kind(
        &self,
        fn_data: &crate::api::FunctionData,
    ) -> Result<FunctionKind, String> {
        // Analyze fn_data to determine if method, method_mut, or static
        // Look at first argument to see if it's self/&self/&mut self
        // Note: fn_args is Vec<IndexMap<String, String>>, not Option
        // In api.json: "self": "ref" | "refmut" | "value"

        if let Some(first_arg) = fn_data.fn_args.first() {
            if let Some((name, value)) = first_arg.iter().next() {
                if name == "self" {
                    // Check the value to determine ref kind
                    if value == "refmut" {
                        return Ok(FunctionKind::MethodMut);
                    } else {
                        // "ref" or "value" are both treated as Method
                        return Ok(FunctionKind::Method);
                    }
                }
                if name == "&self" {
                    return Ok(FunctionKind::Method);
                }
                if name == "&mut self" {
                    return Ok(FunctionKind::MethodMut);
                }
            }
        }

        // Error: functions without self should be constructors
        // This is an error in api.json - functions in "functions" section
        // should have self as the first argument. Use "constructors" section for
        // static factory methods.
        if fn_data
            .fn_body
            .as_ref()
            .map(|b| b.contains("self."))
            .unwrap_or(false)
        {
            return Err(format!(
                "[ERROR] Function uses 'self.' in fn_body but has no 'self' parameter. \
                 This is an error in api.json. The function should have \
                 {{\"self\": \"ref\"}} or {{\"self\": \"value\"}} as the first fn_arg. \
                 fn_body: {:?}",
                fn_data.fn_body
            ));
        }

        Ok(FunctionKind::StaticMethod)
    }

    // ========================================================================
    // Phase 6b: Enum Variant Constructors
    // ========================================================================

    /// Generate constructor functions for each enum variant automatically.
    ///
    /// For simple unit variants like `HoverEventFilter::MouseUp`:
    ///   - C name: `AzHoverEventFilter_mouseUp`
    ///   - Returns: `HoverEventFilter`
    ///   - Body: `HoverEventFilter::MouseUp`
    ///
    /// For tuple variants like `EventFilter::Hover(HoverEventFilter)`:
    ///   - C name: `AzEventFilter_hover`
    ///   - Args: `payload: HoverEventFilter`
    ///   - Returns: `EventFilter`
    ///   - Body: `EventFilter::Hover(payload)`
    fn build_enum_variant_constructors(&mut self) {
        // Collect all enum info first to avoid borrow issues
        let enum_infos: Vec<_> = self.ir.enums.iter()
            .filter(|e| e.generic_params.is_empty()) // Skip generic enums
            .map(|e| (e.name.clone(), e.variants.clone()))
            .collect();

        // Build a set of existing function c_names to avoid duplicates
        // (if api.json already defines a constructor for a variant, skip auto-generation)
        let existing_c_names: std::collections::HashSet<_> =
            self.ir.functions.iter().map(|f| f.c_name.clone()).collect();

        for (enum_name, variants) in enum_infos {
            for variant in variants {
                // Skip "Default" variant to avoid conflict with Default trait's default() function
                if variant.name == "Default" {
                    continue;
                }
                let func = self.build_variant_constructor(&enum_name, &variant);
                // Only add if not already defined manually in api.json
                if !existing_c_names.contains(&func.c_name) {
                    self.ir.functions.push(func);
                }
            }
        }
    }

    fn build_variant_constructor(&self, enum_name: &str, variant: &EnumVariantDef) -> FunctionDef {
        use crate::codegen::v2::ir::FunctionKind;

        // Convert variant name to lowerCamelCase for method name
        // e.g., "MouseUp" -> "mouseUp", "LeftMouseDown" -> "leftMouseDown"
        let method_name = variant
            .name
            .chars()
            .next()
            .map(|c| c.to_lowercase().to_string())
            .unwrap_or_default()
            + &variant.name[1..];

        // C-ABI name: Az{EnumName}_{methodName}
        let c_name = format!("Az{}_{}", enum_name, method_name);

        // Build args and fn_body based on variant kind
        let (args, fn_body) = match &variant.kind {
            EnumVariantKind::Unit => {
                // No args, returns the unit variant directly
                // e.g., `HoverEventFilter::MouseUp`
                (Vec::new(), Some(format!("{}::{}", enum_name, variant.name)))
            }
            EnumVariantKind::Tuple(types) => {
                // For single-element tuples, use "payload" as arg name
                // For multi-element tuples, use payload0, payload1, etc.
                let args: Vec<FunctionArg> = if types.len() == 1 {
                    vec![FunctionArg {
                        name: "payload".to_string(),
                        type_name: types[0].0.clone(),
                        ref_kind: ArgRefKind::Owned,
                        doc: None,
                        callback_info: None,
                    }]
                } else {
                    types
                        .iter()
                        .enumerate()
                        .map(|(i, (t, _))| FunctionArg {
                            name: format!("payload{}", i),
                            type_name: t.clone(),
                            ref_kind: ArgRefKind::Owned,
                            doc: None,
                            callback_info: None,
                        })
                        .collect()
                };

                // Build fn_body like `EventFilter::Hover(payload)` or `Variant(payload0, payload1)`
                let arg_names = if types.len() == 1 {
                    "payload".to_string()
                } else {
                    (0..types.len())
                        .map(|i| format!("payload{}", i))
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let fn_body = format!("{}::{}({})", enum_name, variant.name, arg_names);

                (args, Some(fn_body))
            }
            EnumVariantKind::Struct(fields) => {
                // Use field names as arg names
                let args: Vec<FunctionArg> = fields
                    .iter()
                    .map(|f| FunctionArg {
                        name: f.name.clone(),
                        type_name: f.type_name.clone(),
                        ref_kind: ArgRefKind::Owned,
                        doc: None,
                        callback_info: None,
                    })
                    .collect();

                // Build fn_body like `Variant { field1, field2 }`
                let field_names: Vec<_> = fields.iter().map(|f| f.name.clone()).collect();
                let fn_body = format!(
                    "{}::{} {{ {} }}",
                    enum_name,
                    variant.name,
                    field_names.join(", ")
                );

                (args, Some(fn_body))
            }
        };

        FunctionDef {
            c_name,
            class_name: enum_name.to_string(),
            method_name,
            kind: FunctionKind::EnumVariantConstructor,
            args,
            return_type: Some(enum_name.to_string()),
            fn_body,
            doc: variant.doc.iter().cloned().collect(),
            is_const: true, // These are const constructors
            is_unsafe: false,
        }
    }

    // ========================================================================
    // Phase 7: Trait Functions
    // ========================================================================

    fn build_trait_functions(&mut self) -> Result<()> {
        // TODO: For each struct/enum with relevant traits, generate:
        //   - _delete if has custom drop or !Copy
        //   - _deepCopy if Clone && !Copy
        //   - _partialEq if PartialEq
        //   - _partialCmp if PartialOrd
        //   - _cmp if Ord
        //   - _hash if Hash

        // Collect type info first to avoid borrow issues
        // IMPORTANT: Skip generic types - they need to be monomorphized first
        // Only their monomorphized versions (type aliases) should get trait functions
        let struct_infos: Vec<_> = self
            .ir
            .structs
            .iter()
            .filter(|s| s.generic_params.is_empty())
            .map(|s| (s.name.clone(), s.traits.clone()))
            .collect();

        let enum_infos: Vec<_> = self
            .ir
            .enums
            .iter()
            .filter(|e| e.generic_params.is_empty())
            .map(|e| (e.name.clone(), e.traits.clone()))
            .collect();

        // Generate trait functions for structs
        for (name, traits) in struct_infos {
            self.generate_trait_functions_for_type(&name, &traits);
        }

        // Generate trait functions for enums
        for (name, traits) in enum_infos {
            self.generate_trait_functions_for_type(&name, &traits);
        }

        // Generate trait functions for monomorphized type aliases
        // These are the concrete instantiations of generic types
        let type_alias_infos: Vec<_> = self
            .ir
            .type_aliases
            .iter()
            .filter(|t| t.monomorphized_def.is_some())
            .map(|t| (t.name.clone(), t.traits.clone()))
            .collect();

        for (name, traits) in type_alias_infos {
            self.generate_trait_functions_for_type(&name, &traits);
        }

        Ok(())
    }

    fn generate_trait_functions_for_type(&mut self, type_name: &str, traits: &TypeTraits) {
        // _delete (Drop)
        if traits.needs_delete() {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_delete", type_name),
                class_name: type_name.to_string(),
                method_name: "delete".to_string(),
                kind: FunctionKind::Delete,
                args: vec![FunctionArg {
                    name: "instance".to_string(),
                    type_name: type_name.to_string(),
                    ref_kind: ArgRefKind::PtrMut,
                    doc: None,
                    callback_info: None,
                }],
                return_type: None,
                fn_body: None, // Generated based on config
                doc: vec![],
                is_const: false,
                is_unsafe: false,
            });
        }

        // _clone (Clone)
        if traits.needs_deep_copy() {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_clone", type_name),
                class_name: type_name.to_string(),
                method_name: "clone".to_string(),
                kind: FunctionKind::DeepCopy,
                args: vec![FunctionArg {
                    name: "instance".to_string(),
                    type_name: type_name.to_string(),
                    ref_kind: ArgRefKind::Ptr,
                    doc: None,
                    callback_info: None,
                }],
                return_type: Some(type_name.to_string()),
                fn_body: None,
                doc: vec![],
                is_const: true,
                is_unsafe: false,
            });
        }

        // _partialEq (PartialEq)
        if traits.is_partial_eq {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_partialEq", type_name),
                class_name: type_name.to_string(),
                method_name: "partialEq".to_string(),
                kind: FunctionKind::PartialEq,
                args: vec![
                    FunctionArg {
                        name: "a".to_string(),
                        type_name: type_name.to_string(),
                        ref_kind: ArgRefKind::Ptr,
                        doc: None,
                        callback_info: None,
                    },
                    FunctionArg {
                        name: "b".to_string(),
                        type_name: type_name.to_string(),
                        ref_kind: ArgRefKind::Ptr,
                        doc: None,
                        callback_info: None,
                    },
                ],
                return_type: Some("bool".to_string()),
                fn_body: None,
                doc: vec![],
                is_const: true,
                is_unsafe: false,
            });
        }

        // _hash (Hash)
        if traits.is_hash {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_hash", type_name),
                class_name: type_name.to_string(),
                method_name: "hash".to_string(),
                kind: FunctionKind::Hash,
                args: vec![FunctionArg {
                    name: "instance".to_string(),
                    type_name: type_name.to_string(),
                    ref_kind: ArgRefKind::Ptr,
                    doc: None,
                    callback_info: None,
                }],
                return_type: Some("u64".to_string()),
                fn_body: None,
                doc: vec![],
                is_const: true,
                is_unsafe: false,
            });
        }

        // _partialCmp (PartialOrd)
        if traits.is_partial_ord {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_partialCmp", type_name),
                class_name: type_name.to_string(),
                method_name: "partialCmp".to_string(),
                kind: FunctionKind::PartialCmp,
                args: vec![
                    FunctionArg {
                        name: "a".to_string(),
                        type_name: type_name.to_string(),
                        ref_kind: ArgRefKind::Ptr,
                        doc: None,
                        callback_info: None,
                    },
                    FunctionArg {
                        name: "b".to_string(),
                        type_name: type_name.to_string(),
                        ref_kind: ArgRefKind::Ptr,
                        doc: None,
                        callback_info: None,
                    },
                ],
                return_type: Some("u8".to_string()), // Ordering as u8
                fn_body: None,
                doc: vec![],
                is_const: true,
                is_unsafe: false,
            });
        }

        // _cmp (Ord)
        if traits.is_ord {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_cmp", type_name),
                class_name: type_name.to_string(),
                method_name: "cmp".to_string(),
                kind: FunctionKind::Cmp,
                args: vec![
                    FunctionArg {
                        name: "a".to_string(),
                        type_name: type_name.to_string(),
                        ref_kind: ArgRefKind::Ptr,
                        doc: None,
                        callback_info: None,
                    },
                    FunctionArg {
                        name: "b".to_string(),
                        type_name: type_name.to_string(),
                        ref_kind: ArgRefKind::Ptr,
                        doc: None,
                        callback_info: None,
                    },
                ],
                return_type: Some("u8".to_string()), // Ordering as u8
                fn_body: None,
                doc: vec![],
                is_const: true,
                is_unsafe: false,
            });
        }

        // _default (Default)
        if traits.is_default {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_default", type_name),
                class_name: type_name.to_string(),
                method_name: "default".to_string(),
                kind: FunctionKind::Default,
                args: vec![], // No arguments - static function
                return_type: Some(type_name.to_string()),
                fn_body: None,
                doc: vec![format!("Returns the default value for `{}`.", type_name)],
                is_const: true,
                is_unsafe: false,
            });
        }

        // _toDbgString (Debug)
        if traits.is_debug {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_toDbgString", type_name),
                class_name: type_name.to_string(),
                method_name: "toDbgString".to_string(),
                kind: FunctionKind::DebugToString,
                args: vec![FunctionArg {
                    name: "instance".to_string(),
                    type_name: type_name.to_string(),
                    ref_kind: ArgRefKind::Ptr,
                    doc: None,
                    callback_info: None,
                }],
                return_type: Some("String".to_string()),
                fn_body: None,
                doc: vec![format!("Returns the debug string representation of `{}`.", type_name)],
                is_const: true,
                is_unsafe: false,
            });
        }
    }

    /// Detect if an argument type is a callback typedef and return info for code generation
    ///
    /// A callback typedef type:
    /// - Ends with "CallbackType" (e.g., "CallbackType", "ButtonOnClickCallbackType")
    /// - Is registered in api.json with callback_typedef
    ///
    /// Returns CallbackArgInfo with:
    /// - callback_typedef_name: The typedef name (e.g., "CallbackType")
    /// - callback_wrapper_name: The wrapper struct name (e.g., "Callback", "ButtonOnClickCallback")
    /// - trampoline_name: The name of the Python trampoline function
    fn detect_callback_arg_info(&self, type_name: &str) -> Option<CallbackArgInfo> {
        // Check if type ends with "CallbackType"
        if !type_name.ends_with("CallbackType") {
            return None;
        }

        // Skip destructor and clone callback types - these are internal
        // FontRefDestructorCallbackType ends with "CallbackType" but contains "Destructor"
        if type_name.contains("Destructor") || type_name.ends_with("CloneCallbackType") {
            return None;
        }

        // The wrapper name is the typedef name with "Type" stripped
        // e.g., "ButtonOnClickCallbackType" -> "ButtonOnClickCallback"
        // e.g., "CallbackType" -> "Callback"
        let wrapper_name = type_name.strip_suffix("Type").unwrap_or(type_name);

        // Build trampoline name: invoke_py_{snake_case_of_wrapper}
        let trampoline_name = format!("invoke_py_{}", to_snake_case(wrapper_name));

        Some(CallbackArgInfo {
            callback_typedef_name: type_name.to_string(),
            callback_wrapper_name: wrapper_name.to_string(),
            trampoline_name,
        })
    }

    // ========================================================================
    // Phase 8: Constants
    // ========================================================================

    fn build_constants(&mut self) -> Result<()> {
        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(constants) = &class_data.constants {
                    for constant_map in constants {
                        for (constant_name, constant_data) in constant_map {
                            self.ir.constants.push(ConstantDef {
                                name: format!("{}_{}", class_name, constant_name),
                                type_name: constant_data.r#type.clone(),
                                value: constant_data.value.clone(),
                                doc: vec![],
                                module: module_name.clone(),
                            });
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

// ============================================================================
// Convenience Functions
// ============================================================================

/// Build IR from ApiData (convenience wrapper)
pub fn build_ir(api_data: &ApiData) -> Result<CodegenIR> {
    let version_name = api_data
        .0
        .keys()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No version found in api.json"))?;

    let version_data = api_data
        .get_version(version_name)
        .ok_or_else(|| anyhow::anyhow!("Could not get version data"))?;

    IRBuilder::new(version_data).build()
}

/// Parse a type string to extract ref_kind and the actual type name
///
/// Examples:
/// - "NodeType" → (Owned, "NodeType")
/// - "*const SvgNode" → (Ptr, "SvgNode")
/// - "*mut TextBuffer" → (PtrMut, "TextBuffer")
fn parse_type_ref_kind(type_str: &str) -> (ArgRefKind, String) {
    let trimmed = type_str.trim();

    if let Some(rest) = trimmed.strip_prefix("*const ") {
        return (ArgRefKind::Ptr, rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("*mut ") {
        return (ArgRefKind::PtrMut, rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("&mut ") {
        return (ArgRefKind::RefMut, rest.trim().to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("&") {
        return (ArgRefKind::Ref, rest.trim().to_string());
    }

    (ArgRefKind::Owned, trimmed.to_string())
}

// ============================================================================
// Type Category Classification
// ============================================================================

/// Recursive types that cause "infinite size" errors in PyO3
/// These would need Box<> indirection which the C-API doesn't have
const RECURSIVE_TYPE_NAMES: &[&str] = &[
    "XmlNode",
    "XmlNodeChild",
    "XmlNodeChildVec",
    "Xml",
    "ResultXmlXmlError",
];

/// VecRef types - raw pointer slice wrappers
/// These need special trampolines and are skipped in Python for now
const VECREF_TYPE_NAMES: &[&str] = &[
    // Immutable VecRef types
    "GLuintVecRef",
    "GLintVecRef",
    "GLenumVecRef",
    "U8VecRef",
    "U16VecRef",
    "U32VecRef",
    "I32VecRef",
    "F32VecRef",
    "Refstr",
    "RefstrVecRef",
    "TessellatedSvgNodeVecRef",
    "TessellatedColoredSvgNodeVecRef",
    "OptionU8VecRef",
    "OptionI16VecRef",
    "OptionI32VecRef",
    "OptionF32VecRef",
    "OptionFloatVecRef",
    // Mutable VecRefMut types
    "GLintVecRefMut",
    "GLint64VecRefMut",
    "GLbooleanVecRefMut",
    "GLfloatVecRefMut",
    "U8VecRefMut",
    "F32VecRefMut",
];

/// String type name
const STRING_TYPE_NAME: &str = "String";

/// Vec types that use C-API directly with special conversion
/// These types should NOT get a Python wrapper struct because they
/// have type aliases defined and PyO3 traits implemented on the C-API types
const VEC_TYPE_NAMES: &[&str] = &["U8Vec", "StringVec", "GLuintVec", "GLintVec"];

/// RefAny type name
const REFANY_TYPE_NAME: &str = "RefAny";

/// Types that use C-API types directly without Python wrapper structs
/// These have type aliases + PyO3 trait impls on C-API types in the patches section
/// They must NOT get wrapper structs generated
const CAPI_DIRECT_TYPES: &[&str] = &[
    // String types
    "String",
    // Vec types
    "U8Vec",
    "StringVec",
    "GLuintVec",
    "GLintVec",
    // RefAny
    "RefAny",
    // Destructor types
    "U8VecDestructor",
    "StringVecDestructor",
    // Opaque pointer types
    "InstantPtr",
    // Menu types with special handling
    "StringMenuItem",
];

/// Classify a struct type based on its properties
/// This is the central classification function that replaces all ad-hoc checks
pub fn classify_struct_type(
    name: &str,
    class_data: &ClassData,
    version_data: &VersionData,
) -> TypeCategory {
    // 1. Check for recursive types (by name)
    if RECURSIVE_TYPE_NAMES.contains(&name) {
        return TypeCategory::Recursive;
    }

    // 2. Check for VecRef types (by name or vec_ref_element_type)
    if VECREF_TYPE_NAMES.contains(&name) || class_data.vec_ref_element_type.is_some() {
        return TypeCategory::VecRef;
    }

    // 3. Check for Vec types (by vec_element_type field or hardcoded names)
    // This is the primary detection method - if vec_element_type is set, it's a Vec
    if class_data.vec_element_type.is_some() || VEC_TYPE_NAMES.contains(&name) {
        return TypeCategory::Vec;
    }

    // 4. Check for types that use C-API directly (no Python wrapper)
    // This includes String, RefAny, destructors, InstantPtr, etc.
    if CAPI_DIRECT_TYPES.contains(&name) {
        // Determine the specific sub-category
        if name == STRING_TYPE_NAME {
            return TypeCategory::String;
        }
        if name == REFANY_TYPE_NAME {
            return TypeCategory::RefAny;
        }
        if name.ends_with("Destructor") {
            return TypeCategory::DestructorOrClone;
        }
        // For other C-API direct types (InstantPtr, StringMenuItem, etc.)
        // treat them as types that use C-API directly
        return TypeCategory::Vec; // Vec is used as "uses C-API directly" marker
    }

    // 5. Check for boxed objects
    if class_data.is_boxed_object {
        return TypeCategory::Boxed;
    }

    // 6. Check for generic templates
    if class_data.generic_params.is_some()
        && !class_data
            .generic_params
            .as_ref()
            .map(|v| v.is_empty())
            .unwrap_or(true)
    {
        return TypeCategory::GenericTemplate;
    }

    // 7. Check for destructor/clone callback types (wrapper structs containing function pointers)
    if name.ends_with("Destructor")
        || name.ends_with("DestructorType")
        || name.ends_with("CloneCallbackType")
        || name.ends_with("CloneCallback")
        || name.ends_with("DestructorCallback")
    {
        return TypeCategory::DestructorOrClone;
    }

    // 8. Check for callback+data pair struct (has callback field + RefAny data field)
    if is_callback_data_pair(class_data, version_data) {
        return TypeCategory::CallbackDataPair;
    }

    // 9. Default to Regular
    TypeCategory::Regular
}

/// Classify an enum type based on its properties
pub fn classify_enum_type(
    name: &str,
    class_data: &ClassData,
    _version_data: &VersionData,
) -> TypeCategory {
    // 1. Check for recursive types
    if RECURSIVE_TYPE_NAMES.contains(&name) {
        return TypeCategory::Recursive;
    }

    // 2. Check for destructor/clone callback types (enums like U8VecDestructor)
    if name.ends_with("Destructor")
        || name.ends_with("DestructorType")
        || name.ends_with("CloneCallbackType")
    {
        return TypeCategory::DestructorOrClone;
    }

    // 3. Check for generic templates
    if class_data.generic_params.is_some()
        && !class_data
            .generic_params
            .as_ref()
            .map(|v| v.is_empty())
            .unwrap_or(true)
    {
        return TypeCategory::GenericTemplate;
    }

    // 4. Default to Regular
    TypeCategory::Regular
}

/// Check if a struct is a callback+data pair (has callback typedef field + RefAny data field)
/// These structs need special Python wrappers that accept PyObject for both fields
fn is_callback_data_pair(class_data: &ClassData, version_data: &VersionData) -> bool {
    let struct_fields = match &class_data.struct_fields {
        Some(f) => f,
        None => return false,
    };

    let mut has_callback_field = false;
    let mut has_refany_field = false;

    for field_map in struct_fields {
        for (_field_name, field_data) in field_map {
            let field_type = &field_data.r#type;

            // Check for RefAny field
            if field_type == "RefAny" {
                has_refany_field = true;
            }

            // Check if field type is a callback typedef
            // Look up the field's type in api.json
            if let Some((module, _)) = search_for_class_by_class_name(version_data, field_type) {
                if let Some(field_class) = get_class(version_data, module, field_type) {
                    if field_class.callback_typedef.is_some() {
                        has_callback_field = true;
                    }
                }
            }
        }
    }

    has_callback_field && has_refany_field
}

/// Helper to search for a class by name across all modules
fn search_for_class_by_class_name<'a>(
    version_data: &'a VersionData,
    class_name: &str,
) -> Option<(&'a str, &'a ClassData)> {
    for (module_name, module_data) in &version_data.api {
        if let Some(class_data) = module_data.classes.get(class_name) {
            return Some((module_name.as_str(), class_data));
        }
    }
    None
}

/// Helper to get a class from a module
fn get_class<'a>(
    version_data: &'a VersionData,
    module_name: &str,
    class_name: &str,
) -> Option<&'a ClassData> {
    version_data
        .api
        .get(module_name)
        .and_then(|m| m.classes.get(class_name))
}

/// Convert CamelCase to snake_case
///
/// Examples:
/// - "ButtonOnClickCallback" -> "button_on_click_callback"
/// - "CallbackType" -> "callback_type"
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_ascii_lowercase());
        } else {
            result.push(c);
        }
    }
    result
}

/// Check if a type string is an array type like [T; N]
///
/// Array types are not allowed in api.json because they require
/// special handling in each language binding. Use dedicated structs instead.
fn is_array_type(type_str: &str) -> bool {
    let trimmed = type_str.trim();
    trimmed.starts_with('[') && trimmed.contains(';') && trimmed.ends_with(']')
}
