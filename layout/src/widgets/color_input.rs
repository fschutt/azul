//! Rectangular input that displays a color and invokes a callback when clicked

use azul_core::{
    callbacks::Update,
    dom::Dom,
    refany::RefAny,
};
use azul_css::dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec};
#[allow(clippy::wildcard_imports)] // widget/render module pulls in the css property/value types it builds with
use azul_css::{OptionString, 
    props::{
        basic::*,
        layout::*,
        property::{CssProperty, *},
        style::*,
    },
    *,
};

use crate::callbacks::{Callback, CallbackInfo};

/// Rectangular input that displays a color and triggers a callback when clicked.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct ColorInput {
    pub color_input_state: ColorInputStateWrapper,
    pub style: CssPropertyWithConditionsVec,
    /// What this control is CALLED, for assistive technology.
    ///
    /// Carried by the WIDGET so it knows at build time whether it was named;
    /// forwarded into the accessibility declaration it already builds.
    pub accessibility_name: OptionString,
}

/// Callback function type invoked when the color input value changes.
pub type ColorInputOnValueChangeCallbackType =
    extern "C" fn(RefAny, CallbackInfo, ColorInputState) -> Update;
impl_widget_callback!(
    ColorInputOnValueChange,
    OptionColorInputOnValueChange,
    ColorInputOnValueChangeCallback,
    ColorInputOnValueChangeCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        ColorInputOnValueChangeCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: COLOR_INPUT_ON_VALUE_CHANGE_INVOKER,
    invoker_ty:     AzColorInputOnValueChangeCallbackInvoker,
    thunk_fn:       az_color_input_on_value_change_callback_thunk,
    setter_fn:      AzApp_setColorInputOnValueChangeCallbackInvoker,
    from_handle_fn: AzColorInputOnValueChangeCallback_createFromHostHandle,
    extra_args:     [ state: ColorInputState ],
}

/// Wrapper around [`ColorInputState`] that includes a title and an optional value-change callback.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct ColorInputStateWrapper {
    pub inner: ColorInputState,
    pub title: AzString,
    pub on_value_change: OptionColorInputOnValueChange,
}

impl Default for ColorInputStateWrapper {
    fn default() -> Self {
        Self {
            inner: ColorInputState::default(),
            title: AzString::from_const_str("Pick color"),
            on_value_change: None.into(),
        }
    }
}

/// Holds the current color value of a [`ColorInput`] widget.
#[derive(Copy, Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct ColorInputState {
    pub color: ColorU,
}

impl Default for ColorInputState {
    fn default() -> Self {
        Self {
            color: ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255,
            },
        }
    }
}

static DEFAULT_COLOR_INPUT_STYLE: &[CssPropertyWithConditions] = &[
    CssPropertyWithConditions::simple(CssProperty::const_display(LayoutDisplay::Block)),
    CssPropertyWithConditions::simple(CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
    CssPropertyWithConditions::simple(CssProperty::const_width(LayoutWidth::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_height(LayoutHeight::const_px(14))),
    CssPropertyWithConditions::simple(CssProperty::const_cursor(StyleCursor::Pointer)),
];

impl ColorInput {
    /// Name this control for assistive technology.
    #[must_use]
    pub fn with_accessibility_name<S: Into<AzString>>(mut self, name: S) -> Self {
        self.accessibility_name = Some(name.into()).into();
        self
    }

    /// Creates a new `ColorInput` displaying the given color.
    #[inline]
    #[must_use]
    pub fn create(color: ColorU) -> Self {
        Self {
            color_input_state: ColorInputStateWrapper {
                inner: ColorInputState { color },
                ..Default::default()
            },
            style: CssPropertyWithConditionsVec::from_const_slice(DEFAULT_COLOR_INPUT_STYLE),
            accessibility_name: OptionString::None,
        }
    }

    /// Sets the callback invoked when the color value changes.
    #[inline]
    pub fn set_on_value_change<I: Into<ColorInputOnValueChangeCallback>>(
        &mut self,
        data: RefAny,
        callback: I,
    ) {
        self.color_input_state.on_value_change = Some(ColorInputOnValueChange {
            callback: callback.into(),
            refany: data,
        })
        .into();
    }

    /// Builder-style method to set the value-change callback.
    #[inline]
    #[must_use]
    pub fn with_on_value_change<C: Into<ColorInputOnValueChangeCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_value_change(data, callback);
        self
    }

    /// Replaces `self` with a default `ColorInput` and returns the previous value.
    #[inline]
    #[must_use]
    pub fn swap_with_default(&mut self) -> Self {
        let mut s = Self::default();
        core::mem::swap(&mut s, self);
        s
    }

    /// Converts this `ColorInput` into a styled [`Dom`] node with a click callback.
    #[inline]
    #[must_use]
    pub fn dom(self) -> Dom {
        use azul_core::{
            callbacks::{CoreCallback, CoreCallbackData},
            dom::{EventFilter, HoverEventFilter, IdOrClass::Class},
        };

        let mut style = self.style.into_library_owned_vec();
        style.push(CssPropertyWithConditions::simple(
            CssProperty::const_background_content(
                vec![StyleBackgroundContent::Color(
                    self.color_input_state.inner.color,
                )]
                .into(),
            ),
        ));

        Dom::create_div()
            .with_ids_and_classes(vec![Class("__azul_native_color_input".into())].into())
            .with_css_props(style.into())
            .with_callbacks(
                vec![CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::MouseUp),
                    refany: RefAny::new(self.color_input_state),
                    callback: CoreCallback {
                        cb: on_color_input_clicked as usize,
                        ctx: azul_core::refany::OptionRefAny::None,
                    },
                }]
                .into(),
            )
    }
}

