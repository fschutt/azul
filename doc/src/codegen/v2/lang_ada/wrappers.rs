//! Idiomatic Ada wrapper types using `Ada.Finalization.Controlled`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit:
//!
//! - In the **spec** (`azul.ads`):
//!     - A tagged record `type Foo_T is new Ada.Finalization.Controlled with record Inner :
//!       aliased Az_Foo; Owned : Boolean := True; end record;` - or `Limited_Controlled` when the
//!       C API has no deep copy (`Az_Foo_clone`) for it: such a value cannot be copied safely, so
//!       Ada refuses to copy it at all.
//!     - `overriding procedure Finalize (Self : in out Foo_T);`
//!     - `overriding procedure Adjust (Self : in out Foo_T);` (copyable wrappers only).
//! - In the **body** (`azul.adb`):
//!     - `Finalize` calls `Az_Foo_Delete (Self.Inner'Address)` once (guarded by `Owned`).
//!     - `Adjust` (runs on the target of every copy, e.g. `X := Make (...)`) replaces the copied
//!       bytes with a DEEP COPY, so the copy and the original are released independently. (It
//!       used to mark the copy not-owned, which left `X` aliasing the temporary that `Make`
//!       returned - freed right after the assignment.)
//! - A nested package `Azul.Fields` with a `Get_<Field>` / `Set_<Field>` pair per public field of
//!   every wrapped type (see [`emit_fields_spec`]): getters return independent values (deep
//!   copies for heap-owning fields, `Standard.String` for the string class), setters release
//!   the old value and move the new one in. They live in a nested package because a subprogram
//!   taking two different wrapper types cannot be a primitive of both.
//!
//! Plain POD structs without a `_delete` get *no* wrapper. Tagged-union
//! enums likewise: the FFI variant record is the user-facing surface.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        field_access_classic::{self as fa, AccessField, FieldKind},
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionKind, StructDef, TypeCategory},
    },
    ada_ffi_type_name, ada_wrapper_type_name,
    functions::ada_subprogram_name,
    map_type_to_ada, sanitize_identifier,
    types::pascalize_field_name,
};

// ============================================================================
// Public entry points (called from mod.rs)
// ============================================================================

pub fn emit_wrapper_specs(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Idiomatic Controlled wrappers (Finalize calls _delete on scope exit).");
    builder.line("-- ----------------------------------------------------------------------");
    builder.blank();

    for s in wrapped_structs(ir, config) {
        emit_wrapper_spec(builder, s, ir);
    }
    Ok(())
}

pub fn emit_wrapper_bodies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    for s in wrapped_structs(ir, config) {
        emit_wrapper_body(builder, s, ir);
    }
    Ok(())
}

// ============================================================================
// Discovery / filtering
// ============================================================================

fn should_wrap(s: &StructDef, _ir: &CodegenIR, config: &CodegenConfig) -> bool {
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

fn collect_delete_targets(ir: &CodegenIR) -> BTreeSet<&str> {
    ir.functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect()
}

/// Every struct that gets a `<Name>_T` Controlled wrapper, in IR order.
fn wrapped_structs<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Vec<&'a StructDef> {
    let delete_set = collect_delete_targets(ir);
    ir.structs
        .iter()
        .filter(|s| should_wrap(s, ir, config) && delete_set.contains(s.name.as_str()))
        .collect()
}

// ============================================================================
// Spec emission
// ============================================================================

fn emit_wrapper_spec(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let wrapper = ada_wrapper_type_name(&s.name);
    let ffi = ada_ffi_type_name(&s.name);
    let copyable = fa::clone_fn(ir, &s.name).is_some();

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- {}", d.replace('\n', " ").trim()));
        }
    }
    if !copyable {
        builder.line("-- Limited: the C API has no deep copy for this type, and a byte copy");
        builder.line("-- would be released twice.");
    }

    builder.line(&format!(
        "type {} is new Ada.Finalization.{} with record",
        wrapper,
        if copyable { "Controlled" } else { "Limited_Controlled" }
    ));
    builder.line(&format!("   Inner : aliased {};", ffi));
    builder.line("   Owned : Boolean := True;");
    builder.line("end record;");
    builder.blank();

    builder.line(&format!(
        "overriding procedure Finalize (Self : in out {});",
        wrapper
    ));
    if copyable {
        builder.line(&format!(
            "overriding procedure Adjust   (Self : in out {});",
            wrapper
        ));
    }
    builder.blank();
}

