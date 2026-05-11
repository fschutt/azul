//! Perl (FFI::Platypus) managed-FFI runtime helpers (host-invoker pattern).
//!
//! FFI::Platypus 2.x supports closures via `$ffi->closure(sub { ... })`
//! returning a pinned `FFI::Platypus::Closure` whose lifetime keeps the
//! Perl sub alive. The host-invoker pattern routes user callbacks
//! through pointer-arg invokers so the libffi closure cast is always
//! legal; libazul's static thunks handle the by-value plumbing.
//!
//! ## Output surface
//!
//! Emitted into `Azul.pm`:
//!
//! 1. **`$ffi->attach`** declarations for AzApp_setHostHandleReleaser,
//!    AzRefAny_newHostHandle, AzRefAny_getHostHandle, and per-kind
//!    AzApp_set<K>Invoker / Az<K>_createFromHostHandle.
//! 2. **A Perl-side handle table** (`%_handles` package hash keyed by
//!    integer id).
//! 3. **A pinned releaser closure** registered at module load.
//! 4. **Per-kind invoker closures** dispatching through the table.
//! 5. **`Azul::register_callback(kind, sub)`** + **`Azul::refany_create($value)`**
//!    + **`Azul::refany_get($refany)`** public surface.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{has_return, host_invoker_kinds, wrapper_name};

/// Emit the managed-FFI prelude. Call AFTER `emit_attach_functions`
/// so the regular C-ABI bindings are already set up.
pub fn emit_managed_prelude(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("# ============================================================");
    builder.line("# Managed-FFI runtime helpers (host-invoker pattern).");
    builder.line("# ============================================================");
    builder.line("#");
    builder.line("# libazul exports per callback kind:");
    builder.line("#   * a static thunk (the `cb` field of the callback wrapper),");
    builder.line("#   * Az<Kind>_createFromHostHandle(u64) -> Az<Kind> constructor,");
    builder.line("#   * AzApp_set<Kind>Invoker(fn) setter.");
    builder.line("#");
    builder.line("# We pin one libffi closure per kind at module load (pointer-arg");
    builder.line("# signatures only; by-value plumbing happens inside libazul's");
    builder.line("# static thunks). User callbacks live in a Perl hash keyed by");
    builder.line("# integer id; the RefAny destructor calls back through");
    builder.line("# AzApp_setHostHandleReleaser to clear the entry.");
    builder.line("# ============================================================");
    builder.blank();

    // Foreign attachments for the host-invoker C exports.
    builder.line("$Azul::ffi->attach('AzApp_setHostHandleReleaser' => ['opaque'] => 'void');");
    builder.line(
        "$Azul::ffi->attach('AzRefAny_newHostHandle' => ['uint64'] => 'AzRefAny');",
    );
    // AzRefAny_getHostHandle takes a const AzRefAny*. Platypus passes
    // a record by-pointer when the arg type is the record name
    // itself, so this works whether the user supplies the record
    // value or its address.
    builder.line(
        "$Azul::ffi->attach('AzRefAny_getHostHandle' => ['AzRefAny'] => 'uint64');",
    );

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!(
            "$Azul::ffi->attach('AzApp_set{w}Invoker' => ['opaque'] => 'void');",
            w = wrapper
        ));
        builder.line(&format!(
            "$Azul::ffi->attach('Az{w}_createFromHostHandle' => ['uint64'] => 'Az{w}');",
            w = wrapper
        ));
    }
    builder.blank();

    // Handle table + pinned closures live in the `Azul` package
    // namespace (caller code already does `use Azul;`).
    builder.line("our %_handles;");
    builder.line("our $_next_handle_id = 0;");
    builder.line("our @_live_pins;     # keep Platypus closures alive for process lifetime");
    builder.blank();

    builder.line("sub Azul::_alloc_handle {");
    builder.indent();
    builder.line("my ($value) = @_;");
    builder.line("$_next_handle_id++;");
    builder.line("$_handles{$_next_handle_id} = $value;");
    builder.line("return $_next_handle_id;");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Releaser closure — pinned in @_live_pins.
    builder.line("{");
    builder.indent();
    builder.line("my $releaser = $Azul::ffi->closure(sub {");
    builder.indent();
    builder.line("my ($id) = @_;");
    builder.line("delete $_handles{$id};");
    builder.dedent();
    builder.line("});");
    builder.line("push @_live_pins, $releaser;");
    builder.line("Azul::FFI::AzApp_setHostHandleReleaser($releaser);");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // Per-kind invoker closures.
    for cb in host_invoker_kinds(ir) {
        emit_invoker(builder, cb);
    }

    // Public surface.
    builder.line("# Wrap an arbitrary Perl value (any scalar / ref) in an AzRefAny.");
    builder.line("# The value lives in %_handles until libazul's destructor fires the");
    builder.line("# releaser (last clone drop) — only then is the entry deleted.");
    builder.line("sub Azul::refany_create {");
    builder.indent();
    builder.line("my ($value) = @_;");
    builder.line("my $id = Azul::_alloc_handle($value);");
    builder.line("return Azul::FFI::AzRefAny_newHostHandle($id);");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("# Recover the Perl value previously wrapped via refany_create. Pass");
    builder.line("# a pointer to an AzRefAny (Perl-side: scalar holding the address).");
    builder.line("sub Azul::refany_get {");
    builder.indent();
    builder.line("my ($refany_ptr) = @_;");
    builder.line("my $id = Azul::FFI::AzRefAny_getHostHandle($refany_ptr);");
    builder.line("return undef if $id == 0;");
    builder.line("return $_handles{$id};");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // register_callback dispatch — accepts a wrapper name + a sub
    // reference, returns the Az<Kind> struct.
    builder.line("# Wrap a Perl sub in the matching Az<Kind> cdata struct. KIND is the");
    builder.line("# wrapper type name ('Callback', 'LayoutCallback', ...). The Perl sub");
    builder.line("# is stored in %_handles; libazul's invoker fires it via the static thunk.");
    builder.line("sub Azul::register_callback {");
    builder.indent();
    builder.line("my ($kind, $sub) = @_;");
    builder.line("return undef unless defined $sub;");
    builder.line("my $id = Azul::_alloc_handle($sub);");
    let mut first = true;
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let prefix = if first { "if" } else { "elsif" };
        builder.line(&format!(
            "{} ($kind eq '{}') {{ return Azul::FFI::Az{}_createFromHostHandle($id); }}",
            prefix, wrapper, wrapper
        ));
        first = false;
    }
    builder.line("else { die \"Azul::register_callback: unknown kind '$kind'\"; }");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

