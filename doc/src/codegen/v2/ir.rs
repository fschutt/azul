//! Intermediate Representation (IR) for code generation
//!
//! This module defines the data structures that represent the complete
//! API surface in a language-agnostic way. The IR is built from api.json
//! and then consumed by language-specific generators.

use indexmap::IndexMap;
use std::collections::BTreeMap;

// ============================================================================
// Top-level IR
// ============================================================================

/// Complete IR built from api.json
///
/// This is the "source of truth" for all code generation.
/// Language-specific generators consume this IR with different configurations.
#[derive(Debug, Clone)]
pub struct CodegenIR {
    /// All struct definitions
    pub structs: Vec<StructDef>,

    /// All enum definitions
    pub enums: Vec<EnumDef>,

    /// All functions (including trait functions like _deepCopy, _delete)
    pub functions: Vec<FunctionDef>,

    /// Type aliases
    pub type_aliases: Vec<TypeAliasDef>,

    /// Constants
    pub constants: Vec<ConstantDef>,

    /// Callback typedefs (function pointer types)
    pub callback_typedefs: Vec<CallbackTypedefDef>,

    /// Lookup table: type name → module name
    pub type_to_module: BTreeMap<String, String>,

    /// Lookup table: type name → external path (e.g., "azul_core::dom::Dom")
    pub type_to_external: BTreeMap<String, String>,
}

impl CodegenIR {
    /// Create an empty IR
    pub fn new() -> Self {
        Self {
            structs: Vec::new(),
            enums: Vec::new(),
            functions: Vec::new(),
            type_aliases: Vec::new(),
            constants: Vec::new(),
            callback_typedefs: Vec::new(),
            type_to_module: BTreeMap::new(),
            type_to_external: BTreeMap::new(),
        }
    }

    /// Get all types (structs + enums) in definition order
    pub fn all_types(&self) -> impl Iterator<Item = TypeDef<'_>> {
        let structs = self.structs.iter().map(TypeDef::Struct);
        let enums = self.enums.iter().map(TypeDef::Enum);
        structs.chain(enums)
    }

    /// Find a struct by name
    pub fn find_struct(&self, name: &str) -> Option<&StructDef> {
        self.structs.iter().find(|s| s.name == name)
    }

    /// Find an enum by name
    pub fn find_enum(&self, name: &str) -> Option<&EnumDef> {
        self.enums.iter().find(|e| e.name == name)
    }

    /// Find a type alias by name
    pub fn find_type_alias(&self, name: &str) -> Option<&TypeAliasDef> {
        self.type_aliases.iter().find(|t| t.name == name)
    }

    /// Find functions for a specific class
    pub fn functions_for_class<'a>(
        &'a self,
        class_name: &'a str,
    ) -> impl Iterator<Item = &'a FunctionDef> + 'a {
        self.functions
            .iter()
            .filter(move |f| f.class_name == class_name)
    }

    /// Get all trait functions (deepCopy, delete, partialEq, etc.)
    pub fn trait_functions(&self) -> impl Iterator<Item = &FunctionDef> {
        self.functions.iter().filter(|f| f.kind.is_trait_function())
    }
}

