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
    timer::{Timer, TimerCallback},
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
    /// Empty hangs none on that side - where a neighbouring stone stands
    /// over the rule, there is no rule to ease.
    pub(crate) runout_left: CssPropertyWithConditionsVec,
    /// The right run-out's paint; empty hangs none.
    pub(crate) runout_right: CssPropertyWithConditionsVec,
}

impl TabCurveLook {
    /// This look without the run-out beside its left foot: the tab stands
    /// right after another stone (the ribbon's application button), which
    /// covers the rule there.
    #[must_use]
    pub(crate) fn after_a_stone(&self) -> Self {
        Self {
            runout_left: CssPropertyWithConditionsVec::from_const_slice(&[]),
            ..self.clone()
        }
    }
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

/// The point of the cubic `curve` at `t`, and the tangent there.
fn cubic_at(curve: &SvgCubicCurve, t: f32) -> (SvgPoint, (f32, f32)) {
    let u = 1.0 - t;
    let (p0, p1, p2, p3) = (curve.start, curve.ctrl_1, curve.ctrl_2, curve.end);
    // The Bernstein weights of the point, then those of its derivative.
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    let point = SvgPoint {
        x: w0 * p0.x + w1 * p1.x + w2 * p2.x + w3 * p3.x,
        y: w0 * p0.y + w1 * p1.y + w2 * p2.y + w3 * p3.y,
    };
    let (d0, d1, d2) = (3.0 * u * u, 6.0 * u * t, 3.0 * t * t);
    let tangent = (
        d0 * (p1.x - p0.x) + d1 * (p2.x - p1.x) + d2 * (p3.x - p2.x),
        d0 * (p1.y - p0.y) + d1 * (p2.y - p1.y) + d2 * (p3.y - p2.y),
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
    // The curve, its fill and its metal are drawn as part of the tab: they
    // take its hover and press, so a pressed stone sinks to its feet.
    let fill = Dom::create_div()
        .with_part_of_parent(true)
        .with_ids_and_classes(decl::classes(&[CURVE_FILL_CLASS]))
        .with_css_props(over(if left {
            &look.left_fill
        } else {
            &look.right_fill
        }))
        .with_svg_data(shape(curve_fill(s, w, h, left)));
    let metal = Dom::create_div()
        .with_part_of_parent(true)
        .with_ids_and_classes(decl::classes(&[CURVE_STROKE_CLASS]))
        .with_css_props(over(&look.metal))
        .with_svg_data(shape(curve_band(s, look.gauge)));
    let side = if left {
        decl::simple(CssProperty::const_left(LayoutLeft::px(-w)))
    } else {
        decl::simple(CssProperty::const_right(LayoutRight::px(-w)))
    };
    Dom::create_div()
        .with_part_of_parent(true)
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
        .with_part_of_parent(true)
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
        for (left, paint) in [(true, &look.runout_left), (false, &look.runout_right)] {
            if !paint.as_ref().is_empty() {
                parts.push(runout(look, left));
            }
        }
    }
    parts
}

#[cfg(test)]
mod australis_tests {
    use azul_core::{dom::SvgNodeData, svg::SvgPathElement};

    use super::*;

    fn look() -> TabCurveLook {
        let paint = || {
            CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::simple(
                CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
                    StyleBackgroundContent::Color(ColorU::rgb(0xC6, 0xB2, 0x79)),
                ])),
            )])
        };
        TabCurveLook {
            height: 28.0,
            gauge: 2.0,
            left_fill: CssPropertyWithConditionsVec::from_const_slice(&[]),
            right_fill: CssPropertyWithConditionsVec::from_const_slice(&[]),
            metal: CssPropertyWithConditionsVec::from_const_slice(&[]),
            runout: 34.0,
            runout_left: paint(),
            runout_right: paint(),
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
        // After another stone, which stands over the rule on that side, only
        // the right run-out is hung.
        let after = australis_curves(&look().after_a_stone());
        assert_eq!(after.len(), 3, "two curves, the right run-out");
        assert!(after[2]
            .root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, IdOrClass::Class(s) if s.as_str() == RUNOUT_RIGHT_CLASS)));
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
static STYLE_BACKGROUND_CONTENT_8560341490937422656_ITEMS: &[StyleBackgroundContent] =
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

static STYLE_BACKGROUND_CONTENT_16746671892555275291_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::Color(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    })];
static STYLE_FONT_FAMILY_8122988506401935406_ITEMS: &[StyleFontFamily] =
    &[StyleFontFamily::System(STRING_16146701490593874959)];
static LINEAR_COLOR_STOP_1400070954008106244_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset_px: FloatValue::const_new(0),
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 240,
            g: 240,
            b: 240,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset_px: FloatValue::const_new(0),
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 229,
            g: 229,
            b: 229,
            a: 255,
        }),
    },
];

static CSS_MATCH_13824480602841492081_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_14575853790110873394_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_17290739305197504468_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_18014909903571752977_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_3088386549906605418_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_4415083954137121609_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_4738503469417034630_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_9988039989460234263_PROPERTIES: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_4738503469417034630_PROPERTIES_NO_PADDING: &[CssPropertyWithConditions] = &[
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

static CSS_MATCH_11510695043643111367_PROPERTIES: &[CssPropertyWithConditions] = &[
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

/// The shape of a tab bar's tabs ([`TabHeader::with_style`]).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TabStyle {
    /// The theme's buttons floating on the leather strip, in every theme: [`Self::Pills`] on a
    /// desktop and [`Self::TouchPills`] on a phone.
    #[default]
    Auto,
    /// Classic square tabs, the selected one open onto the panel below.
    Rectangular,
    /// The theme's buttons ("Buttons" in a settings UI) floating on the leather strip: the
    /// selected tab is the submit button, the others the plain button, 22px tall at the house
    /// radius (slightly rounded corners, no capsule ends) in a 28px row (a title bar's) on a
    /// leather strip that closes on a 2px golden line. A row longer than its box scrolls inside
    /// it (the wheel, the arrow keys, a drag; no scrollbar), the selected button comes into view
    /// by itself, and a fade in the leather's colour lies over each end of the row (a bar with a
    /// reveal keeps a row that does not scroll).
    Pills,
    /// The same buttons for a finger, the look `Auto` takes on a phone: the selected tab is the
    /// submit button, the others the plain button, 32 px tall at the house radius in a 48 px
    /// row, floating on the leather strip, each with a hit area of the whole row (the target is
    /// padded, not the picture). The row is longer than the window: it scrolls sideways under a
    /// finger (momentum), the wheel and the keys, without a scrollbar, and the selected button
    /// comes into view by itself. In the status bar's band (`TabHeader::with_status_bar`) it
    /// slides around the cutouts of the display.
    TouchPills,
}

impl TabStyle {
    /// This style under `theme`, with `Auto` answered by the theme.
    #[must_use]
    pub const fn resolved(self, theme: crate::widgets::themes::UiTheme) -> Self {
        use crate::widgets::themes::UiTheme;
        match (self, theme) {
            (Self::Auto, UiTheme::Flora | UiTheme::Flat) => Self::Pills,
            (other, _) => other,
        }
    }

    /// [`Self::resolved`], with `Auto` answering a PHONE's window (`phone`: a touch platform and
    /// a window narrower than [`PHONE_MAX_WIDTH`]) with the buttons at touch sizes instead of
    /// the desktop's. Any style named by the app stays.
    #[must_use]
    pub const fn resolved_for(self, theme: crate::widgets::themes::UiTheme, phone: bool) -> Self {
        match (self, phone) {
            (Self::Auto, true) => Self::TouchPills,
            (other, _) => other.resolved(theme),
        }
    }
}

/// The widest window, in logical px, that is a phone's: `TabStyle::Auto` is the touch pills in a
/// window narrower than this on a touch platform (Android, iOS).
pub const PHONE_MAX_WIDTH: f32 = 600.0;

/// Whether the DOM being built is for a phone: a touch platform (or, on a host with none, a window that
/// sits on a simulated phone, `WindowFlags::display_preset`), and a window narrower than
/// [`PHONE_MAX_WIDTH`] - the ONE rule the phone's widgets share (the touch pills, the icon pills, the
/// form factor of an app frame). A landscape phone (852 dp) and a tablet are wider and get the
/// desktop forms; a desktop with no preset never gets the phone's. Asked through the recorded size
/// queries, so a resize across the width builds the tab bar again.
pub(crate) fn phone_build() -> bool {
    (cfg!(any(target_os = "android", target_os = "ios"))
        || azul_core::callbacks::build_window_simulates_phone())
        && azul_core::callbacks::build_window_width_less_than(PHONE_MAX_WIDTH).unwrap_or(false)
}

/// Header bar for a tab widget, containing the clickable tab labels.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabHeader {
    /// Optional callback invoked when a tab is clicked.
    pub on_click: OptionTabOnClick,
    /// The strip in the status bar's band (`with_status_bar`): it takes the place of the clock
    /// and the battery icons and slides around the cutouts of the display. `None` (the default)
    /// keeps it a row of the layout.
    pub status_bar: crate::widgets::tab_strip::OptionStatusBarBand,
    /// Labels for each tab.
    pub tabs: StringVec,
    /// How the panel opens and closes (`with_reveal`), kept by the app
    /// between rebuilds; `None` keeps the active tab's panel always shown.
    pub reveal: crate::widgets::tab_reveal::OptionTabReveal,
    /// Zero-based index of the currently active tab. An index past the last
    /// tab selects none: no tab is drawn selected.
    pub active_tab: usize,
    /// The widget theme this tab bar is PINNED to (`with_theme`), or `None`
    /// to follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// The shape of the tabs (`with_style`); `Auto` is the theme's buttons on the leather.
    pub style: TabStyle,
    /// Whether a vertical swipe on the panel of this bar flips to the previous / next tab
    /// (`with_flip`, then `TabContent::with_flip_of`): `AtEdges` (the default) flips with a finger
    /// that pulls past the end of the content, never with a wheel.
    pub flip: crate::widgets::tab_flip::TabFlip,
    /// Whether this bar draws the window's chevron at the left end of its row (`with_content_toggle`).
    /// APPENDED for ABI stability.
    pub content_toggle: bool,
}

impl Default for TabHeader {
    fn default() -> Self {
        Self {
            tabs: StringVec::from_const_slice(&[]),
            active_tab: 0,
            on_click: None.into(),
            theme: crate::widgets::themes::OptionUiTheme::None,
            style: TabStyle::Auto,
            reveal: crate::widgets::tab_reveal::OptionTabReveal::None,
            status_bar: crate::widgets::tab_strip::OptionStatusBarBand::None,
            flip: crate::widgets::tab_flip::TabFlip::AtEdges,
            content_toggle: false,
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
            style: TabStyle::Auto,
            reveal: crate::widgets::tab_reveal::OptionTabReveal::None,
            status_bar: crate::widgets::tab_strip::OptionStatusBarBand::None,
            flip: crate::widgets::tab_flip::TabFlip::AtEdges,
            content_toggle: false,
        }
    }

    /// Draw the window's ONE chevron at the left end of this bar's row: `^` while the content of the
    /// tabs shows, `v` while it is folded away, a click (or Enter, or Space) folds or shows it. The state
    /// is the window's (`WindowFlags::tab_content_hidden`): every `TabContent` with `with_content_toggle`
    /// and every ribbon with it shows nothing while it is folded, and the list under the bar gets the room.
    /// A window has exactly one chevron: when more than one bar asks for it, the first one built has it
    /// and the others draw none.
    pub const fn set_content_toggle(&mut self, content_toggle: bool) {
        self.content_toggle = content_toggle;
    }

    /// [`Self::set_content_toggle`] for the builder chain.
    #[must_use]
    pub const fn with_content_toggle(mut self, content_toggle: bool) -> Self {
        self.set_content_toggle(content_toggle);
        self
    }

    /// Whether a vertical swipe on the panel flips to the previous / next tab: `Off`, `AtEdges`
    /// (a finger that pulls past the end of the content; the default) or `Always` (a wheel
    /// that keeps pushing against the end flips too). The panel follows the bar:
    /// `TabContent::with_flip_of(&bar)`. The pills stay the explicit way to choose a tab and the
    /// one assistive technology uses: a flip is a shortcut, never the only path.
    pub const fn set_flip(&mut self, flip: crate::widgets::tab_flip::TabFlip) {
        self.flip = flip;
    }

    /// [`Self::set_flip`] for the builder chain.
    #[must_use]
    pub const fn with_flip(mut self, flip: crate::widgets::tab_flip::TabFlip) -> Self {
        self.set_flip(flip);
        self
    }

