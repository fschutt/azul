use std::collections::BTreeMap;
use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{
        analyze_type, is_primitive_arg, replace_primitive_ctype,
    },
};

/*

let haskell_files = codegen::haskell::generate_haskell_bindings(&api_data, "1.0.0");
for (filename, content) in haskell_files {
    // Write content to "azul-hs/src/" + filename
}

*/
const PREFIX: &str = "Az";

/// Returns a map of Filename -> Content
/// Generates:
/// 1. `Azul/Raw.hsc` - Low-level FFI bindings (safe/unsafe pointers, Storable instances)
pub fn generate_haskell_bindings(api_data: &ApiData, version: &str) -> BTreeMap<String, String> {
    let mut files = BTreeMap::new();
    let version_data = api_data.get_version(version).unwrap();
    let mut hsc = String::new();

    // -------------------------------------------------------------------------
    // 1. HSC Header
    // -------------------------------------------------------------------------
    hsc.push_str("{-# LANGUAGE ForeignFunctionInterface #-}\n");
    hsc.push_str("{-# LANGUAGE CPP, CApiFFI #-}\n");
    hsc.push_str("{-# LANGUAGE GeneralizedNewtypeDeriving #-}\n");
    hsc.push_str("\n");
    hsc.push_str("module Azul.Raw where\n\n");
    
    hsc.push_str("import Foreign.C.Types\n");
    hsc.push_str("import Foreign.Storable\n");
    hsc.push_str("import Foreign.Ptr\n");
    hsc.push_str("import Data.Word\n");
    hsc.push_str("import Data.Int\n");
    hsc.push_str("\n");
    hsc.push_str("#include \"azul.h\"\n"); 
    hsc.push_str("\n");

    // -------------------------------------------------------------------------
    // 2. Type Aliases (Primitives)
    // -------------------------------------------------------------------------
    // These maps basic C types that might not be standard in Foreign.C.Types
    hsc.push_str("-- | Primitive Type Aliases\n");
    hsc.push_str("type AzGLuint = Word32\n");
    hsc.push_str("type AzGLint = Int32\n");
    hsc.push_str("type AzGLenum = Word32\n");
    hsc.push_str("type AzScanCode = Word32\n");
    hsc.push_str("\n");

    // -------------------------------------------------------------------------
    // 3. Structs & Enums Generation
    // -------------------------------------------------------------------------
    // We collect them first to handle dependencies if necessary, 
    // but HSC handles forward declarations okay usually.
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            // Generate Enums
            if let Some(enum_fields) = &class_data.enum_fields {
                generate_enum(&mut hsc, &full_name, enum_fields);
            }
            
            // Generate Structs
            if let Some(struct_fields) = &class_data.struct_fields {
                generate_struct(&mut hsc, &full_name, struct_fields);
            }
        }
    }

    // -------------------------------------------------------------------------
    // 4. Function Imports
    // -------------------------------------------------------------------------
    hsc.push_str("\n-- | Function Bindings\n");
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Constructors
            if let Some(constructors) = &class_data.constructors {
                for (fn_name, fn_data) in constructors {
                    generate_function(&mut hsc, &class_c_name, fn_name, fn_data, true);
                }
            }

            // Methods
            if let Some(functions) = &class_data.functions {
                for (fn_name, fn_data) in functions {
                    generate_function(&mut hsc, &class_c_name, fn_name, fn_data, false);
                }
            }
            
            // Destructors (if applicable)
            let has_dtor = class_data.custom_destructor.unwrap_or(false) 
                || class_data.is_boxed_object;
                
            if has_dtor {
                 // void AzClassName_delete(AzClassName* instance);
                 let c_symbol = format!("{}_delete", class_c_name);
                 hsc.push_str(&format!("foreign import capi unsafe \"{0}\" {0} :: Ptr {1} -> IO ()\n", 
                    c_symbol, class_c_name));
            }
        }
    }

    files.insert("Azul/Raw.hsc".to_string(), hsc);
    files
}

