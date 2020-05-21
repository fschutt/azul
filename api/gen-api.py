import json
import re

def read_file(path):
    text_file = open(path, 'r')
    text_file_contents = text_file.read()
    text_file.close()
    return text_file_contents

prefix = "Az"
fn_prefix = "az_"
postfix = "Ptr"

basic_types = [
    "bool",
    "char",
    "f32",
    "f64",
    "fn",
    "i128",
    "i16",
    "i32",
    "i64",
    "i8",
    "isize",
    "slice",
    "u128",
    "u16",
    "u32",
    "u64",
    "u8",
    "()",
    "usize"
]

azul_readme_path = "../azul/README.md"
license_path = "../LICENSE"
api_file_path = "./public.api.json"
rust_dll_path = "../azul-dll/src/lib.rs"

c_api_path = "../azul/src/c/azul.h"
cpp_api_path = "../azul/src/cpp/azul.h"
rust_api_path = "../azul/src/rust/azul.rs"
python_api_path = "../azul/src/python/azul.py"
js_api_path = "../azul/src/js/azul.js"

dll_patches = {
    tuple(['*']): read_file("./patches/azul-dll/header.rs"),
    tuple(['callbacks', 'RefAny']): read_file("./patches/azul-dll/refany.rs"),
    tuple(['callbacks', 'LayoutCallback']): read_file("./patches/azul-dll/layout_callback.rs")
}

c_api_patches = {
    tuple(['callbacks', 'LayoutCallback']): read_file("./patches/c/layout_callback.h"),
}

rust_api_patches = {
    tuple(['callbacks', 'RefAny']): read_file("./patches/azul.rs/refany.rs"),
    tuple(['app', 'App', 'run']): read_file("./patches/azul.rs/app_run.rs"),
}

# ---------------------------------------------------------------------------------------------

def to_snake_case(name):
    s1 = re.sub('(.)([A-Z][a-z]+)', r'\1_\2', name)
    return re.sub('([a-z0-9])([A-Z])', r'\1_\2', s1).lower()

def read_api_file(path):
    api_file_contents = read_file(path)
    apiData = json.loads(api_file_contents)
    return apiData

def write_file(string, path):
    text_file = open(path, "w+")
    text_file.write(string)
    text_file.close()

def search_for_module_of_class(apiData, class_name):
    for module_name in apiData.keys():
        if class_name in apiData[module_name].keys():
            return module_name

    return None

def is_primitive_arg(arg):
    arg = arg.replace("&", "")
    arg = arg.replace("&mut", "")
    arg = arg.replace("*const", "")
    arg = arg.replace("*const", "")
    arg = arg.replace("*mut", "")
    arg = arg.strip()
    return arg in basic_types

def get_all_imports(apiData, module, module_name, existing_imports = {}):

    imports = {}

    arg_types_to_search = []

    for class_name in module.keys():
        c = module[class_name]

        if "constructors" in c.keys():
            for const in c["constructors"]:
                if "fn_args" in const.keys():
                    for arg_object in const["fn_args"]:
                        arg_name = list(arg_object.keys())[0]
                        if arg_name == "self":
                            continue
                        arg_type = arg_object[arg_name]
                        arg_types_to_search.append(arg_type)

        if "functions" in c.keys():
            for f in c["functions"]:
                if "fn_args" in f.keys():
                    for arg_object in f["fn_args"]:
                        arg_name = list(arg_object.keys())[0]
                        if arg_name == "self":
                            continue
                        arg_type = arg_object[arg_name]
                        arg_types_to_search.append(arg_type)

    for arg in arg_types_to_search:

        arg = arg.replace("*const", "")
        arg = arg.replace("*mut", "")
        arg = arg.strip()

        if arg in basic_types:
            continue

        found_module = None

        for v_module_name in existing_imports.keys():
            for v in existing_imports[v_module_name]:
                if v == arg:
                    found_module = v_module_name

        if found_module is None:
            found_module = search_for_module_of_class(apiData, arg)

        if found_module is None:
            raise Exception("" + arg + " not found!")

        if found_module in imports:
            imports[found_module].append(arg)
        else:
            imports[found_module] = [arg]

    if module_name in imports:
        del imports[module_name]

    imports_str = ""

    for module_name in imports.keys():
        classes = imports[module_name]
        use_str = ""
        if len(classes) == 1:
            use_str = classes[0]
        else:
            use_str = "{"
            for c in classes:
                use_str += c + ", "
            use_str = use_str[:-2]
            use_str += "}"

        imports_str += "    use crate::" + module_name + "::" + use_str + ";\r\n"

    return imports_str

