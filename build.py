import json
import re
import pprint
import os
import subprocess
import shutil
from sys import platform
import time

# dict that keeps the order of insertion
from collections import OrderedDict

def create_folder(path):
    os.mkdir(path)

def remove_path(path):
    """ param <path> could either be relative or absolute. """
    if os.path.isfile(path) or os.path.islink(path):
        os.remove(path)  # remove the file
    elif os.path.isdir(path):
        shutil.rmtree(path)  # remove dir and all contains
    else:
        raise ValueError("file {} is not a file or dir.".format(path))

def zip_directory(output_filename, dir_name):
    shutil.make_archive(output_filename, 'zip', dir_name)

def copy_file(src, dest):
    shutil.copyfile(src, dest)

def read_file(path):
    text_file = open(path, 'r')
    text_file_contents = text_file.read()
    text_file.close()
    return text_file_contents

def read_api_file(path):
    api_file_contents = read_file(path)
    apiData = json.loads(api_file_contents)
    return apiData

html_root = "https://azul.rs"
root_folder = os.path.abspath(os.path.join(__file__, os.pardir))
prefix = "Az"
basic_types = [ # note: "char" is not a primitive type! - use u32 instead
    "bool", "f32", "f64", "fn", "i128", "i16",
    "i32", "i64", "i8", "isize", "slice", "u128", "u16",
    "u32", "u64", "u8", "()", "usize", "c_void"
]

license = read_file(root_folder + "/LICENSE")

rust_api_patches = {
    tuple(['str']): read_file(root_folder + "/api/_patches/azul.rs/string.rs"),
    tuple(['vec']): read_file(root_folder + "/api/_patches/azul.rs/vec.rs"),
    tuple(['option']): read_file(root_folder + "/api/_patches/azul.rs/option.rs"),
    tuple(['dom']): read_file(root_folder + "/api/_patches/azul.rs/dom.rs"),
    tuple(['gl']): read_file(root_folder + "/api/_patches/azul.rs/gl.rs"),
    tuple(['css']): read_file(root_folder + "/api/_patches/azul.rs/css.rs"),
    tuple(['window']): read_file(root_folder + "/api/_patches/azul.rs/window.rs"),
    tuple(['callbacks']): read_file(root_folder + "/api/_patches/azul.rs/callbacks.rs"),
}

# ---------------------------------------------------------------------------------------------

def snake_case_to_lower_camel(snake_str):
    first, *others = snake_str.split('_')
    return ''.join([first.lower(), *map(str.title, others)])

# turns a list of function args into function pointer args
# ex. "mut dom: AzDom, event: AzEventFilter, data: AzRefAny, callback: AzCallback"
# ->  "_: AzDom, _: AzEventFilter, _: AzRefAny, _: AzCallback"
def strip_fn_arg_types(arg_list):
    if len(arg_list) == 0:
        return ""

    arg_list1 = ""

    for item in arg_list.split(","):
        part_b = item.split(":")[1]
        arg_list1 += "_: " + part_b + ", "

    if arg_list1 != "":
        arg_list1 = arg_list1[:-2]

    return arg_list1.strip()

def write_file(string, path):
    text_file = open(path, "w+", newline='')
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

        found_module = search_for_class_by_class_name(apiData, arg)

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
            classes.sort()
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
            raise Exception("wrong self value " + self_val + " " + class_name)

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
                arg_type_new = search_for_class_by_class_name(apiData, arg_type)
                if arg_type_new is None:
                    print("arg_type not found: " + str(arg_type))
                    raise Exception("type not found: " + arg_type)
                arg_type = arg_type_new[1]
                fn_args += arg_name + ": " + ptr_type + prefix + arg_type + ", " # no postfix
        fn_args = fn_args[:-2]

    return fn_args

def c_fn_args_c_api(f, class_name, class_ptr_name, self_as_first_arg):
    fn_args = ""

    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        if (self_val == "value"):
            fn_args += "const " + class_ptr_name + " " + class_name.lower() + ", "
        elif (self_val == "mut value"):
            fn_args += "restrict " + class_ptr_name + ": " + class_name.lower() + ", "
        elif (self_val == "refmut"):
            fn_args += class_ptr_name + "* restrict " + class_name.lower() + ", "
        elif (self_val == "ref"):
            fn_args += "const " + class_ptr_name + "* " + class_name.lower() + ", "
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
                if ptr_type == "*const":
                    fn_args += "const" + replace_primitive_ctype(arg_type) + "* " + arg_name + ", " # no pre, no postfix
                elif ptr_type == "*mut":
                    fn_args += replace_primitive_ctype(arg_type) + "* restrict" + " " + arg_name + ", " # no pre, no postfix
                else:
                    fn_args += replace_primitive_ctype(arg_type) + " " + arg_name + ", " # no pre, no postfix
            else:
                fn_args += prefix + replace_primitive_ctype(arg_type) + replace_primitive_ctype(ptr_type).strip() + " " + arg_name + ", " # no postfix

        fn_args = fn_args[:-2]

    return fn_args

def analyze_type(arg):
    starts = ""
    arg_type = ""
    ends = ""

    if type(arg) is dict:
        print("expected string, got dict: " + str(arg))

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
    return "callback_typedef" in c.keys()

def class_is_stack_allocated(c):
    class_is_boxed_object = not("external" in c.keys() and ("struct_fields" in c.keys() or "enum_fields" in c.keys() or "callback_typedef" in c.keys() or "const" in c.keys()))
    return not(class_is_boxed_object)

# Same as calling get_class(search_class_by_name())
def quick_get_class(api_data, searched_class_name):
    field_type_class_path = search_for_class_by_class_name(api_data, searched_class_name)
    if field_type_class_path is None:
        print("quick_get_class: could not find: " + searched_class_name)
        raise "quick_get_class: could not find: " + searched_class_name
    found_c = get_class(api_data, field_type_class_path[0], field_type_class_path[1])
    if found_c is None:
        print("quick_get_class: could not find: " + searched_class_name)
        raise "quick_get_class: could not find: " + searched_class_name
    return found_c

# Find the [module, classname] given a class_name, returns None if not found
# Then you can use get_class() to get the class object
def search_for_class_by_class_name(api_data, searched_class_name):
    for module_name in api_data.keys():
        module = api_data[module_name]
        for class_name in module["classes"].keys():
            c = module["classes"][class_name]
            class_name = class_name
            if class_name == searched_class_name or class_name == searched_class_name:
                return [module_name, class_name]

    return None

def get_class(api_data, module_name, class_name):
    return api_data[module_name]["classes"][class_name]

# Returns whether a type is external, searches by class_name instead of class_name
def is_stack_allocated_type(api_data, class_name):
    search_result = search_for_class_by_class_name(api_data, class_name)
    if search_result is None:
        raise Exception("type not found " + class_name)
    c = get_class(api_data, search_result[0], search_result[1])
    return class_is_stack_allocated(c)

# Returns if the class is "pure virtual", i.e. if it is an
# object consisting of patches instead of being defined in the API
def class_is_virtual(api_data, className, api):
    for module_name in api_data.keys():
        module = api_data[module_name]["classes"]
        for class_name in module.keys():
            if class_name != className:
                continue
            c = module[class_name]
            if "use_patches" in c.keys() and api in c["use_patches"]:
                return True

    return False

# Generate the string for TAKING rust-api function arguments
def rust_bindings_fn_args(f, class_name, class_ptr_name, self_as_first_arg, api_data):
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
                arg_type_class_name = search_for_class_by_class_name(api_data, arg_type)
                if arg_type_class_name is None:
                    raise Exception("arg type " + arg_type + " not found!")
                arg_type_class = get_class(api_data, arg_type_class_name[0], arg_type_class_name[1])

                if start == "*const " or start == "*mut ":
                    fn_args += arg_name + ": " + start + prefix + arg_type_class_name[1] + ", "
                else:
                    fn_args += arg_name + ": " + start + arg_type_class_name[1] + ", "

        fn_args = fn_args[:-2]

    return fn_args

# Generate the string for CALLING rust-api function args
def rust_bindings_call_fn_args(f, class_name, class_ptr_name, self_as_first_arg, api_data, class_is_boxed_object):
    fn_args = ""
    if self_as_first_arg:
        self_val = list(f["fn_args"][0].values())[0]
        fn_args += "self, "

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
                arg_type_class = search_for_class_by_class_name(api_data, arg_type)
                if arg_type_class is None:
                    raise Exception("arg type " + arg_type + " not found!")
                arg_type_class = get_class(api_data, arg_type_class[0], arg_type_class[1])

                if start == "*const " or start == "*mut ":
                    fn_args += arg_name + ", "
                else:
                    if class_is_typedef(arg_type_class):
                        fn_args += start + arg_name + ", "
                    elif class_is_stack_allocated(arg_type_class):
                        fn_args += start + arg_name + ", " # .object
                    else:
                        fn_args += start + arg_name + ", "

        fn_args = fn_args[:-2]

    return fn_args


# ---------------------------------------------------------------------------------------------


# Generates the azul-dll/lib.rs file
#
# Returns an array:
#
#   [
#      dll.rs code,
#      all structs (in order of dependency),
#      all functions (in order of appearance),
#      C forward_declarations,
#   ]
#
def generate_rust_dll(api_data):

    version = list(api_data.keys())[-1]
    code = ""
    code += "//! WARNING: autogenerated code for azul api version " + str(version) + "\r\n"
    code += "\r\n"
    code += "#![cfg_attr(feature =\"cdylib\", crate_type = \"cdylib\")]\r\n"
    code += "#![cfg_attr(feature =\"staticlib\", crate_type = \"staticlib\")]\r\n"
    code += "#![cfg_attr(feature =\"rlib\", crate_type = \"rlib\")]\r\n"
    code += "#![deny(improper_ctypes_definitions)]\r\n"
    code += "\r\n"
    code += "pub mod widgets;\r\n"
    code += "#[cfg(feature = \"python-extension\")]\r\n"
    code += "pub mod python;\r\n"
    code += "\r\n"

    myapi_data = api_data[version]

    structs_map = OrderedDict({})
    rust_functions_map = OrderedDict({})

    code += read_file(root_folder + "/api/_patches/azul-dll/header.rs")
    code += "\r\n"

    for module_name in myapi_data.keys():
        module = myapi_data[module_name]["classes"]

        for class_name in module.keys():
            c = module[class_name]

            code += "\r\n"

            class_is_boxed_object = not(class_is_stack_allocated(c))
            class_is_const = "const" in c.keys()
            class_can_be_cloned = True
            if "clone" in c.keys():
                class_can_be_cloned = c["clone"]

            struct_derive = []
            if "derive" in c.keys():
                struct_derive = c["derive"]

            class_can_derive_debug = "derive" in c.keys() and "Debug" in c["derive"]
            class_can_be_copied = "derive" in c.keys() and "Copy" in c["derive"]
            class_has_partialeq = "derive" in c.keys() and "PartialEq" in c["derive"]
            class_has_eq = "derive" in c.keys()and "Eq" in c["derive"]
            class_has_partialord = "derive" in c.keys()and "PartialOrd" in c["derive"]
            class_has_ord = "derive" in c.keys() and "Ord" in c["derive"]
            class_can_be_hashed = "derive" in c.keys() and "Hash" in c["derive"]

            class_has_custom_destructor = "custom_destructor" in c.keys() and c["custom_destructor"]
            class_is_callback_typedef = "callback_typedef" in c.keys() and (len(c["callback_typedef"].keys()) > 0)
            is_boxed_object = "is_boxed_object" in c.keys() and c["is_boxed_object"]
            treat_external_as_ptr = "external" in c.keys() and is_boxed_object

            # Small structs and enums are stack-allocated in order to save on indirection
            # They don't have destructors, since they
            c_is_stack_allocated = not(class_is_boxed_object)

            class_ptr_name = prefix + class_name

            if class_is_callback_typedef:
                code += "pub type " + class_ptr_name + " = " + generate_rust_callback_fn_type(myapi_data, c["callback_typedef"]) + ";"
                structs_map[class_ptr_name] = { "callback_typedef": c["callback_typedef"] }
                continue

            struct_doc = ""
            if "doc" in c.keys():
                struct_doc = c["doc"]
            else:
                if c_is_stack_allocated:
                    struct_doc = "Re-export of rust-allocated (stack based) `" + class_name + "` struct"
                else:
                    struct_doc = "Pointer to rust-allocated `Box<" + class_name + ">` struct"

            code += "/// " + struct_doc  + "\r\n"

            if "external" in c.keys():
                external_path = c["external"]
                if class_is_const:
                    code += "pub static " + class_ptr_name + ": " + prefix + c["const"] + " = " + external_path + ";\r\n"
                elif class_is_boxed_object:
                    structs_map[class_ptr_name] = {"external": external_path, "clone": class_can_be_cloned, "is_boxed_object": is_boxed_object, "custom_destructor": class_has_custom_destructor, "derive": struct_derive, "doc": struct_doc, "struct": [{"ptr": {"type": "*mut c_void" }}]}
                    if treat_external_as_ptr:
                        code += "pub type " + class_ptr_name + "TT = " + external_path + ";\r\n"
                        code += "pub use " + class_ptr_name + "TT as " + class_ptr_name + ";\r\n"
                    else:
                        code += "#[repr(C)] pub struct " + class_ptr_name + " { pub ptr: *mut c_void }\r\n"
                else:
                    if "struct_fields" in c.keys():
                        structs_map[class_ptr_name] = {"external": external_path, "clone": class_can_be_cloned, "is_boxed_object": is_boxed_object, "custom_destructor": class_has_custom_destructor, "derive": struct_derive, "doc": struct_doc, "struct": c["struct_fields"]}
                    elif "enum_fields" in c.keys():
                        structs_map[class_ptr_name] = {"external": external_path, "clone": class_can_be_cloned, "is_boxed_object": is_boxed_object, "custom_destructor": class_has_custom_destructor, "derive": struct_derive, "doc": struct_doc, "enum": c["enum_fields"]}

                    code += "pub type " + class_ptr_name + "TT = " + external_path + ";\r\n"
                    code += "pub use " + class_ptr_name + "TT as " + class_ptr_name + ";\r\n"
            else:
                raise Exception("structs without 'external' key are not allowed! " + class_name)

            if "constructors" in c.keys():
                for fn_name in c["constructors"]:

                    const = c["constructors"][fn_name]

                    fn_body = ""

                    if c_is_stack_allocated:
                        fn_body += const["fn_body"]
                    else:
                        fn_body += "let object: " + class_name + " = " + const["fn_body"] + "; " # note: security check, that the returned object is of the correct type
                        fn_body += "let ptr = Box::into_raw(Box::new(object)) as *mut c_void; "
                        fn_body += class_ptr_name + " { ptr }"

                    if "doc" in const.keys():
                        code += "/// " + const["doc"] + "\r\n"
                    else:
                        code += "/// Creates a new `" + class_name + "` instance whose memory is owned by the rust allocator\r\n"
                        code += "/// Equivalent to the Rust `" + class_name  + "::" + fn_name + "()` constructor.\r\n"

                    returns = class_ptr_name
                    if "returns" in const.keys():
                        return_type = const["returns"]["type"]
                        analyzed_return_type = analyze_type(return_type)
                        if is_primitive_arg(analyzed_return_type[1]):
                            returns = return_type
                        else:
                            return_type_class = search_for_class_by_class_name(myapi_data, analyzed_return_type[1])
                            if return_type_class is None:
                                print("rust-dll: (line 549): no return_type_class found for " + return_type)

                            returns = analyzed_return_type[0] + prefix + return_type_class[1] + analyzed_return_type[2] # no postfix


                    fn_args = fn_args_c_api(const, class_name, class_ptr_name, False, myapi_data)

                    rust_functions_map[str(class_ptr_name + "_" + snake_case_to_lower_camel(fn_name))] = [fn_args, returns];
                    code += "#[no_mangle] pub extern \"C\" fn " + class_ptr_name + "_" + snake_case_to_lower_camel(fn_name) + "(" + fn_args + ") -> " + returns + " { "
                    code += fn_body
                    code += " }\r\n"

            if "functions" in c.keys():
                for fn_name in c["functions"]:

                    f = c["functions"][fn_name]

                    fn_body = f["fn_body"]

                    if "doc" in f.keys():
                        code += "/// " + f["doc"] + "\r\n"
                    else:
                        code += "/// Equivalent to the Rust `" + class_name  + "::" + fn_name + "()` function.\r\n"

                    fn_args = fn_args_c_api(f, class_name, class_ptr_name, True, myapi_data)

                    returns = ""
                    if "returns" in f.keys():
                        return_type = f["returns"]["type"]
                        analyzed_return_type = analyze_type(return_type)
                        if is_primitive_arg(analyzed_return_type[1]):
                            returns = return_type
                        else:
                            return_type_class = search_for_class_by_class_name(myapi_data, analyzed_return_type[1])
                            if return_type_class is None:
                                print("rust-dll: (line 549): no return_type_class found for " + return_type)

                            returns = analyzed_return_type[0] + prefix + return_type_class[1] + analyzed_return_type[2] # no postfix

                    rust_functions_map[str(class_ptr_name + "_" + snake_case_to_lower_camel(fn_name))] = [fn_args, returns];
                    return_arrow = "" if returns == "" else " -> "
                    code += "#[no_mangle] pub extern \"C\" fn " + class_ptr_name + "_" + snake_case_to_lower_camel(fn_name) + "(" + fn_args + ")" + return_arrow + returns + " { "
                    code += fn_body
                    code += " }\r\n"

            if c_is_stack_allocated:
                if class_can_be_copied:
                    # intentionally empty, no destructor necessary
                    pass
                elif class_has_custom_destructor or treat_external_as_ptr:
                    # az_item_delete()
                    code += "/// Destructor: Takes ownership of the `" + class_name + "` pointer and deletes it.\r\n"
                    rust_functions_map[str(class_ptr_name + "_delete")] = ["object: &mut " + class_ptr_name, ""];
                    code += "#[no_mangle] pub extern \"C\" fn " + class_ptr_name + "_delete(object: &mut " + class_ptr_name + ") { "
                    code += " unsafe { core::ptr::drop_in_place(object); } "
                    code += "}\r\n"

                if treat_external_as_ptr and class_can_be_cloned:
                    # az_item_deepCopy()
                    code += "/// Clones the object\r\n"
                    rust_functions_map[str(class_ptr_name + "_deepCopy")] = ["object: &" + class_ptr_name, class_ptr_name];
                    code += "#[no_mangle] pub extern \"C\" fn " + class_ptr_name + "_deepCopy(object: &" + class_ptr_name + ") -> " + class_ptr_name + " { "
                    code += "object.clone()"
                    code += " }\r\n"
            else:
                raise Exception("type " + class_name + "is not stack allocated!")

    sort_structs_result = sort_structs_map(myapi_data, structs_map)
    structs_map = sort_structs_result[0]
    forward_delcarations = sort_structs_result[1]

    code += "\r\n\r\n"
    code += generate_size_test(myapi_data, structs_map)

    return [code, structs_map, rust_functions_map, forward_delcarations]

# In order to statically link without code changes,
# all crate-internal types have to be listed as:
#
# use azul_impl::blah::Foo as AzFoo;
# use azul_impl::baz::Baz as AzBaz;
#
# This function generates a list of all these imports
def generate_list_of_struct_imports(structs_map):
    import_str = ""
    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]
        if "external" in struct.keys():
            external_ref = struct["external"]
            import_str += "use " + external_ref + " as " + struct_name + ";\r\n"
    return import_str

