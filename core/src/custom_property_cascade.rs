//! The variable half of the cascade: which custom properties (`--name`) each
//! node sees, and what every `var()` / `env()` reference resolves to.
//!
//! Design `scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md`
//! §7.3: custom properties are cascade-level, not parse-level. A definition
//! is an INHERITABLE, CONDITIONAL declaration - in any stylesheet rule or in
//! a node's own style - and a reference is resolved per node under the
//! window's live context, through ONE resolver
//! ([`azul_css::custom_properties`]) whose answers both cascades read:
//!
//! - stylesheet references are resolved into `CssPropertyCache::css_props` (a placeholder is
//!   pushed in cascade order while the rules are matched, then overwritten here), which the slow
//!   path, the inheritance walk and the compact builder all read already;
//! - a node's own references are resolved into [`ResolvedInline`], read through
//!   `CssPropertyCache::inline_properties` by every reader of inline style (the slow path, the
//!   compact builder, the inheritance passes, the hit-test tagger).
//!
//! It runs inside `CssPropertyCache::restyle`, so everything that re-runs the
//! cascade re-resolves the variables: a light/dark flip
//! (`StyledDom::set_dynamic_selector_context`, the restyle path, no DOM
//! rebuild) moves exactly the values that read a variable defined under
//! `@theme(dark)` / `@theme(light)`, and bumps `cascade_epoch` like every
//! other cascade input (design §9.1 pitfall 11).
//!
//! Pseudo-states: a definition under `:hover` applies while its node is
//! hovered. A resting declaration that reads a variable redefined for a state
//! is re-resolved for that state (a "state variant"), so `.btn:hover { --bg: ..
//! } .btn { background: var(--bg) }` repaints on hover like on the web.
//! Known limit, the same one inherited state properties have in this engine:
//! a descendant sees its ancestor's `:hover` variables in ITS OWN hover
//! state, so `.card:hover { --fg } .card .label { color: var(--fg) }`
//! recolours the label while the pointer is over the label, not over the
//! card's padding.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec::Vec,
};

use azul_css::{
    css::{CssCustomProperty, CssDeclaration, DynamicCssProperty},
    custom_properties::{CustomPropertyMap, VarResolver},
    dynamic_selector::{
        DynamicSelector, DynamicSelectorContext, DynamicSelectorVec, PseudoStateType,
    },
    props::property::{CssProperty, CssPropertyType},
    AzString,
};

use crate::{
    dom::NodeData,
    prop_cache::{CssPropertyCache, StatefulCssProperty},
    styled_dom::{NodeHierarchyItemVec, ParentWithNodeDepth, ParentWithNodeDepthVec},
};

/// What a node without any variable sees.
static EMPTY_MAP: CustomPropertyMap = CustomPropertyMap::new();

/// One custom-property definition that applies to a node: the pseudo-state
/// it applies in, its name (without `--`), its value as written.
pub(crate) type StagedDefinition = (PseudoStateType, AzString, AzString);

/// Every node's custom properties, as one cascade computed them.
///
/// Maps are shared structurally: a node that defines nothing (or re-defines
/// what it already inherits) points at its parent's map, so a DOM with a
/// `:root` palette stores ONE map however many nodes it has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CustomPropertyEnvs {
    /// Distinct maps; index 0 is the empty map once anything was built.
    maps: Vec<CustomPropertyMap>,
    /// Per node, the map of its resting (`Normal`) state. Empty = this DOM
    /// has no variables at all (every node sees the empty map).
    normal: Vec<usize>,
    /// Per node, `(state, map)` for every pseudo-state whose map differs from
    /// the resting one. Empty = no pseudo-state definition anywhere.
    states: Vec<Vec<(PseudoStateType, usize)>>,
}

