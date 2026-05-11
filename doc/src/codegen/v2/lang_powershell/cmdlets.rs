//! Emits idiomatic PowerShell `function Verb-AzulNoun { ... }` shims that
//! call into the embedded C# wrapper classes (compiled at module-import
//! time via `Add-Type`).
//!
//! Strategy:
//! - Walk every IR struct that has a `_delete` (i.e. that the C# generator
//!   produced an `IDisposable` wrapper class for) and emit per-method
//!   PowerShell functions that delegate to the C# wrapper.
//! - Also walk `FunctionKind::Constructor` / `StaticMethod` to emit
//!   `New-Azul<Type>` and `Get-Azul<Type><Method>` style shims.
//! - Method-name → PowerShell verb is derived from the IR `method_name`
//!   using a small lookup table (`new`/`create` → `New`,
//!   `delete`/`drop`/`free` → `Remove`, `run` → `Invoke`,
//!   `get_*` → `Get`, `set_*` → `Set`, `add_*` → `Add`, `clear_*` →
//!   `Clear`, `update_*` → `Update`, `start_*` → `Start`, `stop_*` →
//!   `Stop`). Anything that does not fit a standard verb falls through to
//!   `Invoke-` (PowerShell's documented catch-all for "perform an action"
//!   per `Get-Verb`).
//!
//! The shims are intentionally thin: each one does parameter binding
//! (with `[Parameter(...)]` attributes for pipeline support on the
//! instance argument) and forwards to the embedded `Azul.<Class>` C#
//! type. We do *not* try to translate PowerShell's `[hashtable]`
//! splatting into native struct construction; users who need that should
//! reach for `[Azul.Az<Type>]::new()` directly and set fields.

use anyhow::Result;

use super::super::config::CodegenConfig;
use super::super::generator::CodeBuilder;
use super::super::ir::{
    ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, StructDef, TypeCategory,
};

/// Maximum number of functions to emit a `Verb-Noun` shim for. The full
/// IR has ~thousands of trait helpers; we stop after a sane working set
/// to keep `Azul.psm1` readable. Trait helpers (`_delete`, `_partialEq`,
/// etc.) are filtered out before this cap kicks in.
const SHIM_BUDGET: usize = usize::MAX;

/// PowerShell module prefix for the noun half of every `Verb-Noun` name.
/// PowerShell convention: every cmdlet from a module shares a noun prefix
/// (e.g. `Get-AzVM` for the Az module). We use `Azul` as the noun prefix
/// so collisions with the official Az module from Microsoft are avoided.
pub const NOUN_PREFIX: &str = "Azul";

// ============================================================================
// Public entry point
// ============================================================================

