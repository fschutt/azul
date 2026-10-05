//! Type Index V2 - Core type definitions and indexing
//!
//! This module provides the new architecture for type discovery:
//! - Parse files with syn
//! - Skip `use` re-exports entirely (they are NOT definitions)
//! - Expand impl_vec!, impl_option!, impl_vec_debug! macros during indexing
//! - Build a fast lookup index

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Result;
use indexmap::IndexMap;
use quote::ToTokens;
use rayon::prelude::*;
use syn::{File, Item, UseTree};

// Re-export RefKind from the types module for use elsewhere
pub use crate::autofix::types::RefKind;

// data structures
/// How `self` is passed to a method
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelfKind {
    /// `self` - takes ownership
    Value,
    /// `&self` - immutable borrow
    Ref,
    /// `&mut self` - mutable borrow
    RefMut,
}

/// A method argument (non-self)
#[derive(Debug, Clone)]
pub struct MethodArg {
    /// Argument name
    pub name: String,
    /// The base type (without ref modifiers)
    pub ty: String,
    /// Reference kind (Value, Ref, RefMut, ConstPtr, MutPtr)
    pub ref_kind: RefKind,
    /// The type's name as the source writes it, before the `Az` prefix is
    /// stripped and an `Into<T>` parameter is resolved: `String` for a std
    /// `String` (`ty` says `String` for an `AzString` too), `AzString`, `S`
    /// for `S: Into<AzString>`. The fn_body converts a std `String`.
    pub source_ty: String,
}

/// A method extracted from an impl block
#[derive(Debug, Clone)]
pub struct MethodDef {
    /// Method name (e.g., "add_callback")
    pub name: String,
    /// How self is passed, None for static methods/constructors without self
    pub self_kind: Option<SelfKind>,
    /// Non-self arguments
    pub args: Vec<MethodArg>,
    /// Return type (None for -> ())
    pub return_type: Option<String>,
    /// Return type reference kind
    pub return_ref_kind: RefKind,
    /// True if this looks like a constructor (returns Self or the type name)
    pub is_constructor: bool,
    /// Doc comments
    pub doc: Vec<String>,
    /// Is this method public
    pub is_public: bool,
    /// The trait this method implements (`impl Default for T` -> `Default`),
    /// `None` for an inherent method. A standard trait's method is not an API
    /// function: api.json carries it as a derive or a custom impl.
    pub from_trait: Option<String>,
}

/// The traits whose impl methods api.json never lists as functions: a
/// derived one is a `derive`, a hand-written one a `custom_impls` entry, and
/// the codegen makes `T_default`, `T_clone`, ... from those.
const STD_TRAITS: &[&str] = &[
    "Default", "Clone", "Copy", "Drop", "Debug", "Display", "PartialEq", "Eq", "PartialOrd",
    "Ord", "Hash", "From", "Into", "TryFrom", "TryInto", "AsRef", "AsMut", "Deref", "DerefMut",
    "Iterator", "IntoIterator", "FromIterator", "Extend", "Send", "Sync",
];

/// Crate-internal plumbing traits: their methods serve the engine (the host
/// invoker's `HostOut::unwritten` builds a callback's default answer), never
/// an API caller. Unlike a standard trait they are not a `custom_impls`
/// entry either - they simply do not cross the API.
const INTERNAL_TRAITS: &[&str] = &["HostOut"];

impl MethodDef {
    /// Whether this is an internal plumbing trait's impl method
    /// (see [`INTERNAL_TRAITS`]).
    pub fn is_internal_trait_impl(&self) -> bool {
        self.from_trait
            .as_deref()
            .is_some_and(|t| INTERNAL_TRAITS.contains(&t))
    }

    /// Whether this method can never be an API function: a standard trait's
    /// (a derive / custom impl in api.json) or an internal trait's.
    pub fn is_non_api_trait_impl(&self) -> bool {
        self.is_std_trait_impl() || self.is_internal_trait_impl()
    }

    /// Whether this is a standard trait's impl method (`default`, `clone`,
    /// `drop`, `fmt`, `eq`, ...): never an API function, never "missing from
    /// api.json". A wrapper trait that exposes free functions stays a
    /// candidate.
    pub fn is_std_trait_impl(&self) -> bool {
        self.from_trait
            .as_deref()
            .is_some_and(|t| STD_TRAITS.contains(&t))
    }

    /// The signature as the console shows it: the receiver, every argument
    /// with its reference kind, the return type -
    /// `(&mut self, host: DomNodeId, format: &TextFormat) -> ()`. The
    /// summaries of `autofix add`, `discover` and `debug api` all print this;
    /// they used to show the receiver alone, which read as "the tool dropped
    /// the arguments" although the patch it generated carried them.
    pub fn signature(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        match self.self_kind {
            None => {}
            Some(SelfKind::Value) => parts.push("self".to_string()),
            Some(SelfKind::Ref) => parts.push("&self".to_string()),
            Some(SelfKind::RefMut) => parts.push("&mut self".to_string()),
        }
        for arg in &self.args {
            parts.push(format!("{}: {}{}", arg.name, arg.ref_kind.as_prefix(), arg.ty));
        }
        let ret = self.return_type.as_deref().unwrap_or("()");
        format!(
            "({}) -> {}{}",
            parts.join(", "),
            self.return_ref_kind.as_prefix(),
            ret
        )
    }
}

/// A type definition discovered from parsing source files.
/// This is NOT created for `use` re-exports - only actual definitions.
#[derive(Debug, Clone)]
pub struct TypeDefinition {
    /// Full path: "azul_core::resources::FontCache"
    pub full_path: String,
    /// Simple name: "FontCache"
    pub type_name: String,
    /// Source file where defined
    pub file_path: PathBuf,
    /// Module path within crate: "resources"
    pub module_path: String,
    /// Crate name: "azul_core"
    pub crate_name: String,
    /// Kind of type with all metadata
    pub kind: TypeDefKind,
    /// Source code of the definition
    pub source_code: String,
    /// Methods from impl blocks (non-trait impls only)
    pub methods: Vec<MethodDef>,
}

/// The kind of type definition
#[derive(Debug, Clone)]
pub enum TypeDefKind {
    Struct {
        fields: IndexMap<String, FieldDef>,
        /// The repr attribute value, e.g. Some("C") or None if missing
        repr: Option<String>,
        /// Number of `#[repr(...)]` attributes found on the source type.
        /// More than 1 is an error (duplicate repr attributes).
        repr_attr_count: usize,
        generic_params: Vec<String>,
        derives: Vec<String>,
        /// Traits with manual `impl Trait for Type` blocks (e.g., Clone, Debug, Drop)
        custom_impls: Vec<String>,
        /// True if this is a tuple struct like `struct Foo(pub u64)` instead of `struct Foo {
        /// inner: u64 }`
        is_tuple_struct: bool,
    },
    Enum {
        variants: IndexMap<String, VariantDef>,
        /// The repr attribute value, e.g. Some("C"), Some("C, u8"), or None if missing
        repr: Option<String>,
        /// Number of `#[repr(...)]` attributes found on the source type.
        /// More than 1 is an error (duplicate repr attributes).
        repr_attr_count: usize,
        generic_params: Vec<String>,
        derives: Vec<String>,
        /// Traits with manual `impl Trait for Type` blocks (e.g., Clone, Debug, Drop)
        custom_impls: Vec<String>,
    },
    TypeAlias {
        /// The base type (e.g., "CssPropertyValue" from "CssPropertyValue<BreakInside>")
        target: String,
        /// The generic base type without args (e.g., "CssPropertyValue")
        /// If None, this is a simple alias without generics
        generic_base: Option<String>,
        /// Generic arguments (e.g., ["BreakInside"])
        generic_args: Vec<String>,
    },
    CallbackTypedef {
        args: Vec<CallbackArg>,
        returns: Option<String>,
    },
    /// Type generated by a macro like impl_vec!
    MacroGenerated {
        source_macro: String,
        base_type: String,
        kind: MacroGeneratedKind,
        /// Derives from the macro invocation (e.g., ["Debug", "Clone"])
        derives: Vec<String>,
        /// Traits with manual `impl Trait for Type` blocks (e.g., Eq from impl_vec_eq!)
        implemented_traits: Vec<String>,
    },
}

/// Kind of macro-generated type
#[derive(Debug, Clone)]
pub enum MacroGeneratedKind {
    /// FooVec from impl_vec!(Foo, FooVec, ...)
    Vec,
    /// FooVecDestructor from impl_vec!(Foo, FooVec, FooVecDestructor, ...)
    VecDestructor,
    /// FooVecDestructorType - the callback_typedef for extern "C" fn(*mut FooVec)
    /// Generated from impl_vec! as the External variant's function pointer type
    VecDestructorType,
    /// FooVecSlice from impl_vec!(Foo, FooVec, FooVecDestructor, FooVecDestructorType,
    /// FooVecSlice) C-compatible slice struct with ptr and len
    VecSlice,
    /// OptionFoo from impl_option!(Foo, OptionFoo, ...)
    Option,
    /// OptionFooEnumWrapper from impl_option!(Foo, OptionFoo, OptionFooEnumWrapper)
    OptionEnumWrapper,
    /// ResultFooBar from impl_result!(Foo, Bar, ResultFooBar, ...)
    Result,
    /// CallbackWrapper from impl_widget_callback!(CallbackWrapper, Option, CallbackValue,
    /// CallbackType)
    CallbackWrapper,
    /// CallbackValue from impl_callback!(CallbackWrapper, Option, CallbackValue, CallbackType)
    CallbackValue,
}

/// Field definition in a struct
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: String,
    /// The base type (for pointers/Box, this is "c_void")
    pub ty: String,
    /// The reference kind (Value, ConstPtr, MutPtr, or Boxed)
    pub ref_kind: RefKind,
    pub doc: Vec<String>,
}

/// Variant definition in an enum
#[derive(Debug, Clone)]
pub struct VariantDef {
    pub name: String,
    pub ty: Option<String>,
    pub doc: Vec<String>,
}

/// Argument in a callback typedef
#[derive(Debug, Clone)]
pub struct CallbackArg {
    pub name: Option<String>,
    /// The base type without reference modifiers (e.g., "RefAny" not "&mut RefAny")
    pub ty: String,
    /// The reference kind (Ref, RefMut, Value)
    pub ref_kind: RefKind,
}

