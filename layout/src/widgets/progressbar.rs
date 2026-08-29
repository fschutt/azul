//! Native progress bar widget with customizable backgrounds, height, and
//! gradient styling. The main type is [`ProgressBar`], which is rendered
//! into a DOM via [`ProgressBar::dom()`].
//!
//! # The fast path: updating the bar without a relayout
//!
//! `dom()` wraps the bar in a **`VirtualView`** node whose private dataset
//! holds the [`ProgressBar`]. That makes the widget the reference
//! implementation of the *inter-widget fast path* (see
//! `doc/guide/*/architecture.md`): a callback on some OTHER node (say, a paint
//! area receiving pen pressure) can move this bar live - no app-data-model
//! write, no full `layout()`, no DOM diff - in three steps:
//!
//! 1. The app stamps a MARKER string on the widget's root during `layout()`
//!    (`ProgressBar::dom().with_marker(...)`) and keeps the same string in the
//!    RefAny of the callback that wants to drive the bar.
//! 2. The callback resolves the marker to the node:
//!    `CallbackInfo::get_node_id_by_marker`.
//! 3. It calls [`ProgressBar::update_progress`] with that node id - which
//!    downcasts the widget's PRIVATE dataset (the caller never sees the type),
//!    stores the new percentage, and triggers
//!    `CallbackInfo::trigger_virtual_view_rerender` on the node. The framework
//!    re-invokes ONLY this widget's `VirtualView` callback in place; the
//!    resulting DOM is restyled/relaid out inside the existing bounds and the
//!    damage rect covers just the bar.
//!
//! The heavy path stays valid: store the value in the app data model, return
//! `Update::RefreshDom`, and the full `layout()` + DOM diff repaints the bar
//! like any other widget.

use azul_core::callbacks::{VirtualViewCallbackInfo, VirtualViewReturn};
use azul_core::dom::{Dom, DomNodeId, IdOrClass, IdOrClass::Class, IdOrClassVec};
use azul_core::geom::{LogicalPosition, LogicalRect};
use azul_core::refany::RefAny;
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

use crate::callbacks::CallbackInfo;

const STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_12009347504665939_ITEMS,
        ),
    })];
const STYLE_BACKGROUND_CONTENT_14586281004485141058_ITEMS: &[StyleBackgroundContent] =
    &[StyleBackgroundContent::LinearGradient(LinearGradient {
        direction: Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        }),
        extend_mode: ExtendMode::Clamp,
        stops: NormalizedLinearColorStopVec::from_const_slice(
            LINEAR_COLOR_STOP_3104396762583413726_ITEMS,
        ),
    })];
