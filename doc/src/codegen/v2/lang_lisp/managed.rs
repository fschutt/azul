//! Common Lisp (CFFI) managed-FFI runtime helpers (host-invoker pattern).
//!
//! CFFI is a libffi binding under the hood, so `defcallback` shares
//! the struct-by-value constraint with LuaJIT FFI / ruby-ffi / PHP FFI /
//! koffi. The host-invoker pattern routes user callbacks through
//! pointer-arg invokers so the libffi closure cast is always legal.
//!
//! ## Output surface
//!
//! Emitted into `azul.lisp`:
//!
//! 1. **Internal defcfun bindings** for the host-invoker C-ABI exports
//!    (`%az-app-set-host-handle-releaser`, `%az-ref-any-new-host-handle`,
//!    `%az-ref-any-get-host-handle`, plus per-kind setters and
//!    `%az-<kind>-create-from-host-handle`). Live in `:azul-internal`.
//! 2. **Per-kind defcallback closures** dispatching through a global
//!    handle table keyed by `uint64`.
//! 3. **`(azul:register-callback kind fn)`** that allocates a host
//!    handle, stashes `fn`, and returns the matching cdata struct
//!    pointer from `%az-<kind>-create-from-host-handle`.
//! 4. **`(azul:refany-create value)` / `(azul:refany-get refany)`**
//!    user-data helpers sharing the same handle table — symmetric with
//!    Lua's `azul.refany_create` / `azul.refany_get`.
//!
//! All of this is appended to the existing `azul.lisp` body — no
//! restructuring of the existing types/functions emitters needed.

use super::super::generator::CodeBuilder;
use super::super::ir::CodegenIR;
use super::super::managed_host_invoker::{
    has_return, host_invoker_kinds, to_kebab_case, wrapper_name,
};

/// Emit the host-invoker plumbing.
///
/// Calling order (in `mod.rs`):
///   1. types::generate_types (so `:az-ref-any` etc. exist)
///   2. functions::generate_defcfuns (regular bindings)
///   3. **emit_managed** in `:azul-internal`
///   4. switch to `:azul`
///   5. wrappers::generate_wrappers
///   6. **emit_user_facing_helpers** in `:azul`
pub fn emit_internal_bindings(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line(";;; ────────────────────────────────────────────────────────────────");
    builder.line(";;; Managed-FFI runtime: host-invoker C-ABI bindings.");
    builder.line(";;; ────────────────────────────────────────────────────────────────");
    builder.blank();

    builder.line("(defcfun (\"AzApp_setHostHandleReleaser\" %az-app-set-host-handle-releaser) :void");
    builder.line("  (releaser :pointer))");
    builder.blank();
    builder.line("(defcfun (\"AzRefAny_newHostHandle\" %az-ref-any-new-host-handle)");
    builder.line("    (:struct az-ref-any)");
    builder.line("  (id :uint64))");
    builder.blank();
    builder.line("(defcfun (\"AzRefAny_getHostHandle\" %az-ref-any-get-host-handle) :uint64");
    builder.line("  (refany :pointer))");
    builder.blank();

    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let kebab = to_kebab_case(wrapper);
        builder.line(&format!(
            "(defcfun (\"AzApp_set{w}Invoker\" %az-app-set-{k}-invoker) :void",
            w = wrapper,
            k = kebab
        ));
        builder.line("  (invoker :pointer))");
        builder.blank();
        builder.line(&format!(
            "(defcfun (\"Az{w}_createFromHostHandle\" %az-{k}-create-from-host-handle)",
            w = wrapper,
            k = kebab
        ));
        builder.line(&format!(
            "    (:struct az-{k})",
            k = kebab
        ));
        builder.line("  (id :uint64))");
        builder.blank();
    }

    // ── Handle table + releaser closure (still in :azul-internal) ─────
    builder.line("(defparameter *azul-handles* (make-hash-table :test 'eql)");
    builder.line("  \"Map host-handle id (uint64) -> Lisp object (callback or refany value).\")");
    builder.blank();
    builder.line("(defparameter *azul-next-handle-id* 0");
    builder.line("  \"Monotonic id allocator. Starts at 0; first allocation returns 1.\")");
    builder.blank();
    builder.line("(defun %azul-alloc-handle (value)");
    builder.line("  \"Allocate a fresh id, store VALUE under it, return the id.\"");
    builder.line("  (incf *azul-next-handle-id*)");
    builder.line("  (let ((id *azul-next-handle-id*))");
    builder.line("    (setf (gethash id *azul-handles*) value)");
    builder.line("    id))");
    builder.blank();

    builder.line("(defcallback %azul-host-handle-releaser :void ((id :uint64))");
    builder.line("  (remhash id *azul-handles*))");
    builder.blank();

    // Per-kind invoker callbacks
    for cb in host_invoker_kinds(ir) {
        emit_per_kind_invoker(builder, cb, ir);
    }

    builder.line(";;; Initialise once at module load. Idempotent because");
    builder.line(";;; %az-app-set-* installs the closure regardless of prior state.");
    builder.line("(defparameter *azul-host-invoker-initialised* nil)");
    builder.blank();
    builder.line("(defun %azul-ensure-host-invoker-init ()");
    builder.line("  (unless *azul-host-invoker-initialised*");
    builder.line("    (setf *azul-host-invoker-initialised* t)");
    builder.line("    (%az-app-set-host-handle-releaser");
    builder.line("      (callback %azul-host-handle-releaser))");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let kebab = to_kebab_case(wrapper);
        builder.line(&format!(
            "    (%az-app-set-{k}-invoker (callback %azul-{k}-invoker))",
            k = kebab
        ));
    }
    builder.line("    nil))");
    builder.blank();
}

