//! Lowering: the parsed CSS model -> [`ir`](super::ir) expressions.
//!
//! Written ONCE for every language. The exhaustive per-type part (one match
//! arm per `CssProperty` variant, one `impl Lower` per api.json struct / enum /
//! Vec) is generated into [`super::lower_types`] from api.json by
//! `css/tools/gen_codegen_lowering.py`; this file holds the hand-written
//! special cases and the module builders:
//!
//! * [`lower_styles`] - a `Css` as named styles: one
//!   `CssPropertyWithConditionsVec` per base selector (`.btn`, `.btn:hover`
//!   and `@media (..) { .btn {..} }` all merge into `style_btn`), with the
//!   pseudo-state / `@media` / `@os` / `@theme` conditions as `apply_if`
//!   conditions - how the widgets build their styles.
//! * [`lower_stylesheet`] - the exact `Css` value (rules, selectors,
//!   priorities, conditions, `@keyframes`).
//! * [`lower_property_list`] - an existing `&[CssPropertyWithConditions]`
//!   (a widget's inline style) as one named style.

use alloc::{
    format,
    string::{String, ToString},
    vec::Vec,
};

use super::{
    ir::{EnumShape, Expr, Ident, Item, Module, Prim},
    lower_types::lower_css_property,
};
use crate::{
    corety::AzString,
    css::{BoxOrStatic, Css, CssDeclaration, CssPathPseudoSelector, CssPathSelector, CssPropertyValue},
    dynamic_selector::{
        CssPropertyWithConditions, DynamicSelector, OsCondition, PseudoStateType, ThemeCondition,
    },
    props::{
        basic::{
            font::FontRef,
            length::{FloatValue, PercentageValue, SizeMetric},
            pixel::PixelValue,
        },
        layout::grid::GridMinMax,
        property::CssProperty,
    },
};

/// Lower a value into the construction IR.
pub trait Lower {
    fn lower(&self) -> Expr;
}

// ------------------------------------------------------------------ primitives

macro_rules! lower_int {
    ($($t:ty => $p:ident),* $(,)?) => {$(
        impl Lower for $t {
            fn lower(&self) -> Expr {
                Expr::int(i128::from(*self), Prim::$p)
            }
        }
    )*};
}
lower_int!(u8 => U8, u16 => U16, u32 => U32, u64 => U64, i8 => I8, i16 => I16, i32 => I32, i64 => I64);

impl Lower for usize {
    fn lower(&self) -> Expr {
        Expr::int(i128::try_from(*self).unwrap_or(i128::MAX), Prim::Usize)
    }
}

impl Lower for isize {
    fn lower(&self) -> Expr {
        Expr::int(i128::try_from(*self).unwrap_or(0), Prim::Isize)
    }
}

impl Lower for f32 {
    fn lower(&self) -> Expr {
        Expr::f32(*self)
    }
}

impl Lower for f64 {
    fn lower(&self) -> Expr {
        Expr::f64(*self)
    }
}

impl Lower for bool {
    fn lower(&self) -> Expr {
        Expr::Bool(*self)
    }
}

impl Lower for AzString {
    fn lower(&self) -> Expr {
        Expr::str(self.as_str())
    }
}

// ------------------------------------------------------- fixed-point numbers

/// The shortest decimal text `t` for which `FloatValue::new(t as f32)`
/// stores exactly `number` again.
///
/// `FloatValue` keeps `value * 1000` TRUNCATED to an `isize`, so the naive
/// `number / 1000` is not always stable: `0.7px` parses to `699` (0.7_f32 is
/// 0.69999..), and `PixelValue::px(0.699)` would give `698`. This tries the
/// short roundings first (`699` -> `"0.7"`), then the exact decimal, then a
/// value half a unit further out (`"0.7005"` -> `700`), verifying each with
/// the real `FloatValue::new`.
#[must_use]
pub fn fixed_point_text(number: isize) -> String {
    let ok = |t: &str| {
        t.parse::<f32>()
            .map(|v| FloatValue::new(v).number == number)
            .unwrap_or(false)
    };
    for decimals in 0..=3 {
        let t = fixed_decimal(number, decimals);
        if ok(&t) {
            return t;
        }
    }
    let t = half_unit_out(number);
    if ok(&t) {
        return t;
    }
    fixed_decimal(number, 3)
}

