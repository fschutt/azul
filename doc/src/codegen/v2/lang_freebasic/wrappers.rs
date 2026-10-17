//! Idiomatic FreeBASIC wrapper Types with `Constructor` / `Destructor`.
//!
//! For every IR struct that has a matching `<TypeName>_delete` C
//! function, we emit an idiomatic `Type` inside `Namespace Azul ...
//! End Namespace`. The wrapper:
//!
//! 1. Holds the underlying FFI record (`AzTypeName`) by value in a private `raw` field, plus an
//!    `owned` flag so `Wrap`-style factories can opt out of automatic deletion.
//! 2. Exposes a `Constructor (...)` overload per IR `FunctionKind::Constructor` /
//!    `FunctionKind::Default` method on the type. FreeBASIC supports overloaded constructors out of
//!    the box so we don't need to suffix names like in Pascal.
//! 3. Exposes a `Destructor ()` that calls `<TypeName>_delete(@raw)` when `owned` is true. The
//!    destructor fires automatically when a stack-allocated wrapper goes out of scope (or `Delete`
//!    is called on a heap-allocated one).
//! 4. A copy constructor and `Operator Let`: a copy gets its own deep copy (`_clone`); a type
//!    without `_clone` MOVES on copy (the source stops owning). Without them FreeBASIC copied the
//!    bytes and both copies freed the same memory.
//! 5. Surfaces every non-trait method as an idiomatic instance method calling the C symbol the
//!    `Extern` block declares (`func.c_name`); a by-value receiver hands `this.raw` over and the
//!    wrapper stops owning it.
//! 6. One `Property <Field>` (get + set) per public by-value field: the getter returns an
//!    independent value (FreeBASIC `String` for the string class, decoded without consuming the
//!    field; a wrapper holding a deep copy for heap-owning wrapped types; a plain copy
//!    otherwise), the setter releases the old value and moves the new one in (a wrapper argument
//!    is consumed through `TakeRaw`). An api.json method of the same name wins: the property is
//!    then called `<Field>Field`. Nested fields are read-modify-write:
//!
//!    ```text
//!    Dim ws As Azul.FullWindowState = opts.WindowState   ' deep copy
//!    ws.Title = "Hello"                                  ' releases the old title
//!    Dim sz As AzWindowSize = ws.Size
//!    sz.dimensions.width = 800
//!    ws.Size = sz
//!    opts.WindowState = ws                               ' consumes ws
//!    ```
//!
//! User-facing names drop the `Az` prefix:  `AzApp`  →  `Azul.App`,
//! `AzDom` → `Azul.Dom`, etc. (a name that is a FreeBASIC keyword gets a
//! trailing `_`: `Azul.String_`). The names are emitted as nested types
//! inside `Namespace Azul`. Plain POD structs without a `_delete` get
//! no wrapper.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;

use super::{
    super::{
        config::CodegenConfig,
        field_access_classic::{self as fa, AccessField, FieldKind},
        generator::CodeBuilder,
        ir::{
            ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
        },
    },
    ffi_type_name, map_type_to_fb, sanitize_comment, sanitize_identifier, to_pascal_case,
    types::ptr_type_for_arg,
};

/// Module-level helper: `AzString` -> FreeBASIC `String`, NOT consuming.
const STRING_READ_HELPER: &str = "AzulStringRead";
/// Module-level helper: FreeBASIC `String` -> fresh `AzString`.
const STRING_NEW_HELPER: &str = "AzulStringNew";

// ============================================================================
// Public entry point
// ============================================================================