# Returns a sorted structs map where the structs are sorted
# so that all structs that a class depends on as fields appear
# before the class itself
#
# This is important because then we don't need forward declarations
# when generating C and C++ code (plus it also makes the size-tests
# easier to debug)
def sort_structs_map(api_data, structs_map):

    # From Python 3.6 onwards, the standard dict type maintains insertion order by default.
    sorted_class_map = OrderedDict([])

    # when encountering the class "DomVec", you must forward-declare the class "Dom",
    # because the type is recursive
    extra_forward_delcarations = {
        "AzDomVec": {"type": "struct", "name": "AzDom"},
        "AzMenuItemVec": {"type": "struct", "name": "AzMenuItem"},
        "AzXmlNodeVec": {"type": "struct", "name": "AzXmlNode"},
    }
    forward_delcarations = OrderedDict([
        ("AzDomVec", "Dom"),
        ("AzXmlNodeVec", "XmlNode"),
        ("AzMenuItemVec", "MenuItem")
    ])

    classes_not_found = OrderedDict([])

    # first, insert all types that only have primitive types as fields
    for class_name in structs_map.keys():
        clazz = structs_map[class_name]
        should_insert_struct = True

        found_c_is_callback_typedef = "callback_typedef" in clazz.keys() and (len(clazz["callback_typedef"].keys()) > 0)
        found_c_is_boxed_object = "is_boxed_object" in clazz.keys() and clazz["is_boxed_object"]
        class_in_forward_decl = class_name in forward_delcarations.keys()

        if found_c_is_callback_typedef:
            pass
        elif "struct" in clazz.keys():
            struct = clazz["struct"]
            for field in struct:
                field_name = list(field.keys())[0]
                field_type = list(field.values())[0]
                if not "type" in field_type:
                    raise Exception("missing type field in " + class_name + " " + field_name)
                field_type = analyze_type(field_type["type"])[1]
                if not(is_primitive_arg(field_type)):
                    found_c = search_for_class_by_class_name(api_data, field_type)
                    if found_c is None:
                        print("struct " + field_type + " not found")
                    field_is_fn_ptr = class_is_typedef(get_class(api_data, found_c[0], found_c[1]))
                    if not(class_in_forward_decl and field_type == forward_delcarations[class_name]) and not(field_is_fn_ptr):
                        should_insert_struct = False
        elif "enum" in clazz.keys():
            enum = clazz["enum"]
            for variant in enum:
                variant_name = list(variant.keys())[0]
                variant_type = list(variant.values())[0]
                if "type" in variant_type.keys():
                    variant_type = analyze_type(variant_type["type"])[1]
                    if not(is_primitive_arg(variant_type)):
                        found_c = search_for_class_by_class_name(api_data, variant_type)
                        if found_c is None:
                            print("sort structs map: " + class_name + " variant " + variant_type + " not found")
                        field_is_fn_ptr = class_is_typedef(get_class(api_data, found_c[0], found_c[1]))
                        if not(class_in_forward_decl and variant_type == forward_delcarations[class_name]) and not(field_is_fn_ptr):
                            should_insert_struct = False
        else:
            raise Exception("sort_structs_map: not enum nor struct nor typedef" + class_name + "")

        if should_insert_struct:
            sorted_class_map[class_name] = clazz
        else:
            classes_not_found[class_name] = clazz

    # Now loop through every class that was not a primitive type
    # usually this should resolve in 9 - 10 iterations
    iteration_count = 0;
    while not(len(classes_not_found.keys()) == 0):
        # classes not found in this iteration
        current_classes_not_found = OrderedDict([])

        for class_name in classes_not_found.keys():
            clazz = classes_not_found[class_name]
            should_insert_struct = True
            found_c_is_callback_typedef = "callback_typedef" in clazz.keys() and (len(clazz["callback_typedef"].keys()) > 0)
            class_in_forward_decl = class_name in forward_delcarations.keys()
            if found_c_is_callback_typedef:
                pass
            elif "struct" in clazz.keys():
                struct = clazz["struct"]
                for field in struct:
                    field_name = list(field.keys())[0]
                    field_type = list(field.values())[0]
                    field_type = analyze_type(field_type["type"])[1]
                    if not(is_primitive_arg(field_type)):
                        found_c = search_for_class_by_class_name(api_data, field_type)
                        field_is_fn_ptr = class_is_typedef(get_class(api_data, found_c[0], found_c[1]))
                        if not(class_in_forward_decl and field_type == forward_delcarations[class_name]) and not(field_is_fn_ptr):
                            field_type = prefix + field_type
                            if not(field_type in sorted_class_map.keys()):
                                should_insert_struct = False
            elif "enum" in clazz.keys():
                enum = clazz["enum"]
                for variant in enum:
                    variant_name = list(variant.keys())[0]
                    variant_type = list(variant.values())[0]
                    if "type" in variant_type.keys():
                        variant_type = analyze_type(variant_type["type"])[1]
                        if not(is_primitive_arg(variant_type)):
                            found_c = search_for_class_by_class_name(api_data, variant_type)
                            field_is_fn_ptr = class_is_typedef(get_class(api_data, found_c[0], found_c[1]))
                            if not(class_in_forward_decl and variant_type == forward_delcarations[class_name]) and not(field_is_fn_ptr):
                                variant_type = prefix + variant_type
                                if not(variant_type in sorted_class_map.keys()):
                                    should_insert_struct = False
            else:
                raise Exception("sort_structs_map: not enum nor struct " + class_name + "")

            if should_insert_struct:
                sorted_class_map[class_name] = clazz
            else:
                current_classes_not_found[class_name] = clazz


        classes_not_found = current_classes_not_found
        iteration_count += 1

        # NOTE: if the iteration count is extremely high,
        # something is wrong with the script
        if iteration_count > 500:
            raise Exception("infinite recursion detected in sort_structs_map: " + str(len(current_classes_not_found.keys())) + " unresolved structs = " + str(current_classes_not_found.keys()) + "\r\n")

    return [sorted_class_map, forward_delcarations, extra_forward_delcarations]

# Generate the RUST code for the struct layout of the final API
# This function has to be called twice in order to ensure that the layout of the struct
# matches the layout in the binary
def generate_structs(api_data, structs_map, autoderive, indent = 4, private_pointers=True,no_derive=False,wrapper_postfix=""):

    indent_str = " " * indent

    code = ""

    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]

        opt_extra_derive = ""
        if "extra_derive" in struct.keys():
            opt_extra_derive = struct["extra_derive"] + "\r\n"

        if "doc" in struct.keys():
            code += indent_str + "/// " + struct["doc"] + "\r\n"
        else:
            code += indent_str + "/// `" + struct_name + "` struct\r\n"

        class_is_callback_typedef = "callback_typedef" in struct.keys() and (len(struct["callback_typedef"].keys()) > 0)
        class_can_be_copied = "derive" in struct.keys() and "Copy" in struct["derive"]
        class_has_custom_destructor = "custom_destructor" in struct.keys() and struct["custom_destructor"]
        class_can_be_cloned = True
        if "clone" in struct.keys():
            class_can_be_cloned = struct["clone"]

        is_boxed_object = "is_boxed_object" in struct.keys() and struct["is_boxed_object"]
        treat_external_as_ptr = "external" in struct.keys() and is_boxed_object

        if class_is_callback_typedef:
            fn_ptr = generate_rust_callback_fn_type(api_data, struct["callback_typedef"])
            code += indent_str + "pub type " + struct_name + " = " + fn_ptr + ";\r\n\r\n"
        elif "struct" in struct.keys():
            struct = struct["struct"]

            # for LayoutCallback and RefAny, etc. the #[derive(Debug)] has to be implemented manually

            opt_derive_debug = indent_str + "#[derive(Debug)]\r\n"
            opt_derive_clone = indent_str + "#[derive(Clone)]\r\n"
            opt_derive_copy = indent_str + "#[derive(Copy)]\r\n"
            opt_derive_other = indent_str + "#[derive(PartialEq, PartialOrd)]\r\n"

            if no_derive:
                opt_derive_debug = ""
                opt_derive_clone = ""
                opt_derive_copy = ""
                opt_derive_other = ""

            if not(class_can_be_copied):
                opt_derive_copy = ""

            if not(class_can_be_cloned) or (treat_external_as_ptr and class_can_be_cloned):
                opt_derive_clone = ""

            if class_has_custom_destructor or not(autoderive) or struct_name == "AzU8VecRef":
                opt_derive_copy = ""
                opt_derive_debug = ""
                opt_derive_clone = ""
                opt_derive_other = ""

            for field in struct:
                if "type" in list(field.values())[0]:
                    analyzed_arg_type = analyze_type(list(field.values())[0]["type"])
                    if not(is_primitive_arg(analyzed_arg_type[1])):
                        field_type_class_path = search_for_class_by_class_name(api_data, analyzed_arg_type[1])
                        if field_type_class_path is None:
                            print("no field_type_class_path found for " + str(analyzed_arg_type))
                        found_c = get_class(api_data, field_type_class_path[0], field_type_class_path[1])
                        found_c_is_callback_typedef = "callback_typedef" in found_c.keys() and found_c["callback_typedef"]
                        if found_c_is_callback_typedef:
                            opt_derive_debug = ""
                            opt_derive_other = ""

            repr = "#[repr(C)]\r\n"
            if "repr" in structs_map[struct_name].keys():
                repr = "#[repr(" + structs_map[struct_name]["repr"] + ")]\r\n"

            code += indent_str + repr + opt_derive_debug + opt_derive_clone + opt_derive_other + opt_derive_copy + opt_extra_derive + indent_str + "pub struct " + struct_name + " {\r\n"

            for field in struct:
                if type(field) is str:
                    print("Struct " + struct_name + " should have a dictionary as fields")
                field_name = list(field.keys())[0]
                field_type = list(field.values())[0]
                if "type" in field_type:
                    field_type = field_type["type"]
                    field_extra_derive = ""
                    if "extra_derive" in field[field_name].keys():
                        field_extra_derive = field[field_name]["extra_derive"] + "\r\n"
                    code += field_extra_derive
                    analyzed_arg_type = analyze_type(field_type)
                    if is_primitive_arg(analyzed_arg_type[1]):
                        if field_name == "ptr" and private_pointers:
                            code += indent_str + "    " + "pub(crate) "
                        else:
                            code += indent_str + "    " + "pub "
                        code += field_name + ": " + field_type + ",\r\n"
                    else:
                        field_type_class_path = search_for_class_by_class_name(api_data, analyzed_arg_type[1])
                        if field_type_class_path is None:
                            print("no field_type_class_path found for " + str(analyzed_arg_type))

                        found_c = get_class(api_data, field_type_class_path[0], field_type_class_path[1])
                        if field_name == "ptr":
                            code += indent_str + "    " + "pub(crate) "
                        else:
                            code += indent_str + "    " + "pub "
                        field_postfix = wrapper_postfix
                        prevent_wrapper_recursion = len(wrapper_postfix) != 0 and struct_name.endswith(wrapper_postfix)
                        found_c_is_enum = "enum_fields" in found_c.keys()
                        if (not(found_c_is_enum) or prevent_wrapper_recursion):
                            field_postfix = ""
                        code += field_name + ": " + analyzed_arg_type[0] + prefix + field_type_class_path[1] + field_postfix + analyzed_arg_type[2] + ",\r\n"
                else:
                    print("struct " + struct_name + " does not have a type on field " + field_name)
                    raise Exception("error")
            code += indent_str + "}\r\n\r\n"
        elif "enum" in struct.keys():
            enum = struct["enum"]
            repr = "#[repr(C)]\r\n"

            for variant in enum:
                variant_name = list(variant.keys())[0]
                variant = list(variant.values())[0]
                if "type" in variant.keys():
                    repr = "#[repr(C, u8)]\r\n"

            if "repr" in structs_map[struct_name].keys():
                repr = "#[repr(" + structs_map[struct_name]["repr"] + ")]\r\n"

            # don't derive(Debug) for enums with function pointers in their variants
            opt_derive_debug = indent_str + "#[derive(Debug)]\r\n"
            opt_derive_clone = indent_str + "#[derive(Clone)]\r\n"
            opt_derive_copy = indent_str + "#[derive(Copy)]\r\n"
            opt_derive_other = indent_str + "#[derive(PartialEq, PartialOrd)]\r\n"

            if no_derive:
                opt_derive_debug = ""
                opt_derive_clone = ""
                opt_derive_copy = ""
                opt_derive_other = ""

            if not(class_can_be_copied):
                opt_derive_copy = ""

            if not(class_can_be_cloned) or (treat_external_as_ptr and class_can_be_cloned):
                opt_derive_clone = ""

            if class_has_custom_destructor or not(autoderive):
                opt_derive_copy = ""
                opt_derive_debug = ""
                opt_derive_clone = ""
                opt_derive_other = ""

            for variant in enum:
                variant = list(variant.values())[0]
                if "type" in variant.keys():
                    variant_type = variant["type"]
                    analyzed_arg_type = analyze_type(variant_type)
                    if not(is_primitive_arg(analyzed_arg_type[1])):
                        field_type_class_path = search_for_class_by_class_name(api_data, analyzed_arg_type[1])
                        if field_type_class_path is None:
                            print("no field_type_class_path found for " + str(analyzed_arg_type))
                        found_c = get_class(api_data, field_type_class_path[0], field_type_class_path[1])
                        found_c_is_callback_typedef = "callback_typedef" in found_c.keys() and found_c["callback_typedef"]
                        if found_c_is_callback_typedef:
                            opt_derive_debug = ""
                            opt_derive_other = ""

            code += indent_str + repr + opt_derive_debug + opt_derive_clone + opt_derive_other + opt_derive_copy + opt_extra_derive + indent_str + "pub enum " + struct_name + " {\r\n"
            for variant in enum:
                variant_name = list(variant.keys())[0]
                variant = list(variant.values())[0]
                if "type" in variant.keys():
                    variant_type = variant["type"]
                    if is_primitive_arg(variant_type):
                        code += indent_str + "    " + variant_name + "(" + variant_type + "),\r\n"
                    else:
                        analyzed_arg_type = analyze_type(variant_type)
                        field_type_class_path = search_for_class_by_class_name(api_data, analyzed_arg_type[1])
                        if field_type_class_path is None:
                            print("variant_type not found: " + variant_type + " in " + struct_name)
                        found_c = get_class(api_data, field_type_class_path[0], field_type_class_path[1])
                        found_c_is_enum = "enum" in found_c.keys()
                        variant_postfix = wrapper_postfix
                        if not(found_c_is_enum):
                            variant_postfix = ""
                        code += indent_str + "    "  + variant_name + "(" + analyzed_arg_type[0] + prefix + field_type_class_path[1] + variant_postfix + analyzed_arg_type[2] + "),\r\n"
                else:
                    code += indent_str + "    "  + variant_name + ",\r\n"
            code += indent_str + "}\r\n\r\n"

    return code

# returns the RUST DLL binding code
def generate_rust_dll_bindings(api_data, structs_map, functions_map):

    code = ""

    code += read_file(root_folder + "/api/_patches/azul.rs/dll.rs")

    code += "    #[cfg(not(feature = \"link_static\"))]\r\n"
    code += "    mod dynamic_link {\r\n"
    code += "    use core::ffi::c_void;\r\n\r\n"

    code += generate_structs(api_data, structs_map, True)

    code += "    #[cfg_attr(target_os = \"windows\", link(name=\"azul.dll\"))] // https://github.com/rust-lang/cargo/issues/9082\r\n"
    code += "    #[cfg_attr(not(target_os = \"windows\"), link(name=\"azul\"))] // https://github.com/rust-lang/cargo/issues/9082\r\n"
    code += "    extern \"C\" {\r\n"

    for fn_name in functions_map.keys():
        fn_type = functions_map[fn_name]
        fn_args = fn_type[0]
        fn_return = fn_type[1]
        return_arrow = "" if fn_return == "" else " -> "
        code += "        pub(crate) fn " + fn_name + "(" + strip_fn_arg_types(fn_args) + ")" + return_arrow + fn_return + ";\r\n"

    code += "    }\r\n\r\n"

    code += "    }\r\n\r\n"
    code += "    #[cfg(not(feature = \"link_static\"))]\r\n"
    code += "    pub use self::dynamic_link::*;\r\n"


    code += "\r\n"
    code += "\r\n"

    code += "    #[cfg(feature = \"link_static\")]\r\n"
    code += "    mod static_link {\r\n"
    code += "       #[cfg(feature = \"link_static\")]\r\n"
    code += "        extern crate azul; // the azul_dll package, confusingly it has to also be named \"azul\"\r\n"
    code += "       #[cfg(feature = \"link_static\")]\r\n"
    code += "        use azul::*;\r\n"
    code += "    }\r\n\r\n"
    code += "    #[cfg(feature = \"link_static\")]\r\n"
    code += "    pub use self::static_link::*;\r\n"

    return code