// macro expansion
impl TypeDefinition {
    /// Expand a MacroGenerated type into a real Struct/Enum/CallbackTypedef.
    /// Returns the original kind if not MacroGenerated.
    pub fn expand_macro_generated(&self) -> TypeDefKind {
        match &self.kind {
            TypeDefKind::MacroGenerated {
                kind,
                base_type,
                source_macro,
                derives: macro_derives,
                implemented_traits,
            } => {
                match kind {
                    MacroGeneratedKind::Vec => {
                        // impl_vec!(BaseType, VecType, DestructorType)
                        // VecType has: ptr, len, cap, destructor
                        // Destructor is named VecTypeDestructor (e.g. U8Vec -> U8VecDestructor)
                        let destructor_type = format!("{}Destructor", self.type_name);
                        let mut fields = IndexMap::new();
                        // ptr field type is the element type (base_type), not c_void
                        fields.insert(
                            "ptr".to_string(),
                            FieldDef {
                                name: "ptr".to_string(),
                                ty: base_type.clone(),
                                ref_kind: RefKind::ConstPtr,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "len".to_string(),
                            FieldDef {
                                name: "len".to_string(),
                                ty: "usize".to_string(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "cap".to_string(),
                            FieldDef {
                                name: "cap".to_string(),
                                ty: "usize".to_string(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "destructor".to_string(),
                            FieldDef {
                                name: "destructor".to_string(),
                                ty: destructor_type,
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "flags".to_string(),
                            FieldDef {
                                name: "flags".to_string(),
                                ty: "u8".to_string(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Struct {
                            fields,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                            is_tuple_struct: false,
                        }
                    }
                    MacroGeneratedKind::VecDestructor => {
                        // VecDestructor enum: DefaultRust, NoDestructor,
                        // External(DestructorTypeType), AlreadyDestroyed
                        let destructor_type_type = format!("{}Type", self.type_name);
                        let mut variants = IndexMap::new();
                        variants.insert(
                            "DefaultRust".to_string(),
                            VariantDef {
                                name: "DefaultRust".to_string(),
                                ty: None,
                                doc: Vec::new(),
                            },
                        );
                        variants.insert(
                            "NoDestructor".to_string(),
                            VariantDef {
                                name: "NoDestructor".to_string(),
                                ty: None,
                                doc: Vec::new(),
                            },
                        );
                        variants.insert(
                            "External".to_string(),
                            VariantDef {
                                name: "External".to_string(),
                                ty: Some(destructor_type_type),
                                doc: Vec::new(),
                            },
                        );
                        variants.insert(
                            "AlreadyDestroyed".to_string(),
                            VariantDef {
                                name: "AlreadyDestroyed".to_string(),
                                ty: None,
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Enum {
                            variants,
                            repr: Some("C, u8".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                        }
                    }
                    MacroGeneratedKind::VecDestructorType => {
                        // DestructorTypeType callback: extern "C" fn(*mut VecType)
                        // base_type here is the VecType name
                        TypeDefKind::CallbackTypedef {
                            args: vec![CallbackArg {
                                name: Some("ptr".to_string()),
                                ty: base_type.clone(),
                                ref_kind: RefKind::MutPtr,
                            }],
                            returns: None,
                        }
                    }
                    MacroGeneratedKind::VecSlice => {
                        // C-compatible slice struct: { ptr: *const BaseType, len: usize }
                        // base_type here is the element type
                        let mut fields = IndexMap::new();
                        fields.insert(
                            "ptr".to_string(),
                            FieldDef {
                                name: "ptr".to_string(),
                                ty: base_type.clone(),
                                ref_kind: RefKind::ConstPtr,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "len".to_string(),
                            FieldDef {
                                name: "len".to_string(),
                                ty: "usize".to_string(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Struct {
                            fields,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: Vec::new(),
                            derives: vec![
                                "Debug".to_string(),
                                "Copy".to_string(),
                                "Clone".to_string(),
                            ],
                            custom_impls: Vec::new(),
                            is_tuple_struct: false,
                        }
                    }
                    MacroGeneratedKind::Option => {
                        // impl_option!(BaseType, OptionType, ...)
                        // OptionType enum: None, Some(BaseType)
                        let mut variants = IndexMap::new();
                        variants.insert(
                            "None".to_string(),
                            VariantDef {
                                name: "None".to_string(),
                                ty: None,
                                doc: Vec::new(),
                            },
                        );
                        variants.insert(
                            "Some".to_string(),
                            VariantDef {
                                name: "Some".to_string(),
                                ty: Some(base_type.clone()),
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Enum {
                            variants,
                            repr: Some("C, u8".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string(), "Clone".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                        }
                    }
                    MacroGeneratedKind::OptionEnumWrapper => {
                        // OptionEnumWrapper: wraps the inner type with a tag
                        // Struct with: tag (u8), value (BaseType)
                        let mut fields = IndexMap::new();
                        fields.insert(
                            "tag".to_string(),
                            FieldDef {
                                name: "tag".to_string(),
                                ty: "u8".to_string(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "payload".to_string(),
                            FieldDef {
                                name: "payload".to_string(),
                                ty: base_type.clone(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Struct {
                            fields,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string(), "Clone".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                            is_tuple_struct: false,
                        }
                    }
                    MacroGeneratedKind::Result => {
                        // impl_result!(OkType, ErrType, ResultType, ...)
                        // base_type is "Result<OkType, ErrType>"
                        // ResultType enum: Ok(OkType), Err(ErrType)
                        let (ok_type, err_type) = parse_result_base_type(base_type);
                        let mut variants = IndexMap::new();
                        variants.insert(
                            "Ok".to_string(),
                            VariantDef {
                                name: "Ok".to_string(),
                                ty: Some(ok_type),
                                doc: Vec::new(),
                            },
                        );
                        variants.insert(
                            "Err".to_string(),
                            VariantDef {
                                name: "Err".to_string(),
                                ty: Some(err_type),
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Enum {
                            variants,
                            repr: Some("C, u8".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string(), "Clone".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                        }
                    }
                    MacroGeneratedKind::CallbackWrapper => {
                        // impl_widget_callback!(CallbackWrapper, OptionCallbackWrapper,
                        // CallbackValue, CallbackType) CallbackWrapper
                        // struct: data (RefAny), callback (CallbackValue)
                        // NOTE: Order matters for repr(C)! data comes first, then callback.
                        let mut fields = IndexMap::new();
                        fields.insert(
                            "data".to_string(),
                            FieldDef {
                                name: "data".to_string(),
                                ty: "RefAny".to_string(),
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "callback".to_string(),
                            FieldDef {
                                name: "callback".to_string(),
                                ty: base_type.clone(), // CallbackValue
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Struct {
                            fields,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                            is_tuple_struct: false,
                        }
                    }
                    MacroGeneratedKind::CallbackValue => {
                        // CallbackValue struct: cb (CallbackType - the extern "C" fn), callable
                        // (OptionRefAny)
                        let mut fields = IndexMap::new();
                        fields.insert(
                            "cb".to_string(),
                            FieldDef {
                                name: "cb".to_string(),
                                ty: base_type.clone(), // CallbackType
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        fields.insert(
                            "callable".to_string(),
                            FieldDef {
                                name: "callable".to_string(),
                                ty: "OptionRefAny".to_string(), // For FFI: stores foreign callable
                                ref_kind: RefKind::Value,
                                doc: Vec::new(),
                            },
                        );
                        TypeDefKind::Struct {
                            fields,
                            repr: Some("C".to_string()),
                            repr_attr_count: 1,
                            generic_params: vec![],
                            derives: if macro_derives.is_empty() {
                                vec!["Debug".to_string()]
                            } else {
                                macro_derives.clone()
                            },
                            custom_impls: implemented_traits.clone(),
                            is_tuple_struct: false,
                        }
                    }
                }
            }
            // Not MacroGenerated - return as-is
            other => other.clone(),
        }
    }

    /// Get the expanded kind (expands MacroGenerated, returns original otherwise)
    pub fn get_expanded_kind(&self) -> TypeDefKind {
        self.expand_macro_generated()
    }

    /// Check if this is a MacroGenerated type
    pub fn is_macro_generated(&self) -> bool {
        matches!(&self.kind, TypeDefKind::MacroGenerated { .. })
    }

    /// Convert TypeDefinition to ParsedTypeInfo (for compatibility with existing code)
    /// This expands MacroGenerated types into their real Struct/Enum/CallbackTypedef form
    pub fn to_parsed_type_info(&self) -> crate::patch::index::ParsedTypeInfo {
        use crate::{
            api::BorrowMode,
            patch::index::{CallbackArgInfo, FieldInfo, ParsedTypeInfo, TypeKind, VariantInfo},
        };

        // Get the expanded kind (expands MacroGenerated to real types)
        let expanded_kind = self.expand_macro_generated();

        // Convert module_path from "mod1::mod2" to vec!["mod1", "mod2"]
        let module_path: Vec<String> = if self.module_path.is_empty() {
            vec![]
        } else {
            self.module_path
                .split("::")
                .map(|s| s.to_string())
                .collect()
        };

        // Convert TypeDefKind to TypeKind
        let kind = match expanded_kind {
            TypeDefKind::Struct {
                fields,
                repr,
                generic_params,
                derives,
                custom_impls,
                is_tuple_struct: _,
                ..
            } => TypeKind::Struct {
                fields: fields
                    .into_iter()
                    .map(|(name, field)| {
                        (
                            name.clone(),
                            FieldInfo {
                                name,
                                ty: field.ty,
                                ref_kind: field.ref_kind,
                                doc: if field.doc.is_empty() {
                                    None
                                } else {
                                    Some(field.doc)
                                },
                            },
                        )
                    })
                    .collect(),
                repr,
                doc: None,
                generic_params,
                implemented_traits: custom_impls,
                derives,
            },
            TypeDefKind::Enum {
                variants,
                repr,
                generic_params,
                derives,
                custom_impls,
                ..
            } => TypeKind::Enum {
                variants: variants
                    .into_iter()
                    .map(|(name, variant)| {
                        (
                            name.clone(),
                            VariantInfo {
                                name,
                                ty: variant.ty,
                                doc: if variant.doc.is_empty() {
                                    None
                                } else {
                                    Some(variant.doc)
                                },
                            },
                        )
                    })
                    .collect(),
                repr,
                doc: None,
                generic_params,
                implemented_traits: custom_impls,
                derives,
            },
            TypeDefKind::TypeAlias {
                target,
                generic_base,
                generic_args,
            } => TypeKind::TypeAlias {
                target,
                generic_base,
                generic_args,
                doc: None,
            },
            TypeDefKind::CallbackTypedef { args, returns } => {
                TypeKind::CallbackTypedef {
                    fn_args: args
                        .into_iter()
                        .map(|arg| {
                            // Keep RefKind as-is - no conversion needed anymore
                            // The api.json CallbackArgData now uses RefKind directly
                            CallbackArgInfo {
                                name: arg.name,
                                ty: arg.ty,
                                ref_kind: arg.ref_kind,
                            }
                        })
                        .collect(),
                    returns,
                    doc: None,
                }
            }
            TypeDefKind::MacroGenerated { .. } => {
                // This shouldn't happen after expand_macro_generated(), but handle it
                TypeKind::Struct {
                    fields: IndexMap::new(),
                    repr: Some("C".to_string()),
                    doc: None,
                    generic_params: vec![],
                    implemented_traits: vec![],
                    derives: vec![],
                }
            }
        };

        ParsedTypeInfo {
            full_path: self.full_path.clone(),
            type_name: self.type_name.clone(),
            file_path: self.file_path.clone(),
            module_path,
            kind,
            source_code: self.source_code.clone(),
        }
    }
}

/// Parse "Result<OkType, ErrType>" into (OkType, ErrType)
fn parse_result_base_type(base_type: &str) -> (String, String) {
    // Handle "Result<OkType, ErrType>" format
    if base_type.starts_with("Result<") && base_type.ends_with('>') {
        let inner = &base_type[7..base_type.len() - 1];
        // Split by comma, handling nested generics
        if let Some((ok, err)) = split_generic_args(inner) {
            return (ok.trim().to_string(), err.trim().to_string());
        }
    }
    // Fallback
    ("()".to_string(), "()".to_string())
}

/// Split "A, B" into ("A", "B"), handling nested generics
fn split_generic_args(s: &str) -> Option<(String, String)> {
    let mut depth = 0;
    for (i, c) in s.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                return Some((s[..i].to_string(), s[i + 1..].to_string()));
            }
            _ => {}
        }
    }
    None
}

// type index
/// The crates the index reads: (crate name, source folder under the
/// workspace root).
pub(crate) const CRATE_DIRS: &[(&str, &str)] = &[
    ("azul_core", "core/src"),
    ("azul_css", "css/src"),
    ("azul_layout", "layout/src"),
    ("azul_dll", "dll/src"),
];

/// A public free function, as `autofix add <Class>.<name> --fn <path>`
/// reads it.
#[derive(Debug, Clone)]
pub struct FreeFnDef {
    /// The path the api.json fn_body calls: the one given (a public
    /// re-export path stays as it is)
    pub path: String,
    /// The module the function is defined in (`cpurender::text_raster`)
    pub defined_in: String,
    /// Its signature, read with the class it is added to as `Self`
    pub method: MethodDef,
}

/// The public free function `path` (`azul_layout::cpurender::text_image`),
/// read with `class_name` as the type it is added to. The definition is
/// looked up in the path's crate: in the named module, else in a child of it
/// (a `pub use child::*` re-export). A file a `#[path = ".."]` module
/// declaration names has the declared module's path (`xml::html` lives in
/// xml_html.rs).
pub fn find_free_fn(workspace_root: &Path, path: &str, class_name: &str) -> Result<FreeFnDef, String> {
    let segments: Vec<&str> = path.split("::").collect();
    let (crate_name, module, fn_name) = match segments.as_slice() {
        [crate_name, modules @ .., fn_name] if !fn_name.is_empty() => {
            (*crate_name, modules.join("::"), *fn_name)
        }
        _ => return Err(format!("`{path}` is not a path `<crate>::<module>::<fn>`")),
    };
    let Some((_, src)) = CRATE_DIRS.iter().find(|(c, _)| *c == crate_name) else {
        let known: Vec<&str> = CRATE_DIRS.iter().map(|(c, _)| *c).collect();
        return Err(format!(
            "unknown crate `{crate_name}` in `{path}` (the index reads {})",
            known.join(", ")
        ));
    };
    let mut files = Vec::new();
    collect_rust_files(&mut files, crate_name, &workspace_root.join(src));

    // Files a `#[path = ".."]` module declaration names: the declared path
    let mut declared: BTreeMap<PathBuf, String> = BTreeMap::new();
    for (_, file) in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        if !text.contains("#[path") {
            continue;
        }
        let Ok(ast) = syn::parse_file(&text) else {
            continue;
        };
        let parent_module = infer_module_path(crate_name, file);
        for item in &ast.items {
            let Item::Mod(m) = item else { continue };
            if m.content.is_some() {
                continue;
            }
            let Some(target) = m.attrs.iter().find_map(path_attr_value) else {
                continue;
            };
            let Some(dir) = file.parent() else { continue };
            let module_path = if parent_module.is_empty() {
                m.ident.to_string()
            } else {
                format!("{parent_module}::{}", m.ident)
            };
            declared.insert(dir.join(target), module_path);
        }
    }

    let needle = format!("fn {fn_name}");
    let mut found: Vec<(String, MethodDef)> = Vec::new();
    for (_, file) in &files {
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        if !text.contains(&needle) {
            continue;
        }
        let Ok(ast) = syn::parse_file(&text) else {
            continue;
        };
        let module_path = declared
            .get(file)
            .cloned()
            .unwrap_or_else(|| infer_module_path(crate_name, file));
        collect_free_fns(&ast.items, &module_path, fn_name, class_name, &mut found);
    }

    let child = format!("{module}::");
    let exact: Vec<&(String, MethodDef)> = found.iter().filter(|(m, _)| *m == module).collect();
    let children: Vec<&(String, MethodDef)> = found
        .iter()
        .filter(|(m, _)| module.is_empty() || m.starts_with(&child))
        .collect();
    let chosen = match (exact.as_slice(), children.as_slice()) {
        ([one], _) | ([], [one]) => *one,
        _ => {
            let at: Vec<&str> = found.iter().map(|(m, _)| m.as_str()).collect();
            return Err(if found.is_empty() {
                format!("no public fn `{fn_name}` in {crate_name}")
            } else {
                format!(
                    "`{path}`: no single public fn `{fn_name}` in `{module}` or a child of it \
                     (found in: {})",
                    at.join(", ")
                )
            });
        }
    };
    Ok(FreeFnDef {
        path: path.to_string(),
        defined_in: chosen.0.clone(),
        method: chosen.1.clone(),
    })
}

/// The file a `#[path = "file.rs"]` attribute names.
fn path_attr_value(attr: &syn::Attribute) -> Option<String> {
    if !attr.path().is_ident("path") {
        return None;
    }
    let syn::Meta::NameValue(nv) = &attr.meta else {
        return None;
    };
    match &nv.value {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(s),
            ..
        }) => Some(s.value()),
        _ => None,
    }
}

/// The public free functions `fn_name` in `items` (inline modules too),
/// with their module paths.
fn collect_free_fns(
    items: &[Item],
    module: &str,
    fn_name: &str,
    class_name: &str,
    out: &mut Vec<(String, MethodDef)>,
) {
    for item in items {
        match item {
            Item::Fn(f)
                if f.sig.ident == fn_name && matches!(f.vis, syn::Visibility::Public(_)) =>
            {
                if let Some(method) = free_fn_method(f, class_name) {
                    out.push((module.to_string(), method));
                }
            }
            Item::Mod(m) => {
                if let Some((_, nested)) = &m.content {
                    let nested_module = if module.is_empty() {
                        m.ident.to_string()
                    } else {
                        format!("{module}::{}", m.ident)
                    };
                    collect_free_fns(nested, &nested_module, fn_name, class_name, out);
                }
            }
            _ => {}
        }
    }
}

/// The signature of the free function `f` as a static method of
/// `class_name` (the method extraction's rules: `Into<T>` generics resolved,
/// other generics skipped, `Self` / constructor-ness against `class_name`).
pub(crate) fn free_fn_method(f: &syn::ItemFn, class_name: &str) -> Option<MethodDef> {
    let as_method = syn::ImplItemFn {
        attrs: f.attrs.clone(),
        vis: f.vis.clone(),
        defaultness: None,
        sig: f.sig.clone(),
        block: (*f.block).clone(),
    };
    extract_method_def(&as_method, class_name)
}

/// Fast lookup index for type definitions
#[derive(Debug, Default)]
pub struct TypeIndex {
    /// Map from simple type name to all definitions with that name
    by_name: BTreeMap<String, Vec<Arc<TypeDefinition>>>,
    /// Map from full path to definition
    by_path: BTreeMap<String, Arc<TypeDefinition>>,
    /// The modules declared without `pub` (`azul_layout::cpurender::named`):
    /// a path through one does not name its type from another crate
    private_modules: std::collections::BTreeSet<String>,
    /// Errors encountered during indexing
    pub errors: Vec<String>,
}

impl TypeIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Build the type index from a workspace root
    pub fn build(workspace_root: &Path, verbose: bool) -> Result<Self> {
        let mut index = Self::new();

        // Collect all .rs files
        let mut all_files: Vec<(String, PathBuf)> = Vec::new();

        for (crate_name, src_path) in CRATE_DIRS {
            let src_dir = workspace_root.join(src_path);
            if src_dir.exists() {
                collect_rust_files(&mut all_files, crate_name, &src_dir);
            }
        }

        if verbose {
            eprintln!("[TypeIndex] Found {} Rust files to parse", all_files.len());
        }

        // Phase 1: Parse files in parallel and collect types (with same-file methods)
        let results: Vec<_> = all_files
            .par_iter()
            .map(|(crate_name, file_path)| parse_file_for_types(crate_name, file_path))
            .collect();

        // Merge results
        let mut parsed: Vec<TypeDefinition> = Vec::new();
        let mut facts = ModuleFacts::default();
        for result in results {
            match result {
                Ok((types, file_facts)) => {
                    parsed.extend(types);
                    facts.private.extend(file_facts.private);
                    facts.public.extend(file_facts.public);
                    facts.reexports.extend(file_facts.reexports);
                }
                Err(e) => {
                    index.errors.push(e);
                }
            }
        }

        // A type behind a private module is named by the `pub use` that
        // re-exports it: the private path does not compile in another crate
        // (TextRasterStyle, wave 6). azul_dll is left out: the generated code
        // lives in that crate, where its private modules are in reach.
        let public: std::collections::BTreeSet<&String> = facts.public.iter().collect();
        index.private_modules = facts
            .private
            .iter()
            .filter(|m| !public.contains(m) && !m.starts_with("azul_dll::"))
            .cloned()
            .collect();
        for mut typedef in parsed {
            let public = typedef
                .full_path
                .rsplit_once("::")
                .filter(|(module, _)| first_private_module(module, &index.private_modules).is_some())
                .and_then(|(module, name)| {
                    public_path(module, name, &index.private_modules, &facts.reexports, 0)
                });
            if let Some(path) = public {
                typedef.full_path = path;
            }
            index.add_type(typedef);
        }

        // Phase 2: Collect cross-file impl blocks and attach methods to types
        // This handles cases where `impl SomeType` is in a different file than `struct SomeType`
        let cross_file_methods: Vec<_> = all_files
            .par_iter()
            .filter_map(|(_, file_path)| parse_file_for_cross_file_methods(file_path).ok())
            .collect();

        // Merge cross-file methods into existing types
        for methods_map in cross_file_methods {
            for (type_name, methods) in methods_map {
                index.attach_methods_to_type(&type_name, methods);
            }
        }

        if verbose {
            eprintln!(
                "[TypeIndex] Indexed {} unique type names, {} total paths",
                index.by_name.len(),
                index.by_path.len()
            );
        }

        Ok(index)
    }

    /// Attach methods from cross-file impl blocks to existing types
    fn attach_methods_to_type(&mut self, type_name: &str, methods: Vec<MethodDef>) {
        if let Some(candidates) = self.by_name.get_mut(type_name) {
            // Attach to all candidates (usually there's only one definition)
            for arc in candidates.iter_mut() {
                // We need to get a mutable reference to the TypeDefinition
                // Since we use Arc, we need to use Arc::make_mut
                let typedef = Arc::make_mut(arc);
                for method in &methods {
                    // Avoid duplicates by checking if method already exists
                    if !typedef.methods.iter().any(|m| m.name == method.name) {
                        typedef.methods.push(method.clone());
                    }
                }
                // `by_path` shares the Arc, so `make_mut` cloned the type:
                // point the path at the copy that has the methods.
                self.by_path
                    .insert(typedef.full_path.clone(), Arc::clone(arc));
            }
        }
    }

    /// Add a type definition to the index
    fn add_type(&mut self, typedef: TypeDefinition) {
        let type_name = typedef.type_name.clone();
        let full_path = typedef.full_path.clone();
        let arc = Arc::new(typedef);

        self.by_name
            .entry(type_name)
            .or_default()
            .push(Arc::clone(&arc));
        self.by_path.insert(full_path, arc);
    }

    /// Find the best match for a type name, preferring certain crates
    pub fn resolve(
        &self,
        type_name: &str,
        preferred_crate: Option<&str>,
    ) -> Option<&TypeDefinition> {
        let candidates = self.by_name.get(type_name)?;

        if candidates.is_empty() {
            return None;
        }

        if candidates.len() == 1 {
            return Some(&candidates[0]);
        }

        // Score candidates
        let mut best: Option<&Arc<TypeDefinition>> = None;
        let mut best_score = i32::MIN;

        for candidate in candidates {
            let mut score = 0;

            // Prefer matching crate - this should take ABSOLUTE priority
            // Use a very high score to override all other considerations
            if let Some(pref) = preferred_crate {
                if candidate.crate_name == pref {
                    score += 10000; // Absolute priority for explicit preference
                }
            }

            // Prefer azul_core over others (but lower priority than explicit preference)
            // Blacklist azul_dll - it's being phased out, prefer azul_layout
            if candidate.crate_name == "azul_dll" {
                score -= 1000; // Strongly deprioritize azul_dll
            } else if candidate.crate_name == "azul_core" {
                score += 100;
            } else if candidate.crate_name == "azul_css" {
                score += 80;
            } else if candidate.crate_name == "azul_layout" {
                score += 60;
            }

            // Prefer non-macro-generated
            if !matches!(candidate.kind, TypeDefKind::MacroGenerated { .. }) {
                score += 10;
            }

            // Prefer types with repr(C) or other FFI-safe repr
            let has_ffi_repr = match &candidate.kind {
                TypeDefKind::Struct { repr, .. } | TypeDefKind::Enum { repr, .. } => {
                    is_ffi_safe_repr(repr)
                }
                _ => false,
            };
            if has_ffi_repr {
                score += 20;
            }

            if score > best_score {
                best_score = score;
                best = Some(candidate);
            }
        }

        best.map(|arc| arc.as_ref())
    }

    /// Get a type by its full path
    pub fn get_by_path(&self, full_path: &str) -> Option<&TypeDefinition> {
        self.by_path.get(full_path).map(|arc| arc.as_ref())
    }

    /// Get all definitions for a type name
    pub fn get_all_by_name(&self, type_name: &str) -> Option<&Vec<Arc<TypeDefinition>>> {
        self.by_name.get(type_name)
    }

    /// Check if a type name is a primitive
    pub fn is_primitive(type_name: &str) -> bool {
        const PRIMITIVES: &[&str] = &[
            "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
            "f32", "f64", "bool", "char", "()", "c_void",
        ];

        let trimmed = type_name.trim();

        // Direct primitive check
        if PRIMITIVES.contains(&trimmed) {
            return true;
        }

        // Pointer types
        if trimmed.starts_with("*const ") || trimmed.starts_with("*mut ") {
            return true;
        }

        // References
        if trimmed.starts_with('&') {
            return true;
        }

        // Arrays
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            return true;
        }

        false
    }

    /// Get all type names in the index
    pub fn all_type_names(&self) -> impl Iterator<Item = &String> {
        self.by_name.keys()
    }

    /// Iterate over all types in the index, yielding (type_name, Vec<TypeDefinition>)
    pub fn iter_all(&self) -> impl Iterator<Item = (&String, &Vec<Arc<TypeDefinition>>)> {
        self.by_name.iter()
    }

    /// Get the number of unique type names
    pub fn type_count(&self) -> usize {
        self.by_name.len()
    }

    /// The first private module on the way to `path` (a type or module
    /// path), if any: the path does not name its item from another crate.
    pub fn private_module_on(&self, path: &str) -> Option<String> {
        first_private_module(path, &self.private_modules)
    }

    /// Record `module` as declared without `pub` (tests).
    #[cfg(test)]
    pub fn add_private_module_for_test(&mut self, module: &str) {
        self.private_modules.insert(module.to_string());
    }

    /// Add a type definition for testing purposes
    #[cfg(test)]
    pub fn add_type_for_test(&mut self, typedef: TypeDefinition) {
        let full_path = if typedef.module_path.is_empty() {
            format!("{}::{}", typedef.crate_name, typedef.type_name)
        } else {
            format!(
                "{}::{}::{}",
                typedef.crate_name, typedef.module_path, typedef.type_name
            )
        };
        let arc = Arc::new(typedef.clone());
        self.by_name
            .entry(typedef.type_name.clone())
            .or_default()
            .push(Arc::clone(&arc));
        self.by_path.insert(full_path, arc);
    }

    // // compatibility methods (for workspaceindex replacement)
    //
    /// Find a type by name and return as ParsedTypeInfo (WorkspaceIndex compatibility)
    /// Also tries with "Az" prefix if not found
    pub fn find_type(&self, type_name: &str) -> Option<Vec<crate::patch::index::ParsedTypeInfo>> {
        // Try exact match first
        if let Some(defs) = self.by_name.get(type_name) {
            let parsed: Vec<_> = defs.iter().map(|d| d.to_parsed_type_info()).collect();
            if !parsed.is_empty() {
                return Some(parsed);
            }
        }
        // Try with "Az" prefix
        let az_prefixed = format!("Az{}", type_name);
        if let Some(defs) = self.by_name.get(&az_prefixed) {
            let parsed: Vec<_> = defs.iter().map(|d| d.to_parsed_type_info()).collect();
            if !parsed.is_empty() {
                return Some(parsed);
            }
        }
        None
    }

    /// Find a type by full path and return as ParsedTypeInfo (WorkspaceIndex compatibility)
    /// Also tries with "Az" prefix if not found
    pub fn find_type_by_path(
        &self,
        type_path: &str,
    ) -> Option<crate::patch::index::ParsedTypeInfo> {
        // Try exact match first
        if let Some(def) = self.by_path.get(type_path) {
            return Some(def.to_parsed_type_info());
        }
        // Try with "Az" prefix in path
        let az_prefixed_path = {
            let parts: Vec<&str> = type_path.rsplitn(2, "::").collect();
            if parts.len() == 2 {
                format!("{}::Az{}", parts[1], parts[0])
            } else {
                format!("Az{}", type_path)
            }
        };
        self.by_path
            .get(&az_prefixed_path)
            .map(|d| d.to_parsed_type_info())
    }

    /// Resolve a type by name and return as ParsedTypeInfo
    pub fn resolve_as_parsed_type_info(
        &self,
        type_name: &str,
        preferred_crate: Option<&str>,
    ) -> Option<crate::patch::index::ParsedTypeInfo> {
        self.resolve(type_name, preferred_crate)
            .map(|d| d.to_parsed_type_info())
    }

    /// Find type by string search (for compatibility with WorkspaceIndex)
    /// This uses the existing resolve mechanism but always expands MacroGenerated types
    pub fn find_type_by_string_search(
        &self,
        type_name: &str,
    ) -> Option<crate::patch::index::ParsedTypeInfo> {
        // First try exact name match
        if let Some(def) = self.resolve(type_name, None) {
            return Some(def.to_parsed_type_info());
        }
        // Try with "Az" prefix
        let az_prefixed = format!("Az{}", type_name);
        if let Some(def) = self.resolve(&az_prefixed, None) {
            return Some(def.to_parsed_type_info());
        }
        None
    }
}

// file collection
/// Recursively collect all .rs files in a directory
fn collect_rust_files(files: &mut Vec<(String, PathBuf)>, crate_name: &str, dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Skip excluded paths (tests, examples, build.rs, etc.)
        if crate::autofix::module_map::should_exclude_path(&path) {
            continue;
        }

        if path.is_dir() {
            collect_rust_files(files, crate_name, &path);
        } else if path.extension().map_or(false, |e| e == "rs") {
            files.push((crate_name.to_string(), path));
        }
    }
}

// file parsing
/// Parse a single file and extract type definitions
/// True for a type only its own module can name (`struct X` / `enum X`, no
/// `pub`): the generated C API lives in another module, so such a type is
/// never part of it - indexing it let a private helper shadow a public type
/// of the same name (the XML tree builder's `NodeData`, XML8's `Content`).
fn is_module_private(vis: &syn::Visibility) -> bool {
    matches!(vis, syn::Visibility::Inherited)
}

/// True if the item is gated behind `#[cfg(target_arch = "wasm32")]` — i.e. a
/// platform stub that mirrors a real (non-wasm) type. The indexer skips these so
/// they don't collide with the canonical definition as a "duplicate type name".
fn is_wasm32_only(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        if !a.path().is_ident("cfg") {
            return false;
        }
        if let syn::Meta::List(list) = &a.meta {
            let s = list.tokens.to_string();
            s.contains("target_arch") && s.contains("wasm32")
        } else {
            false
        }
    })
}

/// Expands the file's own single-arm `macro_rules!` that write items, so the
/// methods they generate are seen like any other inherent method: the
/// wizard pages' `dom` / `with_theme` (`page_theme_and_dom!`) and the
/// standard dialogs' `with_on_event` were invisible to `autofix add` and the
/// apps could not call them (2026-10-02). Only the simple, common shape is
/// expanded - one arm whose pattern is `$name:fragment` parameters separated
/// by commas; anything else is left as it is. The expansion is appended
/// after the invocation (the invocation itself stays); only inherent impls.
fn expand_local_item_macros(file: &mut File) {
    use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

    fn single_arm(tokens: TokenStream) -> Option<(Vec<String>, TokenStream)> {
        let tt: Vec<TokenTree> = tokens.into_iter().collect();
        let (pattern, body) = match tt.as_slice() {
            [TokenTree::Group(p), TokenTree::Punct(eq), TokenTree::Punct(gt), TokenTree::Group(b)]
            | [TokenTree::Group(p), TokenTree::Punct(eq), TokenTree::Punct(gt), TokenTree::Group(b), TokenTree::Punct(_)]
                if eq.as_char() == '=' && gt.as_char() == '>' =>
            {
                (p.stream(), b.stream())
            }
            _ => return None,
        };
        let mut params = Vec::new();
        let toks: Vec<TokenTree> = pattern.into_iter().collect();
        let mut i = 0;
        while i < toks.len() {
            match (&toks[i], toks.get(i + 1), toks.get(i + 2), toks.get(i + 3)) {
                (
                    TokenTree::Punct(d),
                    Some(TokenTree::Ident(name)),
                    Some(TokenTree::Punct(colon)),
                    Some(TokenTree::Ident(_frag)),
                ) if d.as_char() == '$' && colon.as_char() == ':' => {
                    params.push(name.to_string());
                    i += 4;
                    match toks.get(i) {
                        None => {}
                        Some(TokenTree::Punct(c)) if c.as_char() == ',' => i += 1,
                        Some(_) => return None,
                    }
                }
                _ => return None,
            }
        }
        Some((params, body))
    }

    fn split_args(tokens: TokenStream) -> Vec<TokenStream> {
        let mut args = vec![TokenStream::new()];
        for t in tokens {
            match &t {
                TokenTree::Punct(c) if c.as_char() == ',' => args.push(TokenStream::new()),
                _ => args.last_mut().expect("one arg at least").extend([t]),
            }
        }
        if args.last().is_some_and(|a| a.is_empty()) {
            args.pop();
        }
        args
    }

    fn substitute(body: TokenStream, params: &[String], args: &[TokenStream]) -> TokenStream {
        let toks: Vec<TokenTree> = body.into_iter().collect();
        let mut out = TokenStream::new();
        let mut i = 0;
        while i < toks.len() {
            if let (TokenTree::Punct(d), Some(TokenTree::Ident(name))) = (&toks[i], toks.get(i + 1)) {
                if d.as_char() == '$' {
                    if let Some(k) = params.iter().position(|p| *p == name.to_string()) {
                        let arg: Vec<TokenTree> = args[k].clone().into_iter().collect();
                        if let [single @ TokenTree::Ident(_)] = arg.as_slice() {
                            // An identifier (a type, a fn name) goes in as itself.
                            out.extend([single.clone()]);
                        } else {
                            // A None-delimited group keeps an `expr` argument one expression.
                            out.extend([TokenTree::Group(Group::new(Delimiter::None, args[k].clone()))]);
                        }
                        i += 2;
                        continue;
                    }
                }
            }
            match &toks[i] {
                TokenTree::Group(g) => {
                    let mut ng = Group::new(g.delimiter(), substitute(g.stream(), params, args));
                    ng.set_span(g.span());
                    out.extend([TokenTree::Group(ng)]);
                }
                t => out.extend([t.clone()]),
            }
            i += 1;
        }
        out
    }

    let mut macros: BTreeMap<String, (Vec<String>, TokenStream)> = BTreeMap::new();
    for item in &file.items {
        if let Item::Macro(m) = item {
            if m.mac.path.is_ident("macro_rules") {
                if let (Some(ident), Some(def)) = (&m.ident, single_arm(m.mac.tokens.clone())) {
                    macros.insert(ident.to_string(), def);
                }
            }
        }
    }
    if macros.is_empty() {
        return;
    }
    let mut expanded = Vec::new();
    for item in &file.items {
        let Item::Macro(m) = item else { continue };
        if m.ident.is_some() {
            continue;
        }
        let Some(name) = m.mac.path.get_ident().map(ToString::to_string) else { continue };
        let Some((params, body)) = macros.get(&name) else { continue };
        let args = split_args(m.mac.tokens.clone());
        if args.len() != params.len() {
            continue;
        }
        if let Ok(f) = syn::parse2::<File>(substitute(body.clone(), params, &args)) {
            // Only the inherent impls: the methods are what the API tool was
            // missing. Types a macro defines are indexed by their own rules
            // (`extract_macro_generated_types`; defining them twice reported
            // every css property as duplicated) and trait impls would turn
            // into custom_impls changes of unrelated types.
            expanded.extend(
                f.items
                    .into_iter()
                    .filter(|i| matches!(i, Item::Impl(imp) if imp.trait_.is_none())),
            );
        }
    }
    file.items.extend(expanded);
}

fn parse_file_for_types(
    crate_name: &str,
    file_path: &Path,
) -> Result<(Vec<TypeDefinition>, ModuleFacts), String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

    let mut syntax_tree: File = syn::parse_file(&content)
        .map_err(|e| format!("Failed to parse {}: {}", file_path.display(), e))?;
    expand_local_item_macros(&mut syntax_tree);

    let module_path = infer_module_path(crate_name, file_path);
    let mut types = Vec::new();

    // First pass: collect all type definitions
    for item in &syntax_tree.items {
        match item {
            // SKIP Item::Use entirely - re-exports are NOT definitions
            Item::Use(_) => continue,

            Item::Struct(s) => {
                // Skip wasm-only platform stubs (e.g. dll/src/unified/* for the
                // desktop-extra types): they intentionally mirror a real type by
                // name + repr-C layout, so indexing them would raise a false
                // "duplicate type name" against the canonical (non-wasm) def.
                if !is_wasm32_only(&s.attrs) && !is_module_private(&s.vis) {
                    if let Some(typedef) = extract_struct(crate_name, &module_path, file_path, s) {
                        types.push(typedef);
                    }
                }
            }

            Item::Enum(e) => {
                if !is_wasm32_only(&e.attrs) && !is_module_private(&e.vis) {
                    if let Some(typedef) = extract_enum(crate_name, &module_path, file_path, e) {
                        types.push(typedef);
                    }
                }
            }

            Item::Type(t) => {
                if let Some(typedef) = extract_type_alias(crate_name, &module_path, file_path, t) {
                    types.push(typedef);
                }
            }

            Item::Macro(m) => {
                // Handle impl_vec!, impl_option!, etc.
                let generated =
                    extract_macro_generated_types(crate_name, &module_path, file_path, m);
                types.extend(generated);
            }

            Item::Mod(m) => {
                // Skip wasm32-only stub modules: their contents mirror the real
                // (non-wasm) types (e.g. `unified::video_codec::provision`) and
                // must not clobber the canonical defs in the index.
                if is_wasm32_only(&m.attrs) {
                    continue;
                }
                // Recursively extract types from inline modules (e.g., `mod decode { ... }`)
                if let Some((_, items)) = &m.content {
                    let mod_name = m.ident.to_string();
                    let nested_module_path = if module_path.is_empty() {
                        mod_name.clone()
                    } else {
                        format!("{}::{}", module_path, mod_name)
                    };
                    let nested_types =
                        extract_types_from_items(crate_name, &nested_module_path, file_path, items);
                    types.extend(nested_types);
                }
            }

            _ => {}
        }
    }

    // Second pass: extract manual trait implementations (`impl Trait for Type`)
    // and attach them to the corresponding type definitions
    let custom_impls_map = extract_custom_impls_from_items(&syntax_tree.items);

    // Attach custom_impls to types
    for typedef in &mut types {
        if let Some(impls) = custom_impls_map.get(&typedef.type_name) {
            attach_custom_impls(typedef, impls.clone());
        }
    }

    // Third pass: extract inherent methods from impl blocks
    let methods_map = extract_inherent_methods_from_items(&syntax_tree.items);
    for typedef in &mut types {
        if let Some(methods) = methods_map.get(&typedef.type_name) {
            attach_methods(typedef, methods.clone());
        }
    }

    let facts = module_facts(crate_name, &module_path, &syntax_tree.items);
    Ok((types, facts))
}

/// What one file's top-level items say about reaching definitions from
/// another crate: its module declarations and its `pub use` re-exports.
#[derive(Debug, Default)]
struct ModuleFacts {
    /// Modules declared without `pub` (absolute paths)
    private: Vec<String>,
    /// Modules declared `pub` (absolute paths): a `#[cfg]` pair of a private
    /// and a public declaration of one module is public
    public: Vec<String>,
    /// `pub use` items
    reexports: Vec<Reexport>,
}

/// One item of a `pub use`.
#[derive(Debug, Clone)]
struct Reexport {
    /// The module declaring it (absolute: `azul_layout::cpurender`)
    at: String,
    /// The module its items come from (absolute: `..::cpurender::text_raster`)
    from: String,
    /// `None` for a glob, else the item's name (a rename is not followed)
    item: Option<String>,
}

/// The [`ModuleFacts`] of `items`, the top level of the module `module_path`
/// of `crate_name`.
fn module_facts(crate_name: &str, module_path: &str, items: &[Item]) -> ModuleFacts {
    let at = if module_path.is_empty() {
        crate_name.to_string()
    } else {
        format!("{crate_name}::{module_path}")
    };
    let mut facts = ModuleFacts::default();
    for item in items {
        match item {
            Item::Mod(m) => {
                let path = format!("{at}::{}", m.ident);
                if matches!(m.vis, syn::Visibility::Public(_)) {
                    facts.public.push(path);
                } else {
                    facts.private.push(path);
                }
            }
            Item::Use(u) if matches!(u.vis, syn::Visibility::Public(_)) => {
                let mut flat = Vec::new();
                flatten_use_tree(&u.tree, &mut Vec::new(), &mut flat);
                for (prefix, item) in flat {
                    if let Some(from) = resolve_use_prefix(crate_name, &at, &prefix) {
                        facts.reexports.push(Reexport {
                            at: at.clone(),
                            from,
                            item,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    facts
}

/// The leaves of a use tree: (path prefix, `None` for a glob or the name).
/// A rename (`a as b`) is left out: its type is reached under another name.
fn flatten_use_tree(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    out: &mut Vec<(Vec<String>, Option<String>)>,
) {
    match tree {
        UseTree::Path(p) => {
            prefix.push(p.ident.to_string());
            flatten_use_tree(&p.tree, prefix, out);
            prefix.pop();
        }
        UseTree::Name(n) => out.push((prefix.clone(), Some(n.ident.to_string()))),
        UseTree::Rename(_) => {}
        UseTree::Glob(_) => out.push((prefix.clone(), None)),
        UseTree::Group(g) => {
            for t in &g.items {
                flatten_use_tree(t, prefix, out);
            }
        }
    }
}

/// The absolute module a `use` path prefix names from module `at`:
/// `crate::..`, `self::..`, `super::..`, another crate of the index, or a
/// child of `at`. `None` for an empty prefix or a `super` above the root.
fn resolve_use_prefix(crate_name: &str, at: &str, prefix: &[String]) -> Option<String> {
    let (first, rest) = prefix.split_first()?;
    let mut base: Vec<String> = match first.as_str() {
        "crate" => vec![crate_name.to_string()],
        "self" => at.split("::").map(str::to_string).collect(),
        "super" => {
            let mut segs: Vec<String> = at.split("::").map(str::to_string).collect();
            segs.pop();
            segs
        }
        other if CRATE_DIRS.iter().any(|(c, _)| *c == other) => vec![other.to_string()],
        other => {
            let mut segs: Vec<String> = at.split("::").map(str::to_string).collect();
            segs.push(other.to_string());
            segs
        }
    };
    for seg in rest {
        if seg == "super" {
            base.pop();
        } else {
            base.push(seg.clone());
        }
    }
    (!base.is_empty()).then(|| base.join("::"))
}

/// The first module of `private` on the way to `path` (`path` itself
/// included): `azul_layout::cpurender::named` for
/// `azul_layout::cpurender::named::NotNamed`.
fn first_private_module(path: &str, private: &std::collections::BTreeSet<String>) -> Option<String> {
    let segs: Vec<&str> = path.split("::").collect();
    (2..=segs.len())
        .map(|i| segs[..i].join("::"))
        .find(|p| private.contains(p))
}

/// The path `module::name` is reached by from another crate: itself when no
/// module on the way is private, else the path of a `pub use` that
/// re-exports it - a glob or the name of its module, or a glob / the name of
/// a (public) module on the way - followed up the tree. `None` when nothing
/// re-exports it.
fn public_path(
    module: &str,
    name: &str,
    private: &std::collections::BTreeSet<String>,
    reexports: &[Reexport],
    depth: usize,
) -> Option<String> {
    if first_private_module(module, private).is_none() {
        return Some(format!("{module}::{name}"));
    }
    if depth > 8 {
        return None;
    }
    reexports.iter().find_map(|r| {
        // `module::name` as a path relative to `r.at`
        let relative = if r.from == module {
            match &r.item {
                None => name.to_string(),
                Some(item) if item == name => name.to_string(),
                Some(_) => return None,
            }
        } else {
            let rest = module.strip_prefix(r.from.as_str())?.strip_prefix("::")?;
            let (first, tail) = match rest.split_once("::") {
                Some((first, tail)) => (first, Some(tail)),
                None => (rest, None),
            };
            // a private child module is not re-exported (neither by a glob
            // nor by name)
            if private.contains(&format!("{}::{first}", r.from)) {
                return None;
            }
            if r.item.as_deref().is_some_and(|item| item != first) {
                return None;
            }
            match tail {
                Some(tail) => format!("{first}::{tail}::{name}"),
                None => format!("{first}::{name}"),
            }
        };
        let full = format!("{}::{relative}", r.at);
        let (m, n) = full.rsplit_once("::")?;
        public_path(m, n, private, reexports, depth + 1)
    })
}

/// Parse a file to extract impl blocks for cross-file method attachment.
/// This extracts both inherent impl blocks (`impl Type`) and trait impl blocks
/// (`impl Trait for Type`) where we want to expose the trait methods as type methods.
fn parse_file_for_cross_file_methods(
    file_path: &Path,
) -> Result<BTreeMap<String, Vec<MethodDef>>, String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;

    let mut syntax_tree: File = syn::parse_file(&content)
        .map_err(|e| format!("Failed to parse {}: {}", file_path.display(), e))?;
    expand_local_item_macros(&mut syntax_tree);

    let mut all_methods: BTreeMap<String, Vec<MethodDef>> = BTreeMap::new();

    // Extract inherent methods from `impl Type { ... }` blocks
    let inherent_methods = extract_inherent_methods_from_items(&syntax_tree.items);
    for (type_name, methods) in inherent_methods {
        all_methods.entry(type_name).or_default().extend(methods);
    }

    // Extract methods from trait implementations `impl Trait for Type { ... }`
    // This allows wrapper traits to expose methods on the original type
    let trait_impl_methods = extract_trait_impl_methods_from_items(&syntax_tree.items);
    for (type_name, methods) in trait_impl_methods {
        all_methods.entry(type_name).or_default().extend(methods);
    }

    Ok(all_methods)
}

/// Extract methods from `impl Trait for Type` blocks.
/// This allows traits that wrap free functions to expose them as methods on the type.
fn extract_trait_impl_methods_from_items(items: &[Item]) -> BTreeMap<String, Vec<MethodDef>> {
    let mut methods_map: BTreeMap<String, Vec<MethodDef>> = BTreeMap::new();

    for item in items {
        if let Item::Impl(impl_item) = item {
            // Only interested in trait impls (has a trait)
            if impl_item.trait_.is_none() {
                continue;
            }

            // Get the type name that this impl block is for
            let type_name = if let syn::Type::Path(type_path) = impl_item.self_ty.as_ref() {
                type_path
                    .path
                    .segments
                    .last()
                    .map(|seg| seg.ident.to_string())
                    .unwrap_or_default()
            } else {
                continue;
            };

            if type_name.is_empty() {
                continue;
            }

            // The trait's name, so a standard trait's methods (`default`,
            // `clone`, `drop`, ...) are known for what they are.
            let trait_name = impl_item
                .trait_
                .as_ref()
                .and_then(|(_, path, _)| path.segments.last().map(|s| s.ident.to_string()));

            // Extract methods from this impl block
            // Trait impl methods are always public (via the trait)
            for impl_item_fn in &impl_item.items {
                if let syn::ImplItem::Fn(method) = impl_item_fn {
                    if let Some(mut method_def) = extract_method_def(method, &type_name) {
                        // Trait impl methods are implicitly public
                        method_def.is_public = true;
                        method_def.from_trait = trait_name.clone();
                        methods_map
                            .entry(type_name.clone())
                            .or_default()
                            .push(method_def);
                    }
                }
            }
        }

        // Recursively handle inline modules
        if let Item::Mod(m) = item {
            if let Some((_, nested_items)) = &m.content {
                let nested_methods = extract_trait_impl_methods_from_items(nested_items);
                for (type_name, methods) in nested_methods {
                    methods_map.entry(type_name).or_default().extend(methods);
                }
            }
        }
    }

    methods_map
}

/// Extract types from a list of items (used for both top-level and nested modules)
fn extract_types_from_items(
    crate_name: &str,
    module_path: &str,
    file_path: &Path,
    items: &[Item],
) -> Vec<TypeDefinition> {
    let mut types = Vec::new();

    // First pass: collect type definitions
    for item in items {
        match item {
            Item::Use(_) => continue,

            Item::Struct(s) if !is_module_private(&s.vis) => {
                if let Some(typedef) = extract_struct(crate_name, module_path, file_path, s) {
                    types.push(typedef);
                }
            }

            Item::Enum(e) if !is_module_private(&e.vis) => {
                if let Some(typedef) = extract_enum(crate_name, module_path, file_path, e) {
                    types.push(typedef);
                }
            }

            Item::Type(t) => {
                if let Some(typedef) = extract_type_alias(crate_name, module_path, file_path, t) {
                    types.push(typedef);
                }
            }

            Item::Macro(m) => {
                let generated =
                    extract_macro_generated_types(crate_name, module_path, file_path, m);
                types.extend(generated);
            }

            Item::Mod(m) => {
                // Skip wasm32-only stub modules (see the sibling handler above).
                if is_wasm32_only(&m.attrs) {
                    continue;
                }
                // Recursively handle nested modules
                if let Some((_, nested_items)) = &m.content {
                    let mod_name = m.ident.to_string();
                    let nested_module_path = if module_path.is_empty() {
                        mod_name.clone()
                    } else {
                        format!("{}::{}", module_path, mod_name)
                    };
                    let nested_types = extract_types_from_items(
                        crate_name,
                        &nested_module_path,
                        file_path,
                        nested_items,
                    );
                    types.extend(nested_types);
                }
            }

            _ => {}
        }
    }

    // Second pass: extract custom trait implementations and attach to types
    let custom_impls_map = extract_custom_impls_from_items(items);
    for typedef in &mut types {
        if let Some(impls) = custom_impls_map.get(&typedef.type_name) {
            attach_custom_impls(typedef, impls.clone());
        }
    }

    // Third pass: extract inherent methods and attach to types
    let methods_map = extract_inherent_methods_from_items(items);
    for typedef in &mut types {
        if let Some(methods) = methods_map.get(&typedef.type_name) {
            attach_methods(typedef, methods.clone());
        }
    }

    types
}

/// Infer the module path from file path
fn infer_module_path(crate_name: &str, file_path: &Path) -> String {
    // Extract path components after "src/"
    let path_str = file_path.to_string_lossy();

    let after_src = if let Some(idx) = path_str.find("/src/") {
        &path_str[idx + 5..]
    } else {
        return String::new();
    };

    // Remove .rs extension and handle mod.rs/lib.rs
    let without_ext = after_src.trim_end_matches(".rs");

    if without_ext == "lib"
        || without_ext == "mod"
        || without_ext.ends_with("/mod")
        || without_ext.ends_with("/lib")
    {
        // lib.rs or mod.rs - module path is the parent directory
        let parent = without_ext.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
        parent.replace('/', "::")
    } else {
        without_ext.replace('/', "::")
    }
}

/// Build full path from crate name, module path, and type name
fn build_full_path(crate_name: &str, module_path: &str, type_name: &str) -> String {
    if module_path.is_empty() {
        format!("{}::{}", crate_name, type_name)
    } else {
        format!("{}::{}::{}", crate_name, module_path, type_name)
    }
}

// type extraction
/// Strip the "Az" prefix from a type name if present.
/// For example, `AzInstantPtr` -> `InstantPtr`, `String` -> `String`
/// This is used because source code uses "Az" prefixes for disambiguation,
/// but api.json should store types WITHOUT the prefix (prefix is added later by codegen).
fn strip_az_prefix(type_name: &str) -> String {
    // Don't strip "Az" if the remaining name would be too short or empty
    if type_name.starts_with("Az") && type_name.len() > 2 {
        // Check that the character after "Az" is uppercase (to avoid stripping "Azure" etc.)
        let after_az = &type_name[2..];
        if after_az
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase())
            .unwrap_or(false)
        {
            return after_az.to_string();
        }
    }
    type_name.to_string()
}

/// Extract just the type name from a syn::Type, without the full path.
/// For example, `crate::props::basic::color::ColorU` -> `ColorU`
/// For generic types like `Option<T>`, returns `Option<T>`.
/// NOTE: This also strips the "Az" prefix from type names.
fn extract_type_name(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(type_path) => {
            // Get the last segment of the path (the actual type name)
            if let Some(segment) = type_path.path.segments.last() {
                let ident = strip_az_prefix(&segment.ident.to_string());
                // Check if there are generic arguments
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    // Recursively extract type names for generic arguments
                    let generic_args: Vec<String> = args
                        .args
                        .iter()
                        .filter_map(|arg| match arg {
                            syn::GenericArgument::Type(inner_ty) => {
                                Some(extract_type_name(inner_ty))
                            }
                            _ => None,
                        })
                        .collect();
                    if generic_args.is_empty() {
                        ident
                    } else {
                        format!("{}<{}>", ident, generic_args.join(", "))
                    }
                } else {
                    ident
                }
            } else {
                // Fallback: no segments, use token stream
                clean_type_string(&ty.to_token_stream().to_string())
            }
        }
        syn::Type::Reference(ref_type) => {
            // &T or &mut T
            let inner = extract_type_name(&ref_type.elem);
            if ref_type.mutability.is_some() {
                format!("&mut {}", inner)
            } else {
                format!("&{}", inner)
            }
        }
        syn::Type::Ptr(ptr_type) => {
            // *const T or *mut T
            let inner = extract_type_name(&ptr_type.elem);
            if ptr_type.mutability.is_some() {
                format!("*mut {}", inner)
            } else {
                format!("*const {}", inner)
            }
        }
        syn::Type::Slice(slice_type) => {
            format!("[{}]", extract_type_name(&slice_type.elem))
        }
        syn::Type::Array(array_type) => {
            let elem = extract_type_name(&array_type.elem);
            let len = array_type.len.to_token_stream().to_string();
            format!("[{}; {}]", elem, len)
        }
        syn::Type::Tuple(tuple_type) => {
            let elems: Vec<String> = tuple_type.elems.iter().map(extract_type_name).collect();
            format!("({})", elems.join(", "))
        }
        syn::Type::ImplTrait(impl_trait) => {
            // Handle `impl Into<T>` - extract T as the actual type for C API
            // For other impl Trait bounds, we skip since they can't be exported to C
            for bound in &impl_trait.bounds {
                if let syn::TypeParamBound::Trait(trait_bound) = bound {
                    if let Some(segment) = trait_bound.path.segments.last() {
                        if segment.ident == "Into" {
                            // Extract the inner type from Into<T>
                            if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                                for arg in &args.args {
                                    if let syn::GenericArgument::Type(inner_ty) = arg {
                                        return extract_type_name(inner_ty);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Fallback: return the full impl trait string (will be filtered out later)
            clean_type_string(&ty.to_token_stream().to_string())
        }
        _ => {
            // Fallback for other types
            clean_type_string(&ty.to_token_stream().to_string())
        }
    }
}

/// Extract field type and ref_kind for struct fields.
/// For pointer types (*const T, *mut T), Box<T>, and Option<Box<T>>, the type is replaced with
/// "c_void" and the ref_kind is set appropriately. This prevents recursion through opaque pointers.
fn extract_struct_field_type(ty: &syn::Type) -> (String, RefKind) {
    // `ManuallyDrop<T>` is layout-transparent: it suppresses T's destructor and
    // changes nothing about what the field IS, so it is peeled before the
    // pointer rules below. Without this, `ManuallyDrop<Box<X>>` fell through to
    // the fallback as a raw type string, `autofix` proposed it into api.json,
    // and every non-C binding then carried `ManuallyDrop<Box<X>>.by_value` as
    // a field type (`azul.rb` failed to parse on exactly that). api.json
    // describes these handles as `c_void` mut-pointers — which is what the
    // `Box` rule yields once the wrapper is gone.
    if let Some(inner) = manually_drop_inner(ty) {
        return extract_struct_field_type(inner);
    }
    match ty {
        syn::Type::Ptr(ptr_type) => {
            // *const T or *mut T -> type becomes "c_void", ref_kind is ConstPtr/MutPtr
            if ptr_type.mutability.is_some() {
                ("c_void".to_string(), RefKind::MutPtr)
            } else {
                ("c_void".to_string(), RefKind::ConstPtr)
            }
        }
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                // Check if it's Box<T>
                if segment.ident == "Box" {
                    // Box<T> -> type becomes "c_void", ref_kind is MutPtr (Box is an owned pointer)
                    // We use MutPtr instead of Boxed because Box<c_void> causes UB in Rust
                    // (Box must be non-null but c_void is an opaque type)
                    return ("c_void".to_string(), RefKind::MutPtr);
                }
                // Check if it's Option<Box<T>>
                if segment.ident == "Option" {
                    if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                        if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                            // Check if the inner type is Box<T>
                            if let syn::Type::Path(inner_path) = inner_ty {
                                if let Some(inner_segment) = inner_path.path.segments.last() {
                                    if inner_segment.ident == "Box" {
                                        // Option<Box<T>> -> type becomes "c_void", ref_kind is
                                        // MutPtr
                                        // (nullable pointer, same as Option<Box<T>> in FFI)
                                        return ("c_void".to_string(), RefKind::MutPtr);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            // Regular type - extract just the type name (not the full path)
            (extract_type_name(ty), RefKind::Value)
        }
        _ => {
            // Any other type - extract just the type name
            (extract_type_name(ty), RefKind::Value)
        }
    }
}

/// The `T` of a `ManuallyDrop<T>` (`core::mem::ManuallyDrop` or bare), else `None`.
fn manually_drop_inner(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(type_path) = ty else {
        return None;
    };
    let segment = type_path.path.segments.last()?;
    if segment.ident != "ManuallyDrop" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return None;
    };
    match args.args.first()? {
        syn::GenericArgument::Type(inner) => Some(inner),
        _ => None,
    }
}

/// Extract a struct definition
fn extract_struct(
    crate_name: &str,
    module_path: &str,
    file_path: &Path,
    s: &syn::ItemStruct,
) -> Option<TypeDefinition> {
    let type_name = s.ident.to_string();
    let full_path = build_full_path(crate_name, module_path, &type_name);

    let generic_params: Vec<String> = s
        .generics
        .type_params()
        .map(|tp| tp.ident.to_string())
        .collect();

    // Check if this is a tuple struct (unnamed fields) vs named struct
    let is_tuple_struct = matches!(s.fields, syn::Fields::Unnamed(_));

    let mut fields = IndexMap::new();
    for (idx, field) in s.fields.iter().enumerate() {
        let field_name = if let Some(ident) = field.ident.as_ref() {
            // Named field
            ident.to_string()
        } else {
            // Tuple struct field - use index as name (e.g., "0", "1", etc.)
            // For FFI compatibility, we'll name it "inner" for single-field tuple structs
            if s.fields.len() == 1 {
                "inner".to_string()
            } else {
                format!("field_{}", idx)
            }
        };
        let (field_ty, ref_kind) = extract_struct_field_type(&field.ty);
        fields.insert(
            field_name.clone(),
            FieldDef {
                name: field_name,
                ty: field_ty,
                ref_kind,
                doc: extract_doc_comments(&field.attrs),
            },
        );
    }

    let (repr, repr_attr_count) = extract_repr_attr(&s.attrs);
    let derives = extract_derives(&s.attrs);

    Some(TypeDefinition {
        full_path,
        type_name,
        file_path: file_path.to_path_buf(),
        module_path: module_path.to_string(),
        crate_name: crate_name.to_string(),
        kind: TypeDefKind::Struct {
            fields,
            repr,
            repr_attr_count,
            generic_params,
            derives,
            custom_impls: vec![], // Will be filled in by second pass
            is_tuple_struct,
        },
        source_code: s.to_token_stream().to_string(),
        methods: Vec::new(), // Will be filled in by second pass
    })
}

/// Extract an enum definition
fn extract_enum(
    crate_name: &str,
    module_path: &str,
    file_path: &Path,
    e: &syn::ItemEnum,
) -> Option<TypeDefinition> {
    let type_name = e.ident.to_string();
    let full_path = build_full_path(crate_name, module_path, &type_name);

    let generic_params: Vec<String> = e
        .generics
        .type_params()
        .map(|tp| tp.ident.to_string())
        .collect();

    let mut variants = IndexMap::new();
    for variant in &e.variants {
        let variant_name = variant.ident.to_string();
        let variant_ty = if variant.fields.is_empty() {
            None
        } else {
            let fields_str = variant
                .fields
                .iter()
                .map(|f| extract_type_name(&f.ty))
                .collect::<Vec<_>>()
                .join(", ");
            Some(fields_str)
        };

        variants.insert(
            variant_name.clone(),
            VariantDef {
                name: variant_name,
                ty: variant_ty,
                doc: extract_doc_comments(&variant.attrs),
            },
        );
    }

    let (repr, repr_attr_count) = extract_repr_attr(&e.attrs);
    let derives = extract_derives(&e.attrs);

    Some(TypeDefinition {
        full_path,
        type_name,
        file_path: file_path.to_path_buf(),
        module_path: module_path.to_string(),
        crate_name: crate_name.to_string(),
        kind: TypeDefKind::Enum {
            variants,
            repr,
            repr_attr_count,
            generic_params,
            derives,
            custom_impls: vec![], // Will be filled in by second pass
        },
        source_code: e.to_token_stream().to_string(),
        methods: Vec::new(), // Will be filled in by second pass
    })
}

/// Extract reference kind and base type from a syn::Type
/// Returns (base_type_string, RefKind)
///
/// For raw pointers (*mut T, *const T), we return the FULL type string and RefKind::Value
/// because pointers are passed by value in C FFI. This matches api.json conventions.
///
/// For references (&T, &mut T), we return the base type and RefKind::Ref/RefMut.
fn extract_ref_kind_from_syn_type(ty: &syn::Type) -> (String, RefKind) {
    match ty {
        syn::Type::Reference(ref_type) => {
            // &T or &mut T - extract base type, use Ref/RefMut
            let inner_type = extract_type_name(&ref_type.elem);
            if ref_type.mutability.is_some() {
                (inner_type, RefKind::RefMut)
            } else {
                (inner_type, RefKind::Ref)
            }
        }
        syn::Type::Ptr(ptr_type) => {
            // *const T or *mut T - keep full pointer type, use Value
            // Raw pointers are passed by value in C FFI
            let inner_type = extract_type_name(&ptr_type.elem);
            let full_type = if ptr_type.mutability.is_some() {
                format!("*mut {}", inner_type)
            } else {
                format!("*const {}", inner_type)
            };
            (full_type, RefKind::Value)
        }
        _ => {
            // Any other type is passed by value
            (extract_type_name(ty), RefKind::Value)
        }
    }
}

/// Extract a type alias
fn extract_type_alias(
    crate_name: &str,
    module_path: &str,
    file_path: &Path,
    t: &syn::ItemType,
) -> Option<TypeDefinition> {
    let type_name = t.ident.to_string();
    let full_path = build_full_path(crate_name, module_path, &type_name);

    // Check if this is a callback typedef (extern "C" fn)
    if let syn::Type::BareFn(bare_fn) = &*t.ty {
        let args: Vec<CallbackArg> = bare_fn
            .inputs
            .iter()
            .map(|arg| {
                let (base_type, ref_kind) = extract_ref_kind_from_syn_type(&arg.ty);
                CallbackArg {
                    name: arg.name.as_ref().map(|(ident, _)| ident.to_string()),
                    ty: base_type,
                    ref_kind,
                }
            })
            .collect();

        let returns = match &bare_fn.output {
            syn::ReturnType::Default => None,
            syn::ReturnType::Type(_, ty) => {
                let ret_str = extract_type_name(ty);
                if ret_str.is_empty() || ret_str == "()" {
                    None
                } else {
                    Some(ret_str)
                }
            }
        };

        return Some(TypeDefinition {
            full_path,
            type_name,
            file_path: file_path.to_path_buf(),
            module_path: module_path.to_string(),
            crate_name: crate_name.to_string(),
            kind: TypeDefKind::CallbackTypedef { args, returns },
            source_code: t.to_token_stream().to_string(),
            methods: Vec::new(),
        });
    }

    // Try to extract generic arguments from the type
    let (generic_base, generic_args) = extract_generic_args_from_type(&t.ty);

    // Extract just the type name for the target, not the full path
    let target = extract_type_name(&t.ty);

    Some(TypeDefinition {
        full_path,
        type_name,
        file_path: file_path.to_path_buf(),
        module_path: module_path.to_string(),
        crate_name: crate_name.to_string(),
        kind: TypeDefKind::TypeAlias {
            target,
            generic_base,
            generic_args,
        },
        source_code: t.to_token_stream().to_string(),
        methods: Vec::new(),
    })
}

/// Extract generic base type and arguments from a syn::Type
/// Returns (Some(base_type), [args]) for generic types, (None, []) for non-generic types
fn extract_generic_args_from_type(ty: &syn::Type) -> (Option<String>, Vec<String>) {
    match ty {
        syn::Type::Path(type_path) => {
            // Get the last segment (the actual type name)
            if let Some(segment) = type_path.path.segments.last() {
                let base_type = segment.ident.to_string();

                // Check for generic arguments
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    let generic_args: Vec<String> = args
                        .args
                        .iter()
                        .filter_map(|arg| {
                            match arg {
                                syn::GenericArgument::Type(ty) => {
                                    // Recursively get the type name
                                    Some(extract_type_name(ty))
                                }
                                syn::GenericArgument::Lifetime(_) => None,
                                _ => None,
                            }
                        })
                        .collect();

                    if !generic_args.is_empty() {
                        return (Some(base_type), generic_args);
                    }
                }
            }
            (None, vec![])
        }
        _ => (None, vec![]),
    }
}

/// Extract derive traits from macro arguments like [Debug, Copy, Clone, ...]
/// Uses syn to properly parse the token stream
fn extract_derives_from_tokens(tokens: &proc_macro2::TokenStream) -> Vec<String> {
    use proc_macro2::TokenTree;

    let mut derives = Vec::new();
    let mut in_bracket = false;

    for token in tokens.clone().into_iter() {
        match token {
            TokenTree::Group(group) => {
                // Check if this is a bracket group [...]
                if group.delimiter() == proc_macro2::Delimiter::Bracket {
                    // Parse the contents as derive names
                    for inner in group.stream().into_iter() {
                        if let TokenTree::Ident(ident) = inner {
                            derives.push(ident.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    derives
}

/// Parse macro arguments from a token stream
/// Returns (base_type, generated_type, optional_wrapper_type, derives)
fn parse_impl_option_args(
    tokens: &proc_macro2::TokenStream,
) -> Option<(String, String, Option<String>, Vec<String>)> {
    use proc_macro2::TokenTree;

    let mut idents = Vec::new();
    let mut derives = Vec::new();
    let mut has_copy_false = false;
    let mut has_clone_false = false;

    let token_vec: Vec<_> = tokens.clone().into_iter().collect();
    let mut i = 0;

    while i < token_vec.len() {
        match &token_vec[i] {
            TokenTree::Ident(ident) => {
                let ident_str = ident.to_string();
                // Check for "copy = false" or "clone = false" patterns
                if ident_str == "copy" || ident_str == "clone" {
                    // Skip "copy/clone = false" pattern
                    if i + 2 < token_vec.len() {
                        if let TokenTree::Punct(p) = &token_vec[i + 1] {
                            if p.as_char() == '=' {
                                if ident_str == "copy" {
                                    has_copy_false = true;
                                }
                                if ident_str == "clone" {
                                    has_clone_false = true;
                                }
                                i += 3; // Skip "copy = false"
                                continue;
                            }
                        }
                    }
                }
                idents.push(ident_str);
            }
            TokenTree::Group(group) => {
                if group.delimiter() == proc_macro2::Delimiter::Bracket {
                    // This is the derives list [Debug, Copy, Clone, ...]
                    for inner in group.stream().into_iter() {
                        if let TokenTree::Ident(ident) = inner {
                            derives.push(ident.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if idents.len() < 2 {
        return None;
    }

    let base_type = idents[0].clone();
    let option_type = idents[1].clone();
    let wrapper_type = if idents.len() >= 3 {
        Some(idents[2].clone())
    } else {
        None
    };

    Some((base_type, option_type, wrapper_type, derives))
}

/// Extract types generated by macros like impl_vec!, impl_option!
fn extract_macro_generated_types(
    crate_name: &str,
    module_path: &str,
    file_path: &Path,
    m: &syn::ItemMacro,
) -> Vec<TypeDefinition> {
    let mut types = Vec::new();

    // Get macro name
    let macro_name = m
        .mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();

    let tokens = m.mac.tokens.to_string();
    let args: Vec<&str> = tokens.split(',').map(|s| s.trim()).collect();

    match macro_name.as_str() {
        "impl_vec" => {
            // impl_vec!(BaseType, VecType, DestructorType, DestructorTypeType, SliceType)
            // Generates:
            //   - VecType (struct with ptr, len, cap, destructor fields)
            //   - DestructorType (enum with DefaultRust, NoDestructor, External variants)
            //   - DestructorTypeType (callback_typedef for extern "C" fn(*mut VecType))
            //   - SliceType (struct with ptr, len for C-compatible slice access)
            // NOTE: impl_vec_clone and impl_vec_debug only add trait impls, not new types
            if args.len() >= 5 {
                let base_type = args[0].to_string();
                let vec_type = args[1].to_string();
                let destructor_type = args[2].to_string();
                let destructor_type_type = args[3].to_string();
                let slice_type = args[4].to_string();

                // VecType (struct)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &vec_type),
                    type_name: vec_type.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name.clone(),
                        base_type: base_type.clone(),
                        kind: MacroGeneratedKind::Vec,
                        derives: Vec::new(),
                        implemented_traits: Vec::new(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });

                // DestructorType (enum)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &destructor_type),
                    type_name: destructor_type.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name.clone(),
                        base_type: base_type.clone(),
                        kind: MacroGeneratedKind::VecDestructor,
                        derives: Vec::new(),
                        implemented_traits: Vec::new(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });

                // DestructorTypeType (callback_typedef for the External variant's fn pointer)
                // This is: extern "C" fn(*mut VecType)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &destructor_type_type),
                    type_name: destructor_type_type,
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name.clone(),
                        base_type: vec_type.clone(), // The vec type is referenced in the callback
                        kind: MacroGeneratedKind::VecDestructorType,
                        derives: Vec::new(),
                        implemented_traits: Vec::new(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });

                // SliceType (C-compatible slice struct with ptr and len)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &slice_type),
                    type_name: slice_type,
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name,
                        base_type: base_type.clone(),
                        kind: MacroGeneratedKind::VecSlice,
                        derives: Vec::new(),
                        implemented_traits: Vec::new(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "impl_option" => {
            // impl_option! can have different forms:
            // impl_option!(BaseType, OptionType, [derives...])
            // impl_option!(BaseType, OptionType, EnumWrapperType, [derives...])
            // impl_option!(BaseType, OptionType, copy = false, [derives...])
            // impl_option!(BaseType, OptionType, copy = false, clone = false, [derives...])

            // Use syn to properly parse the token stream
            if let Some((base_type, option_type, wrapper_type, derives)) =
                parse_impl_option_args(&m.mac.tokens)
            {
                // OptionType (always generated)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &option_type),
                    type_name: option_type.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name.clone(),
                        base_type: base_type.clone(),
                        kind: MacroGeneratedKind::Option,
                        derives: derives.clone(),
                        implemented_traits: Vec::new(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });

                // EnumWrapperType (only if 3rd arg is a type name, not "copy" or similar)
                if let Some(wrapper) = wrapper_type {
                    types.push(TypeDefinition {
                        full_path: build_full_path(crate_name, module_path, &wrapper),
                        type_name: wrapper,
                        file_path: file_path.to_path_buf(),
                        module_path: module_path.to_string(),
                        crate_name: crate_name.to_string(),
                        kind: TypeDefKind::MacroGenerated {
                            source_macro: macro_name,
                            base_type,
                            kind: MacroGeneratedKind::OptionEnumWrapper,
                            derives,
                            implemented_traits: Vec::new(),
                        },
                        source_code: m.to_token_stream().to_string(),
                        methods: Vec::new(),
                    });
                }
            }
        }

        "impl_result" => {
            // impl_result!(OkType, ErrType, ResultType, copy = false, [...])
            // Generates: ResultType (enum with Ok/Err variants)
            if args.len() >= 3 {
                let ok_type = args[0].to_string();
                let err_type = args[1].to_string();
                let result_type = args[2].to_string();

                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &result_type),
                    type_name: result_type,
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name,
                        base_type: format!("Result<{}, {}>", ok_type, err_type),
                        kind: MacroGeneratedKind::Result,
                        derives: Vec::new(),
                        implemented_traits: Vec::new(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "impl_widget_callback" => {
            // impl_widget_callback! is the 4-parameter version for widget callbacks
            // impl_widget_callback!(CallbackWrapper, OptionCallbackWrapper, CallbackValue,
            // CallbackType) Generates: CallbackWrapper (struct), OptionCallbackWrapper
            // (option), CallbackValue (struct)
            //
            // Note: impl_callback!(CallbackValue, CallbackType) is handled separately in
            // extract_custom_impls_from_items() - it only adds trait impls, not new types.

            if args.len() >= 4 {
                let callback_wrapper = args[0].to_string();
                let option_callback_wrapper = args[1].to_string();
                let callback_value = args[2].to_string();
                let callback_type = args[3].to_string(); // The extern "C" fn type
                let callback_traits = vec![
                    "Clone".to_string(),
                    "Debug".to_string(),
                    "Hash".to_string(),
                    "PartialEq".to_string(),
                    "Eq".to_string(),
                    "PartialOrd".to_string(),
                    "Ord".to_string(),
                ];

                // CallbackWrapper struct - has fields: data (RefAny), callback (CallbackValue)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &callback_wrapper),
                    type_name: callback_wrapper.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name.clone(),
                        base_type: callback_value.clone(), // References CallbackValue
                        kind: MacroGeneratedKind::CallbackWrapper,
                        derives: callback_traits.clone(),
                        implemented_traits: callback_traits.clone(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });

                // OptionCallbackWrapper (via nested impl_option!)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &option_callback_wrapper),
                    type_name: option_callback_wrapper,
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name.clone(),
                        base_type: callback_wrapper.clone(),
                        kind: MacroGeneratedKind::Option,
                        derives: callback_traits.clone(),
                        implemented_traits: callback_traits.clone(),
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });

                // CallbackValue struct - has field: cb (CallbackType)
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &callback_value),
                    type_name: callback_value.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::MacroGenerated {
                        source_macro: macro_name,
                        base_type: callback_type, // References CallbackType (the extern "C" fn)
                        kind: MacroGeneratedKind::CallbackValue,
                        derives: callback_traits.clone(),
                        implemented_traits: callback_traits,
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        // // css property macros - generate wrapper structs around pixelvalue/etc.
        "define_dimension_property" => {
            // define_dimension_property!(LayoutMaxWidth, || Self { inner:
            // PixelValue::px(core::f32::MAX) }); Generates a struct with #[repr(C)] and
            // inner: PixelValue
            if !args.is_empty() {
                let struct_name = args[0].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &struct_name),
                    type_name: struct_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Struct {
                        fields: {
                            let mut fields = IndexMap::new();
                            fields.insert(
                                "inner".to_string(),
                                FieldDef {
                                    name: "inner".to_string(),
                                    ty: "PixelValue".to_string(),
                                    ref_kind: RefKind::Value,
                                    doc: Vec::new(),
                                },
                            );
                            fields
                        },
                        repr: Some("C".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Copy".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                            "Eq".to_string(),
                            "PartialOrd".to_string(),
                            "Ord".to_string(),
                            "Hash".to_string(),
                        ],
                        custom_impls: vec![],
                        is_tuple_struct: false,
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_position_property" => {
            // define_position_property!(LayoutTop);
            // Generates a struct with #[repr(C)] and inner: PixelValue
            if !args.is_empty() {
                let struct_name = args[0].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &struct_name),
                    type_name: struct_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Struct {
                        fields: {
                            let mut fields = IndexMap::new();
                            fields.insert(
                                "inner".to_string(),
                                FieldDef {
                                    name: "inner".to_string(),
                                    ty: "PixelValue".to_string(),
                                    ref_kind: RefKind::Value,
                                    doc: Vec::new(),
                                },
                            );
                            fields
                        },
                        repr: Some("C".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Default".to_string(),
                            "Copy".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                            "Eq".to_string(),
                            "PartialOrd".to_string(),
                            "Ord".to_string(),
                            "Hash".to_string(),
                        ],
                        custom_impls: vec![],
                        is_tuple_struct: false,
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_border_side_property" => {
            // define_border_side_property!(StyleBorderTopStyle, BorderStyle, BorderStyle::None);
            // define_border_side_property!(StyleBorderTopColor, ColorU);
            // define_border_side_property!(LayoutBorderTopWidth, PixelValue,
            // MEDIUM_BORDER_THICKNESS);
            if !args.is_empty() {
                let struct_name = args[0].to_string();
                let inner_type = if args.len() > 1 {
                    args[1].to_string()
                } else {
                    "PixelValue".to_string()
                };

                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &struct_name),
                    type_name: struct_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Struct {
                        fields: {
                            let mut fields = IndexMap::new();
                            fields.insert(
                                "inner".to_string(),
                                FieldDef {
                                    name: "inner".to_string(),
                                    ty: inner_type,
                                    ref_kind: RefKind::Value,
                                    doc: Vec::new(),
                                },
                            );
                            fields
                        },
                        repr: Some("C".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Copy".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                            "Eq".to_string(),
                            "PartialOrd".to_string(),
                            "Ord".to_string(),
                            "Hash".to_string(),
                        ],
                        custom_impls: vec![],
                        is_tuple_struct: false,
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_border_radius_property" => {
            // define_border_radius_property!(StyleBorderTopLeftRadius);
            // Generates a struct with inner: PixelValue
            if !args.is_empty() {
                let struct_name = args[0].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &struct_name),
                    type_name: struct_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Struct {
                        fields: {
                            let mut fields = IndexMap::new();
                            fields.insert(
                                "inner".to_string(),
                                FieldDef {
                                    name: "inner".to_string(),
                                    ty: "PixelValue".to_string(),
                                    ref_kind: RefKind::Value,
                                    doc: Vec::new(),
                                },
                            );
                            fields
                        },
                        repr: Some("C".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Default".to_string(),
                            "Copy".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                            "Eq".to_string(),
                            "PartialOrd".to_string(),
                            "Ord".to_string(),
                            "Hash".to_string(),
                        ],
                        custom_impls: vec![],
                        is_tuple_struct: false,
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        // CSS parser error macros - these generate *Owned error enums with repr(C, u8)
        "define_simple_column_parser" | "define_flow_parser" => {
            // define_simple_column_parser!(fn, struct, error, error_owned, prop, vals...)
            // define_flow_parser!(fn, struct, error, error_owned, prop)
            // Both generate: #[repr(C, u8)] enum ErrorOwned { InvalidValue(AzString) }
            if args.len() >= 4 {
                let error_owned_name = args[3].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &error_owned_name),
                    type_name: error_owned_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "InvalidValue".to_string(),
                                VariantDef {
                                    name: "InvalidValue".to_string(),
                                    ty: Some("AzString".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_pixel_dimension_parser" => {
            // define_pixel_dimension_parser!(fn, struct, error, error_owned)
            // Generates: #[repr(C, u8)] enum ErrorOwned { PixelValue(CssPixelValueParseErrorOwned)
            // }
            if args.len() >= 4 {
                let error_owned_name = args[3].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &error_owned_name),
                    type_name: error_owned_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "PixelValue".to_string(),
                                VariantDef {
                                    name: "PixelValue".to_string(),
                                    ty: Some("CssPixelValueParseErrorOwned".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_border_radius_parse_error" => {
            // define_border_radius_parse_error!(Error, ErrorOwned)
            // Generates: #[repr(C, u8)] enum ErrorOwned { PixelValue(CssPixelValueParseErrorOwned)
            // }
            if args.len() >= 2 {
                let error_owned_name = args[1].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &error_owned_name),
                    type_name: error_owned_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "PixelValue".to_string(),
                                VariantDef {
                                    name: "PixelValue".to_string(),
                                    ty: Some("CssPixelValueParseErrorOwned".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_offset_parse_error" => {
            // define_offset_parse_error!(Struct, Error, ErrorOwned, parse_fn)
            // Generates: #[repr(C, u8)] enum ErrorOwned { PixelValue(CssPixelValueParseErrorOwned)
            // }
            if args.len() >= 3 {
                let error_owned_name = args[2].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &error_owned_name),
                    type_name: error_owned_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "PixelValue".to_string(),
                                VariantDef {
                                    name: "PixelValue".to_string(),
                                    ty: Some("CssPixelValueParseErrorOwned".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_widow_orphan_parser" => {
            // define_widow_orphan_parser!(fn, struct, error, error_owned, prop)
            // Generates: #[repr(C, u8)] enum ErrorOwned {
            //   ParseInt(ParseIntErrorWithInput),
            //   NegativeValue(AzString),
            // }
            if args.len() >= 4 {
                let error_owned_name = args[3].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &error_owned_name),
                    type_name: error_owned_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "ParseInt".to_string(),
                                VariantDef {
                                    name: "ParseInt".to_string(),
                                    ty: Some("ParseIntErrorWithInput".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "NegativeValue".to_string(),
                                VariantDef {
                                    name: "NegativeValue".to_string(),
                                    ty: Some("AzString".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_sizing_enum" => {
            // define_sizing_enum!(LayoutWidth) / define_sizing_enum!(LayoutHeight)
            // Generates: #[repr(C, u8)] enum $name {
            //   Auto, Px(PixelValue), MinContent, MaxContent,
            //   FitContent(PixelValue), Calc(CalcAstItemVec),
            // }
            if !args.is_empty() {
                let enum_name = args[0].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &enum_name),
                    type_name: enum_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "Auto".to_string(),
                                VariantDef {
                                    name: "Auto".to_string(),
                                    ty: None,
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "Px".to_string(),
                                VariantDef {
                                    name: "Px".to_string(),
                                    ty: Some("PixelValue".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "MinContent".to_string(),
                                VariantDef {
                                    name: "MinContent".to_string(),
                                    ty: None,
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "MaxContent".to_string(),
                                VariantDef {
                                    name: "MaxContent".to_string(),
                                    ty: None,
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "FitContent".to_string(),
                                VariantDef {
                                    name: "FitContent".to_string(),
                                    ty: Some("PixelValue".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "Calc".to_string(),
                                VariantDef {
                                    name: "Calc".to_string(),
                                    ty: Some("CalcAstItemVec".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                            "Eq".to_string(),
                            "PartialOrd".to_string(),
                            "Ord".to_string(),
                            "Hash".to_string(),
                            "Default".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        "define_sizing_parser" => {
            // define_sizing_parser!(fn, enum, error, error_owned, label)
            // Generates: #[repr(C, u8)] enum ErrorOwned {
            //   PixelValue(CssPixelValueParseErrorOwned),
            //   InvalidKeyword(AzString),
            // }
            if args.len() >= 4 {
                let error_owned_name = args[3].to_string();
                types.push(TypeDefinition {
                    full_path: build_full_path(crate_name, module_path, &error_owned_name),
                    type_name: error_owned_name.clone(),
                    file_path: file_path.to_path_buf(),
                    module_path: module_path.to_string(),
                    crate_name: crate_name.to_string(),
                    kind: TypeDefKind::Enum {
                        variants: {
                            let mut variants = IndexMap::new();
                            variants.insert(
                                "PixelValue".to_string(),
                                VariantDef {
                                    name: "PixelValue".to_string(),
                                    ty: Some("CssPixelValueParseErrorOwned".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants.insert(
                                "InvalidKeyword".to_string(),
                                VariantDef {
                                    name: "InvalidKeyword".to_string(),
                                    ty: Some("AzString".to_string()),
                                    doc: Vec::new(),
                                },
                            );
                            variants
                        },
                        repr: Some("C, u8".to_string()),
                        repr_attr_count: 1,
                        generic_params: vec![],
                        derives: vec![
                            "Debug".to_string(),
                            "Clone".to_string(),
                            "PartialEq".to_string(),
                        ],
                        custom_impls: vec![],
                    },
                    source_code: m.to_token_stream().to_string(),
                    methods: Vec::new(),
                });
            }
        }

        _ => {}
    }

    types
}

// helpers
/// Extract the #[repr(...)] attribute value if present.
/// Returns (merged_repr, count) where count is the number of `#[repr(...)]` attributes found.
/// If there are multiple repr attributes, their contents are merged (e.g. `#[repr(C)]` + `#[repr(C,
/// u8)]` → "C, u8"). A count > 1 indicates duplicate repr attributes, which is an error.
fn extract_repr_attr(attrs: &[syn::Attribute]) -> (Option<String>, usize) {
    let mut all_parts: Vec<String> = Vec::new();
    let mut count = 0usize;
    for attr in attrs {
        if !attr.path().is_ident("repr") {
            continue;
        }
        count += 1;
        // Parse the repr content: #[repr(C)] -> "C", #[repr(C, u8)] -> "C, u8"
        if let syn::Meta::List(list) = &attr.meta {
            let tokens = list.tokens.to_string();
            for part in tokens.split(',') {
                let trimmed = part.trim().to_string();
                if !trimmed.is_empty() && !all_parts.contains(&trimmed) {
                    all_parts.push(trimmed);
                }
            }
        }
    }
    if all_parts.is_empty() {
        (None, count)
    } else {
        (Some(all_parts.join(", ")), count)
    }
}

/// Check if a repr value indicates FFI-safe representation
fn is_ffi_safe_repr(repr: &Option<String>) -> bool {
    match repr {
        Some(r) => {
            r.contains("C")
                || r.contains("transparent")
                || r == "u8"
                || r == "u16"
                || r == "u32"
                || r == "u64"
                || r == "i8"
                || r == "i16"
                || r == "i32"
                || r == "i64"
                || r == "usize"
                || r == "isize"
        }
        None => false,
    }
}

/// Extract #[derive(...)] traits
fn extract_derives(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut derives = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if let syn::Meta::List(list) = &attr.meta {
                let tokens = list.tokens.to_string();
                for item in tokens.split(',') {
                    let item = item.trim();
                    if !item.is_empty() {
                        derives.push(item.to_string());
                    }
                }
            }
        }
    }
    derives
}

/// Extract custom trait implementations from `impl Trait for Type` blocks.
/// Returns a map from type name to list of implemented traits.
fn extract_custom_impls_from_items(items: &[Item]) -> BTreeMap<String, Vec<String>> {
    let mut custom_impls: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for item in items {
        if let Item::Impl(impl_item) = item {
            // Only interested in `impl Trait for Type`, not inherent impls
            if let Some((_, trait_path, _)) = &impl_item.trait_ {
                // Get the trait name (last segment of path)
                let trait_name = trait_path
                    .segments
                    .last()
                    .map(|seg| seg.ident.to_string())
                    .unwrap_or_default();

                // Get the type name that this trait is implemented for
                if let syn::Type::Path(type_path) = impl_item.self_ty.as_ref() {
                    let type_name = type_path
                        .path
                        .segments
                        .last()
                        .map(|seg| seg.ident.to_string())
                        .unwrap_or_default();

                    // We're interested in standard traits that affect code generation
                    let relevant_traits = [
                        "Clone",
                        "Copy",
                        "Debug",
                        "Default",
                        "Drop",
                        "PartialEq",
                        "Eq",
                        "PartialOrd",
                        "Ord",
                        "Hash",
                        "Display",
                        "From",
                        "Into",
                        "AsRef",
                        "AsMut",
                        "Deref",
                        "DerefMut",
                        "Iterator",
                        "IntoIterator",
                    ];

                    if relevant_traits.contains(&trait_name.as_str()) && !type_name.is_empty() {
                        custom_impls.entry(type_name).or_default().push(trait_name);
                    }
                }
            }
        }

        // Recursively handle inline modules
        if let Item::Mod(m) = item {
            if let Some((_, nested_items)) = &m.content {
                let nested_impls = extract_custom_impls_from_items(nested_items);
                for (type_name, traits) in nested_impls {
                    custom_impls.entry(type_name).or_default().extend(traits);
                }
            }
        }

        // Handle impl_vec_*! macros that generate trait implementations
        // These macros expand to `impl Trait for Type` but aren't visible to syn
        if let Item::Macro(m) = item {
            let macro_name = m
                .mac
                .path
                .segments
                .last()
                .map(|s| s.ident.to_string())
                .unwrap_or_default();

            // Handle impl_callback! macro (2-parameter version only)
            // impl_callback!(CallbackValue, CallbackType) - adds traits to CallbackValue
            // Note: 4-parameter version is now impl_widget_callback! and handled in
            // extract_macro_generated_types Note: Also implements From<CallbackType>
            // but that's Rust-only, not exposed to C API
            if macro_name == "impl_callback" {
                let tokens = m.mac.tokens.to_string();
                let args: Vec<&str> = tokens.split(',').map(|s| s.trim()).collect();

                // 2-parameter version adds traits to the first argument
                if args.len() == 2 {
                    let type_name = args[0].to_string();
                    if !type_name.is_empty() {
                        let callback_traits = vec![
                            "Clone",
                            "Debug",
                            "Hash",
                            "PartialEq",
                            "Eq",
                            "PartialOrd",
                            "Ord",
                        ];
                        for trait_name in callback_traits {
                            custom_impls
                                .entry(type_name.clone())
                                .or_default()
                                .push(trait_name.to_string());
                        }
                    }
                }
            }

            // Handle impl_callback_simple! macro (1-parameter version)
            // impl_callback_simple!(CallbackValue) - adds traits to CallbackValue
            // Used for simple callbacks that only have a cb field (no ctx)
            if macro_name == "impl_callback_simple" {
                let tokens = m.mac.tokens.to_string();
                let type_name = tokens.trim().to_string();
                if !type_name.is_empty() {
                    let callback_traits = vec![
                        "Clone",
                        "Debug",
                        "Hash",
                        "PartialEq",
                        "Eq",
                        "PartialOrd",
                        "Ord",
                    ];
                    for trait_name in callback_traits {
                        custom_impls
                            .entry(type_name.clone())
                            .or_default()
                            .push(trait_name.to_string());
                    }
                }
            }

            // Handle impl_widget_callback! macro (4-parameter version for widgets)
            // impl_widget_callback!(Wrapper, OptionWrapper, CallbackValue, CallbackType)
            // Adds traits to CallbackValue (generated type)
            // Note: Also implements From<CallbackType> but that's Rust-only, not exposed to C API
            if macro_name == "impl_widget_callback" {
                let tokens = m.mac.tokens.to_string();
                let args: Vec<&str> = tokens.split(',').map(|s| s.trim()).collect();

                if args.len() >= 4 {
                    let callback_value = args[2].to_string();
                    if !callback_value.is_empty() {
                        let callback_traits = vec![
                            "Clone",
                            "Debug",
                            "Hash",
                            "PartialEq",
                            "Eq",
                            "PartialOrd",
                            "Ord",
                        ];
                        for trait_name in callback_traits {
                            custom_impls
                                .entry(callback_value.clone())
                                .or_default()
                                .push(trait_name.to_string());
                        }
                    }
                }
            }

            // Map from macro name to the trait it implements
            let macro_to_trait: &[(&str, &str)] = &[
                ("impl_vec_clone", "Clone"),
                ("impl_vec_debug", "Debug"),
                ("impl_vec_partialeq", "PartialEq"),
                ("impl_vec_partialord", "PartialOrd"),
                ("impl_vec_eq", "Eq"),
                ("impl_vec_ord", "Ord"),
                ("impl_vec_hash", "Hash"),
            ];

            for (trait_macro, trait_name) in macro_to_trait {
                if macro_name == *trait_macro {
                    // Parse macro arguments: impl_vec_clone!(ElementType, VecType, Destructor)
                    let tokens = m.mac.tokens.to_string();
                    let args: Vec<&str> = tokens.split(',').map(|s| s.trim()).collect();

                    // The second argument is the Vec type name
                    if args.len() >= 2 {
                        let type_name = args[1].to_string();
                        if !type_name.is_empty() {
                            custom_impls
                                .entry(type_name)
                                .or_default()
                                .push(trait_name.to_string());
                        }
                    }
                    break;
                }
            }

            // impl_spacing_type_impls!(TypeName) generates `impl Debug for TypeName`
            // (plus inherent impls that don't affect derive tracking).
            if macro_name == "impl_spacing_type_impls" {
                let type_name = m.mac.tokens.to_string().trim().to_string();
                if !type_name.is_empty() {
                    custom_impls
                        .entry(type_name)
                        .or_default()
                        .push("Debug".to_string());
                }
            }
        }
    }

    custom_impls
}

/// Attach custom_impls to a TypeDefinition
fn attach_custom_impls(typedef: &mut TypeDefinition, impls: Vec<String>) {
    match &mut typedef.kind {
        TypeDefKind::Struct { custom_impls, .. } => {
            custom_impls.extend(impls);
            custom_impls.sort();
            custom_impls.dedup();
        }
        TypeDefKind::Enum { custom_impls, .. } => {
            custom_impls.extend(impls);
            custom_impls.sort();
            custom_impls.dedup();
        }
        TypeDefKind::MacroGenerated {
            implemented_traits, ..
        } => {
            implemented_traits.extend(impls);
            implemented_traits.sort();
            implemented_traits.dedup();
        }
        // Other kinds don't have custom_impls
        _ => {}
    }
}

/// Extract inherent methods from impl blocks (not trait impls)
/// Returns a map from type name to list of methods
fn extract_inherent_methods_from_items(items: &[Item]) -> BTreeMap<String, Vec<MethodDef>> {
    let mut methods_map: BTreeMap<String, Vec<MethodDef>> = BTreeMap::new();

    for item in items {
        if let Item::Impl(impl_item) = item {
            // Only interested in inherent impls (no trait)
            if impl_item.trait_.is_some() {
                continue;
            }

            // Get the type name that this impl block is for
            let type_name = if let syn::Type::Path(type_path) = impl_item.self_ty.as_ref() {
                type_path
                    .path
                    .segments
                    .last()
                    .map(|seg| seg.ident.to_string())
                    .unwrap_or_default()
            } else {
                continue;
            };

            if type_name.is_empty() {
                continue;
            }

            // Extract methods from this impl block
            for impl_item_fn in &impl_item.items {
                if let syn::ImplItem::Fn(method) = impl_item_fn {
                    if let Some(method_def) = extract_method_def(method, &type_name) {
                        methods_map
                            .entry(type_name.clone())
                            .or_default()
                            .push(method_def);
                    }
                }
            }
        }

        // Recursively handle inline modules
        if let Item::Mod(m) = item {
            if let Some((_, nested_items)) = &m.content {
                let nested_methods = extract_inherent_methods_from_items(nested_items);
                for (type_name, methods) in nested_methods {
                    methods_map.entry(type_name).or_default().extend(methods);
                }
            }
        }
    }

    methods_map
}

/// Extract Into<T> bounds from method generics and build a map of type parameter -> concrete type
///
/// For example, `fn foo<C: Into<Callback>>(callback: C)` would return {"C" -> "Callback"}
/// This allows us to "desugar" generic callback parameters back to concrete types for api.json.
fn extract_into_bounds(sig: &syn::Signature) -> BTreeMap<String, String> {
    let mut into_map = BTreeMap::new();

    for param in &sig.generics.params {
        if let syn::GenericParam::Type(type_param) = param {
            let param_name = type_param.ident.to_string();

            // Check bounds on the type parameter itself (e.g., <C: Into<Callback>>)
            for bound in &type_param.bounds {
                if let Some(concrete_type) = extract_into_inner_type(bound) {
                    into_map.insert(param_name.clone(), concrete_type);
                    break; // Use first Into bound found
                }
            }
        }
    }

    // Also check where clause (e.g., where C: Into<Callback>)
    if let Some(where_clause) = &sig.generics.where_clause {
        for predicate in &where_clause.predicates {
            if let syn::WherePredicate::Type(pred_type) = predicate {
                let param_name = extract_type_name(&pred_type.bounded_ty);

                for bound in &pred_type.bounds {
                    if let Some(concrete_type) = extract_into_inner_type(bound) {
                        into_map.insert(param_name.clone(), concrete_type);
                        break;
                    }
                }
            }
        }
    }

    into_map
}

/// Extract the inner type T from an Into<T> trait bound
/// Returns Some("Callback") for Into<Callback>, None for other bounds
fn extract_into_inner_type(bound: &syn::TypeParamBound) -> Option<String> {
    if let syn::TypeParamBound::Trait(trait_bound) = bound {
        let path = &trait_bound.path;

        // Check if this is Into<T> (last segment is "Into" with one generic arg)
        if let Some(last_segment) = path.segments.last() {
            if last_segment.ident == "Into" {
                if let syn::PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(extract_type_name(inner_ty));
                    }
                }
            }
        }
    }
    None
}

/// Extract a single method definition from an ImplItemFn
pub(super) fn extract_method_def(method: &syn::ImplItemFn, type_name: &str) -> Option<MethodDef> {
    let method_name = method.sig.ident.to_string();

    // Check visibility (pub or not) - vis is on the method, not the sig
    let is_public = matches!(&method.vis, syn::Visibility::Public(_));

    // Build map of generic type parameters with Into<T> bounds
    // This allows us to rewrite "C" -> "Callback" when C: Into<Callback>
    let into_bounds = extract_into_bounds(&method.sig);

    // Skip methods with generic type parameters that are NOT just Into<T> bounds
    // These methods cannot be exported to C API (e.g., fn new<T: 'static>(value: T))
    // We allow generic params if they ALL have Into<T> bounds (callback pattern)
    let generic_type_params: Vec<_> = method
        .sig
        .generics
        .params
        .iter()
        .filter_map(|p| {
            if let syn::GenericParam::Type(tp) = p {
                Some(tp.ident.to_string())
            } else {
                None
            }
        })
        .collect();

    // If there are generic type params that are NOT in into_bounds, skip this method
    for param in &generic_type_params {
        if !into_bounds.contains_key(param) {
            // This is a truly generic method that cannot be exported to C API
            return None;
        }
    }

    // Determine self kind
    let self_kind: Option<SelfKind> = match method.sig.receiver() {
        None => None,
        Some(receiver) => {
            if receiver.reference.is_some() {
                if receiver.mutability.is_some() {
                    Some(SelfKind::RefMut)
                } else {
                    Some(SelfKind::Ref)
                }
            } else {
                Some(SelfKind::Value)
            }
        }
    };

    // Determine if this is a constructor
    // Heuristics: no self parameter AND (returns Self, type_name, Result<Self, _>, or Option<Self>)
    let is_constructor = self_kind.is_none() && {
        match &method.sig.output {
            syn::ReturnType::Default => false,
            syn::ReturnType::Type(_, ty) => {
                let ret_str = extract_type_name(ty);
                // Direct match: Self or type_name
                if ret_str == "Self" || ret_str == type_name {
                    true
                }
                // Check for Result<Self, _> or Result<TypeName, _>
                else if ret_str.starts_with("Result<") {
                    let inner = &ret_str[7..]; // Skip "Result<"
                    inner.starts_with("Self,") || inner.starts_with(&format!("{},", type_name))
                }
                // Check for Option<Self> or Option<TypeName>
                else if ret_str.starts_with("Option<") && ret_str.ends_with('>') {
                    let inner = &ret_str[7..ret_str.len() - 1]; // Extract inner type
                    inner == "Self" || inner == type_name
                } else {
                    false
                }
            }
        }
    };

    // Extract arguments (skip self)
    let mut args = Vec::new();
    for arg in &method.sig.inputs {
        if let syn::FnArg::Typed(pat_type) = arg {
            // Get argument name
            let arg_name = if let syn::Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
                pat_ident.ident.to_string()
            } else {
                continue;
            };

            // Get type and ref kind
            let (mut ty, ref_kind) = extract_ref_kind_from_syn_type(&pat_type.ty);

            // If this type is a generic parameter with an Into<T> bound,
            // rewrite it to the concrete type T
            // e.g., for fn foo<C: Into<Callback>>(callback: C), rewrite "C" -> "Callback"
            if let Some(concrete_type) = into_bounds.get(&ty) {
                ty = concrete_type.clone();
            }

            // Replace "Self" with the actual type name
            // This handles cases like fn with_child(child: Self) -> Dom
            if ty == "Self" {
                ty = type_name.to_string();
            }

            args.push(MethodArg {
                name: arg_name,
                ty,
                ref_kind,
                source_ty: written_type_name(&pat_type.ty),
            });
        }
    }

    // Extract return type and ref kind
    let (return_type, return_ref_kind) = match &method.sig.output {
        syn::ReturnType::Default => (None, RefKind::Value),
        syn::ReturnType::Type(_, ty) => {
            let (ret_str, ref_kind) = extract_ref_kind_from_syn_type(ty);
            if ret_str.is_empty() || ret_str == "()" {
                (None, RefKind::Value)
            } else {
                // Replace "Self" with the actual type name in return type
                let ret_str = if ret_str == "Self" {
                    type_name.to_string()
                } else {
                    ret_str
                };
                (Some(ret_str), ref_kind)
            }
        }
    };

    // Extract doc comments
    let doc = extract_doc_comments(&method.attrs);

    Some(MethodDef {
        name: method_name,
        self_kind,
        args,
        return_type,
        return_ref_kind,
        is_constructor,
        doc,
        is_public,
        from_trait: None,
    })
}

/// The last path segment of a type as the source writes it (one reference
/// stripped), without the Az-prefix stripping of [`extract_type_name`]:
/// `&std::string::String` -> `String`, `AzString` -> `AzString`.
fn written_type_name(ty: &syn::Type) -> String {
    let ty = match ty {
        syn::Type::Reference(r) => r.elem.as_ref(),
        other => other,
    };
    written_type(ty)
}

/// A type as the source writes it, each path by its last segment and with
/// its generic arguments (`Option<AzString>` vs `Option<String>`, `&str`):
/// the index's own spelling strips the `Az` prefix and cannot tell an
/// `Option` of a std `String` from one of an `AzString`.
fn written_type(ty: &syn::Type) -> String {
    match ty {
        syn::Type::Path(p) if p.qself.is_none() => match p.path.segments.last() {
            Some(seg) => {
                let ident = seg.ident.to_string();
                let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
                    return ident;
                };
                let inner: Vec<String> = args
                    .args
                    .iter()
                    .filter_map(|a| match a {
                        syn::GenericArgument::Type(t) => Some(written_type(t)),
                        _ => None,
                    })
                    .collect();
                if inner.is_empty() {
                    ident
                } else {
                    format!("{ident}<{}>", inner.join(", "))
                }
            }
            None => String::new(),
        },
        syn::Type::Reference(r) => format!(
            "&{}{}",
            if r.mutability.is_some() { "mut " } else { "" },
            written_type(&r.elem)
        ),
        other => clean_type_string(&other.to_token_stream().to_string()),
    }
}

/// Attach methods to a TypeDefinition
fn attach_methods(typedef: &mut TypeDefinition, methods: Vec<MethodDef>) {
    typedef.methods.extend(methods);
}

/// Extract doc comments from attributes as a multi-line vector
fn extract_doc_comments(attrs: &[syn::Attribute]) -> Vec<String> {
    let mut docs = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    // Preserve the line as-is, including empty lines for paragraph breaks
                    // Only trim leading space that rustdoc adds after ///
                    let line = s.value();
                    // Trim exactly one leading space if present (rustdoc convention)
                    let trimmed = if line.starts_with(' ') {
                        &line[1..]
                    } else {
                        &line[..]
                    };
                    docs.push(trimmed.to_string());
                }
            }
        }
    }
    docs
}

/// Clean up a type string (remove extra whitespace, normalize)
fn clean_type_string(s: &str) -> String {
    let result = s.split_whitespace().collect::<Vec<_>>().join(" ");
    // Fix syn's spacing for raw pointers: "* const T" -> "*const T", "* mut T" -> "*mut T"
    result
        .replace("* const", "*const")
        .replace("* mut", "*mut")
        // Also fix reference spacing: "& mut T" -> "&mut T", though extract_ref_kind_from_syn_type
        // handles these directly, this is for safety
        .replace("& mut", "&mut")
        .replace("& 'a", "&'a")
}

// unit tests
#[cfg(test)]
mod tests {
    use super::*;

    /// `autofix add RawImage.from_text --fn azul_layout::cpurender::text_image`
    /// (wave 6 wrote these by hand): the function is defined in
    /// cpurender/text_raster.rs and re-exported by `pub use text_raster::*`;
    /// `azul_core::xml::html::encode_text` lives in a `#[path]` module
    /// (xml_html.rs). Both are found, the body keeps the path given.
    #[test]
    fn a_free_function_is_found_through_a_re_export_or_a_path_module() {
        let root = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = root.path().join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, text).expect("written");
        };
        write("layout/src/cpurender/mod.rs", "pub mod text_raster;\npub use text_raster::*;\n");
        write(
            "layout/src/cpurender/text_raster.rs",
            "pub fn text_image(text: AzString, style: TextRasterStyle) -> OptionRawImage { todo!() }\n\
             fn private_helper() {}\n",
        );
        write("layout/src/other.rs", "fn text_image() {}\n");
        write("core/src/xml.rs", "#[path = \"xml_html.rs\"]\npub mod html;\n");
        write("core/src/xml_html.rs", "pub fn encode_text(s: &str) -> String { todo!() }\n");

        let found = find_free_fn(root.path(), "azul_layout::cpurender::text_image", "RawImage")
            .expect("found through the re-export");
        assert_eq!(found.path, "azul_layout::cpurender::text_image");
        assert_eq!(found.defined_in, "cpurender::text_raster");
        let args: Vec<&str> = found.method.args.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(args, vec!["text", "style"]);
        assert_eq!(found.method.return_type.as_deref(), Some("OptionRawImage"));
        assert!(found.method.self_kind.is_none());

        let found = find_free_fn(root.path(), "azul_core::xml::html::encode_text", "Xml")
            .expect("found in the #[path] module");
        assert_eq!(found.defined_in, "xml::html", "the declared module path");

        assert!(find_free_fn(root.path(), "azul_layout::other::text_image", "RawImage").is_err(), "not public");
        assert!(find_free_fn(root.path(), "azul_layout::cpurender::nope", "RawImage").is_err());
        assert!(find_free_fn(root.path(), "nocrate::f", "RawImage").is_err());
    }

    /// `TextRasterStyle` is defined in `cpurender/text_raster.rs`, and
    /// `cpurender` re-exported it (`mod text_raster; pub use text_raster::*;`):
    /// api.json got the private path `azul_layout::cpurender::text_raster::
    /// TextRasterStyle` and the dylib did not compile (15 E0603, wave 6) until
    /// the module was made pub. A type behind a private module is indexed by
    /// the path of the `pub use` that re-exports it (a glob of its module or
    /// its name); one nothing re-exports keeps its path and the index names
    /// the private module on the way.
    #[test]
    fn a_type_in_a_private_module_is_indexed_by_its_public_re_export_path() {
        let root = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = root.path().join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, text).expect("written");
        };
        write("layout/src/lib.rs", "pub mod cpurender;\n");
        write(
            "layout/src/cpurender/mod.rs",
            "mod text_raster;\npub use text_raster::*;\nmod named;\npub use self::named::{Only};\n\
             mod internal;\npub mod open;\n",
        );
        write("layout/src/cpurender/text_raster.rs", "#[repr(C)] pub struct TextRasterStyle { pub size: f32 }\n");
        write(
            "layout/src/cpurender/named.rs",
            "#[repr(C)] pub struct Only { pub a: u8 }\n#[repr(C)] pub struct NotNamed { pub a: u8 }\n",
        );
        write("layout/src/cpurender/internal.rs", "#[repr(C)] pub struct Internal { pub a: u8 }\n");
        write("layout/src/cpurender/open.rs", "#[repr(C)] pub struct Open { pub a: u8 }\n");

        let index = TypeIndex::build(root.path(), false).expect("index");
        let path_of = |name: &str| index.resolve(name, None).expect(name).full_path.clone();

        assert_eq!(path_of("TextRasterStyle"), "azul_layout::cpurender::TextRasterStyle");
        assert!(index.get_by_path("azul_layout::cpurender::TextRasterStyle").is_some());
        assert_eq!(path_of("Only"), "azul_layout::cpurender::Only");
        assert_eq!(path_of("Open"), "azul_layout::cpurender::open::Open");
        assert_eq!(index.private_module_on("azul_layout::cpurender::open::Open"), None);

        assert_eq!(path_of("NotNamed"), "azul_layout::cpurender::named::NotNamed");
        assert_eq!(
            index.private_module_on("azul_layout::cpurender::named::NotNamed").as_deref(),
            Some("azul_layout::cpurender::named")
        );
        assert_eq!(
            index.private_module_on("azul_layout::cpurender::internal::Internal").as_deref(),
            Some("azul_layout::cpurender::internal")
        );
    }

    /// `azul_dll::desktop` does not exist on wasm32; the `azul_dll::unified`
    /// facade (`dll/src/unified/<m>.rs`: `pub use crate::desktop::extra::<m>::*;`
    /// off wasm, stubs on wasm) is the path every target names. Nine api.json
    /// externals pointed at desktop paths (AudioEncoder, Mp4Muxer, ParsedPdf,
    /// ...; integration 2026-10-05) because the index named every azul_dll
    /// type by its definition. A type the facade re-exports takes the facade
    /// path - through a module's own `pub use` (`AudioEncoder`), defined at the
    /// module (`AudioSink`), or in a public child module (`pipeline::Decoded`)
    /// - and the desktop path counts as not nameable, so the scan fixes it. A
    /// type the facade cannot reach (a private child) keeps its path.
    #[test]
    fn a_desktop_extra_type_reexported_by_the_unified_facade_takes_the_facade_path() {
        let root = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = root.path().join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, text).expect("written");
        };
        write("dll/src/lib.rs", "pub mod desktop;\npub mod unified;\n");
        write("dll/src/desktop/mod.rs", "pub mod extra;\n");
        write("dll/src/desktop/extra/mod.rs", "pub mod audio;\n");
        write(
            "dll/src/desktop/extra/audio/mod.rs",
            "pub mod codec;\npub use codec::{AudioEncoder};\npub(crate) mod playback;\npub mod pipeline;\n\
             #[repr(C)] pub struct AudioSink { pub ptr: *mut u8 }\n",
        );
        write("dll/src/desktop/extra/audio/codec.rs", "#[repr(C)] pub struct AudioEncoder { pub ptr: *mut u8 }\n");
        write("dll/src/desktop/extra/audio/playback.rs", "#[repr(C)] pub struct Internal { pub a: u8 }\n");
        write("dll/src/desktop/extra/audio/pipeline.rs", "#[repr(C)] pub struct Decoded { pub a: u8 }\n");
        write("dll/src/unified/mod.rs", "pub mod audio;\n");
        write(
            "dll/src/unified/audio.rs",
            "#[cfg(not(target_arch = \"wasm32\"))]\npub use crate::desktop::extra::audio::*;\n\
             #[cfg(target_arch = \"wasm32\")]\n#[repr(C)] pub struct AudioSink { pub ptr: *mut u8 }\n",
        );

        let index = TypeIndex::build(root.path(), false).expect("index");
        let path_of = |name: &str| index.resolve(name, None).expect(name).full_path.clone();

        assert_eq!(path_of("AudioEncoder"), "azul_dll::unified::audio::AudioEncoder");
        assert_eq!(path_of("AudioSink"), "azul_dll::unified::audio::AudioSink");
        assert_eq!(path_of("Decoded"), "azul_dll::unified::audio::pipeline::Decoded");
        assert_eq!(path_of("Internal"), "azul_dll::desktop::extra::audio::playback::Internal");

        // The scan's path fix: the desktop path of a facaded type is not
        // nameable on every target, its facade path is.
        assert_eq!(
            index
                .private_module_on("azul_dll::desktop::extra::audio::codec::AudioEncoder")
                .as_deref(),
            Some("azul_dll::desktop::extra::audio")
        );
        assert_eq!(index.private_module_on("azul_dll::unified::audio::AudioEncoder"), None);
    }

    /// A module-private type can never be part of the C API (the generated
    /// code lives in another module), but the index took it as one: the XML
    /// tree builder's `enum NodeData` (core/src/xml_html_tree.rs) was a
    /// "critical" duplicate of `azul_core::dom::NodeData`, and XML8's
    /// private `Content` hijacked the CSS `Content` (2026-10-03 / 10-05).
    /// Only types visible outside their module are indexed.
    #[test]
    fn a_module_private_type_does_not_shadow_a_public_one_of_the_same_name() {
        let root = tempfile::tempdir().expect("temp dir");
        let write = |rel: &str, text: &str| {
            let path = root.path().join(rel);
            fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
            fs::write(path, text).expect("written");
        };
        write("core/src/lib.rs", "pub mod dom;\npub mod tree;\n");
        write("core/src/dom.rs", "#[repr(C)] pub struct NodeData { pub a: u8 }\n");
        write(
            "core/src/tree.rs",
            "enum NodeData { Document, Element { name: String } }\n\
             struct Hidden;\n\
             pub(crate) struct CrateWide { pub a: u8 }\n\
             mod inner { struct AlsoHidden; pub struct Seen; }\n",
        );

        let index = TypeIndex::build(root.path(), false).expect("index");
        let defs: Vec<String> = index
            .iter_all()
            .filter(|(name, _)| name.as_str() == "NodeData")
            .flat_map(|(_, defs)| defs.iter().map(|d| d.full_path.clone()))
            .collect();
        assert_eq!(defs, vec!["azul_core::dom::NodeData".to_string()]);
        assert!(index.resolve("Hidden", None).is_none());
        assert!(index.resolve("AlsoHidden", None).is_none());
        assert!(index.resolve("CrateWide", None).is_some(), "pub(crate) is visible to the crate's API code");
        assert!(index.resolve("Seen", None).is_some());
    }

    /// A method in an `impl T` block of another file (`impl CallbackInfo` in
    /// widgets/form.rs, `impl RichTextDoc` in rich_text/html.rs) is attached
    /// to the type found by NAME, but the type found by PATH kept the old
    /// copy: both maps share one `Arc`, so `Arc::make_mut` cloned it. The
    /// wave-6 gone-function scan looks the type up by path and called 10
    /// existing methods gone (2026-10-03). Both lookups see the method.
    #[test]
    fn a_cross_file_method_is_seen_by_the_path_lookup_too() {
        let mut index = TypeIndex::new();
        index.add_type_for_test(TypeDefinition {
            full_path: "azul_layout::callbacks::CallbackInfo".to_string(),
            type_name: "CallbackInfo".to_string(),
            file_path: std::path::PathBuf::from("/nonexistent/callbacks.rs"),
            module_path: "callbacks".to_string(),
            crate_name: "azul_layout".to_string(),
            kind: TypeDefKind::Struct {
                fields: IndexMap::new(),
                repr: Some("C".to_string()),
                repr_attr_count: 1,
                generic_params: Vec::new(),
                derives: Vec::new(),
                custom_impls: Vec::new(),
                is_tuple_struct: false,
            },
            source_code: String::new(),
            methods: Vec::new(),
        });
        let method = MethodDef {
            name: "get_form_data".to_string(),
            self_kind: Some(SelfKind::RefMut),
            args: Vec::new(),
            return_type: None,
            return_ref_kind: RefKind::Value,
            is_constructor: false,
            doc: Vec::new(),
            is_public: true,
            from_trait: None,
        };
        index.attach_methods_to_type("CallbackInfo", vec![method]);

        let by_path = index
            .get_by_path("azul_layout::callbacks::CallbackInfo")
            .expect("the type by path");
        assert!(by_path.methods.iter().any(|m| m.name == "get_form_data"));
        let by_name = &index.get_all_by_name("CallbackInfo").expect("by name")[0];
        assert!(by_name.methods.iter().any(|m| m.name == "get_form_data"));
    }

    /// The console summaries (`autofix add`, `discover`, `debug api`) printed
    /// a method as `(&self) -> ()` whatever its arguments, which read as "the
    /// tool dropped the arguments" and had them written by hand (2026-09-30).
    /// One formatter shows every argument with its reference kind.
    #[test]
    fn a_methods_summary_shows_every_argument_with_its_ref_kind() {
        let m = MethodDef {
            name: "toggle_text_format".to_string(),
            self_kind: Some(SelfKind::RefMut),
            args: vec![
                MethodArg {
                    name: "host".to_string(),
                    ty: "DomNodeId".to_string(),
                    ref_kind: RefKind::Value,
                    source_ty: "DomNodeId".to_string(),
                },
                MethodArg {
                    name: "format".to_string(),
                    ty: "TextFormat".to_string(),
                    ref_kind: RefKind::Ref,
                    source_ty: "TextFormat".to_string(),
                },
            ],
            return_type: None,
            return_ref_kind: RefKind::Value,
            is_constructor: false,
            doc: Vec::new(),
            is_public: true,
            from_trait: None,
        };
        assert_eq!(
            m.signature(),
            "(&mut self, host: DomNodeId, format: &TextFormat) -> ()"
        );
        let ctor = MethodDef {
            name: "create".to_string(),
            self_kind: None,
            args: Vec::new(),
            return_type: Some("Tile".to_string()),
            return_ref_kind: RefKind::Value,
            is_constructor: true,
            doc: Vec::new(),
            is_public: true,
            from_trait: None,
        };
        assert_eq!(ctor.signature(), "() -> Tile");
    }

    /// `impl Default for Tile { fn default() .. }` is not an API function:
    /// api.json carries it as `custom_impls: ["Default"]` and the codegen
    /// makes `Tile_default` from that. `autofix add Tile.*` listed it as a
    /// constructor named `default` and `autofix list` as "missing in api.json"
    /// (2026-09-30) - the type index merged trait-impl methods into the
    /// inherent ones without saying where they came from. A method knows its
    /// trait, and the standard traits are never API functions; a wrapper
    /// trait that exposes free functions still is.
    #[test]
    fn a_standard_traits_impl_method_is_not_an_api_function() {
        let mut m = MethodDef {
            name: "default".to_string(),
            self_kind: None,
            args: Vec::new(),
            return_type: Some("Tile".to_string()),
            return_ref_kind: RefKind::Value,
            is_constructor: true,
            doc: Vec::new(),
            is_public: true,
            from_trait: Some("Default".to_string()),
        };
        assert!(m.is_std_trait_impl());
        m.from_trait = Some("WrapperTrait".to_string());
        assert!(!m.is_std_trait_impl(), "a wrapper trait's method stays an API candidate");
        m.from_trait = None;
        assert!(!m.is_std_trait_impl(), "an inherent method stays an API candidate");
        m.from_trait = Some("HostOut".to_string());
        assert!(!m.is_std_trait_impl(), "an internal trait is not a custom impl");
        assert!(m.is_non_api_trait_impl(), "but its methods never become API functions");

        let source = r#"
            pub struct Tile { pub title: String }
            impl Tile { pub fn create(title: String) -> Self { Tile { title } } }
            impl Default for Tile { fn default() -> Self { Tile { title: String::new() } } }
            impl Clone for Tile { fn clone(&self) -> Self { Tile { title: self.title.clone() } } }
        "#;
        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse");
        let mut methods = extract_inherent_methods_from_items(&syntax_tree.items);
        for (ty, trait_methods) in extract_trait_impl_methods_from_items(&syntax_tree.items) {
            methods.entry(ty).or_default().extend(trait_methods);
        }
        let from: Vec<(String, Option<String>)> = methods
            .get("Tile")
            .expect("Tile")
            .iter()
            .map(|m| (m.name.clone(), m.from_trait.clone()))
            .collect();
        assert!(from.contains(&("create".to_string(), None)));
        assert!(from.contains(&("default".to_string(), Some("Default".to_string()))));
        assert!(from.contains(&("clone".to_string(), Some("Clone".to_string()))));
    }

    /// Methods a file's own `macro_rules!` write are methods of the type
    /// (the wizard pages' `dom`, the dialogs' `with_on_event`).
    #[test]
    fn methods_written_by_a_local_macro_are_seen() {
        let source = r#"
            pub struct Page { pub theme: u8 }
            macro_rules! theme_and_dom {
                ($page:ident, $default:expr) => {
                    impl $page {
                        pub fn with_theme(mut self, theme: u8) -> Self { self.theme = theme; self }
                        pub fn dom(self) -> Dom { todo!() }
                    }
                    impl Default for $page { fn default() -> Self { $default } }
                };
            }
            theme_and_dom!(Page, Page { theme: 0 });
        "#;
        let mut file: File = syn::parse_file(source).expect("parses");
        expand_local_item_macros(&mut file);
        let methods = extract_inherent_methods_from_items(&file.items);
        let names: Vec<String> = methods.get("Page").expect("Page").iter().map(|m| m.name.clone()).collect();
        assert!(names.contains(&"with_theme".to_string()) && names.contains(&"dom".to_string()), "{names:?}");
        let traits = extract_trait_impl_methods_from_items(&file.items);
        assert!(traits.get("Page").is_none(), "trait impls are not expanded");
    }

    fn extract_types_from_source(source: &str) -> Vec<TypeDefinition> {
        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse");
        let mut types = Vec::new();

        for item in &syntax_tree.items {
            match item {
                Item::Use(_) => continue, // Skip use statements
                Item::Struct(s) => {
                    if let Some(t) = extract_struct("test_crate", "", Path::new("test.rs"), s) {
                        types.push(t);
                    }
                }
                Item::Enum(e) => {
                    if let Some(t) = extract_enum("test_crate", "", Path::new("test.rs"), e) {
                        types.push(t);
                    }
                }
                Item::Type(t) => {
                    if let Some(td) = extract_type_alias("test_crate", "", Path::new("test.rs"), t)
                    {
                        types.push(td);
                    }
                }
                Item::Macro(m) => {
                    let generated =
                        extract_macro_generated_types("test_crate", "", Path::new("test.rs"), m);
                    types.extend(generated);
                }
                _ => {}
            }
        }

        types
    }

    #[test]
    fn test_skips_use_imports() {
        let source = r#"
            use azul_core::resources::FontCache;
            pub use other::Thing;
        "#;
        let types = extract_types_from_source(source);
        assert!(
            types.is_empty(),
            "use statements should not create type definitions"
        );
    }

    #[test]
    fn test_extracts_struct_definition() {
        let source = r#"
            #[repr(C)]
            pub struct FontCache {
                pub fonts: FontVec,
            }
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].type_name, "FontCache");
        assert_eq!(types[0].full_path, "test_crate::FontCache");

        match &types[0].kind {
            TypeDefKind::Struct { repr, fields, .. } => {
                assert!(repr.is_some());
                assert!(fields.contains_key("fonts"));
            }
            _ => panic!("Expected Struct"),
        }
    }

    #[test]
    fn test_extracts_enum_definition() {
        let source = r#"
            #[repr(C)]
            pub enum Color {
                Red,
                Green,
                Blue,
            }
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].type_name, "Color");

        match &types[0].kind {
            TypeDefKind::Enum { variants, repr, .. } => {
                assert!(repr.is_some());
                assert_eq!(variants.len(), 3);
                assert!(variants.contains_key("Red"));
            }
            _ => panic!("Expected Enum"),
        }
    }

    #[test]
    fn test_extracts_type_alias() {
        let source = r#"
            pub type MyCallback = extern "C" fn(data: *const u8) -> bool;
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].type_name, "MyCallback");

        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);
                assert_eq!(returns.as_deref(), Some("bool"));
            }
            _ => panic!("Expected CallbackTypedef"),
        }
    }

    #[test]
    fn test_callback_typedef_with_ref_mut() {
        // Test that &mut T is correctly parsed with RefKind::RefMut and base type T
        let source = r#"
            pub type OnClickCallback = extern "C" fn(data: &mut RefAny, info: &mut CallbackInfo) -> Update;
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].type_name, "OnClickCallback");

        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 2);

                // First arg: &mut RefAny -> ty="RefAny", ref_kind=RefMut
                assert_eq!(
                    args[0].ty, "RefAny",
                    "First arg type should be base type without &mut"
                );
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::RefMut,
                    "First arg should be RefMut"
                );
                assert_eq!(args[0].name, Some("data".to_string()));

                // Second arg: &mut CallbackInfo -> ty="CallbackInfo", ref_kind=RefMut
                assert_eq!(
                    args[1].ty, "CallbackInfo",
                    "Second arg type should be base type without &mut"
                );
                assert_eq!(
                    args[1].ref_kind,
                    RefKind::RefMut,
                    "Second arg should be RefMut"
                );
                assert_eq!(args[1].name, Some("info".to_string()));

                assert_eq!(returns.as_deref(), Some("Update"));
            }
            _ => panic!("Expected CallbackTypedef, got {:?}", types[0].kind),
        }
    }

    #[test]
    fn test_callback_typedef_with_ref() {
        // Test that &T is correctly parsed with RefKind::Ref and base type T
        let source = r#"
            pub type ReadCallback = extern "C" fn(state: &TextInputState) -> bool;
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);

        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);

                // &TextInputState -> ty="TextInputState", ref_kind=Ref
                assert_eq!(
                    args[0].ty, "TextInputState",
                    "Arg type should be base type without &"
                );
                assert_eq!(args[0].ref_kind, RefKind::Ref, "Arg should be Ref");

                assert_eq!(returns.as_deref(), Some("bool"));
            }
            _ => panic!("Expected CallbackTypedef"),
        }
    }

    #[test]
    fn test_callback_typedef_with_raw_pointer() {
        // Test that *mut T and *const T are correctly parsed as Value with full pointer type
        // Note: syn's to_token_stream().to_string() adds spaces, so we get "* mut" not "*mut"
        let source = r#"
            pub type DestructorCallback = extern "C" fn(ptr: *mut c_void);
            pub type CloneCallback = extern "C" fn(ptr: *const c_void) -> *const c_void;
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 2);

        // DestructorCallback
        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);

                // *mut c_void -> ty="*mut c_void", ref_kind=Value (pointers are passed by value)
                assert_eq!(
                    args[0].ty, "*mut c_void",
                    "Raw pointer should include the *mut prefix"
                );
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::Value,
                    "Raw pointers are passed by value"
                );

                assert!(returns.is_none());
            }
            _ => panic!("Expected CallbackTypedef for DestructorCallback"),
        }

        // CloneCallback
        match &types[1].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);

                // *const c_void -> ty="*const c_void", ref_kind=Value
                // Note: syn adds a space, but extract_ref_kind_from_syn_type reconstructs without
                // space
                assert_eq!(
                    args[0].ty, "*const c_void",
                    "Raw pointer should include the *const prefix"
                );
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::Value,
                    "Raw pointers are passed by value"
                );

                // Return type comes from clean_type_string which may have spaces from syn
                let ret = returns.as_deref().unwrap();
                assert!(
                    ret.contains("const") && ret.contains("c_void"),
                    "Return should be *const c_void, got: {}",
                    ret
                );
            }
            _ => panic!("Expected CallbackTypedef for CloneCallback"),
        }
    }

    #[test]
    fn test_callback_typedef_mixed_args() {
        // Test callback with mixed argument types: &mut, &, value, and raw pointers
        let source = r#"
            pub type MixedCallback = extern "C" fn(
                ref_mut_arg: &mut RefAny,
                ref_arg: &State,
                value_arg: usize,
                ptr_arg: *const Data
            ) -> Result;
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);

        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 4);

                // &mut RefAny
                assert_eq!(args[0].ty, "RefAny");
                assert_eq!(args[0].ref_kind, RefKind::RefMut);

                // &State
                assert_eq!(args[1].ty, "State");
                assert_eq!(args[1].ref_kind, RefKind::Ref);

                // usize (value)
                assert_eq!(args[2].ty, "usize");
                assert_eq!(args[2].ref_kind, RefKind::Value);

                // *const Data (pointer, passed by value)
                assert_eq!(args[3].ty, "*const Data");
                assert_eq!(args[3].ref_kind, RefKind::Value);

                assert_eq!(returns.as_deref(), Some("Result"));
            }
            _ => panic!("Expected CallbackTypedef"),
        }
    }

    #[test]
    fn test_callback_typedef_no_return() {
        // Test callback with no return type (void)
        let source = r#"
            pub type VoidCallback = extern "C" fn(ptr: *mut Data);
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);

        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);
                assert!(
                    returns.is_none(),
                    "Void callbacks should have None as return"
                );
            }
            _ => panic!("Expected CallbackTypedef"),
        }
    }

    #[test]
    fn test_simple_type_alias() {
        // Test that non-callback type aliases are extracted correctly
        let source = r#"
            pub type GLenum = u32;
            pub type MyVec = Vec<String>;
        "#;
        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 2);

        match &types[0].kind {
            TypeDefKind::TypeAlias { target, .. } => {
                assert_eq!(target, "u32");
            }
            _ => panic!("Expected TypeAlias for GLenum"),
        }

        match &types[1].kind {
            TypeDefKind::TypeAlias { target, .. } => {
                // extract_type_name formats generics without spaces
                assert_eq!(target, "Vec<String>");
            }
            _ => panic!("Expected TypeAlias for MyVec"),
        }
    }

    #[test]
    fn test_expands_impl_vec_macro() {
        // The REAL macro signature, e.g. from azul-layout's fluent.rs:
        //   impl_vec!(T, TVec, TVecDestructor, TVecDestructorType, TVecSlice, OptionT)
        // This test used to pass only three arguments, which is a form
        // `impl_vec!` has not taken for a long time. The expander requires
        // `args.len() >= 5` — correctly, since it needs the slice type — so the
        // three-arg call expanded to NOTHING and the test saw just `Font`.
        // Nobody noticed because azul-doc's tests run in no CI job (audit D1).
        let source = r#"
            pub struct Font { pub data: u8 }
            impl_vec!(Font, FontVec, FontVecDestructor, FontVecDestructorType, FontVecSlice, OptionFont);
        "#;
        let types = extract_types_from_source(source);

        let names: Vec<_> = types.iter().map(|t| t.type_name.as_str()).collect();
        assert!(names.contains(&"Font"), "got {names:?}");
        assert!(names.contains(&"FontVec"), "got {names:?}");
        assert!(names.contains(&"FontVecDestructor"), "got {names:?}");
        assert!(names.contains(&"FontVecDestructorType"), "got {names:?}");
        assert!(names.contains(&"FontVecSlice"), "got {names:?}");

        // Font plus the four types the expander emits. Asserted after the names
        // so a count mismatch reports WHICH type appeared or went missing.
        assert_eq!(types.len(), 5, "got {names:?}");
    }

    #[test]
    fn test_expands_impl_option_macro() {
        let source = r#"
            pub struct Foo { pub x: i32 }
            impl_option!(Foo, OptionFoo, OptionFooEnumWrapper);
        "#;
        let types = extract_types_from_source(source);

        // Should have: Foo, OptionFoo, OptionFooEnumWrapper
        assert_eq!(types.len(), 3);

        let names: Vec<_> = types.iter().map(|t| t.type_name.as_str()).collect();
        assert!(names.contains(&"Foo"));
        assert!(names.contains(&"OptionFoo"));
        assert!(names.contains(&"OptionFooEnumWrapper"));
    }

    #[test]
    fn test_expands_impl_widget_callback_macro() {
        // The 4-parameter callback macro is `impl_widget_callback!`. This test
        // used to call `impl_callback!` with four arguments, which is the form it
        // had BEFORE the rename — the comment at the `impl_callback` arm records
        // it: "4-parameter version is now impl_widget_callback!". Today
        // `impl_callback!` takes two arguments and only ADDS TRAITS to an existing
        // type, generating nothing, so the old call expanded to nothing and the
        // test saw only the type alias. azul-doc's tests run in no CI job, so the
        // rename never broke anything visibly (audit D1).
        let source = r#"
            pub type OnClickCallbackType = extern "C" fn(data: &mut RefAny) -> Update;
            impl_widget_callback!(OnClick, OptionOnClick, OnClickCallback, OnClickCallbackType);
        "#;
        let types = extract_types_from_source(source);

        let names: Vec<_> = types.iter().map(|t| t.type_name.as_str()).collect();
        assert!(
            names.contains(&"OnClickCallbackType"),
            "the type alias, got {names:?}"
        );
        assert!(names.contains(&"OnClick"), "CallbackWrapper, got {names:?}");
        assert!(
            names.contains(&"OptionOnClick"),
            "OptionCallbackWrapper, got {names:?}"
        );
        assert!(
            names.contains(&"OnClickCallback"),
            "CallbackValue, got {names:?}"
        );

        assert_eq!(types.len(), 4, "got {names:?}");
    }

    #[test]
    fn test_is_primitive() {
        assert!(TypeIndex::is_primitive("u8"));
        assert!(TypeIndex::is_primitive("i32"));
        assert!(TypeIndex::is_primitive("f64"));
        assert!(TypeIndex::is_primitive("bool"));
        assert!(TypeIndex::is_primitive("*const u8"));
        assert!(TypeIndex::is_primitive("*mut c_void"));
        assert!(TypeIndex::is_primitive("&str"));
        assert!(TypeIndex::is_primitive("[u8; 4]"));

        assert!(!TypeIndex::is_primitive("String"));
        assert!(!TypeIndex::is_primitive("Vec"));
        assert!(!TypeIndex::is_primitive("FontCache"));
    }

    #[test]
    fn test_module_path_inference() {
        assert_eq!(
            infer_module_path("azul_core", Path::new("/foo/core/src/lib.rs")),
            ""
        );
        assert_eq!(
            infer_module_path("azul_core", Path::new("/foo/core/src/dom.rs")),
            "dom"
        );
        assert_eq!(
            infer_module_path("azul_core", Path::new("/foo/core/src/dom/mod.rs")),
            "dom"
        );
        assert_eq!(
            infer_module_path("azul_core", Path::new("/foo/core/src/widgets/button.rs")),
            "widgets::button"
        );
    }

    /// Comprehensive integration test that parses a realistic callbacks.rs file
    /// and verifies that callback_typedef types are correctly extracted.
    ///
    /// This test reproduces the bug where `&mut RefAny` was being parsed as
    /// `type: "& mut RefAny", ref: "value"` instead of `type: "RefAny", ref: "refmut"`.
    ///
    /// The test uses syn's AST directly and avoids string comparisons where possible.
    #[test]
    fn test_parse_realistic_callbacks_file() {
        // This source mirrors the structure of core/src/callbacks.rs
        let source = r#"
use core::ffi::c_void;

/// Specifies if the screen should be updated after the callback function has returned
#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Update {
    DoNothing,
    RefreshDom,
    RefreshDomAllWindows,
}

/// RefAny is a type-erased reference-counted pointer
#[repr(C)]
pub struct RefAny {
    ptr: *mut c_void,
    sharing_info: u64,
}

/// Layout callback info passed to the layout function
#[repr(C)]
pub struct LayoutCallbackInfo {
    pub window_size: (u32, u32),
}

/// The styled DOM returned from layout callbacks
#[repr(C)]
pub struct StyledDom {
    pub root: u64,
}

/// VirtualView callback return value
#[repr(C)]
pub struct VirtualViewReturn {
    pub dom: Dom,
}

/// Information passed to VirtualView callbacks
#[repr(C)]
pub struct VirtualViewCallbackInfo {
    pub bounds: (f32, f32, f32, f32),
}

// Callback typedefs - these are the critical types we need to parse correctly

/// Layout callback - takes &mut RefAny and &mut LayoutCallbackInfo
pub type LayoutCallbackType = extern "C" fn(&mut RefAny, &mut LayoutCallbackInfo) -> Dom;

/// Marshaled layout callback with extra marshal_data parameter
pub type MarshaledLayoutCallbackType = extern "C" fn(
    /* marshal_data */ &mut RefAny,
    /* app_data */ &mut RefAny,
    &mut LayoutCallbackInfo,
) -> Dom;

/// VirtualView callback
pub type VirtualViewCallbackType =
    extern "C" fn(&mut RefAny, &mut VirtualViewCallbackInfo) -> VirtualViewReturn;

/// Simple callback with immutable reference
pub type OnTextInputCallback = extern "C" fn(&RefAny, &TextInputState) -> OnTextInputReturn;

/// Destructor callback using raw pointer
pub type RefAnyDestructorType = extern "C" fn(*mut c_void);

/// Clone callback using raw pointers
pub type RefCountCopyFnType = extern "C" fn(*const c_void) -> *const c_void;

/// Callback with no return value
pub type VoidCallback = extern "C" fn(&mut RefAny);

/// Callback returning a primitive
pub type BoolCallback = extern "C" fn(&RefAny) -> bool;

/// Text input state
#[repr(C)]
pub struct TextInputState {
    pub text: u64,
}

/// Text input return
#[repr(C)]
pub struct OnTextInputReturn {
    pub update: Update,
}
        "#;

        let types = extract_types_from_source(source);

        // Build a lookup map by type name for easier assertions
        let type_map: std::collections::BTreeMap<&str, &TypeDefinition> =
            types.iter().map(|t| (t.type_name.as_str(), t)).collect();

        // // verify struct extractions
        //
        // Update enum
        let update = type_map.get("Update").expect("Update enum not found");
        assert!(
            matches!(&update.kind, TypeDefKind::Enum { .. }),
            "Update should be an Enum, got {:?}",
            update.kind
        );

        // RefAny struct
        let refany = type_map.get("RefAny").expect("RefAny struct not found");
        assert!(
            matches!(&refany.kind, TypeDefKind::Struct { .. }),
            "RefAny should be a Struct, got {:?}",
            refany.kind
        );

        // // verify callback_typedef extractions - the critical part
        //
        // LayoutCallbackType: fn(&mut RefAny, &mut LayoutCallbackInfo) -> Dom
        let layout_cb = type_map
            .get("LayoutCallbackType")
            .expect("LayoutCallbackType not found");

        match &layout_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 2, "LayoutCallbackType should have 2 args");

                // CRITICAL: First arg should be RefAny with RefMut, NOT "& mut RefAny" with Value
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::RefMut,
                    "First arg of LayoutCallbackType should be RefMut"
                );
                assert!(
                    !args[0].ty.contains('&'),
                    "Type should not contain '&', got: {}",
                    args[0].ty
                );
                assert!(
                    args[0].ty.contains("RefAny"),
                    "First arg base type should contain RefAny, got: {}",
                    args[0].ty
                );

                // Second arg: &mut LayoutCallbackInfo
                assert_eq!(
                    args[1].ref_kind,
                    RefKind::RefMut,
                    "Second arg of LayoutCallbackType should be RefMut"
                );
                assert!(
                    !args[1].ty.contains('&'),
                    "Type should not contain '&', got: {}",
                    args[1].ty
                );
                assert!(
                    args[1].ty.contains("LayoutCallbackInfo"),
                    "Second arg base type should contain LayoutCallbackInfo, got: {}",
                    args[1].ty
                );

                // Return type
                assert!(
                    returns.is_some(),
                    "LayoutCallbackType should have a return type"
                );
                // "Dom", matching the fixture above. The fixture was updated from
                // StyledDom to Dom and this assertion was not, so the test
                // contradicted its own input — asserting a return type its source
                // never declared. Nothing caught it because azul-doc's tests run
                // in no CI job (audit D1).
                assert_eq!(
                    returns.as_deref(),
                    Some("Dom"),
                    "LayoutCallbackType's return must match the fixture's `-> Dom`",
                );
            }
            other => panic!(
                "LayoutCallbackType should be CallbackTypedef, got {:?}",
                other
            ),
        }

        // MarshaledLayoutCallbackType: fn(&mut RefAny, &mut RefAny, &mut LayoutCallbackInfo) ->
        // StyledDom
        let marshaled_cb = type_map
            .get("MarshaledLayoutCallbackType")
            .expect("MarshaledLayoutCallbackType not found");

        match &marshaled_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(
                    args.len(),
                    3,
                    "MarshaledLayoutCallbackType should have 3 args"
                );

                // All 3 args should be RefMut
                for (i, arg) in args.iter().enumerate() {
                    assert_eq!(
                        arg.ref_kind,
                        RefKind::RefMut,
                        "Arg {} of MarshaledLayoutCallbackType should be RefMut, got {:?}",
                        i,
                        arg.ref_kind
                    );
                    assert!(
                        !arg.ty.contains('&'),
                        "Arg {} type should not contain '&', got: {}",
                        i,
                        arg.ty
                    );
                }

                assert!(returns.is_some());
            }
            other => panic!(
                "MarshaledLayoutCallbackType should be CallbackTypedef, got {:?}",
                other
            ),
        }

        // OnTextInputCallback: fn(&RefAny, &TextInputState) -> OnTextInputReturn
        // This tests immutable references (Ref, not RefMut)
        let text_input_cb = type_map
            .get("OnTextInputCallback")
            .expect("OnTextInputCallback not found");

        match &text_input_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 2, "OnTextInputCallback should have 2 args");

                // Both args should be Ref (immutable)
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::Ref,
                    "First arg of OnTextInputCallback should be Ref, got {:?}",
                    args[0].ref_kind
                );
                assert_eq!(
                    args[1].ref_kind,
                    RefKind::Ref,
                    "Second arg of OnTextInputCallback should be Ref, got {:?}",
                    args[1].ref_kind
                );

                // Base types should not contain '&'
                assert!(!args[0].ty.contains('&'), "Type should not contain '&'");
                assert!(!args[1].ty.contains('&'), "Type should not contain '&'");

                assert!(returns.is_some());
            }
            other => panic!(
                "OnTextInputCallback should be CallbackTypedef, got {:?}",
                other
            ),
        }

        // RefAnyDestructorType: fn(*mut c_void)
        // This tests raw mutable pointer (passed by value)
        let destructor_cb = type_map
            .get("RefAnyDestructorType")
            .expect("RefAnyDestructorType not found");

        match &destructor_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1, "RefAnyDestructorType should have 1 arg");

                // *mut c_void should be Value (pointer passed by value)
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::Value,
                    "Raw pointer arg should be Value, got {:?}",
                    args[0].ref_kind
                );

                // The type should contain the full pointer type
                assert!(
                    args[0].ty.contains("*") && args[0].ty.contains("mut"),
                    "Type should contain *mut, got: {}",
                    args[0].ty
                );

                assert!(
                    returns.is_none(),
                    "RefAnyDestructorType should have no return type"
                );
            }
            other => panic!(
                "RefAnyDestructorType should be CallbackTypedef, got {:?}",
                other
            ),
        }

        // RefCountCopyFnType: fn(*const c_void) -> *const c_void
        // This tests raw const pointer
        let clone_cb = type_map
            .get("RefCountCopyFnType")
            .expect("RefCountCopyFnType not found");

        match &clone_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1, "RefCountCopyFnType should have 1 arg");

                // *const c_void should be Value
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::Value,
                    "Raw pointer arg should be Value, got {:?}",
                    args[0].ref_kind
                );
                assert!(
                    args[0].ty.contains("*") && args[0].ty.contains("const"),
                    "Type should contain *const, got: {}",
                    args[0].ty
                );

                // Return type should also be a raw pointer
                assert!(returns.is_some());
                let ret = returns.as_ref().unwrap();
                assert!(
                    ret.contains("*") && ret.contains("const"),
                    "Return type should contain *const, got: {}",
                    ret
                );
            }
            other => panic!(
                "RefCountCopyFnType should be CallbackTypedef, got {:?}",
                other
            ),
        }

        // VoidCallback: fn(&mut RefAny)
        // Callback with no return
        let void_cb = type_map
            .get("VoidCallback")
            .expect("VoidCallback not found");

        match &void_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);
                assert_eq!(args[0].ref_kind, RefKind::RefMut);
                assert!(returns.is_none(), "VoidCallback should have no return");
            }
            other => panic!("VoidCallback should be CallbackTypedef, got {:?}", other),
        }

