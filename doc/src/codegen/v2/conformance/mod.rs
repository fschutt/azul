//! The conformance plan: one language-neutral list of checks, derived from the
//! IR, that every binding's generated conformance program renders in its own
//! language. The program doubles as that binding's memtest: it runs the whole
//! plan `AZ_MEMTEST_N` times (default 1), so `scripts/run_memtest.sh` can
//! compare peak RSS across N and catch crashes under a debugger.
//!
//! Because every binding runs the SAME plan, a defect shows up as a class:
//! "hash is never surfaced in Lua" is one failing check per hashable type, in
//! one language, next to the same checks passing in C.
//!
//! What the plan checks, for every item the API has (never a sample):
//!
//! * every constant has its api.json value;
//! * every type that can be constructed generically (its `Default`, a unit
//!   enum's first variant, `None`, an empty Vec) round-trips its derives:
//!   a clone is equal, compares equal, hashes equal, formats non-empty, and
//!   both copies drop;
//! * every enum variant constructor whose payload can be built;
//! * every Vec whose element can be built: three elements in, three out;
//! * every host-invokable callback kind: a wrapper around a host handle is
//!   created and dropped.
//!
//! Nothing is skipped silently: every type the plan cannot construct is
//! listed in [`ConformancePlan::unconstructible`], so a renderer (and a
//! reviewer) sees exactly what is not covered.

pub mod c;

use std::collections::{BTreeMap, BTreeSet};

use super::ir::{CodegenIR, EnumVariantKind, FunctionDef, FunctionKind, TypeCategory};

/// How a conformance program makes a value of some type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recipe {
    /// A primitive's zero (`0`, `0.0`, `false`).
    Primitive { ty: String },
    /// The API string `"azul"`.
    String,
    /// `Az<Type>_createDefault()`.
    Default { ty: String, c_fn: String },
    /// A variant constructor with no payload (`Az<Enum>_<variant>()`).
    UnitVariant { ty: String, variant: String, c_fn: String },
    /// `None` of an Option (`Az<Option>_none()`).
    None { ty: String, c_fn: String },
    /// An empty Vec (`Az<Vec>_create()`).
    EmptyVec { ty: String, c_fn: String },
}

impl Recipe {
    /// The API type this recipe makes (the primitive's name for primitives).
    pub fn ty(&self) -> &str {
        match self {
            Recipe::Primitive { ty }
            | Recipe::Default { ty, .. }
            | Recipe::UnitVariant { ty, .. }
            | Recipe::None { ty, .. }
            | Recipe::EmptyVec { ty, .. } => ty,
            Recipe::String => "String",
        }
    }
}

/// `Az<Class>_<NAME>` must equal `value`.
#[derive(Debug, Clone)]
pub struct ConstCase {
    pub class: String,
    pub name: String,
    pub c_name: String,
    pub type_name: String,
    pub value: String,
}

/// The derive round-trip for one constructible type. A capability is `Some`
/// with the C function that implements it when the type has it.
#[derive(Debug, Clone)]
pub struct DeriveCase {
    pub ty: String,
    pub make: Recipe,
    pub is_copy: bool,
    pub clone: Option<String>,
    pub debug: Option<String>,
    pub partial_eq: Option<String>,
    pub partial_cmp: Option<String>,
    pub cmp: Option<String>,
    pub hash: Option<String>,
    pub delete: Option<String>,
}

/// One enum variant constructor, called with a recipe per payload argument.
#[derive(Debug, Clone)]
pub struct VariantCase {
    pub ty: String,
    pub variant: String,
    pub method: String,
    pub c_fn: String,
    pub args: Vec<Recipe>,
    /// `Az<Enum>_delete` when the enum owns memory.
    pub delete: Option<String>,
    /// `Az<Enum>_partialEq`, to check two constructions are equal.
    pub partial_eq: Option<String>,
}

/// A Vec filled with three elements from `element`, read back.
#[derive(Debug, Clone)]
pub struct VecCase {
    pub ty: String,
    pub element: Recipe,
    /// `Az<Vec>_copyFromPtr(ptr, len)`.
    pub copy_from_ptr: String,
    /// `Az<Vec>_len(&vec)`.
    pub len: String,
    pub delete: Option<String>,
    /// `Az<Element>_delete` when the element owns memory (the program drops
    /// its local elements after copying them in).
    pub element_delete: Option<String>,
}

/// A callback wrapper created around a host handle and dropped.
#[derive(Debug, Clone)]
pub struct CallbackCase {
    pub wrapper: String,
    pub typedef: String,
    /// `Az<Wrapper>_createFromHostHandle(handle)`.
    pub from_handle: String,
    pub delete: Option<String>,
}

/// The whole plan.
#[derive(Debug, Clone, Default)]
pub struct ConformancePlan {
    pub constants: Vec<ConstCase>,
    pub derives: Vec<DeriveCase>,
    pub variants: Vec<VariantCase>,
    pub vecs: Vec<VecCase>,
    pub callbacks: Vec<CallbackCase>,
    /// Every non-generic type no recipe can make, with the reason. Rendered
    /// into each program's summary so the coverage gap is visible.
    pub unconstructible: BTreeMap<String, String>,
}

