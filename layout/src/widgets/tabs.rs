//! Native-styled tab widget consisting of a [`TabHeader`] (the clickable tab bar)
//! and [`TabContent`] (the panel shown for the active tab).
//!
//! Two looks (`with_theme`): flat emulates the Windows-native tab control via
//! the inline CSS constants below; flora is `themes::flora::tab_header_look`.
//! Unpinned, both follow the app theme.

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData, Update},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClass::Class, IdOrClassVec},
    refany::RefAny,
};
use azul_css::css::BoxOrStatic;
#[allow(clippy::wildcard_imports)]
// widget/render module pulls in the css property/value types it builds with
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    widgets::themes::{flat, system_palette},
};

/// Dark theme: a tab's face is the desktop's recessed surface, one step away
/// from the content panel it opens onto (`system:window-background`) - the
/// way the light theme's grey strip sits apart from the white panel.
const TAB_FACE_DARK: CssPropertyWithConditions = CssPropertyWithConditions::dark_mode(
    CssProperty::const_background_content(system_palette::UNDER_PAGE_BACKGROUND),
);

// ---- R5: the BASE - the structure every theme's tab bar and panel share ----
//
// A theme's part is its base below, THEN its skin (paint and metrics): the
// `CSS_MATCH_*` constants for flat (`themes::flat::tab_header_look`), and
// `themes::flora::tab_header_look` for flora. The base comes first in every
// theme, so an unpinned bar declares it once, outside every `@theme` block.
// What the themes lay out differently by design stays in their skins: the
// header's `align-items` (flora sets its tabs ON the strip's rule, `end`;
// flat's native tabs hang from the top of the bar), the spacer before the
// first tab (flat's grows, `flex-grow: 1`; flora's is a fixed curve's width,
// `0`) and flora's selected tab, `position: relative` for the curves it hangs
// off its sides (the Australis tab, below).

/// The bar: a flex row of spacer, tabs, spacer. Without `display: flex` the
/// row's `flex-direction` does nothing and the tabs stack vertically.
pub(crate) static HEADER_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Flex)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_direction(LayoutFlexDirection::Row)),
];

/// The spacer after the last tab: it takes the rest of the bar.
pub(crate) static AFTER_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

/// Every tab, active or not: its border outside its content box (a tab's
/// padding and height are its content's, in every theme), its content
/// centred, and the pointer - a tab is clicked.
pub(crate) static TAB_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_box_sizing(LayoutBoxSizing::ContentBox)),
    CssPropertyWithConditions::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

/// The panel, padded or not: it takes the height the tab widget leaves.
pub(crate) static PANEL_BASE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(1))),
];

// ---- The Australis tab: a selected tab's two curved sides ----
//
// Firefox 29-56 ("Australis", 2014) cut its selected tab in three: a start
// curve, the middle and an end curve. Each side is an S that leaves the
// strip's foot horizontally, climbs, and rolls over into the tab's top edge,
// so the tab and the rule it stands on read as one piece. A theme that cuts
// its selected tab that way (flora's tab bar and ribbon) hangs the two curves
// off the tab's box with [`australis_curves`]: each is a fixed
// `CURVE_WIDTH` x `height` box with an SVG user space of one unit per px - so
// the S keeps its shape on a tab of any width - holding the FILL (the face,
// clipped to the inside of the S: the engine's own path clip) and the METAL
// (the S itself, the tab's edge: the band the S covers at its gauge, filled
// through the same path clip with whatever the theme paints it - a gradient
// rolls along it the way it rolls down the tab's head, which a flat stroke
// could not). The middle is the tab's own box. Beside each foot a run-out can
// ease the strip's rule into the S's foot. The curves and run-outs are
// children of the tab, so a press on a foot presses the tab and never the
// strip behind it.

/// How far a selected tab's foot flares out past its box, each side.
pub(crate) const CURVE_WIDTH: f32 = 18.0;

/// The left curve.
pub(crate) const CURVE_LEFT_CLASS: &str = "__azul-native-tab-curve-left";
/// The right curve.
pub(crate) const CURVE_RIGHT_CLASS: &str = "__azul-native-tab-curve-right";
/// The face inside a curve.
pub(crate) const CURVE_FILL_CLASS: &str = "__azul-native-tab-curve-fill";
/// The S itself, the tab's edge: a band of metal.
pub(crate) const CURVE_STROKE_CLASS: &str = "__azul-native-tab-curve-stroke";
/// The run-out beside the left foot.
pub(crate) const RUNOUT_LEFT_CLASS: &str = "__azul-native-tab-runout-left";
/// The run-out beside the right foot.
pub(crate) const RUNOUT_RIGHT_CLASS: &str = "__azul-native-tab-runout-right";

/// What a theme decides about the curves of its selected tab.
#[derive(Debug, Clone)]
pub(crate) struct TabCurveLook {
    /// The selected tab's height in px (its border box): the S runs from its
    /// foot to its top.
    pub(crate) height: f32,
    /// The edge's gauge in px: the width of the band the S is, and that of
    /// the tab's top edge and the strip's rule, which the S joins - its
    /// centre line runs half a gauge in from the foot and from the top.
    pub(crate) gauge: f32,
    /// The face inside the left S.
    pub(crate) left_fill: CssPropertyWithConditionsVec,
    /// The face inside the right S.
    pub(crate) right_fill: CssPropertyWithConditionsVec,
    /// The S's metal: the band's paint, a background laid over the curve's
    /// whole box (the tab's own height) and seen through the band.
    pub(crate) metal: CssPropertyWithConditionsVec,
    /// How far along the strip's rule each run-out reaches from the foot, in
    /// px; `0` hangs none.
    pub(crate) runout: f32,
    /// The left run-out's paint: a `runout` x `gauge` band lying on the rule.
    pub(crate) runout_left: CssPropertyWithConditionsVec,
    /// The right run-out's paint.
    pub(crate) runout_right: CssPropertyWithConditionsVec,
}

/// `u` (0 at the foot's end, 1 at the tab's side) across a curve `w` wide,
/// in its user space: the left curve rises left to right, the right one is
/// its mirror.
fn curve_x(u: f32, w: f32, left: bool) -> f32 {
    if left {
        u * w
    } else {
        (1.0 - u) * w
    }
}

/// The S of one side in its curve's user space (`w` x `h`): a cubic from the
/// foot to the top, horizontal at both ends, its control points crossed so
/// the middle climbs steeply - the Azlin design system's cut of the
/// Australis tab (`M0 30 C12 30 8 0 22 0`: the controls at 0.55 and 0.36 of
/// the curve's width).
fn curve_s(w: f32, h: f32, gauge: f32, left: bool) -> SvgCubicCurve {
    let (foot, top) = (h - gauge / 2.0, gauge / 2.0);
    let at = |u: f32, y: f32| SvgPoint {
        x: curve_x(u, w, left),
        y,
    };
    SvgCubicCurve {
        start: at(0.0, foot),
        ctrl_1: at(0.55, foot),
        ctrl_2: at(0.36, top),
        end: at(1.0, top),
    }
}

/// The inside of the S - between it, the tab's side of the box and the
/// foot - closed. The stroke covers its curved edge.
fn curve_fill(s: SvgCubicCurve, w: f32, h: f32, left: bool) -> azul_core::svg::SvgPath {
    use azul_core::svg::{SvgLine, SvgPath, SvgPathElement, SvgPathElementVec};
    let foot_corner = SvgPoint {
        x: curve_x(0.0, w, left),
        y: h,
    };
    let side_corner = SvgPoint {
        x: curve_x(1.0, w, left),
        y: h,
    };
    SvgPath::create(SvgPathElementVec::from_vec(vec![
        SvgPathElement::Line(SvgLine::new(foot_corner, s.start)),
        SvgPathElement::CubicCurve(s),
        SvgPathElement::Line(SvgLine::new(s.end, side_corner)),
        SvgPathElement::Line(SvgLine::new(side_corner, foot_corner)),
    ]))
}

/// The point of the cubic `s` at `t`, and the tangent there.
fn cubic_at(s: &SvgCubicCurve, t: f32) -> (SvgPoint, (f32, f32)) {
    let u = 1.0 - t;
    let (p0, p1, p2, p3) = (s.start, s.ctrl_1, s.ctrl_2, s.end);
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    let point = SvgPoint {
        x: a * p0.x + b * p1.x + c * p2.x + d * p3.x,
        y: a * p0.y + b * p1.y + c * p2.y + d * p3.y,
    };
    let (e, f, g) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
    let tangent = (
        e * (p1.x - p0.x) + f * (p2.x - p1.x) + g * (p3.x - p2.x),
        e * (p1.y - p0.y) + f * (p2.y - p1.y) + g * (p3.y - p2.y),
    );
    (point, tangent)
}

/// How many straight runs each edge of the band follows the S in: at a
/// curve's size the chords stay a hundredth of a px off the arc.
const BAND_STEPS: u16 = 24;

/// The band the S covers at `gauge` - the S offset half a gauge to each side
/// along its normal, one edge out and the other back, closed across both
/// ends. The S leaves its foot and reaches its head level, so the ends are
/// cut square: over the rule's gauge at the foot, over the head's at the top.
fn curve_band(s: SvgCubicCurve, gauge: f32) -> azul_core::svg::SvgPath {
    use azul_core::svg::{SvgLine, SvgPath, SvgPathElement, SvgPathElementVec};
    let half = gauge / 2.0;
    let steps = usize::from(BAND_STEPS);
    let mut one_side = Vec::with_capacity(steps + 1);
    let mut other_side = Vec::with_capacity(steps + 1);
    for i in 0..=BAND_STEPS {
        let (p, (dx, dy)) = cubic_at(&s, f32::from(i) / f32::from(BAND_STEPS));
        let length = dx.hypot(dy);
        let (nx, ny) = if length > 0.0 {
            (-dy / length, dx / length)
        } else {
            (0.0, 1.0)
        };
        one_side.push(SvgPoint {
            x: nx.mul_add(half, p.x),
            y: ny.mul_add(half, p.y),
        });
        other_side.push(SvgPoint {
            x: nx.mul_add(-half, p.x),
            y: ny.mul_add(-half, p.y),
        });
    }
    let outline: Vec<SvgPoint> = one_side.into_iter().chain(other_side.into_iter().rev()).collect();
    let runs = (0..outline.len())
        .map(|i| {
            SvgPathElement::Line(SvgLine::new(outline[i], outline[(i + 1) % outline.len()]))
        })
        .collect();
    SvgPath::create(SvgPathElementVec::from_vec(runs))
}

/// One curve: hung off the tab's `left` or right side, standing on its foot,
/// the fill and the metal over the whole of it.
fn curve(look: &TabCurveLook, left: bool) -> Dom {
    use azul_core::{
        dom::SvgNodeData,
        svg::{SvgMultiPolygon, SvgPath, SvgPathVec},
    };

    use crate::widgets::themes::decl;

    let (w, h) = (CURVE_WIDTH, look.height);
    let s = curve_s(w, h, look.gauge, left);
    let shape = |path: SvgPath| {
        SvgNodeData::Path(SvgMultiPolygon::create(SvgPathVec::from_vec(vec![path])))
    };
    // Over the whole curve, as the chart places its shapes: the box IS the
    // user space, whatever padding or border it carries.
    let over = |own: &CssPropertyWithConditionsVec| {
        let mut v = vec![
            decl::position(LayoutPosition::Absolute),
            decl::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
            decl::simple(CssProperty::const_top(LayoutTop::const_px(0))),
            decl::simple(CssProperty::const_right(LayoutRight::const_px(0))),
            decl::simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
        ];
        v.extend(own.as_ref().iter().cloned());
        CssPropertyWithConditionsVec::from_vec(v)
    };
    let fill = Dom::create_div()
        .with_ids_and_classes(decl::classes(&[CURVE_FILL_CLASS]))
        .with_css_props(over(if left {
            &look.left_fill
        } else {
            &look.right_fill
        }))
        .with_svg_data(shape(curve_fill(s, w, h, left)));
    let metal = Dom::create_div()
        .with_ids_and_classes(decl::classes(&[CURVE_STROKE_CLASS]))
        .with_css_props(over(&look.metal))
        .with_svg_data(shape(curve_band(s, look.gauge)));
    let side = if left {
        decl::simple(CssProperty::const_left(LayoutLeft::px(-w)))
    } else {
        decl::simple(CssProperty::const_right(LayoutRight::px(-w)))
    };
    Dom::create_div()
        .with_ids_and_classes(decl::classes(&[if left {
            CURVE_LEFT_CLASS
        } else {
            CURVE_RIGHT_CLASS
        }]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            decl::position(LayoutPosition::Absolute),
            side,
            decl::simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
            decl::px_width(w),
            decl::px_height(h),
        ]))
        .with_svg_data(SvgNodeData::ViewBox {
            min_x: 0.0,
            min_y: 0.0,
            width: w,
            height: h,
        })
        .with_children(DomVec::from_vec(vec![fill, metal]))
}

/// One run-out: a `runout` x `gauge` band hung off the tab's `left` or right
/// side past its curve, lying on the strip's rule and ending at the foot.
fn runout(look: &TabCurveLook, left: bool) -> Dom {
    use crate::widgets::themes::decl;

    let reach = -(CURVE_WIDTH + look.runout);
    let side = if left {
        decl::simple(CssProperty::const_left(LayoutLeft::px(reach)))
    } else {
        decl::simple(CssProperty::const_right(LayoutRight::px(reach)))
    };
    let mut style = vec![
        decl::position(LayoutPosition::Absolute),
        side,
        decl::simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
        decl::px_width(look.runout),
        decl::px_height(look.gauge),
    ];
    let paint = if left {
        &look.runout_left
    } else {
        &look.runout_right
    };
    style.extend(paint.as_ref().iter().cloned());
    Dom::create_div()
        .with_ids_and_classes(decl::classes(&[if left {
            RUNOUT_LEFT_CLASS
        } else {
            RUNOUT_RIGHT_CLASS
        }]))
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
}

/// The two curves of an Australis tab, the left one first, then - when the
/// look has them - the left and the right run-out: children of the selected
/// tab, which is `position: relative` so they hang off its box.
#[must_use]
pub(crate) fn australis_curves(look: &TabCurveLook) -> Vec<Dom> {
    let mut parts = vec![curve(look, true), curve(look, false)];
    if look.runout > 0.0 {
        parts.push(runout(look, true));
        parts.push(runout(look, false));
    }
    parts
}

#[cfg(test)]
mod australis_tests {
    use azul_core::{dom::SvgNodeData, svg::SvgPathElement};

    use super::*;

    fn look() -> TabCurveLook {
        TabCurveLook {
            height: 28.0,
            gauge: 2.0,
            left_fill: CssPropertyWithConditionsVec::from_const_slice(&[]),
            right_fill: CssPropertyWithConditionsVec::from_const_slice(&[]),
            metal: CssPropertyWithConditionsVec::from_const_slice(&[]),
            runout: 34.0,
            runout_left: CssPropertyWithConditionsVec::from_const_slice(&[]),
            runout_right: CssPropertyWithConditionsVec::from_const_slice(&[]),
        }
    }