impl Default for CodegenIR {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference to either a struct or enum definition
#[derive(Debug, Clone, Copy)]
pub enum TypeDef<'a> {
    Struct(&'a StructDef),
    Enum(&'a EnumDef),
}

impl<'a> TypeDef<'a> {
    pub fn name(&self) -> &str {
        match self {
            TypeDef::Struct(s) => &s.name,
            TypeDef::Enum(e) => &e.name,
        }
    }

    pub fn derives(&self) -> &[String] {
        match self {
            TypeDef::Struct(s) => &s.derives,
            TypeDef::Enum(e) => &e.derives,
        }
    }

    pub fn external_path(&self) -> Option<&str> {
        match self {
            TypeDef::Struct(s) => s.external_path.as_deref(),
            TypeDef::Enum(e) => e.external_path.as_deref(),
        }
    }
}

// ============================================================================
// Struct Definition
// ============================================================================

/// A struct definition from api.json
#[derive(Debug, Clone)]
pub struct StructDef {
    /// Type name without prefix (e.g., "Dom", "LayoutCallback")
    pub name: String,

    /// Documentation lines
    pub doc: Vec<String>,

    /// Struct fields
    pub fields: Vec<FieldDef>,

    /// External crate path (e.g., "azul_core::dom::Dom")
    pub external_path: Option<String>,

    /// Module this type belongs to
    pub module: String,

    /// Derive macros from api.json
    pub derives: Vec<String>,

    /// Whether derive was explicitly set (even if empty)
    /// When true and derives is empty, no auto-derives will be generated
    pub has_explicit_derive: bool,

    /// Traits with manual implementations (e.g., ["Clone", "Drop"])
    pub custom_impls: Vec<String>,

    /// Whether this is a boxed/heap-allocated type
    pub is_boxed: bool,

    /// Repr attribute (e.g., "C", "transparent")
    pub repr: Option<String>,

    /// Whether this type is safe to implement Send + Sync
    /// True for Vec-like types that use internal pointers but are semantically safe
    /// (like Rust's Vec<T> which is Send+Sync when T is)
    pub is_send_safe: bool,

    /// Generic type parameters (e.g., ["T"] for Option<T>)
    pub generic_params: Vec<String>,

    /// Trait capabilities computed from derives
    pub traits: TypeTraits,

    /// Type category for code generation
    /// Determines how this type is handled by language generators
    pub category: TypeCategory,

    // === C/C++ ordering fields (populated by analyze_dependencies pass) ===
    /// Types this struct depends on (field types, excluding primitives)
    /// Used by C/C++ backends for topological sorting
    pub dependencies: Vec<String>,

    /// Sort order for C/C++ output (lower = earlier)
    /// Computed by topological sort of dependencies
    pub sort_order: usize,

    /// Whether this type needs a forward declaration in C/C++
    /// True if the type is referenced before its definition
    pub needs_forward_decl: bool,

    /// If this is a callback wrapper struct, contains info about the wrapped callback
    /// A callback wrapper has:
    /// - A field with a callback_typedef type (the function pointer)
    /// - A "callable" field with type "OptionRefAny" (storage for the callable)
    pub callback_wrapper_info: Option<CallbackWrapperInfo>,
}

/// Information about a callback wrapper struct
///
/// Callback wrappers pair a function pointer (callback_typedef) with
/// optional data (OptionRefAny). In Python, the user passes a Py<PyAny>
/// callable, which gets stored in `callable`/`ctx` and invoked via a trampoline.
#[derive(Debug, Clone)]
pub struct CallbackWrapperInfo {
    /// The name of the callback_typedef this wrapper contains
    /// e.g., "IFrameCallbackType", "ButtonOnClickCallbackType"
    pub callback_typedef_name: String,

    /// The field name that holds the callback (usually "cb")
    pub callback_field_name: String,

    /// The field name that holds the context/callable (usually "ctx" or "callable")
    pub context_field_name: String,
}

/// Struct field definition
#[derive(Debug, Clone)]
pub struct FieldDef {
    /// Field name
    pub name: String,

    /// Field type (without prefix, e.g., "Dom", "Option<String>")
    pub type_name: String,

    /// Documentation
    pub doc: Option<String>,

    /// Whether this field is public
    pub is_public: bool,

    /// Reference kind (owned, ref, ref_mut, pointer)
    pub ref_kind: FieldRefKind,
}

/// Reference kind for struct fields
/// Maps to api.json's ref_kind field values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldRefKind {
    /// `T` (by value)
    #[default]
    Owned,
    /// `&T` - immutable reference
    Ref,
    /// `&mut T` - mutable reference
    RefMut,
    /// `*const T` - const raw pointer
    Ptr,
    /// `*mut T` - mutable raw pointer
    PtrMut,
    /// `Box<T>` - owned heap pointer
    Boxed,
    /// `Option<Box<T>>` - optional owned heap pointer
    OptionBoxed,
}

// ============================================================================
// Enum Definition
// ============================================================================

/// An enum definition from api.json
#[derive(Debug, Clone)]
pub struct EnumDef {
    /// Type name without prefix (e.g., "LayoutAxis", "OptionDom")
    pub name: String,

    /// Documentation lines
    pub doc: Vec<String>,

    /// Enum variants
    pub variants: Vec<EnumVariantDef>,

    /// External crate path
    pub external_path: Option<String>,

    /// Module this type belongs to
    pub module: String,

    /// Derive macros
    pub derives: Vec<String>,

    /// Whether derive was explicitly set
    pub has_explicit_derive: bool,