/// `number / 1000` rounded half away from zero to `decimals` (0..=3) places,
/// trailing zeros trimmed but always with a `.` (`"10.0"`, `"0.75"`).
fn fixed_decimal(number: isize, decimals: u32) -> String {
    let neg = number < 0;
    let abs = i128::try_from(number).unwrap_or(0).unsigned_abs();
    let div = 10u128.pow(3 - decimals);
    let q = (abs + div / 2) / div;
    let scale = 10u128.pow(decimals);
    format_decimal(neg && q != 0, q / scale, q % scale, decimals)
}

/// `(|number| + 0.5) / 1000` with the sign of `number`, 4 decimals.
fn half_unit_out(number: isize) -> String {
    let neg = number < 0;
    let abs = i128::try_from(number).unwrap_or(0).unsigned_abs();
    let q = abs * 10 + 5;
    format_decimal(neg, q / 10_000, q % 10_000, 4)
}

fn format_decimal(neg: bool, int: u128, frac: u128, decimals: u32) -> String {
    let mut s = String::new();
    if neg {
        s.push('-');
    }
    s.push_str(&int.to_string());
    s.push('.');
    if decimals == 0 {
        s.push('0');
        return s;
    }
    let digits = format!("{frac:0width$}", width = decimals as usize);
    let trimmed = digits.trim_end_matches('0');
    s.push_str(if trimmed.is_empty() { "0" } else { trimmed });
    s
}

impl Lower for FloatValue {
    fn lower(&self) -> Expr {
        Expr::call(
            "FloatValue",
            "create",
            vec![Expr::f32_text(fixed_point_text(self.number))],
        )
    }
}

impl Lower for PercentageValue {
    fn lower(&self) -> Expr {
        Expr::strukt("PercentageValue", vec![("number", self.raw_number().lower())])
    }
}

impl Lower for PixelValue {
    fn lower(&self) -> Expr {
        let value = Expr::f32_text(fixed_point_text(self.number.number));
        let ctor = match self.metric {
            SizeMetric::Px => Some("px"),
            SizeMetric::Pt => Some("pt"),
            SizeMetric::Em => Some("em"),
            SizeMetric::Rem => Some("rem"),
            SizeMetric::Percent => Some("percent"),
            SizeMetric::In
            | SizeMetric::Cm
            | SizeMetric::Mm
            | SizeMetric::Vw
            | SizeMetric::Vh
            | SizeMetric::Vmin
            | SizeMetric::Vmax => None,
        };
        match ctor {
            Some(method) => Expr::call("PixelValue", method, vec![value]),
            None => Expr::call("PixelValue", "from_metric", vec![self.metric.lower(), value]),
        }
    }
}

// ------------------------------------------------ values the ABI cannot build

impl Lower for FontRef {
    fn lower(&self) -> Expr {
        Expr::unsupported(
            "a FontRef (a font loaded at runtime) has no source form - use a font-family name",
        )
    }
}

impl Lower for GridMinMax {
    fn lower(&self) -> Expr {
        Expr::unsupported(
            "grid `minmax()` tracks hold raw pointers in the C ABI and cannot be built from \
             the bindings",
        )
    }
}

/// `BoxOrStatic<T>` is a pointer wrapper; the api.json constructors that take
/// one (`CssProperty::box_shadow_left`) take the plain `T`.
impl<T: Lower> Lower for BoxOrStatic<T> {
    fn lower(&self) -> Expr {
        self.as_ref().lower()
    }
}

// ------------------------------------------------------------- CssProperty

impl Lower for CssProperty {
    fn lower(&self) -> Expr {
        lower_css_property(self)
    }
}

/// Lower one `CssProperty::<variant>(CssPropertyValue<T>)`.
///
/// * `Exact(x)` with an api.json constructor -> `CssProperty::<ctor>(x)`
///   (`CssProperty::width(LayoutWidth::Px(..))`) - available in every binding;
/// * `Exact(x)` without one -> the variant constructor over the
///   monomorphized alias: `CssProperty::CaretWidth(CaretWidthValue::Exact(x))`;
/// * `auto` / `none` / `initial` / `inherit` ->
///   `CssProperty::auto(CssPropertyType::Width)`;
/// * `revert` / `unset` -> `CssProperty::Width(LayoutWidthValue::Revert)`
///   (shadowed variant constructor: see [`EnumShape::TaggedShadowed`]).
pub(crate) fn prop_value<T: Lower>(
    v: &CssPropertyValue<T>,
    variant: &'static str,
    alias: &'static str,
    inner: &'static str,
    ctor: Option<&'static str>,
) -> Expr {
    match v {
        CssPropertyValue::Exact(x) => match ctor {
            Some(c) => Expr::call("CssProperty", c, vec![x.lower()]),
            None => wrapped_value(variant, alias, inner, ctor, "Exact", vec![x.lower()]),
        },
        CssPropertyValue::Auto => keyword(variant, "auto"),
        CssPropertyValue::None => keyword(variant, "none"),
        CssPropertyValue::Initial => keyword(variant, "initial"),
        CssPropertyValue::Inherit => keyword(variant, "inherit"),
        CssPropertyValue::Revert => wrapped_value(variant, alias, inner, ctor, "Revert", Vec::new()),
        CssPropertyValue::Unset => wrapped_value(variant, alias, inner, ctor, "Unset", Vec::new()),
    }
}

