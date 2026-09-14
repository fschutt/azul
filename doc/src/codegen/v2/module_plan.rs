//! Per-module split plan shared by the Haskell, OCaml and Fortran
//! generators.
//!
//! Every one of those toolchains chokes on one monolithic translation
//! unit (GHC peaks at 3.5 GB RSS on `Azul/Types.hs`, `ocamlopt` overflows
//! an 8 MB stack on `azul.ml`, gfortran needs minutes on `azul.f90`). The
//! obvious cut — one output module per api.json module — does not compile
//! as is: the api.json module graph is one 34-module strongly connected
//! component (`css` and `dom` reference each other 43/22 times, `error`
//! needs `XmlTextPos` from `css`, `callbacks` needs the widget states, and
//! `vec` / `option` are containers over every other module). None of the
//! three languages allows a cycle between compilation units.
//!
//! So the plan is computed from the *type* dependency graph instead:
//!
//! 1. Container types (`XVec`, `XVecDestructor`, `OptionX`, ... — everything
//!    api.json files under `vec` / `option`) are assigned to the module of
//!    their element type. `DomVec` belongs with `Dom`; the `vec` module
//!    keeps only the primitive containers (`U8Vec`, ...).
//! 2. The type graph is condensed into strongly connected components
//!    (mutually recursive types must share a unit) and walked in
//!    dependency order (Kahn). The walk keeps emitting the current api.json
//!    module while it still has ready types and only switches when forced;
//!    every switch cuts a new *chunk*. A module whose types are split by a
//!    dependency on a later module therefore becomes `css`, `css_2`, ... .
//! 3. Chunks smaller than [`MIN_CHUNK_TYPES`] are folded into the previous
//!    chunk (named after whichever side is larger), so the plan does not
//!    fragment into hundreds of five-type units. The types that land in a
//!    chunk named after another module are the "moved" types; the plan
//!    reports them so a generator can document the move.
//!
//! The result is a list of chunks in an order every chunk's dependencies
//! precede it, plus, per type, the api.json module it belongs to after
//! step 1 (used to group the per-class surface — FFI imports, wrappers —
//! which has no ordering constraint and stays one file per api.json
//! module).
//!
//! The dependency edges include pointer references (a `Ptr Dom` field in
//! Haskell and a `ptr az_dom` field in OCaml both need the type in scope),
//! which is a superset of what the C header needs; one plan serves all
//! three languages.

use std::collections::{BTreeMap, BTreeSet};

use super::ir::{CodegenIR, EnumVariantKind, MonomorphizedKind};

/// Chunks with fewer types than this are folded into their predecessor.
pub const MIN_CHUNK_TYPES: usize = 10;

/// api.json modules whose types are containers over other modules' types.
const CONTAINER_MODULES: &[&str] = &["vec", "option"];

/// One output compilation unit.
#[derive(Debug, Clone)]
pub struct Chunk {
    /// Unique unit name: the api.json module (`css`) or, for a later
    /// slice of the same module, `css_2`, `css_3`, ... .
    pub name: String,
    /// The api.json module the chunk is named after.
    pub api_module: String,
    /// 1 for the first chunk of an api.json module, 2 for the second, ... .
    pub ordinal: usize,
    /// IR type names (structs, enums, type aliases, callback typedefs) in
    /// dependency order.
    pub types: Vec<String>,
    /// Indices of the chunks this chunk references; every index is smaller
    /// than the chunk's own.
    pub deps: BTreeSet<usize>,
}

/// The whole split.
#[derive(Debug, Clone)]
pub struct ModulePlan {
    pub chunks: Vec<Chunk>,
    type_chunk: BTreeMap<String, usize>,
    /// Type -> api.json module after container relocation.
    type_module: BTreeMap<String, String>,
    /// Type -> the api.json module api.json itself files it under.
    original_module: BTreeMap<String, String>,
}

