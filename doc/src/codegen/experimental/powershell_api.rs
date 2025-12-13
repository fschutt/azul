use indexmap::IndexMap;
use crate::{
    api::{ApiData, ClassData, FunctionData, EnumVariantData, StructFieldData},
    utils::analyze::{analyze_type, is_primitive_arg},
    utils::string::snake_case_to_lower_camel,
};

/*

PowerShell is a very powerful glue language, and because it sits on top of .NET, it has 
access to **P/Invoke** via the `Add-Type` cmdlet.

This approach is perfect for restricted environments because you don't need `cargo`, 
`gcc`, or `make` on the target machine. You just need the text file (`.psm1`) and the 
compiled DLL (`azul.dll`).

### The Strategy

We will generate a **PowerShell Module (`Azul.psm1`)**.

1.  **Embedded C#**: We generate a string containing C# P/Invoke definitions (Structs, Enums, DllImports).
2.  **`Add-Type`**: The module compiles this C# string in-memory when imported.
3.  **Wrappers**: We generate "Cmdlets" (Functions) like `New-AzApp` that wrap the underlying 
    .NET calls for a native PowerShell feel.

### Usage in PowerShell

1.  **Generate**: Run generator to get `Azul.psm1`.
2.  **Setup**: Put `azul.dll` (or `.so`) in the same folder.

### Key PowerShell Specifics

1.  **`Add-Type`**: This is the heart of the solution. It invokes the C# compiler (`csc.exe`) behind the scenes. If the environment blocks this (Constrained Language Mode), the script will fail.
    *   *Workaround for Strict Env*: Compile the C# code into a standard DLL (`Azul.Bindings.dll`) using `dotnet build` on a dev machine, and ship that. Change the `.psm1` to just `Add-Type -Path "Azul.Bindings.dll"`.
2.  **Verbs**: PowerShell gets angry if you use non-standard verbs (like `Run-AzApp`). `Invoke-` is the standard fallback for "doing something". `New-` is standard for constructors.
3.  **Pipelines**: By adding `ValueFromPipeline=$true` to the Instance parameter, you can chain commands:
    ```powershell
    New-AzApp ... | Invoke-AzAppRun -options $opts
    ```
4.  **Structs**: Because we defined the structs in the C# block, PowerShell treats them as .NET objects. You can do `$config.log_level = ...` naturally.

### Shipping

Standard PowerShell Module layout:

```text
Azul/
├── Azul.psd1    (Manifest)
├── Azul.psm1    (Generated Script)
├── azul.dll     (Native Rust Lib)
└── libazul.so   (Linux support)
```
*/

const PREFIX: &str = "Az";
const DLL_NAME: &str = "azul";

