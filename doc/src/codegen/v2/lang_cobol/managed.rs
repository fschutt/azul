//! COBOL (GnuCOBOL) managed-FFI runtime helpers (host-invoker pattern).
//!
//! COBOL is significantly more constrained than Lua/Pascal/Ada/Fortran:
//!
//! * No closures. User callbacks have to be written as standalone
//!   ENTRY paragraphs in the user's PROCEDURE DIVISION.
//! * No struct-by-value RETURNING — GnuCOBOL's CALL ... RETURNING
//!   doesn't accept a TYPEDEF record; callers pass the return slot
//!   by-reference instead.
//!
//! What we can offer from the copybook side:
//!
//! 1. **Level-78 alias constants** for the host-invoker C symbols
//!    so callers write `CALL FN-AZ-APP-SET-HOST-HANDLE-RELEASER`
//!    instead of `CALL "AzApp_setHostHandleReleaser"`.
//! 2. **A documentation block** describing the expected user-side
//!    pattern: a `WS-AZUL-HANDLES` table in WORKING-STORAGE, an
//!    `AZUL-RELEASER` ENTRY paragraph, and per-kind ENTRY
//!    paragraphs registered at program start.
//!
//! The actual handle-table machinery has to live in the user's
//! program, not the copybook — the copybook isn't a runtime, it's a
//! declaration include.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{host_invoker_kinds, wrapper_name};
use super::{cobol_identifier, to_cobol_case};

/// Emit the host-invoker FN-* aliases. Call between
/// `generate_function_constants` and `generate_wrapper_docs`.
pub fn emit_managed_aliases(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("*> ============================================================");
    builder.line("*> Managed-FFI host-invoker level-78 aliases.");
    builder.line("*> ============================================================");
    builder.line("*>");
    builder.line("*> COBOL has no closures and CALL ... RETURNING doesn't accept");
    builder.line("*> a TYPEDEF record, so the host-invoker pattern requires the");
    builder.line("*> user's PROCEDURE DIVISION to provide the handle-table state");
    builder.line("*> and per-kind ENTRY paragraphs. The constants below give");
    builder.line("*> callable names for the libazul-side hooks.");
    builder.line("*>");
    builder.line("*> Suggested user-side scaffolding:");
    builder.line("*>");
    builder.line("*>   WORKING-STORAGE SECTION.");
    builder.line("*>     01  WS-AZUL-NEXT-ID    USAGE BINARY-DOUBLE UNSIGNED VALUE 0.");
    builder.line("*>     01  WS-AZUL-HANDLES.");
    builder.line("*>       05  WS-AZUL-HANDLE OCCURS 256 TIMES.");
    builder.line("*>         10  WS-HANDLE-ID    USAGE BINARY-DOUBLE UNSIGNED.");
    builder.line("*>         10  WS-HANDLE-DATA  USAGE POINTER.");
    builder.line("*>");
    builder.line("*>   PROCEDURE DIVISION.");
    builder.line("*>     ENTRY \"azul-releaser\" USING BY VALUE WS-ID.");
    builder.line("*>         *> linear-scan and remove entry");
    builder.line("*>     ENTRY \"azul-callback\" USING BY VALUE WS-ID, ...");
    builder.line("*>         *> dispatch user callback for the WS-ID handle");
    builder.line("*>");
    builder.line("*>     CALL FN-AZ-APP-SET-HOST-HANDLE-RELEASER");
    builder.line("*>         USING BY VALUE ENTRY \"azul-releaser\".");
    builder.line("*>     CALL FN-AZ-APP-SET-CALLBACK-INVOKER");
    builder.line("*>         USING BY VALUE ENTRY \"azul-callback\".");
    builder.blank();

    emit_one_alias(builder, "AzApp_setHostHandleReleaser");
    emit_one_alias(builder, "AzRefAny_newHostHandle");
    emit_one_alias(builder, "AzRefAny_getHostHandle");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        emit_one_alias(builder, &format!("AzApp_set{}Invoker", wrapper));
        emit_one_alias(builder, &format!("Az{}_createFromHostHandle", wrapper));
    }
    builder.blank();
}

fn emit_one_alias(builder: &mut CodeBuilder, c_name: &str) {
    let cobol_sym = cobol_identifier(&format!("FN-{}", to_cobol_case(c_name)));
    builder.line(&format!(
        "       78  {:<28} VALUE \"{}\".",
        cobol_sym, c_name
    ));
}