    /// Whether this is a "union" enum (has data in variants)
    pub is_union: bool,

    /// Repr attribute
    pub repr: Option<String>,

    /// Whether this type is safe to implement Send + Sync
    /// True for Vec-like types that use internal pointers but are semantically safe
    pub is_send_safe: bool,

    /// Trait capabilities
    pub traits: TypeTraits,

    /// Generic type parameters (e.g., ["T"] for CssPropertyValue<T>)
    pub generic_params: Vec<String>,

    /// Type category for code generation
    /// Determines how this type is handled by language generators
    pub category: TypeCategory,

    // === C/C++ ordering fields (populated by analyze_dependencies pass) ===
    /// Types this enum depends on (variant payload types, excluding primitives)
    /// Used by C/C++ backends for topological sorting
    pub dependencies: Vec<String>,

    /// Sort order for C/C++ output (lower = earlier)
    /// Computed by topological sort of dependencies
    pub sort_order: usize,

    /// Whether this type needs a forward declaration in C/C++
    /// True if the type is referenced before its definition
    pub needs_forward_decl: bool,
}

/// Enum variant definition
#[derive(Debug, Clone)]
pub struct EnumVariantDef {
    /// Variant name (e.g., "None", "Some")
    pub name: String,

    /// Documentation
    pub doc: Option<String>,

    /// Variant kind (unit, tuple, or struct)
    pub kind: EnumVariantKind,
}

/// Kind of enum variant
#[derive(Debug, Clone)]
pub enum EnumVariantKind {
    /// Unit variant: `None`
    Unit,

    /// Tuple variant with (type_name, ref_kind) pairs: `Some(T)`, `Boxed(*mut T)`
    Tuple(Vec<(String, FieldRefKind)>),

    /// Struct variant with named fields: `Point { x: f32, y: f32 }`
    Struct(Vec<FieldDef>),
}

// ============================================================================
// Function Definition
// ============================================================================

/// A function definition
///
/// This includes:
/// - Regular methods and constructors from api.json
/// - Generated trait functions (_deepCopy, _delete, _partialEq, etc.)
#[derive(Debug, Clone)]
pub struct FunctionDef {
    /// C-ABI function name (e.g., "AzDom_new", "AzDom_deepCopy")
    pub c_name: String,

    /// Class this function belongs to (without prefix)
    pub class_name: String,

    /// Method name (e.g., "new", "deepCopy")
    pub method_name: String,

    /// Function kind
    pub kind: FunctionKind,

    /// Function arguments
    pub args: Vec<FunctionArg>,

    /// Return type (None for void)
    pub return_type: Option<String>,

    /// Function body from api.json (for InternalBindings mode)
    pub fn_body: Option<String>,

    /// Documentation
    pub doc: Vec<String>,

    /// Whether this function is const
    pub is_const: bool,

    /// Whether this function is unsafe
    pub is_unsafe: bool,
}

/// Kind of function
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionKind {
    /// Constructor: `fn new(...) -> Self`
    Constructor,

    /// Instance method: `fn method(&self, ...) -> ...`
    Method,

    /// Mutable instance method: `fn method(&mut self, ...) -> ...`
    MethodMut,

    /// Static method: `fn static_method(...) -> ...`
    StaticMethod,

    // === Auto-generated trait functions ===
    /// Drop::drop: `fn _delete(&mut self)`
    Delete,

    /// Clone::clone: `fn _deepCopy(&self) -> Self`
    DeepCopy,

    /// PartialEq::eq: `fn _partialEq(&self, other: &Self) -> bool`
    PartialEq,

    /// PartialOrd::partial_cmp: `fn _partialCmp(&self, other: &Self) -> u8`
    PartialCmp,

    /// Ord::cmp: `fn _cmp(&self, other: &Self) -> u8`
    Cmp,

    /// Hash::hash: `fn _hash(&self) -> u64`
    Hash,

    /// Default::default: `fn _default() -> Self`
    Default,

    /// fmt::Debug: `fn _toDbgString(&self) -> AzString`
    DebugToString,

    /// Enum variant constructor: `fn EnumName_variantName(payload) -> EnumName`
    /// Generated automatically for each enum variant
    EnumVariantConstructor,
}

impl FunctionKind {
    /// Check if this is a trait-generated function that should be skipped in method generation
    /// (delete, partialEq, etc.)
    /// Note: Default is NOT skipped - it's generated as a static constructor
    /// Note: DeepCopy is NOT skipped - it's generated as clone() method
    pub fn is_trait_function(&self) -> bool {
        matches!(
            self,
            FunctionKind::Delete
                | FunctionKind::PartialEq
                | FunctionKind::PartialCmp
                | FunctionKind::Cmp
                | FunctionKind::Hash
                | FunctionKind::DebugToString
        )
    }

