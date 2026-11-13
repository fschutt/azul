//! Reading an Option or a Result in Fortran.
//!
//! Fortran has no union, so every tagged union is an opaque blob of the
//! right size (see [`super::types`]), and Fortran has no optional value
//! type. Building one goes through the exports (`az_option_string_none()`,
//! `az_option_string_some(..)`); reading one goes through three generic
//! functions this module emits into `azul_api`:
//!
//! ```fortran
//! if (.not. azul_try_get_some(picked, opened)) return
//! ```
//!
//! `azul_try_get_some(o, v)` is `.true.` when `o` holds a value and copies
//! it into `v`; `azul_try_get_ok(res, v)` / `azul_try_get_err(res, e)` do
//! the same for the two sides of a Result. The copy is a view: the blob
//! still owns what the payload points to. The specific functions are
//! private, only the generic names are public.

use super::{
    super::{
        c_layout::union_payload_layout,
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, EnumDef, EnumVariantKind, FieldRefKind},
    },
    ffi_type_name, pascal_to_snake_case, truncate_identifier,
    types::{field_type_for_ref_kind, should_include_enum, should_include_struct},
};

/// One specific reader.
struct Reader {
    generic: &'static str,
    name: String,
    /// The union's Fortran type (`type(AzOptionFilePath)`).
    union: String,
    /// The dummy names: the union, the payload copy.
    arg: &'static str,
    out: &'static str,
    /// The payload's Fortran type (`type(AzFilePath)`, `integer(c_int32_t)`).
    payload: String,
    tag: usize,
    /// `integer(c_int8_t)` for a `repr(C, u8)` tag, `integer(c_int)` otherwise.
    tag_kind: &'static str,
    offset: usize,
}

const GENERICS: [&str; 3] = ["azul_try_get_some", "azul_try_get_ok", "azul_try_get_err"];

/// Whether `ty` has a Fortran declaration in the binding.
fn payload_declared(ty: &str, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if let Some(s) = ir.find_struct(ty) {
        return should_include_struct(s, config);
    }
    if let Some(e) = ir.find_enum(ty) {
        return should_include_enum(e, config);
    }
    super::super::c_layout::type_layout(ty, ir).is_some() && !ty.contains('*') && !ty.contains('&')
        && ir.find_type_alias(ty).is_none()
}

fn readers(ir: &CodegenIR, config: &CodegenConfig) -> Vec<Reader> {
    let mut out = Vec::new();
    let mut push = |e: &EnumDef, generic: &'static str, short: &str, arg, out_name, variant: &str| {
        let Some((tag, v)) = e.variants.iter().enumerate().find(|(_, v)| v.name == variant) else {
            return;
        };
        let EnumVariantKind::Tuple(types) = &v.kind else { return };
        let [(ty, FieldRefKind::Owned)] = types.as_slice() else { return };
        if !payload_declared(ty, ir, config) {
            return;
        }
        let Some(layout) = union_payload_layout(&e.name, ir) else { return };
        out.push(Reader {
            generic,
            name: truncate_identifier(&format!("azul_{}_{}", short, pascal_to_snake_case(&e.name))),
            union: format!("type({})", truncate_identifier(&ffi_type_name(&e.name))),
            arg,
            out: out_name,
            payload: field_type_for_ref_kind(ty, &FieldRefKind::Owned, ir),
            tag,
            tag_kind: if layout.tag.size == 1 { "integer(c_int8_t)" } else { "integer(c_int)" },
            offset: layout.payload_offset,
        });
    };
    for e in &ir.enums {
        if !e.is_union || !should_include_enum(e, config) {
            continue;
        }
        if ir.option_payload(&e.name).is_some() {
            push(e, GENERICS[0], "tgs", "o", "v", "Some");
        } else if ir.result_payloads(&e.name).is_some() {
            push(e, GENERICS[1], "tgo", "res", "v", "Ok");
            push(e, GENERICS[2], "tge", "res", "e", "Err");
        }
    }
    out
}

/// Before `contains`: the three generic interfaces, the generic names
/// public and the specific ones private.
pub(crate) fn emit_option_decls(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let readers = readers(ir, config);
    if readers.is_empty() {
        return;
    }
    // The per-module facades re-export these from `azul_option`.
    b.line(&format!("{}option]", super::API_MODULE_MARKER));
    b.line("! Reading an Option or a Result: azul_try_get_some(o, v) is .true. when o");
    b.line("! holds a value and copies it into v; azul_try_get_ok(res, v) and");
    b.line("! azul_try_get_err(res, e) do the same for a Result. v is a view: the");
    b.line("! option still owns what it points to.");
    for g in GENERICS {
        b.line(&format!("interface {}", g));
        b.indent();
        for r in readers.iter().filter(|r| r.generic == g) {
            b.line(&format!("module procedure {}", r.name));
        }
        b.dedent();
        b.line(&format!("end interface {}", g));
    }
    for g in GENERICS {
        b.line(&format!("public :: {}", g));
    }
    for r in &readers {
        b.line(&format!("private :: {}", r.name));
    }
    b.blank();
}

/// After `contains`: one function per reader. The tag is the blob's first
/// field and the payload sits at the union's payload offset.
pub(crate) fn emit_option_bodies(b: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    for r in readers(ir, config) {
        b.line(&format!("function {}({}, {}) result(found)", r.name, r.arg, r.out));
        b.indent();
        b.line(&format!("{}, intent(in), target :: {}", r.union, r.arg));
        b.line(&format!("{}, intent(out) :: {}", r.payload, r.out));
        b.line("logical :: found");
        b.line(&format!("{}, pointer :: azul_tag", r.tag_kind));
        b.line(&format!("{}, pointer :: azul_payload", r.payload));
        b.line(&format!("call c_f_pointer(c_loc({}), azul_tag)", r.arg));
        b.line(&format!("found = azul_tag == {}", r.tag));
        b.line("if (.not. found) return");
        b.line(&format!(
            "call c_f_pointer(transfer(transfer(c_loc({}), 0_c_intptr_t) + {}_c_intptr_t, c_null_ptr), azul_payload)",
            r.arg, r.offset
        ));
        b.line(&format!("{} = azul_payload", r.out));
        b.dedent();
        b.line(&format!("end function {}", r.name));
        b.blank();
    }
}