pub fn generate_wrappers(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    let targets = collect_wrapper_targets(ir, config);
    if targets.is_empty() {
        return Ok(());
    }
    let target_names: BTreeSet<&str> = targets.iter().map(|s| s.name.as_str()).collect();
    let has_string_helpers = emit_string_helpers(builder, ir);
    let classes: Vec<ClassInfo> = targets
        .iter()
        .map(|s| ClassInfo::new(s, ir, config, &target_names, has_string_helpers))
        .collect();

    builder.line("' --------------------------------------------------------------------");
    builder.line("' Idiomatic wrappers — Constructor/Destructor handle native ownership.");
    builder.line("' Use:   Dim app As Azul.App = Azul.App(data, cfg)   ' auto-cleanup");
    builder.line("' Fields are properties: ws.Title = \"Hi\" (old value released). Nested:");
    builder.line("'   Dim ws As Azul.FullWindowState = opts.WindowState  ' deep copy");
    builder.line("'   ws.Title = \"Hi\" : opts.WindowState = ws           ' consumes ws");
    builder.line("' --------------------------------------------------------------------");
    builder.blank();

    builder.line("Namespace Azul");
    builder.indent();

    for c in &classes {
        emit_wrapper_decl(builder, c, ir);
    }

    builder.dedent();
    builder.line("End Namespace");
    builder.blank();

    // Method bodies live OUTSIDE the Namespace block but reference
    // `Azul.<Name>` qualified names. FreeBASIC accepts both forms;
    // we prefer outside-of-namespace bodies so multi-line definitions
    // are easier to read.
    for c in &classes {
        emit_wrapper_impl(builder, c, ir);
    }

    Ok(())
}

// ============================================================================
// Discovery
// ============================================================================

/// The wrapper targets, ordered so a Type is declared before any Type whose
/// properties return it by value (FreeBASIC needs the complete type there).
fn collect_wrapper_targets<'a>(ir: &'a CodegenIR, config: &CodegenConfig) -> Vec<&'a StructDef> {
    let delete_set: BTreeSet<&str> = ir
        .functions
        .iter()
        .filter(|f| f.kind == FunctionKind::Delete)
        .map(|f| f.class_name.as_str())
        .collect();

    let targets: Vec<&StructDef> = ir
        .structs
        .iter()
        .filter(|s| should_emit_wrapper(s, config) && delete_set.contains(s.name.as_str()))
        .collect();
    let by_name: BTreeMap<&str, &StructDef> = targets.iter().map(|s| (s.name.as_str(), *s)).collect();
    let mut out: Vec<&StructDef> = Vec::with_capacity(targets.len());
    let mut done: BTreeSet<&str> = BTreeSet::new();
    fn visit<'a>(
        s: &'a StructDef,
        by_name: &BTreeMap<&str, &'a StructDef>,
        done: &mut BTreeSet<&'a str>,
        out: &mut Vec<&'a StructDef>,
    ) {
        if !done.insert(s.name.as_str()) {
            return;
        }
        for f in &s.fields {
            if let Some(dep) = by_name.get(f.type_name.trim()) {
                visit(*dep, by_name, done, out);
            }
        }
        out.push(s);
    }
    for s in &targets {
        visit(*s, &by_name, &mut done, &mut out);
    }
    out
}

fn should_emit_wrapper(s: &StructDef, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    !matches!(
        s.category,
        TypeCategory::Recursive
            | TypeCategory::VecRef
            | TypeCategory::DestructorOrClone
            | TypeCategory::GenericTemplate
    )
}

// ============================================================================
// Per-class plan
// ============================================================================

struct ClassInfo<'a> {
    s: &'a StructDef,
    /// The Type's name inside `Namespace Azul`.
    name: String,
    clone: Option<&'a FunctionDef>,
    delete: Option<&'a FunctionDef>,
    methods: Vec<(&'a FunctionDef, String)>,
    props: Vec<PropPlan<'a>>,
}

struct PropPlan<'a> {
    name: String,
    a: AccessField<'a>,
    /// The FreeBASIC type the property traffics in (unqualified, as the
    /// declaration inside the namespace spells it).
    ty: String,
    /// Same, as a body outside the namespace spells it.
    ty_qualified: String,
    /// The field's type has a wrapper Type (`Azul.<W>`).
    wrapped: bool,
}

