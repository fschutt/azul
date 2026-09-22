//! The value layer: one idiomatic Pascal routine for every libazul export.
//!
//! `wrappers.rs` gives a `T<Name>` class to the ~700 types that own native
//! memory (`<Name>_delete` exists). That leaves most of the API with no
//! idiomatic spelling at all: every tagged union (`OptionString`,
//! `ResultSvgSvgParseError`, the 119 monomorphized `CssPropertyValue<T>`
//! instantiations), every POD value type (`ColorU`, `LayoutRect`), and on
//! EVERY type the derives libazul exports — `_partialEq`, `_cmp`, `_hash`,
//! `_toDbgString`, `_clone`, `_delete` — plus the enum-variant
//! constructors. Those are not "internal plumbing": `AzOptionString_some`
//! is how a caller builds an `Option`, and `AzColorU_partialEq` is how two
//! colours compare. Declaring them and never naming them again left 9271
//! of the 13516 exports reachable only by spelling the raw C symbol.
//!
//! This module closes that by emitting, for every export the binding
//! declares, a plain Pascal routine that forwards to it:
//!
//! ```pascal
//! function AzColorUPartialEq(const a: TAzColorU; const b: TAzColorU): ByteBool;
//! begin Result := AzColorU_partialEq(@a, @b); end;
//! ```
//!
//! # The three rules
//!
//! 1. **Name**: `Az` + the class + the PascalCased method, i.e. the C symbol without its
//!    underscore. It is therefore never the same Pascal identifier as the import (which keeps the
//!    underscore) and never the same as a record (`TAz…`), a pointer alias (`PAz…`), a wrapper
//!    class (`T…`) or a helper (`azul_…`) — value-layer names contain no underscore at all. Two
//!    exports that still land on one spelling get an ordinal
//!    ([`super::unique_identifier`]); nothing is ever dropped.
//! 2. **Signature**: exactly the import's, except that a pointer to ONE value of a type this unit
//!    lays out as a record becomes a `const` / `var` record parameter and the routine takes the
//!    address itself ([`single_object_pointer`]). That is the whole ergonomic difference, and it is
//!    the one every caller wants: `AzColorUPartialEq(c1, c2)` instead of `AzColorU_partialEq(@c1,
//!    @c2)`. A hand-written `*const T` stays a pointer — it is as likely the base of a buffer.
//! 3. **Body**: one call, no logic. Ownership is the C API's, unchanged: a routine that returns a
//!    value returns it for the caller to own (and to free through `Az<T>Delete` or a wrapper
//!    class), a by-value argument is still consumed by the callee, and nothing here frees
//!    anything. In particular `Az<T>Delete` is the SAME `_delete` the C API exports: call it on a
//!    value that a `T<T>` wrapper also owns and you get a double free, exactly as you would
//!    calling the import.
//!
//! The bodies are one-liners (`begin … end;` on a single line) because there are ~14 000 of them
//! and the point of each is the call; anything else about a routine — its documentation, its
//! parameter names — lives on the `external` declaration it forwards to, higher up in the same
//! unit.
//!
//! # What is deliberately NOT surfaced
//!
//! A UNIT enum (`TAzStyleCursor = (…)`) is a real Pascal enum: `=`, `<`, `Ord` and `case` already
//! compare, order and switch on it, and its variants ARE the enum constants (plus the `az<Variant>`
//! short spellings). Routing that through libazul would be a slower spelling of what the language
//! does, so a unit enum contributes only its api.json METHODS here, never its derives or its
//! variant constructors.

use std::collections::{BTreeMap, BTreeSet};

use super::{
    super::{
        config::CodegenConfig,
        generator::CodeBuilder,
        ir::{ArgRefKind, CodegenIR, FunctionArg, FunctionDef, FunctionKind, TypeCategory},
    },
    functions::{external_idents, should_emit_function},
    map_type_to_pascal, record_type_name, sanitize_identifier, to_pascal_case, unique_identifier,
    types::{ptr_type_for_arg, should_include_enum, should_include_struct},
};

// ============================================================================
// Public entry points
// ============================================================================

/// The interface-section declarations.
pub fn generate_value_interface(builder: &mut CodeBuilder, ir: &CodegenIR, config: &CodegenConfig) {
    let blocks = value_layer(ir, config);
    if blocks.is_empty() {
        return;
    }

    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Value layer: every libazul export under an idiomatic Pascal name.    }");
    builder.line("{ Same symbol without the underscore (AzColorU_partialEq ->            }");
    builder.line("{ AzColorUPartialEq), with pointers to a single value taken as const / }");
    builder.line("{ var records. Ownership is unchanged: these forward, they do not own. }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    for (class, fns) in &blocks {
        builder.line(&format!("{{ {} }}", class_note(class, ir)));
        for f in fns {
            builder.line(&f.signature());
        }
        builder.blank();
    }
}