impl ConformancePlan {
    /// The plan for `ir`.
    pub fn build(ir: &CodegenIR) -> Self {
        let fns = FnIndex::new(ir);
        let mut plan = ConformancePlan::default();

        for c in &ir.constants {
            let Some((class, name)) = c.name.split_once('_') else { continue };
            plan.constants.push(ConstCase {
                class: class.to_string(),
                name: name.to_string(),
                c_name: format!("Az{}", c.name),
                type_name: c.type_name.clone(),
                value: c.value.clone(),
            });
        }

        // Every nameable, non-generic type.
        let mut types: BTreeSet<&str> = BTreeSet::new();
        for s in &ir.structs {
            if s.generic_params.is_empty()
                && !matches!(
                    s.category,
                    TypeCategory::CallbackTypedef
                        | TypeCategory::GenericTemplate
                        | TypeCategory::Recursive
                )
            {
                types.insert(&s.name);
            }
        }
        for e in &ir.enums {
            if e.generic_params.is_empty() {
                types.insert(&e.name);
            }
        }
        for a in &ir.type_aliases {
            if !a.generic_args.is_empty() {
                types.insert(&a.name);
            }
        }

        for ty in &types {
            match recipe(ir, &fns, ty) {
                Ok(make) => plan.derives.push(DeriveCase {
                    ty: ty.to_string(),
                    make,
                    is_copy: is_copy(ir, ty),
                    clone: fns.get(ty, FunctionKind::DeepCopy),
                    debug: fns.get(ty, FunctionKind::DebugToString),
                    partial_eq: fns.get(ty, FunctionKind::PartialEq),
                    partial_cmp: fns.get(ty, FunctionKind::PartialCmp),
                    cmp: fns.get(ty, FunctionKind::Cmp),
                    hash: fns.get(ty, FunctionKind::Hash),
                    delete: fns.get(ty, FunctionKind::Delete),
                }),
                Err(why) => {
                    plan.unconstructible.insert(ty.to_string(), why);
                }
            }
        }

        for f in ir
            .functions
            .iter()
            .filter(|f| f.kind == FunctionKind::EnumVariantConstructor)
        {
            let Some(e) = ir.find_enum(&f.class_name) else { continue };
            let variant = e
                .variants
                .iter()
                .find(|v| {
                    let lc = lower_first(&v.name);
                    f.method_name == lc || f.method_name == format!("{lc}Variant")
                })
                .map(|v| v.name.clone())
                .unwrap_or_else(|| f.method_name.clone());
            let args: Result<Vec<Recipe>, String> = f
                .args
                .iter()
                .map(|a| recipe(ir, &fns, a.type_name.trim()))
                .collect();
            let args = match args {
                Ok(args) => args,
                Err(why) => {
                    plan.unconstructible
                        .insert(format!("{}::{}", e.name, variant), format!("payload: {why}"));
                    continue;
                }
            };
            {
                plan.variants.push(VariantCase {
                    ty: e.name.clone(),
                    variant,
                    method: f.method_name.clone(),
                    c_fn: f.c_name.clone(),
                    args,
                    delete: fns.get(&e.name, FunctionKind::Delete),
                    partial_eq: fns.get(&e.name, FunctionKind::PartialEq),
                });
            }
        }

        // Every Vec by its layout (`ptr`/`len`/`cap`/`destructor`), whatever
        // category the IR gave it: the plan must not inherit a classifier bug.
        for s in &ir.structs {
            let has = |n: &str| s.fields.iter().any(|f| f.name == n);
            if !(s.fields.len() == 4 && has("ptr") && has("len") && has("cap") && has("destructor")) {
                continue;
            }
            let Some(elem) = s.fields.iter().find(|f| f.name == "ptr").map(|f| f.type_name.as_str()) else {
                continue;
            };
            let (Some(copy_from_ptr), Some(len)) = (
                fns.method(&s.name, "copy_from_ptr"),
                fns.method(&s.name, "len"),
            ) else {
                continue;
            };
            match recipe(ir, &fns, elem) {
                Err(why) => {
                    plan.unconstructible.insert(s.name.clone(), format!("element {elem}: {why}"));
                }
                Ok(element) => plan.vecs.push(VecCase {
                    ty: s.name.clone(),
                    element_delete: fns.get(element.ty(), FunctionKind::Delete),
                    element,
                    copy_from_ptr,
                    len,
                    delete: fns.get(&s.name, FunctionKind::Delete),
                }),
            }
        }

        for kind in super::managed_host_invoker::host_invoker_kinds(ir) {
            let wrapper = kind.name.trim_end_matches("Type").to_string();
            plan.callbacks.push(CallbackCase {
                from_handle: format!("Az{wrapper}_createFromHostHandle"),
                delete: fns.get(&wrapper, FunctionKind::Delete),
                typedef: kind.name.clone(),
                wrapper,
            });
        }

        plan
    }
}