fn emit_invoker(builder: &mut CodeBuilder, cb: &super::super::ir::CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let n_args = cb.args.len();
    let has_ret = has_return(cb);

    // Build the Platypus signature: ['uint64', 'opaque', ..., 'opaque']
    // (id + pointer args + optional out-pointer).
    let mut sig_parts = vec!["'uint64'".to_string()];
    for _ in 0..n_args {
        sig_parts.push("'opaque'".to_string());
    }
    if has_ret {
        sig_parts.push("'opaque'".to_string());
    }

    builder.line("{");
    builder.indent();
    builder.line(&format!(
        "my $invoker = $Azul::ffi->closure(sub {{ # {} invoker",
        wrapper
    ));
    builder.indent();
    let user_args_list: Vec<String> =
        (0..n_args).map(|i| format!("$_[{}]", i + 1)).collect();
    builder.line(&format!("my $id = $_[0];"));
    builder.line("my $sub = $_handles{$id};");
    builder.line("return unless defined $sub;");
    if has_ret {
        builder.line(&format!(
            "my $ret = eval {{ $sub->({}) }};",
            user_args_list.join(", ")
        ));
        builder.line(&format!(
            "if ($@) {{ warn \"[azul] {} invoker error: $@\"; return; }}",
            wrapper
        ));
        builder.line("# out_ptr write left to the user closure — non-primitive returns need");
        builder.line("# struct marshalling we don't surface from a generic invoker.");
        builder.line("return;");
    } else {
        builder.line(&format!(
            "eval {{ $sub->({}) }};",
            user_args_list.join(", ")
        ));
        builder.line(&format!(
            "warn \"[azul] {} invoker error: $@\" if $@;",
            wrapper
        ));
    }
    builder.dedent();
    builder.line(&format!("}}, [{}] => 'void');", sig_parts.join(", ")));
    builder.line("push @_live_pins, $invoker;");
    builder.line(&format!(
        "Azul::FFI::AzApp_set{}Invoker($invoker);",
        wrapper
    ));
    builder.dedent();
    builder.line("}");
    builder.blank();
}