impl CustomPropertyEnvs {
    /// Every node's maps from the definitions that apply to it (per node, in
    /// cascade order), top-down: each node sees its parent's map overlaid
    /// with its own definitions.
    pub(crate) fn build(
        defs: &[Vec<StagedDefinition>],
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
    ) -> Self {
        let n = defs.len();
        let any_state = defs
            .iter()
            .flatten()
            .any(|(s, _, _)| *s != PseudoStateType::Normal);
        let mut envs = Self {
            maps: vec![CustomPropertyMap::new()],
            normal: vec![0; n],
            states: if any_state {
                vec![Vec::new(); n]
            } else {
                Vec::new()
            },
        };
        let hierarchy = node_hierarchy.as_container();
        for (idx, item) in hierarchy.internal.iter().enumerate().take(n) {
            if item.parent_id().is_none() {
                envs.compute_node(idx, None, &defs[idx]);
            }
        }
        // Parents by depth, the order the inheritance walk uses: a parent's
        // maps are final before its children read them.
        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes {
            let Some(parent_id) = node_id.into_crate_internal() else {
                continue;
            };
            for child_id in parent_id.az_children(&hierarchy) {
                let c = child_id.index();
                if c < n && parent_id.index() < n {
                    envs.compute_node(c, Some(parent_id.index()), &defs[c]);
                }
            }
        }
        envs
    }

    fn compute_node(&mut self, idx: usize, parent: Option<usize>, own: &[StagedDefinition]) {
        let resting: Vec<(&str, &str)> = own
            .iter()
            .filter(|(s, _, _)| *s == PseudoStateType::Normal)
            .map(|(_, name, value)| (name.as_str(), value.as_str()))
            .collect();
        let parent_resting = parent.map_or(0, |p| self.normal[p]);
        let normal = self.cascade_into(parent_resting, &resting);
        self.normal[idx] = normal;
        if self.states.is_empty() {
            return;
        }
        // The states the parent already differs in, plus the node's own.
        let mut states: Vec<PseudoStateType> = parent
            .map(|p| self.states[p].iter().map(|(s, _)| *s).collect())
            .unwrap_or_default();
        for (s, _, _) in own {
            if *s != PseudoStateType::Normal && !states.contains(s) {
                states.push(*s);
            }
        }
        let mut differing = Vec::new();
        for state in states {
            let base = parent.map_or(0, |p| self.map_index(p, state));
            // The node's resting definitions, then its definitions for the
            // state (a state rule beats a resting one, as everywhere in the
            // pseudo-state lookup), over the parent's map for the state.
            let mut in_state = resting.clone();
            in_state.extend(
                own.iter()
                    .filter(|(s, _, _)| *s == state)
                    .map(|(_, name, value)| (name.as_str(), value.as_str())),
            );
            let m = self.cascade_into(base, &in_state);
            if m != normal {
                differing.push((state, m));
            }
        }
        self.states[idx] = differing;
    }

    /// Index of `base` overlaid with `defs`: `base` itself when nothing
    /// changes, otherwise a new map.
    fn cascade_into(&mut self, base: usize, defs: &[(&str, &str)]) -> usize {
        match CustomPropertyMap::cascade_if_changed(&self.maps[base], defs) {
            Some(m) => {
                self.maps.push(m);
                self.maps.len() - 1
            }
            None => base,
        }
    }

    fn map_index(&self, node: usize, state: PseudoStateType) -> usize {
        let normal = self.normal.get(node).copied().unwrap_or(0);
        if state == PseudoStateType::Normal {
            return normal;
        }
        self.states
            .get(node)
            .and_then(|v| v.iter().find(|(s, _)| *s == state))
            .map_or(normal, |(_, m)| *m)
    }

    /// The custom properties `node` sees in `state`.
    #[must_use]
    pub fn map_for(&self, node: usize, state: PseudoStateType) -> &CustomPropertyMap {
        self.map(self.map_index(node, state))
    }

    /// The value of `--name` (without `--`) at `node` in `state`.
    #[must_use]
    pub fn get(&self, node: usize, state: PseudoStateType, name: &str) -> Option<&str> {
        self.map_for(node, state).get(name)
    }