impl ModulePlan {
    /// Compute the plan for every type in `ir`.
    pub fn build(ir: &CodegenIR) -> ModulePlan {
        let types = all_types(ir);
        let index: BTreeMap<&str, usize> = types
            .iter()
            .enumerate()
            .map(|(i, t)| (t.name.as_str(), i))
            .collect();

        // Dependency edges, restricted to known types.
        let deps: Vec<BTreeSet<usize>> = types
            .iter()
            .map(|t| {
                type_refs(ir, &t.name)
                    .iter()
                    .filter_map(|r| index.get(r.as_str()).copied())
                    .filter(|&j| types[j].name != t.name)
                    .collect()
            })
            .collect();

        // 1. Container relocation, to a fixpoint (OptionU8Vec -> U8Vec -> vec).
        let mut module: Vec<String> = types.iter().map(|t| t.module.clone()).collect();
        loop {
            let mut changed = false;
            for i in 0..types.len() {
                if !CONTAINER_MODULES.contains(&types[i].module.as_str()) {
                    continue;
                }
                let targets: BTreeSet<&str> = deps[i]
                    .iter()
                    .map(|&j| module[j].as_str())
                    // A dependency still filed under a container module says
                    // nothing about where this type belongs.
                    .filter(|m| !CONTAINER_MODULES.contains(m))
                    .collect();
                if targets.len() == 1 {
                    let m = targets.into_iter().next().unwrap().to_string();
                    if module[i] != m {
                        module[i] = m;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // 2. Condense into SCCs and walk them in dependency order.
        let comps = tarjan_scc(types.len(), &deps);
        let comp_of: Vec<usize> = {
            let mut v = vec![0; types.len()];
            for (ci, c) in comps.iter().enumerate() {
                for &t in c {
                    v[t] = ci;
                }
            }
            v
        };
        let comp_module: Vec<String> = comps
            .iter()
            .map(|c| {
                // Majority module; ties go to the alphabetically first so
                // the plan is deterministic.
                let mut count: BTreeMap<&str, usize> = BTreeMap::new();
                for &t in c {
                    *count.entry(module[t].as_str()).or_insert(0) += 1;
                }
                count
                    .iter()
                    .max_by(|a, b| a.1.cmp(b.1).then_with(|| b.0.cmp(a.0)))
                    .map(|(m, _)| m.to_string())
                    .unwrap()
            })
            .collect();
        let mut succ: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); comps.len()];
        let mut indeg: Vec<usize> = vec![0; comps.len()];
        for (t, ds) in deps.iter().enumerate() {
            for &d in ds {
                let (a, b) = (comp_of[t], comp_of[d]);
                if a != b && succ[b].insert(a) {
                    indeg[a] += 1;
                }
            }
        }

        let mut ready: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
        let mut remaining: BTreeMap<String, usize> = BTreeMap::new();
        for (ci, m) in comp_module.iter().enumerate() {
            *remaining.entry(m.clone()).or_insert(0) += 1;
            if indeg[ci] == 0 {
                ready.entry(m.clone()).or_default().insert(ci);
            }
        }
        let mut raw_chunks: Vec<(String, Vec<usize>)> = Vec::new();
        let mut current: Option<String> = None;
        let mut done = 0;
        while done < comps.len() {
            let switch = match &current {
                Some(m) => ready.get(m).map(|s| s.is_empty()).unwrap_or(true),
                None => true,
            };
            if switch {
                // Prefer a module that can be finished now, then the one
                // with the most ready components, then the name.
                let pick = ready
                    .iter()
                    .filter(|(_, s)| !s.is_empty())
                    .map(|(m, s)| (s.len() == remaining[m], s.len(), m.clone()))
                    .max_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)).then(b.2.cmp(&a.2)))
                    .map(|(_, _, m)| m)
                    .expect("the type graph is acyclic after condensation");
                raw_chunks.push((pick.clone(), Vec::new()));
                current = Some(pick);
            }
            let m = current.as_ref().unwrap();
            let ci = *ready[m].iter().next().unwrap();
            ready.get_mut(m).unwrap().remove(&ci);
            *remaining.get_mut(m).unwrap() -= 1;
            done += 1;
            let mut members = comps[ci].clone();
            members.sort();
            raw_chunks.last_mut().unwrap().1.extend(members);
            for &n in &succ[ci] {
                indeg[n] -= 1;
                if indeg[n] == 0 {
                    ready.entry(comp_module[n].clone()).or_default().insert(n);
                }
            }
        }

        // 3. Fold small chunks into their predecessor; merge equal names.
        let mut folded: Vec<(String, Vec<usize>)> = Vec::new();
        for (m, ts) in raw_chunks {
            match folded.last_mut() {
                Some((pm, pts))
                    if *pm == m || ts.len() < MIN_CHUNK_TYPES || pts.len() < MIN_CHUNK_TYPES =>
                {
                    if ts.len() > pts.len() {
                        *pm = m;
                    }
                    pts.extend(ts);
                }
                _ => folded.push((m, ts)),
            }
        }