# Generates the azul-dll/python.rs file (pyo3 bindings)
def generate_python_api(api_data, structs_map, functions_map):

    version = list(api_data.keys())[-1]

    pyo3_code = ""
    pyo3_code += "#![allow(non_snake_case)]\r\n"
    pyo3_code += "\r\n"
    pyo3_code += read_file(root_folder + "/api/_patches/azul-dll/header.rs")
    pyo3_code += "\r\n"
    pyo3_code += "use core::mem;\r\n"
    pyo3_code += "use pyo3::prelude::*;\r\n"
    pyo3_code += "use pyo3::PyObjectProtocol;\r\n"
    pyo3_code += "use pyo3::types::*;\r\n"
    pyo3_code += "use pyo3::exceptions::PyException;\r\n"

    # This should be done properly, but right now it works

    pyo3_code += "type GLuint = u32; type AzGLuint = GLuint;\r\n"
    pyo3_code += "type GLint = i32; type AzGLint = GLint;\r\n"
    pyo3_code += "type GLint64 = i64; type AzGLint64 = GLint64;\r\n"
    pyo3_code += "type GLuint64 = u64; type AzGLuint64 = GLuint64;\r\n"
    pyo3_code += "type GLenum = u32; type AzGLenum = GLenum;\r\n"
    pyo3_code += "type GLintptr = isize; type AzGLintptr = GLintptr;\r\n"
    pyo3_code += "type GLboolean = u8; type AzGLboolean = GLboolean;\r\n"
    pyo3_code += "type GLsizeiptr = isize; type AzGLsizeiptr = GLsizeiptr;\r\n"
    pyo3_code += "type GLvoid = c_void; type AzGLvoid = GLvoid;\r\n"
    pyo3_code += "type GLbitfield = u32; type AzGLbitfield = GLbitfield;\r\n"
    pyo3_code += "type GLsizei = i32; type AzGLsizei = GLsizei;\r\n"
    pyo3_code += "type GLclampf = f32; type AzGLclampf = GLclampf;\r\n"
    pyo3_code += "type GLfloat = f32; type AzGLfloat = GLfloat;\r\n"
    pyo3_code += "type AzF32 = f32;\r\n"
    pyo3_code += "type AzU16 = u16;\r\n"
    pyo3_code += "type AzU32 = u32;\r\n"
    pyo3_code += "type AzScanCode = u32;\r\n"

    pyo3_code += "\r\n"
    pyo3_code += "\r\n"
    pyo3_code += read_file(root_folder + "/api/_patches/python/api.rs")
    pyo3_code += "\r\n"

    # Functions that have to be implemented manually
    manual_implementations = [

        ("app", "App", "new"), # ok: replaced
        ("window", "WindowCreateOptions", "new"), # ok: replaced
        ("window", "WindowState", "new"), # ok: replaced

        ("dom", "Dom", "iframe"), # ok: replaced
        ("dom", "Dom", "set_dataset"), # ok: replaced
        ("dom", "Dom", "with_dataset"), # ok: replaced
        ("dom", "Dom", "add_callback"), # ok: replaced
        ("dom", "Dom", "with_callback"), # ok: replaced

        ("dom", "NodeData", "add_callback"), # ok: replaced
        ("dom", "NodeData", "with_callback"), # ok: replaced
        ("dom", "NodeData", "iframe"), # ok: replaced
        ("dom", "NodeData", "set_dataset"), # ok: replaced
        ("dom", "NodeData", "with_dataset"), # ok: replaced

        ("widgets", "Button", "set_on_click"), # ok: replaced
        ("widgets", "Button", "with_on_click"), # ok: replaced

        # menu, MenuItem, with_callback
        # menu, StringMenuItem, with_callback

        ("task", "Timer", "new"),
        ("callbacks", "CallbackInfo", "start_thread"),
        ("image", "ImageRef", "callback"),

        ("widgets", "CheckBox", "set_on_toggle"),
        ("widgets", "CheckBox", "with_on_toggle"),
        ("widgets", "ColorInput", "set_on_value_change"),
        ("widgets", "ColorInput", "with_on_value_change"),
        ("widgets", "TextInput", "set_on_text_input"),
        ("widgets", "TextInput", "with_on_text_input"),
        ("widgets", "TextInput", "set_on_virtual_key_down"),
        ("widgets", "TextInput", "with_on_virtual_key_down"),
        ("widgets", "TextInput", "set_on_focus_lost"),
        ("widgets", "TextInput", "with_on_focus_lost"),

        # unnecessary due to Python string wrappers
        ("str", "String", "as_refstr"),
        ("str", "String", "copy_from_bytes"),
        ("vec", "U8Vec", "copy_from_bytes"),
        ("callbacks", "RefAny", "new_c"), # unnecessary, use PyAny
        ("vec", "TesselatedSvgNodeVec", "as_ref_vec"),
        ("vec", "U8Vec", "as_ref_vec"),
    ]

    inject_impls = {
        ("app", "App"): read_file(root_folder + "/api/_patches/python/app.rs"),
        ("dom", "Dom"): read_file(root_folder + "/api/_patches/python/dom.rs"),
        ("dom", "NodeData"): read_file(root_folder + "/api/_patches/python/nodedata.rs"),
        ("widgets", "Button"): read_file(root_folder + "/api/_patches/python/button.rs"),
        ("callbacks", "LayoutCallback"): read_file(root_folder + "/api/_patches/python/layout_callback.rs"),
        ("window", "WindowCreateOptions"): read_file(root_folder + "/api/_patches/python/window_create_options.rs"),
        ("window", "WindowState"): read_file(root_folder + "/api/_patches/python/window_state.rs"),
    }

    staticmethods = [
        ("File", "open"),
        ("File", "create"),
        ("MsgBox", "ok"),
        ("MsgBox", "ok_cancel"),
        ("MsgBox", "yes_no"),
        ("FileDialog", "select_file"),
        ("FileDialog", "select_multiple_files"),
        ("FileDialog", "select_folder"),
        ("FileDialog", "save_file"),
        ("ImageRef", "invalid"),
        ("ImageRef", "raw_image"),
        ("ImageRef", "gl_texture"),
        ("ImageRef", "callback"),
        ("FontRef", "parse"),
        ("ColorPickerDialog", "open"),
        ("SystemClipboard", "new"),
        ("Css", "empty"),
        ("Css", "from_string"),
        ("WindowState", "default"),
    ]

    not_default_constructable = {
        "Gl": {},
        "ThreadSender": {},
        "ThreadReceiver": {},
        "Thread": {},
        "TesselatedSvgNodeVec": {},
        "U8Vec": {},
        "CallbackInfo": {},
        "IFrameCallbackInfo": {},
        "RenderImageCallbackInfo": {},
        "TimerCallbackInfo": {},
        "LayoutCallbackInfo": {},
        "RefAny": {},
        "RefCount": {},
        "IOSHandle": {},
        "MacOSHandle": {},
        "XlibHandle": {},
        "XcbHandle": {},
        "WaylandHandle": {},
        "WindowsHandle": {},
        "AndroidHandle": {},
        "WaylandTheme": {},
        "MarshaledLayoutCallbackInner": {},
        "LayoutCallbackInner": {},
        "Callback": {},
        "IFrameCallback": {},
        "RenderImageCallback": {},
        "TimerCallback": {},
        "WriteBackCallback": {},
        "ThreadCallback": {},
        "StyleBoxShadow": {},
        "CssPropertyCache": {},
        "GlVoidPtrConst": {},
        "GlVoidPtrMut": {},
        "U8VecRef": {},
        "U8VecRefMut": {},
        "F32VecRef": {},
        "I32VecRef": {},
        "GLuintVecRef": {},
        "GLenumVecRef": {},
        "GLintVecRefMut": {},
        "GLint64VecRefMut": {},
        "GLbooleanVecRefMut": {},
        "GLfloatVecRefMut": {},
        "RefstrVecRef": {},
        "Refstr": {},
        "GLsyncPtr": {},
        "TesselatedSvgNodeVecRef": {},
        "InstantPtr": {},
        "InstantPtrCloneFn": {},
        "InstantPtrDestructorFn": {},
        "CreateThreadFn": {},
        "GetSystemTimeFn": {},
        "CheckThreadFinishedFn": {},
        "LibrarySendThreadMsgFn": {},
        "LibraryReceiveThreadMsgFn": {},
        "ThreadRecvFn": {},
        "ThreadSendFn": {},
        "ThreadDestructorFn": {},
        "ThreadReceiverDestructorFn": {},
        "ThreadSenderDestructorFn": {},
        "FontMetrics": {},
        "ColorInputOnValueChangeCallback": {},
        "TextInputOnTextInputCallback": {},
        "TextInputOnVirtualKeyDownCallback": {},
        "TextInputOnFocusLostCallback": {},
        "NumberInputOnValueChangeCallback": {},
        "CheckBoxOnToggleCallback": {},
    }

    python_replacements = {
        "String": ("String", "pystring_to_azstring", "az_string_to_py_string"),
        "U8Vec": ("Vec<u8>", "pyvecu8_to_vecu8", "az_vecu8_to_py_vecu8"),
        "U16Vec": ("Vec<u16>", "pyvecu16_to_vecu16", "az_vecu8_to_py_vecu16"),
        "F32Vec": ("Vec<f32>", "pyvecf32_to_vecf32", "az_vecf32_to_py_vecf32"),
        "Refstr": ("&str", "pystring_to_refstr"),
        "U8VecRefMut": ("mut Vec<u8>", "pybytesrefmut_to_vecu8refmut"),
        "U8VecRef": ("Vec<u8>", "pybytesref_to_vecu8_ref"),
        "F32VecRef": ("Vec<f32>", "pylist_f32_to_rust"),
        "GLuintVecRef": ("Vec<u32>", "pylist_u32_to_rust"),
        "GLintVecRefMut": ("mut Vec<i32>", "pylist_i32_to_rust"),
        "GLint64VecRefMut": ("mut Vec<i64>", "pylist_i64_to_rust"),
        "GLbooleanVecRefMut": ("mut Vec<u8>", "pylist_bool_to_rust"),
        "GLfloatVecRefMut": ("mut Vec<f32>", "pylist_glfoat_to_rust"),
        "RefstrVecRef": ("Vec<&str>", "pylist_str_to_rust", "", "vec_string_to_vec_refstr"),
        "TesselatedSvgNodeVecRef": ("Vec<AzTesselatedSvgNode>", "pylist_tesselated_svg_node"),
    }

    new_struct_map = dict(structs_map)
    raw_pointer_structs = {}

    # pyo3 does not know how to translate enums
    # so we just create a "EnumWrapper" struct that
    # contains the internal type in rust-representation
    for struct_name in list(structs_map.keys()):
        struct = structs_map[struct_name]
        if "struct" in struct.keys():
            new_struct_map[struct_name]["extra_derive"] = "#[pyclass(name = \"" + struct_name[len(prefix):] + "\")]"
            field_index = 0
            for field in struct["struct"]:
                field_name = list(field.keys())[0]
                field_type = list(field.values())[0]["type"]
                if len(analyze_type(field_type)[0]) > 0:
                    raw_pointer_structs[struct_name] = {}
                elif (not(field_name == "cb")):
                    new_struct_map[struct_name]["struct"][field_index][field_name]["extra_derive"] = "    #[pyo3(get, set)]"
                field_index = field_index + 1
        elif "enum" in struct.keys():
            new_struct_map[struct_name + "EnumWrapper"] = {}
            new_struct_map[struct_name + "EnumWrapper"]["struct"] = []
            new_struct_map[struct_name + "EnumWrapper"]["struct"].append({})
            new_struct_map[struct_name + "EnumWrapper"]["struct"][0]["inner"] = {}
            new_struct_map[struct_name + "EnumWrapper"]["struct"][0]["inner"]["type"] = struct_name[len(prefix):]
            new_struct_map[struct_name + "EnumWrapper"]["struct"][0]["inner"]["doc"] = struct_name[len(prefix):] + " wrapper"
            new_struct_map[struct_name + "EnumWrapper"]["repr"] = "transparent"
            new_struct_map[struct_name + "EnumWrapper"]["extra_derive"] = "#[pyclass(name = \"" + struct_name[len(prefix):] + "\")]"

            for variant in struct["enum"]:
                variant_name = list(field.keys())[0]
                variant = list(variant.values())[0]
                if "type" in variant.keys():
                    variant_type = variant["type"]
                    if len(analyze_type(variant_type)[0]) > 0:
                        raw_pointer_structs[struct_name] = {}
        elif "callback_typedef" in struct.keys():
            pass

    pyo3_code += generate_structs(
        api_data[version],
        dict(new_struct_map),
        functions_map,
        indent=0,
        private_pointers=False,
        no_derive=True,
        wrapper_postfix="EnumWrapper"
    )

    pyo3_code += "\r\n"
    pyo3_code += "// Necessary because the Python interpreter may send structs across different threads"
    pyo3_code += "\r\n"
    for raw_pointer_struct in raw_pointer_structs.keys():
        pyo3_code += "unsafe impl Send for " + raw_pointer_struct + " { }\r\n"

    pyo3_code += "\r\n"

    pyo3_code += "\r\n"
    pyo3_code += "// Python objects must implement Clone at minimum"
    pyo3_code += "\r\n"
    for struct_name in list(structs_map.keys()):
        struct = structs_map[struct_name]
        clone_class = True
        if "clone" in struct.keys():
            clone_class = struct["clone"]
        if not(clone_class):
            continue

        if "struct" in struct.keys():
            pyo3_code += "impl Clone for " + struct_name + " { fn clone(&self) -> Self { let r: &" + struct["external"]+ " = unsafe { mem::transmute(self) }; unsafe { mem::transmute(r.clone()) } } }\r\n"
        elif "enum" in struct.keys():
            pyo3_code += "impl Clone for " + struct_name + "EnumWrapper { fn clone(&self) -> Self { let r: &" + struct["external"]+ " = unsafe { mem::transmute(self) }; unsafe { mem::transmute(r.clone()) } } }\r\n"

    pyo3_code += "\r\n"
    pyo3_code += "// Implement Drop for all objects with drop constructors"
    pyo3_code += "\r\n"
    for struct_name in list(structs_map.keys()):
        struct = structs_map[struct_name]
        class_has_custom_destructor = "custom_destructor" in struct.keys() and struct["custom_destructor"]
        is_boxed_object = "is_boxed_object" in struct.keys() and struct["is_boxed_object"]
        should_impl_drop = class_has_custom_destructor or is_boxed_object

        if should_impl_drop:
            if "struct" in struct.keys():
                pyo3_code += "impl Drop for " + struct_name + " { fn drop(&mut self) { crate::" + struct_name + "_delete(unsafe { mem::transmute(self) }); } }\r\n"
            elif "enum" in struct.keys():
                pyo3_code += "impl Drop for " + struct_name + "EnumWrapper { fn drop(&mut self) { crate::" + struct_name + "_delete(unsafe { mem::transmute(self) }); } }\r\n"

    pyo3_code += "\r\n"

    # List of types that are returned as errors, have to implement py03::Error
    errlist = []

    for module_name in api_data[version].keys():
        module = api_data[version][module_name]
        for class_name in module["classes"].keys():
            struct = module["classes"][class_name]

            constants = ""
            if "constants" in struct.keys():
                for constant in struct["constants"]:
                    constant_name = list(constant.keys())[0]
                    constant_type = constant[constant_name]["type"]
                    constant_value = constant[constant_name]["value"]
                    constants += "    #[classattr]\r\n    const " + constant_name + ": " + constant_type + " = " + constant_value + ";\r\n"
                constants += "\r\n"

            if "struct_fields" in struct.keys():

                pyo3_code += "\r\n"
                pyo3_code += "#[pymethods]\r\n"
                pyo3_code += "impl " + prefix + class_name + " {\r\n" + constants
                external = struct["external"]

                if "constructors" in struct.keys():
                    for constructor_name in struct["constructors"]:
                        if (module_name, class_name, constructor_name) in manual_implementations:
                            continue
                        constructor = struct["constructors"][constructor_name]
                        if not("fn_args" in constructor.keys()):
                            print("wrong format: constructor " + class_name + "::" + constructor_name)
                        fn_args = constructor["fn_args"]
                        break_outer_flag = False
                        for f in fn_args:
                            arg_name = list(f.keys())[0]
                            arg_type = f[arg_name]
                            analyzed = analyze_type(arg_type)
                            if len(analyzed[0]) != 0 or arg_type == "RefAny":
                                break_outer_flag = True
                                raise Exception("function " + class_name + " " + constructor_name + " cannot take RefAny as argument in Python API")
                                continue # no constructor can take pointers in the Python API
                        if break_outer_flag:
                            continue # break outer loop
                        py_args = format_py_args(python_replacements, fn_args, api_data[version], constructor=True)
                        return_type = None
                        return_type_match = ""
                        returns_option = None
                        returns_error = None
                        if "returns" in constructor.keys():
                            return_type_match = constructor["returns"]["type"]
                            r = format_py_return(python_replacements, constructor["returns"], api_data[version], errlist, constructor=True)
                            return_type = r[0]
                            returns_option = r[1]
                            returns_error = r[2]
                        return_type_str = prefix + class_name
                        if not(return_type is None):
                            return_type_str = return_type
                        if (constructor_name == "new" and not("returns" in constructor.keys())):
                            pyo3_code += "    #[new]\r\n"
                        else:
                            pyo3_code += "    #[staticmethod]\r\n"
                        pyo3_code += "    fn " + constructor_name + "(" + py_args + ") -> " + return_type_str + " {\r\n"
                        pyo3_code += "        " + format_py_body(python_replacements, module_name, class_name, constructor_name, fn_args, api_data[version], return_type_match, returns_option, returns_error, constructor=True) + "\r\n"
                        pyo3_code += "    }\r\n"

                # Generate constructors
                class_is_vec = class_name.endswith("Vec")
                while True:
                    if (not("constructors") in struct.keys() or len(struct["constructors"]) == 0) and not(class_name in not_default_constructable.keys()):
                        py_new_constructor = ""
                        py_func_args = ""

                        # do not generate a __new__() constructor for struct
                        # that have only "ptr + len" fields
                        # this is a special rule for AzTessellatedSvgNodeVecRef
                        maybe_is_ref_struct = len(struct["struct_fields"]) == 2
                        is_ref_struct = False

                        for field in struct["struct_fields"]:
                            field_name = list(field.keys())[0]
                            if field_name == "ptr":
                                if maybe_is_ref_struct:
                                    is_ref_struct = True
                                break # don't generate code
                            field_type = field[field_name]["type"]
                            analyzed_type = analyze_type(field_type)
                            if len(analyzed_type[0]) != 0:
                                break # don't generate code for structs with raw pointers
                            if is_primitive_arg(analyzed_type[1]):
                                py_func_args += field_name + ": " + field_type + ", "
                                py_new_constructor += "            " + field_name + ",\r\n"
                            else:
                                f_class = quick_get_class(api_data[version], analyzed_type[1])
                                if "enum_fields" in f_class.keys():
                                    py_func_args += field_name + ": " + prefix + field_type + "EnumWrapper, "
                                    py_new_constructor += "            " + field_name + ",\r\n"
                                elif "struct_fields" in f_class.keys():
                                    py_func_args += field_name + ": " + prefix + field_type + ", "
                                    py_new_constructor += "            " + field_name + ",\r\n"
                                else:
                                    break

                        if is_ref_struct: # only necessary for AzTessellatedSvgNodeVecRef
                            break

                        if not(len(py_func_args) == 0):
                            py_func_args = py_func_args[:-2] # strip final ", "

                        if class_is_vec:
                            vec_type = class_name[:-3]
                            vec_ty_excluded = ["ScanCode", "U16", "U32", "I32", "F32", "GLuint", "GLint"]
                            if not(vec_type in vec_ty_excluded):
                                vec_class = quick_get_class(api_data[version], vec_type)
                                if "enum_fields" in vec_class.keys():
                                    vec_type = vec_type + "EnumWrapper"
                            pyo3_code += "    /// Creates a new `" + vec_type + "Vec` from a Python array\r\n"
                            pyo3_code += "    #[new]\r\n"
                            pyo3_code += "    fn __new__(input: Vec<" + prefix + vec_type + ">) -> Self {\r\n"
                            pyo3_code += "        let m: " + external + " = " + external + "::from_vec(unsafe { mem::transmute(input) }); unsafe { mem::transmute(m) }\r\n"

                            pyo3_code += "    }\r\n"
                            pyo3_code += "    \r\n"
                            pyo3_code += "    /// Returns the " + vec_type + " as a Python array\r\n"
                            pyo3_code += "    fn array(&self) -> Vec<" + prefix + vec_type + "> {\r\n"
                            pyo3_code += "        let m: &" + external + " = unsafe { mem::transmute(self) }; unsafe { mem::transmute(m.clone().into_library_owned_vec()) }\r\n"
                            pyo3_code += "    }\r\n"
                        else:
                            pyo3_code += "    #[new]\r\n"
                            pyo3_code += "    fn __new__(" + py_func_args + ") -> Self {\r\n"
                            pyo3_code += "        Self {\r\n"
                            pyo3_code += py_new_constructor
                            pyo3_code += "        }\r\n"
                            pyo3_code += "    }\r\n"
                        pyo3_code += "\r\n"

                    break

                if "functions" in struct.keys():
                    for function_name in struct["functions"]:
                        function = struct["functions"][function_name]
                        if (module_name, class_name, function_name) in manual_implementations:
                            continue
                        if not("fn_args" in function.keys()):
                            print("wrong format: " + class_name + "::" + function_name)
                        fn_args = function["fn_args"]
                        break_outer_flag = False
                        for f in fn_args:
                            arg_name = list(f.keys())[0]
                            arg_type = f[arg_name]
                            analyzed = analyze_type(arg_type)
                            if len(analyzed[0]) != 0 or arg_type == "RefAny":
                                raise Exception("function " + class_name + " " + function_name + " cannot take RefAny as argument in Python API")
                                break_outer_flag = True
                                continue # no constructor can take pointers in the Python API
                        if break_outer_flag:
                            continue # break outer loop
                        self_arg = "&self" # TODO
                        self_needs_clone = False
                        if fn_args[0]["self"] == "refmut":
                            self_arg = "&mut self"
                        elif fn_args[0]["self"] == "value":
                            self_arg = "self" # TODO: possible?

                        return_type = None # TODO
                        return_type_match = ""
                        returns_option = None
                        returns_error = None
                        if "returns" in function.keys():
                            return_type_match = function["returns"]["type"]
                            r = format_py_return(python_replacements, function["returns"], api_data[version], errlist, constructor=False)
                            return_type = r[0]
                            returns_option = r[1]
                            returns_error = r[2]
                        return_type_str = "()"
                        if not(return_type is None):
                            return_type_str = return_type
                        pyo3_code += "    fn " + function_name + "(" + self_arg + format_py_args(python_replacements, fn_args, api_data[version], constructor=False) + ") -> " + return_type_str + " {\r\n"
                        pyo3_code += "        " + format_py_body(python_replacements, module_name, class_name, function_name, fn_args, api_data[version], return_type_match, returns_option, returns_error, constructor=False) + "\r\n"
                        pyo3_code += "    }\r\n"


                if tuple((module_name, class_name)) in inject_impls:
                    pyo3_code += inject_impls[tuple((module_name, class_name))]
                pyo3_code += "}\r\n"

                pyo3_code += "\r\n"
                pyo3_code += "#[pyproto]\r\n"
                pyo3_code += "impl PyObjectProtocol for " + prefix + class_name + " {\r\n"
                pyo3_code += "    fn __str__(&self) -> Result<String, PyErr> { \r\n"
                pyo3_code += "        let m: &" + external + " = unsafe { mem::transmute(self) }; Ok(format!(\"{:#?}\", m))\r\n"
                pyo3_code += "    }\r\n"
                pyo3_code += "    fn __repr__(&self) -> Result<String, PyErr> { \r\n"
                pyo3_code += "        let m: &" + external + " = unsafe { mem::transmute(self) }; Ok(format!(\"{:#?}\", m))\r\n"
                pyo3_code += "    }\r\n"
                pyo3_code += "}\r\n"

            elif "enum_fields" in struct.keys():
                pyo3_code += "\r\n"
                pyo3_code += "#[pymethods]\r\n"
                pyo3_code += "impl " + prefix + class_name + "EnumWrapper {\r\n" + constants

                enum_is_union = False

                # generate a Enum::Blah(...) constructor function
                for enum_name in struct["enum_fields"]:
                    enum_arg_type = ""
                    enum_type = ""
                    needs_transmute = False
                    variant_name = list(enum_name.keys())[0]
                    variant = enum_name[variant_name]
                    if "type" in variant.keys():
                        enum_is_union = True
                        analyzed_type = analyze_type(variant["type"])
                        if (len(analyzed_type[0]) > 0):
                            continue
                        enum_arg_type = "v: " + analyzed_type[1]
                        if not(is_primitive_arg(analyzed_type[1])):
                            e_class = quick_get_class(api_data[version], analyzed_type[1])
                            if "enum_fields" in e_class.keys():
                                enum_arg_type = "v: " + prefix + analyzed_type[1] + "EnumWrapper"
                                needs_transmute = True
                            elif "struct_fields" in e_class.keys():
                                enum_arg_type = "v: " + prefix + analyzed_type[1]
                            else:
                                continue # cannot construct callbacks as function arguments
                        else:
                             enum_arg_type = "v: " + analyzed_type[1]
                        enum_type = enum_arg_type
                    if not(len(enum_type) == 0):
                        pyo3_code += "    #[staticmethod]\r\n    fn " + variant_name + "(" + enum_arg_type + ") -> "
                    else:
                        pyo3_code += "    #[classattr]\r\n    fn " + variant_name + "(" + enum_arg_type + ") -> "
                    pyo3_code += prefix + class_name + "EnumWrapper { "
                    pyo3_code += prefix + class_name + "EnumWrapper { inner: " + prefix + class_name + "::" + variant_name
                    if not(len(enum_type) == 0):
                        if needs_transmute:
                            pyo3_code += "(unsafe { mem::transmute(v) })"
                        else:
                            pyo3_code += "(v)"
                    pyo3_code += " } }\r\n"

                if tuple((module_name, class_name)) in inject_impls:
                    pyo3_code += inject_impls[tuple((module_name, class_name))]

                # Generate a "match" function that returns the enum tag as a string + the object as a tuple
                if enum_is_union:
                    pyo3_code += "\r\n"
                    pyo3_code += "    fn r#match(&self) -> PyResult<Vec<PyObject>> {\r\n"
                    pyo3_code += "        use crate::python::" + prefix + class_name + ";\r\n"
                    pyo3_code += "        use pyo3::conversion::IntoPy;\r\n"
                    pyo3_code += "        let gil = Python::acquire_gil();\r\n"
                    pyo3_code += "        let py = gil.python();\r\n"
                    pyo3_code += "        match &self.inner {\r\n"
                    for enum_name in struct["enum_fields"]:
                        variant_name = list(enum_name.keys())[0]
                        variant = enum_name[variant_name]
                        opt_variant_type_match = ""
                        if "type" in variant.keys():
                            opt_variant_type_match = "(v)"
                        opt_variant_value = "()"
                        if "type" in variant.keys():
                            if (variant["type"] == "*const c_void") or (variant["type"] == "*mut c_void"):
                                opt_variant_value = "()"
                            else:
                                analyzed_type = analyze_type(variant["type"])
                                if not(is_primitive_arg(analyzed_type[1])):
                                    e_class = quick_get_class(api_data[version], analyzed_type[1])
                                    if "enum_fields" in e_class.keys():
                                        opt_variant_value = "{ let m: &" + prefix + analyzed_type[1] + "EnumWrapper = unsafe { mem::transmute(v) }; m.clone() }"
                                    elif class_is_typedef(e_class):
                                        opt_variant_value = "()" # can't destructure function pointer
                                    elif variant["type"] == "[PixelValue;2]":
                                        opt_variant_value = "v.to_vec()"
                                    else:
                                        opt_variant_value = "v.clone()"
                                else:
                                        opt_variant_value = "v"

                        pyo3_code += "            " + prefix + class_name + "::" + variant_name + opt_variant_type_match + " => Ok(vec![\"" + variant_name + "\".into_py(py), " + opt_variant_value + ".into_py(py)]),\r\n"
                    pyo3_code += "        }\r\n"
                    pyo3_code += "    }\r\n"

                pyo3_code += "}\r\n"

                external = struct["external"]
                pyo3_code += "\r\n"
                pyo3_code += "#[pyproto]\r\n"
                pyo3_code += "impl PyObjectProtocol for " + prefix + class_name + "EnumWrapper {\r\n"
                pyo3_code += "    fn __str__(&self) -> Result<String, PyErr> { \r\n"
                pyo3_code += "        let m: &" + external + " = unsafe { mem::transmute(&self.inner) }; Ok(format!(\"{:#?}\", m))\r\n"
                pyo3_code += "    }\r\n"
                pyo3_code += "    fn __repr__(&self) -> Result<String, PyErr> { \r\n"
                pyo3_code += "        let m: &" + external + " = unsafe { mem::transmute(&self.inner) }; Ok(format!(\"{:#?}\", m))\r\n"
                pyo3_code += "    }\r\n"

                # simple C-like enum: implement comparison operators
                if not(enum_is_union):
                    pyo3_code += "    fn __richcmp__(&self, other: " + prefix + class_name + "EnumWrapper, op: pyo3::class::basic::CompareOp) -> PyResult<bool> {\r\n"
                    pyo3_code += "        match op {\r\n"
                    pyo3_code += "            pyo3::class::basic::CompareOp::Lt => { Ok((self.clone().inner as usize) <  (other.clone().inner as usize)) }\r\n"
                    pyo3_code += "            pyo3::class::basic::CompareOp::Le => { Ok((self.clone().inner as usize) <= (other.clone().inner as usize)) }\r\n"
                    pyo3_code += "            pyo3::class::basic::CompareOp::Eq => { Ok((self.clone().inner as usize) == (other.clone().inner as usize)) }\r\n"
                    pyo3_code += "            pyo3::class::basic::CompareOp::Ne => { Ok((self.clone().inner as usize) != (other.clone().inner as usize)) }\r\n"
                    pyo3_code += "            pyo3::class::basic::CompareOp::Gt => { Ok((self.clone().inner as usize) >  (other.clone().inner as usize)) }\r\n"
                    pyo3_code += "            pyo3::class::basic::CompareOp::Ge => { Ok((self.clone().inner as usize) >= (other.clone().inner as usize)) }\r\n"
                    pyo3_code += "        }\r\n"
                    pyo3_code += "    }\r\n"

                pyo3_code += "}\r\n"

    errlist_dict = {}
    for err in errlist:
        errlist_dict[err] = {}

    pyo3_code += "\r\n"
    for err in errlist_dict.keys():
        external = structs_map[err]["external"]
        pyo3_code += "\r\n"
        pyo3_code += "impl core::convert::From<" + err + "> for PyErr {\r\n"
        pyo3_code += "    fn from(err: " + err + ") -> PyErr {\r\n"
        pyo3_code += "        let r: " + external + " = unsafe { mem::transmute(err) };\r\n"
        pyo3_code += "        PyException::new_err(format!(\"{}\", r))\r\n"
        pyo3_code += "    }\r\n"
        pyo3_code += "}\r\n"

    pyo3_code += "\r\n"
    pyo3_code += "#[pymodule]\r\n"
    pyo3_code += "fn azul(py: Python, m: &PyModule) -> PyResult<()> {\r\n"
    pyo3_code += "\r\n"
    pyo3_code += "    #[cfg(all(feature = \"use_pyo3_logger\", not(feature = \"use_fern_logger\")))] {\r\n"

    # Since we can't get access to the AppConfig
    # here, use environment variables for configuration

    pyo3_code += "        let mut filter = log::LevelFilter ::Warn;\r\n"
    pyo3_code += "\r\n"
    pyo3_code += "        if std::env::var(\"AZUL_PY_LOGLEVEL_ERROR\").is_ok() { filter = log::LevelFilter ::Error; }\r\n"
    pyo3_code += "        if std::env::var(\"AZUL_PY_LOGLEVEL_WARN\").is_ok() { filter = log::LevelFilter ::Warn; }\r\n"
    pyo3_code += "        if std::env::var(\"AZUL_PY_LOGLEVEL_INFO\").is_ok() { filter = log::LevelFilter ::Info; }\r\n"
    pyo3_code += "        if std::env::var(\"AZUL_PY_LOGLEVEL_DEBUG\").is_ok() { filter = log::LevelFilter ::Debug; }\r\n"
    pyo3_code += "        if std::env::var(\"AZUL_PY_LOGLEVEL_TRACE\").is_ok() { filter = log::LevelFilter ::Trace; }\r\n"
    pyo3_code += "        if std::env::var(\"AZUL_PY_LOGLEVEL_OFF\").is_ok() { filter = log::LevelFilter ::Off; }\r\n"
    pyo3_code += "\r\n"

    # pyo3_code += "        match pyo3_log::Logger::new(py.clone(), pyo3_log::Caching::LoggersAndLevels) {\r\n"
    # pyo3_code += "            Ok(o) => {\r\n"
    # pyo3_code += "                match o.filter(filter).install() {\r\n"
    # pyo3_code += "                    Ok(_) => { }, \r\n"
    # pyo3_code += "                    Err(e) => { println!(\"Could not initialize Python logger, (continuing execution): {}\", e); }, \r\n"
    # pyo3_code += "                }\r\n"
    # pyo3_code += "            },\r\n"
    # pyo3_code += "            Err(e) => { println!(\"Could not create Python logger (continuing execution)\"); },\r\n"
    # pyo3_code += "        }\r\n"

    # pyo3_code += "        pyo3_log::init();\r\n"
    pyo3_code += "    }\r\n"
    pyo3_code += "\r\n"

    for module_name in api_data[version].keys():
        module = api_data[version][module_name]
        for class_name in module["classes"].keys():
            struct = module["classes"][class_name]
            if "struct_fields" in struct.keys():
                pyo3_code += "    m.add_class::<" + prefix + class_name + ">()?;\r\n"
            elif "enum_fields" in struct.keys():
                pyo3_code += "    m.add_class::<" + prefix + class_name + "EnumWrapper>()?;\r\n"
                pass
            elif "callback_typedef" in struct.keys():
                pass
        pyo3_code += "\r\n"
    pyo3_code += "    Ok(())\r\n"
    pyo3_code += "}\r\n"
    pyo3_code += "\r\n"
    return pyo3_code

