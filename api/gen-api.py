import json
import re
import pprint

def read_file(path):
    text_file = open(path, 'r')
    text_file_contents = text_file.read()
    text_file.close()
    return text_file_contents

prefix = "Az"
fn_prefix = "az_"
postfix = "Ptr"
basic_types = [
    "bool", "char", "f32", "f64", "fn", "i128", "i16",
    "i32", "i64", "i8", "isize", "slice", "u128", "u16",
    "u32", "u64", "u8", "()", "usize", "c_void"
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
    tuple(['callbacks', 'LayoutCallback']): read_file("./patches/azul-dll/layout_callback.rs"),
    tuple(['callbacks', 'Callback']): read_file("./patches/azul-dll/callback.rs"),
    tuple(['callbacks', 'GlCallback']): read_file("./patches/azul-dll/gl_callback.rs"),
    tuple(['callbacks', 'IFrameCallback']): read_file("./patches/azul-dll/iframe_callback.rs"),
}

rust_api_patches = {
    tuple(['*']): read_file("./patches/azul.rs/header.rs"),
    tuple(['str']): read_file("./patches/azul.rs/string.rs"),
    tuple(['vec']): read_file("./patches/azul.rs/vec.rs"),
    tuple(['callbacks', 'RefAny']): read_file("./patches/azul.rs/refany.rs"),
    tuple(['callbacks', 'UpdateScreen']): read_file("./patches/azul.rs/update_screen.rs"),
    tuple(['callbacks', 'LayoutCallback']): read_file("./patches/azul.rs/layout_callback.rs"),
    tuple(['callbacks', 'Callback']): read_file("./patches/azul.rs/callback.rs"),
    tuple(['callbacks', 'GlCallback']): read_file("./patches/azul.rs/gl_callback.rs"),
    tuple(['callbacks', 'IFrameCallback']): read_file("./patches/azul.rs/iframe_callback.rs"),
    tuple(['app', 'App', 'new']): read_file("./patches/azul.rs/app_new.rs"),
}

c_api_patches = {
    tuple(['callbacks', 'LayoutCallback']): read_file("./patches/c/layout_callback.h"),
}

# ---------------------------------------------------------------------------------------------

def to_snake_case(name):
    s1 = re.sub('(.)([A-Z][a-z]+)', r'\1_\2', name)
    return re.sub('([a-z0-9])([A-Z])', r'\1_\2', s1).lower()

# turns a list of functi
# ex. "mut dom: AzDomPtr, event: AzEventFilter, data: AzRefAny, callback: AzCallback"
# ->  "_: AzDomPtr, _: AzEventFilter, _: AzRefAny, _: AzCallback"
def strip_fn_arg_types(arg_list):
    arg_list1 = re.sub(r'(.*): (.*),?', r'_: \2, ', arg_list)
    if arg_list1 != "":
        arg_list1 = arg_list1[:-2]
    return arg_list1.strip()


def read_api_file(path):
    api_file_contents = read_file(path)
    apiData = json.loads(api_file_contents)
    return apiData

def write_file(string, path):
    text_file = open(path, "w+")
    text_file.write(string)
    text_file.close()

def is_primitive_arg(arg):
    return get_stripped_arg(arg) in basic_types

def get_stripped_arg(arg):
    arg = arg.replace("&", "")
    arg = arg.replace("&mut", "")
    arg = arg.replace("*const", "")
    arg = arg.replace("*const", "")
    arg = arg.replace("*mut", "")
    arg = arg.strip()
    return arg

def search_imports_arg_type(c, search_type, arg_types_to_search):
    if search_type in c.keys():
        for fn_name in c[search_type]:
            const = c[search_type][fn_name]
            if "fn_args" in const.keys():
                for arg_object in const["fn_args"]:
                    arg_name = list(arg_object.keys())[0]
                    if arg_name == "self":
                        continue
                    arg_type = arg_object[arg_name]
                    arg_types_to_search.append(arg_type)