    /// Check if this is a Default constructor (should be generated as static method)
    pub fn is_default_constructor(&self) -> bool {
        matches!(self, FunctionKind::Default)
    }

    /// Check if this is a DeepCopy method (should be generated as clone())
    pub fn is_clone_method(&self) -> bool {
        matches!(self, FunctionKind::DeepCopy)
    }

    /// Get the trait name for this function kind
    pub fn trait_name(&self) -> Option<&'static str> {
        match self {
            FunctionKind::Delete => Some("Drop"),
            FunctionKind::DeepCopy => Some("Clone"),
            FunctionKind::PartialEq => Some("PartialEq"),
            FunctionKind::PartialCmp => Some("PartialOrd"),
            FunctionKind::Cmp => Some("Ord"),
            FunctionKind::Hash => Some("Hash"),
            FunctionKind::Default => Some("Default"),
            FunctionKind::DebugToString => Some("Debug"),
            _ => None,
        }
    }

    /// Get the method suffix for C-ABI name
    pub fn c_suffix(&self) -> &'static str {
        match self {
            FunctionKind::Delete => "_delete",
            FunctionKind::DeepCopy => "_clone",
            FunctionKind::PartialEq => "_partialEq",
            FunctionKind::PartialCmp => "_partialCmp",
            FunctionKind::Cmp => "_cmp",
            FunctionKind::Hash => "_hash",
            FunctionKind::Default => "_default",
            FunctionKind::DebugToString => "_toDbgString",
            _ => "",
        }
    }
}

/// Function argument
#[derive(Debug, Clone)]
pub struct FunctionArg {
    /// Argument name
    pub name: String,

    /// Argument type (without prefix)
    pub type_name: String,

    /// Reference kind
    pub ref_kind: ArgRefKind,

    /// Documentation
    pub doc: Option<String>,

    /// If this argument is a callback typedef type (e.g., CallbackType, ButtonOnClickCallbackType),
    /// this contains information about the callback for language generators.
    ///
    /// This is set when:
    /// - The argument type ends with "CallbackType"
    /// - The type is registered as a callback_typedef in api.json
    ///
    /// Python generator uses this to:
    /// 1. Accept `Py<PyAny>` callable instead of the function pointer
    /// 2. Generate trampoline code to invoke the Python callable
    /// 3. Wrap the callable in the callback wrapper struct (e.g., ButtonOnClickCallback)
    pub callback_info: Option<CallbackArgInfo>,
}

/// Information about a callback argument
///
/// When a function takes a callback typedef type (e.g., `callback: CallbackType`),
/// this struct contains the information needed for language generators to
/// properly handle the callback.
#[derive(Debug, Clone)]
pub struct CallbackArgInfo {
    /// The name of the callback typedef type (e.g., "CallbackType", "ButtonOnClickCallbackType")
    pub callback_typedef_name: String,

    /// The name of the callback wrapper struct (e.g., "Callback", "ButtonOnClickCallback")
    /// This is typically the typedef name with "Type" stripped: "ButtonOnClickCallbackType" → "ButtonOnClickCallback"
    /// Some wrappers use different conventions (e.g., "Callback" for "CallbackType" becomes "CoreCallback")
    pub callback_wrapper_name: String,

    /// The name of the trampoline function to use (e.g., "invoke_py_callback", "invoke_py_button_on_click_callback")
    pub trampoline_name: String,
}

/// Reference kind for function arguments
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgRefKind {
    /// Owned: `arg: T`
    Owned,
    /// Reference: `arg: &T`
    Ref,
    /// Mutable reference: `arg: &mut T`
    RefMut,
    /// Pointer: `arg: *const T`
    Ptr,
    /// Mutable pointer: `arg: *mut T`
    PtrMut,
}

// ============================================================================
// Type Alias Definition
// ============================================================================