# Formats the input function arguments for the python DLL
def format_py_args(python_replacements, fn_args, api_data, constructor=False):

    fn_args_string = ""

    # if the argument is a AzString, use a String instead
    for f in fn_args:
        f_name = list(f.keys())[0]
        if f_name == "self":
            continue
        f_type = f[f_name]
        analyzed_fn_type = analyze_type(f_type)
        f_type = analyzed_fn_type[1]

        ref = ""
        if analyzed_fn_type[0].strip() == "*const":
            ref = "&"
        elif analyzed_fn_type[0].strip() == "*mut":
            ref = "&mut "

        f_real_mut = ""
        f_real_type = ""
        if is_primitive_arg(f_type):
            f_real_type = ref + f_type
        else:
            f_class = quick_get_class(api_data, analyzed_fn_type[1])
            if "enum_fields" in f_class.keys():
                f_real_type = ref + prefix + f_type + "EnumWrapper"
            elif "struct_fields" in f_class.keys():
                f_real_type = ref + prefix + f_type
                if f_type in python_replacements.keys():
                    py_replace = python_replacements[f_type][0]
                    if py_replace.startswith("mut "):
                        f_real_mut = "mut "
                        f_real_type = ref + py_replace[4:]
                    else:
                        f_real_type = ref + py_replace
            else:
                raise Exception("cannot use type " + f_name + ": " + f_type + " as a Python function argument")

        fn_args_string += f_real_mut + f_name + ": " + f_real_type + ", "

    if not(len(fn_args_string) == 0):
        fn_args_string = fn_args_string[:-2]

    if (not(constructor) and not(len(fn_args_string) == 0)):
        fn_args_string = ", " + fn_args_string

    return fn_args_string

# Formats the return type for the python DLL
# returns the (return type, class_is_option, class_throws)
def format_py_return(python_replacements, return_type, api_data, errlist, constructor=False):
    if return_type["type"].startswith("Result"):
        found_c = quick_get_class(api_data, return_type["type"])
        ret_type_ok = found_c["enum_fields"][0]["Ok"]["type"]
        return_type_ok = ret_type_ok
        if not(ret_type_ok in python_replacements.keys()):
            return_type_ok = prefix + ret_type_ok
        else:
            return_type_ok = python_replacements[ret_type_ok][0]
        ret_type_err = found_c["enum_fields"][1]["Err"]["type"]
        return_type_err = ret_type_err
        if not(ret_type_err in python_replacements.keys()):
            return_type_err = prefix + ret_type_err
            errlist.append(return_type_err)
        else:
            return_type_err = python_replacements[ret_type_err][0]
        return ("Result<" + return_type_ok + ", PyErr>", None, return_type["type"])
    elif return_type["type"].startswith("Option"):
        found_c = quick_get_class(api_data, return_type["type"])
        ret_type = found_c["enum_fields"][1]["Some"]["type"]
        return_type_opt = ret_type
        if not(ret_type in python_replacements.keys()):
            return_type_opt = prefix + ret_type
            ret_type_c = quick_get_class(api_data, ret_type)
            if "enum_fields" in ret_type_c.keys():
                return_type_opt = return_type_opt + "EnumWrapper"
        else:
            return_type_opt = python_replacements[ret_type][0]
        return ("Option<" + return_type_opt + ">", return_type["type"], None)
    elif return_type["type"] in python_replacements.keys():
        return (python_replacements[return_type["type"]][0], None, None)
    else:
        # TODO: PyBuffer / Vec conversion
        if is_primitive_arg(return_type["type"]):
            return (return_type["type"], None, None)
        else:
            return_type_str = prefix + return_type["type"]
            ret_type_c = quick_get_class(api_data, return_type["type"])
            if "enum_fields" in ret_type_c.keys():
                return_type_str = return_type_str + "EnumWrapper"
            return (return_type_str, None, None)

def format_py_body(python_replacements, module_name, class_name, function_name, fn_args, api_data, return_type_str, returns_option=None, returns_error=None, constructor=False):

    # convert input "String" into azul-internal AzStrings
    string_conversions = ""
    for f in fn_args:
        f_name = list(f.keys())[0]
        f_type = f[f_name]
        if f_type in python_replacements.keys():
            python_replace = python_replacements[f_type][1]
            if len(python_replacements[f_type]) == 4:
                string_conversions += "let " + f_name + " = " + python_replacements[f_type][3] + "(&" + f_name + ");\r\n        "
            if python_replacements[f_type][0].startswith("mut "):
                string_conversions += "let " + f_name + " = " + python_replace + "(&mut " + f_name + ");\r\n        "
            else:
                string_conversions += "let " + f_name + " = " + python_replace + "(&" + f_name + ");\r\n        "

    fn_args_invoke = ""
    for f in fn_args:
        fn_args_invoke += "            mem::transmute(" + list(f.keys())[0] + "),\r\n"
    if not(len(fn_args_invoke) == 0):
        fn_args_invoke = "\r\n" + fn_args_invoke
        fn_args_invoke += "        "
    fn_body = ""
    fn_body += string_conversions

    if (not(returns_option is None)):
        # function returns option: cannot transmute, use match None { ... }
        # function throws an error: cannot transmute, use match Err { ... }
        fn_body += "let m: " + prefix + returns_option + " = unsafe { mem::transmute(crate::" + prefix + class_name + "_" + snake_case_to_lower_camel(function_name) + "(" + fn_args_invoke + ")) };\r\n"
        fn_body += "        match m {\r\n"
        fn_body += "            " + prefix + returns_option + "::Some(s) => Some("

        replace_option = ""

        # if function returns OptionString, OptionVecRefMut, ...
        for entry in python_replacements.keys():
            if returns_option == "Option" + entry:
                replace_option = entry

        if len(replace_option) != 0:
            fn_body += "{ let s: " + prefix + replace_option + " = unsafe { mem::transmute(s) }; s.into() }"
        else:
            fn_body += "unsafe { mem::transmute(s) }"

        fn_body += "),\r\n"
        fn_body += "            " + prefix + returns_option + "::None => None,\r\n"
        fn_body += "        }\r\n"
    elif not(returns_error is None):
        # function throws an error: cannot transmute, use match Err { ... }
        fn_body += "let m: " + prefix + returns_error + " = unsafe { mem::transmute(crate::" + prefix + class_name + "_" + snake_case_to_lower_camel(function_name) + "(" + fn_args_invoke + ")) };\r\n"
        fn_body += "        match m {\r\n"
        fn_body += "            " + prefix + returns_error + "::Ok(o) => Ok(o.into()),\r\n"
        fn_body += "            " + prefix + returns_error + "::Err(e) => Err(e.into()),\r\n"
        fn_body += "        }\r\n"
    else:
        if return_type_str in python_replacements.keys():
            fn_body += python_replacements[return_type_str][2] + "(unsafe { mem::transmute(crate::" + prefix + class_name + "_" + snake_case_to_lower_camel(function_name) + "(" + fn_args_invoke + ")) })"
        else:
            fn_body += "unsafe { mem::transmute(crate::" + prefix + class_name + "_" + snake_case_to_lower_camel(function_name) + "(" + fn_args_invoke + ")) }"
    return fn_body