/// Like [`prop_value`], for a variant whose payload is a `BoxOrStatic<T>`
/// and which has no api.json constructor (`text-shadow`): the pointer
/// wrapper cannot be built from the bindings, so `Exact` is unsupported.
pub(crate) fn prop_boxed_without_ctor<T>(
    v: &CssPropertyValue<T>,
    variant: &'static str,
    alias: &'static str,
    inner: &'static str,
) -> Expr {
    match v {
        CssPropertyValue::Exact(_) => Expr::unsupported(&format!(
            "CssProperty::{variant} holds a {inner} (a pointer wrapper) and api.json has no \
             constructor for it"
        )),
        CssPropertyValue::Auto => keyword(variant, "auto"),
        CssPropertyValue::None => keyword(variant, "none"),
        CssPropertyValue::Initial => keyword(variant, "initial"),
        CssPropertyValue::Inherit => keyword(variant, "inherit"),
        CssPropertyValue::Revert => wrapped_value(variant, alias, inner, None, "Revert", Vec::new()),
        CssPropertyValue::Unset => wrapped_value(variant, alias, inner, None, "Unset", Vec::new()),
    }
}

/// `CssProperty::auto(CssPropertyType::Width)`.
fn keyword(variant: &'static str, which: &str) -> Expr {
    Expr::call(
        "CssProperty",
        which,
        vec![Expr::unit("CssPropertyType", EnumShape::CLike, variant)],
    )
}

/// `CssProperty::Width(LayoutWidthValue::<value_variant>(args))`.
fn wrapped_value(
    variant: &'static str,
    alias: &'static str,
    inner: &'static str,
    ctor: Option<&'static str>,
    value_variant: &str,
    args: Vec<Expr>,
) -> Expr {
    let shape = if ctor.is_some() {
        EnumShape::TaggedShadowed
    } else {
        EnumShape::Tagged
    };
    Expr::variant(
        "CssProperty",
        shape,
        variant,
        vec![Expr::variant(
            alias,
            EnumShape::Generic {
                base: "CssPropertyValue",
                arg: inner,
            },
            value_variant,
            args,
        )],
    )
}

// ------------------------------------------------ CssPropertyWithConditions

impl Lower for CssPropertyWithConditions {
    fn lower(&self) -> Expr {
        lower_with_conditions(lower_css_property(&self.property), self.apply_if.as_slice())
    }
}

/// Wrap a lowered `CssProperty` in the most idiomatic
/// `CssPropertyWithConditions` constructor api.json has for `conditions`
/// (`simple`, `on_hover`, `on_active`, `on_focus`, `when_disabled`,
/// `dark_theme`, `light_theme`, `on_windows` / `on_macos` / `on_linux`,
/// `on_os`, `with_condition`, `with_conditions`).
#[must_use]
pub fn lower_with_conditions(property: Expr, conditions: &[DynamicSelector]) -> Expr {
    if matches!(property, Expr::Unsupported { .. }) {
        return property;
    }
    let one = |method: &str, property: Expr| Expr::call("CssPropertyWithConditions", method, vec![property]);
    match conditions {
        [] => one("simple", property),
        [DynamicSelector::PseudoState(PseudoStateType::Hover)] => one("on_hover", property),
        [DynamicSelector::PseudoState(PseudoStateType::Active)] => one("on_active", property),
        [DynamicSelector::PseudoState(PseudoStateType::Focus)] => one("on_focus", property),
        [DynamicSelector::PseudoState(PseudoStateType::Disabled)] => one("when_disabled", property),
        [DynamicSelector::Theme(ThemeCondition::Dark)] => one("dark_theme", property),
        [DynamicSelector::Theme(ThemeCondition::Light)] => one("light_theme", property),
        [DynamicSelector::Os(OsCondition::Windows)] => one("on_windows", property),
        [DynamicSelector::Os(OsCondition::MacOS)] => one("on_macos", property),
        [DynamicSelector::Os(OsCondition::Linux)] => one("on_linux", property),
        [DynamicSelector::Os(os)] => Expr::call(
            "CssPropertyWithConditions",
            "on_os",
            vec![property, os.lower()],
        ),
        [single] => Expr::call(
            "CssPropertyWithConditions",
            "with_condition",
            vec![property, single.lower()],
        ),
        many => Expr::call(
            "CssPropertyWithConditions",
            "with_conditions",
            vec![
                property,
                Expr::vec(
                    "DynamicSelectorVec",
                    "DynamicSelector",
                    many.iter().map(Lower::lower).collect(),
                ),
            ],
        ),
    }
}

