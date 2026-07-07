//! Idiomatic non-prefixed wrapper emission for the Racket generator.
//!
//! For every included IR function we emit a Racket procedure whose name
//! drops the `Az<Class>_` prefix and kebab-cases the rest:
//!
//! ```racket
//! (define (dom-add-child dom child)        (AzDom_addChild dom child))
//! (define (button-create label)            (AzButton_create label))
//! (define (button-set-on-click b data fn)
//!   (AzButton_setOnClick b data (register-callback "ButtonOnClickCallback" fn)))
//! ```
//!
//! Conventions:
//!   * `Constructor` / `Default` named `new`/`default` → `(make-<class> …)`.
//!   * every other static / constructor → `(<class>-<method> …)`.
//!   * instance methods → `(<class>-<method> self …)`.
//!
//! Callback-typed args (whose wrapper is in the host-invoker allowlist)
//! are wrapped in `(register-callback "Wrapper" arg)` so callers pass a
//! plain Racket procedure — the closure is retained by the managed layer
//! (see `managed.rs`). Racket's cstruct values are cpointers, so a wrapper
//! forwards the receiver to a by-pointer (`_pointer`) or by-value
//! (`_AzFoo`) raw arg without any explicit conversion.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{CodegenIR, EnumDef, FunctionDef, FunctionKind, TypeCategory};
use super::super::managed_host_invoker::HOST_INVOKER_KINDS;
use super::{idiomatic_class_name, kebab, sanitize_racket_ident};

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.blank();
    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Idiomatic non-prefixed wrappers.");
    builder.line(";;");
    builder.line(";;   (make-foo ...)        -- constructor (new/default)");
    builder.line(";;   (foo-method self ...) -- instance method / other static");
    builder.line(";;   callback args accept a plain procedure (register-callback wraps it).");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();

    for s in &ir.structs {
        if !should_emit(&s.name, s.category, &s.generic_params, config) {
            continue;
        }
        emit_class_wrappers(builder, &s.name, ir);
    }
    for e in &ir.enums {
        if !should_emit(&e.name, e.category, &e.generic_params, config) {
            continue;
        }
        emit_enum_wrappers(builder, e, ir);
    }

    Ok(())
}

fn should_emit(
    name: &str,
    category: TypeCategory,
    generic_params: &[String],
    config: &CodegenConfig,
) -> bool {
    if !config.should_include_type(name) {
        return false;
    }
    if !generic_params.is_empty() {
        return false;
    }
    !matches!(
        category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

fn emit_class_wrappers(builder: &mut CodeBuilder, class_name: &str, ir: &CodegenIR) {
    let class = idiomatic_class_name(class_name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(class_name).collect();
    if funcs.is_empty() {
        return;
    }
    let has_new = funcs.iter().any(|f| f.method_name == "new");
    for func in funcs {
        if func.kind.is_trait_function() {
            continue; // Delete/PartialEq/Cmp/Hash/Debug: use raw Az* if needed.
        }
        emit_wrapper(builder, &class, func, has_new);
    }
    builder.blank();
}

fn emit_enum_wrappers(builder: &mut CodeBuilder, e: &EnumDef, ir: &CodegenIR) {
    let class = idiomatic_class_name(&e.name);
    let funcs: Vec<&FunctionDef> = ir.functions_for_class(&e.name).collect();
    let has_new = funcs.iter().any(|f| f.method_name == "new");
    let mut wrote = false;
    for func in funcs {
        if func.kind.is_trait_function() {
            continue;
        }
        emit_wrapper(builder, &class, func, has_new);
        wrote = true;
    }
    if wrote {
        builder.blank();
    }
}

fn emit_wrapper(builder: &mut CodeBuilder, class: &str, func: &FunctionDef, has_new: bool) {
    let public_name = public_name(class, func, has_new);

    // Parameter names (kebab, de-duplicated, sanitized).
    let mut seen = std::collections::HashMap::<String, usize>::new();
    let params: Vec<String> = func
        .args
        .iter()
        .map(|a| {
            let base = sanitize_racket_ident(&kebab(&a.name));
            let n = seen.entry(base.clone()).or_insert(0);
            let out = if *n == 0 {
                base.clone()
            } else {
                format!("{}{}", base, n)
            };
            *n += 1;
            out
        })
        .collect();

    // Call args: substitute callback-typed args with register-callback.
    let call_args: Vec<String> = func
        .args
        .iter()
        .zip(params.iter())
        .map(|(a, p)| match a.callback_info.as_ref() {
            Some(cb) if HOST_INVOKER_KINDS.contains(&cb.callback_wrapper_name.as_str()) => {
                format!(
                    "(register-callback \"{}\" {})",
                    cb.callback_wrapper_name, p
                )
            }
            _ => p.clone(),
        })
        .collect();

    for d in &func.doc {
        builder.line(&format!(";; {}", d.replace('\n', " ")));
    }
    builder.line(&format!(
        "(define ({} {})",
        public_name,
        params.join(" ")
    ));
    builder.indent();
    builder.line(&format!("({} {}))", func.c_name, call_args.join(" ")));
    builder.dedent();
}

/// Public wrapper name for a function.
fn public_name(class: &str, func: &FunctionDef, has_new: bool) -> String {
    let method = kebab(&func.method_name);
    match func.kind {
        // `new` always claims the idiomatic `make-<class>` name. `default` also
        // maps to `make-<class>` — but ONLY when the class has no `new`, since a
        // class with BOTH (e.g. MsgBox) would otherwise emit `make-<class>`
        // twice (a duplicate `define` that fails to load). When both exist,
        // `new` wins and `default` falls back to `<class>-default`.
        FunctionKind::Constructor | FunctionKind::Default
            if func.method_name == "new"
                || (func.method_name == "default" && !has_new) =>
        {
            format!("make-{}", class)
        }
        _ => format!("{}-{}", class, method),
    }
}