impl<'a> ClassInfo<'a> {
    fn new(
        s: &'a StructDef,
        ir: &'a CodegenIR,
        config: &CodegenConfig,
        targets: &BTreeSet<&str>,
        has_string_helpers: bool,
    ) -> Self {
        let methods: Vec<(&FunctionDef, String)> = ir
            .functions_for_class(&s.name)
            .filter(|f| {
                !matches!(
                    f.kind,
                    FunctionKind::Constructor | FunctionKind::Default | FunctionKind::Delete
                ) && !f.kind.is_trait_function()
            })
            .map(|f| (f, idiomatic_method_name(&f.method_name)))
            .collect();
        let name = wrapper_type_name(&s.name);
        let mut taken: BTreeSet<String> = ["raw", "owned", "getraw", "takeraw"]
            .iter()
            .map(|n| n.to_string())
            .collect();
        taken.insert(name.to_ascii_lowercase());
        // A property named like a wrapper Type would hide that Type inside
        // this one (`Declare Property Dom () As Dom`).
        for t in targets {
            taken.insert(wrapper_type_name(t).to_ascii_lowercase());
        }
        for (_, m) in &methods {
            taken.insert(m.to_ascii_lowercase());
        }
        let mut props = Vec::new();
        for a in fa::accessible_fields(s, ir, config) {
            let wrapped = targets.contains(a.ty);
            let (ty, ty_qualified) = match a.kind {
                FieldKind::Prim { is_bool: true } => ("Boolean".to_string(), "Boolean".to_string()),
                FieldKind::Str { .. } => {
                    if !has_string_helpers {
                        continue;
                    }
                    // allow-api-name: FreeBASIC's own `String`, not the API class.
                    ("String".to_string(), "String".to_string())
                }
                FieldKind::Value { .. } if wrapped => {
                    let w = wrapper_type_name(a.ty);
                    (w.clone(), format!("Azul.{}", w))
                }
                _ => {
                    let t = map_type_to_fb(a.ty, ir);
                    if t.contains("Ptr") || t.starts_with("__FB_ARRAY__") {
                        continue;
                    }
                    (t.clone(), t)
                }
            };
            let base = sanitize_identifier(&to_pascal_case(a.field.name.trim_start_matches('_')));
            let name = if taken.contains(&base.to_ascii_lowercase()) {
                format!("{}Field", base)
            } else {
                base
            };
            if !taken.insert(name.to_ascii_lowercase()) {
                continue;
            }
            props.push(PropPlan {
                name,
                a,
                ty,
                ty_qualified,
                wrapped,
            });
        }
        ClassInfo {
            s,
            name,
            clone: fa::clone_fn(ir, &s.name),
            delete: fa::delete_fn(ir, &s.name),
            methods,
            props,
        }
    }
}

/// `AzulStringRead` / `AzulStringNew` at module level (before the
/// namespace). Returns whether they exist (they need the string class's
/// byte layout and its copying constructor).
fn emit_string_helpers(builder: &mut CodeBuilder, ir: &CodegenIR) -> bool {
    let (Some(st), Some(copy), Some((vec, ptr, len))) =
        (fa::string_class(ir), fa::string_copy_fn(ir), fa::string_layout(ir))
    else {
        return false;
    };
    let az = ffi_type_name(&st.name);
    let (vec, ptr, len) = (sanitize_identifier(&vec), sanitize_identifier(&ptr), sanitize_identifier(&len));
    builder.line("' String fields: decoded WITHOUT consuming the field / copied in.");
    builder.line(&format!("Function {} (ByRef s As {}) As String", STRING_READ_HELPER, az));
    builder.indent();
    builder.line(&format!("Dim n As Integer = CInt(s.{}.{})", vec, len));
    builder.line("Dim r As String = Space(n)");
    builder.line(&format!("Dim p As UByte Ptr = CPtr(UByte Ptr, s.{}.{})", vec, ptr));
    builder.line("For i As Integer = 0 To n - 1");
    builder.line("    r[i] = p[i]");
    builder.line("Next");
    builder.line("Return r");
    builder.dedent();
    builder.line("End Function");
    builder.blank();
    builder.line(&format!("Function {} (ByRef s As Const String) As {}", STRING_NEW_HELPER, az));
    builder.indent();
    builder.line(&format!(
        "Return {}(CPtr(UByte Ptr, StrPtr(s)), 0, Len(s))",
        copy.c_name
    ));
    builder.dedent();
    builder.line("End Function");
    builder.blank();
    true
}

