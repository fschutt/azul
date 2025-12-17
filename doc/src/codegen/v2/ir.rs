//! Intermediate Representation (IR) for code generation
//!
//! This module defines the data structures that represent the complete
//! API surface in a language-agnostic way. The IR is built from api.json
//! and then consumed by language-specific generators.

use std::collections::HashMap;
use indexmap::IndexMap;

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
    pub type_to_module: HashMap<String, String>,

    /// Lookup table: type name → external path (e.g., "azul_core::dom::Dom")
    pub type_to_external: HashMap<String, String>,
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
            type_to_module: HashMap::new(),
            type_to_external: HashMap::new(),
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

    /// Find functions for a specific class
    pub fn functions_for_class<'a>(&'a self, class_name: &'a str) -> impl Iterator<Item = &'a FunctionDef> + 'a {
        self.functions.iter().filter(move |f| f.class_name == class_name)
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

    /// Generic type parameters (e.g., ["T"] for Option<T>)
    pub generic_params: Vec<String>,

    /// Trait capabilities computed from derives
    pub traits: TypeTraits,

    /// Type category for code generation
    /// Determines how this type is handled by language generators
    pub category: TypeCategory,
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

    /// Trait capabilities
    pub traits: TypeTraits,

    /// Generic type parameters (e.g., ["T"] for CssPropertyValue<T>)
    pub generic_params: Vec<String>,

    /// Type category for code generation
    /// Determines how this type is handled by language generators
    pub category: TypeCategory,
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

    /// Tuple variant with types: `Some(T)`
    Tuple(Vec<String>),

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
}

impl FunctionKind {
    /// Check if this is a trait-generated function
    pub fn is_trait_function(&self) -> bool {
        matches!(
            self,
            FunctionKind::Delete
                | FunctionKind::DeepCopy
                | FunctionKind::PartialEq
                | FunctionKind::PartialCmp
                | FunctionKind::Cmp
                | FunctionKind::Hash
        )
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
            _ => None,
        }
    }

    /// Get the method suffix for C-ABI name
    pub fn c_suffix(&self) -> &'static str {
        match self {
            FunctionKind::Delete => "_delete",
            FunctionKind::DeepCopy => "_deepCopy",
            FunctionKind::PartialEq => "_partialEq",
            FunctionKind::PartialCmp => "_partialCmp",
            FunctionKind::Cmp => "_cmp",
            FunctionKind::Hash => "_hash",
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
#[derive(Debug, Clone)]
pub struct TypeAliasDef {
    /// Alias name
    pub name: String,

    /// Target type
    pub target: String,

    /// Documentation
    pub doc: Vec<String>,

    /// Module
    pub module: String,
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
        matches!(self, 
            TypeCategory::Regular | 
            TypeCategory::CallbackDataPair
        )
    }

    /// Check if this type should be completely skipped in Python
    pub fn skip_in_python(&self) -> bool {
        matches!(self,
            TypeCategory::Recursive |
            TypeCategory::VecRef |
            TypeCategory::Boxed |
            TypeCategory::GenericTemplate |
            TypeCategory::DestructorOrClone |
            TypeCategory::CallbackTypedef
        )
    }

    /// Check if this type uses the C-API type directly (no wrapper)
    pub fn uses_capi_directly(&self) -> bool {
        matches!(self,
            TypeCategory::Primitive |
            TypeCategory::String |
            TypeCategory::Vec |
            TypeCategory::RefAny
        )
    }

    /// Check if this is a callback-related type that needs trampolines
    pub fn is_callback_related(&self) -> bool {
        matches!(self,
            TypeCategory::CallbackTypedef |
            TypeCategory::CallbackDataPair
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
    pub fn from_derives_and_custom_impls(derives: &[String], custom_impls: &[String], has_custom_drop: bool) -> Self {
        let mut traits = Self::default();
        traits.has_custom_drop = has_custom_drop;

        // Process both derives and custom_impls
        for derive in derives.iter().chain(custom_impls.iter()) {
            match derive.as_str() {
                "Copy" => {
                    traits.is_copy = true;
                    traits.is_clone = true; // Copy implies Clone
                }
                "Clone" => traits.is_clone = true,
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