# Generates the azul/rust/azul.rs file
def generate_rust_api(api_data, structs_map, functions_map):

    module_file_map = {}
    version = list(api_data.keys())[-1]
    module_file_map['dll'] = generate_rust_dll_bindings(api_data[version], structs_map, functions_map)
    myapi_data = api_data[version]

    for module_name in myapi_data.keys():
        code = ""
        module_doc = None
        if "doc" in myapi_data[module_name]:
            module_doc = myapi_data[module_name]["doc"]

        module = myapi_data[module_name]["classes"]

        code += "    #![allow(dead_code, unused_imports)]\r\n"
        if module_doc != None:
            code += "    //! " + module_doc + "\r\n"
        code += "    use crate::dll::*;\r\n"
        code += "    use core::ffi::c_void;\r\n"

        if tuple([module_name]) in rust_api_patches:
            code += rust_api_patches[tuple([module_name])]

        code += get_all_imports(myapi_data, module, module_name)

        for class_name in module.keys():
            c = module[class_name]

            class_can_derive_debug = "derive" in c.keys() and "Debug" in c["derive"]
            class_can_be_copied = "derive" in c.keys() and "Copy" in c["derive"]
            class_has_partialeq = "derive" in c.keys() and "PartialEq" in c["derive"]
            class_has_eq = "derive" in c.keys() and "Eq" in c["derive"]
            class_has_partialord = "derive" in c.keys() and "PartialOrd" in c["derive"]
            class_has_ord = "derive" in c.keys() and "Ord" in c["derive"]
            class_can_be_hashed = "derive" in c.keys() and "Hash" in c["derive"]

            class_is_boxed_object = not(class_is_stack_allocated(c))
            class_is_const = "const" in c.keys()
            class_is_callback_typedef = "callback_typedef" in c.keys() and (len(c["callback_typedef"]) > 0)
            class_has_custom_destructor = "custom_destructor" in c.keys() and c["custom_destructor"]
            treat_external_as_ptr = "external" in c.keys() and "is_boxed_object" in c.keys() and c["is_boxed_object"]

            class_can_be_cloned = True
            if "clone" in c.keys():
                class_can_be_cloned = c["clone"]

            c_is_stack_allocated = not(class_is_boxed_object)
            class_ptr_name = prefix + class_name

            if "doc" in c.keys():
                code += "    /// " + c["doc"] + "\r\n    "
            else:
                code += "    /// `" + class_name + "` struct\r\n    "

            code += "\r\n#[doc(inline)] pub use crate::dll::" + class_ptr_name + " as " + class_name + ";\r\n"

            has_constructors = ("constructors" in c.keys() and len(c["constructors"]) > 0)
            has_functions = ("functions" in c.keys() and len(c["functions"]) > 0)
            has_constants = ("constants" in c.keys() and len(c["constants"]) > 0)

            should_emit_impl = has_constructors or has_functions or has_constants and not(class_is_const or class_is_callback_typedef)

            if should_emit_impl:
                code += "    impl " + class_name + " {\r\n"

                if "constants" in c.keys():
                    for constant in c["constants"]:
                        constant_name = list(constant.keys())[0]
                        constant_type = constant[constant_name]["type"]
                        constant_value = constant[constant_name]["value"]
                        code += "        pub const " + constant_name + ": " + constant_type + " = " + constant_value + ";\r\n"
                    code += "\r\n"

                if "constructors" in c.keys():
                    for fn_name in c["constructors"]:
                        const = c["constructors"][fn_name]

                        c_fn_name = class_ptr_name + "_" + snake_case_to_lower_camel(fn_name)
                        fn_args = rust_bindings_fn_args(const, class_name, class_ptr_name, False, myapi_data)
                        fn_args_call = rust_bindings_call_fn_args(const, class_name, class_ptr_name, False, myapi_data, class_is_boxed_object)

                        fn_body = ""

                        if tuple([module_name, class_name, fn_name]) in rust_api_patches.keys() \
                        and "use_patches" in const.keys() \
                        and "rust" in const["use_patches"]:
                            fn_body = rust_api_patches[tuple([module_name, class_name, fn_name])]
                        else:
                            fn_body = "unsafe { crate::dll::" + c_fn_name + "(" + fn_args_call + ") }"

                        if "doc" in const.keys():
                            code += "        /// " + const["doc"] + "\r\n"
                        else:
                            code += "        /// Creates a new `" + class_name + "` instance.\r\n"

                        returns = "Self"
                        if "returns" in const.keys():
                            return_type = const["returns"]["type"]
                            returns = return_type
                            analyzed_return_type = analyze_type(return_type)
                            if is_primitive_arg(analyzed_return_type[1]):
                                fn_body = fn_body
                            else:
                                return_type_class = search_for_class_by_class_name(myapi_data, analyzed_return_type[1])
                                if return_type_class is None:
                                    print("no return type found for return type: " + return_type)
                                returns = analyzed_return_type[0] + " crate::" + return_type_class[0] + "::" + return_type_class[1] + analyzed_return_type[2]
                                fn_body = fn_body

                        code += "        pub fn " + fn_name + "(" + fn_args + ") -> " + returns + " { " + fn_body + " }\r\n"

                if "functions" in c.keys():
                    for fn_name in c["functions"]:
                        f = c["functions"][fn_name]

                        fn_args = rust_bindings_fn_args(f, class_name, class_ptr_name, True, myapi_data)
                        fn_args_call = rust_bindings_call_fn_args(f, class_name, class_ptr_name, True, myapi_data, class_is_boxed_object)
                        c_fn_name = class_ptr_name + "_" + snake_case_to_lower_camel(fn_name)

                        fn_body = ""

                        if tuple([module_name, class_name, fn_name]) in rust_api_patches.keys() \
                        and "use_patches" in const.keys() \
                        and "rust" in const["use_patches"]:
                            fn_body = rust_api_patches[tuple([module_name, class_name, fn_name])]
                        else:
                            fn_body = "unsafe { crate::dll::" + c_fn_name + "(" + fn_args_call + ") }"

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
                            return_type = f["returns"]["type"]
                            returns = " -> " + return_type
                            analyzed_return_type = analyze_type(return_type)
                            if is_primitive_arg(analyzed_return_type[1]):
                                fn_body = fn_body
                            else:
                                return_type_class = search_for_class_by_class_name(myapi_data, analyzed_return_type[1])
                                if return_type_class is None:
                                    print("no return type found for return type: " + return_type)
                                returns = " ->" + analyzed_return_type[0] + " crate::" + return_type_class[0] + "::" + return_type_class[1] + analyzed_return_type[2]
                                fn_body = fn_body

                        code += "        pub fn " + fn_name + "(" + fn_args + ") " +  returns + " { " + fn_body + " }\r\n"

                code += "    }\r\n\r\n" # end of class

            if treat_external_as_ptr and class_can_be_cloned:
                code += "    impl Clone for " + class_name + " { fn clone(&self) -> Self { unsafe { crate::dll::" + class_ptr_name + "_deepCopy(self) } } }\r\n"
            if treat_external_as_ptr:
                code += "    impl Drop for " + class_name + " { fn drop(&mut self) { unsafe { crate::dll::" + class_ptr_name + "_delete(self) } } }\r\n"


        module_file_map[module_name] = code

    final_code = ""

    for line in license.splitlines():
        final_code += "// " + line + "\r\n"

    final_code += read_file(root_folder + "/api/_patches/azul.rs/header.rs")

    for module_name in module_file_map.keys():
        if module_name != "dll":
            final_code += "pub "
        final_code += "mod " + module_name + " {\r\n"
        final_code += module_file_map[module_name]
        final_code += "}\r\n\r\n"

    return final_code

# Generate the RUST function callback type:
#
# extern "C" fn(&Blah, &Foo) -> FooReturn
def generate_rust_callback_fn_type(api_data, callback_typedef):
    # callback_typedef

    fn_string = "extern \"C\" fn("

    if "fn_args" in callback_typedef.keys():
        fn_args = callback_typedef["fn_args"]
        for fn_arg in fn_args:
            fn_arg_type = fn_arg["type"]
            fn_arg_ref = fn_arg["ref"]
            search_result = search_for_class_by_class_name(api_data, fn_arg_type)
            fn_arg_class = fn_arg_type
            if not(is_primitive_arg(fn_arg_type)):
                if search_result is None:
                    print("fn_arg_type " + fn_arg_type + " not found!")
                fn_arg_class = search_result[1]

            if not(is_primitive_arg(fn_arg_type)):
                if fn_arg_ref == "ref":
                    fn_string += "&" + prefix + fn_arg_class
                elif fn_arg_ref == "refmut":
                    fn_string += "&mut " + prefix + fn_arg_class
                elif fn_arg_ref == "value":
                    fn_string += prefix + fn_arg_class
                else:
                    raise Exception("wrong fn_arg_ref on " + fn_arg_type)
            else:
                if fn_arg_ref == "ref":
                    fn_string += "&"  + fn_arg_class
                elif fn_arg_ref == "refmut":
                    fn_string += "&mut " + fn_arg_class
                elif fn_arg_ref == "value":
                    fn_string += fn_arg_class
                else:
                    raise Exception("wrong fn_arg_ref on " + fn_arg_type)

            fn_string += ", "

        if len(fn_args) > 0:
            fn_string = fn_string[:-2] # trim last comma

    fn_string += ")"

    if "returns" in callback_typedef.keys():
        fn_string += " -> "
        fn_arg_type = callback_typedef["returns"]["type"]
        search_result = search_for_class_by_class_name(api_data, fn_arg_type)
        fn_arg_class = fn_arg_type

        if not(is_primitive_arg(fn_arg_type)):
            if search_result is None:
                print("fn_arg_type " + fn_arg_type + " not found!")
                raise Exception("fn_arg_type " + fn_arg_type + " not found!")
            fn_arg_class = search_result[1]

        if not(is_primitive_arg(fn_arg_type)):
            fn_string += prefix + fn_arg_class
        else:
            fn_string += fn_arg_class

    return fn_string

# Generate the C coded for the struct layout of the final API
def generate_c_structs(api_data, structs_map, forward_declarations, extra_forward_delcarations, use_prefix=True, typedef_style="c"):
    code = ""

    pfx = prefix
    if not(use_prefix):
        pfx = ""

    # Put all function pointers at the top and forward-declare all the structs
    # C does not allow (?) to forward declare function pointers
    function_pointers = []

    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]
        class_is_callback_typedef = "callback_typedef" in struct.keys() and (len(struct["callback_typedef"].keys()) > 0)
        if class_is_callback_typedef:
            if typedef_style == "c":
                function_pointers.append(tuple((struct["callback_typedef"], generate_c_callback_fn_type(api_data, struct["callback_typedef"], struct_name, use_prefix))))
            elif typedef_style == "cpp":
                function_pointers.append(tuple((struct["callback_typedef"], generate_cpp_callback_fn_type(api_data, struct["callback_typedef"], struct_name, use_prefix))))

    function_pointer_string = ""
    already_forward_declared = []

    for fnptr in function_pointers:
        if "fn_args" in fnptr[0].keys():
            for arg in fnptr[0]["fn_args"]:
                arg_type = analyze_type(arg["type"])[1]
                if is_primitive_arg(analyze_type(arg_type)[1]):
                    continue
                if not(arg_type in already_forward_declared):
                    # forward declare the correct type (struct, enum, union)
                    arg_type_type = "struct"
                    found_c = search_for_class_by_class_name(api_data, arg_type)
                    c = get_class(api_data, found_c[0], found_c[1])
                    if "enum_fields" in c.keys():
                        arg_type_type = "enum"
                        if enum_is_union(c["enum_fields"]):
                            arg_type_type = "union"
                    function_pointer_string += "\r\n" + arg_type_type + " " + pfx + arg_type + ";"
                    if typedef_style == "c":
                        function_pointer_string += "\r\ntypedef " + arg_type_type + " " + pfx + arg_type + " " + pfx + arg_type + ";"

                    already_forward_declared.append(arg_type)

        if "returns" in fnptr[0].keys():
            return_type = fnptr[0]["returns"]["type"]
            return_type = analyze_type(return_type)[1]
            if not(is_primitive_arg(return_type)):
                if not(return_type in already_forward_declared):
                    # forward declare the correct type (struct, enum, union)
                    arg_type_type = "struct"
                    found_c = search_for_class_by_class_name(api_data, return_type)
                    c = get_class(api_data, found_c[0], found_c[1])
                    if "enum_fields" in c.keys():
                        arg_type_type = "enum"
                        if enum_is_union(c["enum_fields"]):
                            arg_type_type = "union"
                    function_pointer_string += "\r\n" + arg_type_type + " " + pfx + return_type + ";"
                    if typedef_style == "c":
                        function_pointer_string += "\r\ntypedef " + arg_type_type + " " + pfx + return_type + " " + pfx + return_type + ";"
                    already_forward_declared.append(return_type)

        function_pointer_string += "\r\n"
        function_pointer_string += fnptr[1]
        function_pointer_string += "\r\n"

    code += function_pointer_string
    code += "\r\n"

    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]
        class_is_callback_typedef = "callback_typedef" in struct.keys() and (len(struct["callback_typedef"].keys()) > 0)
        class_can_be_copied = "derive" in struct.keys() and "Copy" in struct["derive"]
        class_has_custom_destructor = "custom_destructor" in struct.keys() and struct["custom_destructor"]
        class_can_be_cloned = True
        if "clone" in struct.keys():
            class_can_be_cloned = struct["clone"]

        is_boxed_object = "is_boxed_object" in struct.keys() and struct["is_boxed_object"]
        treat_external_as_ptr = "external" in struct.keys() and is_boxed_object

        if struct_name in extra_forward_delcarations.keys():
            struct_forward_decl = extra_forward_delcarations[struct_name]
            code += "\r\n" + struct_forward_decl["type"] + " " + struct_forward_decl["name"] + ";"
            if typedef_style == "c":
                code += "\r\ntypedef " + struct_forward_decl["type"] + " " + struct_forward_decl["name"] + " " + struct_forward_decl["name"] + ";"

        if class_is_callback_typedef:
            # function_pointers += generate_c_callback_fn_type(api_data, struct["callback_typedef"], struct_name)
            # function_pointers += "\r\n"
            pass
        elif "struct" in struct.keys():
            struct = struct["struct"]
            # https://stackoverflow.com/questions/65043140/how-to-forward-declare-structs-in-c
            code += "\r\nstruct " + struct_name + " {\r\n"

            for field in struct:
                if type(field) is str:
                    print("Struct " + struct_name + " should have a dictionary as fields")
                field_name = list(field.keys())[0]
                field_type = list(field.values())[0]
                if "type" in field_type:
                    field_type = field_type["type"]
                    analyzed_arg_type = analyze_type(field_type)

                    # arrays: convert blah: [BlahType;4] to BlahType blah[4]
                    is_array = False
                    if (len(analyzed_arg_type[2]) == 3 and analyzed_arg_type[2].startswith(";")):
                        analyzed_arg_type[2] = analyzed_arg_type[2][1:]
                        is_array = True

                    if is_primitive_arg(analyzed_arg_type[1]):
                        if is_array:
                            code += "    " + replace_primitive_ctype(analyzed_arg_type[1]) + " " + field_name + replace_primitive_ctype(analyzed_arg_type[0]).strip() + analyzed_arg_type[2] + ";\r\n"
                        else:
                            code += "    " + replace_primitive_ctype(analyzed_arg_type[1]) + replace_primitive_ctype(analyzed_arg_type[0]).strip() + analyzed_arg_type[2] + " " + field_name + ";\r\n"
                    else:
                        field_type_class_path = search_for_class_by_class_name(api_data, analyzed_arg_type[1])
                        if field_type_class_path is None:
                            print("no field_type_class_path found for " + str(analyzed_arg_type))

                        found_c = get_class(api_data, field_type_class_path[0], field_type_class_path[1])
                        if is_array:
                            code += "    " + pfx + field_type_class_path[1] + " " + field_name + replace_primitive_ctype(analyzed_arg_type[0]).strip() + analyzed_arg_type[2] + ";\r\n"
                        else:
                            code += "    " + pfx + field_type_class_path[1] + replace_primitive_ctype(analyzed_arg_type[0]).strip()  + analyzed_arg_type[2]+ " " + field_name + ";\r\n"
                else:
                    print("struct " + struct_name + " does not have a type on field " + field_name)
                    raise Exception("error")

            if typedef_style == "cpp":
                code += "    " + struct_name + "& operator=(const " + struct_name + "&) = delete; /* disable assignment operator, use std::move (default) or .clone() */\r\n"
                if not(class_can_be_copied):
                    code += "    " + struct_name + "(const " + struct_name + "&) = delete; /* disable copy constructor, use explicit .clone() */\r\n"
                code += "    " + struct_name + "() = delete; /* disable default constructor, use C++20 designated initializer instead */\r\n"
            code += "};\r\n"

            if not(struct_name in already_forward_declared):
                if typedef_style == "c":
                    code += "typedef struct " + struct_name + " " + struct_name + ";\r\n"

        elif "enum" in struct.keys():
            enum = struct["enum"]
            if not(enum_is_union(enum)):
                if typedef_style == "cpp":
                    code += "\r\nenum class " + struct_name + " {\r\n"
                else:
                    code += "\r\nenum " + struct_name + " {\r\n"
                for variant in enum:
                    variant_name = list(variant.keys())[0]
                    variant_real = list(variant.values())[0]
                    if typedef_style == "cpp":
                        code += "   " + variant_name + ",\r\n"
                    else:
                        code += "   " + struct_name + "_" + variant_name + ",\r\n"
                code += "};\r\n"
                if not(struct_name in already_forward_declared):
                    if typedef_style == "c":
                        code += "typedef enum " + struct_name + " " + struct_name + ";\r\n"
            else:
                # generate union tag
                if typedef_style == "cpp":
                    code += "\r\nenum class " + struct_name + "Tag {\r\n"
                else:
                    code += "\r\nenum " + struct_name + "Tag {\r\n"
                for variant in enum:
                    variant_name = list(variant.keys())[0]
                    if typedef_style == "cpp":
                        code += "   " + variant_name + ",\r\n"
                    else:
                        code += "   " + struct_name + "Tag_" + variant_name + ",\r\n"
                code += "};\r\n"
                if typedef_style == "c":
                    code += "typedef enum " + struct_name + "Tag " + struct_name + "Tag;\r\n"

                # generate union variants
                for variant in enum:
                    variant_name = list(variant.keys())[0]
                    variant_real = list(variant.values())[0]
                    c_type = ""
                    if "type" in variant_real.keys():
                        variant_type = variant_real["type"]
                        analyzed_variant_type = analyze_type(variant_type)
                        variant_prefix = pfx
                        if is_primitive_arg(analyzed_variant_type[1]):
                            variant_prefix = ""

                        # arrays: convert blah: [BlahType;4] to BlahType blah[4]
                        is_array = False
                        if (len(analyzed_variant_type[2]) == 3 and analyzed_variant_type[2].startswith(";")):
                            analyzed_variant_type[2] = analyzed_variant_type[2][1:]
                            is_array = True

                        if is_array:
                            c_type = " " + variant_prefix + replace_primitive_ctype(analyzed_variant_type[1]).strip()  + " payload" + replace_primitive_ctype(analyzed_variant_type[0]).strip() + analyzed_variant_type[2] + ";"
                        else:
                            c_type = " " + variant_prefix + replace_primitive_ctype(analyzed_variant_type[1]).strip() + replace_primitive_ctype(analyzed_variant_type[0]).strip() + analyzed_variant_type[2] + " payload;"

                    code += "\r\nstruct " + struct_name + "Variant_" + variant_name + " { " + struct_name + "Tag tag;" + c_type + " };"
                    if typedef_style == "c":
                        code += "\r\ntypedef struct " + struct_name + "Variant_" + variant_name + " " + struct_name + "Variant_" + variant_name + ";"

                # generate union
                code += "\r\nunion " + struct_name + " {\r\n"
                for variant in enum:
                    variant_name = list(variant.keys())[0]
                    code += "    " + struct_name + "Variant_" + variant_name + " " + variant_name + ";\r\n"
                code += "};\r\n"
                if not(struct_name in already_forward_declared):
                    if typedef_style == "c":
                        code += "typedef union " + struct_name + " " + struct_name + ";"

                code += "\r\n"

    return code

