//! Custom properties (`--name: value`) and `var()`: the ONE resolver both
//! cascades consult (design `RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md`
//! §7.3).
//!
//! The model is CSS's:
//!
//! - A definition is a declaration like any other - in a stylesheet rule or in a node's own style,
//!   under that rule's `@theme` / `@media` / `@os` / pseudo-state conditions - and it INHERITS: a
//!   node sees the nearest live definition of each name ([`CustomPropertyMap::cascade`]). Priority
//!   does not cross inheritance distance (design §9.1 pitfall 1): a definition on a panel beats a
//!   `:root` one inside that panel, whatever either's priority.
//! - A definition's own `var()` references are substituted where the definition IS, so a child
//!   inherits the substituted text (CSS computed-value semantics). A reference cycle makes every
//!   name in it invalid at that node - it does NOT fall back to the parent's value.
//! - A `var()` consumer ([`DynamicCssProperty`]) reads the map of the node it applies to: the first
//!   name of its fallback chain whose value parses as the consumer's property wins, otherwise its
//!   fallback ([`resolve_var`]). There is no manifest of a theme's variables, so every `var()` must
//!   declare a fallback (design §9 gap 1): the parser warns about one that does not, the widget
//!   lint rejects it, and it resolves to the property's initial value.
//!
//! The cascade (`azul_core::prop_cache`) builds one map per node and
//! pseudo-state at every restyle, under the window's live context, so a
//! light/dark flip re-resolves every variable through the ordinary restyle
//! path - no DOM rebuild.

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    css::DynamicCssProperty,
    props::property::{CssProperty, CssPropertyType},
};

/// Upper bound on one substituted value, in bytes. Substitution can grow a
/// value exponentially (`--b: var(--a) var(--a)`, `--c: var(--b) var(--b)`,
/// ...); a theme is untrusted input (design §9.1 pitfall 8), so past this a
/// value is invalid instead of eating the process.
pub const MAX_SUBSTITUTED_LEN: usize = 64 * 1024;

/// Upper bound on nested substitution while one node's definitions are
/// resolved (a definition reading a definition reading ..., or a fallback
/// that is itself a `var()`). Past it the value is invalid.
pub const MAX_REFERENCE_DEPTH: usize = 128;

/// The custom properties one node sees in one state: the nearest live
/// definition of each name, with its own `var()` references already
/// substituted. Names are stored without their leading `--`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CustomPropertyMap {
    /// Sorted by name, one entry per name.
    entries: Vec<(String, String)>,
}

impl CustomPropertyMap {
    /// The empty map: no custom property defined.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// The value of `--name` (pass the name without `--`), fully substituted.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .binary_search_by(|(k, _)| k.as_str().cmp(name))
            .ok()
            .map(|i| self.entries[i].1.as_str())
    }

    /// Number of names defined.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no name is defined.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `(name, value)` pairs, sorted by name.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> + '_ {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    fn set(&mut self, name: &str, value: String) {
        match self.entries.binary_search_by(|(k, _)| k.as_str().cmp(name)) {
            Ok(i) => self.entries[i].1 = value,
            Err(i) => self.entries.insert(i, (name.to_string(), value)),
        }
    }

    fn remove(&mut self, name: &str) {
        if let Ok(i) = self.entries.binary_search_by(|(k, _)| k.as_str().cmp(name)) {
            self.entries.remove(i);
        }
    }

    /// The map a node sees: `inherited` (its parent's map), overlaid with the
    /// node's own live definitions `own` as `(name, value)` in cascade order
    /// (a later definition of a name wins). Each own value has its `var()`
    /// references substituted HERE - against the node's other own
    /// definitions first, then `inherited` - and a definition that ends up
    /// invalid (a cycle, a reference with neither a value nor a fallback, a
    /// value past [`MAX_SUBSTITUTED_LEN`]) removes the name at this node.
    #[must_use]
    pub fn cascade(inherited: &Self, own: &[(&str, &str)]) -> Self {
        Self::cascade_if_changed(inherited, own).unwrap_or_else(|| inherited.clone())
    }

    /// [`Self::cascade`], `None` when the result equals `inherited`: most
    /// nodes define nothing, and a `* { --x: .. }` rule re-defines the same
    /// value on every element, so the caller shares the parent's map instead
    /// of storing a copy per node.
    #[must_use]
    pub fn cascade_if_changed(inherited: &Self, own: &[(&str, &str)]) -> Option<Self> {
        if own.is_empty() {
            return None;
        }
        let mut defs: BTreeMap<&str, &str> = BTreeMap::new();
        for &(name, value) in own {
            defs.insert(name, value.trim());
        }
        let mut sub = Substituter {
            defs: &defs,
            inherited,
            done: BTreeMap::new(),
            stack: Vec::new(),
            cyclic: BTreeSet::new(),
            depth: 0,
        };
        let mut out = inherited.clone();
        let mut changed = false;
        for &name in defs.keys() {
            match sub.own(name) {
                Some(value) => {
                    if out.get(name) != Some(value.as_str()) {
                        out.set(name, value);
                        changed = true;
                    }
                }
                None => {
                    if out.get(name).is_some() {
                        out.remove(name);
                        changed = true;
                    }
                }
            }
        }
        changed.then_some(out)
    }
}