fn emit_per_kind_invoker(builder: &mut CodeBuilder, cb: &super::super::ir::CallbackTypedefDef, ir: &CodegenIR) {
    let wrapper = wrapper_name(cb);
    let kebab = to_kebab_case(wrapper);
    let cb_has_return = has_return(cb);

    let mut params = vec!["(id :uint64)".to_string()];
    for (i, a) in cb.args.iter().enumerate() {
        let nm = if a.name.is_empty() {
            format!("arg{}", i)
        } else {
            a.name.clone()
        };
        params.push(format!("({} :pointer)", nm));
    }
    if cb_has_return {
        params.push("(out :pointer)".to_string());
    }

    let user_args: Vec<String> = cb
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

    builder.line(&format!(
        "(defcallback %azul-{}-invoker :void ({})",
        kebab,
        params.join(" ")
    ));
    builder.line("  (let ((fn (gethash id *azul-handles*)))");
    builder.line("    (when fn");
    builder.line("      (handler-case");
    if cb_has_return {
        builder.line(&format!(
            "          (let ((ret (funcall fn {})))",
            user_args.join(" ")
        ));
        // Numeric returns (Update enum) → write int32 at out.
        // Wrapper-class returns (Dom from LayoutCallback, etc.) → the
        // user gave us a CLOS instance whose `*-ptr` accessor holds
        // a `(:struct az-<foo>)` value; memcpy the bytes through the
        // out-pointer. We compute the slot name by convention from
        // the return-type's snake-case class name. The struct's CFFI
        // foreign-type size is requested via `foreign-type-size`.
        // Wrapper class name (no Az prefix) and CFFI struct name (with az- prefix).
        let ret_info = match cb.return_type.as_deref() {
            Some(rt) => {
                let trimmed = rt.trim();
                if let Some(s) = ir.find_struct(trimmed) {
                    let class_kebab = super::ident_to_kebab(&s.name); // e.g. "dom"
                    let struct_kebab = super::to_kebab_case(&s.name); // e.g. "az-dom"
                    Some((class_kebab, struct_kebab))
                } else {
                    None
                }
            }
            None => None,
        };
        builder.line("            (cond");
        builder.line("              ((integerp ret)");
        builder.line("               (setf (cffi:mem-ref out :int32) ret))");
        if let Some((class_kebab, struct_kebab)) = ret_info {
            // Wrapper instance: pointer accessor is `<class>-ptr` (e.g.
            // `dom-ptr`), CFFI struct type is `(:struct az-<...>)`.
            builder.line(&format!(
                "              ((and ret (typep ret '{}))",
                class_kebab
            ));
            builder.line(&format!(
                "               (let ((src ({}-ptr ret)))",
                class_kebab
            ));
            builder.line(&format!(
                "                 (cffi:foreign-funcall \"memcpy\" :pointer out :pointer src :size (cffi:foreign-type-size '(:struct {})) :pointer)))",
                struct_kebab
            ));
        }
        builder.line("              (t nil)))");
    } else {
        builder.line(&format!(
            "          (funcall fn {})",
            user_args.join(" ")
        ));
    }
    // Closing parens (count carefully — off-by-one here was the
    // bug that "leaked" each defcallback across all following forms,
    // sending the reader into AZUL-INTERNAL for the entire wrapper
    // emit):
    //   1 close for the error clause
    //   1 close for handler-case
    //   1 close for the `when fn` form
    //   1 close for the `let ((fn ...))` form
    //   1 close for the outer defcallback
    // = 5 trailing closes (in addition to the close after `e` that
    //   closes `(format ... e)`, giving 6 close parens total at line
    //   end).
    builder.line(&format!(
        "        (error (e) (format *error-output* \"[azul] {} error: ~A~%\" e)))))) ",
        wrapper
    ));
    builder.blank();
}