/// Type alias definition (e.g., `type GLenum = u32;`)
///
/// For generic type aliases like `CaretColorValue = CssPropertyValue<CaretColor>`,
/// the `monomorphized_def` field contains the instantiated enum/struct definition.
#[derive(Debug, Clone)]
pub struct TypeAliasDef {
    /// Alias name
    pub name: String,

    /// Target type (e.g., "u32" or "CssPropertyValue")
    pub target: String,

    /// Generic arguments for the target type (e.g., ["CaretColor"] for CssPropertyValue<CaretColor>)
    pub generic_args: Vec<String>,

    /// Documentation
    pub doc: Vec<String>,

    /// Module
    pub module: String,

    /// External path (from api.json "external" field)
    pub external_path: Option<String>,

    /// Trait capabilities (inherited from target type or explicit in api.json)
    pub traits: TypeTraits,

    // === C/C++ ordering fields (populated by analyze_dependencies pass) ===
    /// If this is a generic type alias, this contains the monomorphized enum definition
    /// For example, `CaretColorValue = CssPropertyValue<CaretColor>` becomes a concrete enum
    /// with variants like Auto, None, Inherit, Initial, Exact(CaretColor)
    pub monomorphized_def: Option<MonomorphizedTypeDef>,

    /// Types this alias depends on
    pub dependencies: Vec<String>,

    /// Sort order for C/C++ output
    pub sort_order: usize,
}

/// A monomorphized type definition created from a generic type alias
///
/// When a type alias points to a generic type (e.g., CssPropertyValue<T>),
/// we need to instantiate it with concrete types for C/C++ code generation.
#[derive(Debug, Clone)]
pub struct MonomorphizedTypeDef {
    /// The kind of monomorphized type (enum union, simple enum, or struct)
    pub kind: MonomorphizedKind,
}

/// The kind of monomorphized type
#[derive(Debug, Clone)]
pub enum MonomorphizedKind {
    /// A tagged union (enum with data in variants)
    /// Contains the tag enum name and the variants
    TaggedUnion {
        /// The repr attribute (e.g., "C, u8")
        repr: Option<String>,
        /// The variants of the enum
        variants: Vec<MonomorphizedVariant>,
    },
    /// A simple enum (no data in variants)
    SimpleEnum {
        repr: Option<String>,
        variants: Vec<String>,
    },
    /// A struct
    Struct { fields: Vec<FieldDef> },
}

/// A variant in a monomorphized tagged union
#[derive(Debug, Clone)]
pub struct MonomorphizedVariant {
    /// Variant name (e.g., "Auto", "Exact")
    pub name: String,
    /// Payload type if any (already substituted with concrete types)
    pub payload_type: Option<String>,
    /// Reference kind for payload (pointer types in variants like BoxOrStatic)
    pub payload_ref_kind: FieldRefKind,
}

// ============================================================================
// Constant Definition
// ============================================================================

/// Constant definition (e.g., `const GL_TRUE: GLboolean = 1;`)
#[derive(Debug, Clone)]
pub struct ConstantDef {
    /// Constant name
    pub name: String,

    /// Constant type
    pub type_name: String,

    /// Constant value (as string literal)
    pub value: String,

    /// Documentation
    pub doc: Vec<String>,

    /// Module
    pub module: String,
}

// ============================================================================
// Callback Typedef Definition
// ============================================================================

/// Callback function pointer type definition
///
/// e.g., `type LayoutCallbackType = extern "C" fn(&mut RefAny, &LayoutCallbackInfo) -> StyledDom;`
#[derive(Debug, Clone)]
pub struct CallbackTypedefDef {
    /// Type name (e.g., "LayoutCallbackType")
    pub name: String,

    /// Function arguments
    pub args: Vec<FunctionArg>,

    /// Return type (None for void)
    pub return_type: Option<String>,

    /// Documentation
    pub doc: Vec<String>,

    /// Module
    pub module: String,

    /// External path (e.g., "azul_core::callbacks::LayoutCallbackType")
    pub external_path: Option<String>,

    // === C/C++ ordering fields (populated by analyze_dependencies pass) ===
    /// Types this callback depends on (argument types and return type, excluding primitives)
    /// Used by C/C++ backends for topological sorting
    pub dependencies: Vec<String>,

    /// Sort order for C/C++ output (lower = earlier)
    /// Computed by topological sort of dependencies
    pub sort_order: usize,
}

// ============================================================================
// Type Category (Central type classification)
// ============================================================================