/// What a name resolves to while one node's definitions are substituted.
enum Lookup {
    Value(String),
    /// Not defined, or defined but invalid: the reference's fallback applies.
    Missing,
    /// The name is being substituted further up this very chain.
    Cycle,
}

/// Substitutes one node's own definitions (see [`CustomPropertyMap::cascade`]).
struct Substituter<'a, 'm> {
    /// The node's own definitions, last one per name.
    defs: &'m BTreeMap<&'a str, &'a str>,
    inherited: &'m CustomPropertyMap,
    /// Finished own definitions (`None` = invalid).
    done: BTreeMap<&'a str, Option<String>>,
    /// The own definitions being substituted, outermost first.
    stack: Vec<&'a str>,
    /// Own definitions found to be part of a reference cycle.
    cyclic: BTreeSet<&'a str>,
    /// Current nesting of [`Self::substitute`].
    depth: usize,
}

impl<'a> Substituter<'a, '_> {
    /// The substituted value of the own definition `name`; `None` = invalid.
    fn own(&mut self, name: &'a str) -> Option<String> {
        if let Some(done) = self.done.get(name) {
            return done.clone();
        }
        let raw = *self.defs.get(name)?;
        self.stack.push(name);
        let value = self.substitute(raw);
        self.stack.pop();
        let value = if self.cyclic.contains(name) {
            None
        } else {
            value
        };
        self.done.insert(name, value.clone());
        value
    }

    fn lookup(&mut self, name: &'a str) -> Lookup {
        if let Some(pos) = self.stack.iter().position(|n| *n == name) {
            for &n in &self.stack[pos..] {
                self.cyclic.insert(n);
            }
            return Lookup::Cycle;
        }
        if self.defs.contains_key(name) {
            return self.own(name).map_or(Lookup::Missing, Lookup::Value);
        }
        self.inherited
            .get(name)
            .map_or(Lookup::Missing, |v| Lookup::Value(v.to_string()))
    }

    /// `text` with every `var()` replaced; `None` = the whole value is invalid.
    ///
    /// Recursion (a definition reading a definition, a fallback that is a
    /// `var()`) is bounded by [`MAX_REFERENCE_DEPTH`]: a theme is untrusted
    /// input, and a chain of 100 000 definitions must not overflow the stack.
    fn substitute(&mut self, text: &'a str) -> Option<String> {
        if self.depth >= MAX_REFERENCE_DEPTH {
            return None;
        }
        self.depth += 1;
        let out = self.substitute_at_depth(text);
        self.depth -= 1;
        out
    }

    fn substitute_at_depth(&mut self, text: &'a str) -> Option<String> {
        let mut out = String::with_capacity(text.len());
        let mut rest = text;
        while let Some(start) = find_var_call(rest) {
            out.push_str(&rest[..start]);
            let after = &rest[start + "var(".len()..];
            let close = closing_paren(after)?;
            let (name, fallback) = split_var_arguments(&after[..close])?;
            match self.lookup(name) {
                Lookup::Value(v) => out.push_str(&v),
                Lookup::Cycle => return None,
                Lookup::Missing => {
                    let fallback = self.substitute(fallback?.trim())?;
                    out.push_str(&fallback);
                }
            }
            if out.len() > MAX_SUBSTITUTED_LEN {
                return None;
            }
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        (out.len() <= MAX_SUBSTITUTED_LEN).then_some(out)
    }
}

