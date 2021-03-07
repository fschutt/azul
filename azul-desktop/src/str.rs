// extra string functions intended for C development

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum FmtValue {
    Bool(bool)
    Uchar(u8)
    Schar(i8)
    Ushort(u16)
    Sshort(i16)
    Uint(u32)
    Sint(i32)
    Ulong(u64)
    Slong(i64)
    Isize(isize)
    Usize(usize)
    Float(f32)
    Double(f64)
    Str(AzString)
    StrVec(AzStringVec)
}

impl fmt::Display for FmtValue {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            FmtValue::Bool(v) => v.fmt(f),
            FmtValue::Uchar(v) => v.fmt(f),
            FmtValue::Schar(v) => v.fmt(f),
            FmtValue::Ushort(v) => v.fmt(f),
            FmtValue::Sshort(v) => v.fmt(f),
            FmtValue::Uint(v) => v.fmt(f),
            FmtValue::Sint(v) => v.fmt(f),
            FmtValue::Ulong(v) => v.fmt(f),
            FmtValue::Slong(v) => v.fmt(f),
            FmtValue::Isize(v) => v.fmt(f),
            FmtValue::Usize(v) => v.fmt(f),
            FmtValue::Float(v) => v.fmt(f),
            FmtValue::Double(v) => v.fmt(f),
            FmtValue::Str(v) => v.as_str().fmt(f),
            FmtValue::StrVec(sv) => {
                let vec: Vec<&str> = sv.as_ref().iter().map(|s| s.as_str()).collect();
                vec.fmt(f)
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FmtArg {
    pub key: AzString,
    pub value: FmtValue
}

impl_vec!(FmtArg, FmtArgVec, FmtArgVecDestructor);
impl_vec_clone!(FmtArg, FmtArgVec, FmtArgVecDestructor);

pub fn fmt_string(format: AzString, args: FmtArgVec) -> AzString {
    use strfmt::Format;
    let format_map = args.iter().map(|a| (a.key, a.value)).collect();
    match format.as_str().format(&format_map) {
        Ok(o) => o,
        Err(e) => format!("{}", e),
    }
}