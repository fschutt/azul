//! IR Builder - Constructs CodegenIR from api.json
//!
//! This module is responsible for parsing the api.json data structure
//! and building a complete Intermediate Representation (IR) that can
//! be consumed by language-specific generators.

use std::collections::HashMap;
use anyhow::Result;
use indexmap::IndexMap;

use crate::api::{ApiData, ClassData, VersionData, FieldData, EnumVariantData};
use crate::utils::string::snake_case_to_lower_camel;
use super::ir::*;

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

        // Phase 7: Generate trait functions (_deepCopy, _delete, _partialEq, etc.)
        self.build_trait_functions()?;

        Ok(self.ir)
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
                self.ir.type_to_module.insert(class_name.clone(), module_name.clone());
                
                if let Some(ref external) = class_data.external {
                    self.ir.type_to_external.insert(class_name.clone(), external.clone());
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
        let traits = TypeTraits::from_derives_and_custom_impls(&derives, &custom_impls, has_custom_drop);

        let fields = self.build_struct_fields(class_data)?;
        
        // Classify the type category
        let category = classify_struct_type(name, class_data, self.version_data);

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

    fn build_enum_def(
        &self,
        name: &str,
        class_data: &ClassData,
        module: &str,
    ) -> Result<EnumDef> {
        let derives = class_data.derive.clone().unwrap_or_default();
        let custom_impls = class_data.custom_impls.clone().unwrap_or_default();
        let has_explicit_derive = class_data.derive.is_some();
        let has_custom_drop = class_data.has_custom_drop();
        let traits = TypeTraits::from_derives_and_custom_impls(&derives, &custom_impls, has_custom_drop);

        let (variants, is_union) = self.build_enum_variants(class_data)?;
        
        // Classify the type category
        let category = classify_enum_type(name, class_data, self.version_data);

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
            // Single-element tuple variant
            return Ok(EnumVariantKind::Tuple(vec![type_name.clone()]));
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
                    let args = callback.fn_args.iter().map(|arg_data| {
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
                    }).collect();

                    let return_type = callback.returns.as_ref().map(|r| r.r#type.clone());

                    self.ir.callback_typedefs.push(CallbackTypedefDef {
                        name: class_name.clone(),
                        args,
                        return_type,
                        doc: class_data.doc.clone().unwrap_or_default(),
                        module: module_name.clone(),
                        external_path: class_data.external.clone(),
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
        
        for (module_name, module_data) in &self.version_data.api {
            for (class_name, class_data) in &module_data.classes {
                if let Some(ref type_alias) = class_data.type_alias {
                    // Build target string with generic arguments
                    // e.g., "CssPropertyValue" + ["StyleCursor"] -> "CssPropertyValue<StyleCursor>"
                    let base_target = if type_alias.generic_args.is_empty() {
                        type_alias.target.clone()
                    } else {
                        format!("{}<{}>", type_alias.target, type_alias.generic_args.join(", "))
                    };
                    
                    // Apply ref_kind for pointer types
                    // e.g., c_void with ref_kind=constptr -> *const c_void
                    let target = match &type_alias.ref_kind {
                        crate::api::RefKind::ConstPtr => format!("*const {}", base_target),
                        crate::api::RefKind::MutPtr => format!("*mut {}", base_target),
                        _ => base_target,
                    };
                    
                    self.ir.type_aliases.push(TypeAliasDef {
                        name: class_name.clone(),
                        target,
                        doc: class_data.doc.clone().unwrap_or_default(),
                        module: module_name.clone(),
                        external_path: class_data.external.clone(),
                    });
                }
            }
        }

        Ok(())
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
        let callback_typedef_names: std::collections::HashSet<String> = self.ir.callback_typedefs
            .iter()
            .map(|cb| cb.name.clone())
            .collect();
        
        // Iterate through all structs and find callback wrappers
        for struct_def in &mut self.ir.structs {
            // Quick reject: must end with "Callback" but not "CallbackType" or "CallbackInfo"
            if !struct_def.name.ends_with("Callback") {
                continue;
            }
            if struct_def.name.ends_with("CallbackType") || struct_def.name.ends_with("CallbackInfo") {
                continue;
            }
            
            // Find the callback typedef field and the callable field
            let mut callback_field: Option<(String, String)> = None; // (field_name, typedef_name)
            let mut has_callable_field = false;
            
            for field in &struct_def.fields {
                // Check if field type is a callback_typedef
                if callback_typedef_names.contains(&field.type_name) {
                    callback_field = Some((field.name.clone(), field.type_name.clone()));
                }
                
                // Check for "callable" field with type "OptionRefAny"
                if field.name == "callable" && field.type_name == "OptionRefAny" {
                    has_callable_field = true;
                }
            }
            
            // If we found both, this is a callback wrapper
            if let Some((field_name, typedef_name)) = callback_field {
                if has_callable_field {
                    struct_def.callback_wrapper_info = Some(CallbackWrapperInfo {
                        callback_typedef_name: typedef_name,
                        callback_field_name: field_name,
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
                        let kind = self.determine_method_kind(fn_data);
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
        let c_name = format!("Az{}_{}", class_name, snake_case_to_lower_camel(method_name));

        let args: Vec<FunctionArg> = fn_data.fn_args.iter().flat_map(|arg_map| {
            arg_map.iter().map(|(name, type_name)| {
                // Handle self parameter specially:
                // In api.json: "self": "ref" | "refmut" | "value"
                // These become reference types (not pointers!) for C-ABI
                // The parameter name becomes the lowercase class name (e.g., Dom -> dom)
                // because fn_body uses that name (e.g., "dom.root.set_node_type(...)")
                if name == "self" {
                    let (ref_kind, actual_type) = match type_name.as_str() {
                        "ref" => (ArgRefKind::Ref, class_name.to_string()),      // &self -> &ClassName
                        "refmut" => (ArgRefKind::RefMut, class_name.to_string()), // &mut self -> &mut ClassName
                        "value" => (ArgRefKind::Owned, class_name.to_string()),  // self -> ClassName
                        "mut value" => (ArgRefKind::Owned, class_name.to_string()), // mut self -> ClassName
                        other => (ArgRefKind::Owned, other.to_string()), // fallback
                    };
                    return FunctionArg {
                        // Use lowercase class name as parameter name
                        // This matches what fn_body expects (e.g., "dom.root.set_node_type(...)")
                        name: class_name.to_lowercase(),
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
        }).collect();

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

    fn determine_method_kind(&self, fn_data: &crate::api::FunctionData) -> FunctionKind {
        // Analyze fn_data to determine if method, method_mut, or static
        // Look at first argument to see if it's self/&self/&mut self
        // Note: fn_args is Vec<IndexMap<String, String>>, not Option
        // In api.json: "self": "ref" | "refmut" | "value"
        
        if let Some(first_arg) = fn_data.fn_args.first() {
            if let Some((name, value)) = first_arg.iter().next() {
                if name == "self" {
                    // Check the value to determine ref kind
                    if value == "refmut" {
                        return FunctionKind::MethodMut;
                    } else {
                        // "ref" or "value" are both treated as Method
                        return FunctionKind::Method;
                    }
                }
                if name == "&self" {
                    return FunctionKind::Method;
                }
                if name == "&mut self" {
                    return FunctionKind::MethodMut;
                }
            }
        }
        FunctionKind::StaticMethod
    }

    // ========================================================================
    // Phase 6: Trait Functions
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
        let struct_infos: Vec<_> = self.ir.structs.iter().map(|s| {
            (s.name.clone(), s.traits.clone())
        }).collect();

        let enum_infos: Vec<_> = self.ir.enums.iter().map(|e| {
            (e.name.clone(), e.traits.clone())
        }).collect();

        // Generate trait functions for structs
        for (name, traits) in struct_infos {
            self.generate_trait_functions_for_type(&name, &traits);
        }

        // Generate trait functions for enums
        for (name, traits) in enum_infos {
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

        // _deepCopy (Clone)
        if traits.needs_deep_copy() {
            self.ir.functions.push(FunctionDef {
                c_name: format!("Az{}_deepCopy", type_name),
                class_name: type_name.to_string(),
                method_name: "deepCopy".to_string(),
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
        if type_name.ends_with("DestructorType") || type_name.ends_with("CloneCallbackType") {
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
    "XmlNode", "XmlNodeChild", "XmlNodeChildVec", 
    "Xml", "ResultXmlXmlError",
];

/// VecRef types - raw pointer slice wrappers
/// These need special trampolines and are skipped in Python for now
const VECREF_TYPE_NAMES: &[&str] = &[
    // Immutable VecRef types
    "GLuintVecRef", "GLintVecRef", "GLenumVecRef",
    "U8VecRef", "U16VecRef", "U32VecRef", "I32VecRef", "F32VecRef",
    "Refstr", "RefstrVecRef",
    "TessellatedSvgNodeVecRef", "TessellatedColoredSvgNodeVecRef",
    "OptionU8VecRef", "OptionI16VecRef", "OptionI32VecRef",
    "OptionF32VecRef", "OptionFloatVecRef",
    // Mutable VecRefMut types
    "GLintVecRefMut", "GLint64VecRefMut", "GLbooleanVecRefMut",
    "GLfloatVecRefMut", "U8VecRefMut", "F32VecRefMut",
];

/// String type name
const STRING_TYPE_NAME: &str = "String";

/// Vec types that use C-API directly with special conversion
/// These types should NOT get a Python wrapper struct because they
/// have type aliases defined and PyO3 traits implemented on the C-API types
const VEC_TYPE_NAMES: &[&str] = &[
    "U8Vec", "StringVec", "GLuintVec", "GLintVec",
];

/// RefAny type name
const REFANY_TYPE_NAME: &str = "RefAny";

/// Types that use C-API types directly without Python wrapper structs
/// These have type aliases + PyO3 trait impls on C-API types in the patches section
/// They must NOT get wrapper structs generated
const CAPI_DIRECT_TYPES: &[&str] = &[
    // String types
    "String",
    // Vec types
    "U8Vec", "StringVec", "GLuintVec", "GLintVec",
    // RefAny
    "RefAny",
    // Destructor types
    "U8VecDestructor", "StringVecDestructor",
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

    // 3. Check for types that use C-API directly (no Python wrapper)
    // This includes String, Vec types, RefAny, destructors, InstantPtr, etc.
    if CAPI_DIRECT_TYPES.contains(&name) {
        // Determine the specific sub-category
        if name == STRING_TYPE_NAME {
            return TypeCategory::String;
        }
        if VEC_TYPE_NAMES.contains(&name) {
            return TypeCategory::Vec;
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

    // 4. Check for boxed objects
    if class_data.is_boxed_object {
        return TypeCategory::Boxed;
    }

    // 5. Check for generic templates
    if class_data.generic_params.is_some() && 
       !class_data.generic_params.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
        return TypeCategory::GenericTemplate;
    }

    // 6. Check for destructor/clone callback types
    if name.ends_with("Destructor") || 
       name.ends_with("DestructorType") ||
       name.ends_with("CloneCallbackType") {
        return TypeCategory::DestructorOrClone;
    }

    // 7. Check for callback+data pair struct (has callback field + RefAny data field)
    if is_callback_data_pair(class_data, version_data) {
        return TypeCategory::CallbackDataPair;
    }

    // 8. Default to Regular
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
    if name.ends_with("Destructor") || 
       name.ends_with("DestructorType") ||
       name.ends_with("CloneCallbackType") {
        return TypeCategory::DestructorOrClone;
    }

    // 3. Check for generic templates
    if class_data.generic_params.is_some() && 
       !class_data.generic_params.as_ref().map(|v| v.is_empty()).unwrap_or(true) {
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