    /// Put the strip in the status bar's band: the tab bar takes the place of the clock and the
    /// battery icons, in the band of `band` (`StatusBarBand::from_display`, made from
    /// `LayoutCallbackInfo::get_display_features`), and its pills slide around the cutouts of the
    /// display: the strip is the lanes either side of the camera, what lies under it is covered,
    /// and the selected pill always stands whole inside a lane. The window must draw into the
    /// band (`WindowFlags::extend_into_safe_area`, `system_bars.status` hidden): the strip is the
    /// first thing in the window, at its top edge. A window with no status bar and no cutout has
    /// no band, and the strip is an ordinary row. Only the touch pills (`TabStyle::TouchPills`,
    /// what `Auto` is on a phone) take a band.
    pub fn set_status_bar(&mut self, band: crate::widgets::tab_strip::StatusBarBand) {
        self.status_bar = crate::widgets::tab_strip::OptionStatusBarBand::Some(band);
    }

    /// [`Self::set_status_bar`] for the builder chain.
    #[must_use]
    pub fn with_status_bar(mut self, band: crate::widgets::tab_strip::StatusBarBand) -> Self {
        self.set_status_bar(band);
        self
    }

    /// How the panel opens and closes: the app makes `reveal` once
    /// (`TabReveal::create`, with its `TabClickBehaviour`), hands the same
    /// one to every build of this bar and of its `TabContent`, and builds
    /// the panel for the tab `on_click` reports. The bar feeds it the
    /// pointer, the clicks and Enter / Space (Shift+Enter, or a double
    /// Enter, locks), and draws no tab selected while the panel is closed.
    pub fn set_reveal(&mut self, reveal: crate::widgets::tab_reveal::TabReveal) {
        self.reveal = crate::widgets::tab_reveal::OptionTabReveal::Some(reveal);
    }

    /// [`Self::set_reveal`] for the builder chain.
    #[must_use]
    pub fn with_reveal(mut self, reveal: crate::widgets::tab_reveal::TabReveal) -> Self {
        self.set_reveal(reveal);
        self
    }

    /// The shape of the tabs: the theme's buttons floating on the leather (`Auto`, `Pills`,
    /// `TouchPills`) or square tabs (`Rectangular`).
    /// `Auto` (the default) is the buttons in every theme, at touch sizes on a phone.
    pub const fn set_style(&mut self, style: TabStyle) {
        self.style = style;
    }

    /// [`Self::set_style`] for the builder chain.
    #[must_use]
    pub const fn with_style(mut self, style: TabStyle) -> Self {
        self.set_style(style);
        self
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

        // The touch pills have a DOM of their own: a strip that scrolls inside a header, pills with
        // a padded hit area, covers over the cutouts. `Auto` is them on a phone.
        let content_toggle = self.content_toggle;
        let mut chevron_row = 28.0_f32;
        {
            let live = crate::widgets::themes::theme_blocks::live_theme(self.theme);
            let phone = self.style == TabStyle::Auto && phone_build();
            let resolved = self.style.resolved_for(live, phone);
            if resolved == TabStyle::TouchPills {
                return self.dom_touch(live);
            }
            if resolved == TabStyle::Pills {
                // The row of desktop pills scrolls when it is longer than the window; a bar with a
                // reveal keeps the row it had (the reveal's hover areas hang on its nodes).
                if self.reveal.is_none() {
                    let look = TabHeaderLook::of(self.theme, self.style);
                    return self.dom_row(live, &look);
                }
                #[allow(clippy::cast_precision_loss)] // a few px
                {
                    chevron_row = PILL_ROW as f32;
                }
            }
        }

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
        // The look's curves (the ribbon's row) and rule (the buttons) are nodes, so they are
        // in the tree when it is built for a theme and style that has them -
        // the pinned look, or the app theme the DOM is built for (a theme
        // switch rebuilds the DOM, as for every widget whose looks build
        // different trees).
        let look = TabHeaderLook::of(self.theme, self.style);
        let tab_count = self.tabs.as_ref().len();
        // An index past the last tab selects none: no tab is drawn selected,
        // and no neighbour of a missing selection gets its seam. A reveal
        // draws the shown tab selected while its panel is open, none while
        // it is closed.
        let active = match self.reveal.as_ref() {
            Some(reveal) => {
                reveal.begin_build(self.active_tab);
                reveal.get_selected().into_option().filter(|t| *t < tab_count)
            }
            None => (self.active_tab < tab_count).then_some(self.active_tab),
        };
        let reveal = self.reveal.into_option();
        let owner = crate::widgets::tab_reveal::RevealOwner::Header(self.on_click.clone());
        let on_click_is_some = self.on_click.is_some() || reveal.is_some();
        // WAI-ARIA APG: an interactive tab list is ONE Tab stop - the active
        // tab, or the first when the index is out of range. The arrow keys
        // move (and activate) within it. A header nobody can activate stays
        // out of the Tab order entirely.
        let tab_stop =
            crate::widgets::roving::stop_index(Some(self.active_tab), self.tabs.as_ref().len());

        let mut header = Dom::create_div()
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
                // The window's one chevron, at the left end of the row (the arrow keys find the tabs
                // by their classes, so a node before them does not move them).
                if content_toggle {
                    if let Some(chevron) = crate::widgets::tab_content_toggle::chevron_for_row(chevron_row) {
                        tab_items.insert(0, chevron);
                    }
                }

                let dataset = TabLocalDataset {
                    tab_idx: 0,
                    on_click: self.on_click,
                };

                for (tab_idx, tab) in self.tabs.as_ref().iter().enumerate() {
                    let next_tab_is_active = active == Some(tab_idx.saturating_add(1));
                    let previous_tab_was_active = match active {
                        Some(0) | None => false,
                        Some(a) => a == tab_idx.saturating_sub(1),
                    };

                    let tab_is_active = active == Some(tab_idx);

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

                    let arrows = CoreCallbackData {
                        event: EventFilter::Focus(
                            azul_core::events::FocusEventFilter::VirtualKeyDown,
                        ),
                        callback: CoreCallback {
                            cb: on_tab_key as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: dataset.clone(),
                    };
                    let mut tab_dom = crate::widgets::widget_p_with_text(tab.clone())
                        .with_callbacks(match reveal.as_ref() {
                            // The reveal hears the pointer, the clicks and
                            // Enter; the arrows still move the Tab stop.
                            Some(reveal) => {
                                let mut v = crate::widgets::tab_reveal::tab_callbacks(
                                    tab_idx, reveal, &owner, true,
                                );
                                v.push(arrows);
                                v.into()
                            }
                            None if on_click_is_some => vec![
                                CoreCallbackData {
                                    event: EventFilter::Hover(HoverEventFilter::Click),
                                    callback: CoreCallback {
                                        cb: on_tab_click as usize,
                                        ctx: azul_core::refany::OptionRefAny::None,
                                    },
                                    refany: dataset.clone(),
                                },
                                arrows,
                            ]
                            .into(),
                            None => CoreCallbackDataVec::from_const_slice(&[]),
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

                // The rule along the strip's foot, last so it lies over the
                // strip and under nothing.
                if let Some(rule) = look.rule.as_ref() {
                    const RULE_CLASSES: &[IdOrClass] =
                        &[Class(AzString::from_const_str(RULE_CLASS))];
                    let mut rule_dom = Dom::create_div()
                        .with_css_props(rule.clone())
                        .with_ids_and_classes(IdOrClassVec::from_const_slice(RULE_CLASSES));
                    if !look.rule_css.is_empty() {
                        rule_dom.root.set_css(look.rule_css);
                    }
                    tab_items.push(rule_dom);
                }

                tab_items.into()
            });
        // The custom properties the strip's material reads (the leather's
        // tint), after its style: `with_css_props` replaces a node's style.
        if !look.header_css.is_empty() {
            header.root.set_css(look.header_css);
        }
        // The bar is half of the reveal's hover area; the panel the other.
        if let Some(reveal) = reveal.as_ref() {
            header = header.with_callbacks(
                crate::widgets::tab_reveal::area_callbacks(reveal, &owner).into(),
            );
        }
        header
    }
}

/// The class of the scrolling row of a strip that sits in the status bar's band: the one handle a
/// scenario or a test finds the band's row by, whatever other strips the page has.
pub const BAND_STRIP_CLASS: &str = "__azul-native-tabs-touch-band";

/// The classes of the touch strip's header, its scrolling row and its pills.
static TOUCH_HEADER_CLASSES: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tabs-touch-header"))];
static TOUCH_STRIP_CLASSES: &[IdOrClass] =
    &[Class(AzString::from_const_str("__azul-native-tabs-touch-strip"))];
static TOUCH_ACTIVE_CLASSES: &[IdOrClass] = &[
    Class(AzString::from_const_str(TAB_ACTIVE_CLASS_NAME)),
    Class(AzString::from_const_str("__azul-native-tabs-touch-pill")),
];
static TOUCH_INACTIVE_CLASSES: &[IdOrClass] = &[
    Class(AzString::from_const_str(TAB_NOT_ACTIVE_CLASS_NAME)),
    Class(AzString::from_const_str("__azul-native-tabs-touch-pill")),
];

/// What the strip's scroll-into-view needs, handed to the marker in the selected pill: the room
/// before the first pill and after the last, the room between pills, and the lanes of the band
/// (the strip's own coordinates; none = the whole view).
#[derive(Clone)]
pub(crate) struct StripScrollData {
    /// The room before the first pill and after the last, px.
    pub(crate) lead: f32,
    pub(crate) trail: f32,
    /// The room between two pills, px.
    pub(crate) gap: f32,
    /// The lanes of the band in the strip's coordinates; none = the whole view.
    pub(crate) lanes: Vec<(f32, f32)>,
    /// The classes that mark a pill among the strip's children (the tabs, the application button of
    /// a ribbon): what the scroll-into-view counts and measures.
    pub(crate) pill_classes: Vec<String>,
    /// Whether the strip keeps its pills out of the gaps of the band at rest by writing room in front
    /// of the ones that would stand in one ([`crate::widgets::tab_strip::room_around_cutouts`]).
    /// `false` for a strip whose pills are placed by other means (a row of icons at positions of its
    /// own).
    pub(crate) camera_room: bool,
}

impl StripScrollData {
    /// The numbers of a strip in `band` (none: an ordinary row), whose pills carry one of
    /// `pill_classes`: the first pill stands [`TOUCH_LEAD`] from the first lane's edge, the last the
    /// same from the last lane's.
    pub(crate) fn for_band(
        band: Option<&crate::widgets::tab_strip::StatusBarBand>,
        pill_classes: Vec<String>,
    ) -> Self {
        #[allow(clippy::cast_precision_loss)] // a few px
        let (lead_px, gap_px) = (TOUCH_LEAD as f32, TOUCH_GAP as f32);
        let lanes = band.map(crate::widgets::tab_strip::StatusBarBand::lane_spans).unwrap_or_default();
        let width = band.map_or(0.0, |b| b.width);
        Self {
            lead: lanes.first().map_or(0.0, |lane| lane.0) + lead_px,
            trail: lanes.last().map_or(0.0, |lane| (width - lane.1).max(0.0)) + lead_px,
            gap: gap_px,
            lanes,
            pill_classes,
            camera_room: true,
        }
    }
}

/// The transparent box that makes a pill's target the whole row: 4 px beside it (half the gap
/// to the next pill) and 8 px above and below it, so a 32 px pill answers a finger on a 48 px
/// row. The picture is the pill; only the target is padded.
pub(crate) fn hit_extender() -> Dom {
    Dom::create_div().with_css("position: absolute; left: -4px; right: -4px; top: -8px; bottom: -8px;")
}

/// The class of the marker hung in the selected pill: what finds the selected pill again when the
/// strip is resized (the marker's own `AfterMount` knows its pill by being in it).
const SELECTED_MARKER_CLASS: &str = "__azul-tabs-selected-marker";

static SELECTED_MARKER_CLASSES: &[IdOrClass] = &[Class(AzString::from_const_str(SELECTED_MARKER_CLASS))];

/// The marker hung in the SELECTED pill, keyed by its tab and by the band's lanes: a new selection, or a
/// band of other lanes (another phone, a turn), mounts a new marker, and its `AfterMount` gives the pills
/// their room around the cutouts and scrolls the strip so that the selected pill stands whole in a lane -
/// whoever changed the selection (a tap, an arrow key, a flip of the panel, the app).
pub(crate) fn selected_marker(data: StripScrollData, tab: usize) -> Dom {
    // What the room of the pills was planned for (FNV-1a over the numbers): a strip with other lanes
    // plans again.
    let lanes = data
        .lanes
        .iter()
        .flat_map(|(left, right)| [left.to_bits(), right.to_bits()])
        .chain([data.lead.to_bits(), u32::from(data.camera_room)])
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, word| (hash ^ u64::from(word)).wrapping_mul(0x0100_0000_01b3));
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(SELECTED_MARKER_CLASSES))
        .with_css("position: absolute; left: 0px; top: 0px; width: 0px; height: 0px;")
        .with_key(("azul-tab-selected", tab, lanes))
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Component(azul_core::dom::ComponentEventFilter::AfterMount),
                callback: CoreCallback {
                    cb: on_selected_pill_mounted as usize,
                    ctx: azul_core::refany::OptionRefAny::None,
                },
                refany: RefAny::new(data),
            }]
            .into(),
        )
}