def get_all_imports(apiData, module, module_name):

    imports = {}

    arg_types_to_search = []

    for class_name in module.keys():
        c = module[class_name]
        search_imports_arg_type(c, "constructors", arg_types_to_search)
        search_imports_arg_type(c, "functions", arg_types_to_search)

    for arg in arg_types_to_search:

        arg = arg.replace("*const", "")
        arg = arg.replace("*mut", "")
        arg = arg.strip()

        if arg in basic_types:
            continue

        found_module = search_for_class_by_rust_class_name(apiData, arg)

        if found_module is None:
            raise Exception(arg + " not found!")

        if found_module[0] in imports:
            imports[found_module[0]].add(found_module[1])
        else:
            imports[found_module[0]] = {found_module[1]}

    if module_name in imports:
        del imports[module_name]

    imports_str = ""

    for module_name in imports.keys():
        classes = list(imports[module_name])
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
        elif (self_val == "mut value"):
            fn_args += "mut " + class_name.lower() + ": " + class_ptr_name + ", "
        elif (self_val == "refmut"):
            fn_args += class_name.lower() + ": &mut " + class_ptr_name + ", "
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

            analyzed_arg_type = analyze_type(arg_type)
            ptr_type = analyzed_arg_type[0]
            arg_type = analyzed_arg_type[1]

            if is_primitive_arg(arg_type):
                fn_args += arg_name + ": " + ptr_type + arg_type + ", " # no pre, no postfix
            else:
                arg_type_new = search_for_class_by_rust_class_name(apiData, arg_type)
                if arg_type_new is None:
                    print("arg_type not found: " + str(arg_type))
                    raise Exception("type not found: " + arg_type)
                arg_type = arg_type_new[1]
                if class_is_virtual(apiData, arg_type, "dll") or is_stack_allocated_type(apiData, arg_type):
                    fn_args += arg_name + ": " + ptr_type + prefix + arg_type + ", " # no postfix
                else:
                    fn_args += arg_name + ": " + ptr_type + prefix + arg_type + postfix + ", "
        fn_args = fn_args[:-2]

    return fn_args

def analyze_type(arg):
    starts = ""
    arg_type = ""
    ends = ""

    if arg.startswith("&mut"):
        starts = "&mut "
        arg_type = arg.replace("&mut", "")
    elif arg.startswith("&"):
        starts = "&"
        arg_type = arg.replace("&", "")
    elif arg.startswith("*const"):
        starts = "*const "
        arg_type = arg.replace("*const", "")
    elif arg.startswith("*mut"):
        starts = "*mut "
        arg_type = arg.replace("*mut", "")
    else:
        arg_type = arg

    arg_type = arg_type.strip()

    if arg_type.startswith("[") and arg_type.endswith("]"):
        starts += "["
        arg_type_array = arg_type[1:].split(";")
        arg_type = arg_type_array[0]
        ends += ";" + arg_type_array[1]

    return [starts, arg_type, ends]

def class_is_small_enum(c):
    return "enum_fields" in c.keys()

def class_is_small_struct(c):
    return "struct_fields" in c.keys()

def class_is_typedef(c):
    return "typedef" in c.keys()

def class_is_stack_allocated(c):
    class_is_boxed_object = not("external" in c.keys() and ("struct_fields" in c.keys() or "enum_fields" in c.keys() or "typedef" in c.keys() or "const" in c.keys()))
    return not(class_is_boxed_object)

# Find the [module, classname] given a rust_class_name, returns None if not found
# For example searching for "Vec<u8>" will return ["vec", "U8Vec"]
# Then you can use get_class() to get the class object
def search_for_class_by_rust_class_name(apiData, searched_rust_class_name):
    for module_name in apiData.keys():
        module = apiData[module_name]
        for class_name in module["classes"].keys():
            c = module["classes"][class_name]
            rust_class_name = class_name
            if "rust_class_name" in c.keys():
                rust_class_name = c["rust_class_name"]
            if rust_class_name == searched_rust_class_name or class_name == searched_rust_class_name:
                return [module_name, class_name]

    return None

def get_class(apiData, module_name, class_name):
    return apiData[module_name]["classes"][class_name]

# Returns whether a type is external, searches by rust_class_name instead of class_name
def is_stack_allocated_type(apiData, rust_class_name):
    search_result = search_for_class_by_rust_class_name(apiData, rust_class_name)
    if search_result is None:
        raise Exception("type not found " + rust_class_name)
    c = get_class(apiData, search_result[0], search_result[1])
    return class_is_stack_allocated(c)

# Returns if the class is "pure virtual", i.e. if it is an
# object consisting of patches instead of being defined in the API
def class_is_virtual(apiData, className, api):
    for module_name in apiData.keys():
        module = apiData[module_name]["classes"]
        for class_name in module.keys():
            if class_name != className:
                continue
            c = module[class_name]
            if "use_patches" in c.keys() and api in c["use_patches"]:
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
            elif class_is_virtual(apiData, arg_type, "rust"):
                fn_args += prefix + arg_type + arg_name + " " + ", " # no postfix
            else:
                fn_args += prefix + arg_type + postfix + arg_name + " " + ", "
        fn_args = fn_args[:-2]
        if (len(f["fn_args"]) == 0):
            fn_args = "void"

    return fn_args