// ============================================================================
// Body emission
// ============================================================================

fn emit_wrapper_body(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let wrapper = ada_wrapper_type_name(&s.name);
    let ffi = ada_ffi_type_name(&s.name);

    builder.line(&format!(
        "overriding procedure Finalize (Self : in out {}) is",
        wrapper
    ));
    builder.line("begin");
    builder.line("   if Self.Owned then");
    // The FFI delete subprogram takes the address of the inner record;
    // we use `Self.Inner'Address` to obtain a `System.Address`.
    builder.line(&format!("      {}_Delete (Self.Inner'Address);", ffi));
    builder.line("      Self.Owned := False;");
    builder.line("   end if;");
    builder.line("end Finalize;");
    builder.blank();

    let Some(clone) = fa::clone_fn(ir, &s.name) else {
        return;
    };
    builder.line(&format!(
        "overriding procedure Adjust   (Self : in out {}) is",
        wrapper
    ));
    builder.line("begin");
    builder.line("   -- Runs on the target of a copy, whose bytes still alias the source:");
    builder.line("   -- give it a deep copy of its own, so both are released independently.");
    builder.line("   if Self.Owned then");
    builder.line(&format!(
        "      Self.Inner := {} (Self.Inner'Address);",
        ada_subprogram_name(clone)
    ));
    builder.line("   end if;");
    builder.line("end Adjust;");
    builder.blank();
}

// ============================================================================
// Field accessors (package Azul.Fields)
// ============================================================================

/// One accessor pair, fully spelled.
struct Accessor {
    getter_spec: Option<String>,
    getter_body: Vec<String>,
    setter_spec: String,
    setter_locals: Vec<String>,
    setter_body: Vec<String>,
    get_name: String,
    set_name: String,
}

/// The Ada spelling of the Fields helper that decodes a string field.
fn string_image_name(ir: &CodegenIR) -> Option<String> {
    fa::string_class(ir).map(|s| format!("{}_Image", ada_ffi_type_name(&s.name)))
}

fn ada_field_ident(name: &str) -> String {
    sanitize_identifier(&pascalize_field_name(name))
}

