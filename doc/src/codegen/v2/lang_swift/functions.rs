//! Swift idiomatic procedure aliases.
//!
//! Every exported libazul symbol is already visible through the imported
//! `CAzul` module under its raw `Az*` name. To mirror the Odin backend's
//! ergonomic layer, we additionally bind each function to the same name
//! without the `Az` prefix (`public let App_create = AzApp_create`). A
//! reference to an imported C function is a first-class Swift value, so
//! the alias is callable exactly like the raw symbol
//! (`App_create(data, config)`), while `AzApp_create` stays available.

use std::collections::BTreeSet;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::{sanitize_identifier, should_emit_function};

pub fn generate_aliases(
    b: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
    emitted: &mut BTreeSet<String>,
) {
    b.line("// ----------------------------------------------------------------------------");
    b.line("// Idiomatic aliases: the same procedures without the `Az` prefix. The raw");
    b.line("// `Az*` symbols remain available through the imported `CAzul` module.");
    b.line("// ----------------------------------------------------------------------------");

    let mut seen: BTreeSet<String> = BTreeSet::new();
    for func in &ir.functions {
        if !should_emit_function(func, ir, config) {
            continue;
        }
        if !seen.insert(func.c_name.clone()) {
            continue;
        }
        let short = match func.c_name.strip_prefix("Az") {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };
        // Guard against a short name colliding with an already-emitted
        // alias (cannot happen for unique c_names, but keeps the output
        // provably redeclaration-free).
        let alias = sanitize_identifier(short);
        if !emitted.insert(alias.clone()) {
            continue;
        }
        b.line(&format!("public let {} = {}", alias, func.c_name));
    }
    b.blank();
}