# Generate the string for TAKING rust-api function arguments
def rust_bindings_fn_args(f, class_name, class_ptr_name, self_as_first_arg, apiData):
    fn_args = ""

    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        if (self_val == "value") or (self_val == "mut value"):
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
            arg_type = arg_type.strip()

            type_analyzed = analyze_type(arg_type)
            start = type_analyzed[0]
            arg_type = type_analyzed[1]

            if is_primitive_arg(arg_type):
                fn_args += arg_name + ": " + start + arg_type + ", " # usize
            else:
                arg_type_class_name = search_for_class_by_rust_class_name(apiData, arg_type)
                if arg_type_class_name is None:
                    raise Exception("arg type " + arg_type + " not found!")
                arg_type_class = get_class(apiData, arg_type_class_name[0], arg_type_class_name[1])

                if start == "*const " or start == "*mut ":
                    fn_args += arg_name + ": " + start + prefix + arg_type_class_name[1] + ", "
                else:
                    fn_args += arg_name + ": " + start + arg_type_class_name[1] + ", "

        fn_args = fn_args[:-2]

    return fn_args

# Generate the string for CALLING rust-api function args
def rust_bindings_call_fn_args(f, class_name, class_ptr_name, self_as_first_arg, apiData, class_is_boxed_object):
    fn_args = ""
    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        if (self_val == "value") or (self_val == "mut value"):
            fn_args += "self.leak(), "
        elif (self_val == "refmut"):
            if class_is_boxed_object:
                fn_args += "&mut self.ptr, "
            else:
                fn_args += "&mut self, "
        elif (self_val == "ref"):
            if class_is_boxed_object:
                fn_args += "&self.ptr, "
            else:
                fn_args += "&self, "
        else:
            raise Exception("wrong self value " + self_val)

    if "fn_args" in f.keys():
        for arg_object in f["fn_args"]:
            arg_name = list(arg_object.keys())[0]
            if arg_name == "self":
                continue

            arg_type = arg_object[arg_name].strip()
            starts = ""
            type_analyzed = analyze_type(arg_type)
            start = type_analyzed[0]
            arg_type = type_analyzed[1]

            if is_primitive_arg(arg_type):
                fn_args += arg_name + ", "
            else:
                arg_type = arg_type.strip()
                arg_type_class = search_for_class_by_rust_class_name(apiData, arg_type)
                if arg_type_class is None:
                    raise Exception("arg type " + arg_type + " not found!")
                arg_type_class = get_class(apiData, arg_type_class[0], arg_type_class[1])

                if start == "*const " or start == "*mut ":
                    fn_args += arg_name + ", "
                else:
                    if class_is_typedef(arg_type_class):
                        fn_args += start + arg_name + ", "
                    elif class_is_stack_allocated(arg_type_class):
                        fn_args += start + arg_name + ", " # .object
                    else:
                        fn_args += start + arg_name + ".leak(), "

        fn_args = fn_args[:-2]

    return fn_args


# ---------------------------------------------------------------------------------------------


