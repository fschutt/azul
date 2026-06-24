//! C-compatible string formatting via `strfmt`.
//!
//! Provides [`FmtValue`], [`FmtArg`], and [`FmtArgVec`] for passing
//! heterogeneous format arguments across FFI, and [`fmt_string`] as the
//! main entry point. Used by `fluent.rs` and `icu.rs` for localization.

use std::fmt;

use azul_css::{AzString, StringVec, impl_option, impl_option_inner};

/// A format argument value that can hold any primitive type or string.
/// Used in [`FmtArg`] to pass typed values into `strfmt`-based formatting.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum FmtValue {
    Bool(bool),
    Uchar(u8),
    Schar(i8),
    Ushort(u16),
    Sshort(i16),
    Uint(u32),
    Sint(i32),
    Ulong(u64),
    Slong(i64),
    Isize(isize),
    Usize(usize),
    Float(f32),
    Double(f64),
    Str(AzString),
    StrVec(StringVec),
}

impl strfmt::DisplayStr for FmtValue {
    fn display_str(&self, f: &mut strfmt::Formatter<'_, '_>) -> strfmt::Result<()> {
        use strfmt::DisplayStr;
        match self {
            Self::Bool(v) => format!("{v}").display_str(f),
            Self::Uchar(v) => v.display_str(f),
            Self::Schar(v) => v.display_str(f),
            Self::Ushort(v) => v.display_str(f),
            Self::Sshort(v) => v.display_str(f),
            Self::Uint(v) => v.display_str(f),
            Self::Sint(v) => v.display_str(f),
            Self::Ulong(v) => v.display_str(f),
            Self::Slong(v) => v.display_str(f),
            Self::Isize(v) => v.display_str(f),
            Self::Usize(v) => v.display_str(f),
            Self::Float(v) => v.display_str(f),
            Self::Double(v) => v.display_str(f),
            Self::Str(v) => v.as_str().display_str(f),
            Self::StrVec(sv) => {
                "[".display_str(f)?;
                for (i, s) in sv.as_ref().iter().enumerate() {
                    if i != 0 {
                        ", ".display_str(f)?;
                    }
                    s.as_str().display_str(f)?;
                }
                "]".display_str(f)?;
                Ok(())
            }
        }
    }
}

impl fmt::Display for FmtValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::Bool(v) => v.fmt(f),
            Self::Uchar(v) => v.fmt(f),
            Self::Schar(v) => v.fmt(f),
            Self::Ushort(v) => v.fmt(f),
            Self::Sshort(v) => v.fmt(f),
            Self::Uint(v) => v.fmt(f),
            Self::Sint(v) => v.fmt(f),
            Self::Ulong(v) => v.fmt(f),
            Self::Slong(v) => v.fmt(f),
            Self::Isize(v) => v.fmt(f),
            Self::Usize(v) => v.fmt(f),
            Self::Float(v) => v.fmt(f),
            Self::Double(v) => v.fmt(f),
            Self::Str(v) => v.as_str().fmt(f),
            Self::StrVec(sv) => {
                use std::fmt::Debug;
                let vec: Vec<&str> = sv.as_ref().iter().map(AzString::as_str).collect();
                vec.fmt(f)
            }
        }
    }
}

/// A key-value pair mapping a format placeholder name to its value.
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FmtArg {
    pub key: AzString,
    pub value: FmtValue,
}

azul_css::impl_option!(FmtArg, OptionFmtArg, copy = false, [Debug, Clone, PartialEq, PartialOrd]);
azul_css::impl_vec!(FmtArg, FmtArgVec, FmtArgVecDestructor, FmtArgVecDestructorType, FmtArgVecSlice, OptionFmtArg);
azul_css::impl_vec_clone!(FmtArg, FmtArgVec, FmtArgVecDestructor);
azul_css::impl_vec_debug!(FmtArg, FmtArgVec);
azul_css::impl_vec_partialeq!(FmtArg, FmtArgVec);
azul_css::impl_vec_partialord!(FmtArg, FmtArgVec);

/// Formats `format` by substituting placeholders with values from `args`.
/// Returns the error message as a string on failure (for C FFI ergonomics).
// FFI-exported formatter: owned AzString/FmtArgVec args are the api.json signature.
#[allow(clippy::needless_pass_by_value)]
#[must_use] pub fn fmt_string(format: AzString, args: FmtArgVec) -> String {
    use strfmt::Format;
    let format_map = args
        .iter()
        .map(|a| (a.key.clone().into_library_owned_string(), a.value.clone()))
        .collect();
    match format.as_str().format(&format_map) {
        Ok(o) => o,
        Err(e) => format!("{e}"),
    }
}