extern "C" fn on_color_input_clicked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(mut color_input) = data.downcast_mut::<ColorInputStateWrapper>() else {
        return Update::DoNothing;
    };

    // No built-in color picker dialog — the on_value_change callback
    // receives the current color so the caller can open their own picker.
    let color_input = &mut *color_input;
    let onvaluechange = &mut color_input.on_value_change;
    let inner = color_input.inner;

    match onvaluechange.as_mut() {
        Some(ColorInputOnValueChange {
            callback,
            refany: data,
        }) => (callback.cb)(data.clone(), info, inner),
        None => Update::DoNothing,
    }
}

#[cfg(all(test, feature = "std"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::{
        collections::{hash_map::DefaultHasher, BTreeMap, HashMap},
        hash::{Hash, Hasher},
        mem::discriminant,
        sync::{Arc, Mutex},
    };

    use azul_core::{
        dom::{DomId, DomNodeId, EventFilter, HoverEventFilter, IdOrClass, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::props::basic::{length::SizeMetric, pixel::PixelValue};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The swatch is a fixed 14x14 box — the entire geometry of the widget.
    const SIDE: f32 = 14.0;

    /// The widget's default title, as promised by `ColorInputStateWrapper::default`.
    const DEFAULT_TITLE: &str = "Pick color";

    /// The color a freshly-defaulted `ColorInputState` holds: **opaque white**, which is
    /// deliberately *not* `ColorU::default()` (that one is opaque black). A swatch that
    /// silently defaulted to black would be indistinguishable from a "real" black pick.
    const DEFAULT_COLOR: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    };

    /// Adversarial `ColorU` inputs. `create`/`dom` must move all four channels through
    /// verbatim, so the set covers both alpha extremes, the two off-by-one alphas, and
    /// `{1,2,3,4}` — four distinct small values that catch any channel reordering (an
    /// r/b swap is invisible for greys and for anything symmetric).
    const SAMPLE_COLORS: [ColorU; 8] = [
        ColorU { r: 0, g: 0, b: 0, a: 0 },
        ColorU { r: 0, g: 0, b: 0, a: 255 },
        ColorU { r: 255, g: 255, b: 255, a: 255 },
        ColorU { r: 255, g: 255, b: 255, a: 0 },
        ColorU { r: 255, g: 0, b: 0, a: 1 },
        ColorU { r: 0, g: 255, b: 0, a: 254 },
        ColorU { r: 1, g: 2, b: 3, a: 4 },
        ColorU { r: 128, g: 64, b: 32, a: 16 },
    ];

    // ------------------------------------------------------------------
    // Style-vec / DOM probes
    // ------------------------------------------------------------------

    fn properties(v: &CssPropertyWithConditionsVec) -> Vec<CssProperty> {
        v.as_ref().iter().map(|p| p.property.clone()).collect()
    }

    fn find<T>(v: &CssPropertyWithConditionsVec, f: impl Fn(&CssProperty) -> Option<T>) -> Option<T> {
        v.as_ref().iter().find_map(|p| f(&p.property))
    }

    /// The `f32` of a `PixelValue`, asserting it is an absolute `px` length. An `em` or
    /// `%` slipping into the swatch geometry would resolve against the parent font/box,
    /// so the "14px" swatch could render at any size at all.
    fn px(pv: &PixelValue) -> f32 {
        assert_eq!(
            pv.metric,
            SizeMetric::Px,
            "color-input geometry must be absolute px, got {:?}",
            pv.metric,
        );
        pv.number.get()
    }

    fn width_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Width(w) => match w.get_property() {
                Some(LayoutWidth::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    fn height_px(v: &CssPropertyWithConditionsVec) -> Option<f32> {
        find(v, |p| match p {
            CssProperty::Height(h) => match h.get_property() {
                Some(LayoutHeight::Px(pv)) => Some(px(pv)),
                _ => None,
            },
            _ => None,
        })
    }

    /// The `background-color` of a style vec (first background layer only).
    fn background_color(v: &CssPropertyWithConditionsVec) -> Option<ColorU> {
        v.as_ref().iter().find_map(|p| match &p.property {
            CssProperty::BackgroundContent(b) => match b.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    fn classes(dom: &Dom) -> Vec<String> {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                IdOrClass::Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The properties of a rendered node's *inline* style, in declaration order.
    fn inline_properties(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The `background-color` actually declared on the rendered node.
    fn dom_background(dom: &Dom) -> Option<ColorU> {
        inline_properties(dom).into_iter().find_map(|p| match p {
            CssProperty::BackgroundContent(b) => match b.get_property()?.as_ref().first()? {
                StyleBackgroundContent::Color(c) => Some(*c),
                _ => None,
            },
            _ => None,
        })
    }

    /// The exact property `dom()` is expected to append for `c`.
    fn expected_background(c: ColorU) -> CssProperty {
        CssProperty::const_background_content(StyleBackgroundContentVec::from_vec(vec![
            StyleBackgroundContent::Color(c),
        ]))
    }

    fn hash_of<T: Hash>(t: &T) -> u64 {
        let mut h = DefaultHasher::new();
        t.hash(&mut h);
        h.finish()
    }

    // ------------------------------------------------------------------
    // Callback harness
    // ------------------------------------------------------------------

    /// A `DomNodeId` in the root DOM pointing at flattened node `idx`.
    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// A `DomNodeId` whose node component is `None` — the "no concrete node was hit" case.
    fn node_none() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// A `DomLayoutResult` carrying only a `styled_dom`. `on_color_input_clicked` never
    /// queries the layout at all, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Runs `f` with a `CallbackInfo` whose window holds `styled_dom` as the root DOM and
    /// whose hit node is `hit`. Returns `f`'s value plus every change the callback pushed
    /// onto the transaction log.
    fn with_info<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window
            .layout_results
            .insert(DomId::ROOT_ID, layout_result(styled_dom));

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
            system_style: Arc::new(system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let r = f(&mut info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// Renders `color_input`, then hands back both the laid-out DOM *and* the very `RefAny`
    /// the widget registered on its own mouse-up callback. Driving the handler with these
    /// two is the real wiring — nothing is re-created by hand, so a mismatch between what
    /// `dom()` stores and what the handler expects cannot hide behind the fixture.
    fn laid_out(color_input: ColorInput) -> (StyledDom, RefAny) {
        let dom = color_input.dom();
        let state = dom.root.callbacks.as_ref()[0].refany.clone();
        (StyledDom::create_from_dom(dom), state)
    }

    /// One "mouse-up on `hit`" delivered to the widget's own registered handler.
    fn click(styled_dom: StyledDom, state: &RefAny, hit: DomNodeId) -> (Update, Vec<CallbackChange>) {
        with_info(styled_dom, hit, |info| {
            on_color_input_clicked(state.clone(), *info)
        })
    }

    fn state_color(state: &RefAny) -> ColorU {
        let mut state = state.clone();
        let wrapper = state
            .downcast_ref::<ColorInputStateWrapper>()
            .expect("the widget state changed type");
        wrapper.inner.color
    }

    /// A payload the value-change callback writes into. It arrives as the `data: RefAny`
    /// argument — a *shared* clone of what the test still holds — so the test can read back
    /// exactly what the widget passed, without any global state.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct ColorLog {
        seen: Vec<ColorU>,
        payload: u32,
    }

    extern "C" fn record_value(
        mut data: RefAny,
        _info: CallbackInfo,
        state: ColorInputState,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<ColorLog>() {
            log.seen.push(state.color);
        }
        Update::RefreshDom
    }

    extern "C" fn value_do_nothing(
        _data: RefAny,
        _info: CallbackInfo,
        _state: ColorInputState,
    ) -> Update {
        Update::DoNothing
    }

    extern "C" fn value_refresh_all(
        _data: RefAny,
        _info: CallbackInfo,
        _state: ColorInputState,
    ) -> Update {
        Update::RefreshDomAllWindows
    }

    /// A `Callback`-shaped (2-arg) function — the shape FFI bindings hand in, which the
    /// `From<Callback>` arm *transmutes* into the 3-arg color-input slot. Never called.
    extern "C" fn generic_shaped(_data: RefAny, _info: CallbackInfo) -> Update {
        Update::DoNothing
    }

    fn log_refany() -> RefAny {
        RefAny::new(ColorLog {
            seen: Vec::new(),
            payload: 0xDEAD_BEEF,
        })
    }

    fn read_log(probe: &RefAny) -> ColorLog {
        let mut probe = probe.clone();
        let log = probe
            .downcast_ref::<ColorLog>()
            .expect("the user payload changed type");
        log.clone()
    }

    // ==================================================================
    // ColorInput::create
    // ==================================================================

    #[test]
    fn create_stores_every_channel_verbatim() {
        // A channel swap (r/b) or a dropped alpha still type-checks and still renders
        // *a* color — only an asymmetric fixture catches it.
        for c in SAMPLE_COLORS {
            let w = ColorInput::create(c);
            assert_eq!(
                w.color_input_state.inner.color, c,
                "create({c:?}) did not store the color it was given",
            );
        }
    }

    #[test]
    fn create_installs_no_callback_and_the_default_title() {
        for c in SAMPLE_COLORS {
            let w = ColorInput::create(c);
            assert!(
                w.color_input_state.on_value_change.as_ref().is_none(),
                "create({c:?}) invented a value-change callback out of nowhere",
            );
            assert_eq!(
                w.color_input_state.title.as_str(),
                DEFAULT_TITLE,
                "create({c:?}) did not keep the default title",
            );
        }
    }

    #[test]
    fn create_is_pure_and_distinguishes_every_sample_color() {
        for c in SAMPLE_COLORS {
            assert_eq!(
                ColorInput::create(c),
                ColorInput::create(c),
                "create({c:?}) is not deterministic",
            );
        }
        for (i, a) in SAMPLE_COLORS.iter().enumerate() {
            for b in &SAMPLE_COLORS[i + 1..] {
                assert_ne!(
                    ColorInput::create(*a),
                    ColorInput::create(*b),
                    "the widgets for {a:?} and {b:?} are indistinguishable",
                );
            }
        }
    }

    #[test]
    fn create_treats_alpha_as_significant() {
        // `{255,0,0,0}` and `{255,0,0,255}` differ only in alpha: an invisible swatch and
        // an opaque red one. Comparing on rgb alone would fuse the two.
        let opaque = ColorU { r: 255, g: 0, b: 0, a: 255 };
        let clear = ColorU { r: 255, g: 0, b: 0, a: 0 };
        assert_ne!(
            ColorInput::create(opaque),
            ColorInput::create(clear),
            "a transparent swatch compares equal to an opaque one",
        );
    }

    #[test]
    fn create_geometry_is_absolute_14px_for_every_color() {
        // `px()` asserts SizeMetric::Px — an em/% here would scale with the parent.
        for c in SAMPLE_COLORS {
            let w = ColorInput::create(c);
            assert_eq!(width_px(&w.style), Some(SIDE), "{c:?}: wrong swatch width");
            assert_eq!(height_px(&w.style), Some(SIDE), "{c:?}: wrong swatch height");
        }
    }

    #[test]
    fn create_marks_the_swatch_as_clickable() {
        // Without `cursor: pointer` the swatch looks inert even though it is the node
        // that carries the mouse-up handler.
        let props = properties(&ColorInput::create(DEFAULT_COLOR).style);
        assert!(
            props.contains(&CssProperty::const_cursor(StyleCursor::Pointer)),
            "the color input does not present as clickable: {props:?}",
        );
    }

    #[test]
    fn create_is_a_non_growing_block() {
        // A swatch with flex-grow != 0 would stretch to fill its row and stop being a
        // 14px square, silently defeating the width/height declarations above.
        let props = properties(&ColorInput::create(DEFAULT_COLOR).style);
        assert!(
            props.contains(&CssProperty::const_flex_grow(LayoutFlexGrow::const_new(0))),
            "the swatch is allowed to flex-grow: {props:?}",
        );
        assert!(
            props.contains(&CssProperty::const_display(LayoutDisplay::Block)),
            "the swatch is not a block box: {props:?}",
        );
    }

    #[test]
    fn create_declares_no_property_twice() {
        // A duplicate declaration means the later one silently wins — a latent
        // "why is my override ignored" bug that never surfaces as an error.
        let props = properties(&ColorInput::create(DEFAULT_COLOR).style);
        let mut seen = Vec::new();
        for p in &props {
            let d = discriminant(p);
            assert!(!seen.contains(&d), "the base style declares {p:?} twice");
            seen.push(d);
        }
    }

    #[test]
    fn create_keeps_the_color_out_of_the_base_style() {
        // The color lives in the *state* and is only turned into a background by `dom()`.
        // A background baked into the shared const table would make every swatch on screen
        // render the same color (and `dom()` would then declare it twice).
        for c in SAMPLE_COLORS {
            assert_eq!(
                background_color(&ColorInput::create(c).style),
                None,
                "create({c:?}) leaked the color into the base style",
            );
        }
    }

    #[test]
    fn create_style_does_not_depend_on_the_color() {
        let reference = properties(&ColorInput::create(SAMPLE_COLORS[0]).style);
        for c in SAMPLE_COLORS {
            assert_eq!(
                properties(&ColorInput::create(c).style),
                reference,
                "create({c:?}) produced a different style than create({:?})",
                SAMPLE_COLORS[0],
            );
        }
    }

    // ==================================================================
    // Default state invariants
    // ==================================================================

    #[test]
    fn the_default_color_is_opaque_white_not_colorus_own_default() {
        // `ColorU::default()` is opaque *black*. If `ColorInputState` ever fell back to the
        // derived default, every un-set swatch would render black — and a user who really
        // picked black would be indistinguishable from one who picked nothing.
        assert_eq!(ColorInputState::default().color, DEFAULT_COLOR);
        assert_ne!(
            ColorInputState::default().color,
            ColorU::default(),
            "the color input's default silently became ColorU::default()",
        );
        assert_eq!(ColorInputStateWrapper::default().inner.color, DEFAULT_COLOR);
        assert_eq!(
            ColorInputStateWrapper::default().title.as_str(),
            DEFAULT_TITLE,
        );
        assert!(ColorInputStateWrapper::default()
            .on_value_change
            .as_ref()
            .is_none());
    }

    #[test]
    fn color_input_state_ord_and_partial_ord_agree() {
        // `ColorInputState` derives both. A hand-written impl drifting from the other would
        // make sorted containers of states behave inconsistently with `<`.
        for a in SAMPLE_COLORS {
            for b in SAMPLE_COLORS {
                let (x, y) = (ColorInputState { color: a }, ColorInputState { color: b });
                assert_eq!(
                    x.partial_cmp(&y),
                    Some(x.cmp(&y)),
                    "PartialOrd and Ord disagree for {a:?} vs {b:?}",
                );
                assert_eq!(
                    x == y,
                    x.cmp(&y) == core::cmp::Ordering::Equal,
                    "Eq and Ord disagree for {a:?} vs {b:?}",
                );
            }
        }
    }

    #[test]
    fn equal_color_input_states_hash_equal() {
        // The Hash/Eq contract: `a == b` must imply `hash(a) == hash(b)`, or a
        // `HashMap<ColorInputState, _>` loses entries.
        for c in SAMPLE_COLORS {
            let a = ColorInputState { color: c };
            let b = ColorInputState { color: c };
            assert_eq!(a, b);
            assert_eq!(hash_of(&a), hash_of(&b), "equal states hash differently ({c:?})");
        }
    }

    #[test]
    fn color_input_state_equality_is_channel_exact() {
        // One channel bumped by one must break equality — in all four channels.
        let base = ColorU { r: 10, g: 20, b: 30, a: 40 };
        let variants = [
            ColorU { r: 11, ..base },
            ColorU { g: 21, ..base },
            ColorU { b: 31, ..base },
            ColorU { a: 41, ..base },
        ];
        for v in variants {
            assert_ne!(
                ColorInputState { color: base },
                ColorInputState { color: v },
                "a one-channel difference ({base:?} vs {v:?}) was swallowed",
            );
        }
    }

    // ==================================================================
    // ColorInput::set_on_value_change / with_on_value_change
    // ==================================================================

    #[test]
    fn set_on_value_change_stores_the_function_pointer_and_the_payload_verbatim() {
        let mut w = ColorInput::create(DEFAULT_COLOR);
        w.set_on_value_change(
            RefAny::new(0xDEAD_BEEF_u32),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );

        let t = w
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("set_on_value_change did not store anything");
        assert_eq!(
            t.callback.cb as *const () as usize,
            value_do_nothing as ColorInputOnValueChangeCallbackType as *const () as usize,
            "the fn pointer was corrupted on the way in",
        );

        let mut data = t.refany.clone();
        assert_eq!(
            *data.downcast_ref::<u32>().expect("the payload changed type"),
            0xDEAD_BEEF,
            "the payload was corrupted",
        );
        assert!(
            data.downcast_ref::<u64>().is_none(),
            "downcasting to the wrong type must fail, not reinterpret the bytes",
        );
    }

    #[test]
    fn set_on_value_change_replaces_rather_than_accumulates() {
        // `OptionColorInputOnValueChange` is a single slot; setting twice must leave the
        // *second* callback installed (and must not leak or free the first one's RefAny).
        let first = log_refany();
        let mut w = ColorInput::create(DEFAULT_COLOR);
        w.set_on_value_change(
            first.clone(),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );
        w.set_on_value_change(
            RefAny::new(1u8),
            value_refresh_all as ColorInputOnValueChangeCallbackType,
        );

        let t = w
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("the callback vanished");
        assert_eq!(
            t.callback.cb as *const () as usize,
            value_refresh_all as ColorInputOnValueChangeCallbackType as *const () as usize,
            "the second set_on_value_change did not win",
        );
        // The displaced payload is still a valid, readable RefAny (not freed twice).
        assert_eq!(read_log(&first).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn set_on_value_change_does_not_disturb_the_color_or_the_style() {
        for c in SAMPLE_COLORS {
            let pristine = ColorInput::create(c);
            let mut w = ColorInput::create(c);
            w.set_on_value_change(
                RefAny::new(0u8),
                value_do_nothing as ColorInputOnValueChangeCallbackType,
            );

            assert_eq!(
                w.color_input_state.inner.color, c,
                "installing a callback rewrote the color",
            );
            assert_eq!(
                properties(&w.style),
                properties(&pristine.style),
                "installing a callback rewrote the style",
            );
            assert_eq!(
                w.color_input_state.title.as_str(),
                pristine.color_input_state.title.as_str(),
                "installing a callback rewrote the title",
            );
        }
    }

    #[test]
    fn with_on_value_change_is_exactly_set_on_value_change_in_builder_form() {
        let by_builder = ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(
            RefAny::new(7u32),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );

        let mut by_setter = ColorInput::create(SAMPLE_COLORS[6]);
        by_setter.set_on_value_change(
            RefAny::new(7u32),
            value_do_nothing as ColorInputOnValueChangeCallbackType,
        );

        assert_eq!(by_builder.color_input_state.inner, by_setter.color_input_state.inner);
        assert_eq!(properties(&by_builder.style), properties(&by_setter.style));

        let a = by_builder
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("builder lost the callback");
        let b = by_setter
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("setter lost the callback");
        assert_eq!(
            a.callback.cb as *const () as usize,
            b.callback.cb as *const () as usize,
        );

        let (mut a, mut b) = (a.refany.clone(), b.refany.clone());
        assert_eq!(
            *a.downcast_ref::<u32>().expect("builder payload changed type"),
            *b.downcast_ref::<u32>().expect("setter payload changed type"),
        );
    }

    #[test]
    fn with_on_value_change_accepts_a_generic_callback_without_mangling_the_pointer() {
        // The `From<Callback>` arm *transmutes* a 2-arg fn pointer into the 3-arg
        // color-input slot — this is the FFI (Python/C) path. The pointer must come out
        // bit-identical; a mangled one would be called as a wild jump on the first click.
        let generic = Callback {
            cb: generic_shaped,
            ctx: OptionRefAny::None,
        };
        let expected = generic_shaped as *const () as usize;

        let w = ColorInput::create(DEFAULT_COLOR).with_on_value_change(RefAny::new(0u8), generic);
        let t = w
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("the generic callback was dropped");
        assert_eq!(
            t.callback.cb as *const () as usize,
            expected,
            "the Callback -> ColorInputOnValueChangeCallback transmute mangled the pointer",
        );
    }

    // ==================================================================
    // ColorInput::swap_with_default
    // ==================================================================

    #[test]
    fn swap_with_default_returns_the_old_widget_and_leaves_a_default_behind() {
        for c in SAMPLE_COLORS {
            let mut w = ColorInput::create(c);
            let old = w.swap_with_default();

            assert_eq!(old, ColorInput::create(c), "{c:?}: the old widget was not returned intact");
            assert_eq!(w, ColorInput::default(), "{c:?}: what was left behind is not a default widget");
        }
    }

    #[test]
    fn swap_with_default_leaves_an_unstyled_widget_behind() {
        // `ColorInput::default()` is *derived*, so its `style` is an empty vec — unlike
        // `create()`, which installs the 14x14 + cursor table. The two therefore differ
        // even though their state is identical. Documented here so a change in either
        // direction is loud rather than silent.
        assert_eq!(
            ColorInput::default().color_input_state,
            ColorInput::create(DEFAULT_COLOR).color_input_state,
            "default() and create(white) no longer agree on the state",
        );
        assert!(
            ColorInput::default().style.as_ref().is_empty(),
            "ColorInput::default() gained a style",
        );
        assert_ne!(
            ColorInput::default(),
            ColorInput::create(DEFAULT_COLOR),
            "default() and create(white) became interchangeable",
        );

        let mut w = ColorInput::create(SAMPLE_COLORS[6]);
        let _ = w.swap_with_default();
        assert_eq!(width_px(&w.style), None, "the swapped-in widget unexpectedly has a width");
        assert_eq!(height_px(&w.style), None, "the swapped-in widget unexpectedly has a height");
    }

    #[test]
    fn swap_with_default_moves_the_callback_out_rather_than_copying_or_dropping_it() {
        let probe = log_refany();
        let mut w = ColorInput::create(SAMPLE_COLORS[4]).with_on_value_change(
            probe.clone(),
            record_value as ColorInputOnValueChangeCallbackType,
        );

        let old = w.swap_with_default();

        // The callback (and its payload) left with the returned value ...
        let moved = old
            .color_input_state
            .on_value_change
            .as_ref()
            .expect("the value-change callback vanished during the swap");
        assert_eq!(
            moved.callback.cb as *const () as usize,
            record_value as ColorInputOnValueChangeCallbackType as *const () as usize,
            "the fn pointer was mangled by the swap",
        );

        // ... and did NOT stay behind: a duplicated callback would fire twice, and a
        // duplicated RefAny would double-free its payload.
        assert!(
            w.color_input_state.on_value_change.as_ref().is_none(),
            "the callback was copied instead of moved",
        );

        // The payload is still alive and unchanged after the move.
        assert_eq!(read_log(&probe).payload, 0xDEAD_BEEF);
    }

    #[test]
    fn swapping_twice_round_trips_the_original_widget() {
        let mut a = ColorInput::create(SAMPLE_COLORS[6]);
        let mut b = a.swap_with_default(); // a = default, b = the original
        let c = b.swap_with_default(); // b = default, c = the original

        assert_eq!(c, ColorInput::create(SAMPLE_COLORS[6]));
        assert_eq!(a, ColorInput::default());
        assert_eq!(b, ColorInput::default());
    }

    // ==================================================================
    // ColorInput::dom
    // ==================================================================

    #[test]
    fn dom_is_a_single_childless_div_with_the_native_class() {
        for c in SAMPLE_COLORS {
            let dom = ColorInput::create(c).dom();
            assert!(
                matches!(dom.root.get_node_type(), NodeType::Div),
                "{c:?}: the color input is not a div",
            );
            assert_eq!(
                classes(&dom),
                vec!["__azul_native_color_input".to_string()],
                "{c:?}: wrong class list",
            );
            assert!(dom.children.as_ref().is_empty(), "{c:?}: the swatch grew children");
        }
    }

    #[test]
    fn dom_appends_the_color_as_the_last_background_and_keeps_the_base_style() {
        // The round trip: the color goes in through `create` and must come back out of the
        // rendered node's background, byte-identical, with the base style untouched and the
        // background appended *after* it (so a user override earlier in the table can't win).
        for c in SAMPLE_COLORS {
            let base = properties(&ColorInput::create(c).style);
            let rendered = inline_properties(&ColorInput::create(c).dom());

            assert_eq!(
                rendered.len(),
                base.len() + 1,
                "{c:?}: dom() added {} properties instead of exactly one",
                rendered.len() as i64 - base.len() as i64,
            );
            assert_eq!(&rendered[..base.len()], &base[..], "{c:?}: dom() rewrote the base style");
            assert_eq!(
                rendered[base.len()],
                expected_background(c),
                "{c:?}: the appended background is not this widget's color",
            );
        }
    }

    #[test]
    fn dom_round_trips_every_channel_of_every_sample_color() {
        for c in SAMPLE_COLORS {
            assert_eq!(
                dom_background(&ColorInput::create(c).dom()),
                Some(c),
                "create({c:?}).dom() does not paint {c:?}",
            );
        }
    }

    #[test]
    fn dom_declares_exactly_one_background_and_no_property_twice() {
        for c in SAMPLE_COLORS {
            let props = inline_properties(&ColorInput::create(c).dom());
            let backgrounds = props
                .iter()
                .filter(|p| matches!(p, CssProperty::BackgroundContent(_)))
                .count();
            assert_eq!(backgrounds, 1, "{c:?}: expected exactly one background declaration");

            let mut seen = Vec::new();
            for p in &props {
                let d = discriminant(p);
                assert!(!seen.contains(&d), "{c:?}: the rendered node declares {p:?} twice");
                seen.push(d);
            }
        }
    }

    #[test]
    fn dom_preserves_the_swatch_geometry() {
        // The geometry has to survive the const-slice -> owned-vec -> vec round trip that
        // `dom()` performs; losing it would leave a background-only, zero-sized node.
        for c in SAMPLE_COLORS {
            let rendered: CssPropertyWithConditionsVec = inline_properties(&ColorInput::create(c).dom())
                .into_iter()
                .map(CssPropertyWithConditions::simple)
                .collect();
            assert_eq!(width_px(&rendered), Some(SIDE), "{c:?}: the rendered swatch lost its width");
            assert_eq!(height_px(&rendered), Some(SIDE), "{c:?}: the rendered swatch lost its height");
        }
    }

    #[test]
    fn dom_registers_exactly_one_mouse_up_handler_and_it_is_the_widgets_own() {
        for c in SAMPLE_COLORS {
            let dom = ColorInput::create(c).dom();
            let callbacks = dom.root.callbacks.as_ref();

            assert_eq!(callbacks.len(), 1, "{c:?}: expected exactly one callback");
            assert_eq!(
                callbacks[0].event,
                EventFilter::Hover(HoverEventFilter::MouseUp),
                "{c:?}: the color input must fire on mouse-up",
            );
            assert_eq!(
                callbacks[0].callback.cb,
                on_color_input_clicked as usize,
                "{c:?}: the registered handler is not on_color_input_clicked",
            );
            assert_eq!(
                callbacks[0].callback.ctx,
                OptionRefAny::None,
                "{c:?}: a native handler must not carry an FFI context",
            );
        }
    }

    #[test]
    fn dom_hands_the_widget_state_to_the_handler_not_the_user_payload() {
        // `dom()` moves `color_input_state` (state + on_value_change + user RefAny) into the
        // callback's RefAny. If it stored the *user's* payload instead, the handler's
        // `downcast_mut::<ColorInputStateWrapper>()` would fail and every click would be a
        // silent no-op.
        for c in SAMPLE_COLORS {
            let dom = ColorInput::create(c)
                .with_on_value_change(
                    RefAny::new(9u32),
                    value_do_nothing as ColorInputOnValueChangeCallbackType,
                )
                .dom();

            let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
            let wrapper = state
                .downcast_ref::<ColorInputStateWrapper>()
                .expect("the handler's RefAny is not a ColorInputStateWrapper");

            assert_eq!(wrapper.inner.color, c, "the color was lost on the way into the DOM");
            assert_eq!(wrapper.title.as_str(), DEFAULT_TITLE, "the title was lost");
            assert!(
                wrapper.on_value_change.as_ref().is_some(),
                "the user's value-change callback was lost on the way into the DOM",
            );
        }
    }

    #[test]
    fn dom_of_a_callback_less_color_input_still_registers_the_click_handler() {
        // The handler must always be installed: without it, adding an `on_value_change`
        // later via the state would never be reachable.
        let dom = ColorInput::create(DEFAULT_COLOR).dom();
        assert_eq!(dom.root.callbacks.as_ref().len(), 1);

        let mut state = dom.root.callbacks.as_ref()[0].refany.clone();
        let wrapper = state
            .downcast_ref::<ColorInputStateWrapper>()
            .expect("wrong RefAny type");
        assert!(wrapper.on_value_change.as_ref().is_none());
    }

    #[test]
    fn dom_of_an_unstyled_default_widget_still_carries_its_background() {
        // `ColorInput::default()` has an empty style vec — pushing onto it must still work
        // and must produce exactly the one background property.
        let dom = ColorInput::default().dom();
        assert_eq!(
            inline_properties(&dom),
            vec![expected_background(DEFAULT_COLOR)],
            "a default color input did not render its background alone",
        );
    }

    #[test]
    fn the_rendered_dom_flattens_to_exactly_one_node() {
        // `Dom::estimated_total_children` is a *cached* count; if it under-reports, the
        // flatten under-allocates its arenas.
        let styled = StyledDom::create_from_dom(ColorInput::create(SAMPLE_COLORS[6]).dom());
        assert_eq!(
            styled.node_data.as_ref().len(),
            1,
            "the color input no longer flattens to a single node",
        );
    }

    // ==================================================================
    // on_color_input_clicked
    // ==================================================================

    #[test]
    fn clicking_without_a_callback_is_a_no_op() {
        for c in SAMPLE_COLORS {
            let (styled, state) = laid_out(ColorInput::create(c));
            let (update, changes) = click(styled, &state, node(0));

            assert_eq!(update, Update::DoNothing, "{c:?}: a callback-less click asked for a redraw");
            assert!(changes.is_empty(), "{c:?}: a callback-less click wrote to the DOM");
            assert_eq!(state_color(&state), c, "{c:?}: the click changed the stored color");
        }
    }

    #[test]
    fn clicking_with_a_refany_of_the_wrong_type_is_a_silent_no_op() {
        // The handler downcasts blind; a foreign RefAny must bail out, not reinterpret the
        // bytes as a ColorInputStateWrapper.
        let (styled, _) = laid_out(ColorInput::create(DEFAULT_COLOR));
        let foreign = RefAny::new(0xDEAD_BEEF_u32);

        let (update, changes) = click(styled, &foreign, node(0));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "the handler wrote to the DOM through a foreign RefAny");

        let mut foreign = foreign;
        assert_eq!(
            *foreign
                .downcast_ref::<u32>()
                .expect("the foreign payload was reinterpreted"),
            0xDEAD_BEEF,
            "the handler corrupted a RefAny it did not understand",
        );
    }

    #[test]
    fn clicking_forwards_the_user_callbacks_verdict_verbatim() {
        // The handler is a pure relay: whatever the user callback decides is what the event
        // loop must see. Swallowing a `RefreshDom` would freeze the UI after a color pick.
        let cases: [(ColorInputOnValueChangeCallbackType, Update); 3] = [
            (value_do_nothing, Update::DoNothing),
            (record_value, Update::RefreshDom),
            (value_refresh_all, Update::RefreshDomAllWindows),
        ];
        for (cb, expected) in cases {
            let (styled, state) = laid_out(
                ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(log_refany(), cb),
            );
            let (update, _) = click(styled, &state, node(0));
            assert_eq!(update, expected, "the handler did not forward {expected:?}");
        }
    }

    #[test]
    fn the_callback_sees_this_widgets_color_not_the_default() {
        // The handler reads `color_input.inner` and passes it on. Passing
        // `ColorInputState::default()` (opaque white) instead would type-check and would
        // look right for exactly one of the sample colors.
        for c in SAMPLE_COLORS {
            let probe = log_refany();
            let (styled, state) = laid_out(
                ColorInput::create(c).with_on_value_change(
                    probe.clone(),
                    record_value as ColorInputOnValueChangeCallbackType,
                ),
            );

            let (update, _) = click(styled, &state, node(0));
            assert_eq!(update, Update::RefreshDom);
            assert_eq!(
                read_log(&probe).seen,
                vec![c],
                "the callback was told the wrong color for {c:?}",
            );
        }
    }

    #[test]
    fn the_callback_receives_the_user_payload_not_the_widget_state() {
        let probe = log_refany();
        let (styled, state) = laid_out(
            ColorInput::create(SAMPLE_COLORS[6]).with_on_value_change(
                probe.clone(),
                record_value as ColorInputOnValueChangeCallbackType,
            ),
        );
        click(styled, &state, node(0));

        // It wrote into the ColorLog, so it got the user's payload ...
        assert_eq!(read_log(&probe).seen.len(), 1);
        assert_eq!(read_log(&probe).payload, 0xDEAD_BEEF);

        // ... and that payload is emphatically not the widget state.
        let mut probe = probe;
        assert!(
            probe.downcast_ref::<ColorInputStateWrapper>().is_none(),
            "the user payload and the widget state got confused",
        );
    }

    #[test]
    fn clicking_never_mutates_the_stored_color() {
        // There is no built-in picker dialog: the handler only *reports* the current color.
        // If it ever started writing back, this is where an unreviewed mutation shows up.
        let probe = log_refany();
        let c = SAMPLE_COLORS[6];
        let (_, state) = laid_out(
            ColorInput::create(c).with_on_value_change(
                probe.clone(),
                record_value as ColorInputOnValueChangeCallbackType,
            ),
        );

        for i in 0..8 {
            let (styled, _) = laid_out(ColorInput::create(c));
            let (_, changes) = click(styled, &state, node(0));
            assert!(changes.is_empty(), "click {i} pushed a DOM change");
            assert_eq!(state_color(&state), c, "click {i} altered the stored color");
        }
        assert_eq!(
            read_log(&probe).seen,
            vec![c; 8],
            "the callback did not see the same color on every click",
        );
    }

    #[test]
    fn clicking_a_stale_or_missing_hit_node_does_not_panic() {
        // Stale hit ids reach callbacks after a DOM mutation, and `node_none()` is the
        // "nothing concrete was hit" case. This handler never queries the layout, so all
        // three must sail through and still report the color rather than panicking.
        // usize::MAX is unencodable by NodeId's 1-based scheme and would overflow while
        // building the fixture; usize::MAX - 1 is the repo's MAX_ENCODABLE_NODE.
        let c = SAMPLE_COLORS[4];
        for hit in [node(0), node(99), node(usize::MAX - 1), node_none()] {
            let probe = log_refany();
            let (styled, state) = laid_out(
                ColorInput::create(c).with_on_value_change(
                    probe.clone(),
                    record_value as ColorInputOnValueChangeCallbackType,
                ),
            );
            let (update, changes) = click(styled, &state, hit);

            assert_eq!(update, Update::RefreshDom, "{hit:?}: wrong verdict");
            assert!(changes.is_empty(), "{hit:?}: a DOM change was pushed");
            assert_eq!(read_log(&probe).seen, vec![c], "{hit:?}: wrong color reported");
        }
    }

    #[test]
    fn two_widgets_built_from_the_same_color_do_not_share_state() {
        // `dom()` allocates a fresh `RefAny` per widget. If two swatches aliased one state,
        // clicking one would report through the other's callback as well.
        let a_probe = log_refany();
        let b_probe = log_refany();
        let (a_styled, a_state) = laid_out(ColorInput::create(SAMPLE_COLORS[1]).with_on_value_change(
            a_probe.clone(),
            record_value as ColorInputOnValueChangeCallbackType,
        ));
        let (_b_styled, _b_state) = laid_out(ColorInput::create(SAMPLE_COLORS[1]).with_on_value_change(
            b_probe.clone(),
            record_value as ColorInputOnValueChangeCallbackType,
        ));

        click(a_styled, &a_state, node(0));

        assert_eq!(read_log(&a_probe).seen.len(), 1, "the clicked widget did not report");
        assert!(
            read_log(&b_probe).seen.is_empty(),
            "clicking one color input fired another one's callback",
        );
    }
}