# Generate BlahVec_fromConstArray() macros and BlahVec_empty() macros
# NOTE: This is only in the C API, the C++ API uses consteval
def generate_c_union_macros_and_vec_constructors(api_data, structs_map):
    code = ""

    version = list(api_data.keys())[-1]
    myapi_data = api_data[version]

    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]

        if not("enum" in struct.keys()):
            continue

        enum = struct["enum"]
        if not(enum_is_union(enum)):
            continue

        # generate macros for creating variants
        for variant in enum:
            variant_name = list(variant.keys())[0]
            if "type" in variant[variant_name]:
                code += "\r\n#define " + struct_name + "_" + variant_name + "(v) { ." + variant_name + " = { .tag = " + struct_name + "Tag_" + variant_name + ", .payload = v } }"
            else:
                code += "\r\n#define " + struct_name + "_" + variant_name + " { ." + variant_name + " = { .tag = " + struct_name + "Tag_" + variant_name + " } }"


    # generate automatic "empty" constructor macros for all types in the "vec" module
    # for struct in api_data["0.1.0"]["classes"]["vec"]
    if "vec" in myapi_data.keys():
        for vec_name in myapi_data["vec"]["classes"].keys():
            if vec_name.endswith("Vec"):
                vec_type = analyze_type(myapi_data["vec"]["classes"][vec_name]["struct_fields"][0]["ptr"]["type"])[1]
                if is_primitive_arg(vec_type):
                    code += "\r\n" + replace_primitive_ctype(vec_type).strip() + " " +  prefix + vec_name + "Array[] = {};"
                    code += "\r\n#define " + prefix + vec_name + "_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(" + replace_primitive_ctype(vec_type).strip() + "), .cap = sizeof(v) / sizeof(" + replace_primitive_ctype(vec_type).strip() + "), .destructor = { .NoDestructor = { .tag = " + prefix + vec_name + "DestructorTag_NoDestructor, }, }, }"
                else:
                    code += "\r\n" + prefix + vec_type + " " +  prefix + vec_name + "Array[] = {};"
                    code += "\r\n#define " + prefix + vec_name + "_fromConstArray(v) { .ptr = &v, .len = sizeof(v) / sizeof(" + prefix + vec_name[:-3] + "), .cap = sizeof(v) / sizeof(" + prefix + vec_name[:-3] + "), .destructor = { .NoDestructor = { .tag = " + prefix + vec_name + "DestructorTag_NoDestructor, }, }, }"
                code += "\r\n#define " + prefix + vec_name + "_empty { .ptr = &" + prefix + vec_name + "Array, .len = 0, .cap = 0, .destructor = { .NoDestructor = { .tag = " + prefix + vec_name + "DestructorTag_NoDestructor, }, }, }"
                code += "\r\n"

    return code

# returns whether an enum is a union
def enum_is_union(enum):
    enum_is_c_enum = True
    for variant in enum:
        variant_name = list(variant.keys())[0]
        variant_real = list(variant.values())[0]
        if "type" in variant_real.keys():
            enum_is_c_enum = False # enum is tagged union
    return not(enum_is_c_enum)

# takes the api data and a function callback and returns
# the C function pointer typedef, i.e.:
#
# generate_c_callback_fn_type(api_data, {"fn_args": [{"type": "Blah", "ref": "value"}], "returns": {"type": "Foo"}}, "BlahCallback")
# => "typedef Foo (*BlahCallback)(Blah);"
def generate_c_callback_fn_type(api_data, callback_typedef, callback_name,use_prefix=True):
    # callback_typedef

    return_val = "void"
    pfx = prefix
    if not(use_prefix):
        pfx = ""

    if "returns" in callback_typedef.keys():
        fn_arg_type = callback_typedef["returns"]["type"]
        search_result = search_for_class_by_class_name(api_data, fn_arg_type)
        fn_arg_class = fn_arg_type

        if not(is_primitive_arg(fn_arg_type)):
            if search_result is None:
                print("fn_arg_type " + fn_arg_type + " not found!")
                raise Exception("fn_arg_type " + fn_arg_type + " not found!")
            fn_arg_class = search_result[1]

        if not(is_primitive_arg(fn_arg_type)):
            return_val = pfx + fn_arg_class
        else:
            return_val = fn_arg_class

    fn_string = "typedef " + return_val + " (*" + callback_name + ")("

    if "fn_args" in callback_typedef.keys():
        fn_args = callback_typedef["fn_args"]
        fn_arg_idx = 0
        for fn_arg in fn_args:
            fn_arg_type = fn_arg["type"]
            fn_arg_ref = fn_arg["ref"]
            search_result = search_for_class_by_class_name(api_data, fn_arg_type)
            fn_arg_class = fn_arg_type
            if not(is_primitive_arg(fn_arg_type)):
                if search_result is None:
                    print("fn_arg_type " + fn_arg_type + " not found!")
                fn_arg_class = search_result[1]

            if not(is_primitive_arg(fn_arg_type)):
                if fn_arg_ref == "ref":
                    fn_string += pfx + fn_arg_class + "* const"
                elif fn_arg_ref == "refmut":
                    fn_string += pfx + fn_arg_class + "* restrict"
                elif fn_arg_ref == "value":
                    fn_string += pfx + fn_arg_class
                else:
                    raise Exception("wrong fn_arg_ref on " + fn_arg_type)
            else:
                if fn_arg_ref == "ref":
                    fn_string += "const " + replace_primitive_ctype(fn_arg_class) + "*"
                elif fn_arg_ref == "refmut":
                    fn_string += replace_primitive_ctype(fn_arg_class) + "* restrict"
                elif fn_arg_ref == "value":
                    fn_string += replace_primitive_ctype(fn_arg_class)
                else:
                    raise Exception("wrong fn_arg_ref on " + fn_arg_type)

            fn_string += " "
            fn_string += chr(fn_arg_idx + 65)
            fn_string += ", "
            fn_arg_idx += 1

        if len(fn_args) > 0:
            fn_string = fn_string[:-2] # trim last comma

    fn_string += ");"

    return fn_string

# takes the api data and a function callback and returns
# the C function pointer typedef, i.e.:
#
# generate_c_callback_fn_type(api_data, {"fn_args": [{"type": "Blah", "ref": "value"}], "returns": {"type": "Foo"}}, "BlahCallback")
# => "using BlahCallback = Foo(*)(Blah);"
def generate_cpp_callback_fn_type(api_data, callback_typedef, callback_name,use_prefix=True):
    # callback_typedef

    return_val = "void"
    pfx = prefix
    if not(use_prefix):
        pfx = ""

    if "returns" in callback_typedef.keys():
        fn_arg_type = callback_typedef["returns"]["type"]
        search_result = search_for_class_by_class_name(api_data, fn_arg_type)
        fn_arg_class = fn_arg_type

        if not(is_primitive_arg(fn_arg_type)):
            if search_result is None:
                print("fn_arg_type " + fn_arg_type + " not found!")
                raise Exception("fn_arg_type " + fn_arg_type + " not found!")
            fn_arg_class = search_result[1]

        if not(is_primitive_arg(fn_arg_type)):
            return_val = pfx + fn_arg_class
        else:
            return_val = fn_arg_class

    fn_string = "using " + callback_name + " = " + return_val + "(*)("

    if "fn_args" in callback_typedef.keys():
        fn_args = callback_typedef["fn_args"]
        fn_arg_idx = 0
        for fn_arg in fn_args:
            fn_arg_type = fn_arg["type"]
            fn_arg_ref = fn_arg["ref"]
            search_result = search_for_class_by_class_name(api_data, fn_arg_type)
            fn_arg_class = fn_arg_type
            if not(is_primitive_arg(fn_arg_type)):
                if search_result is None:
                    print("fn_arg_type " + fn_arg_type + " not found!")
                fn_arg_class = search_result[1]

            if not(is_primitive_arg(fn_arg_type)):
                if fn_arg_ref == "ref":
                    fn_string += pfx + fn_arg_class + "* const"
                elif fn_arg_ref == "refmut":
                    fn_string += pfx + fn_arg_class + "* restrict"
                elif fn_arg_ref == "value":
                    fn_string += pfx + fn_arg_class
                else:
                    raise Exception("wrong fn_arg_ref on " + fn_arg_type)
            else:
                if fn_arg_ref == "ref":
                    fn_string += "const " + replace_primitive_ctype(fn_arg_class) + "*"
                elif fn_arg_ref == "refmut":
                    fn_string += replace_primitive_ctype(fn_arg_class) + "* restrict"
                elif fn_arg_ref == "value":
                    fn_string += replace_primitive_ctype(fn_arg_class)
                else:
                    raise Exception("wrong fn_arg_ref on " + fn_arg_type)

            # fn_string += " "
            # fn_string += chr(fn_arg_idx + 65)
            fn_string += ", "
            fn_arg_idx += 1

        if len(fn_args) > 0:
            fn_string = fn_string[:-2] # trim last comma

    fn_string += ");"

    return fn_string

# returns the C type for the primitive rust type
def replace_primitive_ctype(input):
    # https://locka99.gitbooks.io/a-guide-to-porting-c-to-rust/content/features_of_rust/types.html
    input = input.strip()
    # C: #include <stdint.h>
    # C++: #include <cstdlib>
    switcher = {
        "*const": "* ", # TODO: figure out proper c semantics - the VALUE is const, not the POINTER!
        "*mut": "* restrict ",
        "i8": "int8_t",
        "u8": "uint8_t",
        "i16": "int16_t",
        "u16": "uint16_t",
        "i32": "int32_t",
        "i64": "int64_t",
        "isize": "ssize_t",
        "u32": "uint32_t",
        "u64": "uint64_t",
        "f32": "float",
        "f64": "double",
        "usize": "size_t",
        "c_void": "void",
    }
    return switcher.get(input, input + " ")

# Generates the functions to put in the C header file
# assumes that all structs / data types have already been declared previously
def generate_c_functions(api_data,use_prefix=True,typedef_style="c"):

    code = ""

    pfx = prefix
    if not(use_prefix):
        pfx = ""

    function_prefix = "extern DLLIMPORT "
    if typedef_style == "cpp":
        function_prefix = ""

    version = list(api_data.keys())[-1]
    myapi_data = api_data[version]

    code += "\r\n"
    code += "\r\n/* FUNCTIONS from azul.dll / libazul.so */"

    for module_name in myapi_data.keys():
        module = myapi_data[module_name]["classes"]
        for class_name in module.keys():
            c = module[class_name]

            c_is_stack_allocated = class_is_stack_allocated(c)
            class_can_be_copied = "derive" in c.keys() and "Copy" in c["derive"]
            class_has_custom_destructor = "custom_destructor" in c.keys() and c["custom_destructor"]
            is_boxed_object = "is_boxed_object" in c.keys() and c["is_boxed_object"]
            treat_external_as_ptr = "external" in c.keys() and is_boxed_object
            class_can_be_cloned = True
            if "clone" in c.keys():
                class_can_be_cloned = c["clone"]

            class_ptr_name = pfx + class_name
            print_separator = False

            if "constructors" in c.keys():
                print_separator = True
                for constructor_name in c["constructors"].keys():
                    const = c["constructors"][constructor_name]
                    fn_args = c_fn_args_c_api(const, class_name, class_ptr_name, False)
                    code += "\r\n" + function_prefix + class_ptr_name + " " + class_ptr_name + "_" + snake_case_to_lower_camel(constructor_name) + "(" + fn_args + ");"

            if "functions" in c.keys():
                print_separator = True
                for function_name in c["functions"].keys():
                    function = c["functions"][function_name]
                    fn_args = c_fn_args_c_api(function, class_name, class_ptr_name, True)

                    return_val = "void"
                    if "returns" in function.keys():
                        analyzed_return_type = analyze_type(function["returns"]["type"])
                        if is_primitive_arg(analyzed_return_type[1]):
                            return_val = replace_primitive_ctype(analyzed_return_type[1])
                        else:
                            return_val = pfx + analyzed_return_type[1]

                    code += "\r\n" + function_prefix + return_val + " "+ class_ptr_name + "_" + snake_case_to_lower_camel(function_name) + "(" + fn_args + ");"

            if c_is_stack_allocated:
                if class_can_be_copied:
                    # intentionally empty, no destructor necessary
                    pass
                elif class_has_custom_destructor or treat_external_as_ptr:
                    print_separator = True
                    code += "\r\n" + function_prefix + "void " + class_ptr_name + "_delete(" + class_ptr_name + "* restrict instance);"

                if treat_external_as_ptr and class_can_be_cloned:
                    print_separator = True
                    code += "\r\n" + function_prefix + class_ptr_name + " " + class_ptr_name + "_deepCopy(" + class_ptr_name + "* const instance);"

            # if print_separator:
            #   code += "\r\n"

    return code

# Generates all constants
def generate_c_constants(api_data):

    version = list(api_data.keys())[-1]
    myapi_data = api_data[version]

    code = ""
    code += "\r\n"
    code += "\r\n/* CONSTANTS */\r\n\r\n"

    for module_name in myapi_data.keys():
        module = myapi_data[module_name]["classes"]
        for class_name in module.keys():
            c = module[class_name]

            if "constants" in c.keys():
                for constant in c["constants"]:
                    constant_name = list(constant.keys())[0]
                    constant_type = constant[constant_name]["type"]
                    constant_value = constant[constant_name]["value"]
                    code += "#define " + prefix + class_name + "_" + constant_name + " " + constant_value + "\r\n"
                code += "\r\n"

    return code

# Generates extra functions for C to destructure tagged union enums
def generate_c_extra_functions(api_data):

    version = list(api_data.keys())[-1]
    myapi_data = api_data[version]

    code = ""

    for module_name in myapi_data.keys():
        module = myapi_data[module_name]["classes"]
        for class_name in module.keys():
            c = module[class_name]
            e = False

            if "enum_fields" in c.keys():
                if enum_is_union(c["enum_fields"]):
                    e = True

            if not(e):
                continue

            # generate _matchRef and _matchMut functions for each variant in the enum
            for variant in c["enum_fields"]:
                variant_name = list(variant.keys())[0]
                if "type" in variant[variant_name]:
                    type_name = variant[variant_name]["type"]

                    code += "bool " + prefix + class_name + "_matchRef" + variant_name + "(const " + prefix + class_name + "* value, const " + prefix + type_name + "** restrict out) {\r\n"
                    code += "    const " + prefix + class_name + "Variant_" + variant_name + "* casted = (const " + prefix + class_name +"Variant_" + variant_name + "*)value;\r\n"
                    code += "    bool valid = casted->tag == " + prefix + class_name + "Tag_" + variant_name + ";\r\n"
                    code += "    if (valid) { *out = &casted->payload; } else { *out = 0; }\r\n"
                    code += "    return valid;\r\n"
                    code += "}\r\n\r\n"

                    code += "bool " + prefix + class_name + "_matchMut" + variant_name + "(" + prefix + class_name + "* restrict value, " + prefix + type_name + "* restrict * restrict out) {\r\n"
                    code += "    " + prefix + class_name + "Variant_" + variant_name + "* restrict casted = (" + prefix + class_name +"Variant_" + variant_name + "* restrict)value;\r\n"
                    code += "    bool valid = casted->tag == " + prefix + class_name + "Tag_" + variant_name + ";\r\n"
                    code += "    if (valid) { *out = &casted->payload; } else { *out = 0; }\r\n"
                    code += "    return valid;\r\n"
                    code += "}\r\n\r\n"

    return code

def generate_c_api(api_data, structs_map):
    code = ""

    version = list(api_data.keys())[-1]
    myapi_data = api_data[version]

    structs_map = sort_structs_map(myapi_data, structs_map)
    extra_forward_delcarations = structs_map[2]
    forward_delcarations = structs_map[1]
    structs_map = structs_map[0]

    code += "#ifndef AZUL_H\r\n"
    code += "#define AZUL_H\r\n"
    code += "\r\n"
    code += "#include <stdbool.h>\r\n" # bool
    code += "#include <stdint.h>\r\n" # uint8_t, ...
    code += "#include <stddef.h>\r\n" # size_t
    code += "\r\n"
    code += "/* C89 port for \"restrict\" keyword from C99 */\r\n"
    code += "#if __STDC__ != 1\r\n"
    code += "#    define restrict __restrict\r\n"
    code += "#else\r\n"
    code += "#    ifndef __STDC_VERSION__\r\n"
    code += "#        define restrict __restrict\r\n"
    code += "#    else\r\n"
    code += "#        if __STDC_VERSION__ < 199901L\r\n"
    code += "#            define restrict __restrict\r\n"
    code += "#        endif\r\n"
    code += "#    endif\r\n"
    code += "#endif\r\n"
    code += "\r\n"
    code += "/* cross-platform define for ssize_t (signed size_t) */\r\n"
    code += "#ifdef _WIN32\r\n"
    code += "    #include <windows.h>\r\n"
    code += "    #ifdef _MSC_VER\r\n"
    code += "        typedef SSIZE_T ssize_t;\r\n"
    code += "    #endif\r\n"
    code += "#else\r\n"
    code += "    #include <sys/types.h>\r\n"
    code += "#endif\r\n"
    code += "\r\n"
    code += "/* cross-platform define for __declspec(dllimport) */\r\n"
    code += "#ifdef _WIN32\r\n"
    code += "    #define DLLIMPORT __declspec(dllimport)\r\n"
    code += "#else\r\n"
    code += "    #define DLLIMPORT\r\n"
    code += "#endif\r\n"
    code += "\r\n"

    code += generate_c_structs(myapi_data, structs_map, forward_delcarations, extra_forward_delcarations)
    code += generate_c_union_macros_and_vec_constructors(api_data, structs_map)
    code += generate_c_functions(api_data)
    code += generate_c_constants(api_data)
    code += generate_c_extra_functions(api_data)

    code += "\r\n"
    code += read_file(root_folder + "/api/_patches/c/patch.h")
    code += "\r\n"
    code += "\r\n#endif /* AZUL_H */\r\n"
    return code

