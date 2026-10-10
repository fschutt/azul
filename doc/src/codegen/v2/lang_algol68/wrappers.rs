//! Convention helpers for manual cleanup of native resources.
//!
//! Algol 68 has **no destructors**. The Revised Report (1973) does not
//! describe RAII, smart pointers, or scope-bound finalisation; the
//! handful of language extensions that exist in a68g (HEAP allocation,
//! garbage collection of `LOC` / `HEAP` cells) cover only Algol 68's own
//! managed memory and do not run on resources reached through `ALIEN`.
//!
//! As a result, an Algol 68 user must release every owned native
//! resource by hand. We help them in three ways:
//!
//! 1. For every IR struct that has a matching `<TypeName>_delete` C function, we emit a paired
//!    Algol 68 PROC named `delete <type>` whose body is an `ALIEN` call to that same C symbol. The
//!    user writes `delete app(app)` instead of having to remember the C name.
//! 2. For each such type we emit a comment block reminding the user that they own the value and
//!    must explicitly `delete` it before its `REF` goes out of scope.
//! 3. We provide a `# Manual cleanup convention #` overview comment at the top of the wrappers
//!    section so users browsing the file understand the contract before they hit individual
//!    procedures.
//!
//! ## Why no idiomatic class wrappers?
//!
//! Object-oriented features (constructors / destructors / methods) do
//! not exist in Algol 68. A68G does not ship an OO extension. We
//! deliberately stop at the convention layer rather than synthesising a
//! pseudo-OO API on top of records — anyone using a68g for a serious
//! project will have grown a hand-rolled cleanup discipline already.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FieldRefKind, FunctionKind, StructDef, TypeCategory},
    },
    algol_mode_name, algol_proc_name, camel_or_snake_to_spaced_lower_pub, functions,
    map_type_to_algol, sanitize_identifier, types, LIB_NAME,
};

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    if targets.is_empty() {
        return Ok(());
    }

    emit_convention_header(builder);

    for s in &targets {
        emit_delete_helper(builder, s);
    }

    builder.blank();
    emit_field_helpers(builder, ir, config);
    Ok(())
}