// ============================================================================
// Type declaration (interface side)
// ============================================================================

fn emit_wrapper_decl(builder: &mut CodeBuilder, c: &ClassInfo, ir: &CodegenIR) {
    let s = c.s;
    let class_name = &c.name;
    let raw_record = ffi_type_name(&s.name);

    if !s.doc.is_empty() {
        for d in &s.doc {
            builder.line(&format!("' {}", sanitize_comment(d)));
        }
    }

    builder.line(&format!("Type {}", class_name));
    builder.indent();

    builder.line("Private:");
    builder.indent();
    builder.line(&format!("raw As {}", raw_record));
    builder.line("owned As Boolean");
    builder.dedent();

    builder.line("Public:");
    builder.indent();

    // Wrap-existing constructor. Takes a raw FFI record and assumes
    // ownership. Useful when an FFI function returns `AzFoo` by value
    // and the caller wants to wrap it.
    builder.line(&format!(
        "Declare Constructor (ByVal raw_in As {})",
        raw_record
    ));
    // Copies deep-copy (or move, without a `_clone`).
    builder.line(&format!("Declare Constructor (ByRef other As {})", class_name));
    builder.line(&format!("Declare Operator Let (ByRef other As {})", class_name));

    // One Constructor per IR Constructor / Default function.
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_constructor_decl(builder, func, ir);
    }

    builder.line("Declare Destructor ()");

    // Read-only access to the raw FFI record (escape hatch).
    builder.line(&format!("Declare Property GetRaw () As {}", raw_record));
    // Hand the record over (moved when owned, deep-copied when borrowed).
    builder.line(&format!("Declare Function TakeRaw () As {}", raw_record));

    // Instance / static methods.
    for (func, name) in &c.methods {
        emit_method_decl(builder, func, name, ir);
    }

    for p in &c.props {
        builder.line(&format!("Declare Property {} () As {}", p.name, p.ty));
        builder.line(&format!("Declare Property {} ({})", p.name, setter_param(p)));
    }

    builder.dedent();
    builder.dedent();
    builder.line("End Type");
    builder.blank();
}

/// The setter's parameter clause.
fn setter_param(p: &PropPlan) -> String {
    match p.a.kind {
        FieldKind::Prim { .. } | FieldKind::UnitEnum => format!("ByVal v As {}", p.ty),
        FieldKind::Str { .. } => "ByRef v As Const String".to_string(),
        FieldKind::Value { .. } => format!("ByRef v As {}", p.ty),
    }
}

fn emit_constructor_decl(builder: &mut CodeBuilder, func: &FunctionDef, ir: &CodegenIR) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    if args_str.is_empty() {
        builder.line("Declare Constructor ()");
    } else {
        builder.line(&format!("Declare Constructor ({})", args_str));
    }
}

fn emit_method_decl(builder: &mut CodeBuilder, func: &FunctionDef, method_name: &str, ir: &CodegenIR) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let is_static = !takes_self(func);

    let prefix = if is_static {
        "Declare Static "
    } else {
        "Declare "
    };

    if let Some(ret) = &func.return_type {
        let fb_ret = map_type_to_fb(ret, ir);
        if args_str.is_empty() {
            builder.line(&format!(
                "{}Function {} () As {}",
                prefix, method_name, fb_ret
            ));
        } else {
            builder.line(&format!(
                "{}Function {} ({}) As {}",
                prefix, method_name, args_str, fb_ret
            ));
        }
    } else if args_str.is_empty() {
        builder.line(&format!("{}Sub {} ()", prefix, method_name));
    } else {
        builder.line(&format!("{}Sub {} ({})", prefix, method_name, args_str));
    }
}

// ============================================================================
// Type implementation (method-body side)
// ============================================================================

