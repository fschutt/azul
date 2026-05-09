//! PHP managed-FFI runtime helpers (host-invoker pattern).
//!
//! PHP's built-in `FFI` extension is a libffi binding, which means
//! `FFI::cast()` of a PHP closure to a typedef whose arguments are
//! aggregate-by-value silently produces a junk pointer — exactly the
//! constraint the host-invoker pattern works around. Our invokers have
//! pointer args throughout, so libffi closures are safe.
//!
//! Two pieces emitted into the generated `Azul.php`:
//!
//! 1. **cdef extension** — the host-invoker C-ABI declarations
//!    (releaser, RefAny new/get host-handle, per-kind invoker setters
//!    and `_createFromHostHandle` constructors) appended to the
//!    `Azul::CDEF` heredoc, so PHP's FFI parser sees them on the first
//!    `FFI::cdef(...)` call.
//! 2. **`Azul` class additions** — static `$_handles` storage, lazy
//!    `ensureHostInvokerInit()` that pins one closure per kind via
//!    `$ffi->cast(...)`, public `registerCallback($kind, $fn)` factory
//!    that returns the matching `Az<Kind>` cdata struct, and
//!    `refanyCreate($value)` / `refanyGet($refany)` user-data helpers
//!    that share the same id-keyed table.
//!
//! ## How user code consumes this
//!
//! ```php
//! $data = Azul::refanyCreate(['counter' => 5]);
//!
//! $onClick = function ($data, $info) {
//!     $m = Azul::refanyGet($data);
//!     if ($m === null) { return Azul::lib()->AzUpdate_DoNothing; }
//!     $m['counter']++;
//!     return Azul::lib()->AzUpdate_RefreshDom;
//! };
//!
//! $cb = Azul::registerCallback('Callback', $onClick);
//! $button->setOnClick(Azul::lib()->AzRefAny_clone($data), $cb);
//! ```
//!
//! Once `lang_php/wrappers.rs` learns to substitute callback args via
//! `Azul::registerCallback` automatically, the explicit `$cb =` step
//! disappears and the hello-world looks like Lua's.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{
    emit_cdef_block, has_return, host_invoker_kinds, wrapper_name,
};

/// Build the additional cdef text to append to `Azul::CDEF`. The result
/// includes leading whitespace so it inserts cleanly after the main
/// payload and before the `CDEF;` terminator.
pub fn cdef_extension(ir: &CodegenIR) -> String {
    let mut out = String::new();
    out.push_str("\n");
    out.push_str("/* ──────────────────────────────────────────────────────────── */\n");
    out.push_str("/* Managed-FFI runtime: host-invoker C-ABI exports.            */\n");
    out.push_str("/* Languages with declarative FFI bindings (ruby-ffi, JNA,     */\n");
    out.push_str("/* P/Invoke) translate these by hand; PHP's FFI parser eats    */\n");
    out.push_str("/* plain C declarations, so we splice them in here verbatim.   */\n");
    out.push_str("/* ──────────────────────────────────────────────────────────── */\n");
    emit_cdef_block(&mut out, ir);
    out
}