/// The strip's own `NodeResized`: a window of another width (the phone turned) moves the lanes of the
/// band and the room of the row, so the selected pill is brought into a lane again.
pub(crate) fn strip_resized_callbacks(data: StripScrollData) -> azul_core::callbacks::CoreCallbackDataVec {
    vec![CoreCallbackData {
        event: EventFilter::Component(azul_core::dom::ComponentEventFilter::NodeResized),
        callback: CoreCallback {
            cb: on_strip_resized as usize,
            ctx: azul_core::refany::OptionRefAny::None,
        },
        refany: RefAny::new(data),
    }]
    .into()
}

/// A cover over a gap of the status bar's band: the window's background, over the pills, so
/// that a pill sliding into the gap is hidden by it and never drawn under the camera.
fn gap_cover(rect: &azul_core::geom::LogicalRect) -> Dom {
    Dom::create_div().with_css(&format!(
        "position: absolute; left: {}px; top: 0px; width: {}px; height: {}px; \
         background: system:window-background;",
        rect.origin.x, rect.size.width, rect.size.height
    ))
}

/// `AfterMount` of the marker in the selected pill: scroll the strip so that the pill is whole
/// in a lane ([`crate::widgets::tab_strip::scroll_for_pill`]).
extern "C" fn on_selected_pill_mounted(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let marker = info.get_hit_node();
    let Some(pill) = info.get_parent(marker) else {
        return Update::DoNothing;
    };
    let Some(strip) = info.get_parent(pill) else {
        return Update::DoNothing;
    };
    scroll_selected_into_lane(&mut data, &mut info, strip, Some(pill))
}

/// `NodeResized` of the strip: the same, for the pill that carries the marker.
extern "C" fn on_strip_resized(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let strip = info.get_hit_node();
    scroll_selected_into_lane(&mut data, &mut info, strip, None)
}

/// Scroll `strip` so that its selected pill stands whole in a lane: `pill` is the selected one when
/// the caller knows it, else the one that carries the marker.
fn scroll_selected_into_lane(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    strip: azul_core::dom::DomNodeId,
    pill: Option<azul_core::dom::DomNodeId>,
) -> Update {
    use azul_core::geom::LogicalPosition;

    use crate::widgets::{roving, tab_strip};

    let Some(guard) = data.downcast_ref::<StripScrollData>() else {
        return Update::DoNothing;
    };
    let data = (*guard).clone();
    drop(guard);

    let Some(strip_id) = strip.node.into_crate_internal() else {
        return Update::DoNothing;
    };
    let tabs: Vec<azul_core::dom::DomNodeId> = roving::children_of(info, strip)
        .into_iter()
        .filter(|n| data.pill_classes.iter().any(|class| roving::has_class(info, *n, class)))
        .collect();
    let Some(view) = info.get_node_size(strip).map(|size| size.width) else {
        return Update::DoNothing;
    };
    let widths: Vec<f32> = tabs
        .iter()
        .map(|tab| info.get_node_size(*tab).map_or(0.0, |size| size.width))
        .collect();
    // Nothing laid out yet (a mount before the first frame of the strip): nothing to place or scroll.
    if view <= 0.0 || widths.iter().any(|width| *width <= 0.0) {
        return Update::DoNothing;
    }
    // At rest no pill stands under a camera: the pill that would starts the next lane, which is room in
    // front of it, written as the pill's own left margin (a write at run time follows the node across
    // the app's rebuilds). A strip with no camera in it asks for none, and the same write takes away
    // the room that a window of another shape had given.
    #[allow(clippy::cast_precision_loss)] // a few px
    let room = if data.camera_room {
        tab_strip::room_around_cutouts(&widths, data.lead, data.gap, &data.lanes, TOUCH_LEAD as f32)
    } else {
        vec![0.0; widths.len()]
    };
    if data.camera_room {
        for (tab, extra) in tabs.iter().zip(&room) {
            info.set_css_property(*tab, CssProperty::margin_left(LayoutMarginLeft::px(*extra)));
        }
    }
    let selected = pill.or_else(|| {
        tabs.iter().copied().find(|tab| {
            roving::children_of(info, *tab)
                .into_iter()
                .any(|child| roving::has_class(info, child, SELECTED_MARKER_CLASS))
        })
    });
    let Some(index) = selected.and_then(|pill| tabs.iter().position(|n| *n == pill)) else {
        return Update::DoNothing;
    };
    // Where each pill stands on the strip: from the room before the first, the widths and the room
    // just given, so the answer does not depend on how the engine reports positions of scrolled
    // content.
    let mut spans = Vec::with_capacity(tabs.len());
    let mut x = data.lead;
    for (width, extra) in widths.iter().zip(&room) {
        x += extra;
        spans.push((x, *width));
        x += width + data.gap;
    }
    let content = x - data.gap + data.trail;
    let max = tab_strip::max_scroll(content, view);
    let now = info
        .get_scroll_offset_for_node(strip.dom, strip_id)
        .map_or(0.0, |offset| offset.x);
    let lanes = if data.lanes.is_empty() {
        vec![(0.0, view)]
    } else {
        data.lanes
    };
    let target = tab_strip::scroll_for_pill(spans[index], now, max, &lanes);
    if (target - now).abs() >= 0.5 {
        info.scroll_to_animated(strip.dom, strip_id, LogicalPosition { x: target, y: 0.0 });
    }
    Update::DoNothing
}

/// The leather one stretch of a touch strip lies on - `span` (`left`, `right` in the strip's own
/// coordinates), or the whole row for `None` - with the golden line along its foot: the Pills look of
/// `look`, the theme's buttons on a leather strip that closes on a 2px golden line. In the status bar's
/// band the camera sits between two of them, where the window shows through.
fn leather_lane(look: &TabHeaderLook, span: Option<(f32, f32)>) -> Dom {
    use crate::widgets::themes::decl;

    let leather = || StyleBackgroundContent::Image(AzString::from_const_str("azul-builtin:leather"));
    let layers = decl::themed_layers(
        vec![StyleBackgroundContent::Color(ColorU::rgb(0xF4, 0xF0, 0xE8)), leather()],
        vec![StyleBackgroundContent::Color(ColorU::rgb(0x3A, 0x32, 0x2C)), leather()],
    );
    let mut lane = Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(layers.to_vec()));
    let place = match span {
        Some((left, right)) => format!("left: {left}px; width: {}px;", (right - left).max(0.0)),
        None => "left: 0px; width: 100%;".to_string(),
    };
    // The custom properties the leather's material reads (its tint), after its style.
    lane.root
        .set_css(&format!("position: absolute; top: 0px; bottom: 0px; {place} {}", look.header_css));
    if let Some(rule) = look.rule.as_ref() {
        let mut line = Dom::create_div()
            .with_css_props(rule.clone())
            .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(AzString::from_const_str(RULE_CLASS))]));
        if !look.rule_css.is_empty() {
            line.root.set_css(look.rule_css);
        }
        lane.add_child(line);
    }
    lane
}

/// The leather under a touch row: one stretch per lane of `band`, the whole row without a band.
fn leather_under(look: &TabHeaderLook, band: Option<&crate::widgets::tab_strip::StatusBarBand>) -> Vec<Dom> {
    match band.map(crate::widgets::tab_strip::StatusBarBand::lane_spans) {
        Some(lanes) if !lanes.is_empty() => {
            lanes.into_iter().map(|lane| leather_lane(look, Some(lane))).collect()
        }
        _ => vec![leather_lane(look, None)],
    }
}

/// The width of the fade at the end of a lane: the room the first pill keeps from the lane's edge
/// ([`TOUCH_LEAD`]), so at rest the fade lies over empty leather and a pill is dimmed only while it
/// slides out of the lane.
const LANE_FADE_PX: f32 = TOUCH_LEAD as f32;

/// A horizontal two-stop gradient, `from` at the left to `to` at the right: one layer.
fn side_fade(from: ColorU, to: ColorU) -> StyleBackgroundContent {
    StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Left,
            dir_to: DirectionCorner::Right,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_vec(vec![
            NormalizedLinearColorStop::new(PercentageValue::const_new(0), from),
            NormalizedLinearColorStop::new(PercentageValue::const_new(100), to),
        ]),
    })
}

/// The fades at the two ends of every lane of a touch row (the whole row without a band): the
/// leather's colour, opaque at the lane's edge and clear [`LANE_FADE_PX`] in, over the pills and
/// above the golden line. A pill that slides out of the lane (under a camera cover, off the
/// window's edge) fades out instead of being cut, and with no more pills to scroll to nothing shows:
/// the first and the last pill stand [`TOUCH_LEAD`] in. No "more" pill, no scrollbar.
pub(crate) fn lane_fades(
    band: Option<&crate::widgets::tab_strip::StatusBarBand>,
    row: f32,
    first_left_room: f32,
) -> Vec<Dom> {
    lane_fades_sized(LANE_FADE_PX, band, row, first_left_room)
}

/// [`lane_fades`] with fades `fade_px` wide: the room the first pill keeps from the end of the row.
pub(crate) fn lane_fades_sized(
    fade_px: f32,
    band: Option<&crate::widgets::tab_strip::StatusBarBand>,
    row: f32,
    first_left_room: f32,
) -> Vec<Dom> {
    use crate::widgets::themes::decl;

    #[allow(clippy::cast_precision_loss)] // 2px
    let height = (row - PILL_RULE as f32).max(0.0);
    let fade = |left_side: bool, at: Option<f32>| {
        let (light, dark) = (ColorU::rgb(0xF4, 0xF0, 0xE8), ColorU::rgb(0x3A, 0x32, 0x2C));
        let clear = |c: ColorU| ColorU::new(c.r, c.g, c.b, 0);
        let layer = |c: ColorU| {
            if left_side {
                side_fade(c, clear(c))
            } else {
                side_fade(clear(c), c)
            }
        };
        let layers = decl::themed_layers(vec![layer(light)], vec![layer(dark)]);
        let place = match (left_side, at) {
            (_, Some(x)) => format!("left: {x}px;"),
            (true, None) => "left: 0px;".to_string(),
            (false, None) => "right: 0px;".to_string(),
        };
        let mut node = Dom::create_div().with_css_props(CssPropertyWithConditionsVec::from_vec(layers.to_vec()));
        node.root.set_css(&format!(
            "position: absolute; top: 0px; width: {fade_px}px; height: {height}px; {place}"
        ));
        node
    };
    match band.map(crate::widgets::tab_strip::StatusBarBand::lane_spans) {
        Some(lanes) if !lanes.is_empty() => lanes
            .into_iter()
            .enumerate()
            .flat_map(|(i, (left, right))| {
                // The first lane's left end is the chevron's: its fade starts after it.
                let left = if i == 0 { left + first_left_room } else { left };
                [fade(true, Some(left)), fade(false, Some((right - fade_px).max(left)))]
            })
            .collect(),
        _ => vec![fade(true, Some(first_left_room)), fade(false, None)],
    }
}

/// The window's chevron on a touch strip: a stretch of the leather's own, as wide as the chevron and
/// at the first lane's left end, so that a pill that slides under it is covered, and the chevron over
/// it. The line at the leather's foot goes on under it.
fn chevron_over_leather(look: &TabHeaderLook, left: f32, chevron: Dom) -> Vec<Dom> {
    let width = crate::widgets::tab_content_toggle::CONTENT_TOGGLE_PX;
    vec![
        leather_lane(look, Some((left, left + width))),
        Dom::create_div()
            .with_css(&format!("position: absolute; left: {left}px; top: 0px;"))
            .with_child(chevron),
    ]
}