fn emit_wrapper_impl(builder: &mut CodeBuilder, c: &ClassInfo, ir: &CodegenIR) {
    let s = c.s;
    let class_name = format!("Azul.{}", c.name);
    let raw_record = ffi_type_name(&s.name);

    // Wrap-from-raw constructor.
    builder.line(&format!(
        "Constructor {} (ByVal raw_in As {})",
        class_name, raw_record
    ));
    builder.indent();
    builder.line("this.raw = raw_in");
    builder.line("this.owned = True");
    builder.dedent();
    builder.line("End Constructor");
    builder.blank();

    // Copy constructor + Let: the copy owns a deep copy of its own.
    let copy_from_other = |builder: &mut CodeBuilder| match c.clone {
        Some(clone) => {
            builder.line("If other.owned Then");
            builder.line(&format!("    this.raw = {}(@other.raw)", clone.c_name));
            builder.line("    this.owned = True");
            builder.line("Else");
            builder.line("    this.raw = other.raw");
            builder.line("    this.owned = False");
            builder.line("End If");
        }
        None => {
            builder.line("' No deep copy in the C API: the copy takes ownership over.");
            builder.line("this.raw = other.raw");
            builder.line("this.owned = other.owned");
            builder.line("other.owned = False");
        }
    };
    builder.line(&format!(
        "Constructor {} (ByRef other As {})",
        class_name, class_name
    ));
    builder.indent();
    copy_from_other(builder);
    builder.dedent();
    builder.line("End Constructor");
    builder.blank();
    builder.line(&format!(
        "Operator {}.Let (ByRef other As {})",
        class_name, class_name
    ));
    builder.indent();
    builder.line("If @other = @this Then Exit Operator");
    if let Some(del) = c.delete {
        builder.line(&format!("If this.owned Then {}(@this.raw)", del.c_name));
    }
    copy_from_other(builder);
    builder.dedent();
    builder.line("End Operator");
    builder.blank();

    // Constructors from IR.
    for func in ir.functions_for_class(&s.name) {
        if !matches!(func.kind, FunctionKind::Constructor | FunctionKind::Default) {
            continue;
        }
        emit_constructor_impl(builder, &class_name, func, ir);
    }

    // Destructor.
    builder.line(&format!("Destructor {} ()", class_name));
    builder.indent();
    if let Some(del) = c.delete {
        builder.line("If this.owned Then");
        builder.indent();
        builder.line(&format!("{}(@this.raw)", del.c_name));
        builder.dedent();
        builder.line("End If");
    }
    builder.dedent();
    builder.line("End Destructor");
    builder.blank();

    // Raw accessor.
    builder.line(&format!(
        "Property {}.GetRaw () As {}",
        class_name, raw_record
    ));
    builder.indent();
    builder.line("Return this.raw");
    builder.dedent();
    builder.line("End Property");
    builder.blank();

    builder.line(&format!("Function {}.TakeRaw () As {}", class_name, raw_record));
    builder.indent();
    builder.line("If this.owned Then");
    builder.line("    this.owned = False");
    builder.line("    Return this.raw");
    builder.line("End If");
    match c.clone {
        Some(clone) => builder.line(&format!("Return {}(@this.raw)", clone.c_name)),
        None => {
            builder.line("' Borrowed and no deep copy: cannot be handed over.");
            builder.line("Error 5");
            builder.line("Return this.raw");
        }
    }
    builder.dedent();
    builder.line("End Function");
    builder.blank();

    // Instance / static methods.
    for (func, name) in &c.methods {
        emit_method_impl(builder, &class_name, func, name, ir);
    }

    for p in &c.props {
        emit_property_impl(builder, &class_name, p);
    }
}