    /// Beside each foot the run-out lies on the rule: a `runout` x gauge band
    /// past the curve, ending where the S leaves the rule; none when the
    /// look asks for none.
    #[test]
    fn a_run_out_lies_on_the_rule_beside_each_foot() {
        let parts = australis_curves(&look());
        assert_eq!(parts.len(), 4, "two curves, two run-outs");
        let runout_px = |node: &Dom, ty: CssPropertyType| {
            node.root
                .style
                .iter_inline_properties()
                .filter(|(p, _)| p.get_type() == ty)
                .find_map(|(p, _)| match p {
                    CssProperty::Left(v) => v.get_property().map(|l| l.inner.number.get()),
                    CssProperty::Right(v) => v.get_property().map(|r| r.inner.number.get()),
                    CssProperty::Width(v) => match v.get_property() {
                        Some(LayoutWidth::Px(px)) => Some(px.number.get()),
                        _ => None,
                    },
                    CssProperty::Height(v) => match v.get_property() {
                        Some(LayoutHeight::Px(px)) => Some(px.number.get()),
                        _ => None,
                    },
                    _ => None,
                })
        };
        for (node, class, side) in [
            (&parts[2], RUNOUT_LEFT_CLASS, CssPropertyType::Left),
            (&parts[3], RUNOUT_RIGHT_CLASS, CssPropertyType::Right),
        ] {
            assert!(node
                .root
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class)));
            assert_eq!(runout_px(node, side), Some(-(CURVE_WIDTH + 34.0)), "{class}");
            assert_eq!(runout_px(node, CssPropertyType::Width), Some(34.0));
            assert_eq!(runout_px(node, CssPropertyType::Height), Some(2.0), "the rule's gauge");
        }
        let without = TabCurveLook {
            runout: 0.0,
            ..look()
        };
        assert_eq!(australis_curves(&without).len(), 2, "no run-outs");
    }

    /// The S leaves the strip's rule and reaches the tab's top edge LEVEL -
    /// both control points share their end's height - so it runs on into
    /// both without a corner, and its centre line sits half a gauge inside
    /// the curve's box, where the 2px rule and the 2px top edge are centred.
    #[test]
    fn the_s_leaves_the_rule_and_reaches_the_top_edge_level() {
        let s = curve_s(18.0, 28.0, 2.0, true);
        assert_eq!((s.start.x, s.start.y), (0.0, 27.0), "the foot, on the rule");
        assert_eq!((s.end.x, s.end.y), (18.0, 1.0), "the top, on the top edge");
        assert_eq!(s.ctrl_1.y, s.start.y, "level as it leaves the rule");
        assert_eq!(s.ctrl_2.y, s.end.y, "level as it meets the top edge");
    }

    /// The right curve is the left one mirrored: its foot on the right, its
    /// top against the tab's side on the left.
    #[test]
    fn the_right_s_is_the_left_one_mirrored() {
        let (l, r) = (curve_s(18.0, 28.0, 2.0, true), curve_s(18.0, 28.0, 2.0, false));
        for (a, b) in [
            (l.start, r.start),
            (l.ctrl_1, r.ctrl_1),
            (l.ctrl_2, r.ctrl_2),
            (l.end, r.end),
        ] {
            assert!((a.x - (18.0 - b.x)).abs() < 1e-4, "{a:?} mirrors {b:?}");
            assert_eq!(a.y, b.y);
        }
    }

    /// Each curve is a box of its own user space hung off the tab's side,
    /// holding the face (a CLOSED path, the inside of the S) and the metal:
    /// the band the S covers at its gauge, closed too - it is FILLED, with
    /// the rolled metal the tab's head is cut from (a flat stroke has one
    /// colour). The band leaves the rule over the rule's 2px and arrives at
    /// the head over the head's 2px, so rule, S and head are one piece.
    #[test]
    fn a_curve_holds_the_face_inside_the_s_and_the_band_of_metal_the_s_is() {
        let curves = australis_curves(&look());
        let (left, right) = (&curves[0], &curves[1]);
        for (curve, class) in [(left, CURVE_LEFT_CLASS), (right, CURVE_RIGHT_CLASS)] {
            assert!(curve
                .root
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == class)));
            assert!(matches!(
                curve.root.get_svg_data(),
                Some(SvgNodeData::ViewBox { width, height, .. }) if *width == CURVE_WIDTH && *height == 28.0
            ));
            let kids = curve.children.as_ref();
            assert_eq!(kids.len(), 2, "the fill, then the metal over it");
            let Some(SvgNodeData::Path(fill)) = kids[0].root.get_svg_data() else {
                panic!("the fill is a path");
            };
            let ring = &fill.rings.as_ref()[0];
            let items = ring.items.as_ref();
            let (first, last) = (items[0], items[items.len() - 1]);
            let (SvgPathElement::Line(a), SvgPathElement::Line(z)) = (first, last) else {
                panic!("the fill starts and ends on its straight edges");
            };
            assert_eq!(z.end, a.start, "the face is closed");
            let Some(SvgNodeData::Path(metal)) = kids[1].root.get_svg_data() else {
                panic!("the metal is a path");
            };
            let band = metal.rings.as_ref()[0].items.as_ref();
            let ends: Vec<(SvgPoint, SvgPoint)> = band
                .iter()
                .map(|e| match e {
                    SvgPathElement::Line(l) => (l.start, l.end),
                    other => panic!("the band is cut in straight runs, got {other:?}"),
                })
                .collect();
            assert!(ends.len() > 8, "the band follows the S closely");
            assert!(
                ends.windows(2).all(|w| w[0].1 == w[1].0),
                "the band's outline is one run"
            );
            assert_eq!(ends[ends.len() - 1].1, ends[0].0, "the band is closed");
            let near = |p: SvgPoint, x: f32, y: f32| (p.x - x).abs() < 1e-3 && (p.y - y).abs() < 1e-3;
            let has = |x: f32, y: f32| ends.iter().any(|(p, _)| near(*p, x, y));
            let (foot_x, head_x) = if class == CURVE_LEFT_CLASS {
                (0.0, CURVE_WIDTH)
            } else {
                (CURVE_WIDTH, 0.0)
            };
            for (x, y, what) in [
                (foot_x, 28.0, "the rule's foot"),
                (foot_x, 26.0, "the rule's top"),
                (head_x, 0.0, "the head's top"),
                (head_x, 2.0, "the head's underside"),
            ] {
                assert!(has(x, y), "{class}: the band reaches {what} at ({x}, {y}): {ends:?}");
            }
        }
    }
}

const STRING_16146701490593874959: AzString = AzString::from_const_str("system:ui");
const STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_1400070954008106244_ITEMS,
        ),
    })];

const STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    })];
const STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];
const LINEAR_COLOR_STOP_1400070954008106244_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 240,
            g: 240,
            b: 240,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset_px: azul_css::props::basic::FloatValue::const_new(0),
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 229,
            g: 229,
            b: 229,
            a: 255,
        }),
    },
];

const CSS_MATCH_13824480602841492081_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-noleftborder
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::None,
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    // Dark theme, at rest: the tab's face and outline follow the desktop.
    // Declared AFTER the light values they twin (a twin before its light
    // value is dead) and BEFORE the hover rules below: a `dark_theme`
    // declaration matches in every pseudo-state, so after the dark hover
    // twins it would shadow them and a hovered tab would not light up.
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    TAB_FACE_DARK,
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    //
    // Thirteen hover rules and their thirteen dark twins, declared in the
    // theme module — `themes::flat::TAB_HOVER_STATES` — because the dark half
    // of each pair needs a palette this file cannot see. Declared here they
    // could only ever name the light-mode blue, which is how a hovered tab
    // kept its light ring and fill on a dark surface. The widths and styles
    // are part of the set on purpose: they draw back the edge a seam tab
    // (`-noleftborder` / `-norightborder`) has nulled, so the ring is whole.
    flat::TAB_HOVER_BORDER_BOTTOM_WIDTH,
    flat::TAB_HOVER_BORDER_LEFT_WIDTH,
    flat::TAB_HOVER_BORDER_RIGHT_WIDTH,
    flat::TAB_HOVER_BORDER_TOP_WIDTH,
    flat::TAB_HOVER_BORDER_BOTTOM_STYLE,
    flat::TAB_HOVER_BORDER_LEFT_STYLE,
    flat::TAB_HOVER_BORDER_RIGHT_STYLE,
    flat::TAB_HOVER_BORDER_TOP_STYLE,
    flat::TAB_HOVER_BORDER_BOTTOM_COLOR,
    flat::TAB_HOVER_BORDER_LEFT_COLOR,
    flat::TAB_HOVER_BORDER_RIGHT_COLOR,
    flat::TAB_HOVER_BORDER_TOP_COLOR,
    flat::TAB_HOVER_BG,
    flat::TAB_HOVER_BORDER_BOTTOM_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_LEFT_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_TOP_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_BOTTOM_STYLE_DARK,
    flat::TAB_HOVER_BORDER_LEFT_STYLE_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_STYLE_DARK,
    flat::TAB_HOVER_BORDER_TOP_STYLE_DARK,
    flat::TAB_HOVER_BORDER_BOTTOM_COLOR_DARK,
    flat::TAB_HOVER_BORDER_LEFT_COLOR_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_COLOR_DARK,
    flat::TAB_HOVER_BORDER_TOP_COLOR_DARK,
    flat::TAB_HOVER_BG_DARK,
];
pub(crate) const CSS_MATCH_13824480602841492081: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_13824480602841492081_PROPERTIES);

const CSS_MATCH_14575853790110873394_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(7),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(7),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(3),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(23)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    // Dark theme: the active tab's face and outline follow the desktop. Last,
    // after both of the light backgrounds this style (re)declares.
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    TAB_FACE_DARK,
];
pub(crate) const CSS_MATCH_14575853790110873394: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_14575853790110873394_PROPERTIES);

const CSS_MATCH_17290739305197504468_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-before-tabs
    CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
        LayoutWidth::Px(PixelValue::const_px(2)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
];
pub(crate) const CSS_MATCH_17290739305197504468: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_17290739305197504468_PROPERTIES);

const CSS_MATCH_18014909903571752977_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-content
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(LayoutBorderTopWidthValue::None)),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(StyleBorderTopStyleValue::None)),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(StyleBorderTopColorValue::None)),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
    // Dark theme: the panel the active tab opens onto is the desktop's window
    // surface, outlined with its separator (the top edge stays open).
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    system_palette::DARK_WINDOW_BACKGROUND,
];
pub(crate) const CSS_MATCH_18014909903571752977: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_18014909903571752977_PROPERTIES);

const CSS_MATCH_3088386549906605418_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header .__azul-native-tabs-after-tabs
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
];
pub(crate) const CSS_MATCH_3088386549906605418: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_3088386549906605418_PROPERTIES);

const CSS_MATCH_4415083954137121609_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-norightborder
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::None,
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::None,
    )),
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    // Dark theme, at rest: the tab's face and outline follow the desktop.
    // Declared AFTER the light values they twin (a twin before its light
    // value is dead) and BEFORE the hover rules below: a `dark_theme`
    // declaration matches in every pseudo-state, so after the dark hover
    // twins it would shadow them and a hovered tab would not light up.
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    TAB_FACE_DARK,
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    //
    // Thirteen hover rules and their thirteen dark twins, declared in the
    // theme module — `themes::flat::TAB_HOVER_STATES` — because the dark half
    // of each pair needs a palette this file cannot see. Declared here they
    // could only ever name the light-mode blue, which is how a hovered tab
    // kept its light ring and fill on a dark surface. The widths and styles
    // are part of the set on purpose: they draw back the edge a seam tab
    // (`-noleftborder` / `-norightborder`) has nulled, so the ring is whole.
    flat::TAB_HOVER_BORDER_BOTTOM_WIDTH,
    flat::TAB_HOVER_BORDER_LEFT_WIDTH,
    flat::TAB_HOVER_BORDER_RIGHT_WIDTH,
    flat::TAB_HOVER_BORDER_TOP_WIDTH,
    flat::TAB_HOVER_BORDER_BOTTOM_STYLE,
    flat::TAB_HOVER_BORDER_LEFT_STYLE,
    flat::TAB_HOVER_BORDER_RIGHT_STYLE,
    flat::TAB_HOVER_BORDER_TOP_STYLE,
    flat::TAB_HOVER_BORDER_BOTTOM_COLOR,
    flat::TAB_HOVER_BORDER_LEFT_COLOR,
    flat::TAB_HOVER_BORDER_RIGHT_COLOR,
    flat::TAB_HOVER_BORDER_TOP_COLOR,
    flat::TAB_HOVER_BG,
    flat::TAB_HOVER_BORDER_BOTTOM_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_LEFT_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_TOP_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_BOTTOM_STYLE_DARK,
    flat::TAB_HOVER_BORDER_LEFT_STYLE_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_STYLE_DARK,
    flat::TAB_HOVER_BORDER_TOP_STYLE_DARK,
    flat::TAB_HOVER_BORDER_BOTTOM_COLOR_DARK,
    flat::TAB_HOVER_BORDER_LEFT_COLOR_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_COLOR_DARK,
    flat::TAB_HOVER_BORDER_TOP_COLOR_DARK,
    flat::TAB_HOVER_BG_DARK,
];
pub(crate) const CSS_MATCH_4415083954137121609: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4415083954137121609_PROPERTIES);

const CSS_MATCH_4738503469417034630_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-container
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_4738503469417034630: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_4738503469417034630_PROPERTIES);

const CSS_MATCH_9988039989460234263_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header - flat's skin; the flex row (`display`,
    // `flex-direction`) is `HEADER_BASE`.
    CssPropertyWithConditions::simple(CssProperty::FontSize(StyleFontSizeValue::Exact(
        StyleFontSize {
            inner: PixelValue::const_px(11),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::FontFamily(StyleFontFamilyVecValue::Exact(
        StyleFontFamilyVec::from_const_slice(STYLE_FONT_FAMILY_8122988506401935406_ITEMS),
    ))),
];
pub(crate) const CSS_MATCH_9988039989460234263: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_9988039989460234263_PROPERTIES);

// -- NO PADDING
const CSS_MATCH_18014909903571752977_PROPERTIES_NO_PADDING: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-content
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS,
        )),
    )),
    system_palette::DARK_WINDOW_BACKGROUND,
];
pub(crate) const CSS_MATCH_18014909903571752977_NO_PADDING: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(
        CSS_MATCH_18014909903571752977_PROPERTIES_NO_PADDING,
    );

const CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-container
    CssPropertyWithConditions::simple(CssProperty::FlexGrow(LayoutFlexGrowValue::Exact(
        LayoutFlexGrow {
            inner: FloatValue::const_new(1),
        },
    ))),
];
const CSS_MATCH_4738503469417034630_NO_PADDING: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(
        CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING,
    );

// -- REGULAR_INACTIVE_TAB