impl TabHeader {
    /// The tab bar as touch pills ([`TabStyle::TouchPills`]): a header, a row of pills that
    /// scrolls inside it without a scrollbar, and - in the status bar's band - a cover over each
    /// gap the cutouts leave.
    ///
    /// The pills keep what every tab has: the tab role and the selected state, the tab classes
    /// the arrow keys find their siblings by, the dataset, the click and arrow-key callbacks and
    /// the roving tab stop.
    #[allow(clippy::too_many_lines)] // one build: the pills, the strip, the covers
    fn dom_touch(self, theme: crate::widgets::themes::UiTheme) -> Dom {
        use azul_core::{
            a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
            callbacks::CoreCallbackDataVec,
        };

        use crate::widgets::{button::ButtonType, tab_strip};

        let tab_count = self.tabs.as_ref().len();
        let active = (self.active_tab < tab_count).then_some(self.active_tab);
        let on_click_is_some = self.on_click.is_some();
        let tab_stop = crate::widgets::roving::stop_index(Some(self.active_tab), tab_count);
        let band = self.status_bar.into_option().filter(tab_strip::StatusBarBand::is_in_band);
        #[allow(clippy::cast_precision_loss)] // a few px
        let gap_px = TOUCH_GAP as f32;
        let row = band
            .as_ref()
            .map_or(tab_strip::TOUCH_ROW, |b| b.height.max(tab_strip::TOUCH_ROW));
        // The window's one chevron, at the left end of the first lane: the pills start after it.
        #[allow(clippy::cast_precision_loss)] // 2px
        let chevron = if self.content_toggle {
            crate::widgets::tab_content_toggle::chevron_for_row(row - PILL_RULE as f32)
        } else {
            None
        };
        let mut scroll = StripScrollData::for_band(
            band.as_ref(),
            vec![TAB_ACTIVE_CLASS_NAME.to_string(), TAB_NOT_ACTIVE_CLASS_NAME.to_string()],
        );
        if chevron.is_some() {
            scroll.lead += crate::widgets::tab_content_toggle::CONTENT_TOGGLE_PX;
        }
        let (lead, trail) = (scroll.lead, scroll.trail);

        let dataset = TabLocalDataset {
            tab_idx: 0,
            on_click: self.on_click,
        };
        let mut nodes: Vec<Dom> = Vec::with_capacity(tab_count + 2);
        nodes.push(
            Dom::create_div().with_css(&format!("flex-shrink: 0; width: {}px; height: 1px;", (lead - gap_px).max(0.0))),
        );
        for (tab_idx, label) in self.tabs.as_ref().iter().enumerate() {
            let selected = active == Some(tab_idx);
            let mut tab_dataset = dataset.clone();
            tab_dataset.tab_idx = tab_idx;
            let tab_dataset = RefAny::new(tab_dataset);
            let callbacks: CoreCallbackDataVec = if on_click_is_some {
                vec![
                    CoreCallbackData {
                        event: EventFilter::Hover(HoverEventFilter::Click),
                        callback: CoreCallback {
                            cb: on_tab_click as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: tab_dataset.clone(),
                    },
                    CoreCallbackData {
                        event: EventFilter::Focus(
                            azul_core::events::FocusEventFilter::VirtualKeyDown,
                        ),
                        callback: CoreCallback {
                            cb: on_tab_key as usize,
                            ctx: azul_core::refany::OptionRefAny::None,
                        },
                        refany: tab_dataset.clone(),
                    },
                ]
                .into()
            } else {
                CoreCallbackDataVec::from_const_slice(&[])
            };
            let classes = if selected {
                TOUCH_ACTIVE_CLASSES
            } else {
                TOUCH_INACTIVE_CLASSES
            };
            let mut pill = crate::widgets::widget_p_with_text(label.clone())
                .with_callbacks(callbacks)
                .with_dataset(Some(tab_dataset).into())
                .with_css_props(touch_pill(
                    theme,
                    if selected {
                        ButtonType::Primary
                    } else {
                        ButtonType::Default
                    },
                ))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
                // A tab, and whether it is the active one. The pill's text is not the node's own
                // (the hit area hangs on it too), so the name is given.
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::PageTab,
                    accessibility_name: OptionString::Some(label.clone()),
                    states: if selected {
                        AccessibilityStateVec::from_vec(vec![AccessibilityState::Selected])
                    } else {
                        AccessibilityStateVec::from_const_slice(&[])
                    },
                    ..Default::default()
                });
            if on_click_is_some {
                pill = pill.with_tab_index(crate::widgets::roving::item_tab_index(
                    tab_idx, tab_stop,
                ));
            }
            pill.add_child(hit_extender());
            if selected {
                pill.add_child(selected_marker(scroll.clone(), tab_idx));
            }
            nodes.push(pill);
        }
        nodes.push(
            Dom::create_div().with_css(&format!("flex-shrink: 0; width: {}px; height: 1px;", (trail - gap_px).max(0.0))),
        );

        // The row: longer than the window, it scrolls sideways (a finger, the wheel, the keys)
        // and shows no scrollbar.
        let strip = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOUCH_STRIP_CLASSES))
            .with_css(&format!(
                "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; \
                 width: 100%; height: {row}px; overflow-x: auto; overflow-y: hidden; \
                 scrollbar-width: none; column-gap: {gap_px}px;"
            ))
            // The tab list its tabs belong to: the tabs are its direct children.
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::PageTabList,
                ..Default::default()
            })
            // A turn of the phone brings the selected pill into a lane of the new band.
            .with_callbacks(strip_resized_callbacks(scroll))
            .with_children(DomVec::from_vec(nodes));

        let mut header_css = format!(
            "position: relative; box-sizing: border-box; width: 100%; height: {row}px; \
             flex-shrink: 0; overflow: hidden;"
        );
        let mut strip = strip;
        if band.is_some() {
            strip.add_class(AzString::from_const_str(BAND_STRIP_CLASS));
        }
        // The leather under the row, one stretch per lane of the band (the whole row where there is no
        // band), with the golden line at its foot; the strip's own pills lie over it.
        let look = pill_look(theme, None);
        let mut children = leather_under(&look, band.as_ref());
        children.push(strip);
        // The pills fade out at the ends of the lanes (over the pills, under the covers).
        let chevron_room = if chevron.is_some() { crate::widgets::tab_content_toggle::CONTENT_TOGGLE_PX } else { 0.0 };
        children.extend(lane_fades(band.as_ref(), row, chevron_room));
        // The window's one chevron at the first lane's left end, over the pills and their fade.
        if let Some(chevron) = chevron {
            let left = band
                .as_ref()
                .and_then(|b| b.lane_spans().first().map(|lane| lane.0))
                .unwrap_or(0.0);
            children.extend(chevron_over_leather(&look, left, chevron));
        }
        if let Some(band) = band.as_ref() {
            // The window's background under the band (where the cutouts are, the leather has a gap)
            // and over the pills there.
            header_css.push_str(" background: system:window-background;");
            children.extend(tab_strip::cover_rects(band).iter().map(gap_cover));
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(TOUCH_HEADER_CLASSES))
            .with_css(&header_css)
            .with_children(DomVec::from_vec(children))
    }
}

// ---- The flip of a tab panel ----
//
// A vertical swipe on the panel (`TabContent::with_flip_of`): the touch callbacks feed a
// `FlipGesture` (the rules are in `tab_flip.rs`, as plain numbers), the panel shows the pull (the
// content follows the finger with resistance, a chip names the tab a release goes to), and a flip is
// told to the app exactly as a tap on a pill is. A fling carries on over more tabs through a chain of
// timers, one flip and one tick each in a slowing rhythm.

/// The class of a node that claims a vertical gesture (a slider, a map, a canvas): a finger that
/// went down on it, or inside it, is never the panel's.
pub const CLAIMS_GESTURE_CLASS: &str = "__azul-claims-gesture";

const FLIP_NEXT_CLASS: &str = "__azul-tabs-flip-next";
const FLIP_PREVIOUS_CLASS: &str = "__azul-tabs-flip-previous";

static FLIP_NEXT_CLASSES: &[IdOrClass] = &[Class(AzString::from_const_str(FLIP_NEXT_CLASS))];
static FLIP_PREVIOUS_CLASSES: &[IdOrClass] =
    &[Class(AzString::from_const_str(FLIP_PREVIOUS_CLASS))];

/// Who is told of a flip: a tab bar's `on_click` (a `TabContent`'s panel), or a ribbon's
/// `on_tab_click` (its content band).
#[derive(Clone)]
pub(crate) enum FlipTell {
    /// The tab bar's callback, told as of a tap on a pill.
    Tab(OptionTabOnClick),
    /// The ribbon's callback, told the index of the tab.
    Ribbon(crate::widgets::ribbon::OptionRibbonOnTabClick),
}

/// What the panel's callbacks share: the panel as the last build left it, the finger, the wheel,
/// the clock. The dataset of the panel node: a rebuild keeps the old one and updates the first four
/// fields ([`merge_flip_panel`]), so a gesture or a fling goes on across the rebuilds its own flips
/// make.
pub(crate) struct FlipPanelData {
    flip: crate::widgets::tab_flip::TabFlip,
    tab: usize,
    names: Vec<String>,
    on_flip: FlipTell,
    gesture: Option<crate::widgets::tab_flip::FlipGesture>,
    wheel: crate::widgets::tab_flip::WheelPush,
    /// Bumped by every touch that goes down: a fling made for an older value ends.
    epoch: u64,
    /// The clock's zero (milliseconds are counted from the first touch).
    origin: Option<azul_core::task::Instant>,
}

/// The data of `content`'s panel, or `None` for a panel that does not flip.
fn flip_panel_data(content: &TabContent) -> Option<FlipPanelData> {
    use crate::widgets::tab_flip::TabFlip;
    let tabs = content.flip_tabs.as_ref();
    if content.flip == TabFlip::Off || tabs.len() < 2 || content.on_flip.is_none() {
        return None;
    }
    Some(FlipPanelData {
        flip: content.flip,
        tab: content.flip_tab,
        names: tabs.iter().map(|name| name.as_str().to_string()).collect(),
        on_flip: FlipTell::Tab(content.on_flip.clone()),
        gesture: None,
        wheel: crate::widgets::tab_flip::WheelPush::default(),
        epoch: 0,
        origin: None,
    })
}

/// The panel (already holding the content group) with its touch and wheel callbacks, its dataset
/// and the two hint chips. A panel that scrolls its own content: `overflow` says how.
fn attach_flip(panel: Dom, data: FlipPanelData) -> Dom {
    attach_flip_with(
        panel,
        data,
        "position: relative; overflow-x: hidden; overflow-y: auto; scrollbar-width: none;",
    )
}

/// [`attach_flip`] with the panel's own `css`: a ribbon's content band keeps its overflow and is
/// only positioned for the chips.
pub(crate) fn attach_flip_with(mut panel: Dom, data: FlipPanelData, css: &str) -> Dom {
    let tab = data.tab;
    let count = data.names.len();
    let name = |i: usize| data.names.get(i).cloned().unwrap_or_default();
    if tab + 1 < count {
        panel.add_child(flip_chip(FLIP_NEXT_CLASSES, &hint_for(true, &name(tab + 1)), false));
    }
    if tab > 0 && tab <= count {
        panel.add_child(flip_chip(FLIP_PREVIOUS_CLASSES, &hint_for(false, &name(tab - 1)), true));
    }
    let dataset = RefAny::new(data);
    let mut callbacks = panel.root.get_callbacks().as_ref().to_vec();
    for (event, cb) in [
        (HoverEventFilter::TouchStart, on_flip_touch_start as usize),
        (HoverEventFilter::TouchMove, on_flip_touch_move as usize),
        (HoverEventFilter::TouchEnd, on_flip_touch_end as usize),
        (HoverEventFilter::TouchCancel, on_flip_touch_cancel as usize),
        (HoverEventFilter::Scroll, on_flip_wheel as usize),
    ] {
        callbacks.push(CoreCallbackData {
            event: EventFilter::Hover(event),
            callback: CoreCallback {
                cb,
                ctx: azul_core::refany::OptionRefAny::None,
            },
            refany: dataset.clone(),
        });
    }
    let mut panel = panel
        .with_key("azul-tab-flip-panel")
        .with_css(css)
        .with_callbacks(callbacks.into());
    panel.root.set_dataset(azul_core::refany::OptionRefAny::Some(dataset));
    // The `as` coerces the fn ITEM to the fn POINTER `DatasetMergeCallback` is made from.
    #[allow(trivial_casts)]
    panel
        .root
        .set_merge_callback(merge_flip_panel as azul_core::dom::DatasetMergeCallbackType);
    panel
}

/// A ribbon's content band as a panel that flips: `tabs` are the names of the tabs, `active` the
/// shown one, `tell` the ribbon's `on_tab_click`. The band keeps its own style; it scrolls nothing
/// vertically, so any vertical swipe over it flips.
pub(crate) fn attach_flip_band(
    band: Dom,
    flip: crate::widgets::tab_flip::TabFlip,
    active: usize,
    tabs: &[AzString],
    tell: crate::widgets::ribbon::OptionRibbonOnTabClick,
) -> Dom {
    let data = FlipPanelData {
        flip,
        tab: active,
        names: tabs.iter().map(|name| name.as_str().to_string()).collect(),
        on_flip: FlipTell::Ribbon(tell),
        gesture: None,
        wheel: crate::widgets::tab_flip::WheelPush::default(),
        epoch: 0,
        origin: None,
    };
    attach_flip_with(band, data, "position: relative;")
}

