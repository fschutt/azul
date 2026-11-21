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

use super::{
    super::{
        c_layout,
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{
            CodegenIR, EnumDef, FieldDef, FieldRefKind, FunctionDef, FunctionKind, StructDef,
            TypeCategory,
        },
        managed_host_invoker, managed_lang_helpers,
    },
    c_name, field_ident,
    functions::should_emit_function,
    idiomatic_class_name, kebab, map_type_to_racket, sanitize_racket_ident,
};

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

    builder.line(";; ----------------------------------------------------------------------------");
    builder.line(";; Field accessors of the resource-owning structs (those with an Az*_delete).");
    builder.line(";;");
    builder.line(";;   (foo-field obj)           -- a String field as a Racket string (a copy);");
    builder.line(";;                                any other field as define-cstruct gives it: a");
    builder.line(";;                                struct field is a VIEW into obj, so nested");
    builder.line(";;                                writes reach obj:");
    builder.line(
        ";;     (set-full-window-state-title! (window-create-options-window-state opts) \"Hi\")",
    );
    builder.line(";;   (foo-field-copy obj)      -- an independent deep copy (Az*_clone);");
    builder.line(";;   (set-foo-field! obj v)    -- releases the old value (Az*_delete), then");
    builder.line(";;                                stores v. v is MOVED in: do not delete or");
    builder.line(";;                                reuse it (pass a fresh value or a -copy).");
    builder.line(";;                                A String field also takes a Racket string.");
    builder.line(";; ----------------------------------------------------------------------------");
    builder.blank();
    // Every procedure name the wrappers above defined: the module is one
    // namespace, so an accessor never redefines one (a duplicate `define`
    // fails to load) - the api.json method wins.
    let mut taken = std::collections::BTreeSet::new();
    for (name, category, generic_params) in ir
        .structs
        .iter()
        .map(|s| (&s.name, s.category, &s.generic_params))
        .chain(
            ir.enums
                .iter()
                .map(|e| (&e.name, e.category, &e.generic_params)),
        )
    {
        if !should_emit(name, category, generic_params, config) {
            continue;
        }
        let class = idiomatic_class_name(name);
        let funcs: Vec<&FunctionDef> = ir.functions_for_class(name).collect();
        let has_new = funcs.iter().any(|f| f.method_name == "new");
        for f in funcs.iter().filter(|f| !f.kind.is_trait_function()) {
            taken.insert(public_name(&class, f, has_new));
        }
    }
    for s in &ir.structs {
        if !should_emit(&s.name, s.category, &s.generic_params, config)
            || class_fn(&s.name, FunctionKind::Delete, ir, config).is_none()
        {
            continue;
        }
        emit_field_accessors(builder, s, ir, config, &mut taken);
    }

    Ok(())
}