const LINEAR_COLOR_STOP_12009347504665939_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 193,
            g: 255,
            b: 187,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(10),
        color: ColorOrSystem::color(ColorU {
            r: 205,
            g: 255,
            b: 205,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(15),
        color: ColorOrSystem::color(ColorU {
            r: 156,
            g: 238,
            b: 172,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(20),
        color: ColorOrSystem::color(ColorU {
            r: 0,
            g: 211,
            b: 40,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(30),
        color: ColorOrSystem::color(ColorU {
            r: 0,
            g: 211,
            b: 40,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(70),
        color: ColorOrSystem::color(ColorU {
            r: 32,
            g: 219,
            b: 65,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 32,
            g: 219,
            b: 65,
            a: 255,
        }),
    },
];
const LINEAR_COLOR_STOP_3104396762583413726_ITEMS: &[NormalizedLinearColorStop] = &[
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(0),
        color: ColorOrSystem::color(ColorU {
            r: 243,
            g: 243,
            b: 243,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(10),
        color: ColorOrSystem::color(ColorU {
            r: 252,
            g: 252,
            b: 252,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(15),
        color: ColorOrSystem::color(ColorU {
            r: 218,
            g: 218,
            b: 218,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(20),
        color: ColorOrSystem::color(ColorU {
            r: 201,
            g: 201,
            b: 201,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(30),
        color: ColorOrSystem::color(ColorU {
            r: 218,
            g: 218,
            b: 218,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(70),
        color: ColorOrSystem::color(ColorU {
            r: 203,
            g: 203,
            b: 203,
            a: 255,
        }),
    },
    NormalizedLinearColorStop {
        offset: PercentageValue::const_new(100),
        color: ColorOrSystem::color(ColorU {
            r: 203,
            g: 203,
            b: 203,
            a: 255,
        }),
    },
];

/// A native progress bar widget with customizable bar/container backgrounds and height.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ProgressBar {
    pub progressbar_state: ProgressBarState,
    pub height: PixelValue,
    pub bar_background: StyleBackgroundContentVec,
    pub container_background: StyleBackgroundContentVec,
}

/// Internal state for a [`ProgressBar`], tracking completion percentage.
#[derive(Copy, Debug, Clone)]
#[repr(C)]
pub struct ProgressBarState {
    pub percent_done: f32,
    pub display_percentage: bool,
}

/// The widget's PRIVATE `VirtualView` dataset: the whole [`ProgressBar`],
/// downcast only inside this module ([`ProgressBar::update_progress`] and the
/// `VirtualView` callback). Callers drive it exclusively through the public
/// API - the "RefAny datasets are private to the widget module" rule from
/// `architecture.md`.
#[derive(Debug)]
struct ProgressBarLocalDataset {
    bar: ProgressBar,
}

impl ProgressBar {
    /// Creates a new progress bar with the given completion percentage (0.0 to 100.0).
    #[inline]
    #[must_use]
    pub const fn create(percent_done: f32) -> Self {
        Self {
            progressbar_state: ProgressBarState {
                percent_done,
                display_percentage: false,
            },
            height: PixelValue::const_px(15),
            bar_background: StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS,
            ),
            container_background: StyleBackgroundContentVec::from_const_slice(
                STYLE_BACKGROUND_CONTENT_14586281004485141058_ITEMS,
            ),
        }
    }

    /// Replaces `self` with a default (0%) progress bar, returning the previous value.
    #[inline]
    #[must_use]
    pub const fn swap_with_default(&mut self) -> Self {
        let mut s = Self::create(0.0);
        core::mem::swap(&mut s, self);
        s
    }

    pub fn set_container_background(&mut self, background: StyleBackgroundContentVec) {
        self.container_background = background;
    }

    #[must_use]
    pub fn with_container_background(mut self, background: StyleBackgroundContentVec) -> Self {
        self.set_container_background(background);
        self
    }

    pub fn set_bar_background(&mut self, background: StyleBackgroundContentVec) {
        self.bar_background = background;
    }

    #[must_use]
    pub fn with_bar_background(mut self, background: StyleBackgroundContentVec) -> Self {
        self.set_bar_background(background);
        self
    }

    pub const fn set_height(&mut self, height: PixelValue) {
        self.height = height;
    }

    #[must_use]
    pub const fn with_height(mut self, height: PixelValue) -> Self {
        self.set_height(height);
        self
    }

    /// Build the widget: a `VirtualView` node (explicitly sized to
    /// [`height`](Self::height)) whose callback renders [`render_bar`]
    /// (Self::render_bar) into the node's bounds. The node carries the
    /// widget's private dataset so [`update_progress`](Self::update_progress)
    /// can find and mutate it later - see the module docs for the fast path.
    #[must_use]
    pub fn dom(self) -> Dom {
        // The VV node itself must have a DEFINITE height: the callback's DOM
        // is laid out INSIDE the node's bounds, it can never size the node.
        let height = self.height;
        let dataset = RefAny::new(ProgressBarLocalDataset { bar: self });
        Dom::create_virtual_view(
            dataset.clone(),
            azul_core::callbacks::VirtualViewCallback::create(progressbar_render_virtual_view),
        )
            .with_dataset(Some(dataset).into())
            .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                    LayoutHeight::Px(height),
                ))),
                // A VirtualView lays out like a REPLACED element: without an
                // explicit width it takes the 300px intrinsic default instead
                // of stretching to its parent (found 2026-08-29: a 140px-wide
                // meter rendered a 300px bar, clipped at the window edge, so
                // 50% read as ~93%). 100% = fill whatever box the app gives.
                CssPropertyWithConditions::simple(CssProperty::Width(LayoutWidthValue::Exact(
                    LayoutWidth::Px(PixelValue::percent(100.0)),
                ))),
                // materialized == bounds, so nothing ever overflows - this
                // just guarantees the VirtualView machinery never decides to
                // show a scrollbar on a progress bar.
                CssPropertyWithConditions::simple(CssProperty::OverflowX(
                    LayoutOverflowValue::Exact(LayoutOverflow::Hidden),
                )),
                CssPropertyWithConditions::simple(CssProperty::OverflowY(
                    LayoutOverflowValue::Exact(LayoutOverflow::Hidden),
                )),
            ]))
    }

    /// Update the percentage of the progress bar at `node_id` - the WIDGET
    /// half of the inter-widget fast path (module docs).
    ///
    /// `node_id` is the widget's root node, typically resolved from another
    /// callback via `CallbackInfo::get_node_id_by_marker`. The bar's private
    /// dataset is downcast INSIDE this function, the new value stored, and the
    /// node's `VirtualView` re-render queued
    /// (`CallbackInfo::trigger_virtual_view_rerender`) - so only this widget
    /// re-renders, inside its existing bounds, with a damage rect of just the
    /// bar. No full `layout()`, no DOM diff, no app-data-model round trip.
    ///
    /// Returns `false` (and changes nothing) when `node_id` does not name a
    /// live progress-bar node: no layout result, no dataset, or a dataset of
    /// some other widget's type. The value is stored raw like
    /// [`create`](Self::create) does; `dom()`-side clamping applies when the
    /// bar renders.
    pub fn update_progress(info: &mut CallbackInfo, node_id: DomNodeId, percent_done: f32) -> bool {
        let Some(mut dataset) = info.get_dataset(node_id) else {
            return false;
        };
        {
            // Scoped: the downcast guard must drop before the re-render
            // trigger runs the queue drain path.
            let Some(mut state) = dataset.downcast_mut::<ProgressBarLocalDataset>() else {
                return false;
            };
            state.bar.progressbar_state.percent_done = percent_done;
        }
        let Some(node) = node_id.node.into_crate_internal() else {
            return false;
        };
        info.trigger_virtual_view_rerender(node_id.dom, node);
        true
    }

    /// Renders this progress bar into a [`Dom`] tree consisting of a container div
    /// with two children: the filled bar and the remaining empty space.
    ///
    /// This is the tree the `VirtualView` callback returns on every
    /// (re-)render; [`dom`](Self::dom) is the wrapper that mounts it. Calling
    /// it directly gives the bare bar with NO fast-path machinery - useful for
    /// embedding in a context that manages its own updates.
    #[must_use]
    pub fn render_bar(self) -> Dom {
        render_bar_impl(self, None)
    }
}

/// The render core behind [`ProgressBar::render_bar`] (percentage widths,
/// `bounds_px: None`) and the `VirtualView` callback (absolute pixel sizes
/// computed from the node's known bounds, `Some((width, height))`).
///
/// The split exists because the two contexts size differently: in normal flow
/// the children can use CSS percentages of the container, but inside a
/// `VirtualView` a child's percentage width does not resolve against the
/// materialized bounds (engine gap, 2026-08-29 - the fill rendered
/// full-width regardless of the percentage). The VV callback KNOWS its
/// bounds in pixels, so it renders exact pixel sizes instead - which is also
/// the more honest spelling for a bounds-aware renderer. In the bounds mode
/// the container is additionally sized to `bounds - 2px borders` so its
/// 1px border ring lands INSIDE the box: with the normal-flow sizing
/// (content height + borders) the ring overflowed the VV node and was
/// clipped away at the right and bottom ("oddly cut off", user report
/// 2026-08-29).
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use]
fn render_bar_impl(bar: ProgressBar, bounds_px: Option<(f32, f32)>) -> Dom {
    {
        use azul_core::dom::DomVec;

        let this = bar;
        let percent_done = this.progressbar_state.percent_done.clamp(0.0, 100.0);
        // Sizes resolved per context (see fn docs). The bounds branch
        // subtracts the container's 1px border ring so children + borders
        // exactly fill the VV box.
        let (bar_width, remaining_width) = match bounds_px {
            Some((w, _)) => {
                let inner = (w - 2.0).max(0.0);
                let filled = inner * percent_done / 100.0;
                (PixelValue::px(filled), PixelValue::px(inner - filled))
            }
            None => (
                PixelValue::percent(percent_done),
                PixelValue::percent(100.0 - percent_done),
            ),
        };
        let container_height = match bounds_px {
            Some((_, h)) => PixelValue::px((h - 2.0).max(0.0)),
            None => this.height,
        };

        let mut container_props = vec![
                // .__azul-native-progress-bar-container
                CssPropertyWithConditions::simple(CssProperty::Height(LayoutHeightValue::Exact(
                    LayoutHeight::Px(container_height),
                ))),
                // `display: flex` is LOAD-BEARING: azul's default display is
                // BLOCK, so `flex-direction: row` alone stacks the two
                // children as full-width, zero-height block boxes - the fill
                // never painted anywhere the widget was used (found 2026-08-29
                // via the azpaint pressure meter; also the real culprit behind
                // the "inline-width meter never repaints" ledger entry).
                CssPropertyWithConditions::simple(CssProperty::Display(
                    LayoutDisplayValue::Exact(LayoutDisplay::Flex),
                )),
                CssPropertyWithConditions::simple(CssProperty::FlexDirection(
                    LayoutFlexDirectionValue::Exact(LayoutFlexDirection::Row),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                    StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                        offset_x: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        offset_y: PixelValueNoPercent {
                            inner: PixelValue::const_px(0),
                        },
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 9,
                        },
                        blur_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(15),
                        },
                        spread_radius: PixelValueNoPercent {
                            inner: PixelValue::const_px(2),
                        },
                        clip_mode: BoxShadowClipMode::Inset,
                    })),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                    StyleBorderBottomRightRadiusValue::Exact(StyleBorderBottomRightRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                    StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                    StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                    StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                        inner: PixelValue::const_px(3),
                    }),
                )),
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
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderLeftColor(
                    StyleBorderLeftColorValue::Exact(StyleBorderLeftColor {
                        inner: ColorU {
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderRightColor(
                    StyleBorderRightColorValue::Exact(StyleBorderRightColor {
                        inner: ColorU {
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BorderTopColor(
                    StyleBorderTopColorValue::Exact(StyleBorderTopColor {
                        inner: ColorU {
                            r: 178,
                            g: 178,
                            b: 178,
                            a: 255,
                        },
                    }),
                )),
                CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                    StyleBackgroundContentVecValue::Exact(this.container_background.clone()),
                )),
        ];
        if let Some((w, _)) = bounds_px {
            container_props.push(CssPropertyWithConditions::simple(CssProperty::Width(
                LayoutWidthValue::Exact(LayoutWidth::Px(PixelValue::px((w - 2.0).max(0.0)))),
            )));
        }

        Dom::create_div()
            .with_css_props(CssPropertyWithConditionsVec::from_vec(container_props))
            .with_ids_and_classes({
                const IDS_AND_CLASSES_10874511710181900075: &[IdOrClass] = &[Class(
                    AzString::from_const_str("__azul-native-progress-bar-container"),
                )];
                IdOrClassVec::from_const_slice(IDS_AND_CLASSES_10874511710181900075)
            })
            // For a progress bar the VALUE is the content: two coloured divs
            // say nothing to a screen reader, "75%" says everything. Published
            // on every build so it tracks the bar; a callback that moves the
            // bar live without a rebuild keeps it current with
            // `CallbackInfo::set_accessibility_value` on this node.
            .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
                role: azul_core::a11y::AccessibilityRole::ProgressBar,
                accessibility_value: Some(AzString::from(alloc::format!(
                    "{:.0}%",
                    // NaN clamps to NaN and would read "NaN%"; an unknown
                    // value announces as empty, like the bar it draws.
                    if percent_done.is_finite() { percent_done } else { 0.0 }
                )))
                .into(),
                ..Default::default()
            })
            .with_children(DomVec::from_vec(vec![
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-bar
                        // Use percentage width instead of flex-grow hack
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(bar_width)),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowBottom(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowTop(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowRight(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BoxShadowLeft(
                            StyleBoxShadowValue::Exact(BoxOrStatic::heap(StyleBoxShadow {
                                offset_x: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                offset_y: PixelValueNoPercent {
                                    inner: PixelValue::const_px(0),
                                },
                                color: ColorU {
                                    r: 0,
                                    g: 51,
                                    b: 0,
                                    a: 51,
                                },
                                blur_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(15),
                                },
                                spread_radius: PixelValueNoPercent {
                                    inner: PixelValue::const_px(12),
                                },
                                clip_mode: BoxShadowClipMode::Inset,
                            })),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomRightRadius(
                            StyleBorderBottomRightRadiusValue::Exact(
                                StyleBorderBottomRightRadius {
                                    inner: PixelValue::const_px(1),
                                },
                            ),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderBottomLeftRadius(
                            StyleBorderBottomLeftRadiusValue::Exact(StyleBorderBottomLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopRightRadius(
                            StyleBorderTopRightRadiusValue::Exact(StyleBorderTopRightRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BorderTopLeftRadius(
                            StyleBorderTopLeftRadiusValue::Exact(StyleBorderTopLeftRadius {
                                inner: PixelValue::const_px(1),
                            }),
                        )),
                        CssPropertyWithConditions::simple(CssProperty::BackgroundContent(
                            StyleBackgroundContentVecValue::Exact(this.bar_background),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_16512648314570682783: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-bar"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_16512648314570682783)
                    }),
                Dom::create_div()
                    .with_css_props(CssPropertyWithConditionsVec::from_vec(vec![
                        // .__azul-native-progress-bar-remaining
                        // Use percentage width for the remaining space
                        CssPropertyWithConditions::simple(CssProperty::Width(
                            LayoutWidthValue::Exact(LayoutWidth::Px(remaining_width)),
                        )),
                    ]))
                    .with_ids_and_classes({
                        const IDS_AND_CLASSES_2492405364126620395: &[IdOrClass] = &[Class(
                            AzString::from_const_str("__azul-native-progress-bar-remaining"),
                        )];
                        IdOrClassVec::from_const_slice(IDS_AND_CLASSES_2492405364126620395)
                    }),
            ]))
    }
}

/// The widget's `VirtualView` callback: render the CURRENT state of the bar
/// into the node's bounds. Invoked on mount and again every time
/// [`ProgressBar::update_progress`] queues a re-render.
///
/// The bar is not scrollable content, so all three rects collapse to one:
/// `materialized` == `virtual_rect` == the container's box at origin zero.
extern "C" fn progressbar_render_virtual_view(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let Some(state) = data.downcast_ref::<ProgressBarLocalDataset>() else {
        // Foreign payload: render nothing rather than lying about bounds.
        return VirtualViewReturn::default();
    };
    let size = info.bounds.get_logical_size();
    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    // Clone-per-render is two enum copies + an `AzString`-less state copy; the
    // backgrounds are either `&'static` (shared, no alloc) or a caller-owned
    // heap vec that must be preserved for the NEXT render anyway. Pixel
    // widths, not percentages: the callback knows its bounds (see
    // `render_bar_impl`).
    VirtualViewReturn::with_dom(
        render_bar_impl(state.bar.clone(), Some((size.width, size.height))),
        rect,
        rect,
    )
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::unreadable_literal,
    clippy::too_many_lines
)]
mod autotest_generated {
    use std::collections::HashSet;

    use azul_core::dom::NodeType;

    use super::*;

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// Every `f32` a caller can realistically hand to `ProgressBar::create`.
    /// The percentage is stored raw and only clamped inside `dom()`, where it is
    /// pushed through `PixelValue::percent` — which multiplies by 1000 and casts
    /// to `isize`. That cast saturates (NaN → 0, out of range → `isize::MIN/MAX`),
    /// so none of these may panic or wrap.
    ///
    /// `NAN` is deliberately absent: it is the one input that is unordered against
    /// the clamp bounds, so it gets its own test.
    const ADVERSARIAL_PERCENTS: [f32; 16] = [
        0.0,
        -0.0,
        1.0,
        50.0,
        100.0,
        -1.0,
        101.0,
        0.001,
        -0.001,
        f32::EPSILON,
        f32::MIN_POSITIVE,
        -f32::MIN_POSITIVE,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ];

    /// Heights that stress the `f32 → isize` fixed-point encoding behind
    /// `PixelValue`: zero, both signed zeroes, the saturating extremes, NaN, and
    /// the relative metrics the widget is not supposed to reject.
    fn adversarial_heights() -> Vec<PixelValue> {
        vec![
            PixelValue::zero(),
            PixelValue::const_px(0),
            PixelValue::px(-0.0),
            PixelValue::px(-1.0),
            PixelValue::px(0.001),
            PixelValue::px(f32::MAX),
            PixelValue::px(f32::MIN),
            PixelValue::px(f32::INFINITY),
            PixelValue::px(f32::NEG_INFINITY),
            PixelValue::px(f32::NAN),
            PixelValue::percent(100.0),
            PixelValue::em(0.001),
            // The largest whole-pixel value `const_px` can scale by 1000 without
            // overflowing `isize` (one more would be a debug-build panic *inside
            // the argument*, not inside `set_height`).
            PixelValue::const_px(isize::MAX / 1000),
        ]
    }

    /// The raw fixed-point encoding of a length: `FloatValue` stores `value * 1000`
    /// as an `isize`, so this is what actually survives — comparing it avoids a
    /// second lossy float round-trip through `get()`.
    fn raw(pv: PixelValue) -> isize {
        pv.number.number()
    }

    /// The addresses `ProgressBar::create` hands out for the two static
    /// gradients — the reference every "is this still borrowed?" assertion below
    /// compares against.
    ///
    /// Deliberately NOT `STYLE_BACKGROUND_CONTENT_*_ITEMS.as_ptr()`. Those are
    /// `const` items, and every *use site* of a `const &[T]` gets its own
    /// promoted read-only allocation; two use sites share an address only if the
    /// optimizer merges them, which it does in an optimized build and does not
    /// in an unoptimized one. Comparing a `create()` pointer against the const
    /// was therefore an accidental green that held only because the suite had
    /// never been run on the dev profile. `create()` contains ONE use site of
    /// each const, so the address it returns is stable across calls — and that
    /// is exactly the property under test: a `create()` that copied the slice
    /// into a heap vec would hand out a fresh address every time.
    fn create_gradient_ptrs() -> (*const StyleBackgroundContent, *const StyleBackgroundContent) {
        let pb = ProgressBar::create(0.0);
        (pb.bar_background.as_ptr(), pb.container_background.as_ptr())
    }

    /// A heap-allocated background of `n` distinct solid colours. Heap-backed on
    /// purpose: it is the only case where the vec owns memory that can be
    /// double-freed or leaked.
    fn solid(n: usize) -> StyleBackgroundContentVec {
        StyleBackgroundContentVec::from_vec(
            (0..n)
                .map(|i| {
                    StyleBackgroundContent::Color(ColorU {
                        r: (i % 256) as u8,
                        g: 1,
                        b: 2,
                        a: 255,
                    })
                })
                .collect(),
        )
    }

    fn kids(dom: &Dom) -> &[Dom] {
        dom.children.as_ref()
    }

    /// The filled part (`.__azul-native-progress-bar-bar`).
    fn bar(dom: &Dom) -> &Dom {
        &kids(dom)[0]
    }

    /// The empty part (`.__azul-native-progress-bar-remaining`).
    fn remaining(dom: &Dom) -> &Dom {
        &kids(dom)[1]
    }

    /// The declared properties of a node's inline style, in declaration order.
    fn inline_props(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The CSS classes of a node, in declaration order.
    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    fn width_of(dom: &Dom) -> Option<PixelValue> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Width(v) => match v.get_property() {
                    Some(LayoutWidth::Px(pv)) => Some(*pv),
                    Some(other) => panic!("the progress bar must size in lengths, got {other:?}"),
                    None => None,
                },
                _ => None,
            })
    }

    fn height_of(dom: &Dom) -> Option<PixelValue> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::Height(v) => match v.get_property() {
                    Some(LayoutHeight::Px(pv)) => Some(*pv),
                    Some(other) => panic!("the progress bar must size in lengths, got {other:?}"),
                    None => None,
                },
                _ => None,
            })
    }

    /// The background layers a node declares, cloned out of the DOM.
    fn background_of(dom: &Dom) -> Option<Vec<StyleBackgroundContent>> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::BackgroundContent(v) => v.get_property().map(|b| b.as_ref().to_vec()),
                _ => None,
            })
    }

    /// The *address* of a node's background buffer — the only way to tell a move
    /// from a copy, and a copy from a use-after-free.
    fn background_ptr(dom: &Dom) -> Option<*const StyleBackgroundContent> {
        dom.root
            .style
            .iter_inline_properties()
            .find_map(|(p, _)| match p {
                CssProperty::BackgroundContent(v) => {
                    v.get_property().map(StyleBackgroundContentVec::as_ptr)
                }
                _ => None,
            })
    }

    /// Every absolute length a chrome property declares (box shadows excluded —
    /// they carry `PixelValueNoPercent`, which cannot express a relative unit).
    fn lengths_of(p: &CssProperty) -> Vec<PixelValue> {
        let one = |pv: Option<PixelValue>| pv.into_iter().collect::<Vec<_>>();
        match p {
            CssProperty::BorderBottomWidth(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderLeftWidth(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderRightWidth(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderTopWidth(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderBottomRightRadius(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderBottomLeftRadius(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderTopRightRadius(v) => one(v.get_property().map(|x| x.inner)),
            CssProperty::BorderTopLeftRadius(v) => one(v.get_property().map(|x| x.inner)),
            _ => Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // ProgressBar::create
    // ------------------------------------------------------------------

    #[test]
    fn create_stores_the_percentage_bit_for_bit_and_never_normalises_it() {
        // `create` is documented as taking 0.0..=100.0 but performs no validation:
        // whatever comes in has to come back out untouched, sign of zero included.
        for p in ADVERSARIAL_PERCENTS {
            let pb = ProgressBar::create(p);
            assert_eq!(
                pb.progressbar_state.percent_done.to_bits(),
                p.to_bits(),
                "create() rewrote the percentage {p}",
            );
            assert!(
                !pb.progressbar_state.display_percentage,
                "a fresh progress bar must not opt into the percentage label",
            );
        }

        // NaN cannot be compared, only inspected.
        let nan = ProgressBar::create(f32::NAN);
        assert!(
            nan.progressbar_state.percent_done.is_nan(),
            "create() silently replaced a NaN percentage",
        );
    }

    #[test]
    fn create_defaults_to_a_15px_height() {
        let pb = ProgressBar::create(50.0);
        assert_eq!(
            pb.height.metric,
            SizeMetric::Px,
            "the default height must be absolute"
        );
        assert_eq!(
            raw(pb.height),
            15_000,
            "the default height is 15px in 1/1000 units"
        );
    }

    #[test]
    fn create_borrows_the_static_gradients_instead_of_allocating_them() {
        let pb = ProgressBar::create(50.0);
        let (bar_ptr, container_ptr) = create_gradient_ptrs();

        // Pointer identity, not just content equality: a `create()` that copied the
        // static slice into a heap vec would allocate on every frame, and one that
        // kept the static pointer but claimed ownership of it would free `&'static`
        // memory on drop.
        assert_eq!(
            pb.bar_background.as_ptr(),
            bar_ptr,
            "the bar gradient stopped being shared with the static slice",
        );
        assert_eq!(
            pb.container_background.as_ptr(),
            container_ptr,
            "the container gradient stopped being shared with the static slice",
        );
        // Content still pinned to the declared constants, so "shared" cannot
        // degrade into "shared with something else".
        assert_eq!(
            pb.bar_background.as_ref(),
            STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS,
        );
        assert_eq!(
            pb.container_background.as_ref(),
            STYLE_BACKGROUND_CONTENT_14586281004485141058_ITEMS,
        );
        assert_eq!(pb.bar_background.len(), 1);
        assert_eq!(pb.container_background.len(), 1);
        assert_eq!(
            pb.bar_background.capacity(),
            pb.bar_background.len(),
            "a borrowed buffer must report capacity == len, or the free path over-reads",
        );

        // 10_000 bars built and dropped: if the destructor of a static-backed vec
        // were ever flipped to the owning one, this frees the same `&'static`
        // allocation 10_000 times.
        for i in 0..10_000 {
            let pb = ProgressBar::create(i as f32);
            assert_eq!(pb.bar_background.len(), 1);
            assert_eq!(pb.bar_background.as_ptr(), bar_ptr);
        }
    }

    #[test]
    fn create_backgrounds_are_the_declared_gradients_with_sorted_stops() {
        let pb = ProgressBar::create(0.0);
        assert_eq!(
            pb.bar_background.as_ref(),
            STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS,
        );
        assert_eq!(
            pb.container_background.as_ref(),
            STYLE_BACKGROUND_CONTENT_14586281004485141058_ITEMS,
        );

        for bg in [&pb.bar_background, &pb.container_background] {
            match &bg.as_ref()[0] {
                StyleBackgroundContent::LinearGradient(g) => {
                    let stops = g.stops.as_ref();
                    assert_eq!(stops.len(), 7, "a gradient lost or gained a colour stop");

                    // Unsorted or out-of-range stops make the gradient renderer's
                    // interpolation run backwards over a segment.
                    let mut prev = f32::NEG_INFINITY;
                    for s in stops {
                        let offset = s.offset.normalized() * 100.0;
                        assert!(
                            (0.0..=100.0).contains(&offset),
                            "gradient stop outside 0%..100%: {offset}",
                        );
                        assert!(
                            offset >= prev,
                            "gradient stops are not sorted: {offset} follows {prev}",
                        );
                        prev = offset;
                    }
                }
                other => panic!("the progress bar gradients degraded to {other:?}"),
            }
        }
    }

    #[test]
    fn create_is_usable_in_const_context() {
        // `create` is `const fn`; a caller may therefore build a bar as a `const`
        // item. That only const-evaluates while the backgrounds stay
        // `from_const_slice` (a heap allocation would not be const-evaluable).
        const CONST_BAR: ProgressBar = ProgressBar::create(12.5);

        assert_eq!(CONST_BAR.progressbar_state.percent_done, 12.5);
        assert_eq!(raw(CONST_BAR.height), 15_000);
        assert_eq!(CONST_BAR.bar_background.len(), 1);
    }

    // ------------------------------------------------------------------
    // ProgressBar::swap_with_default
    // ------------------------------------------------------------------

    #[test]
    fn swap_with_default_returns_the_previous_bar_and_installs_a_pristine_one() {
        for p in ADVERSARIAL_PERCENTS {
            let mut pb = ProgressBar::create(p).with_height(PixelValue::const_px(99));
            let prev = pb.swap_with_default();

            assert_eq!(
                prev.progressbar_state.percent_done.to_bits(),
                p.to_bits(),
                "the returned bar is not the one that was there ({p})",
            );
            assert_eq!(raw(prev.height), 99_000, "the returned bar lost its height");

            assert_eq!(
                pb.progressbar_state.percent_done.to_bits(),
                0_u32,
                "the replacement must be +0.0 — a -0.0 would encode with the sign bit set",
            );
            assert_eq!(
                raw(pb.height),
                15_000,
                "the replacement must use the default height"
            );
            assert_eq!(
                pb.bar_background.as_ptr(),
                create_gradient_ptrs().0,
                "the replacement must borrow the static gradient again",
            );
        }
    }

    #[test]
    fn swap_with_default_keeps_a_nan_percentage_and_moves_owned_memory_out() {
        let owned = solid(4);
        let ptr = owned.as_ptr();
        let mut pb = ProgressBar::create(f32::NAN).with_bar_background(owned);

        let prev = pb.swap_with_default();

        assert!(
            prev.progressbar_state.percent_done.is_nan(),
            "a NaN percentage did not survive the swap",
        );
        assert_eq!(
            prev.bar_background.as_ptr(),
            ptr,
            "the heap buffer was copied instead of moved out",
        );
        assert_eq!(prev.bar_background.len(), 4);

        // Dropping the previous value frees that heap buffer. If `swap_with_default`
        // had left `self` pointing at it too, everything below would be a
        // use-after-free.
        drop(prev);
        assert_eq!(
            pb.bar_background.as_ref(),
            STYLE_BACKGROUND_CONTENT_2688422633177340412_ITEMS,
            "the swapped-in bar aliased the memory that was just freed",
        );
        assert_eq!(pb.progressbar_state.percent_done, 0.0);
    }

    #[test]
    fn repeated_swaps_never_alias_or_leak_the_backgrounds() {
        let mut pb = ProgressBar::create(1.0);
        let (bar_ptr, _) = create_gradient_ptrs();
        for i in 0..1_000_usize {
            let want = i % 8 + 1;
            pb.set_bar_background(solid(want));
            let prev = pb.swap_with_default();

            assert_eq!(
                prev.bar_background.len(),
                want,
                "round {i} handed back the wrong buffer"
            );
            assert_eq!(pb.progressbar_state.percent_done, 0.0);
            assert_eq!(pb.bar_background.as_ptr(), bar_ptr);
        }
    }

    // ------------------------------------------------------------------
    // set_/with_ background
    // ------------------------------------------------------------------

    #[test]
    fn each_background_setter_touches_exactly_one_field() {
        let mut pb = ProgressBar::create(50.0);
        let bar_ptr = pb.bar_background.as_ptr();
        pb.set_container_background(solid(3));
        assert_eq!(pb.container_background.len(), 3);
        assert_eq!(
            pb.bar_background.as_ptr(),
            bar_ptr,
            "set_container_background clobbered the bar background",
        );

        let mut pb = ProgressBar::create(50.0);
        let container_ptr = pb.container_background.as_ptr();
        pb.set_bar_background(solid(5));
        assert_eq!(pb.bar_background.len(), 5);
        assert_eq!(
            pb.container_background.as_ptr(),
            container_ptr,
            "set_bar_background clobbered the container background",
        );
        assert_eq!(pb.progressbar_state.percent_done, 50.0);
        assert_eq!(raw(pb.height), 15_000);
    }

    #[test]
    fn the_builder_forms_are_exactly_their_setters() {
        let a = ProgressBar::create(7.5)
            .with_bar_background(solid(3))
            .with_container_background(solid(2))
            .with_height(PixelValue::px(-4.5));

        let mut b = ProgressBar::create(7.5);
        b.set_bar_background(solid(3));
        b.set_container_background(solid(2));
        b.set_height(PixelValue::px(-4.5));

        assert_eq!(a.bar_background.as_ref(), b.bar_background.as_ref());
        assert_eq!(
            a.container_background.as_ref(),
            b.container_background.as_ref()
        );
        assert_eq!(a.height, b.height);
        assert_eq!(
            a.progressbar_state.percent_done,
            b.progressbar_state.percent_done,
        );
        assert_eq!(
            a.progressbar_state.display_percentage,
            b.progressbar_state.display_percentage,
        );
    }

    #[test]
    fn an_empty_background_stays_an_empty_declaration() {
        let pb = ProgressBar::create(0.0)
            .with_bar_background(StyleBackgroundContentVec::from_vec(Vec::new()))
            .with_container_background(StyleBackgroundContentVec::new());

        assert!(pb.bar_background.is_empty());
        assert_eq!(pb.bar_background.len(), 0);
        assert!(pb.container_background.is_empty());

        let dom = pb.render_bar();
        assert_eq!(
            background_of(bar(&dom)),
            Some(Vec::new()),
            "an empty background must reach the DOM as an empty layer list, not vanish",
        );
        assert_eq!(background_of(&dom), Some(Vec::new()));
    }

    #[test]
    fn a_background_with_spare_capacity_keeps_its_allocation_intact() {
        // The free path rebuilds a `Vec` from (ptr, len, cap); a `cap` that drifted
        // to `len` frees the wrong layout.
        let mut v = Vec::with_capacity(64);
        v.push(StyleBackgroundContent::Color(ColorU {
            r: 1,
            g: 2,
            b: 3,
            a: 4,
        }));
        let bg = StyleBackgroundContentVec::from_vec(v);
        assert_eq!(bg.len(), 1);
        assert!(
            bg.capacity() >= 64,
            "from_vec lost the spare capacity: {}",
            bg.capacity()
        );

        let pb = ProgressBar::create(0.0).with_container_background(bg);
        assert_eq!(pb.container_background.len(), 1);
        assert!(
            pb.container_background.capacity() >= 64,
            "the setter rewrote the buffer's capacity",
        );
    }

    #[test]
    fn a_very_large_background_is_neither_truncated_nor_copied() {
        let big = solid(10_000);
        let ptr = big.as_ptr();
        let pb = ProgressBar::create(50.0).with_bar_background(big);
        assert_eq!(pb.bar_background.len(), 10_000);
        assert_eq!(
            pb.bar_background.as_ptr(),
            ptr,
            "the setter deep-copied a 10k-layer background"
        );

        let dom = pb.render_bar();
        assert_eq!(
            background_of(bar(&dom)).map(|v| v.len()),
            Some(10_000),
            "the background was truncated on the way into the DOM",
        );
    }

    #[test]
    fn overwriting_a_background_releases_the_previous_one() {
        // 500 replacements of an owned buffer: a setter that forgot to drop the old
        // value leaks, and one that dropped it twice aborts.
        let n = 500_usize;
        let mut pb = ProgressBar::create(0.0);
        for i in 1..=n {
            pb.set_bar_background(solid(i % 16 + 1));
            pb.set_container_background(solid(i % 4 + 1));
        }
        // The surviving background is whatever the LAST iteration installed, so the
        // expected length is derived from `n` rather than hard-coded.
        assert_eq!(pb.bar_background.len(), n % 16 + 1);
        assert_eq!(pb.container_background.len(), n % 4 + 1);
    }

    #[test]
    fn cloning_deep_copies_owned_backgrounds_but_shares_static_ones() {
        let pb = ProgressBar::create(3.0).with_bar_background(solid(4));
        let copy = pb.clone();

        assert_ne!(
            copy.bar_background.as_ptr(),
            pb.bar_background.as_ptr(),
            "Clone shared an owned heap buffer — dropping both would double-free it",
        );
        assert_eq!(
            copy.container_background.as_ptr(),
            pb.container_background.as_ptr(),
            "the static gradient is never freed, so the clone should keep sharing it",
        );

        drop(pb);
        assert_eq!(copy.bar_background.len(), 4);
        assert_eq!(
            copy.bar_background.as_ref()[3],
            StyleBackgroundContent::Color(ColorU {
                r: 3,
                g: 1,
                b: 2,
                a: 255,
            }),
            "the clone read back garbage after the original was dropped",
        );
    }

    // ------------------------------------------------------------------
    // set_height / with_height
    // ------------------------------------------------------------------

    #[test]
    fn set_height_stores_every_pixel_value_verbatim() {
        for h in adversarial_heights() {
            let mut pb = ProgressBar::create(0.0);
            pb.set_height(h);
            assert_eq!(
                pb.height.metric, h.metric,
                "set_height changed the unit of {h:?}"
            );
            assert_eq!(raw(pb.height), raw(h), "set_height re-encoded {h:?}");
            // and nothing else moved
            assert_eq!(pb.progressbar_state.percent_done, 0.0);
            assert_eq!(pb.bar_background.len(), 1);
        }
    }

    #[test]
    fn an_out_of_range_height_saturates_instead_of_wrapping() {
        let mut pb = ProgressBar::create(0.0);

        // `FloatValue::new` computes `value * 1000.0` in `f32` (which overflows to
        // an infinity) and then casts to `isize` — a saturating cast, so the result
        // is a bound, never a wrapped negative.
        pb.set_height(PixelValue::px(f32::MAX));
        assert_eq!(
            raw(pb.height),
            isize::MAX,
            "an overflowing height wrapped instead of saturating"
        );
        assert!(
            pb.height.number.get().is_finite(),
            "the saturated height decoded to a non-finite f32"
        );

        pb.set_height(PixelValue::px(f32::INFINITY));
        assert_eq!(raw(pb.height), isize::MAX);

        pb.set_height(PixelValue::px(f32::MIN));
        assert_eq!(raw(pb.height), isize::MIN);

        pb.set_height(PixelValue::px(f32::NEG_INFINITY));
        assert_eq!(raw(pb.height), isize::MIN);

        pb.set_height(PixelValue::px(f32::NAN));
        assert_eq!(
            raw(pb.height),
            0,
            "a NaN height must land on 0, not on an arbitrary integer"
        );

        pb.set_height(PixelValue::px(-0.0));
        assert_eq!(raw(pb.height), 0, "-0.0 must encode to the same 0 as +0.0");

        // Below the 1/1000 resolution everything truncates to zero, deterministically.
        pb.set_height(PixelValue::px(0.0004));
        assert_eq!(raw(pb.height), 0);
        pb.set_height(PixelValue::px(f32::MIN_POSITIVE));
        assert_eq!(raw(pb.height), 0);
    }

    #[test]
    fn with_height_is_set_height_and_leaves_the_rest_alone() {
        for h in adversarial_heights() {
            let a = ProgressBar::create(42.0).with_height(h);
            let mut b = ProgressBar::create(42.0);
            b.set_height(h);

            assert_eq!(
                a.height, b.height,
                "with_height disagreed with set_height for {h:?}"
            );
            assert_eq!(raw(a.height), raw(h));
            assert_eq!(a.progressbar_state.percent_done, 42.0);
            assert_eq!(
                a.bar_background.as_ptr(),
                b.bar_background.as_ptr(),
                "with_height reallocated the background",
            );
        }
    }

    // ------------------------------------------------------------------
    // ProgressBar::dom
    // ------------------------------------------------------------------

    #[test]
    fn render_bar_is_a_container_div_with_exactly_two_leaf_children() {
        let dom = ProgressBar::create(50.0).render_bar();

        assert!(matches!(dom.root.get_node_type(), NodeType::Div));
        assert_eq!(
            kids(&dom).len(),
            2,
            "the progress bar must render bar + remaining"
        );
        assert!(kids(bar(&dom)).is_empty(), "the bar must stay a leaf");
        assert!(
            kids(remaining(&dom)).is_empty(),
            "the remaining space must stay a leaf"
        );

        // A cached child count that is too small makes `convert_dom_into_compact_dom`
        // under-allocate its arenas and panic on out-of-bounds writes.
        assert_eq!(dom.estimated_total_children, 2);

        assert_eq!(
            classes(&dom),
            vec!["__azul-native-progress-bar-container".to_string()]
        );
        assert_eq!(
            classes(bar(&dom)),
            vec!["__azul-native-progress-bar-bar".to_string()]
        );
        assert_eq!(
            classes(remaining(&dom)),
            vec!["__azul-native-progress-bar-remaining".to_string()],
        );
    }

    #[test]
    fn render_bar_clamps_every_out_of_range_percentage_into_zero_to_one_hundred() {
        // (input, bar width, remaining width) — widths in 1/1000 of a percent.
        const CASES: [(f32, isize, isize); 12] = [
            (0.0, 0, 100_000),
            (-0.0, 0, 100_000),
            (50.0, 50_000, 50_000),
            (100.0, 100_000, 0),
            (-1.0, 0, 100_000),
            (101.0, 100_000, 0),
            (-1e30, 0, 100_000),
            (1e30, 100_000, 0),
            (f32::MAX, 100_000, 0),
            (f32::MIN, 0, 100_000),
            (f32::INFINITY, 100_000, 0),
            (f32::NEG_INFINITY, 0, 100_000),
        ];

        for (input, bar_width, remaining_width) in CASES {
            let dom = ProgressBar::create(input).render_bar();
            let b = width_of(bar(&dom)).expect("the bar must declare a width");
            let r = width_of(remaining(&dom)).expect("the remaining space must declare a width");

            assert_eq!(
                b.metric,
                SizeMetric::Percent,
                "the bar must size in %, not {:?}",
                b.metric
            );
            assert_eq!(
                r.metric,
                SizeMetric::Percent,
                "the gap must size in %, not {:?}",
                r.metric
            );
            assert_eq!(raw(b), bar_width, "bar width for input {input}");
            assert_eq!(raw(r), remaining_width, "remaining width for input {input}");
            assert!(
                raw(b) >= 0 && raw(r) >= 0,
                "a negative width escaped for input {input}"
            );
        }
    }

    #[test]
    fn render_bar_collapses_a_nan_percentage_to_two_empty_children() {
        // `f32::clamp` propagates NaN rather than clamping it, and the `f32 -> isize`
        // cast inside `FloatValue::new` then turns it into 0. The documented result:
        // BOTH children get 0% — the bar renders as an empty container instead of
        // falling back to 0%/100%. It does not panic, and it is deterministic.
        let dom = ProgressBar::create(f32::NAN).render_bar();
        let b = width_of(bar(&dom)).expect("the bar must declare a width");
        let r = width_of(remaining(&dom)).expect("the remaining space must declare a width");

        assert_eq!(raw(b), 0);
        assert_eq!(raw(r), 0);
        assert_eq!(b.metric, SizeMetric::Percent);
        assert_eq!(r.metric, SizeMetric::Percent);
        assert_eq!(
            kids(&dom).len(),
            2,
            "a NaN percentage must not change the tree shape"
        );
    }

    #[test]
    fn render_bar_splits_the_container_exactly_for_whole_percentages() {
        for i in 0..=100_isize {
            let dom = ProgressBar::create(i as f32).render_bar();
            let b = raw(width_of(bar(&dom)).unwrap());
            let r = raw(width_of(remaining(&dom)).unwrap());

            assert_eq!(b, i * 1000, "the bar is not {i}% wide");
            assert_eq!(
                b + r,
                100_000,
                "the two halves do not add up to the container at {i}%",
            );
        }
    }

    #[test]
    fn render_bar_loses_at_most_the_encoding_truncation_for_fractional_percentages() {
        // Each side is truncated to 1/1000 of a percent independently, so the pair
        // may under-fill by two ticks — but never overflow the container, and never
        // go negative.
        for p in [
            0.0005_f32,
            0.5,
            1.0 / 3.0,
            33.333,
            66.667,
            99.999,
            99.9999,
            f32::EPSILON,
            f32::MIN_POSITIVE,
        ] {
            let dom = ProgressBar::create(p).render_bar();
            let b = raw(width_of(bar(&dom)).unwrap());
            let r = raw(width_of(remaining(&dom)).unwrap());

            assert!(
                (0..=100_000).contains(&b) && (0..=100_000).contains(&r),
                "a width left 0%..100% for {p}: {b} / {r}",
            );
            assert!(
                (b + r - 100_000).abs() <= 10,
                "the two halves drifted apart for {p}: {b} + {r}",
            );
        }
    }

    #[test]
    fn render_bar_routes_each_background_to_its_own_node() {
        let bar_bg = solid(3);
        let container_bg = solid(5);
        let bar_ptr = bar_bg.as_ptr();
        let container_ptr = container_bg.as_ptr();

        let dom = ProgressBar::create(25.0)
            .with_bar_background(bar_bg)
            .with_container_background(container_bg)
            .render_bar();

        assert_eq!(
            background_of(&dom).map(|v| v.len()),
            Some(5),
            "the container lost (or swapped) its background",
        );
        assert_eq!(
            background_of(bar(&dom)).map(|v| v.len()),
            Some(3),
            "the bar lost (or swapped) its background",
        );
        assert_eq!(
            background_of(remaining(&dom)),
            None,
            "the remaining space must not paint anything",
        );

        // The bar background is *moved* into the DOM — same allocation, no copy.
        assert_eq!(
            background_ptr(bar(&dom)),
            Some(bar_ptr),
            "the bar background was copied instead of moved",
        );
        // The container background is cloned, because `self` — and with it the
        // original buffer — is dropped at the end of `render_bar()`. Handing the DOM the
        // same pointer would be a use-after-free.
        assert_ne!(
            background_ptr(&dom),
            Some(container_ptr),
            "the DOM kept a pointer into a buffer that `dom()` then freed",
        );
    }

    #[test]
    fn render_bar_forwards_any_height_to_the_container_and_to_nobody_else() {
        for h in adversarial_heights() {
            let dom = ProgressBar::create(50.0).with_height(h).render_bar();
            let got = height_of(&dom).expect("the container must declare a height");

            assert_eq!(
                got.metric, h.metric,
                "the height unit changed on the way into the DOM"
            );
            assert_eq!(
                raw(got),
                raw(h),
                "the height was re-encoded on the way into the DOM"
            );
            assert_eq!(
                height_of(bar(&dom)),
                None,
                "the bar must not declare its own height"
            );
            assert_eq!(
                height_of(remaining(&dom)),
                None,
                "the remaining space must not declare its own height",
            );
            assert_eq!(
                width_of(&dom),
                None,
                "the container must not declare a width"
            );
        }
    }

    #[test]
    fn render_bar_declares_the_expected_style_blocks_and_no_property_twice() {
        let dom = ProgressBar::create(50.0)
            .with_bar_background(solid(1))
            .render_bar();

        assert_eq!(
            inline_props(&dom).len(),
            24,
            "the container style block drifted"
        );
        assert_eq!(
            inline_props(bar(&dom)).len(),
            10,
            "the bar style block drifted"
        );

        let props = inline_props(remaining(&dom));
        assert_eq!(
            props.len(),
            1,
            "the remaining space grew a style block: {props:?}"
        );
        assert!(
            matches!(&props[0], CssProperty::Width(_)),
            "the remaining space must only declare its width",
        );

        // A property declared twice means one of the two is silently dead, and which
        // one wins depends on cascade order.
        for node in [&dom, bar(&dom), remaining(&dom)] {
            let mut seen = HashSet::new();
            for p in inline_props(node) {
                assert!(
                    seen.insert(core::mem::discriminant(&p)),
                    "duplicate declaration of {p:?}",
                );
            }
        }
    }

    #[test]
    fn render_bar_chrome_lengths_are_all_absolute_pixels() {
        // Only the two child widths are relative. A border or radius that slipped
        // into `em`/`%` would resolve against the parent font or box and either
        // vanish or blow up.
        let dom = ProgressBar::create(50.0).render_bar();
        for node in [&dom, bar(&dom), remaining(&dom)] {
            for p in inline_props(node) {
                for length in lengths_of(&p) {
                    assert_eq!(
                        length.metric,
                        SizeMetric::Px,
                        "{p:?} declares a relative length: {length:?}",
                    );
                }
            }
        }
    }

    #[test]
    fn render_bar_ignores_display_percentage() {
        // The field is public and settable, but nothing in `render_bar()` reads it: the
        // rendered tree has to be byte-identical either way.
        let mut with_label = ProgressBar::create(40.0);
        with_label.progressbar_state.display_percentage = true;
        let without_label = ProgressBar::create(40.0);

        assert_eq!(
            with_label.render_bar(),
            without_label.render_bar(),
            "display_percentage started changing the tree",
        );
    }

    #[test]
    fn render_bar_is_deterministic_for_equal_inputs() {
        let a = ProgressBar::create(37.5)
            .with_bar_background(solid(2))
            .with_height(PixelValue::px(7.25))
            .render_bar();
        let b = ProgressBar::create(37.5)
            .with_bar_background(solid(2))
            .with_height(PixelValue::px(7.25))
            .render_bar();

        assert_eq!(
            a, b,
            "two identically-built progress bars rendered differently"
        );
    }

    /// The bar's percentage is its accessibility VALUE, on the container, as a
    /// `ProgressBar` role — so a screen reader announces "progress bar, 75%"
    /// instead of two anonymous divs. Out-of-range and NaN inputs announce
    /// what the bar draws (clamped / empty), never "NaN%".
    #[test]
    fn render_bar_publishes_its_percentage_as_the_accessibility_value() {
        use azul_core::a11y::AccessibilityRole;

        for (input, expected) in [
            (75.0_f32, "75%"),
            (0.0, "0%"),
            (100.0, "100%"),
            (33.3, "33%"),
            (-5.0, "0%"),
            (250.0, "100%"),
            (f32::NAN, "0%"),
            (f32::INFINITY, "100%"),
        ] {
            let dom = ProgressBar::create(input).render_bar();
            let info = dom
                .root
                .get_accessibility_info()
                .unwrap_or_else(|| panic!("{input}: the container carries accessibility info"));
            assert_eq!(info.role, AccessibilityRole::ProgressBar, "{input}");
            assert_eq!(
                info.accessibility_value.as_ref().map(AzString::as_str),
                Some(expected),
                "{input}: the percentage is the accessibility value"
            );
        }
    }

    #[test]
    fn render_bar_survives_every_extreme_percentage_and_background_size() {
        for p in ADVERSARIAL_PERCENTS.into_iter().chain([f32::NAN]) {
            for layers in [0_usize, 1, 64] {
                let dom = ProgressBar::create(p)
                    .with_bar_background(solid(layers))
                    .with_container_background(solid(layers))
                    .with_height(PixelValue::px(f32::MAX))
                    .render_bar();

                assert_eq!(
                    kids(&dom).len(),
                    2,
                    "shape changed for {p} / {layers} layers"
                );
                assert_eq!(dom.estimated_total_children, 2);
                assert_eq!(background_of(bar(&dom)).map(|v| v.len()), Some(layers));
                assert_eq!(background_of(&dom).map(|v| v.len()), Some(layers));

                let b = width_of(bar(&dom)).expect("the bar must declare a width");
                assert_eq!(b.metric, SizeMetric::Percent);
                assert!(
                    (0..=100_000).contains(&raw(b)),
                    "width out of range for {p}"
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // ProgressBar::dom — the VirtualView wrapper (the fast path's mount)
    // ------------------------------------------------------------------

    /// Runs `f` against a `VirtualViewCallbackInfo` reporting `w x h` bounds
    /// (the same harness shape the map widget's VV tests use).
    fn with_virtual_view_info<R>(
        w: f32,
        h: f32,
        f: impl FnOnce(VirtualViewCallbackInfo) -> R,
    ) -> R {
        use azul_core::callbacks::{HidpiAdjustedBounds, VirtualViewCallbackReason};
        use azul_core::geom::LogicalSize;
        use azul_core::resources::{DpiScaleFactor, ImageCache};
        use azul_core::window::WindowTheme;
        use rust_fontconfig::FcFontCache;

        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let size = LogicalSize::new(w, h);
        let info = VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::InitialRender,
            &fonts,
            &images,
            WindowTheme::LightMode,
            HidpiAdjustedBounds {
                logical_size: size,
                hidpi_factor: DpiScaleFactor::new(1.0),
            },
            LogicalRect::new(LogicalPosition::zero(), size),
            LogicalRect::new(LogicalPosition::zero(), size),
            LogicalPosition::zero(),
        );
        f(info)
    }

    #[test]
    fn dom_mounts_a_virtual_view_leaf_carrying_the_private_dataset() {
        let dom = ProgressBar::create(30.0).dom();

        assert!(
            dom.root.is_virtual_view_node(),
            "dom() must mount the bar as a VirtualView, or update_progress has \
             nothing to re-render"
        );
        assert!(
            kids(&dom).is_empty(),
            "the wrapper is a LEAF - the bar's tree comes from the VV callback, \
             never from the outer DOM"
        );

        let mut ds = dom
            .root
            .get_dataset()
            .cloned()
            .expect("the wrapper must carry the dataset update_progress looks up");
        let state = ds
            .downcast_ref::<ProgressBarLocalDataset>()
            .expect("the dataset is the widget's private type");
        assert_eq!(state.bar.progressbar_state.percent_done, 30.0);
    }

    #[test]
    fn dom_wrapper_declares_its_height_and_hides_overflow_and_nothing_else() {
        // The VV node's box is what the callback renders INTO, so it must be
        // definitely sized by the wrapper itself - and only sized: all visual
        // chrome lives in the rendered tree where a re-render can change it.
        let dom = ProgressBar::create(50.0)
            .with_height(PixelValue::const_px(22))
            .dom();

        let h = height_of(&dom).expect("the wrapper must declare the height");
        assert_eq!(h.metric, SizeMetric::Px);
        assert_eq!(raw(h), 22_000);
        // Width 100%: a VirtualView is replaced-element-like and would take
        // the 300px intrinsic default instead of stretching to its parent.
        let w = width_of(&dom).expect("the wrapper must declare width: 100%");
        assert_eq!(w.metric, SizeMetric::Percent);
        assert_eq!(raw(w), 100_000);
        assert_eq!(
            inline_props(&dom).len(),
            4,
            "wrapper style must stay height + width + overflow-x/y: {:?}",
            inline_props(&dom)
        );
    }

    #[test]
    fn virtual_view_callback_renders_the_stored_bar_into_the_bounds() {
        let dom = ProgressBar::create(40.0).dom();
        let vv = dom
            .root
            .get_virtual_view_node_ref()
            .expect("a VirtualView node stores its callback + refany");
        let payload = vv.refany.clone();

        let ret =
            with_virtual_view_info(200.0, 15.0, |info| progressbar_render_virtual_view(payload, info));

        let rendered = match &ret.dom {
            azul_core::dom::OptionDom::Some(d) => d,
            azul_core::dom::OptionDom::None => panic!("the callback must render the bar"),
        };
        assert_eq!(kids(rendered).len(), 2, "bar + remaining");
        // Inside the VV the callback renders PIXEL widths from its known
        // bounds (percent children do not resolve against VV bounds):
        // 200px - 2px borders = 198px content, 40% of that = 79.2px.
        let b = width_of(bar(rendered)).unwrap();
        assert_eq!(b.metric, SizeMetric::Px);
        assert_eq!(raw(b), 79_200);
        // The container is sized to bounds MINUS its 1px border ring, so the
        // borders land INSIDE the materialized area instead of overflowing
        // it and being clipped ("oddly cut off", user report 2026-08-29).
        assert_eq!(raw(width_of(rendered).unwrap()), 198_000);
        assert_eq!(raw(height_of(rendered).unwrap()), 13_000);

        // Not scrollable content: one rect, three names.
        let want = LogicalRect::new(
            LogicalPosition::zero(),
            azul_core::geom::LogicalSize::new(200.0, 15.0),
        );
        assert_eq!(ret.materialized, want, "materialized must be the bounds");
        assert_eq!(ret.virtual_rect, want, "virtual_rect must be the bounds");
    }

    #[test]
    fn virtual_view_callback_and_dataset_share_one_allocation() {
        // THE fast-path contract: `update_progress` writes through the NODE
        // dataset, the re-render reads through the VV refany - they must be
        // clones of one allocation or the bar re-renders its stale self.
        let dom = ProgressBar::create(10.0).dom();

        let mut ds = dom.root.get_dataset().cloned().expect("node dataset");
        ds.downcast_mut::<ProgressBarLocalDataset>()
            .expect("private type")
            .bar
            .progressbar_state
            .percent_done = 80.0;

        let payload = dom
            .root
            .get_virtual_view_node_ref()
            .expect("vv node")
            .refany
            .clone();
        let ret =
            with_virtual_view_info(100.0, 15.0, |info| progressbar_render_virtual_view(payload, info));

        let rendered = match &ret.dom {
            azul_core::dom::OptionDom::Some(d) => d,
            azul_core::dom::OptionDom::None => panic!("the callback must render the bar"),
        };
        // 100px bounds - 2px borders = 98px content; 80% of that = 78.4px.
        assert_eq!(
            raw(width_of(bar(rendered)).unwrap()),
            78_400,
            "a write through the node dataset did not reach the VV re-render",
        );
    }

    #[test]
    fn virtual_view_callback_rejects_a_foreign_payload() {
        let ret = with_virtual_view_info(100.0, 15.0, |info| {
            progressbar_render_virtual_view(RefAny::new(0_u8), info)
        });
        assert!(
            matches!(ret.dom, azul_core::dom::OptionDom::None),
            "a foreign payload must render nothing, not panic or invent a bar",
        );
        assert_eq!(ret.materialized, LogicalRect::zero());
        assert_eq!(ret.virtual_rect, LogicalRect::zero());
    }
}