/// Emit the public `:azul` package helpers — `register-callback`,
/// `refany-create`, `refany-get`. Called from inside `(in-package :azul)`.
pub fn emit_user_facing_helpers(builder: &mut CodeBuilder, ir: &CodegenIR) {
    builder.blank();
    builder.line(";;; ────────────────────────────────────────────────────────────────");
    builder.line(";;; Managed-FFI public helpers");
    builder.line(";;; ────────────────────────────────────────────────────────────────");
    builder.blank();

    builder.line("(defun register-callback (kind fn)");
    builder.line("  \"Wrap FN in the matching Az<Kind> cdata struct so a native call");
    builder.line("   site (e.g. (button:set-on-click ...)) can store it. KIND is a");
    builder.line("   string — 'Callback', 'LayoutCallback', 'VirtualViewCallback', etc.\"");
    builder.line("  (azul-internal::%azul-ensure-host-invoker-init)");
    builder.line("  (let ((id (azul-internal::%azul-alloc-handle fn)))");
    builder.line("    (cond");
    for cb in host_invoker_kinds(ir) {
        let wrapper = wrapper_name(cb);
        let kebab = to_kebab_case(wrapper);
        builder.line(&format!(
            "      ((string= kind \"{}\") (azul-internal::%az-{}-create-from-host-handle id))",
            wrapper, kebab
        ));
    }
    builder.line("      (t (error \"register-callback: unknown kind ~S\" kind)))))");
    builder.blank();

    builder.line("(defun refany-create (value)");
    builder.line("  \"Wrap an arbitrary Lisp value in an AzRefAny. The value lives in");
    builder.line("   the shared handle table; the destructor clears it on last-clone drop.\"");
    builder.line("  (azul-internal::%azul-ensure-host-invoker-init)");
    builder.line("  (let ((id (azul-internal::%azul-alloc-handle value)))");
    builder.line("    (azul-internal::%az-ref-any-new-host-handle id)))");
    builder.blank();

    builder.line("(defun refany-get (refany-ptr)");
    builder.line("  \"Recover the Lisp value previously wrapped via REFANY-CREATE. Pass a");
    builder.line("   pointer to an AzRefAny (the framework hands callbacks by-pointer).\"");
    builder.line("  (let ((id (azul-internal::%az-ref-any-get-host-handle refany-ptr)))");
    builder.line("    (when (/= id 0)");
    builder.line("      (gethash id azul-internal::*azul-handles*))))");
    builder.blank();
}

/// Returns Lisp `cffi:defcstruct` declarations for the host-invoker types
/// not present in azul.h. Today this is empty (the host-invoker types are
/// all already in the IR via `AzCallback`, `AzLayoutCallback`,
/// `AzVirtualViewCallback`); kept as a hook for future per-kind types.
pub fn extra_type_decls(_ir: &CodegenIR) -> String {
    String::new()
}