fn plan_accessor(
    owner: &StructDef,
    a: &AccessField,
    ir: &CodegenIR,
    wrapped: &BTreeSet<&str>,
) -> Option<Accessor> {
    let w = ada_wrapper_type_name(&owner.name);
    let field = ada_field_ident(&a.field.name);
    let get_name = format!("Get_{}", field);
    let set_name = format!("Set_{}", field);
    let fexpr = format!("Self.Inner.{}", field);
    let get_sig = |ty: &str| format!("function {} (Self : {}) return {}", get_name, w, ty);
    let set_sig = |param: &str| format!("procedure {} (Self : in out {}; Value : {})", set_name, w, param);
    let mut acc = Accessor {
        getter_spec: None,
        getter_body: Vec::new(),
        setter_spec: String::new(),
        setter_locals: Vec::new(),
        setter_body: Vec::new(),
        get_name: get_name.clone(),
        set_name: set_name.clone(),
    };
    match a.kind {
        FieldKind::Prim { is_bool: true } => {
            acc.getter_spec = Some(get_sig("Boolean"));
            acc.getter_body = vec![format!("return {} /= 0;", fexpr)];
            acc.setter_spec = set_sig("Boolean");
            acc.setter_body = vec![format!("{} := (if Value then 1 else 0);", fexpr)];
        }
        FieldKind::Prim { .. } | FieldKind::UnitEnum => {
            let ty = map_type_to_ada(a.ty, ir);
            acc.getter_spec = Some(get_sig(&ty));
            acc.getter_body = vec![format!("return {};", fexpr)];
            acc.setter_spec = set_sig(&ty);
            acc.setter_body = vec![format!("{} := Value;", fexpr)];
        }
        FieldKind::Str { delete } => {
            let image = string_image_name(ir)?;
            let copy = fa::string_copy_fn(ir)?;
            fa::string_layout(ir)?;
            acc.getter_spec = Some(get_sig("Standard.String"));
            acc.getter_body = vec![format!("return {} ({});", image, fexpr)];
            acc.setter_spec = set_sig("Standard.String");
            acc.setter_locals = vec![format!(
                "New_Value : constant {} := {} (Value'Address, 0, Interfaces.C.size_t (Value'Length));",
                ada_ffi_type_name(a.ty),
                ada_subprogram_name(copy)
            )];
            acc.setter_body = vec![
                format!("{} ({}'Address);", ada_subprogram_name(delete), fexpr),
                format!("{} := New_Value;", fexpr),
            ];
        }
        FieldKind::Value { delete, clone } if wrapped.contains(a.ty) => {
            // A field whose type has its own Controlled wrapper: the getter
            // hands out an owned deep copy, the setter consumes a wrapper.
            let fw = ada_wrapper_type_name(a.ty);
            let raw = ada_ffi_type_name(a.ty);
            if let Some(c) = clone {
                acc.getter_spec = Some(get_sig(&fw));
                acc.getter_body = vec![
                    format!("return Result : {} do", fw),
                    format!("   Result.Inner := {} ({}'Address);", ada_subprogram_name(c), fexpr),
                    "   Result.Owned := True;".to_string(),
                    "end return;".to_string(),
                ];
            }
            acc.setter_spec = set_sig(&format!("in out {}", fw));
            acc.setter_locals = vec![format!("New_Value : {};", raw)];
            acc.setter_body = vec![
                "if Value.Owned then".to_string(),
                "   New_Value := Value.Inner;".to_string(),
                "   Value.Owned := False; -- consumed: its Finalize no longer frees it".to_string(),
                "else".to_string(),
                match clone {
                    Some(c) => format!("   New_Value := {} (Value.Inner'Address);", ada_subprogram_name(c)),
                    None => format!(
                        "   raise Program_Error with \"{}: the value is borrowed and {} has no deep copy\";",
                        set_name, a.ty
                    ),
                },
                "end if;".to_string(),
            ];
            if let Some(d) = delete {
                acc.setter_body
                    .push(format!("{} ({}'Address);", ada_subprogram_name(d), fexpr));
            }
            acc.setter_body.push(format!("{} := New_Value;", fexpr));
        }
        FieldKind::Value { delete, clone } => {
            // A raw record / variant record: the setter takes ownership of
            // the record's heap memory (do not delete it afterwards).
            let raw = ada_ffi_type_name(a.ty);
            match (delete, clone) {
                (None, _) => {
                    acc.getter_spec = Some(get_sig(&raw));
                    acc.getter_body = vec![format!("return {};", fexpr)];
                }
                (Some(_), Some(c)) => {
                    acc.getter_spec = Some(get_sig(&raw));
                    acc.getter_body =
                        vec![format!("return {} ({}'Address);", ada_subprogram_name(c), fexpr)];
                }
                (Some(_), None) => {}
            }
            acc.setter_spec = set_sig(&raw);
            if let Some(d) = delete {
                acc.setter_body
                    .push(format!("{} ({}'Address);", ada_subprogram_name(d), fexpr));
            }
            acc.setter_body.push(format!("{} := Value;", fexpr));
        }
    }
    Some(acc)
}

/// `(wrapped type, its accessors)` for every wrapped type with at least one.
fn plan_all<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Vec<(&'a StructDef, Vec<Accessor>)> {
    let structs = wrapped_structs(ir, config);
    let wrapped: BTreeSet<&str> = structs.iter().map(|s| s.name.as_str()).collect();
    structs
        .iter()
        .map(|s| {
            let accs: Vec<Accessor> = fa::accessible_fields(s, ir, config)
                .iter()
                .filter_map(|a| plan_accessor(s, a, ir, &wrapped))
                .collect();
            (*s, accs)
        })
        .filter(|(_, accs)| !accs.is_empty())
        .collect()
}