/// Byte offset of the next `var(` call in `text` (ASCII case-insensitive,
/// not the tail of a longer identifier such as `myvar(`).
fn find_var_call(text: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        if bytes[i..i + 4].eq_ignore_ascii_case(b"var(") && (i == 0 || !is_ident_byte(bytes[i - 1]))
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

const fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b >= 0x80
}

/// Offset of the `)` that closes a call whose arguments start at `args`
/// (nested parentheses and quoted strings skipped); `None` if unbalanced.
pub(crate) fn closing_paren(args: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    for (i, b) in args.bytes().enumerate() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if b == b'\\' {
                escaped = true;
            } else if b == q {
                quote = None;
            }
            continue;
        }
        match b {
            b'"' | b'\'' => quote = Some(b),
            b'(' => depth += 1,
            b')' if depth == 0 => return Some(i),
            b')' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// Whether `name` (without its `--`) is a custom-property name: a non-empty
/// identifier (ASCII letters, digits, `-`, `_`, or any non-ASCII character).
#[must_use]
pub fn is_custom_property_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || !c.is_ascii())
}

/// The inside of a `var( ... )` call split into the variable name (without
/// `--`) and the fallback text, untrimmed: `"--a, rgb(1, 2, 3)"` ->
/// `("a", Some(" rgb(1, 2, 3)"))`, `"--a"` -> `("a", None)`. The split is at
/// the first TOP-LEVEL comma (the crate's one scanner,
/// [`find_top_level`](crate::props::basic::parse::find_top_level)). `None`
/// when the name is not a custom-property name (`var(a)`, `var(--)`,
/// `var(--a b)`).
#[must_use]
pub fn split_var_arguments(inner: &str) -> Option<(&str, Option<&str>)> {
    let comma = crate::props::basic::parse::find_top_level(inner, |byte| byte == b',');
    let (name, fallback) = match comma {
        Some(i) => (&inner[..i], Some(&inner[i + 1..])),
        None => (inner, None),
    };
    let name = name.trim().strip_prefix("--")?;
    is_custom_property_name(name).then_some((name, fallback))
}

/// Resolves `var()` consumers, remembering how each raw value parsed as
/// each property type: one restyle reads the same few variables on many
/// nodes (a `* { color: var(--fg) }` reads one on every element), and the
/// property parser is the expensive part.
#[derive(Debug, Default)]
pub struct VarResolver {
    parsed: BTreeMap<CssPropertyType, BTreeMap<String, Option<CssProperty>>>,
    /// Names a reference WITHOUT a fallback found undefined (or unusable)
    /// during this resolver's lifetime - the cascade warns about each once.
    pub missing_without_fallback: BTreeSet<String>,
}

impl VarResolver {
    /// The value `reference` takes on a node that sees `vars`: the first name
    /// of its fallback chain whose value parses as the reference's property,
    /// else its fallback (the property's initial value when it declared
    /// none).
    pub fn resolve(
        &mut self,
        reference: &DynamicCssProperty,
        vars: &CustomPropertyMap,
    ) -> CssProperty {
        let ty = reference.default_value.get_type();
        for name in reference.var_names() {
            let Some(raw) = vars.get(name) else {
                continue;
            };
            let by_value = self.parsed.entry(ty).or_default();
            let parsed = match by_value.get(raw) {
                Some(p) => p.clone(),
                None => {
                    let p = parse_value(ty, raw);
                    by_value.insert(raw.to_string(), p.clone());
                    p
                }
            };
            if let Some(p) = parsed {
                return p;
            }
        }
        if reference.default_value.is_initial() {
            if let Some(first) = reference.var_names().next() {
                if !self.missing_without_fallback.contains(first) {
                    self.missing_without_fallback.insert(first.to_string());
                }
            }
        }
        reference.default_value.clone()
    }
}

/// [`VarResolver::resolve`] without a memo, for one-off reads.
#[must_use]
pub fn resolve_var(reference: &DynamicCssProperty, vars: &CustomPropertyMap) -> CssProperty {
    VarResolver::default().resolve(reference, vars)
}

#[cfg(feature = "parser")]
fn parse_value(ty: CssPropertyType, raw: &str) -> Option<CssProperty> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    crate::props::property::parse_css_property(ty, raw).ok()
}

/// Without the parser there is no way to type a raw value: every reference
/// takes its fallback.
#[cfg(not(feature = "parser"))]
fn parse_value(_ty: CssPropertyType, _raw: &str) -> Option<CssProperty> {
    None
}