/// The implementation-section bodies.
pub fn generate_value_implementation(
    builder: &mut CodeBuilder,
    ir: &CodegenIR,
    config: &CodegenConfig,
) {
    let blocks = value_layer(ir, config);
    if blocks.is_empty() {
        return;
    }

    builder.line("{ -------------------------------------------------------------------- }");
    builder.line("{ Value layer (bodies): one forwarding call each.                      }");
    builder.line("{ -------------------------------------------------------------------- }");
    builder.blank();

    for (_, fns) in &blocks {
        for f in fns {
            builder.line(&f.signature());
            builder.line(&f.body());
        }
    }
    builder.blank();
}

/// The block header comment for one class.
fn class_note(class: &str, ir: &CodegenIR) -> String {
    // A borrowed slice points at memory the CALLER owns: its `_delete` is a
    // no-op `drop_in_place` and its `_clone` copies the borrow, not the
    // buffer. Say so where a reader meets them, because the spelling looks
    // exactly like the owning types' one line above.
    let borrowed = ir
        .find_struct(class)
        .is_some_and(|s| s.category == TypeCategory::VecRef);
    if borrowed {
        format!(
            "{} (a borrowed slice: the buffer stays the caller's, Delete is a no-op and Clone \
             copies the borrow)",
            class
        )
    } else {
        class.to_string()
    }
}

// ============================================================================
// Planning
// ============================================================================

/// One value-layer routine.
struct ValueFn {
    /// Idiomatic Pascal name (`AzColorUPartialEq`).
    name: String,
    /// The `external` declaration it calls (the import's Pascal identifier,
    /// which is the C symbol except for a case-only duplicate).
    target: String,
    params: Vec<ValueParam>,
    /// Pascal return type; `None` makes it a `procedure`.
    ret: Option<String>,
}

struct ValueParam {
    /// `const a: TAzColorU`
    decl: String,
    /// What the call passes: `a` or `@a`.
    expr: String,
}

impl ValueFn {
    fn signature(&self) -> String {
        let params = if self.params.is_empty() {
            String::new()
        } else {
            let list: Vec<&str> = self.params.iter().map(|p| p.decl.as_str()).collect();
            format!("({})", list.join("; "))
        };
        match &self.ret {
            Some(ret) => format!("function {}{}: {};", self.name, params, ret),
            None => format!("procedure {}{};", self.name, params),
        }
    }

    fn body(&self) -> String {
        let args: Vec<&str> = self.params.iter().map(|p| p.expr.as_str()).collect();
        let call = if args.is_empty() {
            self.target.clone()
        } else {
            format!("{}({})", self.target, args.join(", "))
        };
        match self.ret {
            Some(_) => format!("begin Result := {}; end;", call),
            None => format!("begin {}; end;", call),
        }
    }
}

/// Every routine the value layer emits, grouped by class in the order the
/// classes first appear in `ir.functions` (the same order the `external`
/// block uses, so the two read side by side).
///
/// Both passes call this and consume the result in the same order, so a
/// body always matches its declaration — the same contract
/// `wrappers::constructor_pascal_names` works under.
fn value_layer(ir: &CodegenIR, config: &CodegenConfig) -> Vec<(String, Vec<ValueFn>)> {
    let idents = external_idents(ir, config);
    let mut taken = reserved_identifiers(ir);
    let mut order: Vec<String> = Vec::new();
    let mut blocks: BTreeMap<String, Vec<ValueFn>> = BTreeMap::new();

    for func in &ir.functions {
        if !should_emit_function(func, ir, config) || !surfaces(func, ir) {
            continue;
        }
        let Some(target) = idents.get(&func.c_name) else {
            continue;
        };
        let base = format!(
            "Az{}{}",
            to_pascal_case(&func.class_name),
            to_pascal_case(&func.method_name)
        );
        let name = unique_identifier(&base, "", &mut taken);
        let entry = blocks.entry(func.class_name.clone()).or_insert_with(|| {
            order.push(func.class_name.clone());
            Vec::new()
        });
        entry.push(ValueFn {
            name,
            target: target.clone(),
            params: value_params(func, ir, config),
            ret: func.return_type.as_deref().map(|r| map_type_to_pascal(r, ir)),
        });
    }

    order
        .into_iter()
        .filter_map(|c| blocks.remove(&c).map(|fns| (c, fns)))
        .collect()
}

/// Does the value layer surface `func`? See the module comment for why a
/// unit enum contributes only its api.json methods.
fn surfaces(func: &FunctionDef, ir: &CodegenIR) -> bool {
    let unit_enum = ir
        .find_enum(&func.class_name)
        .is_some_and(|e| !e.is_union && !e.variants.is_empty());
    !unit_enum || func.kind.is_api_function()
}

/// The parameter list, in the order the import declares it (the receiver
/// included: a free routine has no `Self` to hide it behind).
fn value_params(func: &FunctionDef, ir: &CodegenIR, config: &CodegenConfig) -> Vec<ValueParam> {
    func.args
        .iter()
        .map(|a| {
            let name = sanitize_identifier(&a.name);
            if let Some(kw) = single_object_pointer(func, a, ir, config) {
                return ValueParam {
                    decl: format!("{}{}: {}", kw, name, record_type_name(a.type_name.trim())),
                    expr: format!("@{}", name),
                };
            }
            let pas_ty = match a.ref_kind {
                ArgRefKind::Owned => map_type_to_pascal(&a.type_name, ir),
                ArgRefKind::Ref | ArgRefKind::RefMut | ArgRefKind::Ptr | ArgRefKind::PtrMut => {
                    ptr_type_for_arg(&a.type_name, ir)
                }
            };
            ValueParam {
                decl: format!("{}: {}", name, pas_ty),
                expr: name,
            }
        })
        .collect()
}