def generate_cpp_api(api_data, structs_map):
    code = ""

    version = list(api_data.keys())[-1]
    myapi_data = api_data[version]

    structs_map = sort_structs_map(myapi_data, structs_map)
    extra_forward_delcarations = structs_map[2]
    forward_delcarations = structs_map[1]
    structs_map = structs_map[0]

    code += "#ifndef AZUL_H\r\n"
    code += "#define AZUL_H\r\n"
    code += "\r\n"
    code += "namespace dll {\r\n"
    code += "\r\n"
    code += "    #include <cstdint>\r\n" # uint8_t, ...
    code += "    #include <cstddef>\r\n" # size_t

    # strip the prefix from the struct entries
    # (not necessary for the C++ API, only for function names)
    # this could've been done cleaner
    stripped = strip_all_prefixes(structs_map, forward_delcarations, extra_forward_delcarations)
    structs_map = stripped[0]
    forward_delcarations = stripped[1]
    extra_forward_delcarations = stripped[2]

    # add structs, no prefix, use C++ style function pointer typedefs
    c_struct_code = generate_c_structs(myapi_data, structs_map, forward_delcarations, extra_forward_delcarations, use_prefix=False,typedef_style="cpp")
    for line in c_struct_code.splitlines():
        code += "    " + line + "\r\n"
    code += "\r\n"

    code += "    extern \"C\" {"
    c_functions_code = generate_c_functions(api_data,use_prefix=False,typedef_style="cpp")
    for line in c_functions_code.splitlines():
        code += "        " + line + "\r\n"
    code += "\r\n"
    code += "    } /* extern \"C\" */\r\n"
    code += "\r\n"

    code += "} /* namespace */ \r\n"


    code += "\r\n"
    code += "\r\n#endif /* AZUL_H */\r\n"

    return code

def strip_all_prefixes(structs_map, forward_delcarations, extra_forward_delcarations):

    # strip structs_map
    new_structs_map = OrderedDict({})
    for key in structs_map.keys():
        value = structs_map[key]
        key = key[len(prefix):]
        new_structs_map[key] = value

    # strip forward_delcarations
    new_forward_delcarations = OrderedDict({})
    for key in forward_delcarations.keys():
        value = forward_delcarations[key]
        key = key[len(prefix):]
        new_forward_delcarations[key] = value

    new_extra_forward_delcarations = OrderedDict({})
    for key in extra_forward_delcarations.keys():
        value = extra_forward_delcarations[key]
        key = key[len(prefix):]
        new_extra_forward_delcarations[key] = value

    return [new_structs_map, new_forward_delcarations, new_extra_forward_delcarations]

# generate a test function that asserts that the struct layout in the DLL
# is the same as in the generated bindings
def generate_size_test(api_data, structs_map):

    generated_structs = generate_structs(api_data, structs_map, False)

    test_str = ""

    test_str += "#[cfg(all(test, not(feature = \"rlib\")))]\r\n"
    test_str += "#[allow(dead_code)]\r\n"
    test_str += "mod test_sizes {\r\n"

    test_str += read_file(root_folder + "/api/_patches/azul-dll/test-sizes.rs")

    test_str += generated_structs
    test_str += "    use core::ffi::c_void;\r\n"
    test_str += "    use azul_impl::css::*;\r\n"
    test_str += "\r\n"

    test_str += "    #[test]\r\n"
    test_str += "    fn test_size() {\r\n"
    test_str += "         use core::alloc::Layout;\r\n"

    for struct_name in structs_map.keys():
        struct = structs_map[struct_name]
        if "external" in struct.keys():
            external_path = struct["external"]
            test_str += "        assert_eq!((Layout::new::<" + external_path + ">(), \"" + struct_name +  "\"), (Layout::new::<" + struct_name + ">(), \"" + struct_name +  "\"));\r\n"

    test_str += "    }\r\n"
    test_str += "}\r\n"
    return test_str

# ---------------------------

def verify_clang_is_installed():
    if not(os.environ['CC'] == 'clang-cl') or not(os.environ['CXX'] == 'clang-cl'):
        raise Exception("environment variables CC and CXX have to be set to 'clang-cl' before building! Make sure LLVM and clang is installed!")

def cleanup_start():
    # TODO: remove entire /api folder and re-generate it?
    files = ["azul.dll", "azul.sh", "azul.dylib"]

    for f in files:
        if os.path.exists(root_folder + "/target/debug/examples/" + f):
            remove_path(root_folder + "/target/debug/examples/" + f)

        if os.path.exists(root_folder + "/target/release/examples/" + f):
            remove_path(root_folder + "/target/release/examples/" + f)

        if os.path.exists(root_folder + "/target/debug/" + f):
            remove_path(root_folder + "/target/debug/" + f)

        if os.path.exists(root_folder + "/target/release/" + f):
            remove_path(root_folder + "/target/release/" + f)

    # if (len(os.environ.get('AZUL_INSTALL_DIR', '')) > 0):
    #     if os.path.exists(os.environ['AZUL_INSTALL_DIR']):
    #         remove_path(os.environ['AZUL_INSTALL_DIR'])

def generate_api():
    apiData = read_api_file(root_folder + "/api.json")
    rust_dll_result = generate_rust_dll(apiData)

    structs_map = rust_dll_result[1].copy()
    functions_map = rust_dll_result[2]
    forward_declarations = rust_dll_result[3]

    write_file(rust_dll_result[0], root_folder + "/azul-dll/src/lib.rs")
    write_file(generate_rust_api(apiData, structs_map, functions_map.copy()), root_folder + "/api/rust/lib.rs")
    write_file(generate_c_api(apiData, structs_map), root_folder + "/api/c/azul.h")
    write_file(generate_python_api(apiData, structs_map, functions_map.copy()), root_folder + "/azul-dll/src/python.rs")
    write_file(generate_cpp_api(apiData, structs_map), root_folder + "/api/cpp/azul.hpp")

# Build the library with release settings
def build_dll():

    # Make a copy of the current environment
    # d = dict(os.environ)
    # d['CC'] = 'clang'
    # d['CXX'] = 'clang'
    # cwd = root_folder + "/azul-dll"

    if platform == "linux" or platform == "linux2": # TODO: freebsd?
        #             rustup toolchain install stable-x86_64-unknown-linux-gnu  &&
        #             rustup override set stable-x86_64-unknown-linux-gnu  &&
        os.system("""
            cd azul-dll  &&
            RUSTLFLAGS="-Ctarget-feature=-crt-static" cargo build --lib --target=x86_64-unknown-linux-gnu --all-features --release  &&
            cd ..
        """)
    elif platform == "darwin":
        os.system("""
            cd azul-dll
            rustup toolchain install stable-x86_64-unknown-darwin-gnu
            rustup override set stable-x86_64-unknown-darwin-gnu
            rustup override set stable-x86_64-unknown-darwin-gnu
            RUSTFLAGS="-Ctarget-feature=-crt-static" cargo build --lib --target=x86_64-unknown-darwin-gnu --all-features --release
            cd ..
        """)
    elif platform == "win32":
        os.system("""
            cd azul-dll
            rustup toolchain install stable-x86_64-pc-windows-msvc
            rustup override set stable-x86_64-pc-windows-msvc
            RUSTFLAGS="-Ctarget-feature=-crt-static" cargo build --lib --target=x86_64-pc-windows-msvc --all-features --release
            cd ..
        """)
    else:
        raise Exception("unsupported platform: " + platform)

def run_size_test():
    d = dict(os.environ)   # Make a copy of the current environment
    d['CC'] = 'clang-cl'
    d['CXX'] = 'clang-cl'
    d['RUSTFLAGS'] = '-C target-feature=+crt-static'
    cwd = root_folder + "/azul-dll"
    subprocess.Popen(['cargo', 'test', '--all-features', '--release'], env=d, cwd=cwd).wait()

def build_examples():
    cwd = root_folder + "/examples/rust"
    examples = [
        # "async",
        # "calculator",
        # "components",
        # "game_of_life",
        # "headless",
        # "hello_world",
        # "layout_tests",
        # "list",
        # "opengl",
        "public",
        # "heap_corruption_test",
        # "slider",
        # "svg",
        # "table",
        # "text_input",
    ]
    for e in examples:
        subprocess.Popen(['cargo', 'run', '--release', '--bin', e], cwd=cwd).wait()
    pass

def release_on_cargo():
    # Publish packages in the correct order of dedpendencies
    os.system("cd \"" + root_folder + "/azul-css\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-css-parser\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-core\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-text-layout\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azulc\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-layout\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-desktop\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-web\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-dll\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul\" && cargo check && cargo test && cargo publish")
    os.system("cd \"" + root_folder + "/azul-widgets\" && cargo check && cargo test && cargo publish")

def make_debian_release_package():
    # copy the files such that file is Debian deploy-able
    pass

def make_release_zip_files():
    # Generate the [arch]-*x86_64, [arch]-*i686 etc. ZIP files
    pass

def replace_split(d, search, tag):

    newdoc = ""

    split = d.split(search)

    index = 0
    while index < len(split):
        if index % 2 == 0:
            newdoc += split[index]
        else:
            newdoc += "<" + tag + ">" + split[index] + "</" + tag + ">"
        index += 1

    return newdoc

def format_doc(docstring):
    newdoc = docstring
    newdoc = newdoc.replace("<", "&lt;")
    newdoc = newdoc.replace(">", "&gt;")
    newdoc = newdoc.replace("```rust", "<code>")
    newdoc = newdoc.replace("```", "</code>")
    newdoc = replace_split(newdoc, "`", "code")
    newdoc = replace_split(newdoc, "**", "strong")
    newdoc = newdoc.replace("\r\n", "<br/>")
    return newdoc

def render_example_description(descr, replace=True):
    descr = descr.strip()
    if replace:
        descr = descr.replace("\"", "&quot;")
        descr = descr.replace("\n", "")
        descr = descr.replace("\r\n", "")
        descr = descr.replace("#", "&pound;")
    return descr

def render_example_code(jsex, replace=True):
    jsex = jsex.replace(">", "&gt;")
    jsex = jsex.replace("<", "&lt;")
    if replace:
        # jsex = jsex.replace("#", "%23")
        jsex = jsex.replace("\"", "&quot;")
        jsex = jsex.replace("\n", "<br/>")
        jsex = jsex.replace("\r\n", "<br/>")
        jsex = jsex.replace(" ", "&nbsp;")
    jsex = jsex.strip()
    return jsex