/// The header of a strip in the status bar's band: `strip` (the scrolling row) in a box as tall as
/// the band with the window's background, and a cover over each gap the cutouts leave.
pub(crate) fn touch_header(
    strip: Dom,
    band: &crate::widgets::tab_strip::StatusBarBand,
    theme: crate::widgets::themes::UiTheme,
    chevron: Option<Dom>,
) -> Dom {
    let row = band.height.max(crate::widgets::tab_strip::TOUCH_ROW);
    let mut strip = strip;
    strip.add_class(AzString::from_const_str(BAND_STRIP_CLASS));
    // The leather under the row, one stretch per lane, the camera between them.
    let look = pill_look(theme, None);
    let mut children = leather_under(&look, Some(band));
    children.push(strip);
    let chevron_room = if chevron.is_some() { crate::widgets::tab_content_toggle::CONTENT_TOGGLE_PX } else { 0.0 };
    children.extend(lane_fades(Some(band), row, chevron_room));
    // The window's one chevron at the first lane's left end, over the pills and their fade.
    if let Some(chevron) = chevron {
        let left = band.lane_spans().first().map_or(0.0, |lane| lane.0);
        children.extend(chevron_over_leather(&look, left, chevron));
    }
    children.extend(crate::widgets::tab_strip::cover_rects(band).iter().map(gap_cover));
    Dom::create_div()
        .with_css(&format!(
            "position: relative; box-sizing: border-box; width: 100%; height: {row}px; \
             flex-shrink: 0; overflow: hidden; background: system:window-background;"
        ))
        .with_children(DomVec::from_vec(children))
}

/// "Next: Share" / "Previous: Home".
fn hint_for(next: bool, name: &str) -> String {
    crate::widgets::tab_flip::hint_text(
        if next {
            crate::widgets::tab_flip::FlipDirection::Next
        } else {
            crate::widgets::tab_flip::FlipDirection::Previous
        },
        name,
    )
}

/// A hint chip: a full-width row at the bottom (`top`: at the top) of the panel with one rounded
/// label in it, not shown until a pull shows it.
fn flip_chip(classes: &'static [IdOrClass], text: &str, top: bool) -> Dom {
    let edge = if top { "top" } else { "bottom" };
    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_const_slice(classes))
        .with_css(&format!(
            "position: absolute; left: 0px; right: 0px; {edge}: 8px; display: none; \
             justify-content: center;"
        ))
        .with_child(Dom::create_div_with_text(text.to_string()).with_css(
            "padding: 4px 14px; border-radius: 16px; background: system:accent; \
             color: system:accent-text; font-size: 12px; box-sizing: border-box;",
        ))
}

/// The dataset merge of the panel: the old allocation stays (the gesture, the wheel, the clock, the
/// epoch a fling in flight reads) and takes what the new build says of the panel.
extern "C" fn merge_flip_panel(mut new: RefAny, mut old: RefAny) -> RefAny {
    let fresh = new.downcast_ref::<FlipPanelData>().map(|n| {
        (n.flip, n.tab, n.names.clone(), n.on_flip.clone())
    });
    if let (Some((flip, tab, names, on_flip)), Some(mut kept)) =
        (fresh, old.downcast_mut::<FlipPanelData>())
    {
        kept.flip = flip;
        kept.tab = tab;
        kept.names = names;
        kept.on_flip = on_flip;
    }
    old
}

/// The first finger on the window: where it is, in window coordinates, and the key the hit test
/// under it is published with (the events of a finger are aimed at the node under THAT finger).
fn first_touch(info: &CallbackInfo) -> Option<(f32, f32, u64)> {
    info.get_current_window_state()
        .touch_state
        .touch_points
        .as_ref()
        .first()
        .map(|point| {
            (
                point.position.x,
                point.position.y,
                azul_core::window::touch_point_key(point.seat_id, point.id),
            )
        })
}

/// Whether the content of `panel` can scroll, how far it is scrolled and how far it can: the panel is
/// the scroll box whose ends a pull goes past.
fn panel_scroll(info: &CallbackInfo, panel: azul_core::dom::DomNodeId) -> (bool, f32, f32) {
    let Some(node) = panel.node.into_crate_internal() else {
        return (false, 0.0, 0.0);
    };
    match info.get_scroll_node_info(panel.dom, node) {
        Some(scroll) if scroll.max_scroll_y > 0.5 => (true, scroll.current_offset.y, scroll.max_scroll_y),
        _ => (false, 0.0, 0.0),
    }
}

/// Whether the finger that just went down (`finger`: its hit test's key) is on something that claims
/// the gesture: a node of the class [`CLAIMS_GESTURE_CLASS`], or a scrollable below the panel that
/// has room. The node under THAT finger counts, as the touch events are aimed at it; the mouse's
/// when the host published no hit test for the finger.
fn flip_claimed(info: &CallbackInfo, panel: azul_core::dom::DomNodeId, finger: u64) -> bool {
    let under_finger = info
        .get_layout_window()
        .hover_manager
        .hover_node_full_for(&crate::managers::hover::InputPointId::Touch(finger));
    let Some(mut node) = under_finger.or_else(|| info.get_deepest_hovered_node()) else {
        return false;
    };
    for _ in 0..64 {
        if node == panel {
            return false;
        }
        if crate::widgets::roving::has_class(info, node, CLAIMS_GESTURE_CLASS) {
            return true;
        }
        if let Some(id) = node.node.into_crate_internal() {
            if let Some(scroll) = info.get_scroll_node_info(node.dom, id) {
                if scroll.max_scroll_y > 0.5 || scroll.max_scroll_x > 0.5 {
                    return true;
                }
            }
        }
        match info.get_parent(node) {
            Some(parent) => node = parent,
            None => return false,
        }
    }
    false
}

/// Milliseconds since the panel's clock began.
fn flip_millis(data: &mut FlipPanelData, now: &azul_core::task::Instant) -> u64 {
    let origin = data.origin.get_or_insert_with(|| now.clone()).clone();
    now.duration_since(&origin).as_millis_u64()
}

extern "C" fn on_flip_touch_start(mut data: RefAny, info: CallbackInfo) -> Update {
    use crate::widgets::tab_flip::{FlipGesture, FlipPanel};
    let Some((x, y, finger)) = first_touch(&info) else {
        return Update::DoNothing;
    };
    let panel = info.get_hit_node();
    let claimed = flip_claimed(&info, panel, finger);
    let (overflows, scroll, scroll_max) = panel_scroll(&info, panel);
    let now = info.get_current_time();
    let Some(mut d) = data.downcast_mut::<FlipPanelData>() else {
        return Update::DoNothing;
    };
    let t = flip_millis(&mut d, &now);
    d.epoch += 1;
    let flip_panel = FlipPanel {
        flip: d.flip,
        tab: d.tab,
        tab_count: d.names.len(),
        overflows,
        scroll,
        scroll_max,
    };
    d.gesture = Some(FlipGesture::begin(flip_panel, x, y, t, claimed));
    Update::DoNothing
}

extern "C" fn on_flip_touch_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((x, y, _)) = first_touch(&info) else {
        return Update::DoNothing;
    };
    let panel = info.get_hit_node();
    let now = info.get_current_time();
    let outcome = {
        let Some(mut d) = data.downcast_mut::<FlipPanelData>() else {
            return Update::DoNothing;
        };
        let t = flip_millis(&mut d, &now);
        let names = d.names.clone();
        let Some(gesture) = d.gesture.as_mut() else {
            return Update::DoNothing;
        };
        gesture.moved(x, y, t, &|i| names.get(i).cloned().unwrap_or_default())
    };
    apply_flip_outcome(&mut data, &mut info, panel, outcome)
}

extern "C" fn on_flip_touch_end(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let panel = info.get_hit_node();
    let now = info.get_current_time();
    let (outcome, momentum, epoch) = {
        let Some(mut d) = data.downcast_mut::<FlipPanelData>() else {
            return Update::DoNothing;
        };
        let t = flip_millis(&mut d, &now);
        let Some(mut gesture) = d.gesture.take() else {
            return Update::DoNothing;
        };
        let outcome = gesture.lifted(t);
        (outcome, gesture.momentum(), d.epoch)
    };
    let update = apply_flip_outcome(&mut data, &mut info, panel, outcome);
    if let Some(momentum) = momentum {
        schedule_fling(&mut info, &data, &momentum, 0, epoch);
    }
    update
}

extern "C" fn on_flip_touch_cancel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let panel = info.get_hit_node();
    let outcome = {
        let Some(mut d) = data.downcast_mut::<FlipPanelData>() else {
            return Update::DoNothing;
        };
        // The system took the finger: nothing flips, and a fling in flight ends.
        d.epoch += 1;
        match d.gesture.take() {
            Some(mut gesture) => gesture.cancel(),
            None => return Update::DoNothing,
        }
    };
    apply_flip_outcome(&mut data, &mut info, panel, outcome)
}

/// A wheel or trackpad over the panel (`TabFlip::Always` only).
extern "C" fn on_flip_wheel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    use crate::widgets::tab_flip::FlipPanel;
    let panel = info.get_hit_node();
    let Some(node) = panel.node.into_crate_internal() else {
        return Update::DoNothing;
    };
    let Some(delta) = info.get_scroll_delta(panel.dom, node).map(|d| d.y) else {
        return Update::DoNothing;
    };
    let (overflows, scroll, scroll_max) = panel_scroll(&info, panel);
    let now = info.get_current_time();
    let outcome = {
        let Some(mut d) = data.downcast_mut::<FlipPanelData>() else {
            return Update::DoNothing;
        };
        let t = flip_millis(&mut d, &now);
        let flip_panel = FlipPanel {
            flip: d.flip,
            tab: d.tab,
            tab_count: d.names.len(),
            overflows,
            scroll,
            scroll_max,
        };
        d.wheel.step(&flip_panel, delta, t)
    };
    apply_flip_outcome(&mut data, &mut info, panel, outcome)
}

/// Show what a gesture asked: the pull (the content follows the finger, the chip of the tab a
/// release goes to), the end of it, or the flip, told to the app as a tap on a pill is.
fn apply_flip_outcome(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    panel: azul_core::dom::DomNodeId,
    outcome: crate::widgets::tab_flip::FlipOutcome,
) -> Update {
    use crate::widgets::tab_flip::{FlipDirection, FlipOutcome};
    match outcome {
        FlipOutcome::Idle => Update::DoNothing,
        FlipOutcome::Pulling {
            direction,
            offset,
            progress,
            hint,
            ..
        } => {
            // The content follows the finger: up for the next tab, down for the previous one.
            let shift = if direction == FlipDirection::Next { -offset } else { offset };
            show_flip_pull(info, panel, shift, direction, hint.is_some().then_some(progress));
            Update::DoNothing
        }
        FlipOutcome::Release => {
            show_flip_pull(info, panel, 0.0, FlipDirection::Next, None);
            Update::DoNothing
        }
        FlipOutcome::Flip { to, .. } => {
            show_flip_pull(info, panel, 0.0, FlipDirection::Next, None);
            tell_flip(data, info, to)
        }
    }
}

/// The content shifted by `shift` px and the chip of `direction` shown at `progress` (0 to 1; `None`
/// hides both chips). The content at rest is no transform (`CssProperty::initial`: the property's
/// initial value, and a rebuild drops the override) and a chip that is not shown is `display: none`,
/// as a build makes it, so the end of a pull, a flip and a rebuild all leave the panel as a build does.
fn show_flip_pull(
    info: &mut CallbackInfo,
    panel: azul_core::dom::DomNodeId,
    shift: f32,
    direction: crate::widgets::tab_flip::FlipDirection,
    chip_progress: Option<f32>,
) {
    use crate::widgets::{roving, tab_flip::FlipDirection};
    for child in roving::children_of(info, panel) {
        if roving::has_class(info, child, "__azul-native-tabs-content") {
            let transform = if shift == 0.0 {
                CssProperty::initial(CssPropertyType::Transform)
            } else {
                CssProperty::const_transform(StyleTransformVec::from_vec(vec![
                    StyleTransform::TranslateY(PixelValue::px(shift)),
                ]))
            };
            info.set_css_property(child, transform);
            continue;
        }
        let which = if roving::has_class(info, child, FLIP_NEXT_CLASS) {
            Some(FlipDirection::Next)
        } else if roving::has_class(info, child, FLIP_PREVIOUS_CLASS) {
            Some(FlipDirection::Previous)
        } else {
            None
        };
        let Some(which) = which else { continue };
        if let Some(progress) = chip_progress.filter(|_| which == direction) {
            info.set_css_property(child, CssProperty::const_display(LayoutDisplay::Flex));
            info.set_css_property(
                child,
                CssProperty::const_opacity(StyleOpacity {
                    inner: PercentageValue::new(30.0 + 70.0 * progress.clamp(0.0, 1.0)),
                }),
            );
        } else {
            // Hidden is a value, not "no override": `initial` is the property's initial value
            // (a block, for an absolute box), and showed the chip of the other direction as a bar
            // across the panel. A rebuild says `display: none` by itself, which is the same.
            info.set_css_property(child, CssProperty::const_display(LayoutDisplay::None));
            info.set_css_property(child, CssProperty::initial(CssPropertyType::Opacity));
        }
    }
}