/// The `<class>_<kind>` ALIEN PROC name of `class`, when it is declared.
fn class_proc(
    class: &str,
    kind: FunctionKind,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<String> {
    ir.functions_for_class(class)
        .find(|f| f.kind == kind && functions::should_emit_function(f, ir, config))
        .map(|f| algol_proc_name(&f.class_name, &f.method_name))
}

/// Field helpers. A field is reached with `OF` (`title OF window state OF
/// window` is a REF into the record, so nested writes land), but a plain
/// assignment overwrites a heap-owning field without releasing the old
/// value. So, per heap-owning MODE (one with a `_delete`):
///
///   `replace az <type> (field, new)` - releases the field's old value,
///   then moves `new` in (`new` must not be used or deleted afterwards);
///
/// and for the String MODE:
///
///   `read az string (s)` - the Algol 68 STRING it holds (only read);
///   `replace az string text (field, text)` - `replace az string` from a
///   STRING.
fn emit_field_helpers(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    builder
        .line("# ---------------------------------------------------------------------------- #");
    builder
        .line("# Field helpers: `title OF window state OF window` is a REF into the record,    #");
    builder
        .line("# but `:=` on a heap-owning field leaks the old value. Use                      #");
    builder.line(
        "#     replace az string text (title OF window state OF window, \"Hello\")         #",
    );
    builder
        .line("#     replace az <type> (field, new value)   -- the new value is MOVED in        #");
    builder
        .line("# ---------------------------------------------------------------------------- #");
    builder.blank();

    let owning: Vec<&str> = ir
        .structs
        .iter()
        .filter(|s| should_emit_wrapper(s, config))
        .map(|s| s.name.as_str())
        .chain(
            ir.enums
                .iter()
                .filter(|e| e.is_union && types::should_include_enum(e, config))
                .map(|e| e.name.as_str()),
        )
        .collect();
    for name in owning {
        let Some(delete) = class_proc(name, FunctionKind::Delete, ir, config) else {
            continue;
        };
        let mode = algol_mode_name(name);
        builder.line(&format!(
            "PROC replace az {} = (REF {} field, {} new) VOID:",
            camel_or_snake_to_spaced_lower_pub(name),
            mode,
            mode
        ));
        builder.line("BEGIN");
        builder.line(&format!("  {} (field);", delete));
        builder.line("  field := new");
        builder.line("END;");
    }
    builder.blank();

    // The String MODE: found by category; its bytes are described by its
    // one field (a byte Vec), read element by element with that Vec's
    // index accessor (a method taking `usize`, answering an Option).
    let Some(string) = ir
        .structs
        .iter()
        .find(|s| s.category == TypeCategory::String && should_emit_wrapper(s, config))
    else {
        return;
    };
    let mode = algol_mode_name(&string.name);
    let lower = camel_or_snake_to_spaced_lower_pub(&string.name);
    let vec = string.fields.first();
    let inner = vec.and_then(|v| ir.find_struct(v.type_name.trim()));
    let len = inner.and_then(|i| i.fields.iter().find(|f| f.type_name.trim() == "usize"));
    let get = inner.and_then(|i| {
        ir.functions_for_class(&i.name).find(|f| {
            f.kind == FunctionKind::Method
                && f.args.len() == 2
                && f.args[1].type_name.trim() == "usize"
                && f.return_type.as_deref().is_some_and(|r| {
                    ir.find_enum(r.trim()).is_some() || ir.find_type_alias(r.trim()).is_some()
                })
                && functions::should_emit_function(f, ir, config)
        })
    });
    let elem = inner
        .and_then(|i| i.fields.iter().find(|f| f.ref_kind != FieldRefKind::Owned))
        .map(|f| map_type_to_algol(&f.type_name, ir))
        .unwrap_or_else(|| "CHAR".to_string());
    if let (Some(vec), Some(len), Some(get)) = (vec, len, get) {
        let vec_f = sanitize_identifier(&vec.name);
        builder.line(&format!(
            "# The Algol 68 STRING an {} holds; the {} is only read, never consumed. #",
            mode, mode
        ));
        builder.line(&format!(
            "PROC read az {} = (REF {} s) STRING:",
            lower, mode
        ));
        builder.line("BEGIN");
        builder.line(&format!(
            "  INT n = SHORTEN ({} OF {} OF s);",
            sanitize_identifier(&len.name),
            vec_f
        ));
        builder.line("  [1 : n] CHAR out;");
        builder.line("  FOR i TO n DO");
        builder.line(&format!(
            "    out[i] := (payload OF {} ({} OF s, LENG (i - 1)) | ({} c): c | REPR 0)",
            algol_proc_name(&get.class_name, &get.method_name),
            vec_f,
            elem
        ));
        builder.line("  OD;");
        builder.line("  out");
        builder.line("END;");
        builder.blank();
    }
    let from_bytes = ir
        .functions_for_class(&string.name)
        // allow-api-name: no kind or shape distinguishes this constructor.
        .find(|f| f.method_name == "copy_from_bytes")
        .filter(|f| functions::should_emit_function(f, ir, config));
    if let (Some(from_bytes), Some(_)) = (
        from_bytes,
        class_proc(&string.name, FunctionKind::Delete, ir, config),
    ) {
        builder.line(&format!(
            "# Replace an {} field with a copy of TEXT; the old value is released. #",
            mode
        ));
        builder.line(&format!(
            "PROC replace az {} text = (REF {} field, STRING text) VOID:",
            lower, mode
        ));
        builder.line(&format!(
            "  replace az {} (field, {} (text, LENG 0, LENG UPB text));",
            lower,
            algol_proc_name(&from_bytes.class_name, &from_bytes.method_name)
        ));
        builder.blank();
    }
}

fn emit_convention_header(builder: &mut CodeBuilder) {
    builder
        .line("# ---------------------------------------------------------------------------- #");
    builder
        .line("# Manual cleanup convention                                                     #");
    builder
        .line("#                                                                                #");
    builder
        .line("# Algol 68 has no destructors and a68g's reference-counted heap does NOT track  #");
    builder
        .line("# resources reached through ALIEN. Every value listed below is owned by the     #");
    builder
        .line("# caller; the caller MUST invoke the matching `delete <type>` PROC before the   #");
    builder
        .line("# REF leaves scope, otherwise the underlying C-side resource leaks.             #");
    builder
        .line("#                                                                                #");
    builder
        .line("# Idiomatic pattern:                                                             #");
    builder
        .line("#                                                                                #");
    builder
        .line("#     REF AZAPP app := az app create (data, config);                             #");
    builder
        .line("#     az app run (app, window);                                                  #");
    builder
        .line("#     delete az app (app)   # release native memory before scope exit #          #");
    builder
        .line("#                                                                                #");
    builder
        .line("# Wrapping a value in a HEAP cell with a hand-rolled cleanup ON SCOPE EXIT       #");
    builder
        .line("# pragma is also a viable pattern for users who want some automation.           #");
    builder
        .line("# ---------------------------------------------------------------------------- #");
    builder.blank();
}

fn emit_delete_helper(builder: &mut CodeBuilder, s: &StructDef) {
    let type_lower = camel_or_snake_to_spaced_lower_pub(&s.name);
    let mode = algol_mode_name(&s.name);
    let c_symbol = format!("Az{}_delete", s.name);

    builder.line(&format!(
        "# Release native memory owned by an {}. Caller must NOT use the value afterwards. #",
        mode
    ));
    builder.line(&format!(
        "PROC delete az {} = (REF {} value) VOID: ALIEN \"{}\" ! \"{}\";",
        type_lower, mode, c_symbol, LIB_NAME
    ));
}

// ============================================================================
// Discovery (mirrors lang_pascal/wrappers.rs:collect_wrapper_targets)
// ============================================================================

fn collect_wrapper_targets<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Vec<&'a StructDef> {
    let delete_set: BTreeSet<&str> = ir
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect();

    ir.structs
        .iter()
        .filter(|s| should_emit_wrapper(s, config) && delete_set.contains(s.name.as_str()))
        .collect()
}