/// Append the `function Verb-AzulNoun { … }` block plus the closing
/// `Export-ModuleMember` to `builder`.
pub fn generate_cmdlets(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Result<()> {
    builder.line(
        "# --------------------------------------------------------------------------",
    );
    builder.line("# Idiomatic PowerShell function shims (Verb-AzulNoun convention).");
    builder.line("# Each shim forwards to the embedded C# wrapper class via Add-Type.");
    builder.line(
        "# --------------------------------------------------------------------------",
    );
    builder.blank();

    let mut emitted: usize = 0;

    // Emit shims by walking structs that have wrapper classes (i.e. that
    // have a `_delete`). For each wrapper class we also pull in its
    // user-facing methods.
    for s in &ir.structs {
        if !should_emit_for_struct(s, ir, config) {
            continue;
        }
        for func in ir.functions_for_class(&s.name) {
            if emitted >= SHIM_BUDGET {
                break;
            }
            if !should_emit_function(func) {
                continue;
            }
            emit_shim(builder, s, func);
            emitted += 1;
        }
    }

    // Emit a single Export-ModuleMember call covering every Verb-Azul*
    // pattern. Wildcards keep this terse; the manifest is the second
    // filter.
    builder.line("Export-ModuleMember -Function @(");
    builder.indent();
    builder.line("'New-Azul*',");
    builder.line("'Invoke-Azul*',");
    builder.line("'Get-Azul*',");
    builder.line("'Set-Azul*',");
    builder.line("'Remove-Azul*',");
    builder.line("'Add-Azul*',");
    builder.line("'Clear-Azul*',");
    builder.line("'Update-Azul*',");
    builder.line("'Start-Azul*',");
    builder.line("'Stop-Azul*'");
    builder.dedent();
    builder.line(")");
    builder.blank();

    Ok(())
}

// ============================================================================
// Inclusion filters (mirrors lang_csharp/wrappers.rs)
// ============================================================================

fn should_emit_for_struct(s: &StructDef, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !config.should_include_type(&s.name) {
        return false;
    }
    if !s.generic_params.is_empty() {
        return false;
    }
    match s.category {
        TypeCategory::Recursive
        | TypeCategory::VecRef
        | TypeCategory::DestructorOrClone
        | TypeCategory::GenericTemplate => return false,
        _ => {}
    }
    has_delete_function(&s.name, ir)
}

fn has_delete_function(type_name: &str, ir: &CodegenIR) -> bool {
    ir.functions
        .iter()
        .any(|f| f.class_name == type_name && f.kind == FunctionKind::Delete)
}

fn should_emit_function(func: &FunctionDef) -> bool {
    // Skip trait functions — `Dispose` covers `_delete`, equality
    // operators are not idiomatic in PowerShell pipelines.
    if func.kind.is_trait_function() {
        return false;
    }
    // Skip enum-variant constructors; users pick variants via
    // `[Azul.Az<Enum>_Tag]::Variant` or the helper class.
    if matches!(func.kind, FunctionKind::EnumVariantConstructor) {
        return false;
    }
    true
}

// ============================================================================
// Shim emission
// ============================================================================

fn emit_shim(builder: &mut CodeBuilder, s: &StructDef, func: &FunctionDef) {
    let verb = pick_verb(&func.method_name, &func.kind);
    let noun = pick_noun(&s.name, &func.method_name, &func.kind);
    let func_name = format!("{}-{}{}", verb, NOUN_PREFIX, noun);

    let is_static = matches!(
        func.kind,
        FunctionKind::Constructor | FunctionKind::StaticMethod | FunctionKind::Default
    );
    let takes_self = matches!(
        func.kind,
        FunctionKind::Method | FunctionKind::MethodMut | FunctionKind::DeepCopy
    );

    // User-facing arguments — strip the implicit self placeholder used by
    // the C# wrapper (its name matches the lowercased class name).
    let class_lower = s.name.to_lowercase();
    let user_args: Vec<&FunctionArg> = func
        .args
        .iter()
        .filter(|a| a.name != class_lower && a.name != "self")
        .collect();

    // Doc comment block (PowerShell comment-based help).
    builder.line(&format!("function {} {{", func_name));
    builder.indent();
    if !func.doc.is_empty() {
        builder.line("<#");
        builder.line(".SYNOPSIS");
        builder.line(&format!(
            "    {}.{} — {} wrapper.",
            s.name, func.method_name, verb
        ));
        builder.line(".DESCRIPTION");
        for d in &func.doc {
            builder.line(&format!("    {}", ps_doc_escape(d)));
        }
        builder.line("#>");
    }

    // param() block.
    builder.line("[CmdletBinding()]");
    builder.line("param(");
    builder.indent();

    // Each "piece" is a pair of lines: an attribute line and a typed
    // parameter line. We emit them through `builder.line` so the
    // builder's indent state (currently inside `param(`) handles
    // leading whitespace; we only need to manage the trailing comma
    // between pieces.
    struct ParamPiece {
        attr: String,
        decl: String,
    }
    let mut pieces: Vec<ParamPiece> = Vec::new();
    if takes_self {
        // Pipe the wrapped object in: `$app | Invoke-AzulAppRun`.
        pieces.push(ParamPiece {
            attr: "[Parameter(Mandatory=$true, Position=0, ValueFromPipeline=$true)]".to_string(),
            decl: format!("[Azul.{}]$Instance", s.name),
        });
    }
    for (idx, a) in user_args.iter().enumerate() {
        let position = if takes_self { idx + 1 } else { idx };
        let pstype = ps_type_of(*a, takes_self);
        let mut name_pascal = snake_to_pascal_param(&a.name);
        // Avoid colliding with the implicit `$Instance` (the self param)
        // when an api.json arg is also named `instance`. PowerShell
        // rejects duplicate parameter names in a param block.
        if takes_self && name_pascal == "Instance" {
            name_pascal = "InstanceArg".to_string();
        }
        pieces.push(ParamPiece {
            attr: format!("[Parameter(Mandatory=$true, Position={})]", position),
            decl: format!("{}${}", pstype, name_pascal),
        });
    }

    let total = pieces.len();
    for (idx, piece) in pieces.iter().enumerate() {
        builder.line(&piece.attr);
        let suffix = if idx + 1 == total { "" } else { "," };
        builder.line(&format!("{}{}", piece.decl, suffix));
    }

    builder.dedent();
    builder.line(")");

    // Body.
    builder.line("process {");
    builder.indent();

    // Construct the C# call. Constructors / static methods use the
    // wrapper class's static factory (`[Azul.App]::Create(...)`);
    // instance methods use the `$Instance.<Method>($args)` form.
    let csharp_method = idiomatic_cs_method(&func.method_name);
    let call_args: Vec<String> = user_args
        .iter()
        .map(|a| {
            let mut n = snake_to_pascal_param(&a.name);
            if takes_self && n == "Instance" {
                n = "InstanceArg".to_string();
            }
            format!("${}", n)
        })
        .collect();

    if is_static {
        builder.line(&format!(
            "[Azul.{}]::{}({})",
            s.name,
            csharp_method,
            call_args.join(", ")
        ));
    } else if takes_self {
        builder.line(&format!(
            "$Instance.{}({})",
            csharp_method,
            call_args.join(", ")
        ));
    } else {
        // Static-ish helpers without a self argument; route to the C#
        // wrapper class statically.
        builder.line(&format!(
            "[Azul.{}]::{}({})",
            s.name,
            csharp_method,
            call_args.join(", ")
        ));
    }

    builder.dedent();
    builder.line("}");
    builder.dedent();
    builder.line("}");
    builder.blank();
}

// ============================================================================
// Naming helpers
// ============================================================================

/// Pick the PowerShell verb for an api.json method name. We bias toward
/// approved verbs (`Get-Verb`); unknown methods fall back to `Invoke`.
fn pick_verb(method_name: &str, kind: &FunctionKind) -> &'static str {
    // Constructors are always `New-`; the kind is the strongest signal.
    match kind {
        FunctionKind::Constructor | FunctionKind::Default => return "New",
        FunctionKind::Delete => return "Remove",
        _ => {}
    }

    let lower = method_name.to_ascii_lowercase();
    // Whole-word overrides.
    match lower.as_str() {
        "new" | "create" | "build" => return "New",
        "delete" | "drop" | "free" | "destroy" => return "Remove",
        "run" | "exec" | "execute" => return "Invoke",
        "clone" | "deepcopy" | "deep_copy" => return "Copy",
        "default" => return "New",
        _ => {}
    }
    // Prefix matches.
    if lower.starts_with("get") || lower.starts_with("read") || lower.starts_with("from") {
        return "Get";
    }
    if lower.starts_with("set") || lower.starts_with("write") || lower.starts_with("with") {
        return "Set";
    }
    if lower.starts_with("add") || lower.starts_with("push") || lower.starts_with("insert") {
        return "Add";
    }
    if lower.starts_with("clear") || lower.starts_with("reset") {
        return "Clear";
    }
    if lower.starts_with("update") || lower.starts_with("refresh") {
        return "Update";
    }
    if lower.starts_with("start") || lower.starts_with("begin") {
        return "Start";
    }
    if lower.starts_with("stop") || lower.starts_with("end") {
        return "Stop";
    }
    "Invoke"
}

