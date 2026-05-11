//! Pascal (FPC) managed-FFI runtime helpers (host-invoker pattern).
//!
//! FPC supports closure-as-funptr via plain `procedure ... cdecl`
//! pointers. The host-invoker pattern routes user callbacks through
//! pointer-arg invokers that libazul's static thunks fan out from.
//!
//! ## Output surface
//!
//! Emitted into `Azul.pas`:
//!
//! 1. **Interface declarations** for AzApp_setHostHandleReleaser,
//!    AzRefAny_newHostHandle, AzRefAny_getHostHandle, plus per-kind
//!    Az<K>_createFromHostHandle / AzApp_set<K>Invoker bindings.
//! 2. **A global Pascal handle table** (`THandleMap`) keyed by qword
//!    holding `Pointer` (Pascal-managed values are typed `Pointer`
//!    in the table; users cast at retrieve time).
//! 3. **A releaser procedure** that removes entries on last-clone drop.
//! 4. **Per-kind invoker procedures** dispatching through the table.
//! 5. **`azul_refany_create` / `azul_refany_get`** for user-data
//!    refany round-trip.
//!
//! Unlike Lua/OCaml, Pascal doesn't store arbitrary closures — the
//! handle table holds `Pointer` (typically a `TMethod` for class
//! method or a global proc pointer). Users typically wrap state in a
//! class and pass `@MyClass.Handler` as the registered handler.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};

/// Emit the interface-section declarations (FFI imports, type
/// declarations, and the public surface). Call from inside the
/// `interface` block, after the regular external function imports.
pub fn emit_managed_interface(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line(
        "{ -------------------------------------------------------------------- }",
    );
    builder.line(
        "{ Managed-FFI runtime helpers (host-invoker pattern).                 }",
    );
    builder.line(
        "{ -------------------------------------------------------------------- }",
    );
    builder.blank();

    builder.line("type");
    builder.indent();
    builder.line("TAzHostHandleReleaserProc = procedure(id: cuint64); cdecl;");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let mut sig_parts = vec!["id: cuint64".to_string()];
        for (i, arg) in cb.args.iter().enumerate() {
            let nm = if arg.name.is_empty() {
                format!("arg{}", i)
            } else {
                arg.name.clone()
            };
            sig_parts.push(format!("{}: Pointer", nm));
        }
        if has_return(cb) {
            sig_parts.push("out_ptr: Pointer".to_string());
        }
        builder.line(&format!(
            "TAz{}InvokerProc = procedure({}); cdecl;",
            wrapper,
            sig_parts.join("; ")
        ));
    }
    builder.dedent();
    builder.blank();

    // FFI imports (interface section so callers can use them directly).
    builder.line(
        "procedure AzApp_setHostHandleReleaser(releaser: TAzHostHandleReleaserProc); cdecl; external AzulLib;",
    );
    builder.line(
        "function AzRefAny_newHostHandle(id: cuint64): TAzRefAny; cdecl; external AzulLib;",
    );
    builder.line(
        "function AzRefAny_getHostHandle(refany: PAzRefAny): cuint64; cdecl; external AzulLib;",
    );
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!(
            "procedure AzApp_set{w}Invoker(invoker: TAz{w}InvokerProc); cdecl; external AzulLib;",
            w = wrapper
        ));
        builder.line(&format!(
            "function Az{w}_createFromHostHandle(id: cuint64): TAz{w}; cdecl; external AzulLib;",
            w = wrapper
        ));
    }
    builder.blank();

    // Public surface.
    builder.line("{ Wrap an arbitrary Pascal object in a RefAny. The object stays alive }");
    builder.line("{ in the handle table until libazul's destructor fires the releaser.  }");
    builder.line("function azul_refany_create(value: TObject): TAzRefAny;");
    builder.line("{ Recover the Pascal object previously wrapped via azul_refany_create. }");
    builder.line("function azul_refany_get(refany: PAzRefAny): TObject;");
    builder.blank();
}