    /// The pseudo-states in which `node` sees different custom properties
    /// than at rest, each with its map index (see [`Self::map`]).
    #[must_use]
    pub fn differing_states(&self, node: usize) -> &[(PseudoStateType, usize)] {
        match self.states.get(node) {
            Some(v) => v,
            None => &[],
        }
    }

    /// A map by index.
    #[must_use]
    pub fn map(&self, index: usize) -> &CustomPropertyMap {
        self.maps.get(index).unwrap_or(&EMPTY_MAP)
    }

    /// Whether this DOM has no custom property at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.normal.is_empty()
    }

    /// Append another DOM's maps after this one's (`CssPropertyCache::append`),
    /// shifting the other's map indices. The composed tree does not see the
    /// variables across the seam until its next restyle, exactly like the
    /// author rules it was cascaded with.
    pub(crate) fn append(&mut self, other: &mut Self, self_nodes: usize, other_nodes: usize) {
        if self.is_empty() && other.is_empty() {
            return;
        }
        if self.maps.is_empty() {
            self.maps.push(CustomPropertyMap::new());
        }
        if self.normal.is_empty() {
            self.normal = vec![0; self_nodes];
        }
        // The other's map 0 is the empty map, which this one has at 0 too.
        let offset = self.maps.len();
        let remap = move |i: usize| if i == 0 { 0 } else { offset + i - 1 };
        let other_maps = core::mem::take(&mut other.maps);
        self.maps.extend(other_maps.into_iter().skip(1));
        if other.normal.is_empty() {
            self.normal.extend(std::iter::repeat_n(0, other_nodes));
        } else {
            self.normal.extend(other.normal.drain(..).map(remap));
        }
        if !self.states.is_empty() || !other.states.is_empty() {
            if self.states.is_empty() {
                self.states = vec![Vec::new(); self_nodes];
            }
            if other.states.is_empty() {
                self.states
                    .extend(core::iter::repeat_with(Vec::new).take(other_nodes));
            } else {
                for v in other.states.drain(..) {
                    self.states
                        .push(v.into_iter().map(|(s, m)| (s, remap(m))).collect());
                }
            }
        }
    }
}

/// A node's own `var()` / `env()` declarations as one cascade resolved them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedInline {
    /// One entry per `Dynamic` declaration of the node's style, in
    /// declaration order: the declaration (so a style changed since the
    /// cascade is detected, not misread) and its value.
    values: Vec<(DynamicCssProperty, CssProperty)>,
    /// State variants: a resting `var()` declaration that wins its property,
    /// re-resolved for a pseudo-state in which the node sees different
    /// variables, with that state added to its conditions. Read after the
    /// declared ones.
    variants: Vec<(CssProperty, DynamicSelectorVec)>,
}

impl ResolvedInline {
    /// The value of the `ordinal`-th `Dynamic` declaration `declared` of a
    /// node. Before any cascade, or for a declaration added since, its
    /// fallback.
    pub(crate) fn value_of<'a>(
        resolved: Option<&'a Self>,
        ordinal: usize,
        declared: &'a DynamicCssProperty,
    ) -> &'a CssProperty {
        if let Some(r) = resolved {
            if let Some((d, v)) = r.values.get(ordinal) {
                if d == declared {
                    return v;
                }
            }
            if let Some((_, v)) = r.values.iter().find(|(d, _)| d == declared) {
                return v;
            }
        }
        &declared.default_value
    }

    /// The state variants, `(value, conditions)`.
    pub(crate) fn variants(&self) -> impl Iterator<Item = (&CssProperty, &DynamicSelectorVec)> {
        self.variants.iter().map(|(p, c)| (p, c))
    }
}

/// What the stylesheet half of `restyle` hands the variable pass.
#[derive(Debug, Default)]
pub(crate) struct VarStage {
    /// Per node, the stylesheet definitions that apply to it, in cascade
    /// order. Empty when the cascade has no variables to resolve.
    pub(crate) defs: Vec<Vec<StagedDefinition>>,
    /// Stylesheet `var()` references, pushed into `css_props` with their
    /// fallback as a placeholder at the position the cascade order gives
    /// them.
    pub(crate) pending: Vec<PendingVar>,
}