def fn_args_c_api(f, class_name, class_ptr_name, self_as_first_arg, apiData):
    fn_args = ""

    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        if (self_val == "value"):
            fn_args += class_name.lower() + ": " + class_ptr_name + ", "
        elif (self_val == "refmut"):
            fn_args += class_name.lower() + ": &mut" + class_ptr_name + ", "
        elif (self_val == "ref"):
            fn_args += class_name.lower() + ": &" + class_ptr_name + ", "
        else:
            raise Exception("wrong self value " + self_val)

    if "fn_args" in f.keys():
        for arg_object in f["fn_args"]:
            arg_name = list(arg_object.keys())[0]
            if arg_name == "self":
                continue
            arg_type = arg_object[arg_name]

            if is_primitive_arg(arg_type):
                fn_args += arg_name + ": " + arg_type + ", " # no pre, no postfix
            elif class_is_virtual(apiData, arg_type):
                fn_args += arg_name + ": " + prefix + arg_type + ", " # no postfix
            else:
                fn_args += arg_name + ": " + prefix + arg_type + postfix + ", "
        fn_args = fn_args[:-2]

    return fn_args

# Returns if the class is "pure virtual", i.e. if it is an
# object consisting of patches instead of being defined in the API
def class_is_virtual(apiData, className):
    for module_name in apiData.keys():
        module = apiData[module_name]
        for class_name in module.keys():
            if class_name != className:
                continue
            c = module[class_name]
            if "use_patches" in c.keys() and c["use_patches"]:
                return True

    return False

def get_fn_args_c(f, class_name, class_ptr_name, apiData):
    fn_args = ""

    if "fn_args" in f.keys():
        for arg_object in f["fn_args"]:
            arg_name = list(arg_object.keys())[0]
            if arg_name == "self":
                continue
            arg_type = arg_object[arg_name]

            if is_primitive_arg(arg_type):
                fn_args += arg_name + ": " + arg_type + ", " # no pre, no postfix
            elif class_is_virtual(apiData, arg_type):
                fn_args += prefix + arg_type + arg_name + " " + ", " # no postfix
            else:
                fn_args += prefix + arg_type + postfix + arg_name + " " + ", "
        fn_args = fn_args[:-2]
        if (len(f["fn_args"]) == 0):
            fn_args = "void"

    return fn_args

# Generate the string for TAKING rust-api function arguments
def rust_bindings_fn_args(f, class_name, class_ptr_name, self_as_first_arg):
    fn_args = ""

    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        if (self_val == "value"):
            fn_args += "self, "
        elif (self_val == "refmut"):
            fn_args += "&mut self, "
        elif (self_val == "ref"):
            fn_args += "&self, "
        else:
            raise Exception("wrong self value " + self_val)

    if "fn_args" in f.keys():
        for arg_object in f["fn_args"]:
            arg_name = list(arg_object.keys())[0]
            if arg_name == "self":
                continue
            arg_type = arg_object[arg_name]
            fn_args += arg_name + ": " + arg_type + ", "
        fn_args = fn_args[:-2]

    return fn_args

# Generate the string for CALLING rust-api function args
def rust_bindings_call_fn_args(f, class_name, class_ptr_name, self_as_first_arg):
    fn_args = ""
    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        if (self_val == "value"):
            fn_args += "self.leak(), "
        elif (self_val == "refmut"):
            fn_args += "&mut self.ptr, "
        elif (self_val == "ref"):
            fn_args += "&self.ptr, "
        else:
            raise Exception("wrong self value " + self_val)

    if "fn_args" in f.keys():
        for arg_object in f["fn_args"]:
            arg_name = list(arg_object.keys())[0]
            if arg_name == "self":
                continue

            arg_type = arg_object[arg_name]
            if arg_type.startswith("&mut "):
                fn_args += "&mut " + arg_name + ".ptr, "
            elif arg_type.startswith("&"):
                fn_args += "&" + arg_name + ".ptr, "
            elif is_primitive_arg(arg_type):
                fn_args += arg_name + ", "
            else:
                fn_args += arg_name + ".leak(), "

        fn_args = fn_args[:-2]

    return fn_args


# ---------------------------------------------------------------------------------------------