const CSS_MATCH_11510695043643111367_PROPERTIES: &[CssPropertyWithConditions] = &[
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active
    CssPropertyWithConditions::simple(CssProperty::PaddingRight(LayoutPaddingRightValue::Exact(
        LayoutPaddingRight {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingLeft(LayoutPaddingLeftValue::Exact(
        LayoutPaddingLeft {
            inner: PixelValue::const_px(5),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingBottom(LayoutPaddingBottomValue::Exact(
        LayoutPaddingBottom {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::PaddingTop(LayoutPaddingTopValue::Exact(
        LayoutPaddingTop {
            inner: PixelValue::const_px(1),
        },
    ))),
    CssPropertyWithConditions::simple(CssProperty::MarginTop(LayoutMarginTopValue::Exact(
        LayoutMarginTop {
            inner: PixelValue::const_px(2),
        },
    ))),
    // .__azul-native-tabs-header p
    CssPropertyWithConditions::simple(CssProperty::TextAlign(StyleTextAlignValue::Exact(
        StyleTextAlign::Center,
    ))),
    CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
        LayoutHeight::Px(PixelValue::const_px(21)),
    ))),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomWidth(
        LayoutBorderBottomWidthValue::Exact(LayoutBorderBottomWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftWidth(
        LayoutBorderLeftWidthValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightWidth(
        LayoutBorderRightWidthValue::Exact(LayoutBorderRightWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopWidth(
        LayoutBorderTopWidthValue::Exact(LayoutBorderTopWidth {
            inner: PixelValue::const_px(1),
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomStyle(
        StyleBorderBottomStyleValue::Exact(StyleBorderBottomStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftStyle(
        StyleBorderLeftStyleValue::Exact(StyleBorderLeftStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightStyle(
        StyleBorderRightStyleValue::Exact(StyleBorderRightStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopStyle(
        StyleBorderTopStyleValue::Exact(StyleBorderTopStyle {
            inner: BorderStyle::Solid,
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderBottomColor(
        StyleBorderBottomColorValue::Exact(StyleBorderBottomColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
        StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
        StyleBorderRightColorValue::Exact(StyleBorderRightColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
        StyleBorderTopColorValue::Exact(StyleBorderTopColor {
            inner: ColorU {
                r: 172,
                g: 172,
                b: 172,
                a: 255,
            },
        }),
    )),
    CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
        StyleBackgroundContentVecValue::Exact(StyleBackgroundContentVec::from_const_slice(
            STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS,
        )),
    )),
    // Dark theme, at rest: the tab's face and outline follow the desktop.
    // Declared AFTER the light values they twin (a twin before its light
    // value is dead) and BEFORE the hover rules below: a `dark_theme`
    // declaration matches in every pseudo-state, so after the dark hover
    // twins it would shadow them and a hovered tab would not light up.
    system_palette::DARK_SEPARATOR_BORDER_BOTTOM,
    system_palette::DARK_SEPARATOR_BORDER_LEFT,
    system_palette::DARK_SEPARATOR_BORDER_RIGHT,
    system_palette::DARK_SEPARATOR_BORDER_TOP,
    TAB_FACE_DARK,
    // .__azul-native-tabs-header p.__azul-native-tabs-tab-not-active:hover
    //
    // Thirteen hover rules and their thirteen dark twins, declared in the
    // theme module — `themes::flat::TAB_HOVER_STATES` — because the dark half
    // of each pair needs a palette this file cannot see. Declared here they
    // could only ever name the light-mode blue, which is how a hovered tab
    // kept its light ring and fill on a dark surface. The widths and styles
    // are part of the set on purpose: they draw back the edge a seam tab
    // (`-noleftborder` / `-norightborder`) has nulled, so the ring is whole.
    flat::TAB_HOVER_BORDER_BOTTOM_WIDTH,
    flat::TAB_HOVER_BORDER_LEFT_WIDTH,
    flat::TAB_HOVER_BORDER_RIGHT_WIDTH,
    flat::TAB_HOVER_BORDER_TOP_WIDTH,
    flat::TAB_HOVER_BORDER_BOTTOM_STYLE,
    flat::TAB_HOVER_BORDER_LEFT_STYLE,
    flat::TAB_HOVER_BORDER_RIGHT_STYLE,
    flat::TAB_HOVER_BORDER_TOP_STYLE,
    flat::TAB_HOVER_BORDER_BOTTOM_COLOR,
    flat::TAB_HOVER_BORDER_LEFT_COLOR,
    flat::TAB_HOVER_BORDER_RIGHT_COLOR,
    flat::TAB_HOVER_BORDER_TOP_COLOR,
    flat::TAB_HOVER_BG,
    flat::TAB_HOVER_BORDER_BOTTOM_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_LEFT_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_TOP_WIDTH_DARK,
    flat::TAB_HOVER_BORDER_BOTTOM_STYLE_DARK,
    flat::TAB_HOVER_BORDER_LEFT_STYLE_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_STYLE_DARK,
    flat::TAB_HOVER_BORDER_TOP_STYLE_DARK,
    flat::TAB_HOVER_BORDER_BOTTOM_COLOR_DARK,
    flat::TAB_HOVER_BORDER_LEFT_COLOR_DARK,
    flat::TAB_HOVER_BORDER_RIGHT_COLOR_DARK,
    flat::TAB_HOVER_BORDER_TOP_COLOR_DARK,
    flat::TAB_HOVER_BG_DARK,
];
pub(crate) const CSS_MATCH_11510695043643111367: CssPropertyWithConditionsVec =
    CssPropertyWithConditionsVec::from_const_slice(CSS_MATCH_11510695043643111367_PROPERTIES);

/// Header bar for a tab widget, containing the clickable tab labels.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabHeader {
    /// Labels for each tab.
    pub tabs: StringVec,
    /// Zero-based index of the currently active tab.
    pub active_tab: usize,
    /// Optional callback invoked when a tab is clicked.
    pub on_click: OptionTabOnClick,
    /// The widget theme this tab bar is PINNED to (`with_theme`), or `None`
    /// to follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl Default for TabHeader {
    fn default() -> Self {
        Self {
            tabs: StringVec::from_const_slice(&[]),
            active_tab: 0,
            on_click: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }
}

/// State passed to the tab-click callback, indicating which tab was selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct TabHeaderState {
    /// Zero-based index of the newly selected tab.
    pub active_tab: usize,
}

/// Signature for the tab-click callback function.
pub type TabOnClickCallbackType = extern "C" fn(RefAny, CallbackInfo, TabHeaderState) -> Update;
impl_widget_callback!(
    TabOnClick,
    OptionTabOnClick,
    TabOnClickCallback,
    TabOnClickCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        TabOnClickCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: TAB_ON_CLICK_INVOKER,
    invoker_ty:     AzTabOnClickCallbackInvoker,
    thunk_fn:       az_tab_on_click_callback_thunk,
    setter_fn:      AzApp_setTabOnClickCallbackInvoker,
    from_handle_fn: AzTabOnClickCallback_createFromHostHandle,
    from_handle_byref_fn: AzTabOnClickCallback_createFromHostHandleByref,
    extra_args:     [ state: TabHeaderState ],
}

impl TabHeader {
    #[must_use]
    pub fn create(tabs: StringVec) -> Self {
        Self {
            tabs,
            active_tab: 0,
            on_click: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Pin the widget theme: the tab bar keeps this look whatever the app
    /// theme is. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    pub const fn set_active_tab(&mut self, active_tab: usize) {
        self.active_tab = active_tab;
    }

    #[must_use]
    pub const fn with_active_tab(mut self, active_tab: usize) -> Self {
        self.set_active_tab(active_tab);
        self
    }

    pub fn set_on_click<C: Into<TabOnClickCallback>>(&mut self, refany: RefAny, on_click: C) {
        self.on_click = Some(TabOnClick {
            refany,
            callback: on_click.into(),
        })
        .into();
    }

    #[must_use]
    pub fn with_on_click<C: Into<TabOnClickCallback>>(
        mut self,
        refany: RefAny,
        on_click: C,
    ) -> Self {
        self.set_on_click(refany, on_click);
        self
    }

    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::callbacks::CoreCallbackDataVec;

        // classes for previous tab
        const IDS_AND_CLASSES_5117007530891373979: &[IdOrClass] = &[
            Class(AzString::from_const_str(
                "__azul-native-tabs-tab-norightborder",
            )),
            Class(AzString::from_const_str(
                "__azul-native-tabs-tab-not-active",
            )),
        ]; // CSS_MATCH_4415083954137121609

        // classes for current tab
        const IDS_AND_CLASSES_15002865554973741556: &[IdOrClass] = &[Class(
            AzString::from_const_str(TAB_ACTIVE_CLASS_NAME),
        )];

        // classes for next tab
        const IDS_AND_CLASSES_16877793354714897051: &[IdOrClass] = &[
            Class(AzString::from_const_str(
                "__azul-native-tabs-tab-noleftborder",
            )),
            Class(AzString::from_const_str(
                "__azul-native-tabs-tab-not-active",
            )),
        ];

        // classes for default inactive tab
        const IDS_AND_CLASSES_INACTIVE: &[IdOrClass] = &[Class(AzString::from_const_str(
            TAB_NOT_ACTIVE_CLASS_NAME,
        ))];

        // The look comes from the theme module (`themes::flat::tab_header_look`
        // / `themes::flora::tab_header_look`); with no theme pinned every part
        // carries both looks, each in its `@theme(<name>)` block. The tabs,
        // their classes, datasets, click and arrow keys are the same in every
        // theme.
        let look = TabHeaderLook::of(self.theme);
        // Flora cuts its selected tab as Firefox's (`australis_curves`): the
        // curves are nodes, so they are in the tree when it is built for
        // flora - the pinned look, or the app theme the DOM is built for (a
        // theme switch rebuilds the DOM, as for every widget whose looks
        // build different trees).
        let curves = {
            use crate::widgets::themes::{flora, UiTheme};
            let theme = self.theme.into_option().unwrap_or_else(UiTheme::current);
            (theme == UiTheme::Flora).then(|| flora::tab_curves(false))
        };
        let on_click_is_some = self.on_click.is_some();
        // WAI-ARIA APG: an interactive tab list is ONE Tab stop - the active
        // tab, or the first when the index is out of range. The arrow keys
        // move (and activate) within it. A header nobody can activate stays
        // out of the Tab order entirely.
        let tab_stop =
            crate::widgets::roving::stop_index(Some(self.active_tab), self.tabs.as_ref().len());

        Dom::create_div()
            .with_css_props(look.header.clone())
            .with_ids_and_classes({
                const IDS_AND_CLASSES_6172459441955124689: &[IdOrClass] =
                    &[Class(AzString::from_const_str("__azul-native-tabs-header"))];
                match look.marker {
                    None => IdOrClassVec::from_const_slice(IDS_AND_CLASSES_6172459441955124689),
                    Some(marker) => IdOrClassVec::from_vec(vec![
                        Class(AzString::from_const_str("__azul-native-tabs-header")),
                        Class(AzString::from_const_str(marker)),
                    ]),
                }
            })
            // The header is the tab list its tabs belong to.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::PageTabList,
                ..Default::default()
            })
            .with_children({
                let mut tab_items = vec![Dom::create_div()
                    .with_css_props(look.before.clone())
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_8360971686689797550: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-tabs-before-tabs"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_8360971686689797550)
                    })];

                let dataset = TabLocalDataset {
                    tab_idx: 0,
                    on_click: self.on_click,
                };

                for (tab_idx, tab) in self.tabs.as_ref().iter().enumerate() {
                    let next_tab_is_active = self.active_tab == tab_idx.saturating_add(1);
                    let previous_tab_was_active = if self.active_tab == 0 {
                        false
                    } else {
                        self.active_tab == tab_idx.saturating_sub(1)
                    };

                    let tab_is_active = self.active_tab == tab_idx;

                    let (ids_and_classes, css_props) = if tab_is_active {
                        (IDS_AND_CLASSES_15002865554973741556, look.active.clone())
                    } else if next_tab_is_active {
                        // tab before the active tab
                        (IDS_AND_CLASSES_5117007530891373979, look.before_active.clone())
                    } else if previous_tab_was_active {
                        // tab after the active tab
                        (IDS_AND_CLASSES_16877793354714897051, look.after_active.clone())
                    } else {
                        // default inactive tab
                        (IDS_AND_CLASSES_INACTIVE, look.inactive.clone())
                    };

                    let mut dataset = dataset.clone();
                    dataset.tab_idx = tab_idx;
                    let dataset = RefAny::new(dataset);

                    let mut tab_dom = crate::widgets::widget_p_with_text(tab.clone())
                        .with_callbacks(if on_click_is_some {
                            vec![
                                CoreCallbackData {
                                    event: EventFilter::Hover(HoverEventFilter::Click),
                                    callback: CoreCallback {
                                        cb: on_tab_click as usize,
                                        ctx: azul_core::refany::OptionRefAny::None,
                                    },
                                    refany: dataset.clone(),
                                },
                                CoreCallbackData {
                                    event: EventFilter::Focus(
                                        azul_core::events::FocusEventFilter::VirtualKeyDown,
                                    ),
                                    callback: CoreCallback {
                                        cb: on_tab_key as usize,
                                        ctx: azul_core::refany::OptionRefAny::None,
                                    },
                                    refany: dataset.clone(),
                                },
                            ]
                            .into()
                        } else {
                            CoreCallbackDataVec::from_const_slice(&[])
                        })
                        .with_dataset(Some(dataset).into())
                        .with_css_props(css_props)
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(ids_and_classes))
                        // A tab, and whether it is the active one. The NAME
                        // comes from the tab's own text.
                        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                            role: azul_core::a11y::AccessibilityRole::PageTab,
                            states: if tab_is_active {
                                azul_core::a11y::AccessibilityStateVec::from_vec(vec![
                                    azul_core::a11y::AccessibilityState::Selected,
                                ])
                            } else {
                                azul_core::a11y::AccessibilityStateVec::from_const_slice(&[])
                            },
                            ..Default::default()
                        });
                    if on_click_is_some {
                        tab_dom = tab_dom.with_tab_index(crate::widgets::roving::item_tab_index(
                            tab_idx, tab_stop,
                        ));
                    }
                    // The selected tab's curves and run-outs, after its label
                    // text.
                    if tab_is_active {
                        if let Some(curves) = curves.as_ref() {
                            for curve in australis_curves(curves) {
                                tab_dom.add_child(curve);
                            }
                        }
                    }
                    tab_items.push(tab_dom);
                }

                tab_items.push(
                    Dom::create_div()
                        .with_css_props(look.after.clone())
                        .with_ids_and_classes({
                            const IDS_AND_CLASSES_11001585590816277275: &[IdOrClass] = &[Class(
                                AzString::from_const_str("__azul-native-tabs-after-tabs"),
                            )];
                            IdOrClassVec::from_const_slice(IDS_AND_CLASSES_11001585590816277275)
                        }),
                );

                tab_items.into()
            })
    }
}

/// What a theme gives a tab bar: one style per part, and the marker class
/// its header carries (`None` for flat, whose header carries none). A tab
/// next to the active one has a part of its own because the flat look joins
/// neighbouring tabs' borders into one seam; a look without seams gives the
/// three unselected parts one style.
#[derive(Debug, Clone)]
pub(crate) struct TabHeaderLook {
    /// The bar the tabs sit in.
    pub(crate) header: CssPropertyWithConditionsVec,
    /// The spacer before the first tab.
    pub(crate) before: CssPropertyWithConditionsVec,
    /// The spacer after the last tab.
    pub(crate) after: CssPropertyWithConditionsVec,
    /// The active tab.
    pub(crate) active: CssPropertyWithConditionsVec,
    /// The tab just before the active one.
    pub(crate) before_active: CssPropertyWithConditionsVec,
    /// The tab just after the active one.
    pub(crate) after_active: CssPropertyWithConditionsVec,
    /// Every other tab.
    pub(crate) inactive: CssPropertyWithConditionsVec,
    /// The theme marker class on the header, if the look has one.
    pub(crate) marker: Option<&'static str>,
}

impl TabHeaderLook {
    /// The look `theme` pins, or - unpinned - the look that follows the app
    /// theme: every part carries both themes' declarations, each theme's in
    /// its `@theme(<name>)` block (`theme_blocks::follow_props`), and the
    /// header the marker of the theme the DOM is built for.
    pub(crate) fn of(theme: crate::widgets::themes::OptionUiTheme) -> Self {
        use crate::widgets::themes::{flat, flora, theme_blocks::follow_props, UiTheme};
        match theme.into_option() {
            Some(UiTheme::Flat) => flat::tab_header_look(),
            Some(UiTheme::Flora) => flora::tab_header_look(),
            None => {
                let (a, b) = (flat::tab_header_look(), flora::tab_header_look());
                let both = |x: &CssPropertyWithConditionsVec, y: &CssPropertyWithConditionsVec| {
                    follow_props(x.as_ref(), y.as_ref())
                };
                Self {
                    header: both(&a.header, &b.header),
                    before: both(&a.before, &b.before),
                    after: both(&a.after, &b.after),
                    active: both(&a.active, &b.active),
                    before_active: both(&a.before_active, &b.before_active),
                    after_active: both(&a.after_active, &b.after_active),
                    inactive: both(&a.inactive, &b.inactive),
                    marker: match UiTheme::current() {
                        UiTheme::Flat => a.marker,
                        UiTheme::Flora => b.marker,
                    },
                }
            }
        }
    }
}

/// What a theme gives a tab panel: its style with and without the default
/// padding, and the marker class it carries (`None` for flat).
#[derive(Debug, Clone)]
pub(crate) struct TabContentLook {
    /// The panel with the default padding.
    pub(crate) padded: CssPropertyWithConditionsVec,
    /// The panel without it.
    pub(crate) unpadded: CssPropertyWithConditionsVec,
    /// The theme marker class on the panel, if the look has one.
    pub(crate) marker: Option<&'static str>,
}

impl TabContentLook {
    /// The look `theme` pins, or - unpinned - both looks in one
    /// (`theme_blocks::follow_props`), marked for the theme the DOM is built
    /// for. The panel's content is the caller's and is never cloned.
    pub(crate) fn of(theme: crate::widgets::themes::OptionUiTheme) -> Self {
        use crate::widgets::themes::{flat, flora, theme_blocks::follow_props, UiTheme};
        match theme.into_option() {
            Some(UiTheme::Flat) => flat::tab_content_look(),
            Some(UiTheme::Flora) => flora::tab_content_look(),
            None => {
                let (a, b) = (flat::tab_content_look(), flora::tab_content_look());
                Self {
                    padded: follow_props(a.padded.as_ref(), b.padded.as_ref()),
                    unpadded: follow_props(a.unpadded.as_ref(), b.unpadded.as_ref()),
                    marker: match UiTheme::current() {
                        UiTheme::Flat => a.marker,
                        UiTheme::Flora => b.marker,
                    },
                }
            }
        }
    }
}

/// Content panel displayed beneath the active tab in a tab widget.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabContent {
    /// The DOM subtree shown as the tab's content area.
    pub content: Dom,
    /// Whether the content area includes default padding.
    pub has_padding: bool,
    /// The widget theme this panel is PINNED to (`with_theme`), or `None` to
    /// follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
}

impl Default for TabContent {
    fn default() -> Self {
        Self {
            content: Dom::create_div(),
            has_padding: true,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }
}

impl TabContent {
    #[must_use]
    pub const fn new(content: Dom) -> Self {
        Self {
            content,
            has_padding: true,
            theme: crate::widgets::themes::OptionUiTheme::None,
        }
    }

    /// Pin the widget theme: the panel keeps this look whatever the app
    /// theme is. Unset (`None`), it follows the app theme.
    pub const fn set_theme(&mut self, theme: crate::widgets::themes::UiTheme) {
        self.theme = crate::widgets::themes::OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[must_use]
    pub const fn with_theme(mut self, theme: crate::widgets::themes::UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut default = Self::default();
        core::mem::swap(&mut default, self);
        default
    }

    #[must_use]
    pub const fn with_padding(mut self, padding: bool) -> Self {
        self.set_padding(padding);
        self
    }

    pub const fn set_padding(&mut self, padding: bool) {
        self.has_padding = padding;
    }

    #[must_use]
    pub fn dom(self) -> Dom {
        const IDS_AND_CLASSES_2989815829020816222: &[IdOrClass] = &[Class(
            AzString::from_const_str("__azul-native-tabs-content"),
        )];

        let look = TabContentLook::of(self.theme);
        let tab_content_css_style = if self.has_padding {
            look.padded
        } else {
            look.unpadded
        };
        let panel = Dom::create_div().with_css_props(tab_content_css_style);
        let panel = match look.marker {
            None => panel,
            Some(marker) => panel.with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
                AzString::from_const_str(marker),
            )])),
        };

        panel
            .with_children(DomVec::from_vec(vec![Dom::create_div()
                .with_ids_and_classes(IdOrClassVec::from_const_slice(
                    IDS_AND_CLASSES_2989815829020816222,
                ))
                .with_children(DomVec::from_vec(vec![self.content]))]))
    }
}

#[derive(Clone)]
struct TabLocalDataset {
    tab_idx: usize,
    on_click: OptionTabOnClick,
}

extern "C" fn on_tab_click(mut refany: RefAny, info: CallbackInfo) -> Update {
    fn select_new_tab_inner(mut refany: RefAny, info: &CallbackInfo) -> Option<Update> {
        let mut tab_local_dataset = refany.downcast_mut::<TabLocalDataset>()?;
        let tab_idx = tab_local_dataset.tab_idx;
        let tab_header_state = TabHeaderState {
            active_tab: tab_idx,
        };

        let result = {
            // rustc doesn't understand the borrowing lifetime here
            let tab_local_dataset = &mut *tab_local_dataset;
            let onclick = &mut tab_local_dataset.on_click;

            match onclick.as_mut() {
                Some(TabOnClick { callback, refany }) => {
                    callback.invoke(refany.clone(), *info, tab_header_state)
                }
                None => Update::DoNothing,
            }
        };

        Some(result)
    }

    select_new_tab_inner(refany, &info).unwrap_or(Update::RefreshDom)
}

/// Class of the active tab, and of every other tab: between them they name
/// exactly the tabs among the header's children (the two spacers carry
/// neither), which is how the arrow-key handler finds its siblings.
const TAB_ACTIVE_CLASS_NAME: &str = "__azul-native-tabs-tab-active";
const TAB_NOT_ACTIVE_CLASS_NAME: &str = "__azul-native-tabs-tab-not-active";

/// Arrow keys on the focused tab (WAI-ARIA APG tabs, automatic activation):
/// Left and Right move to the previous / next tab, wrapping at the ends; Home
/// and End jump to the first / last. The target tab is focused, becomes the
/// tab list's one Tab stop, and is ACTIVATED - reported through `on_click`
/// exactly as a click on it would be, so the app switches the panel. The key's
/// default action is cancelled. Up/Down, every other key and any key held with
/// Alt, Ctrl, Cmd or Shift keep their default.
extern "C" fn on_tab_key(mut refany: RefAny, mut info: CallbackInfo) -> Update {
    use azul_core::window::VirtualKeyCode as K;

    use crate::widgets::roving::{self, Step};

    let step = match roving::plain_key(&info.get_current_keyboard_state()) {
        Some(K::Left) => Step::Previous,
        Some(K::Right) => Step::Next,
        Some(K::Home) => Step::First,
        Some(K::End) => Step::Last,
        _ => return Update::DoNothing,
    };

    let focused = info.get_hit_node();
    let Some(header) = info.get_parent(focused) else {
        return Update::DoNothing;
    };
    let tabs: Vec<azul_core::dom::DomNodeId> = roving::children_of(&info, header)
        .into_iter()
        .filter(|n| {
            roving::has_class(&info, *n, TAB_ACTIVE_CLASS_NAME)
                || roving::has_class(&info, *n, TAB_NOT_ACTIVE_CLASS_NAME)
        })
        .collect();
    let Some(current) = tabs.iter().position(|n| *n == focused) else {
        return Update::DoNothing;
    };
    let Some(target) = roving::step_target(current, tabs.len(), step, true) else {
        return Update::DoNothing;
    };
    // Not our dataset (or already borrowed): leave the key alone.
    if refany.downcast_ref::<TabLocalDataset>().is_none() {
        return Update::DoNothing;
    }

    info.prevent_default();
    // Moved BEFORE the app hears the activation, so a focus it asks for wins.
    roving::move_stop(&mut info, &tabs, target);
    // The activated tab is the selected one from now on - announced live,
    // before the app's rebuild (if it rebuilds at all) publishes it.
    roving::announce_chosen(
        &mut info,
        &tabs,
        target,
        azul_core::a11y::AccessibilityState::Selected,
        None,
    );

    let Some(mut dataset) = refany.downcast_mut::<TabLocalDataset>() else {
        return Update::DoNothing;
    };
    // The tabs are the header's tab children in order, and every dataset
    // carries its tab's position - so the target's position IS its index.
    let state = TabHeaderState { active_tab: target };
    let dataset = &mut *dataset;
    match dataset.on_click.as_mut() {
        Some(TabOnClick { callback, refany }) => callback.invoke(refany.clone(), info, state),
        None => Update::DoNothing,
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId, NodeType, TabIndex},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::{
        dynamic_selector::{DynamicSelector, PseudoStateType, ThemeCondition},
        system::SystemStyle,
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        widgets::{roving::test_support as rv, theme_probe, themes::UiTheme},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    const CLASS_ACTIVE: &str = "__azul-native-tabs-tab-active";
    const CLASS_NOT_ACTIVE: &str = "__azul-native-tabs-tab-not-active";
    const CLASS_NO_LEFT: &str = "__azul-native-tabs-tab-noleftborder";
    const CLASS_NO_RIGHT: &str = "__azul-native-tabs-tab-norightborder";
    const CLASS_HEADER: &str = "__azul-native-tabs-header";
    const CLASS_BEFORE: &str = "__azul-native-tabs-before-tabs";
    const CLASS_AFTER: &str = "__azul-native-tabs-after-tabs";
    const CLASS_CONTENT: &str = "__azul-native-tabs-content";

    fn strings(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
    }

    fn numbered_labels(n: usize) -> StringVec {
        StringVec::from_vec((0..n).map(|i| AzString::from(format!("tab {i}"))).collect())
    }

    /// The text of a text node, looking through the `<p>` block wrapper the
    /// label convention mandates (`p > text`).
    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(s) => Some(s.as_ref().as_str()),
            NodeType::P => match node.children.as_ref() {
                [only] => match only.root.get_node_type() {
                    NodeType::Text(s) => Some(s.as_ref().as_str()),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn classes(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    fn class_strs(node: &Dom) -> Vec<&'static str> {
        // The widget only ever attaches the eight `&'static str` classes above;
        // map back to them so the tests can compare against string literals.
        classes(node)
            .into_iter()
            .map(|c| {
                [
                    CLASS_ACTIVE,
                    CLASS_NOT_ACTIVE,
                    CLASS_NO_LEFT,
                    CLASS_NO_RIGHT,
                    CLASS_HEADER,
                    CLASS_BEFORE,
                    CLASS_AFTER,
                    CLASS_CONTENT,
                ]
                .into_iter()
                .find(|known| *known == c)
                .unwrap_or_else(|| panic!("tabs.rs emitted an unknown class: {c}"))
            })
            .collect()
    }

    /// A style vec as `(property, condition-count)` pairs in declaration order —
    /// the exact shape `Css::from(CssPropertyWithConditionsVec)` preserves.
    fn declared(v: &CssPropertyWithConditionsVec) -> Vec<(CssProperty, usize)> {
        v.as_ref()
            .iter()
            .map(|p| (p.property.clone(), p.apply_if.as_ref().len()))
            .collect()
    }

    /// The same view, read back off a rendered node's inline `Css`.
    fn inline_declared(node: &Dom) -> Vec<(CssProperty, usize)> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, conds)| (p.clone(), conds.as_ref().len()))
            .collect()
    }

    /// Every declaration in `v` matching `pred`, in declaration order.
    fn decls_where(
        v: &CssPropertyWithConditionsVec,
        pred: fn(&CssProperty) -> bool,
    ) -> Vec<CssProperty> {
        v.as_ref()
            .iter()
            .filter(|p| pred(&p.property))
            .map(|p| p.property.clone())
            .collect()
    }

    /// Same, but skipping the `:hover` block — these styles declare the border
    /// properties once for hover and again unconditionally, so a raw filter would
    /// mix the two sets.
    fn plain_decls_where(
        v: &CssPropertyWithConditionsVec,
        pred: fn(&CssProperty) -> bool,
    ) -> Vec<CssProperty> {
        v.as_ref()
            .iter()
            .filter(|p| p.apply_if.as_ref().is_empty() && pred(&p.property))
            .map(|p| p.property.clone())
            .collect()
    }

    /// `:hover` alone — the light half of a hover pair.
    fn is_hover_only(conds: &[DynamicSelector]) -> bool {
        matches!(
            conds,
            [DynamicSelector::PseudoState(PseudoStateType::Hover)]
        )
    }

    /// `:hover` AND dark — the twin `themes::flat` pairs with every light rule.
    fn is_dark_hover(conds: &[DynamicSelector]) -> bool {
        conds.len() == 2
            && conds
                .iter()
                .any(|c| matches!(c, DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)))
            && conds
                .iter()
                .any(|c| matches!(c, DynamicSelector::PseudoState(PseudoStateType::Hover)))
    }

    /// Dark alone, no pseudo-state — a resting colour's dark-theme twin.
    fn is_dark_resting(conds: &[DynamicSelector]) -> bool {
        matches!(conds, [DynamicSelector::Mode(azul_css::dynamic_selector::ModeCondition::Dark)])
    }

    /// The style vec the widget must pair with a given class combination: the
    /// flat look's part (R5: the widget's `TAB_BASE`, then the const skin).
    fn style_for_classes(cls: &[&str]) -> CssPropertyWithConditionsVec {
        let cls = cls.to_vec();
        let look = flat::tab_header_look();
        if cls == [CLASS_ACTIVE] {
            look.active
        } else if cls == [CLASS_NO_RIGHT, CLASS_NOT_ACTIVE] {
            look.before_active
        } else if cls == [CLASS_NO_LEFT, CLASS_NOT_ACTIVE] {
            look.after_active
        } else if cls == [CLASS_NOT_ACTIVE] {
            look.inactive
        } else {
            panic!("unexpected class combination on a tab node: {cls:?}");
        }
    }

    /// The dataset `RefAny` a rendered tab node carries (cloned, so the caller
    /// can `downcast_*` it without borrowing the Dom mutably).
    fn dataset_of(node: &Dom) -> RefAny {
        node.root
            .get_dataset()
            .expect("every tab node carries a TabLocalDataset")
            .clone()
    }

    fn tab_idx_of(node: &Dom) -> usize {
        let mut ds = dataset_of(node);
        let tab_idx = ds
            .downcast_ref::<TabLocalDataset>()
            .expect("the dataset must be a TabLocalDataset")
            .tab_idx;
        tab_idx
    }

    /// A `RefAny` payload recording every state a user `on_click` observes.
    #[derive(Default)]
    struct ClickLog {
        seen: Vec<TabHeaderState>,
    }

    extern "C" fn record_click(
        mut refany: RefAny,
        _: CallbackInfo,
        state: TabHeaderState,
    ) -> Update {
        if let Some(mut log) = refany.downcast_mut::<ClickLog>() {
            log.seen.push(state);
        }
        Update::RefreshDom
    }

    extern "C" fn click_do_nothing(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::DoNothing
    }

    extern "C" fn click_refresh_all(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::RefreshDomAllWindows
    }

    /// Forces the `fn`-item -> `fn`-pointer coercion the `Into` bound needs.
    fn cb(f: TabOnClickCallbackType) -> TabOnClickCallback {
        f.into()
    }

    fn logged(refany: &mut RefAny) -> Vec<TabHeaderState> {
        refany
            .downcast_ref::<ClickLog>()
            .expect("payload must still be a ClickLog")
            .seen
            .clone()
    }

    fn dataset(tab_idx: usize, on_click: OptionTabOnClick) -> RefAny {
        RefAny::new(TabLocalDataset { tab_idx, on_click })
    }

    fn click_handler(refany: RefAny, callback: TabOnClickCallbackType) -> OptionTabOnClick {
        Some(TabOnClick {
            refany,
            callback: cb(callback),
        })
        .into()
    }

    /// Invokes `on_tab_click` with a minimal `CallbackInfo` (the handler never
    /// touches the layout window — it only downcasts its own dataset — so an
    /// empty `LayoutWindow` and node 0 as the hit node are enough).
    /// Returns the `Update` plus every recorded `CallbackChange`.
    fn run_click(data: RefAny) -> (Update, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let update = on_tab_click(data, info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (update, recorded)
    }

    // ==================================================================
    // TabHeader::create
    // ==================================================================

    #[test]
    fn create_keeps_the_label_vec_verbatim_and_starts_inert() {
        for labels in [
            strings(&[]),
            strings(&["only"]),
            strings(&["a", "b", "c"]),
            numbered_labels(1000),
        ] {
            let expected: Vec<AzString> = labels.as_ref().to_vec();
            let header = TabHeader::create(labels);

            assert_eq!(
                header.tabs.as_ref(),
                expected.as_slice(),
                "create must not reorder, drop or rewrite labels"
            );
            assert_eq!(header.active_tab, 0, "a fresh header selects the first tab");
            assert!(
                header.on_click.is_none(),
                "create must not install a callback"
            );
        }
    }

    #[test]
    fn create_accepts_an_empty_label_vec_and_still_renders_the_spacers() {
        // Zero tabs is the degenerate case: the loop body never runs, but the
        // before/after spacers must still be emitted or the header collapses.
        let dom = TabHeader::create(strings(&[])).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 2, "an empty header is just the two spacers");
        assert_eq!(class_strs(&children[0]), vec![CLASS_BEFORE]);
        assert_eq!(class_strs(&children[1]), vec![CLASS_AFTER]);
    }

    #[test]
    fn create_round_trips_pathological_labels_through_the_dom() {
        // The label is only ever cloned into a `NodeType::Text`, never parsed,
        // trimmed or NUL-terminated — so every byte must survive verbatim.
        let long = "\u{e9}".repeat(50_000);
        let pathological: Vec<String> = vec![
            String::new(),
            " ".to_string(),
            "\0embedded\0nul\0".to_string(),
            "\u{1F600}\u{1F3F4}\u{E0067}\u{E007F}".to_string(), // emoji + tag sequence
            "a\u{0301}\u{0327}\u{0328}".to_string(),            // stacked combining marks
            "\u{202E}reversed\u{202C}".to_string(),             // RTL override
            "\r\n\t".to_string(),
            "\u{FFFD}\u{FEFF}".to_string(), // replacement char + BOM
            long.clone(),
        ];

        let labels = StringVec::from_vec(
            pathological
                .iter()
                .map(|s| AzString::from(s.clone()))
                .collect(),
        );
        let dom = TabHeader::create(labels).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), pathological.len() + 2);
        for (i, expected) in pathological.iter().enumerate() {
            assert_eq!(
                text_of(&children[i + 1]),
                Some(expected.as_str()),
                "label {i} did not round-trip through the DOM"
            );
        }
    }

    // ==================================================================
    // TabHeader::set_active_tab / with_active_tab  (numeric)
    // ==================================================================

    #[test]
    fn set_active_tab_stores_any_usize_without_clamping_or_panicking() {
        // `active_tab` is an unsigned index with no documented upper bound and
        // no relation to `tabs.len()` — there is no signed path to test, so the
        // adversarial values are 0, the wrap-around of 0 and the two extremes.
        let extremes = [
            0usize,
            1,
            2,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
            0usize.wrapping_sub(1),
        ];

        for value in extremes {
            let mut header = TabHeader::create(strings(&["a", "b", "c"]));
            header.set_active_tab(value);
            assert_eq!(
                header.active_tab, value,
                "set_active_tab must store the index verbatim (no clamp to len)"
            );

            // Idempotent + last-write-wins.
            header.set_active_tab(value);
            assert_eq!(header.active_tab, value);
            header.set_active_tab(0);
            assert_eq!(header.active_tab, 0, "the last write must win");
        }
    }

    #[test]
    fn set_active_tab_touches_nothing_but_the_index() {
        let mut header = TabHeader::create(strings(&["a", "b"]));
        header.set_on_click(RefAny::new(ClickLog::default()), cb(record_click));

        header.set_active_tab(usize::MAX);

        assert_eq!(
            header.tabs.as_ref(),
            strings(&["a", "b"]).as_ref(),
            "the labels must be untouched"
        );
        assert!(
            header.on_click.is_some(),
            "the callback must survive a selection change"
        );
    }

    #[test]
    fn with_active_tab_agrees_with_set_active_tab_at_every_extreme() {
        for value in [0usize, 1, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let built = TabHeader::create(strings(&["a", "b"])).with_active_tab(value);
            let mut mutated = TabHeader::create(strings(&["a", "b"]));
            mutated.set_active_tab(value);

            assert_eq!(built.active_tab, mutated.active_tab);
            assert_eq!(built.active_tab, value);
            assert_eq!(built.tabs.as_ref(), mutated.tabs.as_ref());
            assert!(built.on_click.is_none() && mutated.on_click.is_none());
        }
    }

    #[test]
    fn with_active_tab_chains_last_wins() {
        let header = TabHeader::create(strings(&["a"]))
            .with_active_tab(usize::MAX)
            .with_active_tab(7)
            .with_active_tab(0);
        assert_eq!(header.active_tab, 0);
    }

    #[test]
    fn dom_never_marks_a_tab_active_when_the_index_is_out_of_range() {
        // An out-of-range selection must degrade to "nothing selected" rather
        // than panicking or wrapping onto some other tab.
        let n = 4usize;
        for active in [n, n + 1, n + 2, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let dom = TabHeader::create(numbered_labels(n))
                .with_active_tab(active)
                .dom();
            let children = dom.children.as_ref();
            assert_eq!(children.len(), n + 2, "active={active}: wrong child count");

            for (i, node) in children[1..=n].iter().enumerate() {
                let cls = class_strs(node);
                assert!(
                    !cls.contains(&CLASS_ACTIVE),
                    "active={active}: tab {i} must not be styled active"
                );
                assert!(
                    cls.contains(&CLASS_NOT_ACTIVE),
                    "active={active}: tab {i} must be styled inactive"
                );
            }
        }
    }

    #[test]
    fn dom_at_usize_max_gives_every_tab_the_plain_inactive_style() {
        // `tab_idx.saturating_add(1)` / `saturating_sub(1)` must not wrap into a
        // false neighbour match at the extreme.
        // Pinned to flat: the comparison is with flat's const style vecs,
        // which an unpinned bar carries inside its `@theme(flat)` block.
        let n = 5usize;
        let dom = TabHeader::create(numbered_labels(n))
            .with_active_tab(usize::MAX)
            .with_theme(UiTheme::Flat)
            .dom();

        for (i, node) in dom.children.as_ref()[1..=n].iter().enumerate() {
            assert_eq!(
                class_strs(node),
                vec![CLASS_NOT_ACTIVE],
                "tab {i} must carry the plain inactive class only"
            );
            assert_eq!(
                inline_declared(node),
                declared(&flat::tab_header_look().inactive),
                "tab {i} must carry the plain inactive style"
            );
        }
    }

    #[test]
    fn dom_one_past_the_end_still_seams_the_last_tab() {
        // PINNED QUIRK: with `active_tab == tabs.len()` no tab is active, yet the
        // *last* tab still matches `active_tab == tab_idx + 1` and loses its right
        // border — a visible seam against the after-tabs spacer. Deliberately
        // pinned: if the widget starts range-checking `active_tab`, this flips.
        let n = 3usize;
        let dom = TabHeader::create(numbered_labels(n))
            .with_active_tab(n)
            .dom();
        let children = dom.children.as_ref();

        assert_eq!(
            class_strs(&children[n]),
            vec![CLASS_NO_RIGHT, CLASS_NOT_ACTIVE],
            "the last tab is styled as if the (non-existent) next tab were active"
        );
        for (i, node) in children[1..n].iter().enumerate() {
            assert_eq!(class_strs(node), vec![CLASS_NOT_ACTIVE], "tab {i}");
        }
    }

    // ==================================================================
    // TabHeader::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_moves_the_state_out_and_leaves_a_default() {
        let mut header = TabHeader::create(strings(&["a", "b", "c"])).with_active_tab(2);
        header.set_on_click(RefAny::new(ClickLog::default()), cb(record_click));

        let taken = header.swap_with_default();

        assert_eq!(taken.tabs.as_ref(), strings(&["a", "b", "c"]).as_ref());
        assert_eq!(taken.active_tab, 2);
        assert!(
            taken.on_click.is_some(),
            "the callback moves out with the state"
        );

        assert!(
            header.tabs.as_ref().is_empty(),
            "the husk must have no tabs"
        );
        assert_eq!(header.active_tab, 0);
        assert!(
            header.on_click.is_none(),
            "the husk must not keep a live callback"
        );
    }

    #[test]
    fn swap_with_default_twice_yields_a_default_the_second_time() {
        let mut header = TabHeader::create(strings(&["a"])).with_active_tab(usize::MAX);
        let first = header.swap_with_default();
        let second = header.swap_with_default();

        assert_eq!(first.active_tab, usize::MAX);
        assert_eq!(first.tabs.as_ref().len(), 1);
        assert_eq!(second.active_tab, 0);
        assert!(second.tabs.as_ref().is_empty());
    }

    #[test]
    fn swap_with_default_releases_the_callback_payload_when_dropped() {
        let user = RefAny::new(ClickLog::default());
        let mut header = TabHeader::create(strings(&["a"]));
        header.set_on_click(user.clone(), cb(record_click));
        assert_eq!(user.get_ref_count(), 2);

        drop(header.swap_with_default());
        assert_eq!(
            user.get_ref_count(),
            1,
            "the swapped-out header must drop its payload clone"
        );
    }

    // ==================================================================
    // TabHeader::set_on_click / with_on_click
    // ==================================================================

    #[test]
    fn with_on_click_installs_the_callback_and_touches_nothing_else() {
        let before = TabHeader::create(strings(&["a", "b"])).with_active_tab(1);
        let mut after = TabHeader::create(strings(&["a", "b"]))
            .with_active_tab(1)
            .with_on_click(RefAny::new(ClickLog::default()), cb(record_click));

        assert_eq!(after.tabs.as_ref(), before.tabs.as_ref());
        assert_eq!(after.active_tab, before.active_tab);

        let installed = after
            .on_click
            .as_mut()
            .expect("with_on_click must install Some(..)");
        assert_eq!(installed.callback.cb as usize, record_click as usize);
        assert!(
            installed.refany.downcast_ref::<ClickLog>().is_some(),
            "the payload must be stored as handed in"
        );
    }

    #[test]
    fn set_on_click_overwrites_the_previous_callback_and_payload() {
        let mut header = TabHeader::create(strings(&["a"]));
        header.set_on_click(RefAny::new(ClickLog::default()), cb(record_click));
        header.set_on_click(RefAny::new(42u32), cb(click_do_nothing));

        let installed = header
            .on_click
            .as_mut()
            .expect("still Some after overwrite");
        assert_eq!(
            installed.callback.cb as usize, click_do_nothing as usize,
            "the last set_on_click must win"
        );
        assert_eq!(
            installed.refany.downcast_ref::<u32>().map(|v| *v),
            Some(42),
            "the payload must be replaced together with the fn pointer"
        );
        assert!(
            installed.refany.downcast_ref::<ClickLog>().is_none(),
            "the stale payload must be gone"
        );
    }

    #[test]
    fn set_on_click_drops_the_previous_payload() {
        let first = RefAny::new(ClickLog::default());
        let second = RefAny::new(ClickLog::default());
        let mut header = TabHeader::create(strings(&["a"]));

        header.set_on_click(first.clone(), cb(record_click));
        assert_eq!(first.get_ref_count(), 2);
        header.set_on_click(second.clone(), cb(record_click));

        assert_eq!(
            first.get_ref_count(),
            1,
            "overwriting the callback must release the old payload"
        );
        assert_eq!(second.get_ref_count(), 2);
    }

    #[test]
    fn generic_callback_conversion_round_trips_the_fn_pointer() {
        // The FFI path (`From<Callback>`) transmutes the fn pointer; a corrupted
        // value would be an unconditional jump into garbage at click time.
        // (Never invoked here.)
        let raw = record_click as usize;
        let generic = Callback {
            cb: unsafe { core::mem::transmute::<usize, crate::callbacks::CallbackType>(raw) },
            ctx: OptionRefAny::None,
        };
        let converted: TabOnClickCallback = generic.into();
        assert_eq!(converted.cb as usize, raw);
    }

    // ==================================================================
    // TabHeader::dom — structure
    // ==================================================================

    #[test]
    fn dom_is_a_header_div_wrapping_spacer_tabs_spacer() {
        for n in [0usize, 1, 2, 3, 17] {
            // Pinned to flat: the styles compared are flat's parts (the
            // widget's base, then flat's const vecs).
            let dom = TabHeader::create(numbered_labels(n))
                .with_theme(UiTheme::Flat)
                .dom();
            let look = flat::tab_header_look();

            assert_eq!(class_strs(&dom), vec![CLASS_HEADER]);
            assert_eq!(inline_declared(&dom), declared(&look.header));
            assert!(
                matches!(dom.root.get_node_type(), NodeType::Div),
                "the header itself is a plain div"
            );

            let children = dom.children.as_ref();
            assert_eq!(children.len(), n + 2, "n={n}: spacer + {n} tabs + spacer");

            assert_eq!(class_strs(&children[0]), vec![CLASS_BEFORE]);
            assert_eq!(inline_declared(&children[0]), declared(&look.before));
            assert_eq!(class_strs(&children[n + 1]), vec![CLASS_AFTER]);
            assert_eq!(inline_declared(&children[n + 1]), declared(&look.after));

            for (i, node) in children[1..=n].iter().enumerate() {
                assert_eq!(
                    text_of(node),
                    Some(format!("tab {i}").as_str()),
                    "n={n}: tab {i} sits at sibling position {}",
                    i + 1
                );
            }
        }
    }

    #[test]
    fn dom_marks_exactly_one_tab_active_for_every_in_range_index() {
        let n = 6usize;
        for active in 0..n {
            let dom = TabHeader::create(numbered_labels(n))
                .with_active_tab(active)
                .dom();
            let children = dom.children.as_ref();

            let active_nodes: Vec<usize> = children[1..=n]
                .iter()
                .enumerate()
                .filter(|(_, node)| class_strs(node).contains(&CLASS_ACTIVE))
                .map(|(i, _)| i)
                .collect();
            assert_eq!(
                active_nodes,
                vec![active],
                "exactly the selected tab must carry the active class"
            );

            for (i, node) in children[1..=n].iter().enumerate() {
                let cls = class_strs(node);
                assert_eq!(
                    cls.contains(&CLASS_ACTIVE),
                    !cls.contains(&CLASS_NOT_ACTIVE),
                    "active={active}, tab {i}: active/not-active must be exclusive"
                );
                assert!(
                    !(cls.contains(&CLASS_NO_LEFT) && cls.contains(&CLASS_NO_RIGHT)),
                    "active={active}, tab {i}: a tab cannot drop both side borders"
                );
                assert!(
                    !(cls.contains(&CLASS_ACTIVE)
                        && (cls.contains(&CLASS_NO_LEFT) || cls.contains(&CLASS_NO_RIGHT))),
                    "active={active}, tab {i}: the active tab keeps both side borders"
                );
            }
        }
    }

    #[test]
    fn dom_gives_the_neighbours_of_the_active_tab_their_seam_classes() {
        // active = 2 of 5: tab 1 loses its right border, tab 3 its left one.
        let dom = TabHeader::create(numbered_labels(5))
            .with_active_tab(2)
            .dom();
        let children = dom.children.as_ref();

        assert_eq!(class_strs(&children[1]), vec![CLASS_NOT_ACTIVE], "tab 0");
        assert_eq!(
            class_strs(&children[2]),
            vec![CLASS_NO_RIGHT, CLASS_NOT_ACTIVE],
            "tab 1 sits before the active tab"
        );
        assert_eq!(class_strs(&children[3]), vec![CLASS_ACTIVE], "tab 2");
        assert_eq!(
            class_strs(&children[4]),
            vec![CLASS_NO_LEFT, CLASS_NOT_ACTIVE],
            "tab 3 sits after the active tab"
        );
        assert_eq!(class_strs(&children[5]), vec![CLASS_NOT_ACTIVE], "tab 4");
    }

    #[test]
    fn dom_first_tab_active_leaves_the_second_tab_with_a_left_border() {
        // PINNED BUG: `previous_tab_was_active` short-circuits on
        // `self.active_tab == 0`, so for the *default* selection (tab 0) the tab
        // right after the active one never gets `-noleftborder` — it draws a left
        // border straight against the active tab's right edge. Every other
        // selection (see the test above) does emit the class. Pinned as-is so the
        // fix flips this test loudly.
        let dom = TabHeader::create(numbered_labels(4))
            .with_active_tab(0)
            .dom();
        let children = dom.children.as_ref();

        assert_eq!(class_strs(&children[1]), vec![CLASS_ACTIVE], "tab 0");
        assert_eq!(
            class_strs(&children[2]),
            vec![CLASS_NOT_ACTIVE],
            "tab 1 should be [-noleftborder, -not-active] like the active=1..n case"
        );
        assert_eq!(class_strs(&children[3]), vec![CLASS_NOT_ACTIVE], "tab 2");

        // The symmetric neighbour (active = 1) *does* get the class, which is what
        // makes the case above an inconsistency rather than a design choice.
        let dom = TabHeader::create(numbered_labels(4))
            .with_active_tab(1)
            .dom();
        assert_eq!(
            class_strs(&dom.children.as_ref()[3]),
            vec![CLASS_NO_LEFT, CLASS_NOT_ACTIVE],
            "tab 2 after active tab 1"
        );
    }

    #[test]
    fn dom_pairs_every_tab_style_with_the_classes_it_advertises() {
        // A swapped class/style pair (e.g. the "noleftborder" node getting the
        // style that nulls the *right* border) would be invisible in a class-only
        // check, so compare the rendered inline style against the vec the class
        // combination demands.
        let n = 6usize;
        for active in [0usize, 1, 2, n - 1, n, usize::MAX] {
            // Pinned to flat: the styles compared are flat's const vecs.
            let dom = TabHeader::create(numbered_labels(n))
                .with_active_tab(active)
                .with_theme(UiTheme::Flat)
                .dom();
            for (i, node) in dom.children.as_ref()[1..=n].iter().enumerate() {
                let cls = class_strs(node);
                assert_eq!(
                    inline_declared(node),
                    declared(&style_for_classes(&cls)),
                    "active={active}, tab {i}: style does not match classes {cls:?}"
                );
            }
        }
    }

    #[test]
    fn neighbour_styles_null_the_border_on_the_side_their_class_names_claim() {
        let no_left = plain_decls_where(&CSS_MATCH_13824480602841492081, |p| {
            matches!(p, CssProperty::BorderLeftWidth(_))
        });
        let no_right = plain_decls_where(&CSS_MATCH_4415083954137121609, |p| {
            matches!(p, CssProperty::BorderRightWidth(_))
        });

        assert_eq!(
            no_left.first(),
            Some(&CssProperty::BorderLeftWidth(
                LayoutBorderLeftWidthValue::None
            )),
            "the -noleftborder style must null the LEFT border first"
        );
        assert_eq!(
            no_right.first(),
            Some(&CssProperty::BorderRightWidth(
                LayoutBorderRightWidthValue::None
            )),
            "the -norightborder style must null the RIGHT border first"
        );

        // ...and must not null the opposite side.
        assert!(
            !decls_where(&CSS_MATCH_13824480602841492081, |p| matches!(
                p,
                CssProperty::BorderRightWidth(_)
            ))
            .contains(&CssProperty::BorderRightWidth(
                LayoutBorderRightWidthValue::None
            )),
            "the -noleftborder style must keep the right border"
        );
        assert!(
            !decls_where(&CSS_MATCH_4415083954137121609, |p| matches!(
                p,
                CssProperty::BorderLeftWidth(_)
            ))
            .contains(&CssProperty::BorderLeftWidth(
                LayoutBorderLeftWidthValue::None
            )),
            "the -norightborder style must keep the left border"
        );

        // The plain inactive style (no active neighbour) keeps both side borders:
        // exactly one unconditional declaration per side, and neither is `None`.
        let plain_left = plain_decls_where(&CSS_MATCH_11510695043643111367, |p| {
            matches!(p, CssProperty::BorderLeftWidth(_))
        });
        let plain_right = plain_decls_where(&CSS_MATCH_11510695043643111367, |p| {
            matches!(p, CssProperty::BorderRightWidth(_))
        });
        assert_eq!(plain_left.len(), 1, "one unconditional left-border width");
        assert_eq!(plain_right.len(), 1, "one unconditional right-border width");
        assert_ne!(
            plain_left[0],
            CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::None),
            "a tab with no active neighbour keeps its left border"
        );
        assert_ne!(
            plain_right[0],
            CssProperty::BorderRightWidth(LayoutBorderRightWidthValue::None),
            "a tab with no active neighbour keeps its right border"
        );
    }

    #[test]
    fn tab_styles_redeclare_properties_and_therefore_depend_on_declaration_order() {
        // PINNED HAZARD: each vec is emitted most-specific-block-first, so the
        // generic `.__azul-native-tabs-header p` block *repeats* properties the
        // state-specific block already set, with different values. Which one wins
        // is decided by the resolver, and the two in azul disagree:
        // `PropertyCache::get_property` takes the FIRST match (specific wins,
        // intended), `get_property_with_context` takes the LAST ("last wins",
        // which silently repaints the active tab like an inactive one).
        let heights = decls_where(&CSS_MATCH_14575853790110873394, |p| {
            matches!(p, CssProperty::Height(_))
        });
        assert_eq!(heights.len(), 2, "the active tab declares height twice");
        assert_eq!(
            heights[0],
            CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(
                PixelValue::const_px(23)
            ))),
            "the active-tab block (23px) must come first"
        );
        assert_eq!(
            heights[1],
            CssProperty::Height(LayoutHeightValue::Exact(LayoutHeight::Px(
                PixelValue::const_px(21)
            ))),
            "the generic p block (21px) shadows it under last-wins"
        );

        // The light face only: the dark-theme twin comes after both.
        let backgrounds = plain_decls_where(&CSS_MATCH_14575853790110873394, |p| {
            matches!(p, CssProperty::BackgroundContent(_))
        });
        assert_eq!(backgrounds.len(), 2);
        let kind = |p: &CssProperty| -> &'static str {
            let CssProperty::BackgroundContent(v) = p else {
                unreachable!()
            };
            match v
                .get_property()
                .expect("an exact background")
                .as_ref()
                .first()
                .expect("one layer")
            {
                StyleBackgroundContent::Color(_) => "flat",
                StyleBackgroundContent::LinearGradient(_) => "gradient",
                _ => "other",
            }
        };
        assert_eq!(kind(&backgrounds[0]), "flat", "active = white fill");
        assert_eq!(
            kind(&backgrounds[1]),
            "gradient",
            "the generic p block re-declares the inactive gradient"
        );

        // Same shape on the seam styles: the None triple is re-declared as 1px.
        let left = plain_decls_where(&CSS_MATCH_13824480602841492081, |p| {
            matches!(p, CssProperty::BorderLeftWidth(_))
        });
        assert_eq!(left.len(), 2, "the -noleftborder style declares it twice");
        assert_eq!(
            left[0],
            CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::None)
        );
        assert_eq!(
            left[1],
            CssProperty::BorderLeftWidth(LayoutBorderLeftWidthValue::Exact(
                LayoutBorderLeftWidth {
                    inner: PixelValue::const_px(1)
                }
            ))
        );
    }

    #[test]
    fn hover_declarations_stay_conditional_and_the_rest_stay_unconditional() {
        // A hover rule leaking into the unconditional set would permanently paint
        // every inactive tab in the hover colour.
        for style in [
            CSS_MATCH_11510695043643111367,
            CSS_MATCH_13824480602841492081,
            CSS_MATCH_4415083954137121609,
        ] {
            let conditional = style
                .as_ref()
                .iter()
                .filter(|p| !p.apply_if.as_ref().is_empty())
                .count();
            let hover_only = style
                .as_ref()
                .iter()
                .filter(|p| is_hover_only(p.apply_if.as_ref()))
                .count();
            let dark_hover = style
                .as_ref()
                .iter()
                .filter(|p| is_dark_hover(p.apply_if.as_ref()))
                .count();
            let dark_resting = style
                .as_ref()
                .iter()
                .filter(|p| is_dark_resting(p.apply_if.as_ref()))
                .count();
            assert_eq!(
                hover_only, 13,
                "each inactive-tab style carries exactly the 13 :hover declarations"
            );
            assert_eq!(
                dark_hover, 13,
                "...and one dark twin per :hover declaration, from `themes::flat`"
            );
            assert_eq!(
                dark_resting, 5,
                "...and the resting face's dark twins: four edge colours and the fill"
            );
            assert_eq!(
                conditional,
                hover_only + dark_hover + dark_resting,
                "nothing else in an inactive-tab style is conditional"
            );
        }

        for style in [
            CSS_MATCH_14575853790110873394,
            CSS_MATCH_9988039989460234263,
            CSS_MATCH_17290739305197504468,
            CSS_MATCH_3088386549906605418,
            CSS_MATCH_18014909903571752977,
            CSS_MATCH_18014909903571752977_NO_PADDING,
            CSS_MATCH_4738503469417034630,
            CSS_MATCH_4738503469417034630_NO_PADDING,
        ] {
            assert!(
                style
                    .as_ref()
                    .iter()
                    .all(|p| p.apply_if.as_ref().is_empty() || is_dark_resting(p.apply_if.as_ref())),
                "this style must apply unconditionally, apart from its dark-theme colours"
            );
        }
    }

    #[test]
    fn dom_carries_the_themes_tab_hover_state_with_dark_twins() {
        // The thirteen hover rules moved OUT of this file and into
        // `themes::flat`, where the dark half of each pair can be written. That
        // is a move nothing else here would notice: no compiler error, and the
        // verbatim style-vs-node comparisons pass whether or not a slice names
        // the twins. So this asks the RENDERED tabs what they carry.
        //
        // active = 1 of 4 renders every inactive variant at once: tab 0 is the
        // `-norightborder` seam, tab 2 the `-noleftborder` seam, tab 3 the plain
        // inactive tab.
        fn gated(node: &Dom, pick: fn(&[DynamicSelector]) -> bool) -> Vec<CssProperty> {
            node.root
                .style
                .iter_inline_properties()
                .filter(|(_, conds)| pick(conds.as_ref()))
                .map(|(p, _)| p.clone())
                .collect()
        }

        fn is_colour(p: &CssProperty) -> bool {
            matches!(
                p,
                CssProperty::BorderTopColor(_)
                    | CssProperty::BorderBottomColor(_)
                    | CssProperty::BorderLeftColor(_)
                    | CssProperty::BorderRightColor(_)
                    | CssProperty::BackgroundContent(_)
            )
        }

        // Pinned to flat: this counts the flat look's hover rules and twins.
        let dom = TabHeader::create(numbered_labels(4))
            .with_active_tab(1)
            .with_theme(UiTheme::Flat)
            .dom();
        let tabs = &dom.children.as_ref()[1..=4];
        assert_eq!(
            class_strs(&tabs[1]),
            vec![CLASS_ACTIVE],
            "the fixture's active tab"
        );

        for (i, node) in tabs.iter().enumerate() {
            let cls = class_strs(node);
            let light = gated(node, is_hover_only);
            let dark = gated(node, is_dark_hover);

            if cls.contains(&CLASS_ACTIVE) {
                assert!(
                    light.is_empty() && dark.is_empty(),
                    "tab {i}: the active tab has no hover state to twin"
                );
                continue;
            }

            assert_eq!(
                light.len(),
                13,
                "tab {i} {cls:?}: the hover ring (4 widths, 4 styles, 4 colours) and the fill"
            );
            assert_eq!(
                dark.len(),
                13,
                "tab {i} {cls:?}: a dark twin is missing, so a hovered tab keeps its light-mode \
                 ring or fill on a dark surface"
            );
            for rule in &light {
                assert!(
                    dark.iter().any(|twin| twin.get_type() == rule.get_type()),
                    "tab {i} {cls:?}: the light hover rule {rule:?} has no dark twin"
                );
            }
            for twin in &dark {
                assert!(
                    light.iter().any(|rule| rule.get_type() == twin.get_type()),
                    "tab {i} {cls:?}: the dark rule {twin:?} twins nothing the light hover sets"
                );
            }

            // The colour twins are genuinely the other mode's values, not the
            // light rule spelled twice. (The widths and styles ARE the same in
            // both modes; only the colours and the fill have a per-mode value.)
            let light_colours: Vec<&CssProperty> = light.iter().filter(|p| is_colour(p)).collect();
            let dark_colours: Vec<&CssProperty> = dark.iter().filter(|p| is_colour(p)).collect();
            assert_eq!(
                light_colours.len(),
                5,
                "tab {i}: four border colours and the fill"
            );
            assert_ne!(
                light_colours, dark_colours,
                "tab {i} {cls:?}: the dark twins repeat the light colours"
            );

            // The hover twins are gated on dark AND hover. The only declarations
            // gated on dark alone are the resting face's colours (fill + four
            // edges), which is what makes the tab follow the theme at rest.
            let resting = gated(node, is_dark_resting);
            assert_eq!(
                resting.len(),
                5,
                "tab {i} {cls:?}: the resting face needs its dark fill and four edge colours"
            );
            assert!(
                resting.iter().all(is_colour),
                "tab {i} {cls:?}: a resting dark twin restyles something other than a colour"
            );
            assert_eq!(
                theme_probe::dark(node).len(),
                dark.len() + resting.len(),
                "tab {i} {cls:?}: every dark-mode declaration on a tab is a hover twin or a \
                 resting colour"
            );
        }
    }

    #[test]
    fn dom_datasets_carry_the_position_of_their_tab() {
        for n in [1usize, 2, 5, 1000] {
            let dom = TabHeader::create(numbered_labels(n)).dom();
            let children = dom.children.as_ref();
            for (i, node) in children[1..=n].iter().enumerate() {
                assert_eq!(
                    tab_idx_of(node),
                    i,
                    "n={n}: tab {i} must carry its own index, not the loop seed"
                );
            }
        }
    }

    #[test]
    fn dom_attaches_a_dataset_even_without_a_click_callback() {
        let dom = TabHeader::create(numbered_labels(2)).dom();
        for node in &dom.children.as_ref()[1..=2] {
            assert!(
                node.root.get_dataset().is_some(),
                "the dataset is unconditional (only the callback is not)"
            );
            assert!(
                node.root.get_callbacks().as_ref().is_empty(),
                "no callback must be attached when on_click is None"
            );
        }
    }

    #[test]
    fn dom_attaches_the_click_and_the_arrow_key_callback_per_tab_when_on_click_is_set() {
        let n = 4usize;
        let dom = TabHeader::create(numbered_labels(n))
            .with_on_click(RefAny::new(ClickLog::default()), cb(record_click))
            .dom();
        let children = dom.children.as_ref();

        for (i, node) in children[1..=n].iter().enumerate() {
            let cbs = node.root.get_callbacks();
            assert_eq!(cbs.as_ref().len(), 2, "the click and the key callback");
            let data = &cbs.as_ref()[0];
            assert_eq!(
                data.event,
                EventFilter::Hover(HoverEventFilter::Click),
                "tabs must react on mouse-up, not mouse-down"
            );
            assert_eq!(
                data.callback.cb, on_tab_click as usize,
                "the dispatcher must be the widget's own trampoline"
            );
            let key = &cbs.as_ref()[1];
            assert_eq!(
                key.event,
                EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
            );
            assert_eq!(key.callback.cb, on_tab_key as usize);
            // ONE Tab stop per tab list: the active tab (tab 0 of a fresh one).
            assert_eq!(
                node.root.get_tab_index(),
                Some(if i == 0 {
                    TabIndex::Auto
                } else {
                    TabIndex::NoKeyboardFocus
                }),
                "tab {i} has the wrong tab index"
            );
        }

        // The spacers must stay inert — a click on the filler must not select.
        assert!(children[0].root.get_callbacks().as_ref().is_empty());
        assert!(children[n + 1].root.get_callbacks().as_ref().is_empty());
    }

    #[test]
    fn dom_keeps_the_estimated_child_count_consistent() {
        // `estimated_total_children` is what sizes the flat arena; a stale value
        // makes `convert_dom_into_compact_dom` under-allocate and panic.
        for n in [0usize, 1, 2, 50] {
            let dom = TabHeader::create(numbered_labels(n)).dom();
            assert_eq!(
                dom.estimated_total_children,
                dom.recompute_estimated_total_children(),
                "n={n}: cached descendant count desynced"
            );
            assert_eq!(
                dom.node_count(),
                2 * n + 3,
                "header + spacer + {n} tabs (each a <p> wrapping one text node) + spacer"
            );

            let styled = StyledDom::create_from_dom(dom);
            assert_eq!(
                styled.node_hierarchy.as_ref().len(),
                2 * n + 3,
                "n={n}: the flattened arena must match node_count()"
            );
        }
    }

    #[test]
    fn dom_releases_every_dataset_clone_when_the_dom_is_dropped() {
        // Each tab clones the user payload into its own `TabLocalDataset`; if the
        // widget leaked one, the app state would outlive the DOM forever.
        for n in [0usize, 1, 5] {
            let user = RefAny::new(ClickLog::default());
            let dom = TabHeader::create(numbered_labels(n))
                .with_on_click(user.clone(), cb(record_click))
                .dom();

            assert_eq!(
                user.get_ref_count(),
                n + 1,
                "n={n}: one payload clone per tab, plus the caller's handle"
            );

            drop(dom);
            assert_eq!(
                user.get_ref_count(),
                1,
                "n={n}: dropping the DOM must release every payload clone"
            );
        }
    }

    #[test]
    fn dom_is_deterministic() {
        let build = || {
            let dom = TabHeader::create(numbered_labels(5))
                .with_active_tab(2)
                .dom();
            let children = dom.children.as_ref();
            let shape: Vec<(Option<String>, Vec<&'static str>)> = children
                .iter()
                .map(|c| (text_of(c).map(str::to_string), class_strs(c)))
                .collect();
            shape
        };
        assert_eq!(
            build(),
            build(),
            "dom() must be a pure function of its state"
        );
    }

    // ==================================================================
    // TabContent
    // ==================================================================

    fn nested(depth: usize) -> Dom {
        let mut dom = Dom::create_text_do_not_use_without_block_level_wrapper("leaf");
        for _ in 0..depth {
            dom = Dom::create_div().with_child(dom);
        }
        dom
    }

    #[test]
    fn content_new_defaults_to_padding_and_keeps_the_content_verbatim() {
        let content = nested(3);
        let tab_content = TabContent::new(content.clone());

        assert!(tab_content.has_padding, "new() defaults to padded");
        assert_eq!(
            tab_content.content, content,
            "new() must not rewrap or normalise the content"
        );
        assert!(
            TabContent::default().has_padding,
            "Default agrees with new()"
        );
    }

    #[test]
    fn with_padding_and_set_padding_agree_and_are_last_wins() {
        for flag in [true, false] {
            let built = TabContent::new(Dom::create_div()).with_padding(flag);
            let mut mutated = TabContent::new(Dom::create_div());
            mutated.set_padding(flag);
            assert_eq!(built.has_padding, flag);
            assert_eq!(built.has_padding, mutated.has_padding);
        }

        let toggled = TabContent::new(Dom::create_div())
            .with_padding(false)
            .with_padding(true)
            .with_padding(false);
        assert!(!toggled.has_padding, "the last write must win");
    }

    #[test]
    fn content_dom_nests_the_content_under_the_content_class() {
        let content = nested(2);
        let dom = TabContent::new(content.clone()).dom();

        assert!(
            class_strs(&dom).is_empty(),
            "the outer wrapper carries the style, not the class"
        );
        assert_eq!(dom.children.as_ref().len(), 1, "one wrapper child");

        let inner = &dom.children.as_ref()[0];
        assert_eq!(class_strs(inner), vec![CLASS_CONTENT]);
        assert!(
            inline_declared(inner).is_empty(),
            "the classed node carries no inline style of its own"
        );
        assert_eq!(inner.children.as_ref().len(), 1);
        assert_eq!(
            inner.children.as_ref()[0],
            content,
            "the user content must survive the two wrappers untouched"
        );
    }

    #[test]
    fn content_dom_picks_the_style_vec_the_padding_flag_asks_for() {
        // Pinned to flat: the styles compared are flat's parts (the widget's
        // `PANEL_BASE`, then flat's const vecs).
        let look = flat::tab_content_look();
        let padded = TabContent::new(Dom::create_div())
            .with_padding(true)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(inline_declared(&padded), declared(&look.padded));

        let bare = TabContent::new(Dom::create_div())
            .with_padding(false)
            .with_theme(UiTheme::Flat)
            .dom();
        assert_eq!(inline_declared(&bare), declared(&look.unpadded));

        assert_ne!(
            inline_declared(&padded),
            inline_declared(&bare),
            "the two padding modes must not collapse to the same style"
        );
    }

    #[test]
    fn the_unpadded_content_style_declares_no_padding_at_all() {
        let padding_decls = decls_where(&CSS_MATCH_18014909903571752977_NO_PADDING, |p| {
            matches!(
                p,
                CssProperty::PaddingTop(_)
                    | CssProperty::PaddingBottom(_)
                    | CssProperty::PaddingLeft(_)
                    | CssProperty::PaddingRight(_)
            )
        });
        assert!(
            padding_decls.is_empty(),
            "has_padding == false must not leave a padding declaration behind"
        );

        let padded = decls_where(&CSS_MATCH_18014909903571752977, |p| {
            matches!(
                p,
                CssProperty::PaddingTop(_)
                    | CssProperty::PaddingBottom(_)
                    | CssProperty::PaddingLeft(_)
                    | CssProperty::PaddingRight(_)
            )
        });
        assert_eq!(padded.len(), 4, "the padded variant sets all four sides");
    }

    #[test]
    fn content_dom_keeps_the_estimated_child_count_consistent() {
        for depth in [0usize, 1, 3, 64] {
            let content = nested(depth);
            let expected = content.node_count() + 2; // outer wrapper + classed wrapper
            let dom = TabContent::new(content).dom();

            assert_eq!(
                dom.estimated_total_children,
                dom.recompute_estimated_total_children(),
                "depth={depth}: cached descendant count desynced"
            );
            assert_eq!(dom.node_count(), expected, "depth={depth}");

            let styled = StyledDom::create_from_dom(dom);
            assert_eq!(
                styled.node_hierarchy.as_ref().len(),
                expected,
                "depth={depth}: the flattened arena must match node_count()"
            );
        }
    }

    #[test]
    fn content_swap_with_default_returns_the_old_state_and_leaves_an_empty_div() {
        let content = nested(2);
        let mut tab_content = TabContent::new(content.clone()).with_padding(false);

        let taken = tab_content.swap_with_default();
        assert_eq!(taken.content, content);
        assert!(!taken.has_padding, "the flag moves out with the content");

        assert_eq!(
            tab_content.content,
            Dom::create_div(),
            "the husk must be an empty div"
        );
        assert!(
            tab_content.has_padding,
            "the husk is a Default, i.e. padded again"
        );
    }

    // ==================================================================
    // on_tab_click
    // ==================================================================

    #[test]
    fn on_tab_click_reports_the_clicked_index_to_the_user_callback() {
        for idx in [0usize, 1, 7, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let mut user = RefAny::new(ClickLog::default());
            let (update, changes) =
                run_click(dataset(idx, click_handler(user.clone(), record_click)));

            assert_eq!(update, Update::RefreshDom);
            assert!(
                changes.is_empty(),
                "the tab handler is stateless — it must not push CallbackChanges"
            );
            assert_eq!(
                logged(&mut user),
                vec![TabHeaderState { active_tab: idx }],
                "the index must be forwarded verbatim, without arithmetic"
            );
        }
    }

    #[test]
    fn on_tab_click_propagates_the_user_update_verbatim() {
        for (callback, expected) in [
            (record_click as TabOnClickCallbackType, Update::RefreshDom),
            (click_do_nothing, Update::DoNothing),
            (click_refresh_all, Update::RefreshDomAllWindows),
        ] {
            let (update, _) = run_click(dataset(
                3,
                click_handler(RefAny::new(ClickLog::default()), callback),
            ));
            assert_eq!(update, expected, "the user verdict must not be overridden");
        }
    }

    #[test]
    fn on_tab_click_without_a_user_callback_does_nothing() {
        // A dataset with no `on_click` is reachable only by hand (dom() attaches
        // the trampoline only when a callback exists) — it must stay silent
        // rather than force a relayout.
        let (update, changes) = run_click(dataset(2, None.into()));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn on_tab_click_on_a_foreign_payload_falls_back_to_refresh() {
        // Wrong-typed payload => `downcast_mut` returns None => the documented
        // `unwrap_or(RefreshDom)` fallback. Note this is the *opposite* verdict
        // from the no-callback case above, which is the asymmetry to watch.
        for foreign in [RefAny::new(42u32), RefAny::new(String::from("nope"))] {
            let (update, changes) = run_click(foreign);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }
    }

    #[test]
    fn on_tab_click_declines_while_the_dataset_is_already_borrowed() {
        // Borrow tracking is shared across clones, so a live `Ref` from anywhere
        // must make the handler bail out safely instead of aliasing or panicking.
        let mut held = dataset(1, None.into());
        let clone = held.clone();
        let guard = held
            .downcast_ref::<TabLocalDataset>()
            .expect("the fixture is a TabLocalDataset");

        let (update, changes) = run_click(clone);
        assert_eq!(
            update,
            Update::RefreshDom,
            "a contended dataset must fall back, not panic"
        );
        assert!(changes.is_empty());

        drop(guard);

        // ...and the borrow must be released again afterwards.
        let (update, _) = run_click(held.clone());
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn on_tab_click_releases_its_borrow_so_repeated_clicks_keep_working() {
        let mut user = RefAny::new(ClickLog::default());
        let data = dataset(4, click_handler(user.clone(), record_click));

        let (first, _) = run_click(data.clone());
        let (second, _) = run_click(data.clone());
        let (third, _) = run_click(data.clone());

        assert_eq!(
            [first, second, third],
            [Update::RefreshDom; 3],
            "a leaked RefMut would turn later clicks into the RefreshDom fallback without ever \
             reaching the user callback"
        );
        assert_eq!(
            logged(&mut user),
            vec![TabHeaderState { active_tab: 4 }; 3],
            "every click must reach the user callback"
        );
    }

    #[test]
    fn clicking_a_rendered_tab_selects_that_tab_end_to_end() {
        let n = 5usize;
        let mut user = RefAny::new(ClickLog::default());
        let dom = TabHeader::create(numbered_labels(n))
            .with_on_click(user.clone(), cb(record_click))
            .dom();

        for i in 0..n {
            let data = dom.children.as_ref()[i + 1]
                .root
                .get_callbacks()
                .as_ref()
                .first()
                .expect("every tab carries the click callback")
                .refany
                .clone();
            let (update, changes) = run_click(data);
            assert_eq!(update, Update::RefreshDom);
            assert!(changes.is_empty());
        }

        assert_eq!(
            logged(&mut user),
            (0..n)
                .map(|active_tab| TabHeaderState { active_tab })
                .collect::<Vec<_>>(),
            "clicking tab i must report exactly i"
        );
    }

    #[test]
    fn clicking_a_tab_does_not_move_the_active_tab_by_itself() {
        // The widget is stateless: the handler reports the click and nothing
        // else. Re-rendering the *same* header must therefore keep tab 0 active.
        let user = RefAny::new(ClickLog::default());
        let header =
            TabHeader::create(numbered_labels(3)).with_on_click(user.clone(), cb(record_click));
        let dom = header.clone().dom();

        let data = dom.children.as_ref()[3]
            .root
            .get_callbacks()
            .as_ref()
            .first()
            .expect("tab 2 carries the click callback")
            .refany
            .clone();
        let (update, _) = run_click(data);
        assert_eq!(update, Update::RefreshDom);

        assert_eq!(
            header.active_tab, 0,
            "the header itself must be untouched by a click"
        );
        assert_eq!(
            class_strs(&header.dom().children.as_ref()[1]),
            vec![CLASS_ACTIVE],
            "re-rendering the unchanged header keeps tab 0 active"
        );
    }

    // ==================================================================
    // Roving tabindex (WAI-ARIA APG tabs, automatic activation, P2-12)
    // ==================================================================

    /// A plain tab stop, the header, another plain tab stop. Flattened: root
    /// 0, before 1, header 2, the leading spacer 3, tab `i` at `4 + 2 * i`
    /// (a `<p>` + its text), the trailing spacer at `4 + 2 * n`, after at
    /// `5 + 2 * n`.
    fn tab_page(header: TabHeader) -> StyledDom {
        let stop = || Dom::create_div().with_tab_index(TabIndex::Auto);
        let page = Dom::create_div().with_children(vec![stop(), header.dom(), stop()].into());
        StyledDom::create_from_dom(page)
    }

    fn page_node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn page_before() -> DomNodeId {
        page_node(1)
    }

    fn page_tab(i: usize) -> DomNodeId {
        page_node(4 + 2 * i)
    }

    fn page_after(n: usize) -> DomNodeId {
        page_node(5 + 2 * n)
    }

    /// An interactive three-tab header with tab `active` active; clicks (and
    /// arrow activations) are logged into `user`.
    fn three_tabs(active: usize, user: &RefAny) -> TabHeader {
        TabHeader::create(strings(&["one", "two", "three"]))
            .with_active_tab(active)
            .with_on_click(user.clone(), cb(record_click))
    }

    /// Presses `key` on tab `i`; panics when the tab has no key handler - the
    /// state of every tab before P2-12.
    fn press_tab(
        styled: &StyledDom,
        i: usize,
        key: VirtualKeyCode,
        held: &[VirtualKeyCode],
    ) -> (Update, Vec<CallbackChange>) {
        rv::press(styled, page_tab(i), key, held)
            .expect("every tab of an interactive tab list must carry a key handler")
    }

    #[test]
    fn tab_from_the_item_before_lands_on_the_active_tab_and_the_next_tab_leaves_the_tab_list() {
        let user = RefAny::new(ClickLog::default());
        let styled = tab_page(three_tabs(1, &user));
        assert_eq!(
            rv::tab_walk(&styled, Some(page_before()), true, 2),
            vec![page_tab(1), page_after(3)],
            "the tab list is ONE tab stop: the active tab, then out",
        );
        assert_eq!(
            rv::tab_walk(&styled, Some(page_after(3)), false, 2),
            vec![page_tab(1), page_before()],
        );
    }

    #[test]
    fn with_the_active_tab_out_of_range_the_first_tab_is_the_tab_stop() {
        let user = RefAny::new(ClickLog::default());
        let styled = tab_page(three_tabs(usize::MAX, &user));
        assert_eq!(
            rv::tab_walk(&styled, Some(page_before()), true, 2),
            vec![page_tab(0), page_after(3)],
        );
    }

    #[test]
    fn a_tab_list_without_on_click_stays_out_of_the_tab_order() {
        // Guard (green before and after P2-12): a header nobody can activate is
        // inert - no stop, no handlers.
        let styled = tab_page(TabHeader::create(strings(&["one", "two", "three"])));
        assert_eq!(
            rv::tab_walk(&styled, Some(page_before()), true, 1),
            vec![page_after(3)],
        );
        assert!(rv::press(&styled, page_tab(0), VirtualKeyCode::Right, &[]).is_none());
    }

    #[test]
    fn arrow_right_on_the_active_tab_focuses_and_activates_the_next_tab() {
        let mut user = RefAny::new(ClickLog::default());
        let styled = tab_page(three_tabs(0, &user));

        let (update, changes) = press_tab(&styled, 0, VirtualKeyCode::Right, &[]);

        assert_eq!(
            logged(&mut user),
            vec![TabHeaderState { active_tab: 1 }],
            "automatic activation: the arrow reports the new tab like a click",
        );
        assert_eq!(update, Update::RefreshDom, "the app's verdict is forwarded");
        assert_eq!(rv::focus_request(&changes), Some(page_tab(1)));
        assert!(rv::prevented(&changes));
    }

    #[test]
    fn left_and_right_wrap_and_home_and_end_jump_to_the_ends() {
        use azul_core::window::VirtualKeyCode as K;

        for (from, key, to) in [
            (0, K::Left, 2),
            (2, K::Right, 0),
            (1, K::Left, 0),
            (1, K::Home, 0),
            (1, K::End, 2),
            (0, K::End, 2),
            (2, K::Home, 0),
        ] {
            let mut user = RefAny::new(ClickLog::default());
            let styled = tab_page(three_tabs(from, &user));
            let (_, changes) = press_tab(&styled, from, key, &[]);
            assert_eq!(
                logged(&mut user),
                vec![TabHeaderState { active_tab: to }],
                "{key:?} on tab {from} must activate tab {to}",
            );
            assert_eq!(rv::focus_request(&changes), Some(page_tab(to)));
            assert!(rv::prevented(&changes));
        }
    }

    #[test]
    fn after_an_arrow_the_focused_tab_is_the_only_tab_stop_even_before_a_rebuild() {
        let user = RefAny::new(ClickLog::default());
        let mut styled = tab_page(three_tabs(0, &user));
        let (_, changes) = press_tab(&styled, 0, VirtualKeyCode::End, &[]);
        rv::apply_tab_index_writes(&mut styled, &changes);
        assert_eq!(
            rv::tab_walk(&styled, Some(page_after(3)), false, 2),
            vec![page_tab(2), page_before()],
        );
    }

    #[test]
    fn vertical_and_modified_arrows_on_a_tab_are_not_consumed() {
        use azul_core::window::VirtualKeyCode as K;

        for (key, held) in [
            (K::Down, None),
            (K::Up, None),
            (K::Tab, None),
            (K::Right, Some(K::LAlt)),
            (K::Left, Some(K::LControl)),
            (K::End, Some(K::LShift)),
            (K::Home, Some(K::RWin)),
        ] {
            let mut user = RefAny::new(ClickLog::default());
            let styled = tab_page(three_tabs(0, &user));
            let held: Vec<K> = held.into_iter().collect();
            let (update, changes) = press_tab(&styled, 0, key, &held);
            assert_eq!(update, Update::DoNothing);
            assert!(logged(&mut user).is_empty(), "{held:?}+{key:?} activated a tab");
            assert!(
                changes.is_empty(),
                "{held:?}+{key:?} must not be consumed: {changes:?}"
            );
        }
    }

    // ------------------------------------------------------------------
    // Accessibility: a tab says it is a tab, the header that it is a tab
    // list, and the active tab that it is the selected one - also LIVE when
    // an arrow activates another tab, before any rebuild. The header and its
    // tabs declared nothing at all.
    // ------------------------------------------------------------------

    #[test]
    fn every_tab_is_a_page_tab_and_the_active_one_says_it_is_selected() {
        use azul_core::a11y::{AccessibilityRole, AccessibilityState::Selected};

        for with_on_click in [true, false] {
            let user = RefAny::new(ClickLog::default());
            let header = if with_on_click {
                three_tabs(1, &user)
            } else {
                TabHeader::create(strings(&["one", "two", "three"])).with_active_tab(1)
            };
            let styled = tab_page(header);
            assert_eq!(
                rv::declared(&styled, page_node(2)).map(|(role, _)| role),
                Some(AccessibilityRole::PageTabList),
                "on_click={with_on_click}: the header is a tab list",
            );
            for i in 0..3 {
                assert_eq!(
                    rv::declared(&styled, page_tab(i)),
                    Some((
                        AccessibilityRole::PageTab,
                        if i == 1 { vec![Selected] } else { Vec::new() }
                    )),
                    "on_click={with_on_click}: tab {i}",
                );
            }
        }
    }

    #[test]
    fn an_arrow_announces_the_tab_it_activates() {
        use azul_core::a11y::AccessibilityState::Selected;

        let user = RefAny::new(ClickLog::default());
        let styled = tab_page(three_tabs(0, &user));
        let (_, changes) = press_tab(&styled, 0, VirtualKeyCode::Right, &[]);
        assert_eq!(
            rv::announced_states(&changes),
            vec![
                (page_tab(0), Vec::new()),
                (page_tab(1), vec![Selected]),
                (page_tab(2), Vec::new()),
            ],
        );
    }
}

/// The tab bar's two looks (W5b). Flat is the Windows-native control. Flora
/// is flora's navigation strip cut as Firefox's tab row (Australis): raised
/// chrome closed along its foot by a 2px metal rule; the unselected tabs
/// stand on the rule in soft ink, lift to the hover face under the pointer
/// and sink when pressed; the selected tab is the sunken accent stone in a
/// metal surround that climbs its S-curved sides, breaks the rule and opens
/// onto its panel - a leaf in a hairline, open at the top. The tabs, their
/// classes, datasets, click and arrow keys are the widget's in both.
#[cfg(test)]
mod theme_tests {
    use azul_core::dom::Dom;
    use azul_css::{
        dynamic_selector::PseudoStateType,
        props::{basic::color::ColorU, property::CssPropertyType, style::StyleBackgroundContent},
    };

    use super::*;
    use crate::widgets::themes::{flora, theme_checks as tc, OptionUiTheme, UiTheme};

    const FLORA: &str = "__azul-theme-flora";

    extern "C" fn pick(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::DoNothing
    }

    /// `One | Two | Three | Four` with `Two` active, clickable: the header's
    /// children are the spacer before (0), `One` (1, before the active tab),
    /// `Two` (2, active), `Three` (3, after it), `Four` (4) and the spacer
    /// after (5).
    fn bar(theme: UiTheme) -> Dom {
        TabHeader::create(StringVec::from_vec(
            ["One", "Two", "Three", "Four"]
                .iter()
                .map(|s| AzString::from(*s))
                .collect(),
        ))
        .with_active_tab(1)
        .with_on_click(RefAny::new(()), pick as TabOnClickCallbackType)
        .with_theme(theme)
        .dom()
    }

    fn panel(theme: UiTheme, padding: bool) -> Dom {
        TabContent::new(Dom::create_p_with_text("Body"))
            .with_padding(padding)
            .with_theme(theme)
            .dom()
    }

    fn child(dom: &Dom, i: usize) -> &Dom {
        &dom.children.as_ref()[i]
    }

    /// The resolved width of a border edge, in px (`None`: not declared).
    fn width(node: &Dom, ty: CssPropertyType, dark: bool) -> Option<f32> {
        use azul_css::props::property::CssProperty as C;
        match tc::resolve(node, ty, dark, None)? {
            C::BorderTopWidth(v) => v.get_property().map(|w| w.inner.number.get()),
            C::BorderRightWidth(v) => v.get_property().map(|w| w.inner.number.get()),
            C::BorderBottomWidth(v) => v.get_property().map(|w| w.inner.number.get()),
            C::BorderLeftWidth(v) => v.get_property().map(|w| w.inner.number.get()),
            _ => None,
        }
    }

    fn colour(
        node: &Dom,
        ty: CssPropertyType,
        dark: bool,
        state: Option<PseudoStateType>,
    ) -> Option<ColorU> {
        tc::resolve(node, ty, dark, state)
            .as_ref()
            .and_then(tc::border_color)
    }

    fn layers(
        node: &Dom,
        dark: bool,
        state: Option<PseudoStateType>,
    ) -> Vec<StyleBackgroundContent> {
        tc::resolve(node, CssPropertyType::BackgroundContent, dark, state)
            .map(|p| tc::bg_layers(&p))
            .unwrap_or_default()
    }

    #[test]
    fn a_tab_bar_without_a_theme_follows_the_app_theme_and_set_theme_pins_it() {
        let header = TabHeader::create(StringVec::from_const_slice(&[]));
        assert_eq!(header.theme, OptionUiTheme::None, "a fresh bar follows the app");
        assert_eq!(TabHeader::default().theme, OptionUiTheme::None);
        let mut set = header.clone();
        set.set_theme(UiTheme::Flora);
        assert_eq!(set.theme, header.with_theme(UiTheme::Flora).theme);
        assert_eq!(set.theme, OptionUiTheme::Some(UiTheme::Flora));

        let content = TabContent::new(Dom::create_div());
        assert_eq!(content.theme, OptionUiTheme::None, "a fresh panel follows the app");
        assert_eq!(TabContent::default().theme, OptionUiTheme::None);
        let mut set = content.clone();
        set.set_theme(UiTheme::Flora);
        assert_eq!(set.theme, content.with_theme(UiTheme::Flora).theme);
    }

    /// `margin-bottom: <px>`, resolved.
    fn margin_bottom(node: &Dom, dark: bool, state: Option<PseudoStateType>) -> Option<CssProperty> {
        tc::resolve(node, CssPropertyType::MarginBottom, dark, state)
    }

    fn margin_px(px: isize) -> Option<CssProperty> {
        Some(CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(px)))
    }

    #[test]
    fn a_flora_tab_strip_is_raised_chrome_closed_by_the_websites_metal_rule() {
        let dom = bar(UiTheme::Flora);
        assert!(tc::has_class(&dom, FLORA), "the header carries flora's marker");
        assert!(tc::has_class(&dom, "__azul-native-tabs-header"));
        assert!(!tc::has_class(&bar(UiTheme::Flat), FLORA));
        for dark in [false, true] {
            let chrome = if dark {
                flora::RAISED_FACE_DARK
            } else {
                flora::RAISED_FACE_LIGHT
            };
            // `.navbar::after`: the rule is `--fl-rule-metal-bg` - brass at
            // half alpha at both ends of the strip, its glint travelling
            // along it - seen through the strip's transparent 2px foot under
            // the chrome on the padding box. One rule from edge to edge: it
            // runs under both spacers and every unselected tab, and the
            // selected tab stands over it.
            assert_eq!(
                layers(&dom, dark, None),
                vec![tc::flora_css_rule_metal(), chrome],
                "dark={dark}: the rule under the chrome"
            );
            assert_eq!(
                tc::background_clips(&dom, dark, None),
                vec![StyleBackgroundClip::BorderBox, StyleBackgroundClip::PaddingBox],
                "dark={dark}: the rule on the border box, the chrome on the padding box"
            );
            assert_eq!(
                width(&dom, CssPropertyType::BorderBottomWidth, dark),
                Some(2.0),
                "dark={dark}: the rule's gauge, `--fl-metal`"
            );
            assert_eq!(
                colour(&dom, CssPropertyType::BorderBottomColor, dark, None),
                Some(ColorU::TRANSPARENT),
                "dark={dark}: the strip's foot is transparent, so the metal shows"
            );
            assert_eq!(
                tc::resolve(&dom, CssPropertyType::BoxShadowBottom, dark, None),
                None,
                "dark={dark}: no flat inset line over the metal"
            );
            for i in [1usize, 3, 4] {
                let tab = child(&dom, i);
                assert_eq!(
                    tc::text_color(tab, dark),
                    Some(if dark { flora::DARK_SOFT1 } else { flora::LIGHT_SOFT1 }),
                    "dark={dark}: unselected tab {i} is written in soft ink"
                );
                assert_eq!(
                    margin_bottom(tab, dark, None),
                    margin_px(0),
                    "dark={dark}: unselected tab {i} stands on the rule, which stays in sight"
                );
            }
        }
    }

    #[test]
    fn the_flora_selected_tab_is_the_stone_cut_from_the_rolled_metal_and_open_at_its_foot() {
        let dom = bar(UiTheme::Flora);
        let active = child(&dom, 2);
        let face = flora::australis_face(flora::STONE_STREAK);
        let mut cut = vec![tc::flora_css_rolled_tab()];
        cut.extend(face.iter().cloned());
        let mut boxes = vec![StyleBackgroundClip::BorderBox];
        boxes.extend(face.iter().map(|_| StyleBackgroundClip::PaddingBox));
        for dark in [false, true] {
            // `.nav-links a.active { background: var(--fl-gem-sunken)
            // padding-box, var(--fl-rolled-tab) border-box }` through a
            // transparent head: the metal shows only where the border is.
            assert_eq!(
                layers(active, dark, None),
                cut,
                "dark={dark}: the sunken stone over the rolled metal, its own colour in both modes"
            );
            assert_eq!(
                tc::background_clips(active, dark, None),
                boxes,
                "dark={dark}: the metal on the border box, every layer of the stone on the \
                 padding box"
            );
            assert_eq!(tc::text_color(active, dark), Some(flora::LIGHT_ON_ACC));
            assert_eq!(width(active, CssPropertyType::BorderTopWidth, dark), Some(2.0));
            assert_eq!(
                colour(active, CssPropertyType::BorderTopColor, dark, None),
                Some(ColorU::TRANSPARENT),
                "dark={dark}: a transparent head the metal shows through"
            );
            for w in [
                CssPropertyType::BorderLeftWidth,
                CssPropertyType::BorderRightWidth,
                CssPropertyType::BorderBottomWidth,
            ] {
                assert_eq!(
                    width(active, w, dark).unwrap_or(0.0),
                    0.0,
                    "dark={dark}: {w:?} - its sides are its curves, and it has no foot"
                );
            }
            // Open at its foot: it reaches down over the strip's rule and
            // hides it - no line between the stone and what it opens onto.
            assert_eq!(
                margin_bottom(active, dark, None),
                margin_px(-2),
                "dark={dark}: the selected tab breaks the rule"
            );
        }
        // Its sides: the two curves, each the S cut from the same rolled
        // metal, then the run-outs that ease the rule into the turn colour
        // beside each foot (`.fl-tab-runout-l/-r`).
        let kids = active.children.as_ref();
        assert_eq!(kids.len(), 5, "the label, the two curves, the two run-outs");
        for (i, class) in [(1usize, CURVE_LEFT_CLASS), (2, CURVE_RIGHT_CLASS)] {
            assert!(tc::has_class(&kids[i], class));
            let metal = tc::find(&kids[i], CURVE_STROKE_CLASS).expect("the S");
            assert_eq!(
                layers(metal, false, None),
                vec![tc::flora_css_rolled_tab()],
                "the S is the rolled metal - the bead the head is cut from"
            );
            assert_eq!(
                width(metal, CssPropertyType::BorderTopWidth, false).unwrap_or(0.0),
                0.0,
                "the S is the band of metal itself, not a flat stroke"
            );
            // The stone inside the S stands where the middle's does: below
            // the 2px head, so the three boxes meet without a seam.
            let fill = tc::find(&kids[i], CURVE_FILL_CLASS).expect("the stone inside the S");
            assert_eq!(
                tc::background_clips(fill, false, None),
                vec![StyleBackgroundClip::ContentBox]
            );
            assert_eq!(
                tc::resolve(fill, CssPropertyType::PaddingTop, false, None),
                Some(CssProperty::const_padding_top(LayoutPaddingTop::const_px(2)))
            );
        }
        for (i, class, left) in [
            (3usize, "__azul-native-tab-runout-left", true),
            (4, "__azul-native-tab-runout-right", false),
        ] {
            assert!(tc::has_class(&kids[i], class), "child {i} is the {class}");
            assert_eq!(layers(&kids[i], false, None), vec![tc::flora_css_runout(left)]);
        }
        assert!(
            tc::find(&bar(UiTheme::Flat), CURVE_LEFT_CLASS).is_none(),
            "flat's native tabs are boxes"
        );
    }

    #[test]
    fn flora_tabs_answer_the_pointer_and_ring_on_focus_in_both_modes() {
        let dom = bar(UiTheme::Flora);
        let hover = Some(PseudoStateType::Hover);
        for dark in [false, true] {
            for i in [1usize, 3, 4] {
                let tab = child(&dom, i);
                assert_ne!(
                    layers(tab, dark, hover),
                    layers(tab, dark, None),
                    "dark={dark}: tab {i} lifts under the pointer"
                );
                assert_eq!(
                    tc::resolve(tab, CssPropertyType::TextColor, dark, hover),
                    Some(azul_css::props::property::CssProperty::const_text_color(
                        azul_css::props::style::StyleTextColor {
                            inner: if dark { flora::DARK_INK } else { flora::LIGHT_INK },
                        }
                    )),
                    "dark={dark}: tab {i}'s label darkens to the ink under the pointer"
                );
                assert_eq!(
                    margin_bottom(tab, dark, hover),
                    margin_px(0),
                    "dark={dark}: a hovered tab still stands on the rule, which stays in sight"
                );
                // `.nav-links a { border: 1px solid transparent; border-bottom:
                // none }`, `:hover { border-color: var(--fl-bd) }`, `:active {
                // border-color: var(--fl-bd3) }`: a hairline comes up round
                // the lifted face, open at its foot on the rule.
                for (w, px) in [
                    (CssPropertyType::BorderTopWidth, 1.0),
                    (CssPropertyType::BorderLeftWidth, 1.0),
                    (CssPropertyType::BorderRightWidth, 1.0),
                    (CssPropertyType::BorderBottomWidth, 0.0),
                ] {
                    assert_eq!(
                        width(tab, w, dark).unwrap_or(0.0),
                        px,
                        "dark={dark}: tab {i}'s {w:?}"
                    );
                }
                let edge = CssPropertyType::BorderTopColor;
                assert_eq!(colour(tab, edge, dark, None), Some(ColorU::TRANSPARENT));
                assert_eq!(
                    colour(tab, edge, dark, hover),
                    Some(if dark { flora::DARK_BD } else { flora::LIGHT_BD }),
                    "dark={dark}: tab {i}'s hairline under the pointer"
                );
                assert_eq!(
                    colour(tab, edge, dark, Some(PseudoStateType::Active)),
                    Some(if dark { flora::DARK_BD3 } else { flora::LIGHT_BD3 }),
                    "dark={dark}: tab {i}'s hairline while pressed"
                );
            }
            // The arrow keys move focus to ANY tab, so every tab rings.
            for i in 1usize..=4 {
                assert!(
                    tc::has_focus_ring(child(&dom, i), dark),
                    "dark={dark}: tab {i} shows no focus ring"
                );
            }
        }
        tc::assert_theme_invariants("flora tab bar", &dom);
    }

    /// The tabs move as flora.css's nav links do - the declaration that
    /// wins in the stylesheet, `.nav-links a { transition: background
    /// var(--fl-dur) var(--fl-ease), border-color .., color .., box-shadow
    /// .. }` with `:active { transition-duration: var(--fl-dur-fast) }`, not
    /// the `0.2s ease` it overrides - and the light falls into the selected
    /// stone at the shafts' pace (`--fl-dur-ray`, 1.8s ease-in-out).
    #[test]
    fn flora_tabs_fade_as_the_websites_nav_links_do() {
        let dom = bar(UiTheme::Flora);
        for i in [1usize, 3, 4] {
            let tab = child(&dom, i);
            let rest = tc::fades(tab, None);
            for name in [
                "background",
                "border-top-color",
                "border-left-color",
                "border-right-color",
                "color",
                "-azul-box-shadow-top",
            ] {
                assert!(
                    rest.contains(&(String::from(name), flora::FL_DUR_MS, flora::FL_EASE)),
                    "tab {i} fades its {name} over --fl-dur on --fl-ease: {rest:?}"
                );
            }
            let pressed = tc::fades(tab, Some(PseudoStateType::Active));
            assert!(!pressed.is_empty(), "tab {i} declares its press");
            assert!(
                pressed.iter().all(|(_, ms, _)| *ms == flora::FL_DUR_FAST_MS),
                "tab {i} takes a press in --fl-dur-fast: {pressed:?}"
            );
        }
        assert_eq!(
            tc::fades(child(&dom, 2), None),
            vec![(String::from("background"), 1800, AnimationTiming::EaseInOut)],
            "the selected stone's light"
        );
    }

    #[test]
    fn a_flora_tab_panel_is_a_leaf_open_at_the_top() {
        let padded = panel(UiTheme::Flora, true);
        let bare = panel(UiTheme::Flora, false);
        assert!(tc::has_class(&padded, FLORA), "the panel carries flora's marker");
        assert!(!tc::has_class(&panel(UiTheme::Flat, true), FLORA));
        for dark in [false, true] {
            let (leaf, rule) = if dark {
                (flora::DARK_SUR, flora::DARK_BD)
            } else {
                (flora::LIGHT_SUR, flora::LIGHT_BD)
            };
            for node in [&padded, &bare] {
                assert_eq!(
                    tc::background(node, dark).as_ref().and_then(tc::bg_color),
                    Some(leaf),
                    "dark={dark}: the leaf"
                );
            }
            for c in [
                CssPropertyType::BorderLeftColor,
                CssPropertyType::BorderRightColor,
                CssPropertyType::BorderBottomColor,
            ] {
                assert_eq!(colour(&padded, c, dark, None), Some(rule), "dark={dark}: {c:?}");
            }
            assert_eq!(
                width(&padded, CssPropertyType::BorderTopWidth, dark),
                None,
                "dark={dark}: open at the top, where the tab's rule closes it"
            );
        }
        assert!(
            tc::resolve(&bare, CssPropertyType::PaddingTop, false, None).is_none(),
            "an unpadded panel declares no padding"
        );
        assert!(tc::resolve(&padded, CssPropertyType::PaddingTop, false, None).is_some());
        tc::assert_theme_invariants("flora tab panel", &padded);
    }

    /// `dom` without the selected tab's curves and run-outs, at any depth.
    fn without_curves(dom: &Dom) -> Dom {
        let mut d = dom.clone();
        d.children = DomVec::from_vec(
            dom.children
                .as_ref()
                .iter()
                .filter(|c| {
                    ![
                        CURVE_LEFT_CLASS,
                        CURVE_RIGHT_CLASS,
                        "__azul-native-tab-runout-left",
                        "__azul-native-tab-runout-right",
                    ]
                    .iter()
                    .any(|class| tc::has_class(c, class))
                })
                .map(without_curves)
                .collect(),
        );
        d
    }

    /// The tabs are the widget's in both looks; flora's selected tab only
    /// adds its two curves and their run-outs.
    #[test]
    fn both_looks_build_the_same_tabs_datasets_and_accessibility_tree() {
        let (flat, flora) = (bar(UiTheme::Flat), without_curves(&bar(UiTheme::Flora)));
        let (a, b) = (tc::nodes(&flat), tc::nodes(&flora));
        assert_eq!(a.len(), b.len(), "the same tree of nodes");
        assert_eq!(tc::a11y_outline(&flat), tc::a11y_outline(&flora));
        for ((path, x), (_, y)) in a.iter().zip(b.iter()).skip(1) {
            assert_eq!(
                x.root.get_ids_and_classes(),
                y.root.get_ids_and_classes(),
                "{path}: the same classes (the arrow keys find the tabs by them)"
            );
            assert_eq!(
                x.root.get_callbacks().as_ref().len(),
                y.root.get_callbacks().as_ref().len(),
                "{path}: the same click and arrow-key handlers"
            );
            assert_eq!(
                x.root.get_dataset().is_some(),
                y.root.get_dataset().is_some(),
                "{path}: the same datasets"
            );
        }
        let (fp, flp) = (panel(UiTheme::Flat, true), panel(UiTheme::Flora, true));
        assert_eq!(tc::nodes(&fp).len(), tc::nodes(&flp).len());
        assert_eq!(
            child(&fp, 0).root.get_ids_and_classes(),
            child(&flp, 0).root.get_ids_and_classes(),
            "the panel's classed wrapper is the same in both looks"
        );
    }
}

/// R5: the tab bar's and the panel's STRUCTURE (display, flex, box-sizing,
/// cursor, ...) is their base - declared once, outside every
/// `@theme(<name>)` block, so it holds under flat, flora and any theme to
/// come. What a theme owns is its skin: paint and metrics.
#[cfg(test)]
mod structure_tests {
    use azul_core::dom::Dom;
    use azul_css::{AzString, StringVec};

    use super::{TabContent, TabHeader};
    use crate::widgets::themes::{
        theme_blocks::checks::{under, BOTH},
        theme_checks::assert_structure_is_shared,
    };

    fn labels() -> StringVec {
        StringVec::from_vec(vec![
            AzString::from("General"),
            AzString::from("Colours"),
            AzString::from("Fonts"),
            AzString::from("Layout"),
            AzString::from("About"),
        ])
    }

    #[test]
    fn a_tab_bar_declares_its_structure_once_for_every_theme() {
        use azul_css::props::property::CssPropertyType;
        // What the two looks lay out differently by design (`tabs::HEADER_BASE`).
        let allowed = [
            (
                "__azul-native-tabs-header",
                CssPropertyType::AlignItems,
                "flora stands its tabs on the strip's rule (end); flat's native tabs hang from \
                 the top of the bar",
            ),
            (
                "__azul-native-tabs-before-tabs",
                CssPropertyType::FlexGrow,
                "flat's leading spacer grows (1); flora's tabs start a fixed curve's width in (0)",
            ),
            (
                "__azul-native-tabs-tab-active",
                CssPropertyType::Position,
                "flora hangs the Australis curves off its selected tab",
            ),
        ];
        for t in BOTH {
            // Each tab active in turn: the active tab, the two seam tabs
            // beside it and the other inactive tabs.
            for active in 0..5 {
                let dom = under(t, || TabHeader::create(labels()).with_active_tab(active).dom());
                assert_structure_is_shared(
                    &format!("tab bar (tab {active} active), built for {}", t.name()),
                    &dom,
                    &allowed,
                );
            }
        }
    }

    #[test]
    fn a_tab_panel_declares_its_structure_once_for_every_theme() {
        for t in BOTH {
            for padding in [true, false] {
                let dom = under(t, || {
                    TabContent::new(Dom::create_div()).with_padding(padding).dom()
                });
                assert_structure_is_shared(
                    &format!("tab panel (padded: {padding}), built for {}", t.name()),
                    &dom,
                    &[],
                );
            }
        }
    }
}