/// The tick of one tab crossing: the platform's own tab-change sound (`SystemSound::TabChange`: on
/// Android the touch-sound click, on iOS the selection haptic, on a desktop none). One tick per flip,
/// the momentum's too (a timer callback may ask: the shell plays at the end of the timer turn); the
/// engine drops the same sound asked for twice within 40 ms and the OS's own switches stay in charge.
fn flip_tick(info: &mut CallbackInfo) {
    info.play_system_sound(azul_core::sound::SystemSound::TabChange);
}

/// Tell the app of a flip to `to`, as a tap on its pill would, and tick.
fn tell_flip(data: &mut RefAny, info: &mut CallbackInfo, to: usize) -> Update {
    let on_flip = {
        let Some(mut d) = data.downcast_mut::<FlipPanelData>() else {
            return Update::DoNothing;
        };
        d.tab = to;
        d.on_flip.clone()
    };
    flip_tick(info);
    // The app's own answer is the answer, as for a tap on a pill: a flip the app does not rebuild
    // for has changed nothing it shows.
    match on_flip {
        FlipTell::Tab(mut option) => match option.as_mut() {
            Some(TabOnClick { callback, refany }) => {
                callback.invoke(refany.clone(), *info, TabHeaderState { active_tab: to })
            }
            None => Update::DoNothing,
        },
        FlipTell::Ribbon(mut option) => match option.as_mut() {
            Some(crate::widgets::ribbon::RibbonOnTabClick { refany, callback }) => {
                callback.invoke(refany.clone(), *info, to)
            }
            None => Update::DoNothing,
        },
    }
}

/// A fling in flight: the flips it has left, one timer at a time.
struct FlingTimer {
    data: RefAny,
    momentum: crate::widgets::tab_flip::Momentum,
    /// The next flip's index in `momentum.delays`.
    step: usize,
    /// The panel's epoch when the fling was made: a touch since ends it.
    epoch: u64,
}

/// The timer of flip `step` of `momentum` (the delays are from the lift, or from the flip before).
fn schedule_fling(
    info: &mut CallbackInfo,
    data: &RefAny,
    momentum: &crate::widgets::tab_flip::Momentum,
    step: usize,
    epoch: u64,
) {
    let Some(delay) = momentum.delays.get(step).copied() else {
        return;
    };
    let timer = Timer::create(
        RefAny::new(FlingTimer {
            data: data.clone(),
            momentum: momentum.clone(),
            step,
            epoch,
        }),
        TimerCallback::create(on_fling_tick),
        info.get_system_time_fn(),
    )
    .with_delay(azul_core::task::Duration::from_millis(delay));
    info.add_timer(azul_core::task::TimerId::unique(), timer);
}

/// One flip of a fling: flip, tick, and set the timer of the next.
extern "C" fn on_fling_tick(
    mut timer_data: RefAny,
    info: crate::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::{callbacks::TimerCallbackReturn, task::TerminateTimer};
    let end = || TimerCallbackReturn::create(Update::DoNothing, TerminateTimer::Terminate);
    let Some((mut data, momentum, step, epoch)) = timer_data
        .downcast_ref::<FlingTimer>()
        .map(|t| (t.data.clone(), t.momentum.clone(), t.step, t.epoch))
    else {
        return end();
    };
    let (alive, tab, count) = match data.downcast_ref::<FlipPanelData>() {
        Some(d) => (d.epoch == epoch, d.tab, d.names.len()),
        None => return end(),
    };
    if !alive {
        return end();
    }
    // The tab the flip goes to: one past the tab the fling stands on (the flips so far moved it).
    let Some(to) = momentum.direction.neighbour(tab, count) else {
        return end();
    };
    let mut callback_info = *info.get_callback_info();
    let update = tell_flip(&mut data, &mut callback_info, to);
    schedule_fling(&mut callback_info, &data, &momentum, step + 1, epoch);
    TimerCallbackReturn::create(update, TerminateTimer::Terminate)
}

/// The rule a pill strip closes on, along its foot.
pub(crate) const RULE_CLASS: &str = "__azul-native-tabs-rule";

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
    /// The rule along the strip's foot, its own node (Pills); `None` for a
    /// look whose rule is the header's own border.
    pub(crate) rule: Option<CssPropertyWithConditionsVec>,
    /// Inline CSS laid on the header after its style - the custom
    /// properties its material reads (the leather's tint); `""` for none.
    pub(crate) header_css: &'static str,
    /// Inline CSS laid on the rule after its style (the metal's tint); `""`
    /// for none.
    pub(crate) rule_css: &'static str,
}

impl TabHeaderLook {
    /// A look of the six parts with nothing hung on them: no rule node, no extra CSS.
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn plain(
        header: CssPropertyWithConditionsVec,
        before: CssPropertyWithConditionsVec,
        after: CssPropertyWithConditionsVec,
        active: CssPropertyWithConditionsVec,
        before_active: CssPropertyWithConditionsVec,
        after_active: CssPropertyWithConditionsVec,
        inactive: CssPropertyWithConditionsVec,
        marker: Option<&'static str>,
    ) -> Self {
        Self {
            header,
            before,
            after,
            active,
            before_active,
            after_active,
            inactive,
            marker,
            rule: None,
            header_css: "",
            rule_css: "",
        }
    }

    /// The look `theme` pins in `style`, or - unpinned - the look of the
    /// theme the DOM is built for, in `style` as that theme answers it.
    pub(crate) fn of(theme: crate::widgets::themes::OptionUiTheme, style: TabStyle) -> Self {
        use crate::widgets::themes::{flat, flora, UiTheme};
        match crate::widgets::themes::theme_blocks::live_theme(theme) {
            UiTheme::Flat => flat::tab_header_look_in(style),
            UiTheme::Flora => flora::tab_header_look_in(style),
        }
    }
}

// ---- Pills: tab buttons on leather ("Buttons" in the UI) ----
//
// A tab button is the theme's own command at the house radius: the selected
// one is its submit button (the primary command), every other one its plain
// button, each with the button's whole style - resting face, hover, press
// (pointer, Space, Enter), the keyboard ring and the fades. They stand in a
// row as tall as a title bar, centred where the traffic lights are centred
// when the row is the window's title bar, on a strip of leather laid over
// the strip's whole border box so it reaches its top edge. Under the row
// runs a 2px golden line: the theme's metal shimmer over aluminium tinted
// gold. The line is a node of its own (each material reads its tint from
// its own node). In flora the plain buttons wear the leather faces (a little
// grain shows through) and every button the gold edge.

/// The leather's tint (pale grey nappa, mixed toward black at night) and
/// its pillow: the strip rises from its edges, the golden line its seam.
pub(crate) const PILL_STRIP_CSS: &str = "--azul-material-tint: #e4e2dd; --azul-pillow: 76px .16; \
     @theme(dark) { --azul-material-tint: #373635; }";
/// The golden line: aluminium tinted gold, lit along its own shape by a
/// cushion bevel.
pub(crate) const PILL_RULE_CSS: &str = "--azul-material-tint: #e9c672; --azul-bevel: 1px; \
     --azul-bevel-depth: 2.5; --azul-bevel-softness: 1;";
/// The room between two tab buttons, in px.
pub(crate) const PILL_GAP: isize = 6;
/// The row the buttons stand in, in px: a title bar's height.
pub(crate) const PILL_ROW: isize = 28;
/// A tab button's height, in px.
pub(crate) const PILL_HEIGHT: isize = 22;
/// The room beside a tab button's label, in px.
pub(crate) const PILL_SIDE: isize = 12;
/// The house radius of a tab button, in px.
pub(crate) const PILL_RADIUS: isize = 3;
/// The golden line's height, in px (four device pixels at 2x).
pub(crate) const PILL_RULE: isize = 2;
/// The room before the first and after the last button of a strip that is
/// not the window's title bar, in px.
pub(crate) const PILL_LEAD: isize = 10;

/// The style `theme`'s `button_type` button is drawn with: its container's
/// declarations, conditions and all.
fn button_style(
    theme: crate::widgets::themes::UiTheme,
    button_type: crate::widgets::button::ButtonType,
) -> Vec<CssPropertyWithConditions> {
    let dom = crate::widgets::button::Button::with_type(AzString::from_const_str("x"), button_type)
        .with_theme(theme)
        .dom();
    dom.root
        .style
        .iter_inline_properties()
        .map(|(property, apply_if)| CssPropertyWithConditions {
            property: property.clone(),
            apply_if: apply_if.clone(),
        })
        .collect()
}

/// The size of a pill: the height of its border box, its corner radius and the room beside its
/// label, in px.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub(crate) struct PillMetrics {
    pub(crate) height: isize,
    pub(crate) radius: isize,
    pub(crate) side: isize,
}

/// The pill of a title bar: the theme's button at the house radius, 22 tall in a 28 row.
pub(crate) static PILL_DESKTOP: PillMetrics = PillMetrics {
    height: PILL_HEIGHT,
    radius: PILL_RADIUS,
    side: PILL_SIDE,
};

/// How tall a touch pill is, in px: the border box. The target of a finger is the whole row.
pub(crate) const TOUCH_PILL_HEIGHT: isize = 32;
/// The room beside a touch pill's label, in px.
pub(crate) const TOUCH_PILL_SIDE: isize = 16;
/// The room between two touch pills, in px.
pub(crate) const TOUCH_GAP: isize = 8;
/// The room before the first and after the last touch pill, in px: also the width of the fade at the
/// ends of a lane, which lies over this room at rest.
pub(crate) const TOUCH_LEAD: isize = 16;

/// The pill of a finger: 32 tall with full-radius ends, 16 beside the label.
pub(crate) static PILL_TOUCH: PillMetrics = PillMetrics {
    height: TOUCH_PILL_HEIGHT,
    radius: PILL_RADIUS,
    side: TOUCH_PILL_SIDE,
};

/// A tab button: the tab's base, `theme`'s `button_type` button at the house
/// radius, `PILL_HEIGHT` tall (its border box) with its label centred on
/// one line, `PILL_SIDE` beside the label. In flora the label is set in the
/// tab capitals (the right side giving their trailing tracking back), a
/// plain button wears the leather faces, and every button the gold edge.
#[must_use]
pub(crate) fn pill(
    theme: crate::widgets::themes::UiTheme,
    button_type: crate::widgets::button::ButtonType,
) -> CssPropertyWithConditionsVec {
    pill_sized(theme, button_type, &PILL_DESKTOP)
}

/// [`pill`] at the size `metrics` gives.
#[must_use]
pub(crate) fn pill_sized(
    theme: crate::widgets::themes::UiTheme,
    button_type: crate::widgets::button::ButtonType,
    metrics: &PillMetrics,
) -> CssPropertyWithConditionsVec {
    use crate::widgets::{
        button::ButtonType,
        themes::{decl, flora, UiTheme},
    };
    type P = CssPropertyWithConditions;
    let mut v = TAB_BASE.to_vec();
    v.extend(button_style(theme, button_type));
    v.extend(decl::radius(metrics.radius));
    v.extend(decl::padding(0, metrics.side, 0, metrics.side));
    v.extend(decl::margin(0, 0, 0, 0));
    v.push(decl::nowrap());
    // The border box is the button's height: the button's own base sizes
    // its content box and starts it at the top of a column (`align-self:
    // start`); here it is centred in the row.
    v.push(P::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)));
    v.push(P::simple(CssProperty::const_height(LayoutHeight::const_px(metrics.height))));
    v.push(P::simple(CssProperty::align_self(LayoutAlignSelf::Center)));
    // One line as tall as the content box (the border box less its 1px
    // border), so the label is centred in either box model.
    v.push(P::simple(CssProperty::const_line_height(StyleLineHeight::Length(
        PixelValue::const_px(metrics.height - 2),
    ))));
    if theme == UiTheme::Flora {
        v.extend(flora::tab_button_caps());
        let (px, em) = flora::TAB_BUTTON_CAPS;
        #[allow(clippy::cast_precision_loss)] // a few px
        let side = metrics.side as f32;
        v.push(decl::px_padding_right(side - decl::trailing_tracking_px(px, em)));
        if button_type == ButtonType::Default {
            v.extend(flora::leather_faces());
        }
        v.extend(flora::gold_edge());
        // One fade for the whole face: the face, the edge and the shadows move together and quickly
        // (the button's own face takes 420 ms, its stone 1200 ms - half a second of a white label on a
        // cream face on a pill that changes with the selection: the iOS Simulator's recording, QA
        // 2026-11-28), and the label steps where the old ink and the new one read equally well on it.
        v.extend(flora::flora_face_fade(
            flora::FloraButtonKind::of(button_type),
            PILL_FADE_MS,
        ));
    }
    CssPropertyWithConditionsVec::from_vec(v)
}