# Generates the azul-dll/lib.rs file
def generate_rust_dll(apiData):

    version = list(apiData.keys())[-1]
    code = "// WARNING: autogenerated code for azul api version " + str(version) + "\r\n"
    code += "\r\n\r\n"

    apiData = apiData[version]

    if tuple(['*']) in dll_patches.keys():
        code += dll_patches[tuple(['*'])]

    for module_name in apiData.keys():
        module = apiData[module_name]

        if tuple([module_name]) in dll_patches.keys():
            code += dll_patches[tuple([module_name])]
            if "use_patches" in module.keys() and module["use_patches"]:
                continue

        for class_name in module.keys():
            c = module[class_name]

            code += "\r\n"

            if tuple([module_name, class_name]) in dll_patches.keys():
                code += dll_patches[tuple([module_name, class_name])]
                if "use_patches" in c.keys() and c["use_patches"]:
                    continue

            rust_class_name = class_name
            if "rust_class_name" in c.keys():
                rust_class_name = c["rust_class_name"]

            class_ptr_name = prefix + class_name + postfix;

            if "doc" in c.keys():
                code += "/// " + c["doc"] + "\r\n"
            else:
                code += "/// Pointer to rust-allocated `Box<" + class_name + ">` struct\r\n"

            if "external" in c.keys():
                external_path = c["external"]
                code += "pub use ::" + external_path + " as " + class_ptr_name + ";\r\n"
            else:
                code += "#[no_mangle] #[repr(C)] pub struct " + class_ptr_name + " { ptr: *mut c_void }\r\n"

            if "constructors" in c.keys():
                for const in c["constructors"]:

                    fn_name = const["fn_name"]

                    if tuple([module_name, class_name, fn_name]) in dll_patches.keys():
                        code += dll_patches[tuple([module_name, class_name, fn_name])]
                        if "use_patches" in const.keys() and const["use_patches"]:
                            continue

                    if "doc" in const.keys():
                        code += "/// " + const["doc"] + "\r\n"
                    else:
                        code += "// Creates a new `" + class_name + "` instance whose memory is owned by the rust allocator\r\n"
                        code += "// Equivalent to the Rust `" + class_name  + "::" + const["fn_name"] + "()` constructor.\r\n"

                    fn_args = fn_args_c_api(const, class_name, class_ptr_name, False, apiData)

                    code += "#[no_mangle] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_" + const["fn_name"] + "(" + fn_args + ") -> " + class_ptr_name + " { "
                    code += "let object: " + class_name + " = " + const["fn_body"] + "; " # note: security check, that the returned object is of the correct type
                    code += class_ptr_name + " { ptr: Box::into_raw(Box::new(object)) as *mut c_void }"
                    code += " }\r\n"

            if "functions" in c.keys():
                for f in c["functions"]:

                    fn_name = f["fn_name"]

                    if tuple([module_name, class_name, fn_name]) in dll_patches.keys():
                        code += dll_patches[tuple([module_name, class_name, fn_name])]
                        if "use_patches" in f.keys() and f["use_patches"]:
                            continue

                    if "doc" in f.keys():
                        code += "/// " + f["doc"] + "\r\n"
                    else:
                        code += "// Equivalent to the Rust `" + class_name  + "::" + f["fn_name"] + "()` function.\r\n"

                    fn_args = fn_args_c_api(f, class_name, class_ptr_name, True, apiData)

                    returns = ""
                    if "returns" in f.keys():
                        returns = " -> " + f["returns"]

                    code += "#[no_mangle] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_" + f["fn_name"] + "(" + fn_args + ")" + returns + " { "
                    code += f["fn_body"]
                    code += " }\r\n"

            lifetime = ""
            if "<'a>" in rust_class_name:
                lifetime = "<'a>"

            code += "/// Destructor: Takes ownership of the `" + class_name + "` pointer and deletes it.\r\n"
            code += "#[no_mangle] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_delete" + lifetime + "(ptr: &mut " + class_ptr_name + ") { "
            code += "let _ = unsafe { Box::<" + rust_class_name + ">::from_raw(ptr.ptr  as *mut " + rust_class_name + ") };"
            code += " }\r\n"

            code += "/// Copies the pointer: WARNING: After calling this function you'll have two pointers to the same Box<`" + class_name + "`>!.\r\n"
            code += "#[no_mangle] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_shallow_copy" + lifetime + "(ptr: &" + class_ptr_name + ") -> " + class_ptr_name + " { "
            code += class_ptr_name + " { ptr: ptr.ptr }"
            code += " }\r\n"

            code += "/// (private): Downcasts the `" + class_ptr_name + "` to a `Box<" + rust_class_name + ">`. Note that this takes ownership of the pointer.\r\n"
            code += "fn " + fn_prefix + to_snake_case(class_name) + "_downcast" + lifetime + "(ptr: " + class_ptr_name + ") -> Box<" + rust_class_name + "> { "
            code += "unsafe { Box::<" + rust_class_name + ">::from_raw(ptr.ptr  as *mut " + rust_class_name + ") }"
            code += " }\r\n"

    return code