/// `package Fields is ... end Fields;` (inside `package Azul`).
///
/// Nested writes are read-modify-write, because every getter returns an
/// independent copy:
///
/// ```ada
/// WS := Get_Window_State (Opts);       --  deep copy
/// Set_Title (WS, "Hello");             --  releases the old title
/// Size := Get_Size (WS);               --  plain record
/// Size.Dimensions.Width := 800.0;
/// Set_Size (WS, Size);
/// Set_Window_State (Opts, WS);         --  consumes WS
/// ```
pub fn emit_fields_spec(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let plans = plan_all(ir, config);
    if plans.is_empty() {
        return;
    }
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("-- Field accessors of the wrapper types (`use Azul.Fields;`).");
    builder.line("-- Get_<Field> returns an independent value (deep copy / Standard.String);");
    builder.line("-- Set_<Field> releases the old value and moves the new one in (a wrapper");
    builder.line("-- argument is consumed). Nested fields: read, modify, write back:");
    builder.line("--   WS := Get_Window_State (Opts); Set_Title (WS, \"Hi\");");
    builder.line("--   Set_Window_State (Opts, WS);  -- consumes WS");
    builder.line("-- ----------------------------------------------------------------------");
    builder.line("package Fields is");
    builder.indent();
    for (s, accs) in &plans {
        builder.line(&format!("-- {}", ada_wrapper_type_name(&s.name)));
        for a in accs {
            if let Some(g) = &a.getter_spec {
                builder.line(&format!("{};", g));
            }
            builder.line(&format!("{};", a.setter_spec));
        }
    }
    builder.dedent();
    builder.line("end Fields;");
    builder.blank();
}

/// `package body Fields is ... end Fields;` (inside `package body Azul`).
pub fn emit_fields_body(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let plans = plan_all(ir, config);
    if plans.is_empty() {
        return;
    }
    builder.line("package body Fields is");
    builder.indent();
    if let (Some(st), Some(image), Some((vec, ptr, len))) =
        (fa::string_class(ir), string_image_name(ir), fa::string_layout(ir))
    {
        let (vec, ptr, len) = (ada_field_ident(&vec), ada_field_ident(&ptr), ada_field_ident(&len));
        // Decodes WITHOUT consuming: the field keeps its bytes.
        builder.line(&format!(
            "function {} (S : {}) return Standard.String is",
            image,
            ada_ffi_type_name(&st.name)
        ));
        builder.line(&format!("   Len : constant Natural := Natural (S.{}.{});", vec, len));
        builder.line("begin");
        builder.line("   if Len = 0 then");
        builder.line("      return \"\";");
        builder.line("   end if;");
        builder.line("   declare");
        builder.line("      Bytes : Standard.String (1 .. Len);");
        builder.line(&format!("      for Bytes'Address use S.{}.{};", vec, ptr));
        builder.line("      pragma Import (Ada, Bytes);");
        builder.line("   begin");
        builder.line("      return Bytes;");
        builder.line("   end;");
        builder.line(&format!("end {};", image));
        builder.blank();
    }
    for (_, accs) in &plans {
        for a in accs {
            if let Some(g) = &a.getter_spec {
                builder.line(&format!("{} is", g));
                builder.line("begin");
                for l in &a.getter_body {
                    builder.line(&format!("   {}", l));
                }
                builder.line(&format!("end {};", a.get_name));
                builder.blank();
            }
            builder.line(&format!("{} is", a.setter_spec));
            for l in &a.setter_locals {
                builder.line(&format!("   {}", l));
            }
            builder.line("begin");
            for l in &a.setter_body {
                builder.line(&format!("   {}", l));
            }
            builder.line(&format!("end {};", a.set_name));
            builder.blank();
        }
    }
    builder.dedent();
    builder.line("end Fields;");
    builder.blank();
}

#[cfg(test)]
mod field_access_tests {
    use std::sync::OnceLock;

    use super::super::super::config::CodegenConfig;