/// The dynamic pseudo-state a trailing `:hover` / `:active` / ... selector
/// maps to; `None` for structural pseudo-classes (`:first`, `:nth-child`, ..)
/// that stay part of the base selector.
#[must_use]
pub const fn pseudo_state(p: &CssPathPseudoSelector) -> Option<PseudoStateType> {
    match p {
        CssPathPseudoSelector::Hover => Some(PseudoStateType::Hover),
        CssPathPseudoSelector::Active => Some(PseudoStateType::Active),
        CssPathPseudoSelector::Focus => Some(PseudoStateType::Focus),
        CssPathPseudoSelector::SeatFocus => Some(PseudoStateType::SeatFocus),
        CssPathPseudoSelector::Backdrop => Some(PseudoStateType::Backdrop),
        CssPathPseudoSelector::Dragging => Some(PseudoStateType::Dragging),
        CssPathPseudoSelector::DragOver => Some(PseudoStateType::DragOver),
        CssPathPseudoSelector::Placeholder => Some(PseudoStateType::Placeholder),
        CssPathPseudoSelector::First
        | CssPathPseudoSelector::Last
        | CssPathPseudoSelector::NthChild(_)
        | CssPathPseudoSelector::Lang(_)
        | CssPathPseudoSelector::Root => None,
    }
}

// ------------------------------------------------------------ module builders

/// One property of a flat style, lowered, plus the notes a reader needs.
fn lower_declaration(decl: &CssDeclaration, conditions: &[DynamicSelector], notes: &mut Vec<String>) -> Expr {
    let (prop, dynamic_id) = match decl {
        CssDeclaration::Static(p) => (p, None),
        CssDeclaration::Dynamic(d) => (&d.default_value, Some(d.dynamic_id.as_str())),
    };
    if let Some(id) = dynamic_id {
        notes.push(format!(
            "`{}` is a runtime reference (`{id}`); a flat property list holds its fallback",
            prop.format_css().trim_end_matches(';')
        ));
    }
    let e = lower_with_conditions(lower_css_property(prop), conditions);
    note_if_unsupported(&e, prop, notes);
    e
}

/// "dropped `grid-auto-rows: ...`: <reasons>" when `e` cannot be built.
fn note_if_unsupported(e: &Expr, prop: &CssProperty, notes: &mut Vec<String>) {
    if e.contains_unsupported() {
        let mut reasons = Vec::new();
        e.unsupported_reasons(&mut reasons);
        notes.push(format!(
            "dropped `{}`: {}",
            prop.format_css().trim_end_matches(';'),
            reasons.join("; ")
        ));
    }
}