# Generates the azul.h header
"""
def generate_c_api(apiData):

    version = list(apiData.keys())[-1]
    header = "// WARNING: autogenerated code for azul api version " + version + "\r\n\r\n"
    apiData = apiData[version]

    license = read_file(license_path)

    for line in license.splitlines():
        header += "// " + line + "\r\n"
    header += "\r\n\r\n"

    header += "#ifndef AZUL_GUI_H\r\n"
    header += "#define AZUL_GUI_H\r\n"
    header += "\r\n"
    header += "#include <stdarg.h>\r\n"
    header += "#include <stdbool.h>\r\n"
    header += "#include <stdint.h>\r\n"
    header += "#include <stdlib.h>\r\n"
    header += "\r\n"
    header += "\r\n"

    header += c_typedefs

    header += "\r\n"
    header += "\r\n"

    for module_name in apiData.keys():
        module = apiData[module_name]
        for class_name in module.keys():
            c = module[class_name]
            header += "\r\n"

            class_ptr_name = prefix + class_name + postfix;

            if "doc" in c.keys():
                header += "// " + c["doc"] + "\r\n"
            else:
                header += "// Pointer to rust-allocated `Box<" + class_name + ">` struct\r\n"

            header += "typedef struct " + class_ptr_name + " { void *ptr; } "  +  class_ptr_name + "\r\n"

            if "constructors" in c.keys():
                for const in c["constructors"]:
                    if "doc" in const.keys():
                        header += "// " + const["doc"] + "\r\n"
                    else:
                        header += "// Creates a new `" + class_name + "` instance whose memory is owned by the rust allocator\r\n"
                        header += "// Equivalent to the Rust `" + class_name  + "::" + const["fn_name"] + "()` constructor.\r\n"

                    fn_args = get_fn_args_c(const, class_name, class_ptr_name, apiData)

                    fn_name = fn_prefix + to_snake_case(class_name) + "_" + const["fn_name"]
                    header += class_ptr_name + " " + fn_name + "(" + fn_args + ");\r\n"

            if "functions" in c.keys():
                for f in c["functions"]:
                    if "doc" in f.keys():
                        header += "// " + f["doc"] + "\r\n"
                    else:
                        header += "// Equivalent to the Rust `" + class_name  + "::" + f["fn_name"] + "()` function.\r\n"

                    fn_args = get_fn_args_c(f, class_name, class_ptr_name, apiData)

                    fn_name = fn_prefix + to_snake_case(class_name) + "_" + f["fn_name"]
                    header += class_ptr_name + " " + fn_name + "(" + fn_args + ");\r\n"

            header += "// Destructor: Takes ownership of the `" + class_name + "` pointer and deletes it.\r\n"
            header += "void " + fn_prefix + to_snake_case(class_name) + "_delete(" + class_ptr_name + "* ptr);\r\n"

    header += "\r\n\r\n#endif /* AZUL_GUI_H */\r\n"

    return header
"""

