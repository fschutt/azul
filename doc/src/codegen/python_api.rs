use std::collections::{BTreeMap, HashMap};

use indexmap::IndexMap;

use crate::{
    api::{ApiData, ClassData},
    utils::{
        analyze::{
            analyze_type, class_is_small_enum, class_is_small_struct, class_is_stack_allocated,
            class_is_typedef, enum_is_union, get_class, has_recursive_destructor, is_primitive_arg,
            search_for_class_by_class_name,
        },
        string::snake_case_to_lower_camel,
    },
};

const PREFIX: &str = "Az";

/// Generate Python API code from API data
pub fn generate_python_api(api_data: &ApiData, version: &str) -> String {
    let mut code = String::new();

    // Get the latest version
    let version_data = api_data.get_version(version).unwrap();

    // Add header comments and imports
    code.push_str("#![allow(non_snake_case)]\r\n");
    code.push_str("\r\n");
    code.push_str(include_str!("./dll-patch/header.rs"));
    code.push_str("\r\n");
    code.push_str("use core::mem;\r\n");
    code.push_str("use pyo3::prelude::*;\r\n");
    code.push_str("use pyo3::PyObjectProtocol;\r\n");
    code.push_str("use pyo3::types::*;\r\n");
    code.push_str("use pyo3::exceptions::PyException;\r\n");

    // Define GL types as they're used in the API
    code.push_str("\r\n");
    code.push_str("// GL type definitions\r\n");
    code.push_str("type GLuint = u32; type AzGLuint = GLuint;\r\n");
    code.push_str("type GLint = i32; type AzGLint = GLint;\r\n");
    code.push_str("type GLint64 = i64; type AzGLint64 = GLint64;\r\n");
    code.push_str("type GLuint64 = u64; type AzGLuint64 = GLuint64;\r\n");
    code.push_str("type GLenum = u32; type AzGLenum = GLenum;\r\n");
    code.push_str("type GLintptr = isize; type AzGLintptr = GLintptr;\r\n");
    code.push_str("type GLboolean = u8; type AzGLboolean = GLboolean;\r\n");
    code.push_str("type GLsizeiptr = isize; type AzGLsizeiptr = GLsizeiptr;\r\n");
    code.push_str("type GLvoid = c_void; type AzGLvoid = GLvoid;\r\n");
    code.push_str("type GLbitfield = u32; type AzGLbitfield = GLbitfield;\r\n");
    code.push_str("type GLsizei = i32; type AzGLsizei = GLsizei;\r\n");
    code.push_str("type GLclampf = f32; type AzGLclampf = GLclampf;\r\n");
    code.push_str("type GLfloat = f32; type AzGLfloat = GLfloat;\r\n");
    code.push_str("type AzF32 = f32;\r\n");
    code.push_str("type AzU16 = u16;\r\n");
    code.push_str("type AzU32 = u32;\r\n");
    code.push_str("type AzScanCode = u32;\r\n");

    code.push_str("\r\n");
    code.push_str(include_str!("./python-patch/api.rs"));
    code.push_str("\r\n");

    // Collect struct and enum mappings
    let (struct_map, enum_map) = collect_structs_and_enums(api_data);

    // Generate structure definitions
    for (struct_name, class_data) in &struct_map {
        // Add class documentation
        if let Some(doc) = &class_data.doc {
            code.push_str(&format!("/// {}\r\n", doc));
        } else {
            code.push_str(&format!("/// `{}` struct\r\n", struct_name));
        }

        // Generate PyClass attribute
        code.push_str(&format!(
            "#[pyclass(name = \"{}\")]\r\n",
            struct_name.strip_prefix(PREFIX).unwrap_or(struct_name)
        ));

        // Start struct definition
        code.push_str(&format!("pub struct {} {{\r\n", struct_name));

        // Add fields
        if let Some(struct_fields) = &class_data.struct_fields {
            for field_map in struct_fields {
                for (field_name, field_data) in field_map {
                    if field_name == "ptr" && field_data.r#type.contains("*") {
                        // Don't expose raw pointers to Python
                        code.push_str(&format!(
                            "    pub(crate) {}: {}, // raw pointer not exposed to Python\r\n",
                            field_name, field_data.r#type
                        ));
                    } else if field_name != "cb" {
                        // Don't expose callback fields
                        code.push_str(&format!("    #[pyo3(get, set)]\r\n"));
                        code.push_str(&format!(
                            "    pub {}: {}, \r\n",
                            field_name, field_data.r#type
                        ));
                    }
                }
            }
        }

        // End struct definition
        code.push_str("}\r\n\r\n");
    }

    // Generate enum wrapper definitions
    for (enum_name, class_data) in &enum_map {
        // Add class documentation
        if let Some(doc) = &class_data.doc {
            code.push_str(&format!("/// {}\r\n", doc));
        } else {
            code.push_str(&format!("/// `{}` enum\r\n", enum_name));
        }

        // Generate PyClass attribute for wrapper
        let wrapper_name = format!("{}EnumWrapper", enum_name);
        code.push_str(&format!(
            "#[pyclass(name = \"{}\")]\r\n",
            enum_name.strip_prefix(PREFIX).unwrap_or(enum_name)
        ));

        // Define enum wrapper as transparent struct containing the inner enum
        code.push_str(&format!("#[repr(transparent)]\r\n"));
        code.push_str(&format!("pub struct {} {{\r\n", wrapper_name));
        code.push_str(&format!("    pub inner: {},\r\n", enum_name));
        code.push_str("}\r\n\r\n");
    }

    // Add marker for Send implementations
    code.push_str("\r\n");
    code.push_str(
        "// Necessary because the Python interpreter may send structs across different threads\r\n",
    );

    // Implement Send for all structs with raw pointers
    let raw_pointer_structs: Vec<&String> = struct_map
        .keys()
        .filter(|name| {
            if let Some(class_data) = struct_map.get(*name) {
                if let Some(struct_fields) = &class_data.struct_fields {
                    for field_map in struct_fields {
                        for (_, field_data) in field_map {
                            if field_data.r#type.contains('*') {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        })
        .collect();

    for struct_name in raw_pointer_structs {
        code.push_str(&format!("unsafe impl Send for {} {{ }}\r\n", struct_name));
    }

    code.push_str("\r\n");

    // Add Clone implementations for all cloneable structs
    code.push_str("\r\n");
    code.push_str("// Python objects must implement Clone at minimum\r\n");

    for (struct_name, class_data) in &struct_map {
        let clone_class = class_data.clone.unwrap_or(true);
        if !clone_class {
            continue;
        }

        if class_data.external.is_some() {
            code.push_str(&format!(
                "impl Clone for {} {{ fn clone(&self) -> Self {{ let r: &{} = unsafe {{ \
                 mem::transmute(self) }}; unsafe {{ mem::transmute(r.clone()) }} }} }}\r\n",
                struct_name,
                class_data.external.as_ref().unwrap()
            ));
        }
    }

    for (enum_name, class_data) in &enum_map {
        let clone_class = class_data.clone.unwrap_or(true);
        if !clone_class {
            continue;
        }

        if class_data.external.is_some() {
            code.push_str(&format!(
                "impl Clone for {}EnumWrapper {{ fn clone(&self) -> Self {{ let r: &{} = unsafe \
                 {{ mem::transmute(&self.inner) }}; unsafe {{ mem::transmute(r.clone()) }} }} \
                 }}\r\n",
                enum_name,
                class_data.external.as_ref().unwrap()
            ));
        }
    }

    // Add Drop implementations
    code.push_str("\r\n");
    code.push_str("// Implement Drop for all objects with drop constructors\r\n");

    for (struct_name, class_data) in &struct_map {
        let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
        let is_boxed_object = class_data.is_boxed_object;
        let should_impl_drop = class_has_custom_destructor || is_boxed_object;

        if should_impl_drop {
            code.push_str(&format!(
                "impl Drop for {} {{ fn drop(&mut self) {{ crate::{}_delete(unsafe {{ \
                 mem::transmute(self) }}); }} }}\r\n",
                struct_name, struct_name
            ));
        }
    }

    for (enum_name, class_data) in &enum_map {
        let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
        let is_boxed_object = class_data.is_boxed_object;
        let should_impl_drop = class_has_custom_destructor || is_boxed_object;

        if should_impl_drop {
            code.push_str(&format!(
                "impl Drop for {}EnumWrapper {{ fn drop(&mut self) {{ crate::{}_delete(unsafe {{ \
                 mem::transmute(self) }}); }} }}\r\n",
                enum_name, enum_name
            ));
        }
    }

    // Generate Python methods
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_name_with_prefix = format!("{}{}", PREFIX, class_name);

            // Skip callback typedefs
            if class_data.callback_typedef.is_some() {
                continue;
            }

            // Generate methods for structs
            if class_data.struct_fields.is_some() {
                code.push_str("\r\n");
                code.push_str(&format!("#[pymethods]\r\n"));
                code.push_str(&format!("impl {} {{\r\n", class_name_with_prefix));

                // Add constants as class attributes
                if let Some(constants) = &class_data.constants {
                    for constant_map in constants {
                        for (constant_name, constant_data) in constant_map {
                            code.push_str(&format!("    #[classattr]\r\n"));
                            code.push_str(&format!(
                                "    const {}: {} = {};\r\n",
                                constant_name, constant_data.r#type, constant_data.value
                            ));
                        }
                    }
                    code.push_str("\r\n");
                }

                // Add constructors
                if let Some(constructors) = &class_data.constructors {
                    for (constructor_name, constructor) in constructors {
                        // Skip complex constructors
                        if constructor.fn_args.iter().any(|arg| {
                            arg.iter()
                                .any(|(_, arg_type)| arg_type == "RefAny" || arg_type.contains('*'))
                        }) {
                            continue;
                        }

                        // Use the #[new] attribute for the default constructor
                        if constructor_name == "new" && constructor.returns.is_none() {
                            code.push_str("    #[new]\r\n");
                        } else {
                            code.push_str("    #[staticmethod]\r\n");
                        }

                        // Generate simplified Python constructor
                        code.push_str(&format!(
                            "    fn {}(/* args */) -> {} {{\r\n",
                            constructor_name, class_name_with_prefix
                        ));
                        code.push_str(
                            "        // Implementation would convert Python args to Rust types\r\n",
                        );
                        code.push_str("        // and call the appropriate C function\r\n");
                        code.push_str("        unimplemented!()\r\n");
                        code.push_str("    }\r\n\r\n");
                    }
                }

                // Add methods
                if let Some(functions) = &class_data.functions {
                    for (function_name, function) in functions {
                        // Skip complex functions
                        if function.fn_args.iter().any(|arg| {
                            arg.iter().any(|(name, arg_type)| {
                                name != "self" && (arg_type == "RefAny" || arg_type.contains('*'))
                            })
                        }) {
                            continue;
                        }

                        // Generate simplified Python method
                        code.push_str(&format!(
                            "    fn {}(&self/* , args */) -> PyResult<()> {{\r\n",
                            function_name
                        ));
                        code.push_str(
                            "        // Implementation would convert Python args to Rust \
                             types,\r\n",
                        );
                        code.push_str(
                            "        // call the appropriate C function, and convert results \
                             back\r\n",
                        );
                        code.push_str("        Ok(())\r\n");
                        code.push_str("    }\r\n\r\n");
                    }
                }

                // Close the impl block
                code.push_str("}\r\n");

                // Add string representation
                code.push_str("\r\n");
                code.push_str("#[pyproto]\r\n");
                code.push_str(&format!(
                    "impl PyObjectProtocol for {} {{\r\n",
                    class_name_with_prefix
                ));
                code.push_str("    fn __str__(&self) -> Result<String, PyErr> { \r\n");
                if let Some(external) = &class_data.external {
                    code.push_str(&format!(
                        "        let m: &{} = unsafe {{ mem::transmute(self) }}; \
                         Ok(format!(\"{{:#?}}\", m))\r\n",
                        external
                    ));
                } else {
                    code.push_str("        Ok(format!(\"{}{{:?}}\", self))\r\n");
                }
                code.push_str("    }\r\n");
                code.push_str("    fn __repr__(&self) -> Result<String, PyErr> { \r\n");
                if let Some(external) = &class_data.external {
                    code.push_str(&format!(
                        "        let m: &{} = unsafe {{ mem::transmute(self) }}; \
                         Ok(format!(\"{{:#?}}\", m))\r\n",
                        external
                    ));
                } else {
                    code.push_str("        Ok(format!(\"{}{{:?}}\", self))\r\n");
                }
                code.push_str("    }\r\n");
                code.push_str("}\r\n");
            }
            // Generate methods for enums
            else if let Some(enum_fields) = &class_data.enum_fields {
                let wrapper_name = format!("{}EnumWrapper", class_name_with_prefix);

                code.push_str("\r\n");
                code.push_str(&format!("#[pymethods]\r\n"));
                code.push_str(&format!("impl {} {{\r\n", wrapper_name));

                // Add constants as class attributes
                if let Some(constants) = &class_data.constants {
                    for constant_map in constants {
                        for (constant_name, constant_data) in constant_map {
                            code.push_str(&format!("    #[classattr]\r\n"));
                            code.push_str(&format!(
                                "    const {}: {} = {};\r\n",
                                constant_name, constant_data.r#type, constant_data.value
                            ));
                        }
                    }
                    code.push_str("\r\n");
                }

                // Generate static constructor methods for each variant
                let is_union = enum_is_union(enum_fields);

                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        if is_union {
                            // For tagged unions, generate constructor methods
                            if let Some(variant_type) = &variant_data.r#type {
                                let (prefix, base_type, suffix) = analyze_type(variant_type);

                                code.push_str("    #[staticmethod]\r\n");

                                if is_primitive_arg(&base_type) {
                                    code.push_str(&format!(
                                        "    fn {}(v: {}) -> {} {{\r\n",
                                        variant_name, variant_type, wrapper_name
                                    ));
                                } else if let Some((_, type_class_name)) =
                                    search_for_class_by_class_name(version_data, &base_type)
                                {
                                    code.push_str(&format!(
                                        "    fn {}(v: {}{}) -> {} {{\r\n",
                                        variant_name, PREFIX, type_class_name, wrapper_name
                                    ));
                                } else {
                                    continue; // Skip if type not found
                                }

                                code.push_str(&format!(
                                    "        {} {{ inner: {}::{}",
                                    wrapper_name, class_name_with_prefix, variant_name
                                ));
                                code.push_str("(v) }}\r\n");
                                code.push_str("    }\r\n");
                            } else {
                                // Unit variant
                                code.push_str("    #[classattr]\r\n");
                                code.push_str(&format!(
                                    "    fn {}() -> {} {{\r\n",
                                    variant_name, wrapper_name
                                ));
                                code.push_str(&format!(
                                    "        {} {{ inner: {}::{} }}\r\n",
                                    wrapper_name, class_name_with_prefix, variant_name
                                ));
                                code.push_str("    }\r\n");
                            }
                        } else {
                            // For simple enums, just add class attributes
                            code.push_str("    #[classattr]\r\n");
                            code.push_str(&format!(
                                "    const {}: {} = {}::{};\r\n",
                                variant_name,
                                class_name_with_prefix,
                                class_name_with_prefix,
                                variant_name
                            ));
                        }
                    }
                }

                // For tagged unions, add a match method
                if is_union {
                    code.push_str("\r\n");
                    code.push_str("    fn r#match(&self) -> PyResult<Vec<PyObject>> {\r\n");
                    code.push_str(&format!(
                        "        use crate::python::{};\r\n",
                        class_name_with_prefix
                    ));
                    code.push_str("        use pyo3::conversion::IntoPy;\r\n");
                    code.push_str("        let gil = Python::acquire_gil();\r\n");
                    code.push_str("        let py = gil.python();\r\n");
                    code.push_str("        match &self.inner {\r\n");

                    for variant_map in enum_fields {
                        for (variant_name, variant_data) in variant_map {
                            let opt_variant_type_match = if variant_data.r#type.is_some() {
                                "(v)"
                            } else {
                                ""
                            };

                            let opt_variant_value = if let Some(variant_type) = &variant_data.r#type
                            {
                                if variant_type == "*const c_void" || variant_type == "*mut c_void"
                                {
                                    "()"
                                } else {
                                    let (_, base_type, _) = analyze_type(variant_type);

                                    if is_primitive_arg(&base_type) {
                                        "v"
                                    } else if let Some((_, type_class_name)) =
                                        search_for_class_by_class_name(version_data, &base_type)
                                    {
                                        if let Some(found_class) =
                                            get_class(version_data, &type_class_name, &base_type)
                                        {
                                            if found_class.callback_typedef.is_some() {
                                                "()" // Function pointer
                                            } else if found_class.enum_fields.is_some() {
                                                "{ let m: &AzEnumWrapper = unsafe { \
                                                 mem::transmute(v) }; m.clone() }"
                                            } else {
                                                "v.clone()"
                                            }
                                        } else {
                                            "v"
                                        }
                                    } else {
                                        "v"
                                    }
                                }
                            } else {
                                "()"
                            };

                            code.push_str(&format!(
                                "            {}::{}{} => Ok(vec![\"{}\".into_py(py), \
                                 {}.into_py(py)]),\r\n",
                                class_name_with_prefix,
                                variant_name,
                                opt_variant_type_match,
                                variant_name,
                                opt_variant_value
                            ));
                        }
                    }

                    code.push_str("        }\r\n");
                    code.push_str("    }\r\n");
                }

                // Close the impl block
                code.push_str("}\r\n");

                // Add string representation
                code.push_str("\r\n");
                code.push_str("#[pyproto]\r\n");
                code.push_str(&format!(
                    "impl PyObjectProtocol for {} {{\r\n",
                    wrapper_name
                ));
                code.push_str("    fn __str__(&self) -> Result<String, PyErr> { \r\n");
                if let Some(external) = &class_data.external {
                    code.push_str(&format!(
                        "        let m: &{} = unsafe {{ mem::transmute(&self.inner) }}; \
                         Ok(format!(\"{{:#?}}\", m))\r\n",
                        external
                    ));
                } else {
                    code.push_str("        Ok(format!(\"{}{{:?}}\", self.inner))\r\n");
                }
                code.push_str("    }\r\n");
                code.push_str("    fn __repr__(&self) -> Result<String, PyErr> { \r\n");
                if let Some(external) = &class_data.external {
                    code.push_str(&format!(
                        "        let m: &{} = unsafe {{ mem::transmute(&self.inner) }}; \
                         Ok(format!(\"{{:#?}}\", m))\r\n",
                        external
                    ));
                } else {
                    code.push_str("        Ok(format!(\"{}{{:?}}\", self.inner))\r\n");
                }
                code.push_str("    }\r\n");

                // Add comparison operators for simple enums
                if !is_union {
                    code.push_str("    fn __richcmp__(&self, other: ");
                    code.push_str(&wrapper_name);
                    code.push_str(", op: pyo3::class::basic::CompareOp) -> PyResult<bool> {\r\n");
                    code.push_str("        match op {\r\n");
                    code.push_str(
                        "            pyo3::class::basic::CompareOp::Lt => { \
                         Ok((self.clone().inner as usize) <  (other.clone().inner as usize)) }\r\n",
                    );
                    code.push_str(
                        "            pyo3::class::basic::CompareOp::Le => { \
                         Ok((self.clone().inner as usize) <= (other.clone().inner as usize)) }\r\n",
                    );
                    code.push_str(
                        "            pyo3::class::basic::CompareOp::Eq => { \
                         Ok((self.clone().inner as usize) == (other.clone().inner as usize)) }\r\n",
                    );
                    code.push_str(
                        "            pyo3::class::basic::CompareOp::Ne => { \
                         Ok((self.clone().inner as usize) != (other.clone().inner as usize)) }\r\n",
                    );
                    code.push_str(
                        "            pyo3::class::basic::CompareOp::Gt => { \
                         Ok((self.clone().inner as usize) >  (other.clone().inner as usize)) }\r\n",
                    );
                    code.push_str(
                        "            pyo3::class::basic::CompareOp::Ge => { \
                         Ok((self.clone().inner as usize) >= (other.clone().inner as usize)) }\r\n",
                    );
                    code.push_str("        }\r\n");
                    code.push_str("    }\r\n");
                }

                code.push_str("}\r\n");
            }
        }
    }

    // Generate Error implementations
    code.push_str("\r\n");
    code.push_str("// Error type conversions\r\n");

    // Placeholder for error type implementations
    code.push_str("// Error type implementations would go here\r\n");

    // Generate the Python module
    code.push_str("\r\n");
    code.push_str("#[pymodule]\r\n");
    code.push_str("fn azul(py: Python, m: &PyModule) -> PyResult<()> {\r\n");
    code.push_str("\r\n");

    // Set up logging
    code.push_str(
        "    #[cfg(all(feature = \"use_pyo3_logger\", not(feature = \"use_fern_logger\")))] {\r\n",
    );
    code.push_str("        let mut filter = log::LevelFilter::Warn;\r\n");
    code.push_str("\r\n");
    code.push_str(
        "        if std::env::var(\"AZUL_PY_LOGLEVEL_ERROR\").is_ok() { filter = \
         log::LevelFilter::Error; }\r\n",
    );
    code.push_str(
        "        if std::env::var(\"AZUL_PY_LOGLEVEL_WARN\").is_ok() { filter = \
         log::LevelFilter::Warn; }\r\n",
    );
    code.push_str(
        "        if std::env::var(\"AZUL_PY_LOGLEVEL_INFO\").is_ok() { filter = \
         log::LevelFilter::Info; }\r\n",
    );
    code.push_str(
        "        if std::env::var(\"AZUL_PY_LOGLEVEL_DEBUG\").is_ok() { filter = \
         log::LevelFilter::Debug; }\r\n",
    );
    code.push_str(
        "        if std::env::var(\"AZUL_PY_LOGLEVEL_TRACE\").is_ok() { filter = \
         log::LevelFilter::Trace; }\r\n",
    );
    code.push_str(
        "        if std::env::var(\"AZUL_PY_LOGLEVEL_OFF\").is_ok() { filter = \
         log::LevelFilter::Off; }\r\n",
    );
    code.push_str("    }\r\n");
    code.push_str("\r\n");

    // Add all classes to the module
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_name_with_prefix = format!("{}{}", PREFIX, class_name);

            if class_data.struct_fields.is_some() {
                code.push_str(&format!(
                    "    m.add_class::<{}>()?;\r\n",
                    class_name_with_prefix
                ));
            } else if class_data.enum_fields.is_some() {
                code.push_str(&format!(
                    "    m.add_class::<{}EnumWrapper>()?;\r\n",
                    class_name_with_prefix
                ));
            }
        }
        code.push_str("\r\n");
    }

    // Finish the module
    code.push_str("    Ok(())\r\n");
    code.push_str("}\r\n");

    code
}

/// Collect struct and enum definitions
fn collect_structs_and_enums<'a>(
    api_data: &'a ApiData,
) -> (
    IndexMap<String, &'a ClassData>,
    IndexMap<String, &'a ClassData>,
) {
    let mut struct_map = IndexMap::new();
    let mut enum_map = IndexMap::new();

    // Get the latest version
    let latest_version = api_data.get_latest_version_str().unwrap();
    let version_data = api_data.get_version(latest_version).unwrap();

    // Collect all classes from all modules
    for (module_name, module) in &version_data.api {
        for (class_name, class_data) in &module.classes {
            let class_name_with_prefix = format!("{}{}", PREFIX, class_name);

            if class_data.struct_fields.is_some() {
                struct_map.insert(class_name_with_prefix.clone(), class_data);
            } else if class_data.enum_fields.is_some() {
                enum_map.insert(class_name_with_prefix.clone(), class_data);
            }
        }
    }

    (struct_map, enum_map)
}
