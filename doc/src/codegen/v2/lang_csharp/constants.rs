//! api.json constant emission for the C# generator.
//!
//! Every `ConstantDef` is `<Class>_<NAME>` with a literal value (the GL
//! enum values on `GlContextPtr` today). They become `public const` fields
//! of a `<Class>Constants` static class, so user code writes
//! `GlContextPtrConstants.ACCUM_ALPHA_BITS` rather than re-typing `0x0D5B`.
//!
//! A sibling class rather than members of the wrapper class: the owning
//! class does not always get a wrapper (the shared rule needs a `_delete`
//! or a method), and a constant must not silently disappear when it
//! doesn't.

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{CodegenIR, ConstantDef},
    },
    map_type_to_csharp, sanitize_identifier,
};

pub fn generate_constants(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    // Group by owning class, keeping the IR's (api.json) order in both
    // dimensions so the output is stable across runs.
    let mut by_class: Vec<(String, Vec<&ConstantDef>)> = Vec::new();
    for c in &ir.constants {
        let (class, _) = split_owner(c);
        if !config.should_include_type(class) {
            continue;
        }
        match by_class.iter().position(|(k, _)| k.as_str() == class) {
            Some(i) => by_class[i].1.push(c),
            None => by_class.push((class.to_string(), vec![c])),
        }
    }
    if by_class.is_empty() {
        return Ok(());
    }

    builder.line("// --------------------------------------------------------------------------");
    builder.line("// api.json constants, one static class per owning API class.");
    builder.line("// --------------------------------------------------------------------------");
    builder.blank();

    for (class, constants) in by_class {
        builder.line(&format!(
            "/// <summary>The constants api.json declares on {}.</summary>",
            class
        ));
        builder.line(&format!(
            "public static class {}Constants",
            sanitize_identifier(&class)
        ));
        builder.line("{");
        builder.indent();
        for c in constants {
            for d in &c.doc {
                builder.line(&format!("/// <summary>{}</summary>", xml_escape(d)));
            }
            let (_, name) = split_owner(c);
            builder.line(&format!(
                "public const {} {} = {};",
                map_type_to_csharp(&c.type_name, ir),
                sanitize_identifier(&name),
                c.value.trim()
            ));
        }
        builder.dedent();
        builder.line("}");
        builder.blank();
    }

    Ok(())
}

/// `GlContextPtr_ACCUM_ALPHA_BITS` -> `("GlContextPtr", "ACCUM_ALPHA_BITS")`.
/// A name without the owning class keeps its full spelling and lands in a
/// class of its own module's name.
fn split_owner(c: &ConstantDef) -> (&str, String) {
    let owner = c.name.split_once('_').map_or(c.module.as_str(), |(o, _)| o);
    (owner, c.member_name())
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
