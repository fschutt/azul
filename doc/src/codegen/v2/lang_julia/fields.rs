//! Field accessors: `get_<field>` / `set_<field>!` / `<field>_ptr` for every
//! accessible field of every emitted struct (see `raw_field_access` for the
//! contract).
//!
//! The structs are immutable isbits values, so the accessors work on a
//! mutable box: a `Ref{AzT}` (`opts = Ref(AzWindowCreateOptions_create(cb))`)
//! or a `Ptr{AzT}` view into a parent's memory, which `<field>_ptr` returns.
//! That is what makes nested writes land in the parent:
//!
//! ```julia
//! opts = Ref(Azul.AzWindowCreateOptions_create(layout_ptr))
//! GC.@preserve opts begin
//!     ws = Azul.window_state_ptr(opts)
//!     Azul.set_title!(ws, "Hello")
//!     Azul.set_dimensions!(Azul.size_ptr(ws), Azul.AzLogicalSize(400f0, 300f0))
//! end
//! ```
//!
//! Generic functions dispatch on the box's element type, so `get_title`
//! serves every struct with a `title` field, and an api.json method (always
//! spelled `Type_method`) can never collide with one.

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::CodegenIR,
        raw_field_access::{accessible_fields, RawFieldKind},
    },
    ffi_type_name, field_type_for_ref_kind, include_struct, should_emit_function,
};

/// Helpers the accessors share; they reference no generated type, so they
/// can sit in the prelude.
pub fn emit_helpers(b: &mut CodeBuilder) {
    b.line("# --- field-accessor helpers ------------------------------------------------");
    b.line("# A mutable box around a struct: a `Ref` the caller owns, or a `Ptr` view");
    b.line("# into a parent's memory (from `<field>_ptr`). Keep the root `Ref` alive");
    b.line("# (`GC.@preserve`) while a view is in use.");
    b.line("const _AzRef{T} = Union{Base.RefValue{T}, Ptr{T}}");
    b.line("_az_base(x::Base.RefValue{T}) where {T} = Base.unsafe_convert(Ptr{T}, x)");
    b.line("_az_base(x::Ptr) = x");
    b.line("# The address of field `i` (1-based) of the box's struct, typed as `F`.");
    b.line("_az_fptr(x::_AzRef{T}, ::Type{F}, i::Int) where {T, F} = Ptr{F}(_az_base(x) + fieldoffset(T, i))");
    b.line("# Release the field's old value, then move `v` in (the field owns it now).");
    b.line("_az_replace!(p::Ptr{F}, v::F, release) where {F} = (release(p); unsafe_store!(p, v); nothing)");
    b.blank();
}

pub fn generate_field_accessors(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    b.line("# ----------------------------------------------------------------------------");
    b.line("# Field accessors. `x` is a `Ref{AzT}` or a `Ptr{AzT}` view (`<field>_ptr`).");
    b.line("#   get_<field>(x)      an independent value: strings are decoded without");
    b.line("#                       being consumed, heap-owning values deep-copied");
    b.line("#                       (`Az<T>_clone`) - the caller owns the copy;");
    b.line("#   set_<field>!(x, v)  releases the old value (`Az<T>_delete`), then moves");
    b.line("#                       `v` in - do not use or delete `v` afterwards;");
    b.line("#   <field>_ptr(x)      a view of the field, for nested writes.");
    b.line("# ----------------------------------------------------------------------------");
    b.blank();

    let emitted = |f: &super::super::ir::FunctionDef| should_emit_function(f, ir, config);
    for s in &ir.structs {
        if !include_struct(s, config) || s.fields.is_empty() {
            continue;
        }
        let fields = accessible_fields(s, ir, config, &emitted);
        if fields.is_empty() {
            continue;
        }
        let st = ffi_type_name(&s.name);
        let recv = format!("x::_AzRef{{{}}}", st);
        b.line(&format!("# {}", st));
        for f in &fields {
            let name = &f.def.name;
            let fty = field_type_for_ref_kind(&f.def.type_name, &f.def.ref_kind, ir);
            let fp = format!("_az_fptr(x, {}, {})", fty, f.index + 1);
            match &f.kind {
                RawFieldKind::Prim { .. } => {
                    b.line(&format!(
                        "get_{name}({recv}) = GC.@preserve x unsafe_load({fp})"
                    ));
                    b.line(&format!(
                        "set_{name}!({recv}, v) = (GC.@preserve x unsafe_store!({fp}, v); x)"
                    ));
                }
                RawFieldKind::Str { delete } => {
                    b.line(&format!(
                        "get_{name}({recv}) = GC.@preserve x native_string(unsafe_load({fp}))"
                    ));
                    b.line(&format!(
                        "set_{name}!({recv}, v::{fty}) = (GC.@preserve x _az_replace!({fp}, v, {delete}); x)"
                    ));
                    b.line(&format!(
                        "set_{name}!({recv}, v::AbstractString) = set_{name}!(x, az_string(v))"
                    ));
                }
                RawFieldKind::Heap { delete, clone } => {
                    if let Some(clone) = clone {
                        b.line(&format!("get_{name}({recv}) = GC.@preserve x {clone}({fp})"));
                    }
                    b.line(&format!(
                        "set_{name}!({recv}, v::{fty}) = (GC.@preserve x _az_replace!({fp}, v, {delete}); x)"
                    ));
                    b.line(&format!("{name}_ptr({recv}) = {fp}"));
                }
                RawFieldKind::Pod => {
                    b.line(&format!(
                        "get_{name}({recv}) = GC.@preserve x unsafe_load({fp})"
                    ));
                    b.line(&format!(
                        "set_{name}!({recv}, v::{fty}) = (GC.@preserve x unsafe_store!({fp}, v); x)"
                    ));
                    b.line(&format!("{name}_ptr({recv}) = {fp}"));
                }
            }
        }
        b.blank();
    }
}
