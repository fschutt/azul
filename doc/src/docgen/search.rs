//! Build a compact JSON search index from `api.json`.
//!
//! The output ships alongside each version's `api/<version>.html` and is
//! consumed by `azul-search.js` for client-side fuzzy search. Anchors match
//! the IDs already emitted by `apidocs::generate_api_html` (`#m.<module>`,
//! `#st.<Class>`, `#<Class>.<member>`) so a click jumps to the existing
//! deep-link target.
//!
//! Field names are intentionally short (`n`, `k`, `m`, `p`, `a`, `d`, `s`)
//! to keep the on-the-wire size small once gzipped.

use serde_derive::Serialize;

use crate::api::{CallbackDefinition, FunctionData, VersionData};

/// One searchable entity. Keys are short to keep the gzipped index small;
/// the JS reader knows the shape.
#[derive(Serialize)]
struct Entry<'a> {
    /// Short kind code: `m` module, `s` struct, `e` enum, `fp` fnptr,
    /// `ev` enum variant, `f` struct field, `fn` method, `cn` constructor.
    k: &'static str,
    /// The thing's own name.
    n: &'a str,
    /// Module the entity lives in (omitted for the module entry itself).
    #[serde(skip_serializing_if = "Option::is_none")]
    m: Option<&'a str>,
    /// Parent class for variants/fields/methods/constructors.
    #[serde(skip_serializing_if = "Option::is_none")]
    p: Option<&'a str>,
    /// Anchor fragment within the api page (no leading `#`).
    a: String,
    /// Plain-text doc body. Empty string when the source has no doc.
    d: String,
    /// Optional signature line for fns/constructors/callbacks.
    #[serde(skip_serializing_if = "Option::is_none")]
    s: Option<String>,
}

#[derive(Serialize)]
struct Index<'a> {
    v: &'a str,
    e: Vec<Entry<'a>>,
}