# Generates the azul-dll/lib.rs file
def generate_rust_dll(apiData):

    version = list(apiData.keys())[-1]
    code = "// WARNING: autogenerated code for azul api version " + str(version) + "\r\n"
    code += "\r\n\r\n"

    apiData = apiData[version]

    structs_map = {}
    functions_map = {}

    if tuple(['*']) in dll_patches.keys():
        code += dll_patches[tuple(['*'])]

    for module_name in apiData.keys():
        module = apiData[module_name]["classes"]

        if tuple([module_name]) in dll_patches.keys() and "use_patches" in module.keys() and "dll" in module["use_patches"]:
            code += dll_patches[tuple([module_name])]
            continue

        for class_name in module.keys():
            c = module[class_name]

            code += "\r\n"

            if tuple([module_name, class_name]) in dll_patches.keys() and "use_patches" in c.keys() and "dll" in c["use_patches"]:
                code += dll_patches[tuple([module_name, class_name])]
                continue

            rust_class_name = class_name
            if "rust_class_name" in c.keys():
                rust_class_name = c["rust_class_name"]

            class_is_boxed_object = not(class_is_stack_allocated(c))
            class_is_const = "const" in c.keys()
            class_is_typedef = "typedef" in c.keys() and c["typedef"]
            treat_external_as_ptr = "external" in c.keys() and "is_boxed_object" in c.keys() and c["is_boxed_object"]

            # Small structs and enums are stack-allocated in order to save on indirection
            # They don't have destructors, since they
            c_is_stack_allocated = not(class_is_boxed_object)

            if c_is_stack_allocated:
                class_ptr_name = prefix + class_name
            else:
                class_ptr_name = prefix + class_name + postfix

            if "doc" in c.keys():
                code += "/// " + c["doc"] + "\r\n"
            else:
                if c_is_stack_allocated:
                    code += "/// Re-export of rust-allocated (stack based) `" + class_name + "` struct\r\n"
                else:
                    code += "/// Pointer to rust-allocated `Box<" + class_name + ">` struct\r\n"

            if "external" in c.keys():
                external_path = c["external"]
                if class_is_const:
                    code += "#[no_mangle] pub static " + class_ptr_name + ": " + prefix + c["const"] + " = " + external_path + ";\r\n"
                elif class_is_typedef:
                    code += "#[no_mangle] pub type " + class_ptr_name + " = " + external_path + ";\r\n"
                elif class_is_boxed_object:
                    structs_map[class_ptr_name] = {"struct": [{"ptr": {"type": "*mut c_void" }}]}
                    if treat_external_as_ptr:
                        code += "pub type " + class_ptr_name + "Type = " + external_path + ";\r\n"
                        code += "#[no_mangle] pub use " + class_ptr_name + "Type as " + class_ptr_name + ";\r\n"
                    else:
                        code += "#[no_mangle] #[repr(C)] pub struct " + class_ptr_name + " { pub ptr: *mut c_void }\r\n"
                else:
                    if "struct_fields" in c.keys():
                        structs_map[class_ptr_name] = {"struct": c["struct_fields"]}
                    elif "enum_fields" in c.keys():
                        structs_map[class_ptr_name] = {"enum": c["enum_fields"]}
                    code += "pub use " + external_path + " as " + class_ptr_name + ";\r\n"

            else:
                structs_map[class_ptr_name] = {"struct": [{"ptr": {"type": "*mut c_void"}}]}
                code += "#[no_mangle] #[repr(C)] pub struct " + class_ptr_name + " { ptr: *mut c_void }\r\n"

            if "constructors" in c.keys():
                for fn_name in c["constructors"]:

                    const = c["constructors"][fn_name]

                    fn_body = ""

                    if tuple([module_name, class_name, fn_name]) in dll_patches.keys() \
                    and "use_patches" in const.keys() \
                    and "dll" in const["use_patches"]:
                        fn_body = dll_patches[tuple([module_name, class_name, fn_name])]
                    else:
                        fn_body += "let object: " + rust_class_name + " = " + const["fn_body"] + "; " # note: security check, that the returned object is of the correct type
                        if c_is_stack_allocated:
                            fn_body += "object"
                        else:
                            fn_body += class_ptr_name + " { ptr: Box::into_raw(Box::new(object)) as *mut c_void }"

                    if "doc" in const.keys():
                        code += "/// " + const["doc"] + "\r\n"
                    else:
                        code += "// Creates a new `" + class_name + "` instance whose memory is owned by the rust allocator\r\n"
                        code += "// Equivalent to the Rust `" + class_name  + "::" + fn_name + "()` constructor.\r\n"

                    fn_args = fn_args_c_api(const, class_name, class_ptr_name, False, apiData)

                    functions_map[str(fn_prefix + to_snake_case(class_name) + "_" + fn_name)] = [fn_args, class_ptr_name];
                    code += "#[no_mangle] #[inline] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_" + fn_name + "(" + fn_args + ") -> " + class_ptr_name + " { "
                    code += fn_body
                    code += " }\r\n"

            if "functions" in c.keys():
                for fn_name in c["functions"]:

                    f = c["functions"][fn_name]

                    fn_body = ""
                    if tuple([module_name, class_name, fn_name]) in dll_patches.keys() \
                    and "use_patches" in f.keys() \
                    and "dll" in f["use_patches"]:
                        fn_body = dll_patches[tuple([module_name, class_name, fn_name])]
                    else:
                        fn_body = f["fn_body"]

                    if "doc" in f.keys():
                        code += "/// " + f["doc"] + "\r\n"
                    else:
                        code += "// Equivalent to the Rust `" + class_name  + "::" + fn_name + "()` function.\r\n"

                    fn_args = fn_args_c_api(f, class_name, class_ptr_name, True, apiData)

                    returns = ""
                    if "returns" in f.keys():
                        return_type = f["returns"]
                        if is_primitive_arg(return_type):
                            returns = return_type
                        else:
                            return_type_class = search_for_class_by_rust_class_name(apiData, return_type)
                            if return_type_class is None:
                                print("rust-dll: (line 549): no return_type_class found for " + return_type)
                            if class_is_stack_allocated(get_class(apiData, return_type_class[0], return_type_class[1])):
                                returns = prefix + return_type_class[1] # no postfix
                            else:
                                returns = prefix + return_type_class[1] + postfix

                    functions_map[str(fn_prefix + to_snake_case(class_name) + "_" + fn_name)] = [fn_args, returns];
                    return_arrow = "" if returns == "" else " -> "
                    code += "#[no_mangle] #[inline] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_" + fn_name + "(" + fn_args + ")" + return_arrow + returns + " { "
                    code += fn_body
                    code += " }\r\n"

            lifetime = ""
            if "<'a>" in rust_class_name:
                lifetime = "<'a>"


            if c_is_stack_allocated:
                if not(class_is_const or class_is_typedef):

                    # Generate the destructor for an enum
                    stack_delete_body = ""
                    if class_is_small_enum(c):
                        stack_delete_body += "match object { "
                        for enum_variant in c["enum_fields"]:
                            enum_variant_name = list(enum_variant.keys())[0]
                            enum_variant = list(enum_variant.values())[0]
                            if "type" in enum_variant.keys():
                                stack_delete_body += c["external"] + "::" + enum_variant_name + "(_) => { }, "
                            else:
                                stack_delete_body += c["external"] + "::" + enum_variant_name + " => { }, "
                        stack_delete_body += "}\r\n"

                    # az_item_delete()
                    code += "/// Destructor: Takes ownership of the `" + class_name + "` pointer and deletes it.\r\n"
                    functions_map[str(fn_prefix + to_snake_case(class_name) + "_delete")] = ["object: &mut " + class_ptr_name, ""];
                    code += "#[no_mangle] #[inline] #[allow(unused_variables)] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_delete" + lifetime + "(object: &mut " + class_ptr_name + ") { " + stack_delete_body + "}\r\n"

                    # az_item_deep_copy()
                    code += "/// Copies the object\r\n"
                    functions_map[str(fn_prefix + to_snake_case(class_name) + "_deep_copy")] = ["object: &" + class_ptr_name, class_ptr_name];
                    code += "#[no_mangle] #[inline] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_deep_copy" + lifetime + "(object: &" + class_ptr_name + ") -> " + class_ptr_name + " { "
                    code += "object.clone()"
                    code += " }\r\n"
            else:
                # az_item_delete()
                code += "/// Destructor: Takes ownership of the `" + class_name + "` pointer and deletes it.\r\n"
                functions_map[str(fn_prefix + to_snake_case(class_name) + "_delete")] = ["ptr: &mut " + class_ptr_name, ""];
                code += "#[no_mangle] #[inline] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_delete" + lifetime + "(ptr: &mut " + class_ptr_name + ") { "
                code += "let _ = unsafe { Box::<" + rust_class_name + ">::from_raw(ptr.ptr  as *mut " + rust_class_name + ") };"
                code += " }\r\n"

                # az_item_shallow_copy()
                code += "/// Copies the pointer: WARNING: After calling this function you'll have two pointers to the same Box<`" + class_name + "`>!.\r\n"
                functions_map[str(fn_prefix + to_snake_case(class_name) + "_shallow_copy")] = ["ptr: &" + class_ptr_name, class_ptr_name];
                code += "#[no_mangle] #[inline] pub extern \"C\" fn " + fn_prefix + to_snake_case(class_name) + "_shallow_copy" + lifetime + "(ptr: &" + class_ptr_name + ") -> " + class_ptr_name + " { "
                code += class_ptr_name + " { ptr: ptr.ptr }"
                code += " }\r\n"

                # az_item_downcast()
                code += "/// (private): Downcasts the `" + class_ptr_name + "` to a `Box<" + rust_class_name + ">`. Note that this takes ownership of the pointer.\r\n"
                code += "#[inline(always)] fn " + fn_prefix + to_snake_case(class_name) + "_downcast" + lifetime + "(ptr: " + class_ptr_name + ") -> Box<" + rust_class_name + "> { "
                code += "unsafe { Box::<" + rust_class_name + ">::from_raw(ptr.ptr  as *mut " + rust_class_name + ") }"
                code += " }\r\n"

                # az_item_downcast_refmut()
                downcast_refmut_generics = "<F: FnOnce(&mut Box<" + rust_class_name + ">)>"
                if lifetime == "<'a>":
                    downcast_refmut_generics = "<'a, F: FnOnce(&mut Box<" + rust_class_name + ">)>"
                code += "/// (private): Downcasts the `" + class_ptr_name + "` to a `&mut Box<" + rust_class_name + ">` and runs the `func` closure on it\r\n"
                code += "#[inline(always)] fn " + fn_prefix + to_snake_case(class_name) + "_downcast_refmut" + downcast_refmut_generics + "(ptr: &mut " + class_ptr_name + ", func: F) { "
                code += "let mut box_ptr: Box<" + rust_class_name + "> = unsafe { Box::<" + rust_class_name + ">::from_raw(ptr.ptr  as *mut " + rust_class_name + ") };"
                code += "func(&mut box_ptr);"
                code += "ptr.ptr = Box::into_raw(box_ptr) as *mut c_void;"
                code += " }\r\n"

                # az_item_downcast_ref()
                downcast_ref_generics = "<P, F: FnOnce(&Box<" + rust_class_name + ">) -> P>"
                if lifetime == "<'a>":
                    downcast_ref_generics = "<'a, P, F: FnOnce(&Box<" + rust_class_name + ">) -> P>"
                code += "/// (private): Downcasts the `" + class_ptr_name + "` to a `&Box<" + rust_class_name + ">` and runs the `func` closure on it\r\n"
                code += "#[inline(always)] fn " + fn_prefix + to_snake_case(class_name) + "_downcast_ref" + downcast_ref_generics + "(ptr: &mut " + class_ptr_name + ", func: F) -> P { "
                code += "let box_ptr: Box<" + rust_class_name + "> = unsafe { Box::<" + rust_class_name + ">::from_raw(ptr.ptr  as *mut " + rust_class_name + ") }; "
                code += "let ret_val = func(&box_ptr); "
                code += "ptr.ptr = Box::into_raw(box_ptr) as *mut c_void;"
                code += "ret_val"
                code += " }\r\n"

    return [code, structs_map, functions_map]