/// A stylesheet as named styles: one `CssPropertyWithConditionsVec` per base
/// selector, in first-appearance order. The trailing pseudo-states of a
/// selector (`.btn:hover`) and the rule's `@media` / `@os` / `@theme` /
/// `@lang` conditions become `apply_if` conditions; the rest of the selector
/// names the style (`.btn-primary` -> `style_btn_primary`). `@keyframes` are
/// not part of a flat style (see [`lower_stylesheet`]).
#[must_use]
pub fn lower_styles(css: &Css) -> Module {
    struct Group {
        base: String,
        selectors: Vec<String>,
        props: Vec<Expr>,
        notes: Vec<String>,
    }
    let mut groups: Vec<Group> = Vec::new();

    for rule in css.rules.as_slice() {
        let sels = rule.path.selectors.as_slice();
        let mut end = sels.len();
        let mut states: Vec<PseudoStateType> = Vec::new();
        while end > 0 {
            match &sels[end - 1] {
                CssPathSelector::PseudoSelector(p) => match pseudo_state(p) {
                    Some(s) => {
                        states.push(s);
                        end -= 1;
                    }
                    None => break,
                },
                _ => break,
            }
        }
        states.reverse();
        let mut base: String = sels[..end].iter().map(ToString::to_string).collect();
        if base.is_empty() {
            base.push('*');
        }
        let full: String = sels.iter().map(ToString::to_string).collect();

        let mut conditions: Vec<DynamicSelector> =
            states.into_iter().map(DynamicSelector::PseudoState).collect();
        conditions.extend(rule.conditions.as_slice().iter().cloned());

        let idx = match groups.iter().position(|g| g.base == base) {
            Some(i) => i,
            None => {
                groups.push(Group {
                    base: base.clone(),
                    selectors: Vec::new(),
                    props: Vec::new(),
                    notes: Vec::new(),
                });
                groups.len() - 1
            }
        };
        let g = &mut groups[idx];
        let shown = if full.is_empty() { "*".to_string() } else { full };
        let shown = if rule.conditions.as_slice().is_empty() {
            shown
        } else {
            format!("{shown} (conditional)")
        };
        if !g.selectors.contains(&shown) {
            g.selectors.push(shown);
        }
        for decl in rule.declarations.as_slice() {
            let e = lower_declaration(decl, &conditions, &mut g.notes);
            g.props.push(e);
        }
    }

    let mut used: Vec<String> = Vec::new();
    let items = groups
        .into_iter()
        .map(|g| {
            let mut name = Ident::from_text(&g.base).with_prefix("style");
            let base_name = name.snake();
            let mut n = 2;
            while used.contains(&name.snake()) {
                name = Ident::from_text(&format!("{base_name}_{n}"));
                n += 1;
            }
            used.push(name.snake());
            let mut doc = vec![format!("CSS: {}", g.selectors.join(", "))];
            doc.extend(g.notes);
            Item {
                name,
                doc,
                ty: "CssPropertyWithConditionsVec".to_string(),
                value: Expr::vec(
                    "CssPropertyWithConditionsVec",
                    "CssPropertyWithConditions",
                    g.props,
                ),
            }
        })
        .collect();
    Module { items }
}

/// The exact `Css` value (rules with their selectors, declarations,
/// conditions and priorities, plus `@keyframes`) as one item `stylesheet`.
#[must_use]
pub fn lower_stylesheet(css: &Css) -> Module {
    let value = css.lower();
    let mut notes = Vec::new();
    value.unsupported_reasons(&mut notes);
    let mut doc = vec!["The whole stylesheet as a `Css` value.".to_string()];
    doc.extend(notes.into_iter().map(|n| format!("dropped: {n}")));
    Module {
        items: vec![Item {
            name: Ident::from_text("stylesheet"),
            doc,
            ty: "Css".to_string(),
            value,
        }],
    }
}

/// An existing property list (a widget's inline style) as one named style.
#[must_use]
pub fn lower_property_list(name: &str, props: &[CssPropertyWithConditions]) -> Module {
    let mut notes = Vec::new();
    let items: Vec<Expr> = props
        .iter()
        .map(|p| {
            let e = p.lower();
            note_if_unsupported(&e, &p.property, &mut notes);
            e
        })
        .collect();
    Module {
        items: vec![Item {
            name: Ident::from_text(name),
            doc: notes,
            ty: "CssPropertyWithConditionsVec".to_string(),
            value: Expr::vec(
                "CssPropertyWithConditionsVec",
                "CssPropertyWithConditions",
                items,
            ),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrips(n: isize) -> bool {
        FloatValue::new(fixed_point_text(n).parse::<f32>().unwrap()).number == n
    }

    #[test]
    fn a_parsed_decimal_prints_as_the_decimal_the_author_wrote() {
        // 0.7_f32 * 1000 truncates to 699: "0.7" is what reproduces it
        let n = FloatValue::new(0.7).number;
        assert_eq!(fixed_point_text(n), "0.7");
        assert_eq!(fixed_point_text(10_000), "10.0");
        assert_eq!(fixed_point_text(-1_250), "-1.25");
        assert_eq!(fixed_point_text(0), "0.0");
    }

    #[test]
    fn every_fixed_point_number_in_the_css_range_round_trips_exactly() {
        for n in -20_000..=20_000 {
            assert!(roundtrips(n), "{n} -> {}", fixed_point_text(n));
        }
        for n in [1_000_000, 123_456_789, -987_654, 16_777_216] {
            assert!(roundtrips(n), "{n} -> {}", fixed_point_text(n));
        }
    }
}