# Generates the azul/rust/azul.rs file
def generate_rust_api(apiData):

    version = list(apiData.keys())[-1]
    code = "//! Auto-generated public Rust API for the Azul GUI toolkit version " + version + "\r\n"
    code += "//!\r\n"

    readme = read_file(azul_readme_path)

    for line in readme.splitlines():
        code += "//! " + line + "\r\n"
    code += "\r\n"

    license = read_file(license_path)

    for line in license.splitlines():
        code += "// " + line + "\r\n"
    code += "\r\n\r\n"

    code += "extern crate azul_dll;"
    code += "\r\n\r\n"

    if tuple(['*']) in rust_api_patches:
        code += rust_api_patches[tuple(['*'])]

    apiData = apiData[version]

    for module_name in apiData.keys():
        module = apiData[module_name]

        code += "#[allow(dead_code, unused_imports)]\r\n"
        code += "pub mod " + module_name + " {\r\n\r\n"
        code += "    use azul_dll::*;\r\n"

        if tuple([module_name]) in rust_api_patches:
            code += rust_api_patches[tuple([module_name])]

        code += get_all_imports(apiData, module, module_name, {"callbacks": ["RefAny", "LayoutInfo", "LayoutCallback"], "dom": ["Dom"]})

        for class_name in module.keys():
            c = module[class_name]

            class_ptr_name = prefix + class_name + postfix;

            code += "\r\n\r\n"

            if tuple([module_name, class_name]) in rust_api_patches.keys():
                code += rust_api_patches[tuple([module_name, class_name])]
                if "use_patches" in c.keys() and c["use_patches"]:
                    continue

            if "doc" in c.keys():
                code += "    /// " + c["doc"] + "\r\n    "
            else:
                code += "    /// `" + class_name + "` struct\r\n    "

            code += "pub struct " + class_name + " { pub(crate) ptr: " +  class_ptr_name + " }\r\n\r\n"

            code += "    impl " + class_name + " {\r\n"

            if "constructors" in c.keys():
                for const in c["constructors"]:

                    fn_name = const["fn_name"]

                    if tuple([module_name, class_name, fn_name]) in rust_api_patches:
                        code += rust_api_patches[tuple([module_name, class_name, fn_name])]
                        if "use_patches" in const.keys() and const["use_patches"]:
                            continue

                    if "doc" in const.keys():
                        code += "        /// " + const["doc"] + "\r\n"
                    else:
                        code += "        /// Creates a new `" + class_name + "` instance.\r\n"

                    fn_args = rust_bindings_fn_args(const, class_name, class_ptr_name, False)
                    fn_args_call = rust_bindings_call_fn_args(const, class_name, class_ptr_name, False)
                    c_fn_name = fn_prefix + to_snake_case(class_name) + "_" + fn_name
                    fn_body = c_fn_name + "(" + fn_args_call + ")"

                    code += "        pub fn " + fn_name + "(" + fn_args + ") -> Self { Self { ptr: " + fn_body + " } }\r\n"

            if "functions" in c.keys():
                for f in c["functions"]:

                    fn_name = f["fn_name"]
                    if tuple([module_name, class_name, fn_name]) in rust_api_patches:
                        code += rust_api_patches[tuple([module_name, class_name, fn_name])]
                        if "use_patches" in f.keys() and f["use_patches"]:
                            continue

                    if "doc" in f.keys():
                        code += "        /// " + f["doc"] + "\r\n"
                    else:
                        code += "        /// Calls the `" + class_name + "::" + f["fn_name"] + "` function.\r\n"

                    fn_args = rust_bindings_fn_args(f, class_name, class_ptr_name, True)
                    fn_args_call = rust_bindings_call_fn_args(f, class_name, class_ptr_name, True)
                    c_fn_name = fn_prefix + to_snake_case(class_name) + "_" + fn_name
                    fn_body = c_fn_name + "(" + fn_args_call + ")"

                    returns = ""
                    if "returns" in f.keys():
                        returns = " -> " + f["returns"]

                    code += "        pub fn " + fn_name + "(" + fn_args + ") " +  returns + " { " + fn_body + "}\r\n"

            code += "       /// Prevents the destructor from running and returns the internal `" + class_ptr_name + "`\r\n"
            code += "       #[allow(dead_code)]\r\n"
            code += "       pub(crate) fn leak(self) -> " + class_ptr_name + " { let p = " +  fn_prefix + to_snake_case(class_name) + "_shallow_copy(&self.ptr); std::mem::forget(self); p }\r\n"
            code += "    }\r\n\r\n"

            code += "    impl Drop for " + class_name + " { fn drop(&mut self) { " + fn_prefix + to_snake_case(class_name) + "_delete(&mut self.ptr); } }\r\n"

        code += "}\r\n\r\n"

    return code

"""
# TODO
# Generates the azul/cpp/azul.h file
def generate_cpp_api(apiData):
    return generate_c_api(apiData)

# TODO
# Generates the azul/python/azul.py file
def generate_python_api(apiData):
    return ""

# TODO
# Generates the azul/js/azul.js file (wasm preparation)
def generate_js_api(apiData):
    return ""
"""

def main():
    apiData = read_api_file(api_file_path)
    write_file(generate_rust_dll(apiData), rust_dll_path)
    write_file(generate_rust_api(apiData), rust_api_path)
    # write_file(generate_c_api(apiData), c_api_path)
    # write_file(generate_cpp_api(apiData), cpp_api_path)
    # write_file(generate_python_api(apiData), python_api_path)
    # write_file(generate_js_api(apiData), js_api_path)

if __name__ == "__main__":
    main()