/// One stylesheet `var()` reference waiting for its node's variables.
#[derive(Debug)]
pub(crate) struct PendingVar {
    pub(crate) node: usize,
    /// Index in the node's `css_props` build vec.
    pub(crate) slot: usize,
    pub(crate) state: PseudoStateType,
    pub(crate) reference: DynamicCssProperty,
}

impl VarStage {
    pub(crate) fn new(node_count: usize, active: bool) -> Self {
        Self {
            defs: if active {
                vec![Vec::new(); node_count]
            } else {
                Vec::new()
            },
            pending: Vec::new(),
        }
    }

    pub(crate) fn define(&mut self, node: usize, state: PseudoStateType, c: &CssCustomProperty) {
        if let Some(v) = self.defs.get_mut(node) {
            v.push((state, c.name.clone(), c.value.clone()));
        }
    }
}

/// Whether the cascade has variable work on this DOM: the stylesheet or a
/// node's own style defines or reads a custom property, or a node's own
/// style holds an `env()` (resolved by the same pass).
pub(crate) fn needs_variable_pass(css: &azul_css::css::Css, node_data: &[NodeData]) -> bool {
    css.uses_custom_properties()
        || node_data.iter().any(|nd| {
            nd.style.rules.as_ref().iter().any(|r| {
                r.declarations.as_ref().iter().any(|d| {
                    matches!(
                        d,
                        CssDeclaration::Dynamic(_) | CssDeclaration::CustomProperty(_)
                    )
                })
            })
        })
}

/// The pseudo-state a declaration under `conditions` belongs to (`Normal`
/// without a pseudo-state condition), ignoring every other condition.
fn declared_state(conditions: &[DynamicSelector]) -> PseudoStateType {
    conditions
        .iter()
        .find_map(|c| match c {
            DynamicSelector::PseudoState(s) => Some(*s),
            _ => None,
        })
        .unwrap_or(PseudoStateType::Normal)
}

/// The pseudo-state a declaration under `conditions` applies in, if every
/// other condition holds under `ctx` (none does without a context - the rule
/// conditional rule blocks follow).
fn applying_state(
    conditions: &[DynamicSelector],
    ctx: Option<&DynamicSelectorContext>,
) -> Option<PseudoStateType> {
    let mut state = PseudoStateType::Normal;
    for c in conditions {
        match c {
            DynamicSelector::PseudoState(s) => state = *s,
            other => {
                if !ctx.is_some_and(|ctx| other.matches(ctx)) {
                    return None;
                }
            }
        }
    }
    Some(state)
}

/// Whether the node's own style declares `ty` in `state` (conditions
/// holding).
fn inline_declares(
    nd: &NodeData,
    ty: CssPropertyType,
    state: PseudoStateType,
    ctx: Option<&DynamicSelectorContext>,
) -> bool {
    nd.style.rules.as_ref().iter().any(|r| {
        applying_state(r.conditions.as_slice(), ctx) == Some(state)
            && r.declarations
                .as_ref()
                .iter()
                .any(|d| d.get_type() == Some(ty))
    })
}