        // BoolCallback: fn(&RefAny) -> bool
        // Callback with immutable ref and primitive return
        let bool_cb = type_map
            .get("BoolCallback")
            .expect("BoolCallback not found");

        match &bool_cb.kind {
            TypeDefKind::CallbackTypedef { args, returns } => {
                assert_eq!(args.len(), 1);
                assert_eq!(
                    args[0].ref_kind,
                    RefKind::Ref,
                    "BoolCallback arg should be Ref (immutable), got {:?}",
                    args[0].ref_kind
                );
                assert!(
                    returns
                        .as_ref()
                        .map(|s| s.contains("bool"))
                        .unwrap_or(false),
                    "BoolCallback should return bool"
                );
            }
            other => panic!("BoolCallback should be CallbackTypedef, got {:?}", other),
        }
    }

    /// Test that verifies the RefKind is correctly converted to/from string representation
    /// for JSON serialization in api.json
    #[test]
    fn test_refkind_to_json_representation() {
        // This test verifies that when we convert to JSON, we get the expected values

        let source = r#"
            pub type TestCallback = extern "C" fn(
                ref_arg: &Data,
                refmut_arg: &mut Data,
                value_arg: Data,
                ptr_arg: *const Data
            ) -> bool;
        "#;

        let types = extract_types_from_source(source);
        assert_eq!(types.len(), 1);

        match &types[0].kind {
            TypeDefKind::CallbackTypedef { args, .. } => {
                // ref_arg: &Data -> should become "ref" in JSON
                assert_eq!(args[0].ref_kind, RefKind::Ref);
                assert_eq!(args[0].ref_kind.as_str(), "ref");

                // refmut_arg: &mut Data -> should become "refmut" in JSON
                assert_eq!(args[1].ref_kind, RefKind::RefMut);
                assert_eq!(args[1].ref_kind.as_str(), "refmut");

                // value_arg: Data -> should become "value" in JSON
                assert_eq!(args[2].ref_kind, RefKind::Value);
                assert_eq!(args[2].ref_kind.as_str(), "value");

                // ptr_arg: *const Data -> should become "value" in JSON (pointer passed by value)
                assert_eq!(args[3].ref_kind, RefKind::Value);
                assert_eq!(args[3].ref_kind.as_str(), "value");
            }
            _ => panic!("Expected CallbackTypedef"),
        }
    }

    /// Test that parses the actual callbacks.rs file from the workspace
    /// to ensure our extraction works on real code
    #[test]
    fn test_parse_actual_callbacks_file() {
        let callbacks_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("core/src/callbacks.rs");

        if !callbacks_path.exists() {
            // Skip test if file doesn't exist (e.g., in CI without full repo)
            eprintln!(
                "Skipping test_parse_actual_callbacks_file: {:?} not found",
                callbacks_path
            );
            return;
        }

        let content =
            std::fs::read_to_string(&callbacks_path).expect("Failed to read callbacks.rs");

        let syntax_tree: syn::File =
            syn::parse_file(&content).expect("Failed to parse callbacks.rs");

        let mut callback_typedefs = Vec::new();

        for item in &syntax_tree.items {
            if let syn::Item::Type(type_item) = item {
                if let syn::Type::BareFn(_) = &*type_item.ty {
                    if let Some(td) =
                        extract_type_alias("azul_core", "callbacks", &callbacks_path, type_item)
                    {
                        callback_typedefs.push(td);
                    }
                }
            }
        }

        // We should find at least some callback typedefs
        assert!(
            !callback_typedefs.is_empty(),
            "Should find callback typedefs in callbacks.rs"
        );

        // Verify that LayoutCallbackType is correctly parsed
        let layout_cb = callback_typedefs
            .iter()
            .find(|t| t.type_name == "LayoutCallbackType");

        if let Some(layout) = layout_cb {
            match &layout.kind {
                TypeDefKind::CallbackTypedef { args, .. } => {
                    assert!(
                        args.len() >= 2,
                        "LayoutCallbackType should have at least 2 args"
                    );

                    // THE INVARIANT, and the bug this test was written for: the
                    // reference-ness of an argument belongs in `ref_kind`, never
                    // smuggled into the type STRING. A parser that reports
                    // `Value` + "& mut RefAny" emits a C signature taking a
                    // by-value `AzRefAny` named `& mut RefAny`, which does not
                    // compile.
                    //
                    // This deliberately no longer pins arg[0] to RefMut. The real
                    // signature is now
                    //   extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom
                    // i.e. RefAny is passed BY VALUE, so `Value` is the correct
                    // answer and the old assertion had been failing ever since
                    // that change — invisibly, because azul-doc's tests run in no
                    // CI job (audit D1). Pinning one callback's arity/ref-ness
                    // just re-breaks the moment the API moves again; the sigil
                    // invariant is what actually has to hold.
                    for (i, a) in args.iter().enumerate() {
                        assert!(
                            !a.ty.starts_with('&'),
                            "LayoutCallbackType arg[{i}] carries its reference in the TYPE ({:?}) \
                             instead of in ref_kind ({:?})",
                            a.ty,
                            a.ref_kind,
                        );
                    }
                }
                _ => panic!("LayoutCallbackType should be CallbackTypedef"),
            }
        }

        // Every callback typedef in the real file, not just the one above: the
        // sigil invariant is global, and checking one type would let the next
        // regression hide in any of the others.
        for cb in &callback_typedefs {
            if let TypeDefKind::CallbackTypedef { args, .. } = &cb.kind {
                for (i, a) in args.iter().enumerate() {
                    assert!(
                        !a.ty.starts_with('&'),
                        "{} arg[{i}] carries its reference in the TYPE ({:?}) instead of in \
                         ref_kind ({:?})",
                        cb.type_name,
                        a.ty,
                        a.ref_kind,
                    );
                }
            }
        }

        eprintln!(
            "Found {} callback typedefs in callbacks.rs",
            callback_typedefs.len()
        );
        for cb in &callback_typedefs {
            eprintln!("  - {}", cb.type_name);
        }
    }

    #[test]
    fn test_extract_into_bounds_rewrites_generic_callback_arg() {
        // Test that fn foo<C: Into<Callback>>(callback: C) extracts arg type as "Callback"
        let source = r#"
            pub struct TestType;
            
            impl TestType {
                pub fn with_callback<C: Into<Callback>>(self, event: EventFilter, data: RefAny, callback: C) -> Self {
                    self
                }
            }
        "#;

        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse");

        // Find the impl block and extract methods
        for item in &syntax_tree.items {
            if let Item::Impl(impl_block) = item {
                for impl_item in &impl_block.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        let method_def =
                            extract_method_def(method, "TestType").expect("Should extract method");

                        assert_eq!(method_def.name, "with_callback");
                        assert_eq!(
                            method_def.args.len(),
                            3,
                            "Should have 3 args: event, data, callback"
                        );

                        // The callback arg should have type "Callback", not "C"
                        let callback_arg = &method_def.args[2];
                        assert_eq!(callback_arg.name, "callback");
                        assert_eq!(
                            callback_arg.ty, "Callback",
                            "Generic C: Into<Callback> should be rewritten to 'Callback', got '{}'",
                            callback_arg.ty
                        );

                        eprintln!("✓ Generic Into<T> bound correctly rewritten:");
                        eprintln!(
                            "  callback arg: type={}, ref_kind={:?}",
                            callback_arg.ty, callback_arg.ref_kind
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn test_extract_into_bounds_where_clause() {
        // Test where clause syntax: fn foo<C>(callback: C) where C: Into<Callback>
        let source = r#"
            pub struct TestType;
            
            impl TestType {
                pub fn add_callback<C>(&mut self, callback: C) where C: Into<CoreCallback> {
                }
            }
        "#;

        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse");

        for item in &syntax_tree.items {
            if let Item::Impl(impl_block) = item {
                for impl_item in &impl_block.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        let method_def =
                            extract_method_def(method, "TestType").expect("Should extract method");

                        assert_eq!(method_def.name, "add_callback");
                        assert_eq!(method_def.args.len(), 1);

                        let callback_arg = &method_def.args[0];
                        assert_eq!(callback_arg.name, "callback");
                        assert_eq!(
                            callback_arg.ty, "CoreCallback",
                            "Where clause C: Into<CoreCallback> should be rewritten to \
                             'CoreCallback', got '{}'",
                            callback_arg.ty
                        );

                        eprintln!("✓ Where clause Into<T> correctly rewritten:");
                        eprintln!("  callback arg: type={}", callback_arg.ty);
                    }
                }
            }
        }
    }

    #[test]
    fn test_non_into_generic_is_skipped_entirely() {
        // A generic parameter that is NOT an `Into<T>` bound cannot be expressed
        // in the C API at all, so `extract_method_def` skips the whole method —
        // see its own comment: "This is a truly generic method that cannot be
        // exported to C API".
        //
        // This test used to `.expect("Should extract method")` and then assert
        // the argument type had been left as "T". That was the OLD behaviour,
        // from when such a method was extracted-but-not-rewritten. Skipping is
        // strictly stronger: a method that is never extracted can never be
        // emitted with an unexpressible signature. The test asserted the weaker
        // guarantee and has been failing since the behaviour tightened, unnoticed
        // because azul-doc's tests run in no CI job (audit D1).
        let source = r#"
            pub struct TestType;

            impl TestType {
                pub fn clone_item<T: Clone>(item: T) -> T {
                    item
                }
            }
        "#;

        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse");

        let mut saw_method = false;
        for item in &syntax_tree.items {
            if let Item::Impl(impl_block) = item {
                for impl_item in &impl_block.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        saw_method = true;
                        assert!(
                            extract_method_def(method, "TestType").is_none(),
                            "`fn clone_item<T: Clone>` has a generic that is not an Into<T> \
                             bound, so it cannot be exported to the C API and must not be \
                             extracted",
                        );
                    }
                }
            }
        }
        assert!(
            saw_method,
            "the fixture parsed to no impl method, so this test asserted nothing",
        );
    }

    #[test]
    fn test_concrete_callback_type_unchanged() {
        // Test that concrete Callback types are not affected
        let source = r#"
            pub struct TestType;
            
            impl TestType {
                pub fn with_callback(self, callback: Callback) -> Self {
                    self
                }
            }
        "#;

        let syntax_tree: File = syn::parse_file(source).expect("Failed to parse");

        for item in &syntax_tree.items {
            if let Item::Impl(impl_block) = item {
                for impl_item in &impl_block.items {
                    if let syn::ImplItem::Fn(method) = impl_item {
                        let method_def =
                            extract_method_def(method, "TestType").expect("Should extract method");

                        assert_eq!(method_def.name, "with_callback");
                        let callback_arg = &method_def.args[0];
                        assert_eq!(callback_arg.name, "callback");
                        assert_eq!(
                            callback_arg.ty, "Callback",
                            "Concrete Callback should remain 'Callback', got '{}'",
                            callback_arg.ty
                        );

                        eprintln!("✓ Concrete Callback type preserved");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod opaque_handle_fields {
    //! The struct-field mapper's treatment of opaque handles. Pinned because the
    //! gap it closes reached api.json through `autofix` and broke every non-C
    //! binding, and nothing else in the pipeline checks the mapper on its own.
    use super::*;

    fn map(src: &str) -> (String, RefKind) {
        extract_struct_field_type(&syn::parse_str::<syn::Type>(src).expect("a valid type"))
    }

    fn is_c_void(res: &(String, RefKind), kind: RefKind) -> bool {
        res.0 == "c_void" && res.1 == kind
    }

    #[test]
    fn manually_drop_box_is_an_opaque_mut_pointer() {
        // The three fields that leaked into api.json as raw Rust types, plus the
        // fully-qualified spelling.
        for src in [
            "ManuallyDrop<Box<IconProviderInner>>",
            "ManuallyDrop<Box<CssPropertyCache>>",
            "ManuallyDrop<Box<Rc<GlContextPtrInner>>>",
            "core::mem::ManuallyDrop<Box<X>>",
        ] {
            let res = map(src);
            assert!(
                is_c_void(&res, RefKind::MutPtr),
                "{src} mapped to {:?}",
                res.0
            );
        }
    }

    #[test]
    fn the_pointer_rules_are_unchanged() {
        assert!(is_c_void(&map("Box<X>"), RefKind::MutPtr));
        assert!(is_c_void(&map("Option<Box<X>>"), RefKind::MutPtr));
        assert!(is_c_void(&map("*mut X"), RefKind::MutPtr));
        assert!(is_c_void(&map("*const X"), RefKind::ConstPtr));
    }

    #[test]
    fn a_manually_drop_value_is_still_its_inner_type() {
        // Peeling the wrapper must not turn a by-value field into a pointer.
        let res = map("ManuallyDrop<Foo>");
        assert_eq!(res.0, "Foo");
        assert!(res.1 == RefKind::Value);
    }
}