/// How long a pill's face, label and edge take to change with the selection, ms: the same for all of
/// them, so that no frame shows a white label on a cream face.
pub(crate) const PILL_FADE_MS: u32 = 180;

/// A touch pill: [`pill_sized`] at [`PILL_TOUCH`], which does not shrink in a row longer than
/// the window and is the box the pill's hit area hangs on.
#[must_use]
pub(crate) fn touch_pill(
    theme: crate::widgets::themes::UiTheme,
    button_type: crate::widgets::button::ButtonType,
) -> CssPropertyWithConditionsVec {
    use crate::widgets::themes::decl;
    let mut v = pill_sized(theme, button_type, &PILL_TOUCH).into_library_owned_vec();
    v.push(CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(
        LayoutFlexShrink {
            inner: FloatValue::const_new(0),
        },
    )));
    v.push(decl::position(LayoutPosition::Relative));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The Pills look in `theme` (whose header carries `marker`).
pub(crate) fn pill_look(
    theme: crate::widgets::themes::UiTheme,
    marker: Option<&'static str>,
) -> TabHeaderLook {
    use crate::widgets::{button::ButtonType, themes::decl};
    type P = CssPropertyWithConditions;
    let leather = || StyleBackgroundContent::Image(AzString::from_const_str("azul-builtin:leather"));

    let mut header = HEADER_BASE.to_vec();
    header.extend([
        P::simple(CssProperty::const_align_items(LayoutAlignItems::Center)),
        P::simple(CssProperty::ColumnGap(LayoutColumnGapValue::Exact(LayoutColumnGap {
            inner: PixelValue::const_px(PILL_GAP),
        }))),
        decl::position(LayoutPosition::Relative),
        decl::font_size(13),
        // The row and the line under it, nothing else: no air above or
        // below the buttons.
        P::simple(CssProperty::const_box_sizing(LayoutBoxSizing::BorderBox)),
        P::simple(CssProperty::const_height(LayoutHeight::const_px(PILL_ROW + PILL_RULE))),
    ]);
    header.extend(decl::padding(0, PILL_LEAD, PILL_RULE, PILL_LEAD));
    // The leather over a base of its own colour (seen while the material
    // renders, or without the materials feature).
    header.extend(decl::themed_layers(
        vec![StyleBackgroundContent::Color(ColorU::rgb(0xF4, 0xF0, 0xE8)), leather()],
        vec![StyleBackgroundContent::Color(ColorU::rgb(0x3A, 0x32, 0x2C)), leather()],
    ));

    #[allow(clippy::cast_precision_loss)] // 2px
    let rule = vec![
        decl::position(LayoutPosition::Absolute),
        decl::simple(CssProperty::const_left(LayoutLeft::const_px(0))),
        decl::simple(CssProperty::const_right(LayoutRight::const_px(0))),
        decl::simple(CssProperty::const_bottom(LayoutInsetBottom::const_px(0))),
        decl::px_height(PILL_RULE as f32),
        P::simple(decl::layers(vec![
            StyleBackgroundContent::Image(AzString::from_const_str("azul-builtin:aluminium")),
            crate::widgets::themes::flora::RULE_SHIMMER,
        ])),
    ];

    let tab = pill(theme, ButtonType::Default);
    let mut look = TabHeaderLook::plain(
        CssPropertyWithConditionsVec::from_vec(header),
        CssPropertyWithConditionsVec::from_vec(vec![
            P::simple(CssProperty::const_width(LayoutWidth::const_px(0))),
            P::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
            // No spacer before the first button: the strip's padding is
            // its lead, and a spacer would add a gap to it.
            P::simple(CssProperty::const_display(LayoutDisplay::None)),
        ]),
        CssPropertyWithConditionsVec::from_const_slice(AFTER_BASE),
        pill(theme, ButtonType::Primary),
        tab.clone(),
        tab.clone(),
        tab,
        marker,
    );
    look.rule = Some(CssPropertyWithConditionsVec::from_vec(rule));
    look.header_css = PILL_STRIP_CSS;
    look.rule_css = PILL_RULE_CSS;
    look
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
    /// The look `theme` pins, or - unpinned - the look of the theme the DOM
    /// is built for. The panel's content is the caller's and is never cloned.
    pub(crate) fn of(theme: crate::widgets::themes::OptionUiTheme) -> Self {
        use crate::widgets::themes::{flat, flora, UiTheme};
        match crate::widgets::themes::theme_blocks::live_theme(theme) {
            UiTheme::Flat => flat::tab_content_look(),
            UiTheme::Flora => flora::tab_content_look(),
        }
    }
}

/// Which way a tab panel's content moves in when its tab bar switched tabs
/// ([`TabContent::with_switch`]).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum TabSlide {
    /// No switch: the content stands still.
    #[default]
    None,
    /// Switched to a tab on the left: the content slides left into place.
    Left,
    /// Switched to a tab on the right: the content slides right into place.
    Right,
}

/// Content panel displayed beneath the active tab in a tab widget.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct TabContent {
    /// The DOM subtree shown as the tab's content area.
    pub content: Dom,
    /// Told of a flip as the bar's `on_click` is told of a tap, set with `flip`.
    pub on_flip: OptionTabOnClick,
    /// The names of the bar's tabs, for the hint ("Next: Share"), set with `flip`.
    pub flip_tabs: StringVec,
    /// The reveal its `TabHeader` was built with (`with_reveal`): the panel
    /// is the other half of its hover area, and it closes to no height.
    pub reveal: crate::widgets::tab_reveal::OptionTabReveal,
    /// The tab the content belongs to, set with `slide`: it keys the content
    /// group, so a switch to another tab replaces the group (the old one
    /// fades out, the new one slides in) while a rebuild for the same tab
    /// keeps it still.
    pub tab: usize,
    /// The tab the panel shows, set with `flip`.
    pub flip_tab: usize,
    /// The widget theme this panel is PINNED to (`with_theme`), or `None` to
    /// follow the app theme (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`; flat unless the app chose another).
    pub theme: crate::widgets::themes::OptionUiTheme,
    /// How the content came in with the last tab switch (`with_switch`).
    /// Anything but `None` gives the content group its slide and fade.
    pub slide: TabSlide,
    /// What a vertical swipe on the panel does (`with_flip_of`): `Off` until the panel is told the
    /// bar it belongs to. APPENDED for ABI stability.
    pub flip: crate::widgets::tab_flip::TabFlip,
    /// The room the panel leaves at its left, px (`with_content_inset`): the tab row stays the whole width.
    /// APPENDED for ABI stability.
    pub content_inset: f32,
    /// Whether the content area includes default padding.
    pub has_padding: bool,
    /// Whether the panel folds away with the window's chevron (`with_content_toggle`).
    /// APPENDED for ABI stability.
    pub content_toggle: bool,
}

impl Default for TabContent {
    fn default() -> Self {
        Self {
            content: Dom::create_div(),
            has_padding: true,
            theme: crate::widgets::themes::OptionUiTheme::None,
            slide: TabSlide::None,
            tab: 0,
            reveal: crate::widgets::tab_reveal::OptionTabReveal::None,
            flip: crate::widgets::tab_flip::TabFlip::Off,
            flip_tabs: StringVec::from_const_slice(&[]),
            flip_tab: 0,
            on_flip: OptionTabOnClick::None,
            content_inset: 0.0,
            content_toggle: false,
        }
    }
}

/// The content group's entrance after a switch to a tab on the right.
const SLIDE_IN_FROM_LEFT: &str = "__azul-tabs-slide-in-from-left";
/// The content group's entrance after a switch to a tab on the left.
const SLIDE_IN_FROM_RIGHT: &str = "__azul-tabs-slide-in-from-right";
/// The content group's exit, the same for both directions.
const FADE_OUT: &str = "__azul-tabs-fade-out";
/// How far the content travels as it slides in, in px: a hint of the
/// direction, not a page turn.
const SLIDE_PX: isize = 16;
/// How long the slide-in takes.
const SLIDE_IN_MS: u32 = 220;
/// How long the fade-out takes: shorter than the slide, so the old content
/// is gone before the new one settles.
const FADE_OUT_MS: u32 = 120;

/// One stop of a slide or fade: its `x` shift in px and its opacity.
fn slide_stop(permille: u16, x: isize, opacity: f32) -> css::KeyframeStop {
    css::KeyframeStop {
        permille,
        props: CssPropertyVec::from_vec(vec![
            CssProperty::const_opacity(StyleOpacity {
                inner: PercentageValue::new(opacity * 100.0),
            }),
            CssProperty::const_transform(StyleTransformVec::from_vec(vec![
                StyleTransform::TranslateX(PixelValue::const_px(x)),
            ])),
        ]),
    }
}

/// The three tracks a switched panel's content group runs.
fn slide_keyframes() -> css::Css {
    use azul_css::css::{KeyframeStopVec, Keyframes, KeyframesVec};
    let track = |name: &'static str, stops: Vec<css::KeyframeStop>| Keyframes {
        name: AzString::from_const_str(name),
        stops: KeyframeStopVec::from_vec(stops),
    };
    let mut css = css::Css::empty();
    css.keyframes = KeyframesVec::from_vec(vec![
        track(
            SLIDE_IN_FROM_LEFT,
            vec![slide_stop(0, -SLIDE_PX, 0.0), slide_stop(1000, 0, 1.0)],
        ),
        track(
            SLIDE_IN_FROM_RIGHT,
            vec![slide_stop(0, SLIDE_PX, 0.0), slide_stop(1000, 0, 1.0)],
        ),
        track(
            FADE_OUT,
            vec![
                css::KeyframeStop {
                    permille: 0,
                    props: CssPropertyVec::from_vec(vec![CssProperty::const_opacity(
                        StyleOpacity {
                            inner: PercentageValue::new(100.0),
                        },
                    )]),
                },
                css::KeyframeStop {
                    permille: 1000,
                    props: CssPropertyVec::from_vec(vec![CssProperty::const_opacity(
                        StyleOpacity {
                            inner: PercentageValue::new(0.0),
                        },
                    )]),
                },
            ],
        ),
    ]);
    css
}

/// `-azul-animation-in` / `-out` naming `track`, run once over `millis` -
/// only where the reader has not asked for less motion.
fn slide_animation(
    out: bool,
    track: &'static str,
    millis: u32,
    timing: AnimationTiming,
) -> CssPropertyWithConditions {
    use azul_css::{
        dynamic_selector::{BoolCondition, DynamicSelector},
        props::basic::animation::{AnimationIterationCount, StyleAnimation, StyleAnimationVec},
    };
    const NO_REDUCED_MOTION: &[DynamicSelector] =
        &[DynamicSelector::PrefersReducedMotion(BoolCondition::False)];
    let list = StyleAnimationVecValue::Exact(StyleAnimationVec::from_vec(vec![StyleAnimation {
        name: AzString::from_const_str(track),
        duration: CssDuration::from_millis(millis),
        delay: CssDuration::from_millis(0),
        iterations: AnimationIterationCount::Count(1),
        timing,
        clip: true,
    }]));
    let property = if out {
        CssProperty::AnimationOut(list)
    } else {
        CssProperty::AnimationIn(list)
    };
    CssPropertyWithConditions::with_single_condition(property, NO_REDUCED_MOTION)
}

/// The motion of a content group whose strip switched from tab `from` to
/// tab `to` (`None` for no switch): its entrance - in from the side the
/// switch leaves, fading in - and its exit (a fade where it stands), and
/// the keyframes they name, for the node that ships them.
pub(crate) fn switch_motion(
    from: usize,
    to: usize,
) -> Option<(Vec<CssPropertyWithConditions>, css::Css)> {
    use azul_css::props::basic::animation::AnimationTiming;
    let enter = match to.cmp(&from) {
        core::cmp::Ordering::Greater => SLIDE_IN_FROM_LEFT,
        core::cmp::Ordering::Less => SLIDE_IN_FROM_RIGHT,
        core::cmp::Ordering::Equal => return None,
    };
    Some((
        vec![
            slide_animation(false, enter, SLIDE_IN_MS, AnimationTiming::EaseOut),
            slide_animation(true, FADE_OUT, FADE_OUT_MS, AnimationTiming::EaseIn),
        ],
        slide_keyframes(),
    ))
}

