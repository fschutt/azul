//! Racket managed-FFI runtime helpers (host-invoker pattern) + the
//! GC-retention handling that keeps callback closures alive.
//!
//! ## Why an invoker layer at all, if Racket callbacks are C-ABI-direct?
//!
//! Racket `_fun` produces a real C fn-ptr, so a click callback *could* be
//! handed straight to `AzButton_setOnClick`. We still route through the
//! shared host-invoker plumbing because (a) the layout callback returns
//! an `AzDom` (240-byte aggregate) which is cleanest to write back through
//! an out-pointer, and (b) it gives the user-data RefAny and the callback
//! the same host-handle lifetime story as every other managed binding.
//!
//! ## The GC-retention gotcha
//!
//! The `ffi_closure` behind a `_fun` callback is kept alive only while the
//! Racket procedure it wraps stays reachable (`#:keep #t`, the default,
//! ties the closure's lifetime to that procedure value). If the procedure
//! is dropped, the GC frees the closure and the next C call crashes.
//!
//! We defend on two axes:
//!   * **Invoker closures + releaser** — pinned in the module-level
//!     `live-pins` list, alive for the whole process.
//!   * **User callbacks** — stored in the module-level `azul-handles`
//!     hash keyed by host-handle id; the RefAny destructor calls back
//!     through the releaser to drop the entry only once the framework
//!     drops the last clone.

use super::super::generator::CodeBuilder;
use super::super::ir::{CallbackTypedefDef, CodegenIR};
use super::super::managed_host_invoker::{
    has_return, host_invoker_kinds, to_kebab_case, wrapper_name,
};

/// Emit the whole managed-FFI runtime.
pub fn emit_managed(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line(";; ============================================================================");
    builder.line(";; Managed-FFI runtime: host-invoker plumbing + GC-retained callbacks.");
    builder.line(";; ============================================================================");
    builder.blank();

    // ── Host-invoker C-ABI bindings not present as regular api.json fns ──
    builder.line(";; Host-handle releaser — fired once per RefAny last-clone drop.");
    builder.line("(define-azul AzApp_setHostHandleReleaser");
    builder.line("  (_fun (_fun _uint64 -> _void) -> _void))");
    builder.blank();
    builder.line(";; User-data RefAny on top of the host-handle path (shared lifetime).");
    builder.line("(define-azul AzRefAny_newHostHandle (_fun _uint64 -> _AzRefAny))");
    builder.line("(define-azul AzRefAny_getHostHandle (_fun _pointer -> _uint64))");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        let invoker_ty = invoker_fun_type(cb);
        builder.line(&format!(
            "(define-azul AzApp_set{w}Invoker (_fun {ty} -> _void))",
            w = w,
            ty = invoker_ty
        ));
        builder.line(&format!(
            "(define-azul Az{w}_createFromHostHandle (_fun _uint64 -> _Az{w}))",
            w = w
        ));
    }
    builder.blank();

    // ── Handle table + id allocator ─────────────────────────────────────
    builder.line(";; id (uint64) -> Racket value (a user callback proc or a refany value).");
    builder.line(";; A strong hash: every stored value is a GC root until the releaser");
    builder.line(";; drops it, so a live callback's closure is never collected out from");
    builder.line(";; under the C side.");
    builder.line("(define azul-handles (make-hash))");
    builder.line("(define azul-next-id (box 0))");
    builder.blank();
    builder.line("(define (azul-alloc-handle value)");
    builder.line("  (set-box! azul-next-id (add1 (unbox azul-next-id)))");
    builder.line("  (define id (unbox azul-next-id))");
    builder.line("  (hash-set! azul-handles id value)");
    builder.line("  id)");
    builder.blank();

    // ── live-pins: strong roots for the invoker/releaser C closures ─────
    builder.line(";; Every invoker + the releaser is pinned here so its `_fun` ffi_closure");
    builder.line(";; is never GC'd (the whole raison d'être of this list — see module docs).");
    builder.line("(define live-pins (box '()))");
    builder.line("(define (pin! proc) (set-box! live-pins (cons proc (unbox live-pins))) proc)");
    builder.blank();

    // ── Releaser closure ────────────────────────────────────────────────
    builder.line("(define azul-releaser");
    builder.line("  (pin! (lambda (id) (hash-remove! azul-handles id))))");
    builder.blank();

    // ── Per-kind invoker closures ───────────────────────────────────────
    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker(builder, cb, ir);
    }

    // ── One-time init: install releaser + every per-kind invoker ────────
    builder.line(";; Install the releaser + every per-kind invoker exactly once. Idempotent:");
    builder.line(";; the AzApp_set* setters overwrite whatever was installed before.");
    builder.line("(define azul-host-invoker-initialised (box #f))");
    builder.line("(define (azul-ensure-host-invoker-init)");
    builder.line("  (unless (unbox azul-host-invoker-initialised)");
    builder.line("    (set-box! azul-host-invoker-initialised #t)");
    builder.line("    (AzApp_setHostHandleReleaser azul-releaser)");
    for cb in host_invoker_kinds(ir) {
        let kebab = to_kebab_case(wrapper_name(cb));
        builder.line(&format!(
            "    (AzApp_set{w}Invoker {k}-invoker)",
            w = wrapper_name(cb),
            k = kebab
        ));
    }
    builder.line("    (void)))");
    builder.blank();

    // ── Public helpers ──────────────────────────────────────────────────
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Public managed-FFI helpers.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    builder.line(";; Wrap FN in the matching Az<Kind> wrapper struct so a native call site");
    builder.line(";; (e.g. (button-set-on-click ...)) can store it. KIND is a string —");
    builder.line(";; \"Callback\", \"LayoutCallback\", \"ButtonOnClickCallback\", ...");
    builder.line("(define (register-callback kind fn)");
    builder.line("  (azul-ensure-host-invoker-init)");
    builder.line("  (define id (azul-alloc-handle fn))");
    builder.line("  (cond");
    for cb in host_invoker_kinds(ir) {
        let w = wrapper_name(cb);
        builder.line(&format!(
            "    [(string=? kind \"{w}\") (Az{w}_createFromHostHandle id)]",
            w = w
        ));
    }
    builder.line("    [else (error 'register-callback \"unknown kind ~s\" kind)]))");
    builder.blank();

    builder.line(";; Wrap an arbitrary Racket value in an AzRefAny. The value lives in the");
    builder.line(";; shared handle table; the destructor clears it on last-clone drop.");
    builder.line("(define (refany-create value)");
    builder.line("  (azul-ensure-host-invoker-init)");
    builder.line("  (AzRefAny_newHostHandle (azul-alloc-handle value)))");
    builder.blank();

    builder.line(";; Recover the Racket value previously wrapped via refany-create. Pass the");
    builder.line(";; AzRefAny pointer the framework hands each callback.");
    builder.line("(define (refany-get refany-ptr)");
    builder.line("  (define id (AzRefAny_getHostHandle refany-ptr))");
    builder.line("  (and (not (= id 0)) (hash-ref azul-handles id #f)))");
    builder.blank();
}

