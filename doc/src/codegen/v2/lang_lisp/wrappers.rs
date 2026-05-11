//! Idiomatic CLOS-wrapper emission for the Common Lisp generator.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function we emit:
//!
//! - A `(defclass <name> () ((ptr :initarg :ptr :reader <name>-ptr)))`
//!   that wraps the foreign pointer.
//! - A `(defmethod close-<name> ((obj <name>)))` that calls the
//!   matching `%az-<name>-delete` and nulls out the pointer slot. CL
//!   has no RAII; users invoke this manually or via the macro below.
//! - A `(defmacro with-<name> ((var ...) &body body) ...)` that wraps
//!   the constructor call in `unwind-protect` so the close method runs
//!   on non-local exit.
//! - Idiomatic functions:
//!   - `(make-<name> ...)` for `Constructor` / `Default`.
//!   - `(<name>-<method> obj ...)` for `Method` / `MethodMut`.
//!   - `(<name>-<method> ...)` for `StaticMethod`.
//!
//! Plain POD types without a `_delete` get no CLOS wrapper; users
//! manipulate them through `cffi:foreign-slot-value` directly.
//!
//! Tagged unions get a minimal helper layer: a `tag` reader and per
//! unit-variant constructor functions. Payload-bearing variants are
//! left to the user (they can construct the FFI struct via
//! `cffi:foreign-alloc` / `with-foreign-object`).

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, EnumDef, EnumVariantKind, FunctionDef, FunctionKind, StructDef,
    TypeCategory,
};
use super::{idiomatic_class_name, ident_to_kebab, raw_fn_name, to_kebab_case};

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Idiomatic CLOS wrappers (in :azul package).");
    builder.line(";;");
    builder.line(";; Conventions:");
    builder.line(";;   (make-foo ...)        -- constructor");
    builder.line(";;   (foo-method obj ...)  -- instance method");
    builder.line(";;   (close-foo obj)       -- explicit destructor; nulls the pointer");
    builder.line(";;   (with-foo (var ...) body...) -- unwind-protect helper");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit_wrapper(s, ir, config) {
            continue;
        }
        emit_struct_wrapper(builder, s, ir);
    }

    // Tagged-union enums: minimal helpers (tag reader + unit constructors).
    for e in &ir.enums {
        if !should_emit_union_helper(e, config) {
            continue;
        }
        emit_union_helper(builder, e);
    }

    Ok(())
}

// =============================================================================
// Inclusion filters
// =============================================================================

fn should_emit_wrapper(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    if matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    has_delete_function(&s.name, ir)
}

fn should_emit_union_helper(e: &EnumDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&e.name) {
        return false;
    }
    if !e.generic_params.is_empty() {
        return false;
    }
    if matches!(
        e.category,
        TypeCategory::Recursive | TypeCategory::DestructorOrClone | TypeCategory::GenericTemplate
    ) {
        return false;
    }
    e.is_union
}

fn has_delete_function(class_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == class_name && f.kind == FunctionKind::Delete)
}

// =============================================================================
// Struct wrapper emission
// =============================================================================

fn emit_struct_wrapper(builder: &mut CodeBuilder, s: &StructDef, ir: &CodegenIR) {
    let class = idiomatic_class_name(&s.name);
    let close_sym = format!("close-{}", class);
    let with_sym = format!("with-{}", class);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    // (defclass app () ((ptr ...)))
    builder.line(&format!("(defclass {} ()", class));
    builder.indent();
    builder.line("((ptr :initarg :ptr");
    builder.line(&format!(
        "        :accessor {}-ptr",
        class
    ));
    builder.line("        :initform (cffi:null-pointer))))");
    builder.dedent();
    builder.blank();

    // Export the class name.
    builder.line(&format!("(export '{} :azul)", class));
    builder.line(&format!("(export '{}-ptr :azul)", class));

    // close-<name>: free the underlying pointer if non-null.
    let delete_raw = raw_fn_name(&format!("Az{}_delete", s.name));
    builder.line(&format!("(defmethod {} ((obj {}))", close_sym, class));
    builder.indent();
    builder.line(&format!("(let ((p ({}-ptr obj))) ", class));
    builder.indent();
    builder.line(&format!(
        "(unless (cffi:null-pointer-p p) (azul-internal::{} p)) ",
        delete_raw
    ));
    builder.line(&format!("(setf ({}-ptr obj) (cffi:null-pointer))))", class));
    builder.dedent();
    builder.dedent();
    builder.blank();
    builder.line(&format!("(export '{} :azul)", close_sym));

    // Idiomatic constructors / methods / static methods.
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&s.name).collect();

    // Track the first usable constructor (for with-<name> sugar).
    let mut ctor_for_with: Option<&FunctionDef> = None;

    for func in &funcs {
        if func.kind.is_trait_function() {
            continue; // Skip Delete/PartialEq/Cmp/Hash/Debug -- close-<name> covers Delete.
        }
        match func.kind {
            FunctionKind::Constructor
            | FunctionKind::StaticMethod
            | FunctionKind::Default
            | FunctionKind::EnumVariantConstructor => {
                if ctor_for_with.is_none()
                    && matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default)
                {
                    ctor_for_with = Some(func);
                }
                emit_static_or_ctor(builder, &class, func, ir);
            }
            FunctionKind::Method
            | FunctionKind::MethodMut
            | FunctionKind::DeepCopy => {
                emit_instance_method(builder, &class, func, ir);
            }
            _ => {}
        }
    }

    // with-<name>: convenience macro that wraps the chosen constructor
    // call in unwind-protect. If no constructor was found we emit a
    // generic form that takes a pre-built object.
    emit_with_macro(builder, &class, &with_sym, &close_sym, ctor_for_with);

    builder.blank();
}