def generate_docs():
    apiData = read_api_file(root_folder + "/api.json")
    html_template = read_file(root_folder + "/api/_patches/html/api.template.html")
    html_template = html_template.replace("$$ROOT_RELATIVE$$", html_root)

    if os.path.exists(root_folder + "/target/html"):
        remove_path(root_folder + "/target/html")

    if not(os.path.exists(root_folder + "/target")):
        create_folder(root_folder + "/target")

    all_versions = list(apiData.keys())
    current_version = all_versions[-1]

    create_folder(root_folder + "/target/html")
    create_folder(root_folder + "/target/html/api")
    create_folder(root_folder + "/target/html/guide")
    for version in all_versions:
        create_folder(root_folder + "/target/html/guide/" + version)
    create_folder(root_folder + "/target/html/release")
    create_folder(root_folder + "/target/html/fonts")
    create_folder(root_folder + "/target/html/images")

    # copy files
    # copy_file(, )
    copy_file(root_folder + "/api/_patches/html/logo.svg", root_folder + "/target/html/logo.svg")
    copy_file(root_folder + "/api/_patches/html/fleur-de-lis.svg", root_folder + "/target/html/images/fleur-de-lis.svg")
    copy_file(root_folder + "/api/_patches/html/main.css", root_folder + "/target/html/main.css")
    copy_file(root_folder + "/examples/assets/fonts/Morris Jenson Initialen.ttf", root_folder + "/target/html/fonts/Morris Jenson Initialen.ttf")
    copy_file(root_folder + "/examples/assets/fonts/SourceSerifPro-Regular.ttf", root_folder + "/target/html/fonts/SourceSerifPro-Regular.ttf")

    index_template = read_file(root_folder + "/api/_patches/html/index.template.html")
    index_template = index_template.replace("$$ROOT_RELATIVE$$", html_root)
    index_examples = [
        {
            "id": "helloworld",
            "description": render_example_description("""
                The UI structure is created via composition instead of inheritance.
                Callbacks can modify the application data and then tell the framework to
                reconstruct the entire UI again - but only if it's necessary, not on every frame.
            """),
            "screenshot_path": root_folder + "/examples/assets/screenshots/helloworld.png",
            "screenshot_url": html_root + "/images/helloworld.png",
            "cpu": "CPU: 0%",
            "memory": "Memory: 23MB",
            "image_alt": "Rendering a simple UI using the Azul GUI toolkit",
            "code:c": render_example_code(read_file(root_folder + "/examples/c/hello-world.c")),
            "code:cpp": render_example_code(read_file(root_folder + "/examples/cpp/hello-world.cpp")),
            "code:rust": render_example_code(read_file(root_folder + "/examples/rust/hello-world.rs")),
            "code:python": render_example_code(read_file(root_folder + "/examples/python/hello-world.py")),
        },
        {
            "id": "table",
            "description": render_example_description("""
                Azul supports lazy loading and can render infinitely large datasets
                (such as a table, shown here) while using a comparably small amount of memory.
                DOM nodes share their CSS style efficiently via pointers,
                so that properties do not get duplicated in memory.
            """),
            "screenshot_path": root_folder + "/examples/assets/screenshots/table.png",
            "screenshot_url": html_root + "/images/table.png",
            "cpu": "CPU: 0%",
            "memory": "Memory: 23MB",
            "image_alt": "Rendering a table using the Azul GUI toolkit",
            "code:c": render_example_code(read_file(root_folder + "/examples/c/table.c")),
            "code:cpp": render_example_code(read_file(root_folder + "/examples/cpp/table.cpp")),
            "code:rust": render_example_code(read_file(root_folder + "/examples/rust/table.rs")),
            "code:python": render_example_code(read_file(root_folder + "/examples/python/table.py")),
        },
        {
            "id": "svg",
            "description": render_example_description("""
                Azul contains a SVG1.1 compatible SVG renderer as well as functions
                for tesselating and drawing shapes to OpenGL textures. Images / textures
                can be composited as clip masks and even be animated.
            """),
            "screenshot_path": root_folder + "/examples/assets/screenshots/svg.png",
            "screenshot_url": html_root + "/images/svg.png",
            "cpu": "CPU: 0%",
            "memory": "Memory: 23MB",
            "image_alt": "Rendering a SVG file using the Azul GUI toolkit",
            "code:c": render_example_code(read_file(root_folder + "/examples/c/svg.c")),
            "code:cpp": render_example_code(read_file(root_folder + "/examples/cpp/svg.cpp")),
            "code:rust": render_example_code(read_file(root_folder + "/examples/rust/svg.rs")),
            "code:python": render_example_code(read_file(root_folder + "/examples/python/svg.py")),
        },
        {
            "id": "calculator",
            "description": render_example_description("""
                Composing larger UIs is just a matter of proper function composition.
                Widget-specific data is either stored on the callback object itself -
                or on the DOM node, similar to a HTML 'dataset' attribute.
            """),
            "screenshot_path": root_folder + "/examples/assets/screenshots/calculator.png",
            "screenshot_url": html_root + "/images/calculator.png",
            "cpu": "CPU: 0%",
            "memory": "Memory: 23MB",
            "image_alt": "Composing widgets via functions in the Azul GUI toolkit",
            "code:c": render_example_code(read_file(root_folder + "/examples/c/calculator.c")),
            "code:cpp": render_example_code(read_file(root_folder + "/examples/cpp/calculator.cpp")),
            "code:rust": render_example_code(read_file(root_folder + "/examples/rust/calculator.rs")),
            "code:python": render_example_code(read_file(root_folder + "/examples/python/calculator.py")),
        },
        {
            "id": "xml",
            "description": render_example_description("""
                Azul contains an XML-based UI description which can be instantly
                hot-reloaded from a file. After prototyping the UI in XML / CSS,
                you can compile the code to a native language in order to get both
                fast design iteration times as well as performant code.
            """),
            "screenshot_path": root_folder + "/examples/assets/screenshots/xml.png",
            "screenshot_url": html_root + "/images/xml.png",
            "cpu": "Memory: 0%",
            "memory": "Memory: 23MB",
            "image_alt": "XML UI hot-reloading for fast prototyping",
            "code:c": render_example_code(read_file(root_folder + "/examples/c/xml.c")),
            "code:cpp": render_example_code(read_file(root_folder + "/examples/cpp/xml.cpp")),
            "code:rust": render_example_code(read_file(root_folder + "/examples/rust/xml.rs")),
            "code:python": render_example_code(read_file(root_folder + "/examples/python/xml.py")),
        }
    ]

    for ex in index_examples:
        copy_file(ex["screenshot_path"], root_folder + "/target/html/images/" + ex["id"] + ".png")

    first_example = index_examples[0]
    index_html = index_template.replace("$$EXAMPLE_CODE$$", first_example["code:python"])
    index_html = index_html.replace("$$EXAMPLE_IMAGE_SOURCE$$", first_example["screenshot_url"])
    index_html = index_html.replace("$$EXAMPLE_IMAGE_ALT$$", first_example["image_alt"])
    index_html = index_html.replace("$$EXAMPLE_STATS_MEMORY$$", first_example["memory"])
    index_html = index_html.replace("$$EXAMPLE_STATS_CPU$$", first_example["cpu"])
    index_html = index_html.replace("$$EXAMPLE_DESCRIPTION$$", first_example["description"])
    index_html = index_html.replace("$$JAVASCRIPT_EXAMPLES$$", json.dumps(index_examples))

    write_file(index_html, root_folder + "/target/html/index.html")

    guide_sidebar = "<ul>"
    guide_sidebar_nested = "<ul>"
    guides_rendered = []
    for entry in sorted(os.scandir(root_folder + "/api/_patches/html/guide"),key=lambda x: x.name):
        if entry.path.endswith(".md") and entry.is_file():
            entry_name = entry.name[3:-3]
            html_path_name = entry_name.replace(" ", "")
            guide_sidebar += "<li><a href=\"" + html_root + "/guide/" + current_version + "/" + html_path_name + "\">" + entry_name + "</a></li>"
            guide_sidebar_nested += "<li><a href=\"./" + html_path_name + "\">" + entry_name + "</a></li>"
            guides_rendered.append(tuple((entry_name, read_file(entry.path))))
    guide_sidebar += "</ul>"
    guide_sidebar_nested += "</ul>"

    guide_combined_page = html_template.replace("$$ROOT_RELATIVE$$", html_root)
    guide_combined_page = guide_combined_page.replace("$$SIDEBAR_GUIDE$$", "")
    guide_combined_page = guide_combined_page.replace("$$SIDEBAR_RELEASES$$", "")
    guide_combined_page = guide_combined_page.replace("$$SIDEBAR_API$$", "")
    guide_combined_page = guide_combined_page.replace("$$TITLE$$", "User guide")
    guide_combined_page = guide_combined_page.replace("$$CONTENT$$", guide_sidebar)
    write_file(guide_combined_page, root_folder + "/target/html/guide.html")

    for guide in guides_rendered:
        entry_name = guide[0]
        html_path_name = entry_name.replace(" ", "")
        guide_content = guide[1]
        guide_content = guide_content.replace("$$ROOT_RELATIVE$$", html_root)
        formatted_guide = html_template.replace("$$ROOT_RELATIVE$$", html_root)
        formatted_guide = formatted_guide.replace("$$SIDEBAR_GUIDE$$", guide_sidebar_nested)
        formatted_guide = formatted_guide.replace("$$SIDEBAR_RELEASES$$", "")
        formatted_guide = formatted_guide.replace("$$SIDEBAR_API$$", "")
        formatted_guide = formatted_guide.replace("$$TITLE$$", entry_name)
        formatted_guide = formatted_guide.replace("$$CONTENT$$", guide_content)
        extra_css = """
        main > div { max-width: 80ch; }
        main > div > p { margin-left: 10px; margin-top: 10px; }
        main p, main a, main strong { font-family: "Source Serif Pro", serif; font-size: 16px; }
        main > div > h3 { margin: 10px; }
        main .warning h4 { margin-bottom: 10px; }
        main .warning {
            padding: 10px;
            border-radius: 5px;
            border: 1px dashed #facb26;
            margin: 10px;
            background: #fff8be;
            color: #222;
            box-shadow: 0px 0px 20px #facb2655;
        }
        main code.expand { display: block; margin-top: 20px; padding: 10px; border-radius: 5px; }
        """
        formatted_guide = formatted_guide.replace("/*$$_EXTRA_CSS$$*/", extra_css)
        write_file(formatted_guide, root_folder + "/target/html/guide/" + current_version + "/" + html_path_name + ".html")

    releases_string = "<ul>"

    for version in all_versions:
        create_folder(root_folder + "/target/html/release/" + version)
        create_folder(root_folder + "/target/html/release/" + version + "/files")

    for version in all_versions:
        releases_string += "<li><a href=\"" + html_root + "/release/" + version + "\">" + version + "</a></li>"

    releases_string += "</ul>"

    releases_combined_page = html_template.replace("$$ROOT_RELATIVE$$", html_root)
    releases_combined_page = releases_combined_page.replace("$$SIDEBAR_GUIDE$$", "")
    releases_combined_page = releases_combined_page.replace("$$SIDEBAR_RELEASES$$", releases_string)
    releases_combined_page = releases_combined_page.replace("$$SIDEBAR_API$$", "")
    releases_combined_page = releases_combined_page.replace("$$TITLE$$", "Choose release version")
    releases_combined_page = releases_combined_page.replace("$$CONTENT$$", releases_string)
    write_file(releases_combined_page, root_folder + "/target/html/releases.html")

    for version in all_versions:
        release_announcement = read_file(root_folder + "/api/_patches/html/release/" + version + ".html")
        release_page = html_template.replace("$$ROOT_RELATIVE$$", html_root)
        release_page = release_page.replace("$$SIDEBAR_GUIDE$$", "")
        release_page = release_page.replace("$$SIDEBAR_RELEASES$$", releases_string)
        release_page = release_page.replace("$$SIDEBAR_API$$", "")
        release_page = release_page.replace("$$TITLE$$", "Release notes - Azul GUI v" + version)
        release_page = release_page.replace("$$CONTENT$$", release_announcement)
        write_file(release_page, root_folder + "/target/html/release/" + version + ".html")

    api_sidebar_string = "<ul>"
    for version in all_versions:
        api_sidebar_string += "<li><a href=\"" + html_root + "/api/" + version + "\">" + version + "</a></li>"
    api_sidebar_string += "</ul>"

    for version in all_versions:

        api_page_contents = ""
        api_page_contents += read_file(root_folder + "/api/_patches/html/api-header.html")
        api_page_contents += "<ul>"

        if "doc" in apiData[version].keys():
            api_page_contents += "<p class=\"version doc\">" + format_doc(apiData[version]["doc"]) + "</p>"

        for module_name in apiData[version].keys():

            api_page_contents += "<li class=\"m\" id=\"m." + module_name + "\">"

            module = apiData[version][module_name]

            if "doc" in module.keys():
                api_page_contents += "<p class=\"m doc\">" + format_doc(module["doc"]) + "</p>"

            api_page_contents += "<h3>mod <a href=\"#m." + module_name + "\">" + module_name + "</a>:</h3>"

            api_page_contents += "<ul>"

            for class_name in module["classes"].keys():
                c = module["classes"][class_name]
                is_boxed_object = "is_boxed_object" in c.keys() and c["is_boxed_object"]
                treat_external_as_ptr = "external" in c.keys() and is_boxed_object
                class_has_custom_destructor = "custom_destructor" in c.keys() and c["custom_destructor"]

                destructor_warning = ""
                if class_has_custom_destructor or treat_external_as_ptr:
                    destructor_warning = "&nbsp;<span class=\"chd\">has destructor</span>"

                if "enum_fields" in c.keys():
                    api_page_contents += "<li class=\"st e pbi\" id=\"st." + class_name + "\">"
                    if "doc" in c.keys():
                        api_page_contents += "<p class=\"class doc\">" + format_doc(c["doc"]) + "</p>"
                    enum_type = "enum"
                    if enum_is_union(c["enum_fields"]):
                        enum_type = "union enum"

                    api_page_contents += "<h4>" + enum_type + " <a href=\"#st." + class_name + "\">" + class_name + "</a>" + destructor_warning + "</h4>"
                    for enum_variant in c["enum_fields"]:
                        enum_variant_name = list(enum_variant.keys())[0]
                        if "doc" in enum_variant[enum_variant_name]:
                            api_page_contents += "<p class=\"v doc\">" + format_doc(enum_variant[enum_variant_name]["doc"]) + "</p>"

                        if "type" in enum_variant[enum_variant_name]:
                            enum_variant_type = enum_variant[enum_variant_name]["type"]
                            analyzed_variant_type = analyze_type(enum_variant_type)

                            if is_primitive_arg(analyzed_variant_type[1]):
                                api_page_contents += "<p class=\"f\">" + enum_variant_name + "(" + enum_variant_type + ")</p>"
                            else:
                                api_page_contents += "<p class=\"f\">" + enum_variant_name + "(" + analyzed_variant_type[0] + "<a href=\"#st." + analyzed_variant_type[1] + "\">" + analyzed_variant_type[1] +"</a>" + analyzed_variant_type[2] + ")</p>"
                        else:
                            api_page_contents += "<p class=\"f\">" + enum_variant_name + "</p>"

                elif "struct_fields" in c.keys():
                    api_page_contents += "<li class=\"st s pbi\" id=\"st." + class_name + "\">"
                    if "doc" in c.keys():
                        api_page_contents += "<p class=\"class doc\">" + format_doc(c["doc"]) + "</p>"
                    api_page_contents += "<h4>struct <a href=\"#st." + class_name + "\">" + class_name + "</a>" + destructor_warning + "</h4>"
                    for struct_field in c["struct_fields"]:
                        struct_field_name = list(struct_field.keys())[0]
                        struct_type = struct_field[struct_field_name]["type"]
                        analyzed_struct_type = analyze_type(struct_type)

                        if "doc" in struct_field[struct_field_name]:
                            api_page_contents += "<p class=\"f doc\">" + format_doc(struct_field[struct_field_name]["doc"]) + "</p>"

                        if is_primitive_arg(analyzed_struct_type[1]):
                            api_page_contents += "<p class=\"f\">" + struct_field_name + ": " + struct_type + "</p>"
                        else:
                            api_page_contents += "<p class=\"f\">" + struct_field_name + ": " + analyzed_struct_type[0] + "<a href=\"#st." + analyzed_struct_type[1] + "\">" + analyzed_struct_type[1] +"</a>" + analyzed_struct_type[2] + "</p>"

                elif "callback_typedef" in c.keys():
                    api_page_contents += "<li class=\"pbi fnty\" id=\"st." + class_name + "\">"
                    if "doc" in c.keys():
                        api_page_contents += "<p class=\"class doc\">" + format_doc(c["doc"]) + "</p>"
                    api_page_contents += "<h4>fnptr <a href=\"#fnty." + class_name + "\">" + class_name + "</a></h4>"
                    callback_typedef = c["callback_typedef"]

                    if "fn_args" in callback_typedef:
                        api_page_contents += "<ul>"
                        for fn_arg in callback_typedef["fn_args"]:

                            if "doc" in fn_arg.keys():
                                api_page_contents += "<p class=\"arg doc\">" + format_doc(fn_arg["doc"]) + "</p>"

                            fn_arg_type = fn_arg["type"]
                            analyzed_fn_arg_type = analyze_type(fn_arg_type)
                            fn_arg_ref = fn_arg["ref"]

                            fn_arg_ref_html = ""
                            if (fn_arg_ref == "value"):
                                fn_arg_ref_html = ""
                            elif (fn_arg_ref == "ref"):
                                fn_arg_ref_html = "&"
                            elif (fn_arg_ref == "refmut"):
                                fn_arg_ref_html = "&mut "

                            if is_primitive_arg(analyzed_fn_arg_type[1]):
                                api_page_contents += "<li><p class=\"f\">arg " + analyzed_fn_arg_type[1] + "</p></li>"
                            else:
                                api_page_contents += "<li><p class=\"fnty arg\">arg " + fn_arg_ref_html + " <a href=\"#st." + analyzed_fn_arg_type[1] + "\">" + fn_arg_type + "</a></p></li>"
                        api_page_contents += "</ul>"

                    if "returns" in callback_typedef.keys():
                        if "doc" in callback_typedef["returns"].keys():
                            api_page_contents += "<p class=\"ret doc\">" + format_doc(callback_typedef["returns"]["doc"]) + "</p>"
                        return_type = callback_typedef["returns"]["type"]
                        analyzed_return_type = analyze_type(return_type)
                        if is_primitive_arg(analyzed_fn_arg_type[1]):
                            api_page_contents += "<p class=\"fnty ret\">-&gt;&nbsp;" + analyzed_return_type[1] + "</p>"
                        else:
                            api_page_contents += "<p class=\"fnty ret\">-&gt;&nbsp;<a href=\"#st." + analyzed_return_type[1] + "\">" + analyzed_return_type[1] + "</a></p>"

                if "constructors" in c.keys():
                    api_page_contents += "<ul>"
                    for function_name in c["constructors"]:
                        f = c["constructors"][function_name]
                        if "doc" in f:
                            api_page_contents += "<p class=\"cn doc\">" + format_doc(c["constructors"][function_name]["doc"]) + "</p>"
                        arg_string = ""
                        if "fn_args" in f:
                            args = f["fn_args"]
                            for arg in args:
                                arg_name = list(arg.keys())[0]
                                arg_val = arg[arg_name]
                                if "doc" in arg.keys():
                                    arg_string += "<p class=\"arg doc\">" + arg["doc"] + "</p>"

                                analyzed_arg_val = analyze_type(arg_val)
                                if is_primitive_arg(analyzed_arg_val[1]):
                                    arg_string += "<li><p class=\"arg\">arg " + arg_name + ": " + analyzed_arg_val[1] + "</p></li>"
                                else:
                                    arg_string += "<li><p class=\"arg\">arg " + arg_name + ": " + analyzed_arg_val[0] + "<a href=\"#st." + analyzed_arg_val[1] + "\">" + analyzed_arg_val[1] + "</a>" + analyzed_arg_val[2] + "</p></li>"

                        api_page_contents += "<li class=\"cn\" id=\"" + class_name + "." + function_name + "\">"
                        api_page_contents += "<p>constructor <a href=\"#" + class_name + "." + function_name + "\">" + function_name + "</a>:</p>"
                        api_page_contents += "<ul>"
                        if not(len(arg_string) == 0):
                            api_page_contents += arg_string
                        if "returns" in f.keys():
                            api_page_contents += "<li>"
                            if "doc" in f["returns"].keys():
                                api_page_contents += "<p class=\"ret doc\">" + format_doc(f["returns"]["doc"]) + "</p>"
                            return_type = f["returns"]["type"]
                            analyzed_return_type = analyze_type(return_type)
                            if is_primitive_arg(analyzed_return_type[1]):
                                api_page_contents += "<p class=\"cn ret\">-&gt;&nbsp;" + analyzed_return_type[1] + "</p>"
                            else:
                                api_page_contents += "<p class=\"cn ret\">-&gt;&nbsp;" + analyzed_return_type[0] + "<a href=\"#st."+ analyzed_return_type[1] + "\">" + analyzed_return_type[1] + "</a>" + analyzed_return_type[2] + "</p>"
                            api_page_contents += "</li>"

                        api_page_contents += "<li><p class=\"ret\">-&gt;&nbsp;<a href=\"#st." + class_name + "\">" + class_name + "</a></p></li>"
                        api_page_contents += "</ul>"
                        api_page_contents += "</li>"

                    api_page_contents += "</ul>"

                if "functions" in c.keys():
                    api_page_contents += "<ul>"
                    for function_name in c["functions"]:
                        f = c["functions"][function_name]
                        if "doc" in f:
                            api_page_contents += "<p class=\"fn doc\">" + format_doc(c["functions"][function_name]["doc"]) + "</p>"
                        arg_string = ""
                        self_arg = ""
                        if "fn_args" in f:
                            args = f["fn_args"]
                            for arg in args:
                                arg_name = list(arg.keys())[0]
                                arg_val = arg[arg_name]

                                if arg_name == "self":
                                    if arg_val == "value":
                                        self_arg = "self"
                                    elif arg_val == "ref":
                                        self_arg = "&self"
                                    elif arg_val == "refmut":
                                        self_arg = "&mut self"
                                else:
                                    if "doc" in arg.keys():
                                        arg_string += "<p class=\"arg doc\">" + arg["doc"] + "</p>"

                                    analyzed_arg_val = analyze_type(arg_val)
                                    if is_primitive_arg(analyzed_arg_val[1]):
                                        arg_string += "<li><p class=\"arg\">arg " + arg_name + ": " + analyzed_arg_val[1] + "</p></li>"
                                    else:
                                        arg_string += "<li><p class=\"arg\">arg " + arg_name + ": " + analyzed_arg_val[0] + "<a href=\"#st." + analyzed_arg_val[1] + "\">" + analyzed_arg_val[1] + "</a>" + analyzed_arg_val[2] + "</p></li>"

                        api_page_contents += "<li class=\"fn\" id=\"" + class_name + "." + function_name + "\">"
                        api_page_contents += "<p>fn <a href=\"#" + class_name + "." + function_name + "\">" + function_name + "</a>:</p>"
                        api_page_contents += "<ul>"
                        api_page_contents += "<li><p class=\"arg\">" + self_arg + "</p></li>"
                        if not(len(arg_string) == 0):
                            api_page_contents += arg_string
                        if "returns" in f.keys():
                            api_page_contents += "<li>"
                            if "doc" in f["returns"].keys():
                                api_page_contents += "<p class=\"ret doc\">" + format_doc(f["returns"]["doc"]) + "</p>"
                            return_type = f["returns"]["type"]
                            analyzed_return_type = analyze_type(return_type)
                            if is_primitive_arg(analyzed_return_type[1]):
                                api_page_contents += "<p class=\"fn ret\">-&gt;&nbsp;" + analyzed_return_type[1] + "</p>"
                            else:
                                api_page_contents += "<p class=\"fn ret\">-&gt;&nbsp;" + analyzed_return_type[0] + "<a href=\"#st."+ analyzed_return_type[1] + "\">" + analyzed_return_type[1] + "</a>" + analyzed_return_type[2] + "</p>"
                            api_page_contents += "</li>"

                        api_page_contents += "</ul>"
                        api_page_contents += "</li>"

                    api_page_contents += "</ul>"

                api_page_contents += "</li>"

            api_page_contents += "</ul>"

            api_page_contents += "</li>"

        api_page_contents += "</ul>"

        releases_string = "<ul>"
        for version in all_versions:
            releases_string += "<li><a href=\"./" + version + "\">" + version + "</a></li>"
        releases_string += "</ul>"

        final_html = html_template.replace("$$ROOT_RELATIVE$$", html_root)
        extra_css = "\
        body > .center > main > div > ul * { font-size: 12px; font-weight: normal; list-style-type: none; font-family: monospace; }\
        body > .center > main > div > ul > li ul { margin-left: 20px; }\
        body > .center > main > div > ul > li.m { margin-top: 40px; margin-bottom: 20px; }\
        body > .center > main > div > ul > li.m > ul > li { margin-bottom: 15px; }\
        body > .center > main > div > ul > li.m > ul > li.st.e { color: #2b6a2d; }\
        body > .center > main > div > ul > li.m > ul > li.st.s { color: #905; }\
        body > .center > main > div > ul > li.m > ul > li.fnty,\
        body > .center > main > div > ul > li.m > ul > li .arg { color: #4c1c1a; }\
        body > .center > main > div > ul > li.m > ul > li.st .f { margin-left: 20px; }\
        body > .center > main > div > ul > li.m > ul > li.st .v.doc { margin-left: 20px; }\
        body > .center > main > div > ul > li.m > ul > li.st .cn { margin-left: 20px; color: #07a; }\
        body > .center > main > div > ul > li.m > ul > li.st .fn { margin-left: 20px; color: #004e92; }\
        body > .center > main > div > ul > li.m > ul > li p.ret,\
        body > .center > main > div > ul > li.m > ul > li p.fn.ret,\
        body > .center > main > div > ul > li.m > ul > li p.ret.doc { margin-left: 0px; }\
        body > .center > main > div p.doc { margin-top: 5px !important; color: black !important; max-width: 70ch !important; font-weight: bolder; }\
        body > .center > main > div a { color: inherit !important; }\
        "
        final_html = final_html.replace("/*$$_EXTRA_CSS$$*/", extra_css)
        final_html = final_html.replace("$$SIDEBAR_RELEASES$$", "")
        final_html = final_html.replace("$$SIDEBAR_GUIDE$$", "")
        final_html = final_html.replace("$$SIDEBAR_API$$", api_sidebar_string)
        final_html = final_html.replace("$$TITLE$$", "v" + version)
        final_html = final_html.replace("$$CONTENT$$", api_page_contents)
        write_file(final_html, root_folder + "/target/html/api/" + version + ".html")

    api_combined_page = html_template.replace("$$ROOT_RELATIVE$$", html_root)
    api_combined_page = api_combined_page.replace("$$SIDEBAR_GUIDE$$", "")
    api_combined_page = api_combined_page.replace("$$SIDEBAR_RELEASES$$", "")
    api_combined_page = api_combined_page.replace("$$SIDEBAR_API$$", api_sidebar_string)
    api_combined_page = api_combined_page.replace("$$TITLE$$", "Choose API version")
    api_combined_page = api_combined_page.replace("$$CONTENT$$", api_sidebar_string)
    write_file(api_combined_page, root_folder + "/target/html/api.html")

def build_azulc():
    # enable features="image_loading, font_loading" to enable layouting
    os.system('cd "' + root_folder + '/azulc" && cargo build --bin azulc --no-default-features --features="xml std font_loading image_loading text_layout" --release')

def full_test():
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --verbose --all-features')
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --verbose --examples')
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --no-default-features')
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --verbose --release --all-features')
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --no-default-features --features="svg"')
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --no-default-features --features="image_loading"')
    os.system('cd "' + root_folder + '/azul-dll" && cargo check --no-default-features --features="font_loading"')
    os.system('cd "' + root_folder + '/azul-dll" && cargo test --verbose --all-features')
    os.system('cd "' + root_folder + "/examples && cargo run --bin layout_tests -- --nocapture")

def debug_test_compile_c():
    if platform == "linux" or platform == "linux2":
        os.system('cd "' + root_folder + '/api/c" && gcc -ansi ./main.c')
    elif platform == "win32":
        os.system("cd \"" + root_folder + "/api/c\" && clang -ansi ./main.c -lazul -I\"" + root_folder + "/target/release/x86_64-unknown-windows-msvc/release\" ")
    else:
        pass

def main():
    print("removing old azul.dll...")
    # cleanup_start()
    # print("verifying that LLVM / clang-cl is installed...")
    # verify_clang_is_installed()
    print("generating API...")
    generate_api()
    print("generating documentation in /target/html...")
    generate_docs()
    print("building azulc (release mode)...")
    # build_azulc()
    print("building azul-dll (release mode)...")
    # build_dll()
    print("checking azul-dll for struct size integrity...")
    # run_size_test()
    print("building examples...")
    # build_examples()
    print("building and linking C examples from /examples/c/...")
    # debug_test_compile_c()
    # full_test()
    # release_on_cargo()
    # make_debian_release_package()
    # make_release_zip_files()
    # travis: github release - copy azul.zip!

if __name__ == "__main__":
    main()