/// Generate Haskell data type and Enum instance for C enums
fn generate_enum(
    hsc: &mut String,
    enum_name: &str,
    fields: &Vec<IndexMap<String, EnumVariantData>>,
) {
    // Check if it's a simple enum (no payload types)
    // Complex enums (tagged unions) are mapped as opaque structs or raw unions in C.
    // For Haskell FFI, we often treat complex C unions as opaque structs + accessors.
    // Here we focus on simple integer enums which are most common for flags/constants.
    
    let is_simple_enum = fields.iter().all(|map| {
        map.values().all(|v| v.r#type.is_none())
    });

    if !is_simple_enum {
        // Tagged unions in C are complex. We treat them as opaque Storable structs here.
        // The user can use the helper functions generated in C (matchRef/matchMut) to access them.
        hsc.push_str(&format!("data {}\n", enum_name));
        hsc.push_str(&format!("instance Storable {} where\n", enum_name));
        hsc.push_str(&format!("    sizeOf _ = (#size {})\n", enum_name));
        hsc.push_str(&format!("    alignment _ = (#alignment {})\n", enum_name));
        hsc.push_str("    peek = error \"Direct peek of Tagged Union not implemented\"\n");
        hsc.push_str("    poke = error \"Direct poke of Tagged Union not implemented\"\n\n");
        return;
    }

    hsc.push_str(&format!("data {} = \n", enum_name));
    
    let mut first = true;
    for variant_map in fields {
        for (variant_name, _) in variant_map {
            let prefix = if first { "    " } else { "    | " };
            hsc.push_str(&format!("{}{}_{}\n", prefix, enum_name, variant_name));
            first = false;
        }
    }
    hsc.push_str("    deriving (Eq, Show)\n\n");

    // Generate Enum instance using hsc2hs #const lookups
    hsc.push_str(&format!("instance Enum {} where\n", enum_name));
    
    // toEnum
    hsc.push_str("    toEnum x = case x of\n");
    for variant_map in fields {
        for (variant_name, _) in variant_map {
            // C Enum Value Lookup
            hsc.push_str(&format!("        (#const {}_{}) -> {}_{}\n", 
                enum_name, variant_name, enum_name, variant_name));
        }
        hsc.push_str(&format!("        _ -> error \"Unknown value for enum {}\"\n", enum_name));

    }

    // fromEnum
    hsc.push_str("    fromEnum x = case x of\n");
    for variant_map in fields {
        for (variant_name, _) in variant_map {
            hsc.push_str(&format!("        {}_{} -> (#const {}_{})\n", 
                enum_name, variant_name, enum_name, variant_name));
        }
    }
    hsc.push_str("\n");
    
    // Generate Storable for Enum (treating it as CInt/Word32 usually)
    hsc.push_str(&format!("instance Storable {} where\n", enum_name));
    hsc.push_str(&format!("    sizeOf _ = (#size {})\n", enum_name));
    hsc.push_str(&format!("    alignment _ = (#alignment {})\n", enum_name));
    hsc.push_str("    peek ptr = do\n");
    hsc.push_str("        v <- peek (castPtr ptr :: Ptr CInt)\n");
    hsc.push_str("        return $ toEnum (fromIntegral v)\n");
    hsc.push_str("    poke ptr v = poke (castPtr ptr :: Ptr CInt) (fromIntegral (fromEnum v))\n\n");
}

/// Generate Haskell data type and Storable instance for C Structs
fn generate_struct(
    hsc: &mut String,
    struct_name: &str,
    fields: &Vec<IndexMap<String, StructFieldData>>,
) {
    // Define the data type
    hsc.push_str(&format!("data {} = {} {{\n", struct_name, struct_name));
    
    let mut field_names = Vec::new();

    for (i, field_map) in fields.iter().enumerate() {
        for (field_name, field_data) in field_map {
            let hs_type = map_type_to_haskell(&field_data.r#type);
            
            // Haskell record fields must be unique per module usually, so we prefix them
            // e.g. azDomId_inner
            let record_field_name = format!("{}_{}", 
                struct_name_to_camel(struct_name), field_name);
            
            let comma = if i == fields.len() - 1 && field_map.len() == 1 { "" } else { "," };
            
            hsc.push_str(&format!("    {} :: {}{}\n", record_field_name, hs_type, comma));
            field_names.push((field_name.clone(), hs_type));
        }
    }
    hsc.push_str("} deriving (Show, Eq)\n\n");

    // Storable Instance
    hsc.push_str(&format!("instance Storable {} where\n", struct_name));
    hsc.push_str(&format!("    sizeOf _ = (#size {})\n", struct_name));
    hsc.push_str(&format!("    alignment _ = (#alignment {})\n", struct_name));
    
    // Peek
    hsc.push_str("    peek ptr = do\n");
    for (field_name, _) in &field_names {
        hsc.push_str(&format!("        v_{0} <- (#peek {1}, {0}) ptr\n", field_name, struct_name));
    }
    hsc.push_str(&format!("        return $ {} ", struct_name));
    for (field_name, _) in &field_names {
        hsc.push_str(&format!("v_{} ", field_name));
    }
    hsc.push_str("\n");

    // Poke
    hsc.push_str("    poke ptr v = do\n");
    for (field_name, _) in &field_names {
        let record_name = format!("{}_{}", struct_name_to_camel(struct_name), field_name);
        hsc.push_str(&format!("        (#poke {0}, {1}) ptr ({2} v)\n", 
            struct_name, field_name, record_name));
    }
    hsc.push_str("\n");
}

fn generate_function(
    hsc: &mut String,
    class_name: &str,
    fn_name: &str,
    data: &FunctionData,
    is_constructor: bool
) {
    // Name mangling must match C API: AzClassName_functionName
    // Exception: Constructors are AzClassName_functionName usually, but check C logic
    let c_symbol = if is_constructor {
        // In C API: format!("{}_{}", class_ptr_name, snake_case_to_lower_camel(fn_name));
        // We need to replicate `snake_case_to_lower_camel` logic or assume input is already sanitized.
        // Assuming API JSON `fn_name` is "new" -> `AzDom_new`
        format!("{}_{}", class_name, crate::utils::string::snake_case_to_lower_camel(fn_name))
    } else {
         format!("{}_{}", class_name, crate::utils::string::snake_case_to_lower_camel(fn_name))
    };

    hsc.push_str(&format!("foreign import capi unsafe \"{}\" {}\n", c_symbol, c_symbol));
    hsc.push_str("    :: ");

    // Argument Types
    // If it's a method (not constructor), implicit self is first
    if !is_constructor {
        // C methods usually take a pointer to self: `AzDom*`
        hsc.push_str(&format!("Ptr {} -> ", class_name));
    }

    for arg_map in &data.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" { continue; }
            hsc.push_str(&format!("{} -> ", map_type_to_haskell(arg_type)));
        }
    }

    // Return Type
    let ret_type = if let Some(ret) = &data.returns {
        // C functions returning structs by value:
        // Haskell CApiFFI can theoretically handle this if the struct is Storable.
        // Since we generated Storable instances, we can return the type directly in IO.
        map_type_to_haskell(&ret.r#type)
    } else {
        "()".to_string()
    };

    hsc.push_str(&format!("IO ({})\n", ret_type));
}

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn struct_name_to_camel(s: &str) -> String {
    // Quick hack to make record fields lowerCamelCase if struct is PascalCase
    if let Some(c) = s.chars().next() {
        let mut res = c.to_lowercase().to_string();
        res.push_str(&s[1..]);
        res
    } else {
        s.to_string()
    }
}

/// Map Rust/Azul types to Haskell FFI types
fn map_type_to_haskell(type_str: &str) -> String {
    let (prefix, base, suffix) = analyze_type(type_str);
    
    // Arrays [T; N] -> Ptr T (simplified for FFI)
    if !suffix.is_empty() {
        let inner = map_type_to_haskell(&base);
        return format!("Ptr {}", inner);
    }

    // Pointers
    if prefix.contains("*") || prefix.contains("&") {
        let inner = map_type_to_haskell(&base);
        // void* -> Ptr ()
        if inner == "()" {
            return "Ptr ()".to_string();
        }
        return format!("Ptr {}", inner);
    }

    // Primitives
    match base.as_str() {
        "void" | "c_void" | "GLvoid" | "AzGLvoid" => "()".to_string(),
        "bool" | "GLboolean" | "AzGLboolean" => "CBool".to_string(),
        "char" => "CChar".to_string(),
        "i8" => "Int8".to_string(),
        "u8" => "Word8".to_string(),
        "i16" => "Int16".to_string(),
        "u16" | "AzU16" => "Word16".to_string(),
        "i32" | "GLint" | "AzGLint" | "GLsizei" | "AzGLsizei" => "Int32".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "AzGLenum" | "GLbitfield" | "AzGLbitfield" => "Word32".to_string(),
        "i64" | "GLint64" | "AzGLint64" => "Int64".to_string(),
        "u64" | "GLuint64" | "AzGLuint64" => "Word64".to_string(),
        "f32" | "GLfloat" | "AzGLfloat" | "GLclampf" | "AzGLclampf" | "AzF32" => "CFloat".to_string(),
        "f64" | "GLdouble" | "GLclampd" => "CDouble".to_string(),
        "usize" | "uintptr_t" | "size_t" => "CSize".to_string(),
        "isize" | "intptr_t" | "ssize_t" | "GLsizeiptr" | "GLintptr" | "AzGLintptr" => "CIntPtr".to_string(),
        "AzString" => "AzString".to_string(), // It's a struct, defined in Raw
        _ => {
            // Check if it's one of our structs (starts with Az, or we prepend Az)
            if base.starts_with("Az") {
                base
            } else {
                format!("{}{}", PREFIX, base)
            }
        }
    }
}