use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*
    For **Zig**, we can generate a fully idiomatic wrapper. Zig excels at C 
    interop, allowing us to define `extern struct` layouts that match C binary 
    compatibility perfectly, while attaching methods (`pub fn`) to those structs for 
    an Object-Oriented feel.

    This generator produces a single `azul.zig` file.

    ### Usage in Zig

    1.  **Generate**: Output to `src/azul.zig`.
    2.  **Build System (`build.zig`)**:

    ### Key Zig Details

    1.  **`extern struct`**: This is equivalent to `#[repr(C)]`. It ensures the struct 
        layout matches C exactly.
    2.  **`opaque` types**: For types where we don't know or don't care about the fields 
        (Handles), we use `pub const AzApp = opaque {};`. References are then `*AzApp`. This 
        prevents the user from accidentally dereferencing the pointer or accessing fields that 
        don't exist in the generated binding.
    3.  **Namespacing**: By putting the `extern` functions inside the `struct` namespace 
        (e.g., `AzApp.create`), we get a very nice, logical API (`AzApp.create(...)`) instead of a 
        flat C-style API (`AzApp_create(...)`), although both are generated.
    4.  **`?*T` (Optional Pointers)**: C pointers can be NULL. Zig forces you to handle this. The 
        generator produces `?*AzApp`. In `main`, you must use `orelse` or `if (ptr) |p| { ... }` to 
        unwrap safely. This prevents Segfaults.
    5.  **Linking**: The `extern "azul"` string in the generator matches the `exe.linkSystemLibrary("azul")` 
        in `build.zig`. If you change one, you must change the other.
*/

/*
    const std = @import("std");
    const azul = @import("azul.zig");

    pub fn main() !void {
        // 1. Create Config
        // Note: Rust constructors usually return ?*T (nullable pointer)
        const config = azul.AzAppConfig.create() orelse return error.AzulInitFailed;
        
        // Modify fields (if struct is not opaque)
        // config.log_level = azul.AzAppLogLevel.Debug; 

        // 2. Create Options
        const opts = azul.AzWindowCreateOptions.create(null) orelse return error.AzulInitFailed;

        // 3. Create App
        const app = azul.AzApp.create(null, config) orelse return error.AzulInitFailed;

        // 4. Run
        azul.AzApp.run(app, opts);

        // 5. Cleanup
        // Note: Defer works great here
        // But AzApp.run might consume 'app' depending on API design.
        // If explicit delete is needed:
        // azul.AzApp.delete(app);
    }
*/