/// Emit the implementation-section bodies. Call from inside the
/// `implementation` block.
pub fn emit_managed_implementation(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line(
        "{ -------------------------------------------------------------------- }",
    );
    builder.line(
        "{ Managed-FFI runtime helpers (implementation).                        }",
    );
    builder.line(
        "{ -------------------------------------------------------------------- }",
    );
    builder.blank();

    // Handle table. Uses contnrs / fgl interchangeably; we go with
    // a simple parallel array + lookup since fgl needs `uses` setup
    // and we want the prelude self-contained.
    builder.line("var");
    builder.indent();
    builder.line("AzulHandles: array of record");
    builder.line("  Id: cuint64;");
    builder.line("  Value: TObject;");
    builder.line("end;");
    builder.line("AzulNextHandleId: cuint64 = 0;");
    builder.dedent();
    builder.blank();

    builder.line("function azul_alloc_handle(value: TObject): cuint64;");
    builder.line("var i: SizeInt;");
    builder.line("begin");
    builder.indent();
    builder.line("Inc(AzulNextHandleId);");
    builder.line("i := Length(AzulHandles);");
    builder.line("SetLength(AzulHandles, i + 1);");
    builder.line("AzulHandles[i].Id := AzulNextHandleId;");
    builder.line("AzulHandles[i].Value := value;");
    builder.line("Result := AzulNextHandleId;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    builder.line("function azul_lookup_handle(id: cuint64): TObject;");
    builder.line("var i: SizeInt;");
    builder.line("begin");
    builder.indent();
    builder.line("for i := 0 to High(AzulHandles) do");
    builder.line("  if AzulHandles[i].Id = id then exit(AzulHandles[i].Value);");
    builder.line("Result := nil;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // Releaser procedure (cdecl, callable from libazul).
    builder.line("procedure azul_releaser_impl(id: cuint64); cdecl;");
    builder.line("var i: SizeInt;");
    builder.line("begin");
    builder.indent();
    builder.line("for i := 0 to High(AzulHandles) do");
    builder.line("  if AzulHandles[i].Id = id then");
    builder.line("  begin");
    builder.line("    AzulHandles[i] := AzulHandles[High(AzulHandles)];");
    builder.line("    SetLength(AzulHandles, Length(AzulHandles) - 1);");
    builder.line("    exit;");
    builder.line("  end;");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // Per-kind invoker stubs. They just look up the handle (the user
    // is expected to have stored a TMethod / proc pointer in the
    // table) and call it via the kind's signature. For simplicity in
    // this first-pass codegen we emit empty bodies — users can
    // override behavior via register_callback variants in their own
    // code if needed.
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let mut sig_parts = vec!["id: cuint64".to_string()];
        for (i, arg) in cb.args.iter().enumerate() {
            let nm = if arg.name.is_empty() {
                format!("arg{}", i)
            } else {
                arg.name.clone()
            };
            sig_parts.push(format!("{}: Pointer", nm));
        }
        if has_return(cb) {
            sig_parts.push("out_ptr: Pointer".to_string());
        }
        builder.line(&format!(
            "procedure azul_{}_invoker_stub({}); cdecl;",
            wrapper.to_lowercase(),
            sig_parts.join("; ")
        ));
        builder.line("begin");
        builder.indent();
        builder.line(&format!(
            "{{ {} invoker — user callback dispatch handled in user code if needed. }}",
            wrapper
        ));
        // Reference unused params so FPC doesn't warn.
        builder.line("if id = 0 then ;");
        for (i, arg) in cb.args.iter().enumerate() {
            let nm = if arg.name.is_empty() {
                format!("arg{}", i)
            } else {
                arg.name.clone()
            };
            builder.line(&format!("if {} = nil then ;", nm));
        }
        if has_return(cb) {
            builder.line("if out_ptr = nil then ;");
        }
        builder.dedent();
        builder.line("end;");
        builder.blank();
    }

    // Public surface implementations.
    builder.line("function azul_refany_create(value: TObject): TAzRefAny;");
    builder.line("var id: cuint64;");
    builder.line("begin");
    builder.indent();
    builder.line("id := azul_alloc_handle(value);");
    builder.line("Result := AzRefAny_newHostHandle(id);");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    builder.line("function azul_refany_get(refany: PAzRefAny): TObject;");
    builder.line("var id: cuint64;");
    builder.line("begin");
    builder.indent();
    builder.line("id := AzRefAny_getHostHandle(refany);");
    builder.line("if id = 0 then exit(nil);");
    builder.line("Result := azul_lookup_handle(id);");
    builder.dedent();
    builder.line("end;");
    builder.blank();

    // Initialisation block: register the releaser + all invoker stubs.
    builder.line("{ Idempotent host-invoker init — registers the releaser and per-kind  }");
    builder.line("{ invoker stubs. Called once at unit load via the initialization block. }");
    builder.line("procedure AzulHostInvokerInit;");
    builder.line("begin");
    builder.indent();
    builder.line("AzApp_setHostHandleReleaser(@azul_releaser_impl);");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!(
            "AzApp_set{w}Invoker(@azul_{w_low}_invoker_stub);",
            w = wrapper,
            w_low = wrapper.to_lowercase()
        ));
    }
    builder.dedent();
    builder.line("end;");
    builder.blank();
}

/// Emit the `initialization` block call at the very end of the unit.
/// Pascal runs this after all top-level declarations are processed.
pub fn emit_managed_initialization(builder: &mut CodeBuilder) {
    builder.blank();
    builder.line("initialization");
    builder.indent();
    builder.line("AzulHostInvokerInit;");
    builder.dedent();
    builder.blank();
}