fn emit_static_or_ctor(
    builder: &mut CodeBuilder,
    class: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let raw = raw_fn_name(&func.c_name);
    let lisp_method = idiomatic_method_name(&func.method_name);

    // Constructor / Default -> `make-<class>` (replace the method name).
    let public_name = match func.kind {
        FunctionKind::Constructor | FunctionKind::Default => {
            if func.method_name == "new" {
                format!("make-{}", class)
            } else {
                format!("make-{}-{}", class, lisp_method)
            }
        }
        _ => format!("{}-{}", class, lisp_method),
    };

    let (param_list, mut call_args) = build_param_lists(&func.args, ir, /*has_self*/ false);
    substitute_callback_args(&func.args, &mut call_args, /*self_offset*/ 0);

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    builder.line(&format!("(defun {} ({})", public_name, param_list.join(" ")));
    builder.indent();
    if returns_self {
        builder.line(&format!(
            "(make-instance '{} :ptr (azul-internal::{} {})))",
            class,
            raw,
            call_args.join(" ")
        ));
    } else {
        builder.line(&format!(
            "(azul-internal::{} {}))",
            raw,
            call_args.join(" ")
        ));
    }
    builder.dedent();
    builder.line(&format!("(export '{} :azul)", public_name));
    builder.blank();
}

fn emit_instance_method(
    builder: &mut CodeBuilder,
    class: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let raw = raw_fn_name(&func.c_name);
    let lisp_method = idiomatic_method_name(&func.method_name);
    let public_name = format!("{}-{}", class, lisp_method);

    if !func.doc.is_empty() {
        for d in &func.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }

    let (mut param_list, mut call_args) =
        build_param_lists(&func.args, ir, /*has_self*/ true);

    // The first arg from the IR is implicit `self` (named after the
    // lowercased class). Replace it with `obj` and pass the inner ptr.
    if !param_list.is_empty() {
        param_list[0] = "obj".to_string();
        call_args[0] = format!("({}-ptr obj)", class);
    }
    substitute_callback_args(&func.args, &mut call_args, /*self_offset*/ 0);

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    builder.line(&format!("(defun {} ({})", public_name, param_list.join(" ")));
    builder.indent();
    if returns_self {
        builder.line(&format!(
            "(make-instance '{} :ptr (azul-internal::{} {})))",
            class,
            raw,
            call_args.join(" ")
        ));
    } else {
        builder.line(&format!(
            "(azul-internal::{} {}))",
            raw,
            call_args.join(" ")
        ));
    }
    builder.dedent();
    builder.line(&format!("(export '{} :azul)", public_name));
    builder.blank();
}

/// Wrap each callback-typed call-arg in `(azul:register-callback "Wrapper" arg)`
/// so users can pass plain Lisp lambdas. Only kinds in the host-invoker
/// allowlist are substituted.
fn substitute_callback_args(
    args: &[super::super::ir::FunctionArg],
    call_args: &mut [String],
    self_offset: usize,
) {
    for (i, a) in args.iter().enumerate() {
        if i < self_offset {
            continue;
        }
        let Some(cb) = a.callback_info.as_ref() else {
            continue;
        };
        let wrapper = cb.callback_wrapper_name.as_str();
        if !super::super::managed_host_invoker::HOST_INVOKER_KINDS.contains(&wrapper) {
            continue;
        }
        // call_args holds the raw param names already; wrap them.
        let original = call_args[i].clone();
        call_args[i] = format!("(azul:register-callback \"{}\" {})", wrapper, original);
    }
}