/// `Some("const ")` / `Some("var ")` when `arg` points at exactly ONE value
/// of a type this unit lays out as a record, so the routine can take the
/// record and hand libazul its address. Two shapes qualify, nothing else:
///
/// * `&T` / `&mut T` — api.json's `"ref"` / `"refmut"`, which includes every `self` receiver. A
///   Rust reference is one object by definition.
/// * the receiver of a SYNTHESISED trait entry point (`_delete`, `_clone`, `_partialEq`, `_hash`,
///   `_cmp`, `_partialCmp`, `_toDbgString`). The IR builder spells those `*const T` / `*mut T`, but
///   they are likewise always one value of the owning class.
///
/// A hand-written `*const T` stays a raw pointer: it is just as likely the
/// base of a caller-owned buffer (`copyFromPtr(ptr, len)`), where handing
/// over the address of a single record would be wrong. And a type this unit
/// forward-declares as opaque (`TAzXmlNodeChild = Pointer`) stays a pointer
/// too — `@` on a `Pointer` variable is the address OF THE VARIABLE, one
/// indirection too many.
fn single_object_pointer(
    func: &FunctionDef,
    arg: &FunctionArg,
    ir: &CodegenIR,
    config: &CodegenConfig,
) -> Option<&'static str> {
    let ty = arg.type_name.trim();
    let one_value = match arg.ref_kind {
        ArgRefKind::Owned => return None,
        ArgRefKind::Ref | ArgRefKind::RefMut => true,
        ArgRefKind::Ptr | ArgRefKind::PtrMut => is_synthesised_trait_fn(func) && ty == func.class_name,
    };
    if !one_value || !emits_as_record(ty, ir, config) {
        return None;
    }
    Some(match arg.ref_kind {
        ArgRefKind::RefMut | ArgRefKind::PtrMut => "var ",
        _ => "const ",
    })
}

/// A trait entry point the IR BUILDER synthesised from an api.json `derive`
/// (as opposed to a function api.json spells out, or an enum-variant
/// constructor). Its receiver is always one value of its class.
fn is_synthesised_trait_fn(func: &FunctionDef) -> bool {
    !func.kind.is_api_function() && func.kind != FunctionKind::EnumVariantConstructor
}

/// Does `types.rs` give `ty` a real `record` (or variant record), as
/// opposed to the opaque `TAzFoo = Pointer;` forward alias or a plain
/// alias of a primitive?
///
/// `is_value_aggregate` answers "does this cross the ABI as a struct or
/// union", which is necessary but not sufficient: a type the inclusion
/// filters skip has a name but no layout.
fn emits_as_record(ty: &str, ir: &CodegenIR, config: &CodegenConfig) -> bool {
    if !ir.is_value_aggregate(ty) {
        return false;
    }
    if let Some(s) = ir.find_struct(ty) {
        return should_include_struct(s, config);
    }
    if let Some(e) = ir.find_enum(ty) {
        return should_include_enum(e, config);
    }
    if let Some(alias) = ir.find_type_alias(ty) {
        return alias.monomorphized_def.is_some() && config.should_include_type(ty);
    }
    false
}

/// Identifiers the rest of the unit already owns, lowercased (Pascal is
/// case-insensitive).
///
/// A value-layer name is `Az<Class><Method>` and so contains NO underscore,
/// which by construction rules out the whole `Az<Class>_<method>` import
/// surface, the `azul_*` helpers, the `AzulGlContextPtr_*` constants and the
/// `TAz<Enum>_<Variant>` enum constants; and it starts with `Az`, which
/// rules out every `TAz…` record, `PAz…` pointer alias and `T…` wrapper
/// class. What is left to avoid is the `az<Variant>` short enum spellings
/// (`azRefreshDom`) and the handful of names the managed runtime declares
/// for itself.
fn reserved_identifiers(ir: &CodegenIR) -> BTreeSet<String> {
    // The managed runtime's own globals (managed.rs). They are this
    // binding's identifiers, not API items.
    let mut taken: BTreeSet<String> = [
        "azullib",
        "azulabicheck",
        "azulhandles",
        "azulhandleslock",
        "azulnexthandleid",
        "azuldefaultdata",
        "azulhasdefaultdata",
        "azulcurrentdata",
        "azulhascurrentdata",
        "azulhostinvokerinit",
        "eazulerror",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();

    // `managed::emit_enum_aliases` binds `az<Variant>` for the unit enums.
    // It skips the ones that clash with something it knows about, but it
    // does not know about us, so reserve every candidate: over-reserving
    // only costs an ordinal on our side.
    for e in &ir.enums {
        if e.is_union {
            continue;
        }
        for v in &e.variants {
            taken.insert(format!("az{}", v.name).to_ascii_lowercase());
        }
    }
    taken
}