/// Generate the JSON index for one API version.
pub fn generate_search_index(version: &str, version_data: &VersionData) -> String {
    let mut entries: Vec<Entry> = Vec::new();

    for (module_name, module) in &version_data.api {
        entries.push(Entry {
            k: "m",
            n: module_name,
            m: None,
            p: None,
            a: format!("m.{}", module_name),
            d: doc_to_text(module.doc.as_deref()),
            s: None,
        });

        for (class_name, class) in &module.classes {
            let class_doc = doc_to_text(class.doc.as_deref());

            // Top-level kind for the class itself.
            let class_kind = if class.enum_fields.is_some() {
                "e"
            } else if class.callback_typedef.is_some() {
                "fp"
            } else {
                // Treat structs and method-only "container" classes the same.
                "s"
            };

            entries.push(Entry {
                k: class_kind,
                n: class_name,
                m: Some(module_name),
                p: None,
                a: format!("st.{}", class_name),
                d: class_doc.clone(),
                s: None,
            });

            // Enum variants.
            if let Some(variants) = &class.enum_fields {
                for variant_map in variants {
                    for (variant_name, variant) in variant_map {
                        entries.push(Entry {
                            k: "ev",
                            n: variant_name,
                            m: Some(module_name),
                            p: Some(class_name),
                            a: format!("st.{}", class_name),
                            d: doc_to_text(variant.doc.as_deref()),
                            s: variant.r#type.clone(),
                        });
                    }
                }
            }

            // Struct fields.
            if let Some(fields) = &class.struct_fields {
                for field_map in fields {
                    for (field_name, field) in field_map {
                        entries.push(Entry {
                            k: "f",
                            n: field_name,
                            m: Some(module_name),
                            p: Some(class_name),
                            a: format!("st.{}", class_name),
                            d: doc_to_text(field.doc.as_deref()),
                            s: Some(field.r#type.clone()),
                        });
                    }
                }
            }

            // Callback typedef signature.
            if let Some(cb) = &class.callback_typedef {
                // The class entry already covers the name + doc; just attach
                // a signature so search results can show it.
                if let Some(last) = entries.last_mut() {
                    last.s = Some(callback_signature(cb));
                }
            }

            // Constructors.
            if let Some(constructors) = &class.constructors {
                for (ctor_name, ctor) in constructors {
                    entries.push(Entry {
                        k: "cn",
                        n: ctor_name,
                        m: Some(module_name),
                        p: Some(class_name),
                        a: format!("{}.{}", class_name, ctor_name),
                        d: doc_to_text(ctor.doc.as_deref()),
                        s: Some(function_signature(ctor, Some(class_name))),
                    });
                }
            }

            // Methods.
            if let Some(functions) = &class.functions {
                for (fn_name, func) in functions {
                    entries.push(Entry {
                        k: "fn",
                        n: fn_name,
                        m: Some(module_name),
                        p: Some(class_name),
                        a: format!("{}.{}", class_name, fn_name),
                        d: doc_to_text(func.doc.as_deref()),
                        s: Some(function_signature(func, None)),
                    });
                }
            }
        }
    }

    let index = Index { v: version, e: entries };
    serde_json::to_string(&index).unwrap_or_else(|_| String::from("{\"v\":\"\",\"e\":[]}"))
}

/// Flatten `Vec<String>` doc lines into a single plain-text blob, stripping
/// fenced code blocks and markdown markers we don't want in match snippets.
/// We keep it terse — no HTML, no escaping; the JS layer escapes when it
/// renders.
fn doc_to_text(lines: Option<&[String]>) -> String {
    let Some(lines) = lines else { return String::new() };

    let mut in_code = false;
    let mut out = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if trimmed.is_empty() {
            // Preserve a paragraph break as a single space — keeps token
            // boundaries without bloating with "\n\n".
            if !out.is_empty() && !out.ends_with(' ') {
                out.push(' ');
            }
            continue;
        }
        if !out.is_empty() && !out.ends_with(' ') {
            out.push(' ');
        }
        out.push_str(trimmed);
    }

    // Strip the most common markdown markers — backticks, bold/italic stars
    // and underscores. We don't try to be a markdown parser; the goal is
    // matchable plain text.
    let mut cleaned = String::with_capacity(out.len());
    for ch in out.chars() {
        match ch {
            '`' | '*' => continue,
            _ => cleaned.push(ch),
        }
    }
    cleaned
}

fn function_signature(func: &FunctionData, ctor_returns: Option<&str>) -> String {
    let mut sig = String::new();
    sig.push('(');
    let mut first = true;
    for arg_map in &func.fn_args {
        for (arg_name, arg_type) in arg_map {
            if !first {
                sig.push_str(", ");
            }
            first = false;
            if arg_name == "self" {
                match arg_type.as_str() {
                    "value" => sig.push_str("self"),
                    "ref" => sig.push_str("&self"),
                    "refmut" => sig.push_str("&mut self"),
                    other => sig.push_str(other),
                }
            } else {
                sig.push_str(arg_name);
                sig.push_str(": ");
                sig.push_str(arg_type);
            }
        }
    }
    sig.push(')');

    if let Some(ret) = &func.returns {
        sig.push_str(" -> ");
        sig.push_str(&ret.r#type);
    } else if let Some(ret) = ctor_returns {
        sig.push_str(" -> ");
        sig.push_str(ret);
    }
    sig
}

fn callback_signature(cb: &CallbackDefinition) -> String {
    let mut sig = String::from("fn(");
    let mut first = true;
    for arg in &cb.fn_args {
        if !first {
            sig.push_str(", ");
        }
        first = false;
        sig.push_str(arg.ref_kind.to_rust_prefix());
        sig.push_str(&arg.r#type);
    }
    sig.push(')');
    if let Some(ret) = &cb.returns {
        sig.push_str(" -> ");
        sig.push_str(&ret.r#type);
    }
    sig
}