pub fn get_build_zig() -> String {
    format!("
const std = @import(\"std\");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const exe = b.addExecutable(.{
        .name = \"azul-app\",
        .root_source_file = .{ .path = \"src/main.zig\" },
        .target = target,
        .optimize = optimize,
    });

    // Link against shared library
    // For \"extern 'azul'\", we need to link libazul.so / azul.dll
    exe.linkSystemLibrary(\"azul\"); 
    
    // Add library path if it's local
    exe.addLibraryPath(.{ .path = \"./lib\" }); 
    
    // On Windows, you might need to copy the DLL to zig-out/bin
    
    b.installArtifact(exe);
}
    ").trim().to_string()
}

const PREFIX: &str = "Az";
// The library name used in 'extern "azul" fn ...'
// In build.zig, you will link libazul to this name.
const LIB_NAME: &str = "azul";

/// Maps C types to Zig types
fn map_to_zig_type(ty: &str) -> String {
    // Handle Pointers
    if ty.contains('*') {
        if ty.contains("char") {
            // Null-terminated C string pointer
            return "[*:0]const u8".to_string(); 
        }
        
        let inner = ty.replace("*", "").replace("const", "").replace("mut", "").trim().to_string();
        
        if inner == "void" || inner == "c_void" {
            // Opaque pointer, nullable
            return "?*anyopaque".to_string();
        }
        
        // Typed pointer (nullable for safety in bindings)
        if inner.starts_with(PREFIX) {
             return format!("?*{}", inner);
        }
        
        return "?*anyopaque".to_string();
    }

    match ty {
        "void" | "c_void" => "void".to_string(),
        "bool" | "GLboolean" => "bool".to_string(),
        "char" | "u8" | "i8" => "u8".to_string(), // or i8
        "u16" | "i16" | "AzU16" => "u16".to_string(), // or i16
        "i32" | "GLint" | "GLsizei" => "i32".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "u32".to_string(),
        "i64" | "GLint64" | "isize" | "ssize_t" | "intptr_t" => "i64".to_string(),
        "u64" | "GLuint64" | "usize" | "size_t" | "uintptr_t" => "u64".to_string(),
        "f32" | "GLfloat" | "AzF32" => "f32".to_string(),
        "f64" | "GLdouble" => "f64".to_string(),
        // Struct passed by value
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "usize".to_string(), // Fallback for unknown
    }
}

pub fn generate_zig_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let version_data = api_data.get_version(version).unwrap();

    // 1. Header
    code.push_str("//! Auto-generated Zig bindings for Azul GUI\n");
    code.push_str("const std = @import(\"std\");\n\n");

    // 2. Enums
    // Zig enums are powerful. We use `extern enum` to guarantee C ABI size.
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                
                if is_simple {
                    code.push_str(&format!("pub const {} = extern enum(c_int) {{\n", full_name));
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            // Zig doesn't allow Enum variants to start with numbers usually, 
                            // but Azul variants are usually PascalCase.
                            code.push_str(&format!("    {},\n", variant_name));
                        }
                    }
                    code.push_str("};\n\n");
                }
            }
        }
    }

    // 3. Structs and Opaque Types
    // We define the type (layout) and attach methods to it directly.
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);
            let has_struct_fields = class_data.struct_fields.is_some();
            
            if has_struct_fields {
                // Value Type (C Struct)
                code.push_str(&format!("pub const {} = extern struct {{\n", full_name));
                
                if let Some(fields) = &class_data.struct_fields {
                    for field_map in fields {
                        for (field_name, field_data) in field_map {
                            let zig_type = map_to_zig_type(&field_data.r#type);
                            code.push_str(&format!("    {}: {},\n", field_name, zig_type));
                        }
                    }
                }
            } else {
                // Opaque Handle (Pointer Type)
                // We define it as an opaque struct so we can enforce type safety (vs void*)
                // and attach methods to it.
                code.push_str(&format!("pub const {} = opaque {{\n", full_name));
            }
            
            // --- Methods & Constructors inside the struct namespace ---
            
            // 3a. Constructors (Static functions)
            if let Some(ctors) = &class_data.constructors {
                for (fn_name, fn_data) in ctors {
                    let zig_fn_name = if fn_name == "new" { "create" } else { fn_name }; // 'new' is not keywords, but create is idiomatic
                    
                    let c_symbol = format!("{}_{}", full_name, snake_case_to_lower_camel(fn_name));
                    
                    // extern decl
                    let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_zig_type(&r.r#type));
                    
                    // We need to generate the arguments for the wrappers
                    let mut args_sig = Vec::new();
                    let mut args_call = Vec::new();
                    
                    for arg_map in &fn_data.fn_args {
                         for (name, ty) in arg_map {
                             let z_ty = map_to_zig_type(ty);
                             args_sig.push(format!("{}: {}", name, z_ty));
                             args_call.push(name.clone());
                         }
                    }

                    // Wrapper function
                    code.push_str(&format!("\n    pub fn {}({}) {} {{\n", zig_fn_name, args_sig.join(", "), ret_type));
                    code.push_str(&format!("        return {}({});\n", c_symbol, args_call.join(", ")));
                    code.push_str("    }\n");
                }
            }

            // 3b. Methods (Member functions)
            if let Some(fns) = &class_data.functions {
                for (fn_name, fn_data) in fns {
                    let zig_fn_name = snake_case_to_lower_camel(fn_name);
                    let c_symbol = format!("{}_{}", full_name, zig_fn_name);
                    
                    let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_zig_type(&r.r#type));

                    let mut args_sig = Vec::new();
                    let mut args_call = Vec::new();
                    
                    for arg_map in &fn_data.fn_args {
                         for (name, ty) in arg_map {
                             if name == "self" {
                                 // Cast self appropriately
                                 if has_struct_fields {
                                     // Passed by pointer in C usually for methods
                                     args_sig.push(format!("self: *{}", full_name));
                                     args_call.push("self".to_string());
                                 } else {
                                     // Opaque type is already a pointer effectively in usage, 
                                     // but in Zig 'opaque' types are used as *T.
                                     args_sig.push(format!("self: *{}", full_name));
                                     args_call.push("self".to_string());
                                 }
                                 continue; 
                             }
                             let z_ty = map_to_zig_type(ty);
                             args_sig.push(format!("{}: {}", name, z_ty));
                             args_call.push(name.clone());
                         }
                    }
                    
                    code.push_str(&format!("\n    pub fn {}({}) {} {{\n", zig_fn_name, args_sig.join(", "), ret_type));
                    code.push_str(&format!("        return {}({});\n", c_symbol, args_call.join(", ")));
                    code.push_str("    }\n");
                }
            }
            
            // 3c. Destructor wrapper
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                let c_symbol = format!("{}_delete", full_name);
                code.push_str("\n    pub fn delete(self: *anyopaque) void {\n");
                // We cast to anyopaque in signature to allow flexibility, but internally call typed C function
                // Actually, let's keep it typed.
                code.push_str(&format!("        {}_delete(@ptrCast(self));\n", full_name));
                code.push_str("    }\n");
            }

            code.push_str("};\n\n");
        }
    }

    // 4. Extern C Declarations
    // Zig requires declaring the extern functions. We put them at the end or in a 'extern' block.
    code.push_str("// --- C ABI Declarations ---\n");
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            let mut emit_extern = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                 let raw_suffix = snake_case_to_lower_camel(fn_name);
                 let c_symbol = if is_ctor { format!("{}_{}", full_name, raw_suffix) } else { format!("{}_{}", full_name, raw_suffix) };
                 
                 let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_zig_type(&r.r#type));
                 
                 let mut args = Vec::new();
                 if !is_ctor {
                     // Self pointer
                     args.push(format!("self: ?*{}", full_name)); 
                 }
                 
                 for arg_map in &fn_data.fn_args {
                     for (name, ty) in arg_map {
                         if name == "self" { continue; }
                         args.push(format!("{}: {}", name, map_to_zig_type(ty)));
                     }
                 }
                 
                 code.push_str(&format!("extern \"{}\" fn {}({}) {};\n", 
                    LIB_NAME, c_symbol, args.join(", "), ret_type));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_extern(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_extern(name, data, false); }
            }
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                let c_symbol = format!("{}_delete", full_name);
                code.push_str(&format!("extern \"{}\" fn {}(self: ?*{}) void;\n", LIB_NAME, c_symbol, full_name));
            }
        }
    }

    code
}
