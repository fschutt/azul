//! Idiomatic Ada wrapper types using `Ada.Finalization.Controlled`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C function,
//! we emit:
//!
//! - In the **spec** (`azul.ads`):
//!     - A tagged record `type Foo is new Ada.Finalization.Controlled with record Inner : aliased
//!       Az_Foo; Owned : Boolean := True; end record;`
//!     - `overriding procedure Finalize (Self : in out Foo);`
//!     - `overriding procedure Adjust (Self : in out Foo);` (set the Owned flag conservatively —
//!       clones aren't free, so we don't silently double-free).
//! - In the **body** (`azul.adb`):
//!     - The implementation of `Finalize`, which calls `Az_Foo_Delete (Self.Inner'Access)` once.
//!       After finalization the Owned flag is cleared so a second pass is a no-op.
//!     - The implementation of `Adjust`, which marks the freshly assigned copy as not-owned by
//!       default (Ada `Adjust` runs after a copy; we cannot safely deep-copy through the C ABI
//!       without an explicit clone API, so the safe default is "the copy does not own").
//!
//! Plain POD structs without a `_delete` get *no* wrapper. Tagged-union
//! enums likewise: the FFI variant record is the user-facing surface.

use std::collections::BTreeSet;

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, FunctionKind, StructDef, TypeCategory},
    },
    ada_ffi_type_name, ada_wrapper_type_name,
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

    let delete_set = collect_delete_targets(ir);

    for s in &ir.structs {
        if !should_wrap(s, ir, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_spec(builder, s);
    }
    Ok(())
}

pub fn emit_wrapper_bodies(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let delete_set = collect_delete_targets(ir);
    for s in &ir.structs {
        if !should_wrap(s, ir, config) {
            continue;
        }
        if !delete_set.contains(s.name.as_str()) {
            continue;
        }
        emit_wrapper_body(builder, s);
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

// ============================================================================
// Spec emission
// ============================================================================

fn emit_wrapper_spec(builder: &mut CodeBuilder, s: &StructDef) {
    let wrapper = ada_wrapper_type_name(&s.name);
    let ffi = ada_ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("-- {}", d.replace('\n', " ").trim()));
        }
    }

    builder.line(&format!(
        "type {} is new Ada.Finalization.Controlled with record",
        wrapper
    ));
    builder.line(&format!("   Inner : aliased {};", ffi));
    builder.line("   Owned : Boolean := True;");
    builder.line("end record;");
    builder.blank();

    builder.line(&format!(
        "overriding procedure Finalize (Self : in out {});",
        wrapper
    ));
    builder.line(&format!(
        "overriding procedure Adjust   (Self : in out {});",
        wrapper
    ));
    builder.blank();
}

// ============================================================================
// Body emission
// ============================================================================

fn emit_wrapper_body(builder: &mut CodeBuilder, s: &StructDef) {
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

    builder.line(&format!(
        "overriding procedure Adjust   (Self : in out {}) is",
        wrapper
    ));
    builder.line("begin");
    builder.line("   -- After assignment, the copy is conservatively not the owner;");
    builder.line("   -- the user must call an explicit clone primitive to take ownership.");
    builder.line("   Self.Owned := False;");
    builder.line("end Adjust;");
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
            set.contains("Az_String_Copy_From_Bytes (Value'Address, 0, Value'Length)"),
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