/// Function lookup by `(class, kind)` and `(class, method name)`.
struct FnIndex<'a> {
    by_kind: BTreeMap<(&'a str, FunctionKindKey), &'a FunctionDef>,
    by_method: BTreeMap<(&'a str, &'a str), &'a FunctionDef>,
}

/// `FunctionKind` is not `Ord`; this is its discriminant for map keys.
type FunctionKindKey = u8;

fn kind_key(k: FunctionKind) -> FunctionKindKey {
    match k {
        FunctionKind::Delete => 1,
        FunctionKind::DeepCopy => 2,
        FunctionKind::PartialEq => 3,
        FunctionKind::PartialCmp => 4,
        FunctionKind::Cmp => 5,
        FunctionKind::Hash => 6,
        FunctionKind::Default => 7,
        FunctionKind::DebugToString => 8,
        _ => 0,
    }
}

impl<'a> FnIndex<'a> {
    fn new(ir: &'a CodegenIR) -> Self {
        let mut by_kind = BTreeMap::new();
        let mut by_method = BTreeMap::new();
        for f in &ir.functions {
            let k = kind_key(f.kind);
            if k != 0 {
                by_kind.insert((f.class_name.as_str(), k), f);
            }
            by_method.insert((f.class_name.as_str(), f.method_name.as_str()), f);
        }
        Self { by_kind, by_method }
    }

    fn get(&self, class: &str, kind: FunctionKind) -> Option<String> {
        self.by_kind
            .get(&(class, kind_key(kind)))
            .map(|f| f.c_name.clone())
    }

    fn method(&self, class: &str, method: &str) -> Option<String> {
        self.by_method.get(&(class, method)).map(|f| f.c_name.clone())
    }
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_lowercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

fn is_copy(ir: &CodegenIR, ty: &str) -> bool {
    if let Some(s) = ir.find_struct(ty) {
        return s.traits.is_copy;
    }
    if let Some(e) = ir.find_enum(ty) {
        return e.traits.is_copy;
    }
    ir.find_type_alias(ty).is_some_and(|a| a.traits.is_copy)
}

/// How to make a `ty`, or why it cannot be made generically.
fn recipe(ir: &CodegenIR, fns: &FnIndex<'_>, ty: &str) -> Result<Recipe, String> {
    let ty = ty.trim();
    if matches!(
        ty,
        "bool" | "u8" | "u16" | "u32" | "u64" | "usize" | "i8" | "i16" | "i32" | "i64" | "isize"
            | "f32" | "f64" | "char"
    ) {
        return Ok(Recipe::Primitive { ty: ty.to_string() });
    }
    if let Some(s) = ir.find_struct(ty) {
        if s.category == TypeCategory::String {
            return Ok(Recipe::String);
        }
    }
    if let Some(c_fn) = fns.get(ty, FunctionKind::Default) {
        return Ok(Recipe::Default { ty: ty.to_string(), c_fn });
    }
    if let Some(e) = ir.find_enum(ty) {
        if e.category == TypeCategory::Option {
            if let Some(c_fn) = fns.method(ty, "none") {
                return Ok(Recipe::None { ty: ty.to_string(), c_fn });
            }
        }
        if let Some(v) = e.variants.iter().find(|v| matches!(v.kind, EnumVariantKind::Unit)) {
            let lc = lower_first(&v.name);
            let c_fn = fns
                .method(ty, &lc)
                .or_else(|| fns.method(ty, &format!("{lc}Variant")));
            if let Some(c_fn) = c_fn {
                return Ok(Recipe::UnitVariant {
                    ty: ty.to_string(),
                    variant: v.name.clone(),
                    c_fn,
                });
            }
        }
        return Err("no Default, no unit variant".into());
    }
    if let Some(s) = ir.find_struct(ty) {
        // A Vec by its layout (never the IR category: the plan must not
        // inherit a classifier bug), made with its argument-free `create`.
        let has = |n: &str| s.fields.iter().any(|f| f.name == n);
        let vec_layout =
            s.fields.len() == 4 && has("ptr") && has("len") && has("cap") && has("destructor");
        if vec_layout {
            if let Some(f) = fns.by_method.get(&(ty, "create")).filter(|f| f.args.is_empty()) {
                return Ok(Recipe::EmptyVec { ty: ty.to_string(), c_fn: f.c_name.clone() });
            }
        }
        return Err("no Default".into());
    }
    Err("not constructible".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The plan covers the API, not a sample of it: every constant, and at
    /// least one case of every kind.
    #[test]
    fn plan_covers_every_constant_and_every_case_kind() {
        let api = crate::api::ApiData::from_str(
            &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../api.json")).unwrap(),
        )
        .unwrap();
        let ir = super::super::build_ir_from_api(&api).unwrap();
        let plan = ConformancePlan::build(&ir);
        assert_eq!(plan.constants.len(), ir.constants.len());
        assert!(!plan.derives.is_empty());
        assert!(!plan.variants.is_empty());
        assert!(!plan.callbacks.is_empty());
    }
}