/// Type category for code generation
///
/// This is the central type classification system that replaces all ad-hoc
/// checks with a single source of truth. Language generators use this to
/// determine how to handle each type.
///
/// The category is computed from api.json properties during IR building.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TypeCategory {
    /// Recursive types that cause "infinite size" errors
    /// These types are skipped entirely in Python (no wrapper, no methods)
    /// Examples: XmlNode, Xml, XmlNodeChildVec
    Recursive,

    /// VecRef types - raw pointer slice wrappers
    /// These need special trampolines and are skipped in Python for now
    /// Examples: U8VecRef, GLuintVecRef, Refstr
    VecRef,

    /// Primitive types and their type aliases
    /// Used directly without wrappers
    /// Examples: u8, f32, GLuint (alias for u32), c_void
    Primitive,

    /// String type - uses C-API directly with special conversion
    /// Has From<String>/Into<AzString> and PyO3 traits on C-API type
    /// No separate Python wrapper struct
    String,

    /// Vec types (U8Vec, StringVec, etc.) - use C-API directly
    /// Has special conversion traits
    /// No separate Python wrapper struct
    Vec,

    /// RefAny type - opaque pointer to Python object
    /// Used for storing Python callables in callbacks
    /// No separate wrapper, uses C-API type directly
    RefAny,

    /// Callback typedef - raw function pointer type
    /// Examples: LayoutCallbackType, CallbackType
    /// Not directly usable from Python, needs trampoline
    CallbackTypedef,

    /// Callback+Data pair struct - callback field + RefAny data field
    /// Needs custom Python wrapper with PyObject storage
    /// Examples: Callback, IFrameCallback, LayoutCallback
    CallbackDataPair,

    /// Boxed/heap-allocated pointer wrapper
    /// Internal types, skipped in Python
    /// Examples: types with is_boxed_object: true
    Boxed,

    /// Generic template type (has type parameters)
    /// Cannot be instantiated directly in Python
    /// Examples: Option<T>, CssPropertyValue<T>
    GenericTemplate,

    /// Type alias that resolves to another type
    /// May generate a type alias or be skipped depending on target
    TypeAlias,

    /// Destructor/Clone callback types - internal use only
    /// Skipped in Python bindings
    DestructorOrClone,

    /// Regular struct or enum - gets full Python wrapper treatment
    /// - #[pyclass] wrapper struct with .inner field
    /// - From/Into impls for C-API conversion
    /// - #[pymethods] with transmute to external types
    #[default]
    Regular,
}

impl TypeCategory {
    /// Check if this type should get a Python wrapper struct
    /// Returns false for types that use C-API types directly
    pub fn needs_python_wrapper(&self) -> bool {
        matches!(self, TypeCategory::Regular | TypeCategory::CallbackDataPair)
    }

    /// Check if this type should be completely skipped in Python
    pub fn skip_in_python(&self) -> bool {
        matches!(
            self,
            TypeCategory::Recursive
                | TypeCategory::VecRef
                | TypeCategory::GenericTemplate
                | TypeCategory::DestructorOrClone
                | TypeCategory::CallbackTypedef
        )
    }

    /// Check if this type uses the C-API type directly (no wrapper)
    pub fn uses_capi_directly(&self) -> bool {
        matches!(
            self,
            TypeCategory::Primitive
                | TypeCategory::String
                | TypeCategory::Vec
                | TypeCategory::RefAny
        )
    }

    /// Check if this is a callback-related type that needs trampolines
    pub fn is_callback_related(&self) -> bool {
        matches!(
            self,
            TypeCategory::CallbackTypedef | TypeCategory::CallbackDataPair
        )
    }

    /// Human-readable description for debugging
    pub fn description(&self) -> &'static str {
        match self {
            TypeCategory::Recursive => "recursive (infinite size)",
            TypeCategory::VecRef => "VecRef (raw slice pointer)",
            TypeCategory::Primitive => "primitive type",
            TypeCategory::String => "string type (AzString)",
            TypeCategory::Vec => "vec type (AzU8Vec, etc.)",
            TypeCategory::RefAny => "RefAny (opaque callback data)",
            TypeCategory::CallbackTypedef => "callback typedef (fn pointer)",
            TypeCategory::CallbackDataPair => "callback+data pair",
            TypeCategory::Boxed => "boxed/heap pointer",
            TypeCategory::GenericTemplate => "generic template",
            TypeCategory::TypeAlias => "type alias",
            TypeCategory::DestructorOrClone => "destructor/clone callback",
            TypeCategory::Regular => "regular type",
        }
    }
}