        // Name and wire the chunks.
        let mut ordinal: BTreeMap<String, usize> = BTreeMap::new();
        let mut chunks: Vec<Chunk> = Vec::new();
        let mut type_chunk: BTreeMap<String, usize> = BTreeMap::new();
        for (m, ts) in folded {
            let n = ordinal.entry(m.clone()).or_insert(0);
            *n += 1;
            let name = if *n == 1 {
                m.clone()
            } else {
                format!("{}_{}", m, n)
            };
            let idx = chunks.len();
            let mut names = Vec::with_capacity(ts.len());
            for t in ts {
                type_chunk.insert(types[t].name.clone(), idx);
                names.push(types[t].name.clone());
            }
            chunks.push(Chunk {
                name,
                api_module: m,
                ordinal: *n,
                types: names,
                deps: BTreeSet::new(),
            });
        }
        for (t, ds) in deps.iter().enumerate() {
            let a = type_chunk[&types[t].name];
            for &d in ds {
                let b = type_chunk[&types[d].name];
                if a != b {
                    debug_assert!(b < a, "chunk order must respect the type dependencies");
                    chunks[a].deps.insert(b);
                }
            }
        }

        ModulePlan {
            chunks,
            type_chunk,
            type_module: types
                .iter()
                .enumerate()
                .map(|(i, t)| (t.name.clone(), module[i].clone()))
                .collect(),
            original_module: types
                .iter()
                .map(|t| (t.name.clone(), t.module.clone()))
                .collect(),
        }
    }

    /// The chunk that declares `type_name`.
    pub fn chunk_of(&self, type_name: &str) -> Option<&Chunk> {
        self.type_chunk.get(type_name).map(|&i| &self.chunks[i])
    }

    pub fn chunk_index(&self, type_name: &str) -> Option<usize> {
        self.type_chunk.get(type_name).copied()
    }

    /// The api.json module `type_name` belongs to after container
    /// relocation (`DomVec` -> `dom`). This is the grouping key for the
    /// per-class surface (FFI imports, wrappers, methods).
    pub fn api_module_of(&self, type_name: &str) -> Option<&str> {
        self.type_module.get(type_name).map(|s| s.as_str())
    }

    /// Every api.json module that owns at least one type after
    /// relocation, sorted.
    pub fn api_modules(&self) -> Vec<String> {
        let set: BTreeSet<&String> = self.type_module.values().collect();
        set.into_iter().cloned().collect()
    }

    /// Chunk indices, transitively, that `idx` depends on.
    pub fn transitive_deps(&self, idx: usize) -> BTreeSet<usize> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<usize> = self.chunks[idx].deps.iter().copied().collect();
        while let Some(i) = stack.pop() {
            if out.insert(i) {
                stack.extend(self.chunks[i].deps.iter().copied());
            }
        }
        out
    }

    /// Types that live in a chunk named after a different api.json module
    /// than the one they belong to: `(type, api module, chunk name)`.
    pub fn moved_types(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        for c in &self.chunks {
            for t in &c.types {
                let m = &self.type_module[t];
                if *m != c.api_module {
                    out.push((t.clone(), m.clone(), c.name.clone()));
                }
            }
        }
        out
    }

    /// Types api.json files under another module than the plan uses
    /// (the container relocation of step 1): `(type, api.json module,
    /// planned module)`.
    pub fn relocated_types(&self) -> Vec<(String, String, String)> {
        self.type_module
            .iter()
            .filter(|(t, m)| self.original_module[*t] != **m)
            .map(|(t, m)| (t.clone(), self.original_module[t].clone(), m.clone()))
            .collect()
    }

    /// A human-readable summary for the generated file headers.
    pub fn summary(&self) -> String {
        let mut s = format!(
            "{} units over {} types; the largest unit holds {} types.",
            self.chunks.len(),
            self.type_chunk.len(),
            self.chunks.iter().map(|c| c.types.len()).max().unwrap_or(0)
        );
        let moved = self.moved_types();
        if !moved.is_empty() {
            s.push_str(&format!(
                " {} types live in a unit named after a neighbouring module because their own slice was too small to stand alone.",
                moved.len()
            ));
        }
        s
    }
}