fn emit_property_impl(builder: &mut CodeBuilder, class_name: &str, p: &PropPlan) {
    let fexpr = format!("this.raw.{}", sanitize_identifier(&p.a.field.name));
    let release = |builder: &mut CodeBuilder, delete: Option<&FunctionDef>| {
        if let Some(d) = delete {
            builder.line(&format!("{}(@{})", d.c_name, fexpr));
        }
    };
    // Getter (skipped when the value owns heap memory it cannot deep-copy).
    let getter: Option<String> = match p.a.kind {
        FieldKind::Prim { is_bool: true } => Some(format!("Return {} <> 0", fexpr)),
        FieldKind::Prim { .. } | FieldKind::UnitEnum => Some(format!("Return {}", fexpr)),
        FieldKind::Str { .. } => Some(format!("Return {}({})", STRING_READ_HELPER, fexpr)),
        FieldKind::Value { delete, clone } => match (delete, clone, p.wrapped) {
            (_, Some(c), true) => Some(format!("Return {}({}(@{}))", p.ty_qualified, c.c_name, fexpr)),
            (None, _, true) => Some(format!("Return {}({})", p.ty_qualified, fexpr)),
            (_, Some(c), false) => Some(format!("Return {}(@{})", c.c_name, fexpr)),
            (None, None, false) => Some(format!("Return {}", fexpr)),
            (Some(_), None, _) => None,
        },
    };
    if let Some(g) = getter {
        builder.line(&format!(
            "Property {}.{} () As {}",
            class_name, p.name, p.ty_qualified
        ));
        builder.indent();
        builder.line(&g);
        builder.dedent();
        builder.line("End Property");
        builder.blank();
    }
    let param = match p.a.kind {
        FieldKind::Value { .. } => format!("ByRef v As {}", p.ty_qualified),
        _ => setter_param(p),
    };
    builder.line(&format!("Property {}.{} ({})", class_name, p.name, param));
    builder.indent();
    match p.a.kind {
        FieldKind::Prim { is_bool: true } => builder.line(&format!("{} = IIf(v, 1, 0)", fexpr)),
        FieldKind::Prim { .. } | FieldKind::UnitEnum => builder.line(&format!("{} = v", fexpr)),
        FieldKind::Str { delete } => {
            builder.line(&format!(
                "Dim nv As {} = {}(v)",
                ffi_type_name(p.a.ty),
                STRING_NEW_HELPER
            ));
            release(builder, Some(delete));
            builder.line(&format!("{} = nv", fexpr));
        }
        FieldKind::Value { delete, .. } if p.wrapped => {
            // Consumed: `v` stops owning (or hands over a deep copy).
            builder.line(&format!("Dim nv As {} = v.TakeRaw()", ffi_type_name(p.a.ty)));
            release(builder, delete);
            builder.line(&format!("{} = nv", fexpr));
        }
        FieldKind::Value { delete, .. } => {
            // A raw record: the field takes over its heap memory.
            release(builder, delete);
            builder.line(&format!("{} = v", fexpr));
        }
    }
    builder.dedent();
    builder.line("End Property");
    builder.blank();
}

fn emit_constructor_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    ir: &CodegenIR,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);

    let signature = if args_str.is_empty() {
        format!("Constructor {} ()", class_name)
    } else {
        format!("Constructor {} ({})", class_name, args_str)
    };
    builder.line(&signature);
    builder.indent();

    let call_args: Vec<String> = visible
        .iter()
        .map(|a| sanitize_identifier(&a.name))
        .collect();

    let returns_self = func
        .return_type
        .as_deref()
        .map(|r| r.trim() == func.class_name)
        .unwrap_or(false);

    // The symbol the Extern block declares - never a respelling of it.
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    if returns_self {
        builder.line(&format!("this.raw = {}", call));
        builder.line("this.owned = True");
    } else {
        builder.line(&format!(
            "' SKIPPED: constructor returns {:?}, not {}",
            func.return_type, func.class_name
        ));
        builder.line(&call);
        builder.line("this.owned = False");
    }
    builder.dedent();
    builder.line("End Constructor");
    builder.blank();
}