// ============================================================================
// Type Traits (Computed from derives)
// ============================================================================

/// Trait capabilities for a type, computed from api.json derives
#[derive(Debug, Clone, Default)]
pub struct TypeTraits {
    /// Can be copied (implements Copy)
    pub is_copy: bool,

    /// Can be cloned (implements Clone or has _deepCopy)
    pub is_clone: bool,

    /// Clone should be derived (vs manual impl via custom_impls)
    pub clone_is_derived: bool,

    /// Can be compared for equality (implements PartialEq)
    pub is_partial_eq: bool,

    /// Can be fully compared for equality (implements Eq)
    pub is_eq: bool,

    /// Can be partially ordered (implements PartialOrd)
    pub is_partial_ord: bool,

    /// Can be fully ordered (implements Ord)
    pub is_ord: bool,

    /// Can be hashed (implements Hash)
    pub is_hash: bool,

    /// Has a default value (implements Default)
    pub is_default: bool,

    /// Can be debug-printed (implements Debug)
    pub is_debug: bool,

    /// Has a custom destructor
    pub has_custom_drop: bool,

    /// Can be serialized with serde
    pub is_serialize: bool,

    /// Can be deserialized with serde
    pub is_deserialize: bool,
}

impl TypeTraits {
    /// Build from a list of derive names and custom_impls
    pub fn from_derives(derives: &[String], has_custom_drop: bool) -> Self {
        Self::from_derives_and_custom_impls(derives, &[], has_custom_drop)
    }

    /// Build from derives and custom_impls lists
    pub fn from_derives_and_custom_impls(
        derives: &[String],
        custom_impls: &[String],
        has_custom_drop: bool,
    ) -> Self {
        let mut traits = Self::default();
        traits.has_custom_drop = has_custom_drop;

        // First process derives - these are traits that should be auto-derived
        for derive in derives.iter() {
            match derive.as_str() {
                "Copy" => {
                    traits.is_copy = true;
                    traits.is_clone = true; // Copy implies Clone
                    traits.clone_is_derived = true;
                }
                "Clone" => {
                    traits.is_clone = true;
                    traits.clone_is_derived = true;
                }
                "PartialEq" => traits.is_partial_eq = true,
                "Eq" => {
                    traits.is_eq = true;
                    traits.is_partial_eq = true; // Eq implies PartialEq
                }
                "PartialOrd" => traits.is_partial_ord = true,
                "Ord" => {
                    traits.is_ord = true;
                    traits.is_partial_ord = true; // Ord implies PartialOrd
                }
                "Hash" => traits.is_hash = true,
                "Default" => traits.is_default = true,
                "Debug" => traits.is_debug = true,
                "Serialize" => traits.is_serialize = true,
                "Deserialize" => traits.is_deserialize = true,
                _ => {}
            }
        }

        // Then process custom_impls - these have manual implementations
        // Clone in custom_impls means the type has Clone but it's manually implemented
        for custom_impl in custom_impls.iter() {
            match custom_impl.as_str() {
                "Copy" => {
                    traits.is_copy = true;
                    traits.is_clone = true;
                    // Don't set clone_is_derived - Copy with custom impl is rare but possible
                }
                "Clone" => {
                    traits.is_clone = true;
                    // Don't set clone_is_derived - it's manually implemented
                }
                "PartialEq" => traits.is_partial_eq = true,
                "Eq" => {
                    traits.is_eq = true;
                    traits.is_partial_eq = true;
                }
                "PartialOrd" => traits.is_partial_ord = true,
                "Ord" => {
                    traits.is_ord = true;
                    traits.is_partial_ord = true;
                }
                "Hash" => traits.is_hash = true,
                "Default" => traits.is_default = true,
                "Debug" => traits.is_debug = true,
                "Serialize" => traits.is_serialize = true,
                "Deserialize" => traits.is_deserialize = true,
                _ => {}
            }
        }

        traits
    }

    /// Check if this type needs a _deepCopy function
    pub fn needs_deep_copy(&self) -> bool {
        self.is_clone && !self.is_copy
    }

    /// Check if this type needs a _delete function
    pub fn needs_delete(&self) -> bool {
        self.has_custom_drop || !self.is_copy
    }
}