fn should_emit_wrapper(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

#[cfg(test)]
mod tests {
    use super::super::super::{bug_classes::ir, config::CodegenConfig};

    fn a68() -> &'static str {
        static OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        OUT.get_or_init(|| super::super::generate(ir(), &CodegenConfig::c_header()).unwrap())
    }

    /// The PROC declaration starting with `head`, up to its closing `;`
    /// at column 0 (`END;`) or the end of a one-line declaration.
    fn proc_text(head: &str) -> &'static str {
        let out = a68();
        let start = out
            .find(head)
            .unwrap_or_else(|| panic!("no `{head}` in azul.a68"));
        let rest = &out[start..];
        let end = rest.find("\nEND;").map_or(rest.len(), |e| e + 5);
        &rest[..end]
    }

    #[test]
    fn a_string_is_read_into_an_algol_string_without_consuming_it() {
        let read = proc_text("PROC read az string = (REF AZSTRING s) STRING:");
        assert!(read.contains("az u8 vec get (vec OF s, "), "{read}");
        assert!(!read.contains("delete"), "{read}");
    }

    #[test]
    fn replacing_a_string_field_releases_the_old_one_first() {
        let set = proc_text("PROC replace az string = (REF AZSTRING field, AZSTRING new) VOID:");
        let delete = set.find("az string delete (field)").expect(set);
        let store = set.find("field := new").expect(set);
        assert!(delete < store, "{set}");
        let text =
            proc_text("PROC replace az string text = (REF AZSTRING field, STRING text) VOID:");
        assert!(text.contains("replace az string (field, "), "{text}");
    }

    #[test]
    fn replacing_a_window_state_releases_the_old_one_first() {
        let set = proc_text(
            "PROC replace az full window state = (REF AZFULLWINDOWSTATE field, AZFULLWINDOWSTATE \
             new) VOID:",
        );
        assert!(set.contains("az full window state delete (field)"), "{set}");
    }
}