    /// `(azul.ads, azul.adb)` generated from the real api.json.
    fn generated() -> &'static (String, String) {
        static OUT: OnceLock<(String, String)> = OnceLock::new();
        OUT.get_or_init(|| {
            let ir = crate::codegen::v2::bug_classes::ir();
            let all = super::super::generate(ir, &CodegenConfig::c_header()).expect("ada codegen");
            let (spec, body) = all.split_once(super::super::SPLIT_MARKER).expect("split marker");
            (spec.to_string(), body.to_string())
        })
    }

    /// The text of one subprogram body, from its header to its `end <Name>;`.
    fn body_of<'a>(body: &'a str, header: &str, name: &str) -> &'a str {
        let start = body.find(header).unwrap_or_else(|| panic!("no `{}` in the body", header));
        let rest = &body[start..];
        let end = rest.find(&format!("end {};", name)).expect("end of the subprogram");
        &rest[..end]
    }

    #[test]
    fn copying_a_wrapper_deep_copies_the_record_instead_of_leaving_the_copy_dangling() {
        let (_, body) = generated();
        let adjust = body_of(body, "overriding procedure Adjust   (Self : in out FullWindowState_T) is", "Adjust");
        assert!(
            adjust.contains("Self.Inner := Az_FullWindowState_Deep_Copy (Self.Inner'Address);"),
            "{}",
            adjust
        );
        assert!(!adjust.contains("Self.Owned := False;"), "{}", adjust);
    }

    #[test]
    fn a_wrapper_without_a_deep_copy_cannot_be_copied_at_all() {
        let (spec, _) = generated();
        let ir = crate::codegen::v2::bug_classes::ir();
        let no_clone = ["CameraWidget", "VideoWidget", "DomSplit"]
            .into_iter()
            .find(|n| spec.contains(&format!("type {}_T is new", n)))
            .expect("a wrapped type without _clone");
        assert!(crate::codegen::v2::field_access_classic::clone_fn(ir, no_clone).is_none());
        assert!(
            spec.contains(&format!(
                "type {}_T is new Ada.Finalization.Limited_Controlled with record",
                no_clone
            )),
            "{} must be limited",
            no_clone
        );
    }

    #[test]
    fn the_window_title_reads_as_an_ada_string_and_its_setter_releases_the_old_string() {
        let (spec, body) = generated();
        assert!(spec.contains("package Fields is"), "accessors live in Azul.Fields");
        assert!(spec.contains("function Get_Title (Self : FullWindowState_T) return Standard.String;"));
        assert!(spec.contains(
            "procedure Set_Title (Self : in out FullWindowState_T; Value : Standard.String);"
        ));
        let get = body_of(
            body,
            "function Get_Title (Self : FullWindowState_T) return Standard.String is",
            "Get_Title",
        );
        assert!(get.contains("Self.Inner.Title"), "{}", get);
        assert!(!get.contains("_Delete"), "reading must not free the field:\n{}", get);
        let set = body_of(
            body,
            "procedure Set_Title (Self : in out FullWindowState_T; Value : Standard.String) is",
            "Set_Title",
        );
        assert!(
            set.contains(
                "Az_String_Copy_From_Bytes (Value'Address, 0, Interfaces.C.size_t (Value'Length))"
            ),
            "{}",
            set
        );
        assert!(set.contains("Az_String_Delete (Self.Inner.Title'Address);"), "{}", set);
        assert!(set.contains("Self.Inner.Title := New_Value;"), "{}", set);
    }

    #[test]
    fn the_window_state_getter_deep_copies_and_its_setter_consumes_the_argument() {
        let (spec, body) = generated();
        assert!(spec.contains(
            "function Get_Window_State (Self : WindowCreateOptions_T) return FullWindowState_T;"
        ));
        let get = body_of(
            body,
            "function Get_Window_State (Self : WindowCreateOptions_T) return FullWindowState_T is",
            "Get_Window_State",
        );
        assert!(
            get.contains("Az_FullWindowState_Deep_Copy (Self.Inner.Window_State'Address)"),
            "{}",
            get
        );
        let set = body_of(
            body,
            "procedure Set_Window_State (Self : in out WindowCreateOptions_T; Value : in out FullWindowState_T) is",
            "Set_Window_State",
        );
        assert!(set.contains("Value.Owned := False;"), "the argument is consumed:\n{}", set);
        assert!(
            set.contains("Az_FullWindowState_Delete (Self.Inner.Window_State'Address);"),
            "{}",
            set
        );
    }

    #[test]
    fn the_window_size_is_a_plain_record_copy_in_both_directions() {
        let (spec, body) = generated();
        assert!(spec.contains("function Get_Size (Self : FullWindowState_T) return Az_WindowSize;"));
        let set = body_of(
            body,
            "procedure Set_Size (Self : in out FullWindowState_T; Value : Az_WindowSize) is",
            "Set_Size",
        );
        assert!(set.contains("Self.Inner.Size := Value;"), "{}", set);
        assert!(!set.contains("_Delete"), "a POD has nothing to release:\n{}", set);
    }

    #[test]
    fn a_text_input_text_field_is_writable_even_though_get_text_is_a_method() {
        let (spec, _) = generated();
        assert!(spec.contains(
            "procedure Set_Text (Self : in out TextInputState_T; Value : in out U32Vec_T);"
        ));
        assert!(spec.contains("function Get_Text (Self : TextInputState_T) return U32Vec_T;"));
    }
}