fn emit_method_impl(
    builder: &mut CodeBuilder,
    class_name: &str,
    func: &FunctionDef,
    method_name: &str,
    ir: &CodegenIR,
) {
    let visible = visible_user_args(func);
    let args_str = format_arg_list(&visible, ir);
    let is_static = !takes_self(func);
    let consumes_self = takes_self(func)
        && func
            .args
            .first()
            .is_some_and(|a| a.ref_kind == ArgRefKind::Owned);

    let signature = if let Some(ret) = &func.return_type {
        let fb_ret = map_type_to_fb(ret, ir);
        if args_str.is_empty() {
            format!(
                "{}Function {}.{} () As {}",
                if is_static { "Static " } else { "" },
                class_name,
                method_name,
                fb_ret
            )
        } else {
            format!(
                "{}Function {}.{} ({}) As {}",
                if is_static { "Static " } else { "" },
                class_name,
                method_name,
                args_str,
                fb_ret
            )
        }
    } else if args_str.is_empty() {
        format!(
            "{}Sub {}.{} ()",
            if is_static { "Static " } else { "" },
            class_name,
            method_name
        )
    } else {
        format!(
            "{}Sub {}.{} ({})",
            if is_static { "Static " } else { "" },
            class_name,
            method_name,
            args_str
        )
    };

    builder.line(&signature);
    builder.indent();

    let mut call_args: Vec<String> = Vec::new();
    if takes_self(func) {
        // The receiver: the record itself when the C function consumes it
        // (by value), else a pointer to it.
        call_args.push(if consumes_self { "this.raw" } else { "@this.raw" }.to_string());
    }
    for a in &visible {
        call_args.push(sanitize_identifier(&a.name));
    }

    // The symbol the Extern block declares (`AzTextInputState_getText`),
    // never `<ffi>_<method_name>` (`..._get_text`, which does not exist).
    let call = format!("{}({})", func.c_name, call_args.join(", "));

    if func.return_type.is_some() {
        if consumes_self {
            builder.line(&format!(
                "Dim r_ As {} = {}",
                map_type_to_fb(func.return_type.as_deref().unwrap_or_default(), ir),
                call
            ));
            builder.line("this.owned = False ' the call took the record");
            builder.line("Return r_");
        } else {
            builder.line(&format!("Return {}", call));
        }
    } else {
        builder.line(&call);
        if consumes_self {
            builder.line("this.owned = False ' the call took the record");
        }
    }
    builder.dedent();
    if func.return_type.is_some() {
        builder.line("End Function");
    } else {
        builder.line("End Sub");
    }
    builder.blank();
}

// ============================================================================
// Argument helpers
// ============================================================================

/// Does `func` take the wrapper's record as its first argument? Instance,
/// mutating and deep-copy functions do, whatever api.json named that
/// argument (`instance`, the snake-cased class, ...).
fn takes_self(func: &FunctionDef) -> bool {
    matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    ) && !func.args.is_empty()
}

/// The user-visible arguments: everything but the receiver.
fn visible_user_args(func: &FunctionDef) -> Vec<&FunctionArg> {
    if takes_self(func) {
        func.args.iter().skip(1).collect()
    } else {
        func.args.iter().collect()
    }
}

fn format_arg_list(args: &[&FunctionArg], ir: &CodegenIR) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| {
            let fb_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_fb(&a.type_name, ir),
                ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    ptr_type_for_arg(&a.type_name, ir)
                }
            };
            format!("ByVal {} As {}", sanitize_identifier(&a.name), fb_ty)
        })
        .collect();
    parts.join(", ")
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Idiomatic wrapper type name — drop the `Az` prefix, keep the rest;
/// a FreeBASIC keyword (`String`) gets a trailing `_` like every other
/// identifier this binding sanitizes.
fn wrapper_type_name(raw: &str) -> String {
    sanitize_identifier(raw.strip_prefix("Az").unwrap_or(raw))
}

