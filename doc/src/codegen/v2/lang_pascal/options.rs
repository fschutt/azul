//! Reading an Option or a Result in Pascal.
//!
//! Pascal has no optional type every supported compiler shares (FPC 3.2's
//! `TNullable` is missing from 3.0), so Options and Results keep their
//! variant records and the unit adds one overloaded reader per kind:
//!
//! ```pascal
//! if not AzTryGetSome(Picked.path, Path) then Exit(azDoNothing);
//! ```
//!
//! `AzTryGetSome` / `AzTryGetOk` / `AzTryGetErr` answer whether the record
//! holds that variant and, if it does, copy its payload into the `out`
//! argument. The copy is a view: the record still owns what the payload
//! points to. Building one stays the export's job (`AzOptionString_none`,
//! `AzOptionString_some(..)`).

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, EnumDef},
    },
    record_type_name, sanitize_identifier,
    types::{field_type_for_ref_kind, should_include_enum},
};
use crate::codegen::v2::ir::{EnumVariantKind, FieldRefKind};

/// One reader: its name, the record and payload types, the tag it tests and
/// the payload field it copies.
struct Reader {
    name: &'static str,
    arg: &'static str,
    out: &'static str,
    record: String,
    payload: String,
    tag: usize,
    field: String,
}

impl Reader {
    fn signature(&self) -> String {
        format!(
            "function {}(const {}: {}; out {}: {}): Boolean",
            self.name, self.arg, self.record, self.out, self.payload
        )
    }
}

/// The tag and Pascal payload type of the variant `name`, when it carries
/// exactly one owned value.
fn variant(e: &EnumDef, name: &str, ir: &CodegenIR) -> Option<(usize, String)> {
    let (tag, v) = e.variants.iter().enumerate().find(|(_, v)| v.name == name)?;
    match &v.kind {
        EnumVariantKind::Tuple(types) if types.len() == 1 && types[0].1 == FieldRefKind::Owned => {
            Some((tag, field_type_for_ref_kind(&types[0].0, &types[0].1, ir)))
        }
        _ => None,
    }
}

fn readers(ir: &CodegenIR, config: &CodegenConfig) -> Vec<Reader> {
    let mut out = Vec::new();
    for e in &ir.enums {
        if !should_include_enum(e, config) {
            continue;
        }
        let record = record_type_name(&e.name);
        let mut push = |name, arg, out_name, variant_name: &str| {
            if let Some((tag, payload)) = variant(e, variant_name, ir) {
                out.push(Reader {
                    name,
                    arg,
                    out: out_name,
                    record: record.clone(),
                    payload,
                    tag,
                    field: format!("Payload_{}", sanitize_identifier(variant_name)),
                });
            }
        };
        if ir.option_payload(&e.name).is_some() {
            push("AzTryGetSome", "O", "V", "Some");
        } else if ir.result_payloads(&e.name).is_some() {
            push("AzTryGetOk", "R", "V", "Ok");
            push("AzTryGetErr", "R", "E", "Err");
        }
    }
    out
}

/// The interface half: one declaration per reader, under a note on how to
/// use them.
pub fn generate_option_interface(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let readers = readers(ir, config);
    if readers.is_empty() {
        return;
    }
    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Options and results. AzTryGetSome(O, V) is True when O holds a value }");
    builder.line("{ and copies it into V; AzTryGetOk / AzTryGetErr do the same for the   }");
    builder.line("{ two sides of a result. V is a view: the record still owns the value. }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();
    for r in &readers {
        builder.line(&format!("{}; overload; inline;", r.signature()));
    }
    builder.blank();
}

/// The implementation half: each reader tests the tag and copies the payload.
pub fn generate_option_implementation(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    for r in readers(ir, config) {
        builder.line(&format!("{};", r.signature()));
        builder.line("begin");
        builder.indent();
        builder.line(&format!("Result := {}.Tag = {};", r.arg, r.tag));
        builder.line(&format!("if Result then {} := {}.{};", r.out, r.arg, r.field));
        builder.dedent();
        builder.line("end;");
        builder.blank();
    }
}