impl CssPropertyCache {
    /// The variable pass of `restyle`: every node's custom properties, the
    /// stylesheet references (placeholders in `css_props`) and the nodes' own
    /// references resolved under the live context, state variants included.
    /// Runs BEFORE the inheritance walk, which copies resolved values to
    /// children.
    pub(crate) fn run_variable_pass(
        &mut self,
        mut stage: VarStage,
        node_data: &[NodeData],
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
    ) {
        let ctx_owned = self.dynamic_context.clone();
        let ctx = ctx_owned.as_deref();
        let n = node_data.len();
        if stage.defs.len() < n {
            stage.defs.resize_with(n, Vec::new);
        }

        // 1. The nodes' own definitions, after the stylesheet's: a node's
        //    style beats the rules that match it.
        let mut depends_on_context = false;
        for (idx, nd) in node_data.iter().enumerate() {
            for rule in nd.style.rules.as_ref() {
                let conditions = rule.conditions.as_slice();
                for d in rule.declarations.as_ref() {
                    match d {
                        CssDeclaration::CustomProperty(c) => {
                            if conditions
                                .iter()
                                .any(|cond| !matches!(cond, DynamicSelector::PseudoState(_)))
                            {
                                depends_on_context = true;
                            }
                            if let Some(state) = applying_state(conditions, ctx) {
                                stage.defs[idx].push((state, c.name.clone(), c.value.clone()));
                            }
                        }
                        CssDeclaration::Dynamic(_) if d.env_variable().is_some() => {
                            depends_on_context = true;
                        }
                        _ => {}
                    }
                }
            }
        }

        // 2. Every node's maps.
        let envs = CustomPropertyEnvs::build(&stage.defs, node_hierarchy, non_leaf_nodes);
        let mut resolver = VarResolver::default();

        // 3. Stylesheet references: overwrite the placeholders.
        for p in &stage.pending {
            let value = resolver.resolve(&p.reference, envs.map_for(p.node, p.state));
            if let Some(entry) = self.css_props.build_mut(p.node).get_mut(p.slot) {
                entry.property = value;
            }
        }
        // 3b. State variants of the resting stylesheet references that WIN
        //     their property on the node (the last resting entry of the type,
        //     not overridden by the node's own style), for every state the node
        //     sees other variables in and does not declare the property for.
        for p in stage
            .pending
            .iter()
            .filter(|p| p.state == PseudoStateType::Normal)
        {
            let differing = envs.differing_states(p.node);
            if differing.is_empty() {
                continue;
            }
            let Some(nd) = node_data.get(p.node) else {
                continue;
            };
            let ty = p.reference.default_value.get_type();
            let winner = self.css_props.build_get(p.node).and_then(|v| {
                v.iter()
                    .rposition(|e| e.state == PseudoStateType::Normal && e.prop_type == ty)
            });
            if winner != Some(p.slot) || inline_declares(nd, ty, PseudoStateType::Normal, ctx) {
                continue;
            }
            for &(state, map) in differing {
                let declared = self
                    .css_props
                    .build_get(p.node)
                    .is_some_and(|v| v.iter().any(|e| e.state == state && e.prop_type == ty))
                    || inline_declares(nd, ty, state, ctx);
                if declared {
                    continue;
                }
                let property = resolver.resolve(&p.reference, envs.map(map));
                self.css_props.push_to(
                    p.node,
                    StatefulCssProperty {
                        state,
                        prop_type: ty,
                        property,
                        ua_origin: false,
                    },
                );
            }
        }

        // 4. The nodes' own references (and their state variants).
        let mut resolved_inline: BTreeMap<usize, ResolvedInline> = BTreeMap::new();
        for (idx, nd) in node_data.iter().enumerate() {
            let mut entry = ResolvedInline::default();
            for rule in nd.style.rules.as_ref() {
                let state = declared_state(rule.conditions.as_slice());
                for d in rule.declarations.as_ref() {
                    let CssDeclaration::Dynamic(dy) = d else {
                        continue;
                    };
                    let value = match d.var_reference() {
                        Some(r) => resolver.resolve(r, envs.map_for(idx, state)),
                        None => d
                            .resolve_in_cascade(ctx)
                            .unwrap_or_else(|| dy.default_value.clone()),
                    };
                    entry.values.push((dy.clone(), value));
                }
            }
            let differing = envs.differing_states(idx);
            if !differing.is_empty() {
                entry.variants = inline_state_variants(nd, differing, &envs, &mut resolver, ctx);
            }
            if !entry.values.is_empty() || !entry.variants.is_empty() {
                resolved_inline.insert(idx, entry);
            }
        }

        self.custom_property_envs = envs;
        self.resolved_inline = resolved_inline;
        self.variables_depend_on_context = depends_on_context;
        warn_undefined_without_fallback(&resolver.missing_without_fallback);
    }
}

