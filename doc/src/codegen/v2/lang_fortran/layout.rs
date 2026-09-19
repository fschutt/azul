//! The C-ABI layout of IR types lives in the shared `codegen::v2::c_layout`;
//! this module adds the Fortran spelling of an opaque blob of that layout.

pub(crate) use super::super::c_layout::{mono_layout, type_layout, AbiLayout};

/// Emit the single Fortran component declaration for an ABI-opaque blob of
/// the given layout: an array of the widest integer kind matching the
/// alignment. `size` is always a multiple of `align` for C aggregates.
pub(crate) fn blob_field_decl(l: AbiLayout) -> String {
    match l.align {
        8 => format!("integer(c_int64_t) :: opaque_({})", l.size / 8),
        4 => format!("integer(c_int32_t) :: opaque_({})", l.size / 4),
        2 => format!("integer(c_int16_t) :: opaque_({})", l.size / 2),
        _ => format!("integer(c_int8_t) :: opaque_({})", l.size),
    }
}