/// The `_fun` invoker type for one callback kind: `_uint64` host handle id,
/// one `_pointer` per callback arg (by-pointer, libffi-friendly), and a
/// trailing `_pointer` out-param when the callback returns non-void.
fn invoker_fun_type(cb: &CallbackTypedefDef) -> String {
    let mut parts = vec!["_uint64".to_string()];
    for _ in &cb.args {
        parts.push("_pointer".to_string());
    }
    if has_return(cb) {
        parts.push("_pointer".to_string());
    }
    format!("(_fun {} -> _void)", parts.join(" "))
}

fn emit_per_kind_invoker(builder: &mut CodeBuilder, cb: &CallbackTypedefDef, ir: &CodegenIR) {
    let w = wrapper_name(cb);
    let kebab = to_kebab_case(w);
    let cb_has_return = has_return(cb);

    // Param names: id, one per arg, plus out when there's a return.
    let mut params = vec!["id".to_string()];
    let user_args: Vec<String> = cb
        .args
        .iter()
        .enumerate()
        .map(|(i, a)| {
            if a.name.is_empty() {
                format!("a{}", i)
            } else {
                super::sanitize_racket_ident(&super::kebab(&a.name))
            }
        })
        .collect();
    params.extend(user_args.iter().cloned());
    if cb_has_return {
        params.push("out".to_string());
    }

    builder.line(&format!(
        ";; Invoker for {w}: dispatch to the user proc stored under `id`.",
        w = w
    ));
    builder.line(&format!(
        "(define {k}-invoker",
        k = kebab
    ));
    builder.indent();
    builder.line(&format!("(pin! (lambda ({}) ", params.join(" ")));
    builder.indent();
    builder.line("(define fn (hash-ref azul-handles id #f))");
    builder.line("(when fn");
    builder.indent();
    builder.line("(with-handlers ([exn:fail?");
    builder.line(&format!(
        "                (lambda (e) (eprintf \"[azul] {w} error: ~a\\n\" (exn-message e)))])",
        w = w
    ));
    if cb_has_return {
        builder.line(&format!("  (define ret (fn {}))", user_args.join(" ")));
        // Scalar returns (Update etc.) → write int32. Aggregate returns
        // (Dom from LayoutCallback) → the user handed us an _AzDom cstruct
        // value (a cpointer); memcpy its bytes through `out`. `ctype-sizeof`
        // gives the true C size (240 for AzDom), matching return_c_size.
        let ret_ctype = cb
            .return_type
            .as_deref()
            .filter(|r| *r != "void")
            .map(|r| super::map_type_to_racket(r, ir))
            .unwrap_or_else(|| "_int32".to_string());
        builder.line("  (cond");
        builder.line("    [(exact-integer? ret) (ptr-set! out _int32 ret)]");
        builder.line(&format!(
            "    [(cpointer? ret) (memcpy out ret (ctype-sizeof {}))]",
            ret_ctype
        ));
        builder.line("    [else (void)]))");
    } else {
        builder.line(&format!("  (fn {}))", user_args.join(" ")));
    }
    builder.dedent();
    builder.line(")"); // close (when fn ...)
    builder.dedent();
    builder.line(")))"); // close lambda, pin!, and the outer form's paren balance
    builder.dedent();
    builder.blank();
}