/// The state variants of a node's own resting `var()` declarations: each one
/// that wins its property at rest (no later resting declaration of the type),
/// re-resolved for every state in `differing` the node does not declare the
/// property for itself.
fn inline_state_variants(
    nd: &NodeData,
    differing: &[(PseudoStateType, usize)],
    envs: &CustomPropertyEnvs,
    resolver: &mut VarResolver,
    ctx: Option<&DynamicSelectorContext>,
) -> Vec<(CssProperty, DynamicSelectorVec)> {
    // Resting declarations in order: (type, var reference if it is one, conditions).
    let mut resting: Vec<(
        CssPropertyType,
        Option<&DynamicCssProperty>,
        &DynamicSelectorVec,
    )> = Vec::new();
    for rule in nd.style.rules.as_ref() {
        if applying_state(rule.conditions.as_slice(), ctx) != Some(PseudoStateType::Normal) {
            continue;
        }
        for d in rule.declarations.as_ref() {
            if let Some(ty) = d.get_type() {
                resting.push((ty, d.var_reference(), &rule.conditions));
            }
        }
    }
    let mut out = Vec::new();
    for (i, (ty, reference, conditions)) in resting.iter().enumerate() {
        let Some(reference) = reference else {
            continue;
        };
        if resting[i + 1..].iter().any(|(t, _, _)| t == ty) {
            continue; // a later resting declaration wins
        }
        for &(state, map) in differing {
            if inline_declares(nd, *ty, state, ctx) {
                continue;
            }
            let value = resolver.resolve(reference, envs.map(map));
            let mut in_state = vec![DynamicSelector::PseudoState(state)];
            in_state.extend(conditions.as_slice().iter().cloned());
            out.push((value, in_state.into()));
        }
    }
    out
}

/// Design: "with no fallback: the property's initial value, and a warning
/// once". Once per name per process, through the framework diagnostics
/// channel; `AZ_SUPPRESS=var_fallback` silences it.
#[cfg(feature = "std")]
fn warn_undefined_without_fallback(names: &BTreeSet<String>) {
    use std::sync::{Mutex, OnceLock};

    if names.is_empty() {
        return;
    }
    static SUPPRESSED: OnceLock<bool> = OnceLock::new();
    if *SUPPRESSED.get_or_init(|| {
        let v = std::env::var("AZ_SUPPRESS")
            .or_else(|_| std::env::var("AZ_SUPRESS"))
            .unwrap_or_default();
        v.split(',').any(|t| {
            let t = t.trim();
            t.eq_ignore_ascii_case(VAR_FALLBACK_SUPPRESS_TAG) || t.eq_ignore_ascii_case("all")
        })
    }) {
        return;
    }
    static WARNED: OnceLock<Mutex<BTreeSet<String>>> = OnceLock::new();
    let Ok(mut warned) = WARNED.get_or_init(|| Mutex::new(BTreeSet::new())).lock() else {
        return; // a poisoned lint set must never take the app down
    };
    for name in names {
        // Bounded: a theme full of typos is one bug, not a thousand lines.
        if warned.len() >= 64 || !warned.insert(name.clone()) {
            continue;
        }
        crate::diagnostics::emit(alloc::format!(
            "[azul][var-fallback] var(--{name}) is undefined here and declares no fallback, so \
             the property takes its initial value. Write var(--{name}, <fallback>). (suppress \
             with AZ_SUPPRESS={VAR_FALLBACK_SUPPRESS_TAG})"
        ));
    }
}

#[cfg(not(feature = "std"))]
fn warn_undefined_without_fallback(_names: &BTreeSet<String>) {}

/// Suppression tag of the undefined-variable warning, honored from
/// `AZ_SUPPRESS`.
pub const VAR_FALLBACK_SUPPRESS_TAG: &str = "var_fallback";
