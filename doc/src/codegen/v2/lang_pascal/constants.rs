//! api.json's constant table as a Pascal `const` block.
//!
//! Every constant api.json declares belongs to a class and carries a C
//! literal (`{"type": "u32", "value": "0x0D5B"}`) — today that is the whole
//! OpenGL enum table hanging off `GlContextPtr`. They are plain compile-time
//! values, so Pascal wants a `const` section; only the spelling needs care.
//!
//! # Why the names carry a `Azul` prefix and not the C header's `Az`
//!
//! The C header writes `#define AzGlContextPtr_ACCUM_ALPHA_BITS 0x0D5B`, but
//! Pascal identifiers are CASE-INSENSITIVE and live in one namespace with the
//! `external` declarations. `AzGlContextPtr_VIEWPORT` (the GL enum) and
//! `AzGlContextPtr_viewport` (the `glViewport` export) are then the SAME
//! identifier and the unit does not compile — `CLEAR`/`clear` collides the
//! same way, and a dozen more only escape because the constant has an extra
//! underscore. `Azul` in front cannot collide with anything: every export is
//! `Az<Class>_<method>` and no api.json class starts with `ul`.
//!
//! The bare api.json spelling stays inside the identifier
//! (`AzulGlContextPtr_ACCUM_ALPHA_BITS`), so a reader porting C or Rust code
//! can still search for `ACCUM_ALPHA_BITS`.

use std::collections::BTreeSet;

use super::{super::{generator::CodeBuilder, ir::CodegenIR}, unique_identifier};

/// Emit the `const` block. Call from the interface section, after the type
/// blocks (a `const` section may follow a `type` section and vice versa).
pub fn generate_constants(builder: &mut CodeBuilder, ir: &CodegenIR) {
    if ir.constants.is_empty() {
        return;
    }

    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ api.json constants. Spelled Azul<Class>_<NAME> because Pascal is     }");
    builder.line("{ case-insensitive and AzGlContextPtr_VIEWPORT would BE the            }");
    builder.line("{ AzGlContextPtr_viewport export (see the module comment).             }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();
    builder.line("const");
    builder.indent();

    let mut taken: BTreeSet<String> = BTreeSet::new();
    for constant in &ir.constants {
        for d in &constant.doc {
            builder.line(&format!("{{ {} }}", sanitize_comment(d)));
        }
        let name = unique_identifier(&format!("Azul{}", constant.name), "_", &mut taken);
        builder.line(&format!("{} = {};", name, pascal_literal(&constant.value)));
    }

    builder.dedent();
    builder.blank();
}

/// A C integer literal in Pascal spelling: `0x0D5B` -> `$0D5B`. Anything
/// else (a plain decimal, `0` / `1` for the two `u8` flags) is already
/// valid Pascal and passes through. FPC widens a hex literal that does not
/// fit an Int64 to a QWord on its own, which is what `TIMEOUT_IGNORED`
/// (`0xFFFFFFFFFFFFFFFF`, the one `u64`) needs.
fn pascal_literal(value: &str) -> String {
    let v = value.trim();
    match v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        Some(hex) => format!("${}", hex),
        None => v.to_string(),
    }
}

/// Doc text that cannot open or close a Pascal `{ ... }` comment (same rule
/// as `types::sanitize_comment`).
fn sanitize_comment(s: &str) -> String {
    s.replace('{', "(")
        .replace('}', ")")
        .replace(['\n', '\r'], " ")
}