def generate_dll_loader(apiData, structs_map, functions_map):
    code = "extern crate libloading;\r\n\r\n"

    code += "pub(crate) mod dll {\r\n\r\n"
    code += "    use std::ffi::c_void;\r\n\r\n"
    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]
        if "struct" in struct.keys():
            struct = struct["struct"]
            code += "    #[repr(C)] pub struct " + struct_name + " {\r\n"
            for field in struct:
                field_name = list(field.keys())[0]
                field_type = list(field.values())[0]
                if "type" in field_type:
                    field_type = field_type["type"]
                    if is_primitive_arg(field_type):
                        code += "        pub " + field_name + ": " + field_type + ",\r\n"
                    else:
                        analyzed_arg_type = analyze_type(field_type)
                        field_type_class_path = search_for_class_by_rust_class_name(apiData, analyzed_arg_type[1])
                        if field_type_class_path is None:
                            print("no field_type_class_path found for " + str(analyzed_arg_type))
                        code += "        pub " + field_name + ": " + analyzed_arg_type[0] + prefix + field_type_class_path[1] + analyzed_arg_type[2] + ",\r\n"
                else:
                    print("struct " + struct_name + " does not have a type on field " + field_name)
                    raise Exception("error")
            code += "    }\r\n"
        elif "enum" in struct.keys():
            enum = struct["enum"]
            repr = "#[repr(C)]"
            for variant in enum:
                variant_name = list(variant.keys())[0]
                variant = list(variant.values())[0]
                if "type" in variant.keys():
                    repr = "#[repr(C, u8)]"

            code += "    " + repr + " pub enum " + struct_name + " {\r\n"
            for variant in enum:
                variant_name = list(variant.keys())[0]
                variant = list(variant.values())[0]
                if "type" in variant.keys():
                    variant_type = variant["type"]
                    if is_primitive_arg(variant_type):
                        code += "        " + variant_name + "(" + variant_type + "),\r\n"
                    else:
                        analyzed_arg_type = analyze_type(variant_type)
                        field_type_class_path = search_for_class_by_rust_class_name(apiData, analyzed_arg_type[1])
                        code += "        " + variant_name + "(" + analyzed_arg_type[0] + prefix + field_type_class_path[1] + analyzed_arg_type[2] + "),\r\n"
                else:
                    code += "        " + variant_name + ",\r\n"
            code += "    }\r\n"

    code += "\r\n"
    code += "\r\n"

    code += "    #[cfg(unix)]\r\n"
    code += "    use libloading::os::unix::{Library, Symbol};\r\n"
    code += "    #[cfg(windows)]\r\n"
    code += "    use libloading::os::windows::{Library, Symbol};\r\n\r\n"

    code += "    pub struct AzulDll {\r\n"
    code += "        lib: Box<Library>,\r\n"
    for fn_name in functions_map.keys():
        fn_type = functions_map[fn_name]
        fn_args = fn_type[0]
        fn_return = fn_type[1]
        return_arrow = "" if fn_return == "" else " -> "
        code += "        " + fn_name + ": Symbol<extern fn(" + strip_fn_arg_types(fn_args) + ")" + return_arrow + fn_return + ">,\r\n"
    code += "    }\r\n\r\n"

    code += "    pub fn initialize_library(path: &str) -> Option<AzulDll> {\r\n"
    code += "        let lib = Library::new(path).ok()?;\r\n"
    for fn_name in functions_map.keys():
        fn_type = functions_map[fn_name]
        fn_args = fn_type[0]
        fn_return = fn_type[1]
        return_arrow = "" if fn_return == "" else " -> "
        code += "        let " + fn_name + " = unsafe { lib.get::<extern fn(" + strip_fn_arg_types(fn_args) + ")" + return_arrow + fn_return + ">(b\"" + fn_name + "\").ok()? };\r\n"

    code += "        Some(AzulDll {\r\n"
    code += "            lib: Box::new(lib),\r\n"
    for fn_name in functions_map.keys():
        code += "            " + fn_name + ",\r\n"

    code += "        })\r\n"
    code += "    }\r\n"
    code += "}\r\n\r\n"

    return code