/// The raw binding (its C name) of `class`'s function of `kind`, when emitted.
fn class_fn(
    class: &str,
    kind: FunctionKind,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<String> {
    ir.functions_for_class(class.trim())
        .find(|f| f.kind == kind && should_emit_function(f, ir, config))
        .map(|f| f.c_name.clone())
}

/// How one field of a resource-owning struct is read and written.
enum FieldShape {
    /// A C scalar, a unit enum, or a struct/union that owns no heap memory:
    /// define-cstruct's own accessor and mutator are the whole story.
    Plain,
    /// The String class: read as a Racket string; written from a Racket
    /// string or an AzString after the old one is released.
    Str { delete: String },
    /// A heap-owning struct/union: read as define-cstruct's view, copied
    /// with `_clone`, written after the old value is released.
    Owning {
        delete: String,
        clone: Option<String>,
    },
}

fn field_shape(f: &FieldDef, ir: &CodegenIR, config: &CodegenConfig) -> Option<FieldShape> {
    if !f.is_public || f.ref_kind != FieldRefKind::Owned {
        return None;
    }
    let t = f.type_name.trim();
    if t.contains('<') || t.starts_with('*') || t.starts_with('&') || t.starts_with('[') {
        return None;
    }
    // Callbacks, their wrappers and the type-erased handle are wired up by
    // the closure plumbing (register-callback / refany-create).
    if managed_host_invoker::is_callback_wrapper(ir, t)
        || managed_lang_helpers::is_refany_type(t, ir)
        || ir.callback_typedefs.iter().any(|c| c.name.trim() == t)
    {
        return None;
    }
    let ct = map_type_to_racket(t, ir);
    if ct == "_pointer" || ct == "_fpointer" || ct == "_void" {
        return None;
    }
    let is_type =
        ir.find_struct(t).is_some() || ir.find_enum(t).is_some() || ir.find_type_alias(t).is_some();
    if is_type && !config.should_include_type(t) {
        return None;
    }
    if !managed_lang_helpers::has_delete_function(t, ir) {
        return Some(FieldShape::Plain);
    }
    // A heap-owning field without a reachable `_delete` cannot be replaced
    // without leaking the old value: no accessor at all.
    let delete = class_fn(t, FunctionKind::Delete, ir, config)?;
    if ir
        .find_struct(t)
        .is_some_and(|s| s.category == TypeCategory::String)
    {
        return Some(FieldShape::Str { delete });
    }
    Some(FieldShape::Owning {
        delete,
        clone: class_fn(t, FunctionKind::DeepCopy, ir, config),
    })
}

/// `(<class>-<field> obj)`, `(<class>-<field>-copy obj)` and
/// `(set-<class>-<field>! obj v)` for the public by-value fields of one
/// resource-owning struct.
///
/// - The getter of a String field decodes it without consuming it; any
///   other getter is define-cstruct's own (a struct field is a view into
///   `obj`, so `(set-...! (<class>-<field> obj) ...)` writes through).
/// - `-copy` is an independent deep copy (`_clone`; none -> no `-copy`).
/// - The setter releases the field's old value (`_delete` on its address,
///   from the azul.h layout), then stores `v` - which is moved in.
///
/// An api.json method of the same name wins: the getter is skipped, the
/// setter is still emitted.
fn emit_field_accessors(
    builder: &mut CodeBuilder,
    s: &StructDef,
    ir: &CodegenIR,
    config: &CodegenConfig,
    taken: &mut std::collections::BTreeSet<String>,
) {
    let class = idiomatic_class_name(&s.name);
    let cn = c_name(&s.name);
    let Some(offsets) = c_layout::field_offsets_of(&s.fields, ir) else {
        return;
    };
    let mut wrote = false;
    for (f, off) in s.fields.iter().zip(offsets) {
        let Some(shape) = field_shape(f, ir, config) else {
            continue;
        };
        let field = field_ident(&f.name);
        let accessor = format!("{}-{}", class, field);
        let native_get = format!("({}-{} obj)", cn, field);
        let native_set = format!("set-{}-{}!", cn, field);
        let fp = format!("(ptr-add obj {})", off);
        if let Some(d) = &f.doc {
            builder.line(&format!(";; {}", d.replace(['\n', '\r'], " ")));
        }
        let getter = match &shape {
            FieldShape::Str { .. } => format!("(azul-string->string {})", native_get),
            FieldShape::Plain | FieldShape::Owning { .. } => native_get.clone(),
        };
        if taken.insert(accessor.clone()) {
            builder.line(&format!("(define ({} obj)", accessor));
            builder.line(&format!("  {})", getter));
        }
        if let FieldShape::Owning { clone: Some(c), .. } = &shape {
            let copy = format!("{}-copy", accessor);
            if taken.insert(copy.clone()) {
                builder.line(&format!("(define ({} obj)", copy));
                builder.line(&format!("  ({} {}))", c, fp));
            }
        }
        let setter = format!("set-{}!", accessor);
        if !taken.insert(setter.clone()) {
            continue;
        }
        builder.line(&format!("(define ({} obj v)", setter));
        match &shape {
            FieldShape::Plain => builder.line(&format!("  ({} obj v))", native_set)),
            FieldShape::Str { delete } => {
                builder.line("  (define new (if (string? v) (string->azul-string v) v))");
                builder.line(&format!("  ({} {})", delete, fp));
                builder.line(&format!("  ({} obj new))", native_set));
            }
            FieldShape::Owning { delete, .. } => {
                builder.line(&format!("  ({} {})", delete, fp));
                builder.line(&format!("  ({} obj v))", native_set));
            }
        }
        wrote = true;
    }
    if wrote {
        builder.blank();
    }
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
            Some(cb) => {
                format!("(register-callback \"{}\" {})", cb.callback_wrapper_name, p)
            }
            _ => p.clone(),
        })
        .collect();

    for d in &func.doc {
        builder.line(&format!(";; {}", d.replace('\n', " ")));
    }
    builder.line(&format!("(define ({} {})", public_name, params.join(" ")));
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
            if func.method_name == "new" || (func.method_name == "default" && !has_new) =>
        {
            format!("make-{}", class)
        }
        _ => format!("{}-{}", class, method),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::{bug_classes::ir, config::CodegenConfig};

    fn racket() -> &'static str {
        static OUT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
        OUT.get_or_init(|| super::super::generate(ir(), &CodegenConfig::c_header()).unwrap())
    }

    /// The text of the top-level form starting with `head`.
    fn form(head: &str) -> &'static str {
        let out = racket();
        let start = out
            .find(head)
            .unwrap_or_else(|| panic!("no `{head}` in azul.rkt"));
        let rest = &out[start..];
        let end = rest[1..].find("\n(").map_or(rest.len(), |e| e + 1);
        &rest[..end]
    }

    #[test]
    fn string_decoding_copies_the_bytes_without_make_sized_byte_string() {
        let decode = form("(define (azul-string->string az)");
        assert!(!decode.contains("make-sized-byte-string"), "{decode}");
        assert!(decode.contains("(memcpy b ptr len)"), "{decode}");
    }

    #[test]
    fn the_title_getter_decodes_the_field_without_consuming_it() {
        let get = form("(define (full-window-state-title obj)");
        assert!(
            get.contains("(azul-string->string (AzFullWindowState-title obj))"),
            "{get}"
        );
    }

    #[test]
    fn the_title_setter_releases_the_old_string_before_storing_the_new_one() {
        let set = form("(define (set-full-window-state-title! obj v)");
        let delete = set.find("(AzString_delete (ptr-add obj ").expect(set);
        let store = set
            .find("(set-AzFullWindowState-title! obj new)")
            .expect(set);
        assert!(delete < store, "{set}");
        assert!(set.contains("(string->azul-string v)"), "{set}");
    }

    #[test]
    fn the_window_state_is_a_view_with_a_deep_copy_and_a_releasing_setter() {
        let view = form("(define (window-create-options-window-state obj)");
        assert!(
            view.contains("(AzWindowCreateOptions-window-state obj)"),
            "{view}"
        );
        let copy = form("(define (window-create-options-window-state-copy obj)");
        assert!(
            copy.contains("(AzFullWindowState_clone (ptr-add obj "),
            "{copy}"
        );
        let set = form("(define (set-window-create-options-window-state! obj v)");
        assert!(
            set.contains("(AzFullWindowState_delete (ptr-add obj "),
            "{set}"
        );
        assert!(
            set.contains("(set-AzWindowCreateOptions-window-state! obj v)"),
            "{set}"
        );
    }

    #[test]
    fn the_text_field_stays_writable_next_to_the_get_text_method() {
        assert!(racket().contains("(define (text-input-state-get-text "));
        let set = form("(define (set-text-input-state-text! obj v)");
        assert!(set.contains("(AzU32Vec_delete (ptr-add obj "), "{set}");
    }
}