struct TypeEntry {
    name: String,
    module: String,
}

/// Every named type the IR declares, in IR order.
fn all_types(ir: &CodegenIR) -> Vec<TypeEntry> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |name: &str, module: &str| {
        if seen.insert(name.to_string()) {
            out.push(TypeEntry {
                name: name.to_string(),
                module: module.to_string(),
            });
        }
    };
    for s in &ir.structs {
        push(&s.name, &s.module);
    }
    for e in &ir.enums {
        push(&e.name, &e.module);
    }
    for ta in &ir.type_aliases {
        push(&ta.name, &ta.module);
    }
    for cb in &ir.callback_typedefs {
        push(&cb.name, &cb.module);
    }
    out
}

/// The bare type name inside a field / argument spelling: pointer and
/// reference prefixes and array brackets are stripped, generic arguments
/// are dropped.
pub fn base_type_name(spelling: &str) -> String {
    let mut t = spelling.trim();
    loop {
        let before = t;
        for p in ["*const ", "*mut ", "&mut ", "&", "*const", "*mut"] {
            if let Some(rest) = t.strip_prefix(p) {
                t = rest.trim();
            }
        }
        if t.starts_with('[') {
            let inner = t.trim_start_matches('[').trim_end_matches(']');
            t = inner.split(';').next().unwrap_or("").trim();
        }
        if t == before {
            break;
        }
    }
    match t.find('<') {
        Some(i) => t[..i].trim().to_string(),
        None => t.to_string(),
    }
}

/// Every type name `name` references directly (fields, variant payloads,
/// alias target and monomorphized shape, callback arguments and return).
fn type_refs(ir: &CodegenIR, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(s) = ir.find_struct(name) {
        for f in &s.fields {
            out.push(base_type_name(&f.type_name));
        }
    }
    if let Some(e) = ir.find_enum(name) {
        for v in &e.variants {
            match &v.kind {
                EnumVariantKind::Unit => {}
                EnumVariantKind::Tuple(types) => {
                    for (t, _) in types {
                        out.push(base_type_name(t));
                    }
                }
                EnumVariantKind::Struct(fields) => {
                    for f in fields {
                        out.push(base_type_name(&f.type_name));
                    }
                }
            }
        }
    }
    if let Some(ta) = ir.find_type_alias(name) {
        out.push(base_type_name(&ta.target));
        for g in &ta.generic_args {
            out.push(base_type_name(g));
        }
        if let Some(m) = &ta.monomorphized_def {
            match &m.kind {
                MonomorphizedKind::TaggedUnion { variants, .. } => {
                    for v in variants {
                        if let Some(p) = &v.payload_type {
                            out.push(base_type_name(p));
                        }
                    }
                }
                MonomorphizedKind::Struct { fields } => {
                    for f in fields {
                        out.push(base_type_name(&f.type_name));
                    }
                }
                MonomorphizedKind::SimpleEnum { .. } => {}
            }
        }
    }
    if let Some(cb) = ir.callback_typedefs.iter().find(|c| c.name == name) {
        for a in &cb.args {
            out.push(base_type_name(&a.type_name));
        }
        if let Some(r) = &cb.return_type {
            out.push(base_type_name(r));
        }
    }
    out
}

/// Iterative Tarjan: components in reverse topological order (a
/// component precedes the components that depend on it — which does not
/// matter to the caller, who runs Kahn on the condensation anyway).
fn tarjan_scc(n: usize, deps: &[BTreeSet<usize>]) -> Vec<Vec<usize>> {
    const UNSET: usize = usize::MAX;
    let mut index = vec![UNSET; n];
    let mut low = vec![0usize; n];
    let mut on_stack = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut comps: Vec<Vec<usize>> = Vec::new();
    let mut next = 0usize;
    let adj: Vec<Vec<usize>> = deps.iter().map(|d| d.iter().copied().collect()).collect();

    for root in 0..n {
        if index[root] != UNSET {
            continue;
        }
        // (node, next neighbour position)
        let mut work: Vec<(usize, usize)> = vec![(root, 0)];
        index[root] = next;
        low[root] = next;
        next += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&mut (v, ref mut pos)) = work.last_mut() {
            if *pos < adj[v].len() {
                let w = adj[v][*pos];
                *pos += 1;
                if index[w] == UNSET {
                    index[w] = next;
                    low[w] = next;
                    next += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    work.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                work.pop();
                if let Some(&(p, _)) = work.last() {
                    low[p] = low[p].min(low[v]);
                }
                if low[v] == index[v] {
                    let mut comp = Vec::new();
                    loop {
                        let w = stack.pop().unwrap();
                        on_stack[w] = false;
                        comp.push(w);
                        if w == v {
                            break;
                        }
                    }
                    comps.push(comp);
                }
            }
        }
    }
    comps
}