impl TabContent {
    #[must_use]
    pub const fn new(content: Dom) -> Self {
        Self {
            content,
            has_padding: true,
            theme: crate::widgets::themes::OptionUiTheme::None,
            slide: TabSlide::None,
            tab: 0,
            reveal: crate::widgets::tab_reveal::OptionTabReveal::None,
            flip: crate::widgets::tab_flip::TabFlip::Off,
            flip_tabs: StringVec::from_const_slice(&[]),
            flip_tab: 0,
            on_flip: OptionTabOnClick::None,
            content_inset: 0.0,
            content_toggle: false,
        }
    }

    /// Fold the panel away with the window's chevron (`TabHeader::with_content_toggle`,
    /// `WindowFlags::tab_content_hidden`): while the content is folded the panel shows nothing, and
    /// whatever lies under it gets the room.
    pub const fn set_content_toggle(&mut self, content_toggle: bool) {
        self.content_toggle = content_toggle;
    }

    /// [`Self::set_content_toggle`] for the builder chain.
    #[must_use]
    pub const fn with_content_toggle(mut self, content_toggle: bool) -> Self {
        self.set_content_toggle(content_toggle);
        self
    }

    /// Leave `px` of room at the panel's left (a rail beside the content, under the tab row): the panel's
    /// box starts that far in while the tab row stays the whole width. Default `0`.
    pub const fn set_content_inset(&mut self, px: f32) {
        self.content_inset = px;
    }

    /// [`Self::set_content_inset`] for the builder chain.
    #[must_use]
    pub const fn with_content_inset(mut self, px: f32) -> Self {
        self.set_content_inset(px);
        self
    }

    /// Let a vertical swipe on the panel flip to the previous / next tab, as `header` says: its
    /// `flip` (`TabHeader::with_flip`), its tabs (the hint names the one a pull goes to), its
    /// active tab and its `on_click`, which is told of every flip exactly as of a tap on a pill.
    ///
    /// The rules: content that can scroll scrolls first, and a pull that starts at its end
    /// flips; content that does not scroll flips on a swipe; a finger that carries on flips one
    /// tab per 56 px of travel, a fast lift carries over up to four more tabs in a slowing
    /// rhythm, a long page flips once per gesture; never while the finger went down on a nested
    /// scrollable or on a node of the class `__azul-claims-gesture` (a slider, a map, a canvas),
    /// never for a gesture that is more sideways than vertical, never after a cancel (the system
    /// took the finger); the first tab has no previous and the last no next. A tab bar without an
    /// `on_click` has nobody to tell: the panel does not flip.
    pub fn set_flip_of(&mut self, header: &TabHeader) {
        self.flip = header.flip;
        self.flip_tabs = header.tabs.clone();
        self.flip_tab = header.active_tab;
        self.on_flip = header.on_click.clone();
    }

    /// [`Self::set_flip_of`] for the builder chain.
    #[must_use]
    pub fn with_flip_of(mut self, header: &TabHeader) -> Self {
        self.set_flip_of(header);
        self
    }

    /// The reveal its `TabHeader` was built with: the panel opens and closes
    /// by its height as the reveal says, and the pointer on it keeps it open.
    pub fn set_reveal(&mut self, reveal: crate::widgets::tab_reveal::TabReveal) {
        self.reveal = crate::widgets::tab_reveal::OptionTabReveal::Some(reveal);
    }

    /// [`Self::set_reveal`] for the builder chain.
    #[must_use]
    pub fn with_reveal(mut self, reveal: crate::widgets::tab_reveal::TabReveal) -> Self {
        self.set_reveal(reveal);
        self
    }

    /// The tab bar switched from tab `from` to tab `to` and this panel now
    /// shows `to`: its content slides in from the side the switch leaves -
    /// right into place for a tab on the right, left for one on the left -
    /// as it fades in, and the content it replaces fades out. Keep passing
    /// the last switch on every rebuild: the same switch again keeps the
    /// content still; `from == to` is no switch.
    pub const fn set_switch(&mut self, from: usize, to: usize) {
        self.slide = if to > from {
            TabSlide::Right
        } else if to < from {
            TabSlide::Left
        } else {
            TabSlide::None
        };
        self.tab = to;
    }

    /// [`Self::set_switch`] for the builder chain.
    #[must_use]
    pub const fn with_switch(mut self, from: usize, to: usize) -> Self {
        self.set_switch(from, to);
        self
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
        let panel = match self.reveal.as_ref() {
            Some(reveal) => {
                let mut style = tab_content_css_style.into_library_owned_vec();
                if reveal.get_behaviour()
                    != crate::widgets::tab_reveal::TabClickBehaviour::AlwaysOpen
                {
                    style.extend(crate::widgets::tab_reveal::content_motion(reveal.is_open()));
                }
                let owner = crate::widgets::tab_reveal::RevealOwner::Header(
                    OptionTabOnClick::None,
                );
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
                    .with_callbacks(
                        crate::widgets::tab_reveal::area_callbacks(reveal, &owner).into(),
                    )
            }
            None => Dom::create_div().with_css_props(tab_content_css_style),
        };
        let panel = match look.marker {
            None => panel,
            Some(marker) => panel.with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
                AzString::from_const_str(marker),
            )])),
        };

        let flip = flip_panel_data(&self);
        let mut group = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(
                IDS_AND_CLASSES_2989815829020816222,
            ))
            .with_children(DomVec::from_vec(vec![self.content]));
        let panel = match self.slide {
            TabSlide::None => panel,
            slide => {
                use azul_css::props::basic::animation::AnimationTiming;
                let enter = match slide {
                    TabSlide::Left => SLIDE_IN_FROM_RIGHT,
                    _ => SLIDE_IN_FROM_LEFT,
                };
                group = group
                    .with_key(("azul-tab-panel", self.tab))
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        slide_animation(false, enter, SLIDE_IN_MS, AnimationTiming::EaseOut),
                        slide_animation(true, FADE_OUT, FADE_OUT_MS, AnimationTiming::EaseIn),
                    ]));
                panel.with_component_css(slide_keyframes())
            }
        };
        let mut panel = panel.with_children(DomVec::from_vec(vec![group]));
        if self.content_inset > 0.0 {
            panel.root.set_css(&format!("margin-left: {}px;", self.content_inset));
        }
        // Folded away with the window's chevron: nothing, and the list under the panel gets the room.
        if self.content_toggle && azul_core::callbacks::build_tab_content_hidden() {
            panel.root.set_css("display: none;");
        }
        match flip {
            None => panel,
            Some(flip) => attach_flip(panel, flip),
        }
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
            let header = TabHeader::create(labels).with_style(TabStyle::Rectangular);

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
        let dom = TabHeader::create(strings(&[])).with_style(TabStyle::Rectangular).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), 2, "an empty header is just the two spacers");
        assert_eq!(class_strs(&children[0]), vec![CLASS_BEFORE]);
        assert_eq!(class_strs(&children[1]), vec![CLASS_AFTER]);
    }

    #[test]
    fn create_round_trips_pathological_labels_through_the_dom() {
        // The label is only ever cloned into a `NodeType::Text`, never parsed,
        // trimmed or NUL-terminated — so it must survive as the input rule reads
        // every text (NUL as U+FFFD, CR LF as LF), and nothing else may change.
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
        let dom = TabHeader::create(labels).with_style(TabStyle::Rectangular).dom();
        let children = dom.children.as_ref();

        assert_eq!(children.len(), pathological.len() + 2);
        for (i, expected) in pathological.iter().enumerate() {
            assert_eq!(
                text_of(&children[i + 1]),
                Some(&*azul_css::input_chars::normalize_input_text(expected)),
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
            let mut header = TabHeader::create(strings(&["a", "b", "c"])).with_style(TabStyle::Rectangular);
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
        let mut header = TabHeader::create(strings(&["a", "b"])).with_style(TabStyle::Rectangular);
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
            let built = TabHeader::create(strings(&["a", "b"])).with_style(TabStyle::Rectangular).with_active_tab(value);
            let mut mutated = TabHeader::create(strings(&["a", "b"])).with_style(TabStyle::Rectangular);
            mutated.set_active_tab(value);

            assert_eq!(built.active_tab, mutated.active_tab);
            assert_eq!(built.active_tab, value);
            assert_eq!(built.tabs.as_ref(), mutated.tabs.as_ref());
            assert!(built.on_click.is_none() && mutated.on_click.is_none());
        }
    }

    #[test]
    fn with_active_tab_chains_last_wins() {
        let header = TabHeader::create(strings(&["a"])).with_style(TabStyle::Rectangular)
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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
        let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
    fn dom_one_past_the_end_seams_no_tab() {
        // With `active_tab == tabs.len()` no tab is selected, so no tab is
        // drawn as the neighbour of a selected one: the last tab keeps its
        // right border.
        let n = 3usize;
        let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
            .with_active_tab(n)
            .dom();
        let children = dom.children.as_ref();
        for (i, node) in children[1..=n].iter().enumerate() {
            assert_eq!(class_strs(node), vec![CLASS_NOT_ACTIVE], "tab {i}");
        }
    }

    // ==================================================================
    // TabHeader::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_moves_the_state_out_and_leaves_a_default() {
        let mut header = TabHeader::create(strings(&["a", "b", "c"])).with_style(TabStyle::Rectangular).with_active_tab(2);
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
        let mut header = TabHeader::create(strings(&["a"])).with_style(TabStyle::Rectangular).with_active_tab(usize::MAX);
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
        let mut header = TabHeader::create(strings(&["a"])).with_style(TabStyle::Rectangular);
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
        let before = TabHeader::create(strings(&["a", "b"])).with_style(TabStyle::Rectangular).with_active_tab(1);
        let mut after = TabHeader::create(strings(&["a", "b"])).with_style(TabStyle::Rectangular)
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
        let mut header = TabHeader::create(strings(&["a"])).with_style(TabStyle::Rectangular);
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
        let mut header = TabHeader::create(strings(&["a"])).with_style(TabStyle::Rectangular);

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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
        let dom = TabHeader::create(numbered_labels(5)).with_style(TabStyle::Rectangular)
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
        let dom = TabHeader::create(numbered_labels(4)).with_style(TabStyle::Rectangular)
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
        let dom = TabHeader::create(numbered_labels(4)).with_style(TabStyle::Rectangular)
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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
        let dom = TabHeader::create(numbered_labels(4)).with_style(TabStyle::Rectangular)
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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular).dom();
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
        let dom = TabHeader::create(numbered_labels(2)).with_style(TabStyle::Rectangular).dom();
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
        let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular).dom();
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
            let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
            let dom = TabHeader::create(numbered_labels(5)).with_style(TabStyle::Rectangular)
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
        let dom = TabHeader::create(numbered_labels(n)).with_style(TabStyle::Rectangular)
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
            TabHeader::create(numbered_labels(3)).with_style(TabStyle::Rectangular).with_on_click(user.clone(), cb(record_click));
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
        TabHeader::create(strings(&["one", "two", "three"])).with_style(TabStyle::Rectangular)
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
        let styled = tab_page(TabHeader::create(strings(&["one", "two", "three"])).with_style(TabStyle::Rectangular));
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
                TabHeader::create(strings(&["one", "two", "three"])).with_style(TabStyle::Rectangular).with_active_tab(1)
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
        )).with_style(TabStyle::Rectangular)
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
        let header = TabHeader::create(StringVec::from_const_slice(&[])).with_style(TabStyle::Rectangular);
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
        let face = flora::australis_face(flora::TAB_STREAK);
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
        // Its sides are not curves any more: a square tab has none to hang (only the ribbon's row has curves).
        assert_eq!(
            active.children.as_ref().len(),
            1,
            "a square selected tab holds its label and nothing hung off its sides"
        );
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
                "__azul-native-tabs-before-tabs",
                CssPropertyType::Display,
                "flora's square tabs need no room before the first one: its spacer leaves the row",
            ),
            (
                "__azul-native-tabs-tab-active",
                CssPropertyType::Position,
                "flora's selected tab is positioned for what hangs off it",
            ),
            (
                "__azul-native-tabs-tab-not-active",
                CssPropertyType::Position,
                "flora's curved ribbon row hangs curves off every tab",
            ),
            (
                "__azul-native-tabs-tab-active",
                CssPropertyType::ZIndex,
                "flora's selected tab stands over its neighbours' curves",
            ),
            (
                "__azul-native-tabs-tab-not-active",
                CssPropertyType::ZIndex,
                "flora's unselected tabs stand under the selected tab's curves",
            ),
        ];
        for t in BOTH {
            // Each tab active in turn: the active tab, the two seam tabs
            // beside it and the other inactive tabs.
            for active in 0..5 {
                let dom = under(t, || TabHeader::create(labels()).with_style(crate::widgets::tabs::TabStyle::Rectangular).with_active_tab(active).dom());
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

#[path = "tab_row.rs"]
mod tab_row;

#[cfg(test)]
#[path = "tab_pills_test.rs"]
mod tab_pills_test;

#[cfg(test)]
#[path = "tab_flip_panel_test.rs"]
mod tab_flip_panel_test;
