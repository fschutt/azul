use super::HTML_ROOT;
use crate::{
    api::ApiData,
    utils::{
        analyze::{
            analyze_type, class_is_stack_allocated, enum_is_union, has_recursive_destructor,
            is_primitive_arg, search_for_class_by_class_name,
        },
        string::format_doc,
    },
};

const PREFIX: &str = "Az";

/// Generate API documentation HTML for a specific version
pub fn generate_api_html(api_data: &ApiData, version: &str) -> String {
    let mut html = String::new();

    // Load the HTML template - in a real implementation, this would be read from a file
    html.push_str("<!DOCTYPE html>\n<html><head><title>API Documentation</title></head><body>\n");

    // Get the version data
    let version_data = match api_data.get_version(version) {
        Some(data) => data,
        None => {
            html.push_str("<h1>Error: Version not found</h1>");
            html.push_str("</body></html>");
            return html;
        }
    };

    html.push_str(&format!(
        "<h1>API Documentation - Version {}</h1>\n",
        version
    ));

    // Add version documentation if available
    if let Some(doc) = &version_data.doc {
        html.push_str(&format!(
            "<p class=\"version doc\">{}</p>\n",
            format_doc(doc)
        ));
    }

    html.push_str("<ul>\n");

    // Process each module
    for (module_name, module) in &version_data.modules {
        html.push_str(&format!("<li class=\"m\" id=\"m.{}\">", module_name));

        // Add module documentation if available
        if let Some(doc) = &module.doc {
            html.push_str(&format!("<p class=\"m doc\">{}</p>", format_doc(doc)));
        }

        html.push_str(&format!(
            "<h3>mod <a href=\"#m.{}\">{}</a>:</h3>",
            module_name, module_name
        ));
        html.push_str("<ul>");

        // Process each class in the module
        for (class_name, class_data) in &module.classes {
            let is_boxed_object = class_data.is_boxed_object;
            let treat_external_as_ptr = class_data.external.is_some() && is_boxed_object;
            let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
            let class_has_recursive_destructor = has_recursive_destructor(api_data, class_data);

            let destructor_warning = if class_has_custom_destructor
                || treat_external_as_ptr
                || class_has_recursive_destructor
            {
                "&nbsp;<span class=\"chd\">has destructor</span>"
            } else {
                ""
            };

            // Handle enums
            if let Some(enum_fields) = &class_data.enum_fields {
                html.push_str(&format!("<li class=\"st e pbi\" id=\"st.{}\">", class_name));

                // Add class documentation if available
                if let Some(doc) = &class_data.doc {
                    html.push_str(&format!("<p class=\"class doc\">{}</p>", format_doc(doc)));
                }

                let enum_type = if enum_is_union(enum_fields) {
                    "union enum"
                } else {
                    "enum"
                };

                html.push_str(&format!(
                    "<h4>{} <a href=\"#st.{}\">{}</a>{}</h4>",
                    enum_type, class_name, class_name, destructor_warning
                ));

                // Process enum variants
                for variant_map in enum_fields {
                    for (variant_name, variant_data) in variant_map {
                        // Add variant documentation if available
                        if let Some(doc) = &variant_data.doc {
                            html.push_str(&format!("<p class=\"v doc\">{}</p>", format_doc(doc)));
                        }

                        // Handle variant with or without type
                        if let Some(variant_type) = &variant_data.r#type {
                            let (prefix, type_name, suffix) = analyze_type(variant_type);

                            if is_primitive_arg(&type_name) {
                                html.push_str(&format!(
                                    "<p class=\"f\">{}({})</p>",
                                    variant_name, variant_type
                                ));
                            } else if let Some((_, class_name)) =
                                search_for_class_by_class_name(api_data, &type_name)
                            {
                                html.push_str(&format!(
                                    "<p class=\"f\">{}({}<a href=\"#st.{}\">{}</a>{})</p>",
                                    variant_name, prefix, type_name, class_name, suffix
                                ));
                            } else {
                                html.push_str(&format!(
                                    "<p class=\"f\">{}({})</p>",
                                    variant_name, variant_type
                                ));
                            }
                        } else {
                            html.push_str(&format!("<p class=\"f\">{}</p>", variant_name));
                        }
                    }
                }
            }
            // Handle structs
            else if let Some(struct_fields) = &class_data.struct_fields {
                html.push_str(&format!("<li class=\"st s pbi\" id=\"st.{}\">", class_name));

                // Add class documentation if available
                if let Some(doc) = &class_data.doc {
                    html.push_str(&format!("<p class=\"class doc\">{}</p>", format_doc(doc)));
                }

                html.push_str(&format!(
                    "<h4>struct <a href=\"#st.{}\">{}</a>{}</h4>",
                    class_name, class_name, destructor_warning
                ));

                // Process struct fields
                for field_map in struct_fields {
                    for (field_name, field_data) in field_map {
                        let field_type = &field_data.r#type;
                        let (prefix, type_name, suffix) = analyze_type(field_type);

                        // Add field documentation if available
                        if let Some(doc) = &field_data.doc {
                            html.push_str(&format!("<p class=\"f doc\">{}</p>", format_doc(doc)));
                        }

                        if is_primitive_arg(&type_name) {
                            html.push_str(&format!(
                                "<p class=\"f\">{}: {}</p>",
                                field_name, field_type
                            ));
                        } else if let Some((_, class_name)) =
                            search_for_class_by_class_name(api_data, &type_name)
                        {
                            html.push_str(&format!(
                                "<p class=\"f\">{}: {}<a href=\"#st.{}\">{}</a>{}</p>",
                                field_name, prefix, type_name, class_name, suffix
                            ));
                        } else {
                            html.push_str(&format!(
                                "<p class=\"f\">{}: {}</p>",
                                field_name, field_type
                            ));
                        }
                    }
                }
            }
            // Handle typedefs
            else if let Some(callback_typedef) = &class_data.callback_typedef {
                html.push_str(&format!("<li class=\"pbi fnty\" id=\"st.{}\">", class_name));

                // Add class documentation if available
                if let Some(doc) = &class_data.doc {
                    html.push_str(&format!("<p class=\"class doc\">{}</p>", format_doc(doc)));
                }

                html.push_str(&format!(
                    "<h4>fnptr <a href=\"#fnty.{}\">{}</a></h4>",
                    class_name, class_name
                ));

                // Process callback arguments
                if !callback_typedef.fn_args.is_empty() {
                    html.push_str("<ul>");

                    for arg in &callback_typedef.fn_args {
                        // Add argument documentation if available
                        if let Some(doc) = &arg.doc {
                            html.push_str(&format!("<p class=\"arg doc\">{}</p>", format_doc(doc)));
                        }

                        let arg_type = &arg.r#type;
                        let (_, type_name, _) = analyze_type(arg_type);
                        let ref_kind = &arg.ref_kind;

                        let ref_prefix = match ref_kind.as_str() {
                            "value" => "",
                            "ref" => "&",
                            "refmut" => "&mut ",
                            _ => "",
                        };

                        if is_primitive_arg(&type_name) {
                            html.push_str(&format!(
                                "<li><p class=\"f\">arg {}</p></li>",
                                type_name
                            ));
                        } else if let Some((_, class_name)) =
                            search_for_class_by_class_name(api_data, &type_name)
                        {
                            html.push_str(&format!(
                                "<li><p class=\"fnty arg\">arg {} <a \
                                 href=\"#st.{}\">{}</a></p></li>",
                                ref_prefix, type_name, class_name
                            ));
                        } else {
                            html.push_str(&format!("<li><p class=\"f\">arg {}</p></li>", arg_type));
                        }
                    }

                    html.push_str("</ul>");
                }

                // Process callback return type
                if let Some(returns) = &callback_typedef.returns {
                    if let Some(doc) = &returns.doc {
                        html.push_str(&format!("<p class=\"ret doc\">{}</p>", format_doc(doc)));
                    }

                    let return_type = &returns.r#type;
                    let (_, type_name, _) = analyze_type(return_type);

                    if is_primitive_arg(&type_name) {
                        html.push_str(&format!("<p class=\"fnty ret\">->&nbsp;{}</p>", type_name));
                    } else if let Some((_, class_name)) =
                        search_for_class_by_class_name(api_data, &type_name)
                    {
                        html.push_str(&format!(
                            "<p class=\"fnty ret\">->&nbsp;<a href=\"#st.{}\">{}</a></p>",
                            type_name, class_name
                        ));
                    } else {
                        html.push_str(&format!(
                            "<p class=\"fnty ret\">->&nbsp;{}</p>",
                            return_type
                        ));
                    }
                }
            }

            // Process constructors
            if let Some(constructors) = &class_data.constructors {
                html.push_str("<ul>");

                for (constructor_name, constructor) in constructors {
                    if let Some(doc) = &constructor.doc {
                        html.push_str(&format!("<p class=\"cn doc\">{}</p>", format_doc(doc)));
                    }

                    html.push_str(&format!(
                        "<li class=\"cn\" id=\"{}.{}\">",
                        class_name, constructor_name
                    ));
                    html.push_str(&format!(
                        "<p>constructor <a href=\"#{}.{}\">{}</a>:</p>",
                        class_name, constructor_name, constructor_name
                    ));
                    html.push_str("<ul>");

                    // Process constructor arguments
                    for arg_map in &constructor.fn_args {
                        for (arg_name, arg_type) in arg_map {
                            if arg_name == "self" {
                                continue;
                            }

                            let (prefix, type_name, suffix) = analyze_type(arg_type);

                            if is_primitive_arg(&type_name) {
                                html.push_str(&format!(
                                    "<li><p class=\"arg\">arg {}: {}</p></li>",
                                    arg_name, arg_type
                                ));
                            } else if let Some((_, class_name)) =
                                search_for_class_by_class_name(api_data, &type_name)
                            {
                                html.push_str(&format!(
                                    "<li><p class=\"arg\">arg {}: {}<a \
                                     href=\"#st.{}\">{}</a>{}</p></li>",
                                    arg_name, prefix, type_name, class_name, suffix
                                ));
                            } else {
                                html.push_str(&format!(
                                    "<li><p class=\"arg\">arg {}: {}</p></li>",
                                    arg_name, arg_type
                                ));
                            }
                        }
                    }

                    // Process return type
                    if let Some(returns) = &constructor.returns {
                        html.push_str("<li>");

                        if let Some(doc) = &returns.doc {
                            html.push_str(&format!("<p class=\"ret doc\">{}</p>", format_doc(doc)));
                        }

                        let return_type = &returns.r#type;
                        let (prefix, type_name, suffix) = analyze_type(return_type);

                        if is_primitive_arg(&type_name) {
                            html.push_str(&format!(
                                "<p class=\"cn ret\">->&nbsp;{}</p>",
                                type_name
                            ));
                        } else if let Some((_, class_name)) =
                            search_for_class_by_class_name(api_data, &type_name)
                        {
                            html.push_str(&format!(
                                "<p class=\"cn ret\">->&nbsp;{}<a href=\"#st.{}\">{}</a>{}</p>",
                                prefix, type_name, class_name, suffix
                            ));
                        } else {
                            html.push_str(&format!(
                                "<p class=\"cn ret\">->&nbsp;{}</p>",
                                return_type
                            ));
                        }

                        html.push_str("</li>");
                    } else {
                        html.push_str(&format!(
                            "<li><p class=\"ret\">->&nbsp;<a href=\"#st.{}\">{}</a></p></li>",
                            class_name, class_name
                        ));
                    }

                    html.push_str("</ul>");
                    html.push_str("</li>");
                }

                html.push_str("</ul>");
            }

            // Process methods
            if let Some(functions) = &class_data.functions {
                html.push_str("<ul>");

                for (function_name, function) in functions {
                    if let Some(doc) = &function.doc {
                        html.push_str(&format!("<p class=\"fn doc\">{}</p>", format_doc(doc)));
                    }

                    html.push_str(&format!(
                        "<li class=\"fn\" id=\"{}.{}\">",
                        class_name, function_name
                    ));
                    html.push_str(&format!(
                        "<p>fn <a href=\"#{}.{}\">{}</a>:</p>",
                        class_name, function_name, function_name
                    ));
                    html.push_str("<ul>");

                    // Handle self argument
                    let mut self_arg = String::new();

                    for arg_map in &function.fn_args {
                        for (arg_name, arg_type) in arg_map {
                            if arg_name == "self" {
                                if arg_type == "value" {
                                    self_arg = "self".to_string();
                                } else if arg_type == "ref" {
                                    self_arg = "&self".to_string();
                                } else if arg_type == "refmut" {
                                    self_arg = "&mut self".to_string();
                                }
                                break;
                            }
                        }
                    }

                    html.push_str(&format!("<li><p class=\"arg\">{}</p></li>", self_arg));

                    // Process method arguments
                    for arg_map in &function.fn_args {
                        for (arg_name, arg_type) in arg_map {
                            if arg_name == "self" {
                                continue;
                            }

                            let (prefix, type_name, suffix) = analyze_type(arg_type);

                            if is_primitive_arg(&type_name) {
                                html.push_str(&format!(
                                    "<li><p class=\"arg\">arg {}: {}</p></li>",
                                    arg_name, arg_type
                                ));
                            } else if let Some((_, class_name)) =
                                search_for_class_by_class_name(api_data, &type_name)
                            {
                                html.push_str(&format!(
                                    "<li><p class=\"arg\">arg {}: {}<a \
                                     href=\"#st.{}\">{}</a>{}</p></li>",
                                    arg_name, prefix, type_name, class_name, suffix
                                ));
                            } else {
                                html.push_str(&format!(
                                    "<li><p class=\"arg\">arg {}: {}</p></li>",
                                    arg_name, arg_type
                                ));
                            }
                        }
                    }

                    // Process return type
                    if let Some(returns) = &function.returns {
                        html.push_str("<li>");

                        if let Some(doc) = &returns.doc {
                            html.push_str(&format!("<p class=\"ret doc\">{}</p>", format_doc(doc)));
                        }

                        let return_type = &returns.r#type;
                        let (prefix, type_name, suffix) = analyze_type(return_type);

                        if is_primitive_arg(&type_name) {
                            html.push_str(&format!(
                                "<p class=\"fn ret\">->&nbsp;{}</p>",
                                type_name
                            ));
                        } else if let Some((_, class_name)) =
                            search_for_class_by_class_name(api_data, &type_name)
                        {
                            html.push_str(&format!(
                                "<p class=\"fn ret\">->&nbsp;{}<a href=\"#st.{}\">{}</a>{}</p>",
                                prefix, type_name, class_name, suffix
                            ));
                        } else {
                            html.push_str(&format!(
                                "<p class=\"fn ret\">->&nbsp;{}</p>",
                                return_type
                            ));
                        }

                        html.push_str("</li>");
                    }

                    html.push_str("</ul>");
                    html.push_str("</li>");
                }

                html.push_str("</ul>");
            }

            html.push_str("</li>"); // Close class
        }

        html.push_str("</ul>"); // Close module classes
        html.push_str("</li>"); // Close module
    }

    html.push_str("</ul>");
    html.push_str("</body></html>");

    html
}

/// Generate a combined API index page
pub fn generate_api_index(api_data: &ApiData) -> String {
    let mut html = String::new();

    // Load the HTML template - in a real implementation, this would be read from a file
    html.push_str("<!DOCTYPE html>\n<html><head><title>API Versions</title></head><body>\n");

    html.push_str("<h1>Choose API version</h1>\n");

    // Create a sidebar with API version links
    html.push_str("<ul>\n");

    for version in api_data.get_sorted_versions() {
        html.push_str(&format!(
            "<li><a href=\"{}/api/{}\">{}</a></li>\n",
            HTML_ROOT, version, version
        ));
    }

    html.push_str("</ul>\n");
    html.push_str("</body></html>");

    html
}