pub fn get_azul_psd1() -> String {
    format!("
@{
    ModuleVersion = '1.0.0'
    RootModule = 'Azul.psm1'
    FunctionsToExport = '*'
    # ...
}
    ").trim().to_string()
}

/*

    # Import the module
    Import-Module ./Azul.psm1

    # Ensure DLL can be found (if not in System32)
    # Set-AzulLibraryPath -Path $PSScriptRoot 

    # Create Config (Using Generated Function)
    $config = New-AzAppConfig

    # Set Log Level
    # We can access Enums directly via the compiled .NET type
    $config.log_level = [Azul.Native.AzAppLogLevel]::Debug

    # Create Options
    # Passing 0/Zero for null pointers
    $opts = New-AzWindowCreateOptions -layout_callback 0

    # Create App
    $app = New-AzApp -initial_data 0 -app_config $config

    # Run
    Invoke-AzAppRun -Instance $app -options $opts

    # Cleanup
    # Remove-AzApp -Instance $app

*/
/// Maps to C# types (for the internal P/Invoke definition)
fn map_to_csharp_type(ty: &str, is_return: bool) -> String {
    if ty.contains('*') {
        if ty.contains("char") { return "string".to_string(); }
        return "IntPtr".to_string();
    }

    match ty {
        "void" | "c_void" | "GLvoid" => "void".to_string(),
        "bool" | "GLboolean" => "bool".to_string(),
        "char" | "u8" | "i8" => "byte".to_string(),
        "u16" | "i16" | "AzU16" => "ushort".to_string(),
        "u32" | "GLuint" | "AzU32" | "AzScanCode" | "GLenum" | "GLbitfield" => "uint".to_string(),
        "i32" | "GLint" | "GLsizei" => "int".to_string(),
        "u64" | "GLuint64" => "ulong".to_string(),
        "i64" | "GLint64" => "long".to_string(),
        "f32" | "GLfloat" | "AzF32" => "float".to_string(),
        "f64" | "GLdouble" => "double".to_string(),
        "usize" | "size_t" => "UIntPtr".to_string(),
        "isize" | "ssize_t" => "IntPtr".to_string(),
        s if s.starts_with(PREFIX) => s.to_string(),
        _ => "IntPtr".to_string(),
    }
}

/// Helper to determine appropriate PowerShell Verb
fn get_ps_verb(fn_name: &str) -> String {
    match fn_name {
        "new" | "create" => "New".to_string(),
        "delete" | "drop" | "free" => "Remove".to_string(),
        "run" => "Invoke".to_string(),
        "get" => "Get".to_string(),
        "set" => "Set".to_string(),
        "add" => "Add".to_string(),
        "clear" => "Clear".to_string(),
        "update" => "Update".to_string(),
        "start" => "Start".to_string(),
        "stop" => "Stop".to_string(),
        _ => "Invoke".to_string(), // Fallback
    }
}

pub fn generate_powershell_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();
    let mut cs = String::new(); // The C# source string
    let version_data = api_data.get_version(version).unwrap();

    // -------------------------------------------------------------------------
    // 1. Generate C# Definition String
    // -------------------------------------------------------------------------
    cs.push_str("using System;\n");
    cs.push_str("using System.Runtime.InteropServices;\n\n");
    cs.push_str("namespace Azul.Native {\n\n");

    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let full_name = format!("{}{}", PREFIX, class_name);

            // Enums
            if let Some(enum_fields) = &class_data.enum_fields {
                let is_simple = enum_fields.iter().all(|m| m.values().all(|v| v.r#type.is_none()));
                if is_simple {
                    cs.push_str(&format!("    public enum {} : uint {{\n", full_name));
                    let mut idx = 0;
                    for variant_map in enum_fields {
                        for (variant_name, _) in variant_map {
                            cs.push_str(&format!("        {} = {},\n", variant_name, idx));
                            idx += 1;
                        }
                    }
                    cs.push_str("    }\n");
                }
            }

            // Structs
            if let Some(struct_fields) = &class_data.struct_fields {
                cs.push_str("    [StructLayout(LayoutKind.Sequential)]\n");
                cs.push_str(&format!("    public struct {} {{\n", full_name));
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let c_type = map_to_csharp_type(&field_data.r#type, false);
                        cs.push_str(&format!("        public {} {};\n", c_type, field_name));
                    }
                }
                cs.push_str("    }\n");
            }
        }
    }
    
    // NativeMethods Class
    cs.push_str("\n    public static class API {\n");
    // Windows DLL handling
    cs.push_str(&format!("        const string DllName = \"{}\";\n\n", DLL_NAME));
    
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            let mut emit_cs_func = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let c_symbol = if is_ctor {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                } else {
                     format!("{}_{}", class_c_name, snake_case_to_lower_camel(fn_name))
                };
                
                let ret_type = fn_data.returns.as_ref().map_or("void".to_string(), |r| map_to_csharp_type(&r.r#type, true));
                
                let mut args = Vec::new();
                if !is_ctor { args.push("IntPtr instance".to_string()); }
                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        args.push(format!("{} {}", map_to_csharp_type(ty, false), name));
                    }
                }
                
                // Add EntryPoint to handle symbol names cleanly
                cs.push_str(&format!("        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint=\"{}\")]\n", c_symbol));
                if ret_type == "bool" { cs.push_str("        [return: MarshalAs(UnmanagedType.I1)]\n"); }
                cs.push_str(&format!("        public static extern {} {}({});\n\n", ret_type, c_symbol, args.join(", ")));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_cs_func(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_cs_func(name, data, false); }
            }
            if class_data.custom_destructor.unwrap_or(false) || class_data.is_boxed_object {
                cs.push_str(&format!("        [DllImport(DllName, CallingConvention = CallingConvention.Cdecl, EntryPoint=\"{}_delete\")]\n", class_c_name));
                cs.push_str(&format!("        public static extern void {}_delete(IntPtr instance);\n\n", class_c_name));
            }
        }
    }
    cs.push_str("    }\n"); // End Class API
    cs.push_str("}\n"); // End Namespace

    // -------------------------------------------------------------------------
    // 2. Generate PowerShell Module (.psm1) Content
    // -------------------------------------------------------------------------
    
    code.push_str("# Azul GUI Bindings for PowerShell\n");
    code.push_str("# Auto-generated\n\n");
    
    // Embed the C# code
    code.push_str("$AzulCSharpDefinitions = @'\n");
    code.push_str(&cs);
    code.push_str("'@\n\n");
    
    // Compile it
    code.push_str("try {\n");
    code.push_str("    Add-Type -TypeDefinition $AzulCSharpDefinitions -Language CSharp\n");
    code.push_str("} catch {\n");
    code.push_str("    Write-Error \"Failed to load Azul Native bindings. Ensure 'azul.dll' is in the system path or current directory.\"\n");
    code.push_str("    throw $_\n");
    code.push_str("}\n\n");

    // Helper: Resolve DLL path if not in system path (Optional convenience)
    code.push_str("function Set-AzulLibraryPath {\n");
    code.push_str("    param([string]$Path = $PSScriptRoot)\n");
    code.push_str("    $env:PATH += \";$Path\"\n");
    code.push_str("}\n\n");

    // 3. Generate PowerShell Functions
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_c_name = format!("{}{}", PREFIX, class_name);

            // Helpers
            let mut emit_ps_func = |fn_name: &str, fn_data: &FunctionData, is_ctor: bool| {
                let suffix = snake_case_to_lower_camel(fn_name);
                let c_method_name = format!("{}_{}", class_c_name, suffix);
                
                // Determine PS Verb-Noun
                let verb = get_ps_verb(fn_name);
                // Noun: AzWindow
                let noun = if is_ctor { class_c_name.clone() } else { format!("{}{}", class_c_name, suffix) };
                
                // If it's not a constructor, simplified: e.g. Invoke-AzAppRun
                let ps_func_name = if is_ctor {
                     format!("{}-{}", verb, class_c_name) // New-AzApp
                } else {
                     // Heuristic to make nice names: Invoke-AzAppRun
                     // Or maybe simpler: AzApp-Run ? PS prefers Verb-Noun.
                     // Let's stick to strict Verb-Noun.
                     // Class: AzApp, Method: Run -> Invoke-AzAppRun
                     format!("{}-{}{}", verb, class_c_name, crate::utils::string::snake_case_to_lower_camel(fn_name)) // Start-AzApp
                };

                code.push_str(&format!("function {} {{\n", ps_func_name));
                code.push_str("    param(\n");
                
                let mut call_args = Vec::new();
                if !is_ctor {
                    code.push_str("        [Parameter(Mandatory=$true, ValueFromPipeline=$true)]\n");
                    code.push_str("        [IntPtr]$Instance");
                    call_args.push("$Instance".to_string());
                }

                for arg_map in &fn_data.fn_args {
                    for (name, ty) in arg_map {
                        if name == "self" { continue; }
                        let ps_type = match map_to_csharp_type(ty, false).as_str() {
                            "IntPtr" => "[IntPtr]",
                            "string" => "[string]",
                            "bool" => "[bool]",
                            "int" => "[int]",
                            "uint" => "[uint32]",
                            "float" => "[float]",
                            "double" => "[double]",
                            // If it matches our prefix Az.., it's a struct/enum defined in the C# above
                            s if s.starts_with(PREFIX) => format!("[Azul.Native.{}]", s), 
                            _ => "[IntPtr]".to_string() // Fallback
                        };
                        // Add comma if needed
                        let comma = if !call_args.is_empty() { "," } else { "" };
                        if !is_ctor { code.push_str(",\n"); }
                        
                        code.push_str(&format!("        {}${}", ps_type, name));
                        call_args.push(format!("${}", name));
                    }
                }
                
                code.push_str("\n    )\n");
                code.push_str("    process {\n");
                code.push_str(&format!("        [Azul.Native.API]::{}[{}\n", c_method_name, if call_args.is_empty() { "" } else { "(" }));
                if !call_args.is_empty() {
                    code.push_str(&format!("            {}\n", call_args.join(", ")));
                    code.push_str("        )\n");
                } else {
                    code.push_str(")\n");
                }
                code.push_str("    }\n");
                code.push_str("}\n");
                code.push_str(&format!("Export-ModuleMember -Function {}\n\n", ps_func_name));
            };

            if let Some(ctors) = &class_data.constructors {
                for (name, data) in ctors { emit_ps_func(name, data, true); }
            }
            if let Some(fns) = &class_data.functions {
                for (name, data) in fns { emit_ps_func(name, data, false); }
            }
        }
    }
    
    code
}