/// A small api.json-shaped world shared by the split tests of the
/// Haskell, OCaml and Fortran generators: two `css` types, a `dom` class
/// (constructor, builder method, `_delete`) that embeds one of them and a
/// container over itself, a `dom` unit enum, and a widget in `widgets`
/// whose method returns the `dom` class — enough types for two chunks
/// and a cross-module method.
#[cfg(test)]
pub(crate) fn test_fixture_ir() -> CodegenIR {
    use crate::codegen::v2::ir::{
        ArgRefKind, EnumDef, EnumVariantDef, FieldDef, FieldRefKind, FunctionArg, FunctionDef,
        FunctionKind, StructDef, TypeCategory,
    };
    fn field(name: &str, ty: &str, rk: FieldRefKind) -> FieldDef {
        FieldDef {
            name: name.into(),
            type_name: ty.into(),
            doc: None,
            is_public: true,
            ref_kind: rk,
        }
    }
    fn st(name: &str, module: &str, fields: Vec<FieldDef>) -> StructDef {
        StructDef {
            name: name.into(),
            doc: vec![],
            fields,
            external_path: None,
            module: module.into(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".into()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category: TypeCategory::Regular,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        }
    }
    fn arg(name: &str, ty: &str, rk: ArgRefKind) -> FunctionArg {
        FunctionArg {
            name: name.into(),
            type_name: ty.into(),
            ref_kind: rk,
            doc: None,
            callback_info: None,
        }
    }
    fn func(class: &str, method: &str, kind: FunctionKind, args: Vec<FunctionArg>, ret: Option<&str>) -> FunctionDef {
        FunctionDef {
            c_name: format!("Az{}_{}", class, method),
            class_name: class.into(),
            method_name: method.into(),
            kind,
            args,
            return_type: ret.map(|s| s.to_string()),
            fn_body: None,
            doc: vec![],
            is_const: false,
            is_unsafe: false,
        }
    }
    let mut ir = CodegenIR::new();
    ir.api_version = "0.0.0-test".into();
    for i in 0..MIN_CHUNK_TYPES {
        ir.structs.push(st(&format!("Color{}", i), "css", vec![field("r", "u8", FieldRefKind::Owned)]));
    }
    ir.structs.push(st(
        "Dom",
        "dom",
        vec![
            field("color", "Color0", FieldRefKind::Owned),
            field("children", "DomVec", FieldRefKind::Owned),
        ],
    ));
    ir.structs.push(st(
        "DomVec",
        "vec",
        vec![
            field("ptr", "*const Dom", FieldRefKind::Ptr),
            field("len", "usize", FieldRefKind::Owned),
            field("cap", "usize", FieldRefKind::Owned),
        ],
    ));
    for i in 0..MIN_CHUNK_TYPES {
        ir.structs.push(st(&format!("Node{}", i), "dom", vec![field("id", "u32", FieldRefKind::Owned)]));
    }
    ir.enums.push(EnumDef {
        name: "Update".into(),
        doc: vec![],
        variants: vec![
            EnumVariantDef { name: "DoNothing".into(), doc: None, kind: EnumVariantKind::Unit },
            EnumVariantDef { name: "RefreshDom".into(), doc: None, kind: EnumVariantKind::Unit },
        ],
        external_path: None,
        module: "dom".into(),
        derives: vec![],
        has_explicit_derive: false,
        is_union: false,
        repr: Some("C".into()),
        is_send_safe: true,
        traits: Default::default(),
        generic_params: vec![],
        category: TypeCategory::Regular,
        dependencies: vec![],
        sort_order: 0,
        needs_forward_decl: false,
    });
    ir.structs.push(st("Button", "widgets", vec![field("dom", "Dom", FieldRefKind::Owned)]));
    for i in 0..MIN_CHUNK_TYPES {
        ir.structs.push(st(&format!("Widget{}", i), "widgets", vec![field("x", "f32", FieldRefKind::Owned)]));
    }
    ir.functions.push(func("Dom", "create_body", FunctionKind::Constructor, vec![], Some("Dom")));
    ir.functions.push(func(
        "Dom",
        "with_child",
        FunctionKind::Method,
        vec![arg("dom", "Dom", ArgRefKind::Owned), arg("child", "Dom", ArgRefKind::Owned)],
        Some("Dom"),
    ));
    ir.functions.push(func("Dom", "delete", FunctionKind::Delete, vec![arg("dom", "Dom", ArgRefKind::RefMut)], None));
    ir.functions.push(func("Button", "create", FunctionKind::Constructor, vec![], Some("Button")));
    ir.functions.push(func("Button", "dom", FunctionKind::Method, vec![arg("button", "Button", ArgRefKind::Owned)], Some("Dom")));
    ir.functions.push(func("Button", "delete", FunctionKind::Delete, vec![arg("button", "Button", ArgRefKind::RefMut)], None));
    for s in &ir.structs {
        ir.type_to_module.insert(s.name.clone(), s.module.clone());
    }
    ir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::v2::ir::{EnumDef, EnumVariantDef, FieldDef, FieldRefKind, StructDef, TypeCategory};

    fn field(name: &str, ty: &str, rk: FieldRefKind) -> FieldDef {
        FieldDef {
            name: name.into(),
            type_name: ty.into(),
            doc: None,
            is_public: true,
            ref_kind: rk,
        }
    }

    fn st(name: &str, module: &str, fields: Vec<FieldDef>) -> StructDef {
        StructDef {
            name: name.into(),
            doc: vec![],
            fields,
            external_path: None,
            module: module.into(),
            derives: vec![],
            has_explicit_derive: false,
            custom_impls: vec![],
            is_boxed: false,
            repr: Some("C".into()),
            is_send_safe: true,
            generic_params: vec![],
            traits: Default::default(),
            category: TypeCategory::Regular,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
            callback_wrapper_info: None,
        }
    }

    fn en(name: &str, module: &str, payload: Option<&str>) -> EnumDef {
        let mut variants = vec![EnumVariantDef {
            name: "None".into(),
            doc: None,
            kind: EnumVariantKind::Unit,
        }];
        if let Some(p) = payload {
            variants.push(EnumVariantDef {
                name: "Some".into(),
                doc: None,
                kind: EnumVariantKind::Tuple(vec![(p.into(), FieldRefKind::Owned)]),
            });
        }
        EnumDef {
            name: name.into(),
            doc: vec![],
            variants,
            external_path: None,
            module: module.into(),
            derives: vec![],
            has_explicit_derive: false,
            is_union: payload.is_some(),
            repr: Some("C, u8".into()),
            is_send_safe: true,
            traits: Default::default(),
            generic_params: vec![],
            category: TypeCategory::Regular,
            dependencies: vec![],
            sort_order: 0,
            needs_forward_decl: false,
        }
    }

    /// A small api.json-shaped world: a `vec` container over a `dom`
    /// type, a `css` type that `dom` embeds, and a `css` type that
    /// references `dom` back — the shape that makes the module graph
    /// cyclic while the type graph is not.
    fn fixture() -> CodegenIR {
        let mut ir = CodegenIR::new();
        ir.structs.push(st("ColorU", "css", vec![field("r", "u8", FieldRefKind::Owned)]));
        ir.structs.push(st(
            "Dom",
            "dom",
            vec![
                field("color", "ColorU", FieldRefKind::Owned),
                field("children", "DomVec", FieldRefKind::Owned),
            ],
        ));
        ir.structs.push(st(
            "DomVec",
            "vec",
            vec![
                field("ptr", "*const Dom", FieldRefKind::Ptr),
                field("len", "usize", FieldRefKind::Owned),
                field("destructor", "DomVecDestructor", FieldRefKind::Owned),
            ],
        ));
        ir.enums.push(en("DomVecDestructor", "vec", Some("*const c_void")));
        ir.enums.push(en("OptionDom", "option", Some("Dom")));
        // css references dom: lands after dom, in a second css slice.
        ir.structs.push(st("StyledDom", "css", vec![field("root", "Dom", FieldRefKind::Owned)]));
        ir.structs.push(st("U8Vec", "vec", vec![field("ptr", "*const u8", FieldRefKind::Ptr)]));
        ir
    }

    #[test]
    fn containers_follow_their_element() {
        let plan = ModulePlan::build(&fixture());
        assert_eq!(plan.api_module_of("DomVec"), Some("dom"));
        assert_eq!(plan.api_module_of("DomVecDestructor"), Some("dom"));
        assert_eq!(plan.api_module_of("OptionDom"), Some("dom"));
        assert_eq!(plan.api_module_of("U8Vec"), Some("vec"), "primitive containers stay in vec");
        let relocated: Vec<String> = plan.relocated_types().into_iter().map(|(t, _, _)| t).collect();
        assert_eq!(relocated, vec!["DomVec", "DomVecDestructor", "OptionDom"]);
    }

    #[test]
    fn chunks_are_in_dependency_order_and_cover_every_type() {
        let ir = fixture();
        let plan = ModulePlan::build(&ir);
        let mut seen = BTreeSet::new();
        for (i, c) in plan.chunks.iter().enumerate() {
            for d in &c.deps {
                assert!(*d < i, "chunk {} depends on later chunk {}", c.name, d);
            }
            for t in &c.types {
                assert!(seen.insert(t.clone()), "{} planned twice", t);
            }
        }
        for s in &ir.structs {
            assert!(seen.contains(&s.name), "{} missing from the plan", s.name);
        }
        for e in &ir.enums {
            assert!(seen.contains(&e.name), "{} missing from the plan", e.name);
        }
        // Dom embeds ColorU, so css comes first; StyledDom embeds Dom, so
        // it cannot sit in that first css chunk.
        let css = plan.chunk_index("ColorU").unwrap();
        let dom = plan.chunk_index("Dom").unwrap();
        let styled = plan.chunk_index("StyledDom").unwrap();
        assert!(css < dom && dom <= styled);
        assert_ne!(plan.chunk_of("ColorU").unwrap().name, plan.chunk_of("StyledDom").unwrap().name);
    }

    #[test]
    fn mutually_recursive_types_share_a_chunk() {
        let plan = ModulePlan::build(&fixture());
        assert_eq!(plan.chunk_index("Dom"), plan.chunk_index("DomVec"));
    }

    #[test]
    fn small_chunks_fold_into_their_predecessor() {
        // Seven types, all below MIN_CHUNK_TYPES per module: everything
        // folds into one unit, named after the biggest slice.
        let plan = ModulePlan::build(&fixture());
        assert_eq!(plan.chunks.len(), 1, "{:?}", plan.chunks);
        assert_eq!(plan.chunks[0].api_module, "dom");
        assert_eq!(plan.chunks[0].ordinal, 1);
        assert_eq!(plan.chunks[0].name, "dom");
        assert!(plan.moved_types().iter().any(|(t, m, c)| t == "ColorU" && m == "css" && c == "dom"));
    }

    #[test]
    fn later_slices_get_an_ordinal_suffix() {
        // Two modules that alternate: a (10 types) <- b (10 types) <- a (10 types).
        let mut ir = CodegenIR::new();
        for i in 0..10 {
            ir.structs.push(st(&format!("A{}", i), "a", vec![]));
        }
        for i in 0..10 {
            ir.structs.push(st(&format!("B{}", i), "b", vec![field("a", "A0", FieldRefKind::Owned)]));
        }
        for i in 0..10 {
            ir.structs.push(st(&format!("C{}", i), "a", vec![field("b", "B0", FieldRefKind::Owned)]));
        }
        let plan = ModulePlan::build(&ir);
        let names: Vec<&str> = plan.chunks.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "a_2"]);
        assert_eq!(plan.chunks[2].ordinal, 2);
        assert_eq!(plan.chunks[1].deps, BTreeSet::from([0]));
        assert_eq!(plan.chunks[2].deps, BTreeSet::from([1]));
        assert_eq!(plan.transitive_deps(2), BTreeSet::from([0, 1]));
        assert!(plan.moved_types().is_empty());
    }

    #[test]
    fn base_type_name_strips_decorations() {
        assert_eq!(base_type_name("*const Dom"), "Dom");
        assert_eq!(base_type_name("&mut RefAny"), "RefAny");
        assert_eq!(base_type_name("[u8; 4]"), "u8");
        assert_eq!(base_type_name("Option<Dom>"), "Option");
        assert_eq!(base_type_name(" ColorU "), "ColorU");
    }
}