/// Build the noun half of a `Verb-AzulNoun` cmdlet. For constructors and
/// trait `Default` we emit `Azul<Type>`; for everything else we emit
/// `Azul<Type><Method>` so the names stay unambiguous when one type has
/// many methods.
fn pick_noun(class_name: &str, method_name: &str, kind: &FunctionKind) -> String {
    match kind {
        FunctionKind::Constructor | FunctionKind::Default => class_name.to_string(),
        _ => format!("{}{}", class_name, snake_to_pascal_param(method_name)),
    }
}

/// PowerShell-friendly type annotation for an argument. We map blittable
/// primitives to their `[type]` literal; everything else falls back to
/// the C# wrapper or FFI struct under `[Azul.*]`.
fn ps_type_of(arg: &FunctionArg, _takes_self: bool) -> String {
    // Anything that the C# layer surfaces as `IntPtr` is opaque on the
    // PS side too — annotate as `[IntPtr]` so the binder doesn't try to
    // coerce it.
    match arg.ref_kind {
        ArgRefKind::Owned => match arg.type_name.trim() {
            "bool" => "[bool]".to_string(),
            "u8" | "i8" => "[byte]".to_string(),
            "u16" | "i16" => "[uint16]".to_string(),
            "u32" | "c_uint" => "[uint32]".to_string(),
            "i32" | "c_int" => "[int32]".to_string(),
            "u64" => "[uint64]".to_string(),
            "i64" => "[int64]".to_string(),
            "f32" => "[single]".to_string(),
            "f64" => "[double]".to_string(),
            "usize" | "isize" => "[IntPtr]".to_string(),
            other => format!("[Azul.Az{}]", other),
        },
        ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
            "[IntPtr]".to_string()
        }
    }
}

/// PowerShell parameter names are PascalCase by convention.
fn snake_to_pascal_param(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut upper = true;
    for c in s.chars() {
        if c == '_' {
            upper = true;
        } else if upper {
            out.extend(c.to_uppercase());
            upper = false;
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        return "Value".to_string();
    }
    out
}

/// Mirror `lang_csharp/wrappers.rs::idiomatic_method_name`. We can't
/// import it directly without exposing it; the rule is small enough to
/// duplicate.
fn idiomatic_cs_method(method_name: &str) -> String {
    if method_name == "new" {
        return "Create".to_string();
    }
    if method_name.contains('_') {
        return snake_to_pascal_param(method_name);
    }
    let mut chars = method_name.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Escape characters that would terminate a PowerShell `<# … #>` comment
/// or break the line. We aren't strict about it — newlines are split
/// upstream — but `#>` inside doc text is fatal.
fn ps_doc_escape(s: &str) -> String {
    s.replace("#>", "# >")
}