fn emit_with_macro(
    builder: &mut CodeBuilder,
    class: &str,
    with_sym: &str,
    close_sym: &str,
    ctor: Option<&FunctionDef>,
) {
    builder.line(&format!(
        ";; ({} (var ctor-args...) body...) -- unwind-protect-wrapped binding.",
        with_sym
    ));
    if ctor.is_none() {
        // No constructor in the IR — emitting a `(let ((,var (first
        // ctor-args))) ...)` macro both (a) fails READ because the
        // comma quote on the `ctor-args` reference doesn't survive the
        // SKIP-the-ctor branch, and (b) is conceptually wrong: the
        // caller would have to provide an already-built object via the
        // first ctor-arg, at which point a plain `let` is clearer.
        // Skip the macro entirely; users use raw `unwind-protect`.
        builder.line(&format!(
            ";; SKIPPED: no constructor for `{}` in the IR — use raw",
            class
        ));
        builder.line(&format!(
            ";; `(unwind-protect (progn ...body...) ({} obj))` instead.",
            close_sym
        ));
        builder.blank();
        return;
    }
    builder.line(&format!(
        "(defmacro {} ((var &rest ctor-args) &body body)",
        with_sym
    ));
    builder.indent();
    // Use `make-<class>` (already exported) for the constructor.
    builder.line(&format!(
        "`(let ((,var (apply #'make-{} (list ,@ctor-args))))",
        class
    ));
    builder.indent();
    builder.line(&format!(
        "   (unwind-protect (progn ,@body) ({} ,var))))",
        close_sym
    ));
    builder.dedent();
    builder.dedent();
    builder.line(&format!("(export '{} :azul)", with_sym));
    builder.blank();
}

// =============================================================================
// Tagged-union helpers (minimal)
// =============================================================================

fn emit_union_helper(builder: &mut CodeBuilder, e: &EnumDef) {
    let class = idiomatic_class_name(&e.name);

    if !e.doc.is_empty() {
        for d in &e.doc {
            builder.line(&format!(";; {}", sanitize_comment(d)));
        }
    }
    builder.line(&format!(";; Tagged-union helpers for {}.", class));

    let union_kebab = to_kebab_case(&e.name); // e.g. "az-option-i64"
    for v in &e.variants {
        let variant = ident_to_kebab(&v.name);
        match &v.kind {
            EnumVariantKind::Unit => {
                let public = format!("{}-make-{}", class, variant);
                builder.line(&format!("(defun {} ()", public));
                builder.indent();
                builder.line(&format!(
                    ";; Construct a fresh {} with tag = {}.",
                    class, v.name
                ));
                builder.line(&format!(
                    "(let ((u (cffi:foreign-alloc '(:union azul-internal::{}))))",
                    union_kebab
                ));
                builder.indent();
                builder.line(&format!(
                    "(setf (cffi:foreign-slot-value (cffi:foreign-slot-pointer u '(:union azul-internal::{}) '{}) '(:struct azul-internal::{}-variant-{}) 'azul-internal::tag) :{})",
                    union_kebab,
                    variant,
                    union_kebab,
                    variant,
                    variant
                ));
                builder.line("u))");
                builder.dedent();
                builder.dedent();
                builder.line(&format!("(export '{} :azul)", public));
                builder.blank();
            }
            EnumVariantKind::Tuple(_) | EnumVariantKind::Struct(_) => {
                builder.line(&format!(
                    ";; SKIPPED: variant {}.{} has payload -- construct via cffi:foreign-alloc + slot setters.",
                    class, v.name
                ));
            }
        }
    }
    builder.blank();
}

// =============================================================================
// Helpers
// =============================================================================

/// Build the (param-names, raw-call-args) lists for a function.
/// When `has_self` is true the first arg is treated as the instance
/// pointer and surfaces in both lists; the caller will overwrite the
/// names afterwards.
fn build_param_lists(
    args: &[super::super::ir::FunctionArg],
    _ir: &CodegenIR,
    _has_self: bool,
) -> (Vec<String>, Vec<String>) {
    let mut params = Vec::with_capacity(args.len());
    let mut calls = Vec::with_capacity(args.len());
    for a in args {
        let name = ident_to_kebab(&a.name);
        let bound = match a.ref_kind {
            ArgRefKind::Owned => name.clone(),
            ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                // Caller passes a pointer-bearing object (CLOS instance,
                // CFFI pointer, foreign struct...). We forward it as-is
                // and let CFFI sort out the type.
                name.clone()
            }
        };
        params.push(name);
        calls.push(bound);
    }
    (params, calls)
}

fn idiomatic_method_name(method_name: &str) -> String {
    // Lisp loves kebab-case; convert camelCase / snake_case uniformly.
    let mut out = String::new();
    let mut prev_lower = false;
    for c in method_name.chars() {
        if c == '_' {
            if !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            prev_lower = false;
        } else if c.is_uppercase() {
            if prev_lower && !out.is_empty() && !out.ends_with('-') {
                out.push('-');
            }
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_lower = false;
        } else {
            out.push(c);
            prev_lower = c.is_ascii_lowercase();
        }
    }
    if out.is_empty() {
        return "op".to_string();
    }
    // Special-case: `new` as a method name reads better as `new` itself
    // when paired with `make-<class>-new` -- but we already convert the
    // `new` constructor to `make-<class>` upstream, so leave it as-is.
    out
}

fn sanitize_comment(s: &str) -> String {
    s.replace('\n', " ").replace('\r', " ")
}