/// Convert an api.json method name (snake_case / camelCase / "new") to
/// an idiomatic FreeBASIC method name (PascalCase). `new` is reserved
/// by FreeBASIC and is mapped to `Create`.
fn idiomatic_method_name(method_name: &str) -> String {
    if method_name == "new" {
        return "Create".to_string();
    }
    if method_name.contains('_') {
        return to_pascal_case(method_name);
    }
    let mut chars = method_name.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod field_access_tests {
    use std::sync::OnceLock;

    use super::super::super::config::CodegenConfig;

    fn generated() -> &'static str {
        static OUT: OnceLock<String> = OnceLock::new();
        OUT.get_or_init(|| {
            let ir = crate::codegen::v2::bug_classes::ir();
            super::super::generate(ir, &CodegenConfig::c_header()).expect("freebasic codegen")
        })
    }

    /// One member body, from its header line to the next `End <kind>`.
    fn body_of(header: &str, kind: &str) -> &'static str {
        let src = generated();
        let start = src.find(header).unwrap_or_else(|| panic!("no `{}`", header));
        let end = src[start..].find(&format!("End {}", kind)).expect("end of member");
        &src[start..start + end]
    }

    #[test]
    fn wrapper_methods_call_the_declared_c_symbol() {
        let get = body_of("Function Azul.TextInputState.GetText () As AzString", "Function");
        assert!(get.contains("AzTextInputState_getText(@this.raw)"), "{}", get);
        assert!(!generated().contains("AzTextInputState_get_text("));
    }

    #[test]
    fn clone_takes_no_extra_argument() {
        assert!(generated().contains("Declare Function Clone () As AzTextInputState"));
        let c = body_of("Function Azul.TextInputState.Clone () As AzTextInputState", "Function");
        assert!(c.contains("AzTextInputState_clone(@this.raw)"), "{}", c);
    }

    #[test]
    fn the_string_wrapper_is_not_named_after_the_reserved_word() {
        assert!(!generated().contains("\n    Type String\n"));
        assert!(generated().contains("\n    Type String_\n"));
    }

    #[test]
    fn copying_a_wrapper_deep_copies_it_through_a_copy_constructor_and_let() {
        let src = generated();
        assert!(src.contains("Declare Constructor (ByRef other As FullWindowState)"));
        assert!(src.contains("Declare Operator Let (ByRef other As FullWindowState)"));
        let ctor = body_of("Constructor Azul.FullWindowState (ByRef other As Azul.FullWindowState)", "Constructor");
        assert!(ctor.contains("this.raw = AzFullWindowState_clone(@other.raw)"), "{}", ctor);
        let assign = body_of("Operator Azul.FullWindowState.Let (ByRef other As Azul.FullWindowState)", "Operator");
        assert!(assign.contains("AzFullWindowState_delete(@this.raw)"), "{}", assign);
    }

    #[test]
    fn the_window_title_is_a_string_property_that_releases_the_old_value() {
        let src = generated();
        assert!(src.contains("Declare Property Title () As String"));
        assert!(src.contains("Declare Property Title (ByRef v As Const String)"));
        let set = body_of("Property Azul.FullWindowState.Title (ByRef v As Const String)", "Property");
        assert!(set.contains("AzString_delete(@this.raw.title)"), "{}", set);
        let get = body_of("Property Azul.FullWindowState.Title () As String", "Property");
        assert!(!get.contains("_delete"), "reading must not free the field:\n{}", get);
    }

    #[test]
    fn the_window_state_property_deep_copies_and_consumes() {
        let src = generated();
        assert!(src.contains("Declare Property WindowState () As FullWindowState"));
        let get = body_of("Property Azul.WindowCreateOptions.WindowState () As Azul.FullWindowState", "Property");
        assert!(get.contains("AzFullWindowState_clone(@this.raw.window_state)"), "{}", get);
        let set = body_of(
            "Property Azul.WindowCreateOptions.WindowState (ByRef v As Azul.FullWindowState)",
            "Property",
        );
        assert!(set.contains("v.TakeRaw()"), "the argument is consumed:\n{}", set);
        assert!(set.contains("AzFullWindowState_delete(@this.raw.window_state)"), "{}", set);
    }

    #[test]
    fn a_text_input_text_field_is_writable_even_though_get_text_is_a_method() {
        assert!(generated().contains("Declare Property Text (ByRef v As U32Vec)"));
    }
}