/// Emit the host-invoker plumbing INSIDE `final class Azul { ... }`.
///
/// Call order in `mod.rs`:
///   1. The existing `cdef()` accessor.
///   2. **This function** — adds storage + ensureHostInvokerInit +
///      registerCallback + refanyCreate / refanyGet.
///   3. Closing `}` of the class.
pub fn emit_azul_class_members(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.line("// Managed-FFI runtime helpers (host-invoker pattern)");
    builder.line("//");
    builder.line("// libazul exports per supported callback kind:");
    builder.line("//   * a static thunk (the `cb` field of the callback wrapper),");
    builder.line("//   * Az<Kind>_createFromHostHandle(u64) -> Az<Kind> constructor,");
    builder.line("//   * AzApp_set<Kind>Invoker(fn) setter.");
    builder.line("//");
    builder.line("// We pin one closure per kind via FFI::cast() at module load — pointer-");
    builder.line("// arg signatures only, so libffi handles the cast without aggregate");
    builder.line("// trampolines. Per-callback registration goes through a PHP id→callable");
    builder.line("// map (`Azul::$_handles`). The framework's RefAny destructor calls");
    builder.line("// back through `AzApp_setHostHandleReleaser` so we drop the entry on");
    builder.line("// last-clone collection.");
    builder.line("// ────────────────────────────────────────────────────────────────");
    builder.blank();

    // Storage: static + public so the FFI::cast'd closures can write through
    // `Azul::$_handles[$id]`. (Closures cast at module load have no `$this`
    // binding to private state.)
    builder.line("/** Per-process callback / user-data table keyed by host-handle id. */");
    builder.line("public static array $_handles = [];");
    builder.line("/** Monotonic id allocator. Starts at 1; 0 is reserved as `no value`. */");
    builder.line("private static int $_nextHandleId = 0;");
    builder.line("/** Pinned cdata closures (releaser + per-kind invokers). */");
    builder.line("private static array $_livePins = [];");
    builder.line("/** True once `ensureHostInvokerInit()` has registered everything. */");
    builder.line("private static bool $_invokersWired = false;");
    builder.blank();

    // ensureHostInvokerInit: lazy registration of releaser + per-kind invokers.
    builder.line("/**");
    builder.line(" * Register the releaser and per-kind invoker closures with libazul.");
    builder.line(" * Idempotent: the first call to `registerCallback()` or `refanyCreate()`");
    builder.line(" * triggers it. Subsequent calls are a no-op.");
    builder.line(" */");
    builder.line("private static function ensureHostInvokerInit(): void");
    builder.line("{");
    builder.indent();
    builder.line("if (self::$_invokersWired) { return; }");
    builder.line("self::$_invokersWired = true;");
    builder.line("$ffi = self::lib();");
    builder.blank();

    builder.line("// Releaser: framework calls this with the host-handle id when the");
    builder.line("// last RefAny clone tied to that id is dropped. We unset our hash entry.");
    builder.line("$releaser = $ffi->cast(");
    builder.indent();
    builder.line("'void(*)(uint64_t)',");
    builder.line("static function (int $id): void { unset(Azul::$_handles[$id]); }");
    builder.dedent();
    builder.line(");");
    builder.line("self::$_livePins[] = $releaser;");
    builder.line("$ffi->AzApp_setHostHandleReleaser($releaser);");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker(builder, cb);
    }

    builder.dedent();
    builder.line("}");
    builder.blank();

    // registerCallback: dispatch table built from the IR.
    builder.line("/**");
    builder.line(" * Wrap a PHP callable into the matching `Az<Kind>` cdata struct so a");
    builder.line(" * native call site (e.g. `Button::setOnClick(...)`) can store it.");
    builder.line(" *");
    builder.line(" * @param string $kind  Wrapper name — `'Callback'`, `'LayoutCallback'`,");
    builder.line(" *                       `'VirtualViewCallback'`, etc.");
    builder.line(" * @param callable $fn  User callback; signature matches the kind.");
    builder.line(" * @return \\FFI\\CData  A cdata struct of FFI type `Az<Kind>`.");
    builder.line(" */");
    builder.line("public static function registerCallback(string $kind, callable $fn): \\FFI\\CData");
    builder.line("{");
    builder.indent();
    builder.line("self::ensureHostInvokerInit();");
    builder.line("self::$_nextHandleId++;");
    builder.line("$id = self::$_nextHandleId;");
    builder.line("self::$_handles[$id] = $fn;");
    builder.line("$ffi = self::lib();");
    builder.line("switch ($kind) {");
    builder.indent();
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        builder.line(&format!("case '{}':", wrapper));
        builder.indent();
        builder.line(&format!(
            "return $ffi->Az{}_createFromHostHandle($id);",
            wrapper
        ));
        builder.dedent();
    }
    builder.line("default:");
    builder.indent();
    builder.line("throw new \\InvalidArgumentException(");
    builder.indent();
    builder.line("\"Azul::registerCallback: unknown kind '\".$kind.\"'\"");
    builder.dedent();
    builder.line(");");
    builder.dedent();
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();

    // refanyCreate / refanyGet: user-data helpers on the same id-keyed path.
    builder.line("/**");
    builder.line(" * Wrap an arbitrary PHP value in an AzRefAny. The value lives in the");
    builder.line(" * shared id-keyed table; the destructor clears it on last-clone drop.");
    builder.line(" *");
    builder.line(" * @param mixed $value");
    builder.line(" */");
    builder.line("public static function refanyCreate($value): \\FFI\\CData");
    builder.line("{");
    builder.indent();
    builder.line("self::ensureHostInvokerInit();");
    builder.line("self::$_nextHandleId++;");
    builder.line("$id = self::$_nextHandleId;");
    builder.line("self::$_handles[$id] = $value;");
    builder.line("return self::lib()->AzRefAny_newHostHandle($id);");
    builder.dedent();
    builder.line("}");
    builder.blank();

    builder.line("/**");
    builder.line(" * Recover the PHP value previously wrapped via `refanyCreate()`.");
    builder.line(" * Returns null if `$refany` was not a host-handle RefAny.");
    builder.line(" *");
    builder.line(" * @param \\FFI\\CData $refany Either an AzRefAny by-value or a pointer");
    builder.line(" *                              into one (the framework hands callbacks");
    builder.line(" *                              the by-pointer form).");
    builder.line(" * @return mixed");
    builder.line(" */");
    builder.line("public static function refanyGet(\\FFI\\CData $refany)");
    builder.line("{");
    builder.indent();
    builder.line("$ffi = self::lib();");
    builder.line("// Accept either AzRefAny or AzRefAny*; AzRefAny_getHostHandle takes");
    builder.line("// `const AzRefAny*`, so a by-value handle needs FFI::addr().");
    builder.line("$ptr = FFI::isNull(FFI::addr($refany)) ? null : FFI::addr($refany);");
    builder.line("$id = $ffi->AzRefAny_getHostHandle($ptr);");
    builder.line("if ($id === 0) { return null; }");
    builder.line("return self::$_handles[$id] ?? null;");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