# Generates the azul/rust/azul.rs file
def generate_rust_api(apiData, structs_map, functions_map):

    version = list(apiData.keys())[-1]
    code = "//! Auto-generated public Rust API for the Azul GUI toolkit version " + version + "\r\n"
    code += "//!\r\n"

    # readme = read_file(azul_readme_path)
    #
    # for line in readme.splitlines():
    #     code += "//! " + line + "\r\n"
    # code += "\r\n"

    license = read_file(license_path)

    for line in license.splitlines():
        code += "// " + line + "\r\n"
    code += "\r\n\r\n"

    code += "extern crate azul_dll;"
    code += "\r\n\r\n"

    apiData = apiData[version]

    code += generate_dll_loader(apiData, structs_map, functions_map)

    if tuple(['*']) in rust_api_patches:
        code += rust_api_patches[tuple(['*'])]

    for module_name in apiData.keys():
        module_doc = None
        if "doc" in apiData[module_name]:
            module_doc = apiData[module_name]["doc"]

        module = apiData[module_name]["classes"]

        if module_doc != None:
            code += "/// " + module_doc + "\r\n"

        code += "#[allow(dead_code, unused_imports)]\r\n"
        code += "pub mod " + module_name + " {\r\n\r\n"
        code += "    use azul_dll::*;\r\n"

        if tuple([module_name]) in rust_api_patches:
            code += rust_api_patches[tuple([module_name])]

        code += get_all_imports(apiData, module, module_name)

        for class_name in module.keys():
            c = module[class_name]

            class_is_boxed_object = not(class_is_stack_allocated(c))
            class_is_const = "const" in c.keys()
            class_is_typedef = "typedef" in c.keys() and c["typedef"]
            treat_external_as_ptr = "external" in c.keys() and "is_boxed_object" in c.keys() and c["is_boxed_object"]

            c_is_stack_allocated = not(class_is_boxed_object)
            class_ptr_name = prefix + class_name + postfix
            if c_is_stack_allocated:
                class_ptr_name = prefix + class_name

            code += "\r\n\r\n"

            if tuple([module_name, class_name]) in rust_api_patches.keys() and "use_patches" in c.keys() and "rust" in c["use_patches"]:
                code += rust_api_patches[tuple([module_name, class_name])]
                continue

            if "doc" in c.keys():
                code += "    /// " + c["doc"] + "\r\n    "
            else:
                code += "    /// `" + class_name + "` struct\r\n    "

            if c_is_stack_allocated:
                if class_is_typedef:
                    code += "pub struct " + class_name + " { pub(crate) object: " +  class_ptr_name + " }\r\n    "
                elif class_is_const:
                    code += "pub static " + to_snake_case(class_name).upper() + ": " + prefix + c["const"] + " = " + class_ptr_name + ";\r\n\r\n"
                else:
                    code += "pub struct " + class_name + " { pub(crate) object: " +  class_ptr_name + " }\r\n\r\n"
            else:
                code += "pub struct " + class_name + " { pub(crate) ptr: " +  class_ptr_name + " }\r\n\r\n"

            if not(class_is_const or class_is_typedef):
                code += "    impl " + class_name + " {\r\n"

            if c_is_stack_allocated:
                if class_is_small_enum(c):
                    for enum_variant in c["enum_fields"]:
                        enum_variant_name = list(enum_variant.keys())[0]
                        enum_variant = list(enum_variant.values())[0]
                        if "doc" in enum_variant.keys():
                            code += "        /// " + enum_variant["doc"] + "\r\n"
                        if "type" in enum_variant.keys():
                            variant_type = enum_variant["type"]
                            if is_primitive_arg(variant_type):
                                code += "        pub fn " + to_snake_case(enum_variant_name) + "(variant_data: " + variant_type + ") -> Self { "
                                code += fn_prefix + to_snake_case(class_name) + "_" + to_snake_case(enum_variant_name) + "(variant_data)"
                                code += "}\r\n"
                            else:
                                analyzed_arg_type = analyze_type(variant_type)
                                found_class_path = search_for_class_by_rust_class_name(apiData, analyzed_arg_type[1])
                                stack_enum_args = analyzed_arg_type[0] + "crate::" + found_class_path[0] + "::" + found_class_path[1] + analyzed_arg_type[2]
                                code += "        pub fn " + to_snake_case(enum_variant_name) + "(variant_data: " + stack_enum_args + ") -> Self { "
                                code += fn_prefix + to_snake_case(class_name) + "_" + to_snake_case(enum_variant_name) + "(variant_data.leak())"
                                code += "}\r\n"
                        else:
                            # enum variant with no arguments
                            code += "        pub fn " + to_snake_case(enum_variant_name) + "() -> Self { "
                            code += fn_prefix + to_snake_case(class_name) + "_" + to_snake_case(enum_variant_name) + "() "
                            code += " }\r\n"

            if "constructors" in c.keys():
                for fn_name in c["constructors"]:
                    const = c["constructors"][fn_name]

                    c_fn_name = fn_prefix + to_snake_case(class_name) + "_" + fn_name
                    fn_args = rust_bindings_fn_args(const, class_name, class_ptr_name, False, apiData)
                    fn_args_call = rust_bindings_call_fn_args(const, class_name, class_ptr_name, False, apiData, class_is_boxed_object)

                    fn_body = ""

                    if tuple([module_name, class_name, fn_name]) in rust_api_patches.keys() \
                    and "use_patches" in const.keys() \
                    and "rust" in const["use_patches"]:
                        fn_body = rust_api_patches[tuple([module_name, class_name, fn_name])]
                    else:
                        if c_is_stack_allocated:
                            fn_body = c_fn_name + "(" + fn_args_call + ")"
                        else:
                            fn_body = "Self { ptr: " + c_fn_name + "(" + fn_args_call + ") }"

                    if "doc" in const.keys():
                        code += "        /// " + const["doc"] + "\r\n"
                    else:
                        code += "        /// Creates a new `" + class_name + "` instance.\r\n"

                    code += "        pub fn " + fn_name + "(" + fn_args + ") -> Self { " + fn_body + " }\r\n"

            if "functions" in c.keys():
                for fn_name in c["functions"]:
                    f = c["functions"][fn_name]

                    fn_args = rust_bindings_fn_args(f, class_name, class_ptr_name, True, apiData)
                    fn_args_call = rust_bindings_call_fn_args(f, class_name, class_ptr_name, True, apiData, class_is_boxed_object)
                    c_fn_name = fn_prefix + to_snake_case(class_name) + "_" + fn_name

                    fn_body = ""

                    if tuple([module_name, class_name, fn_name]) in rust_api_patches.keys() \
                    and "use_patches" in const.keys() \
                    and "rust" in const["use_patches"]:
                        fn_body = rust_api_patches[tuple([module_name, class_name, fn_name])]
                    else:
                        fn_body = c_fn_name + "(" + fn_args_call + ")"

                    if tuple([module_name, class_name, fn_name]) in rust_api_patches:
                        code += rust_api_patches[tuple([module_name, class_name, fn_name])]
                        if "use_patches" in f.keys() and f["use_patches"]:
                            continue

                    if "doc" in f.keys():
                        code += "        /// " + f["doc"] + "\r\n"
                    else:
                        code += "        /// Calls the `" + class_name + "::" + fn_name + "` function.\r\n"

                    returns = ""
                    if "returns" in f.keys():
                        return_type = f["returns"]
                        returns = " -> " + return_type
                        if is_primitive_arg(return_type):
                            fn_body = fn_body
                        else:
                            return_type_class = search_for_class_by_rust_class_name(apiData, return_type)
                            returns = " -> crate::" + return_type_class[0] + "::" + return_type_class[1]
                            if class_is_stack_allocated(get_class(apiData, return_type_class[0], return_type_class[1])):
                                fn_body = "{ " + fn_body + "}"
                            else:
                                fn_body = "crate::" + return_type_class[0] + "::" + return_type_class[1] + " { ptr: { " + fn_body + " } }"

                    code += "        pub fn " + fn_name + "(" + fn_args + ") " +  returns + " { " + fn_body + " }\r\n"

            if not(class_is_const or class_is_typedef):
                if class_is_boxed_object:
                    leak_fn_body = "let p = " +  fn_prefix + to_snake_case(class_name) + "_shallow_copy(&self.ptr); std::mem::forget(self); p"
                    code += "       /// Prevents the destructor from running and returns the internal `" + class_ptr_name + "`\r\n"
                    code += "       pub fn leak(self) -> " + class_ptr_name + " { " + leak_fn_body + " }\r\n"

                code += "    }\r\n\r\n" # end of class

                destructor_fn_object = "self.ptr" if class_is_boxed_object else "self"
                code += "    impl Drop for " + class_name + " { fn drop(&mut self) { " + fn_prefix + to_snake_case(class_name) + "_delete(&mut " + destructor_fn_object + "); } }\r\n"

        code += "}\r\n\r\n" # end of module

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
    rust_dll_result = generate_rust_dll(apiData)
    write_file(rust_dll_result[0], rust_dll_path)
    write_file(generate_rust_api(apiData, rust_dll_result[1], rust_dll_result[2]), rust_api_path)
    # write_file(generate_c_api(apiData), c_api_path)
    # write_file(generate_cpp_api(apiData), cpp_api_path)
    # write_file(generate_python_api(apiData), python_api_path)
    # write_file(generate_js_api(apiData), js_api_path)

if __name__ == "__main__":
    main()