/// Emit one per-kind invoker registration (`$ffi->cast(...)` + setter call).
fn emit_per_kind_invoker(builder: &mut CodeBuilder, cb: &super::super::ir::CallbackTypedefDef) {
    let wrapper = wrapper_name(cb);
    let cb_has_return = has_return(cb);

    // PHP closure params: $id, then one $arg<i> per callback arg, then $out
    // when there's a return value. Names match `arg.name` where present.
    let arg_names: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.name.is_empty() {
                format!("arg{}", i)
            } else {
                a.name.clone()
            }
        })
        .collect();
    let mut params: Vec<String> = vec!["int $id".to_string()];
    for nm in &arg_names {
        params.push(format!("${}", nm));
    }
    if cb_has_return {
        params.push("$out".to_string());
    }

    // The user's callable is invoked with each pointer arg passed through.
    let user_args: Vec<String> = arg_names.iter().map(|nm| format!("${}", nm)).collect();

    builder.line(&format!("// {} invoker", wrapper));
    builder.line(&format!(
        "${}_invoker = $ffi->cast(",
        snake_lower(wrapper)
    ));
    builder.indent();
    builder.line(&format!("'Az{}Invoker',", wrapper));
    builder.line(&format!(
        "static function ({}): void {{",
        params.join(", ")
    ));
    builder.indent();
    builder.line("$fn = Azul::$_handles[$id] ?? null;");
    builder.line("if ($fn === null) { return; }");
    builder.line("try {");
    builder.indent();
    if cb_has_return {
        builder.line(&format!("$ret = $fn({});", user_args.join(", ")));
        builder.line("if ($ret === null) { return; }");
        // For numeric returns (Update enum, etc.) we can write directly.
        // For struct returns, FFI::memcpy handles the bytes.
        builder.line("if (\\is_int($ret)) {");
        builder.indent();
        builder.line("$out[0] = $ret;");
        builder.dedent();
        builder.line("} elseif ($ret instanceof \\FFI\\CData) {");
        builder.indent();
        builder.line(
            "FFI::memcpy(FFI::addr($out[0]), FFI::addr($ret), FFI::sizeof($ret));",
        );
        builder.dedent();
        builder.line("}");
    } else {
        builder.line(&format!("$fn({});", user_args.join(", ")));
    }
    builder.dedent();
    builder.line("} catch (\\Throwable $e) {");
    builder.indent();
    builder.line(&format!(
        "\\fwrite(\\STDERR, \"[azul] {} error: \".$e->getMessage().\"\\n\");",
        wrapper
    ));
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line(");");
    builder.line(&format!(
        "self::$_livePins[] = ${}_invoker;",
        snake_lower(wrapper)
    ));
    builder.line(&format!(
        "$ffi->AzApp_set{}Invoker(${}_invoker);",
        wrapper,
        snake_lower(wrapper)
    ));
    builder.blank();
}

/// Local lower-snake helper. PHP variable names go `$callback_invoker`.
/// (We can't reuse `to_snake_case` from the shared helper because PHP's
/// convention here is lower-case-first regardless of input.)
fn snake_lower(name: &str) -> String {
    super::super::managed_host_invoker::to_snake_case(name)
}
