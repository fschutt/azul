//! The touch pills of a tab bar: what `Auto` is on a phone, the strip that scrolls inside the
//! header, the padded hit area, the marker that brings the selected pill into view, the covers
//! over the cutouts in the status bar's band.
//!
//! Not compiled by the author (house rule).

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    callbacks::Update,
    display_features::{DisplayFeatures, DisplayPreset},
    dom::{Dom, IdOrClass, IdOrClass::Class, NodeType},
    geom::LogicalSize,
    refany::RefAny,
};
use azul_css::{system::SafeAreaInsets, AzString, StringVec};

use super::*;
use crate::widgets::themes::UiTheme;

fn labels(n: usize) -> StringVec {
    StringVec::from_vec((0..n).map(|i| AzString::from(format!("tab {i}"))).collect())
}

fn classes_of(node: &Dom) -> Vec<String> {
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

fn band_of(
    preset: DisplayPreset,
    width: f32,
    height: f32,
) -> crate::widgets::tab_strip::StatusBarBand {
    let size = LogicalSize { width, height };
    crate::widgets::tab_strip::StatusBarBand::from_display(
        &DisplayFeatures::simulate(preset, size),
        &preset.safe_area(size),
        size,
    )
}

fn touch_header(n: usize, active: usize) -> TabHeader {
    TabHeader::create(labels(n))
        .with_active_tab(active)
        .with_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flat)
}

/// The scrolling row of a touch header: its child with the strip's class.
fn strip_of(header: &Dom) -> &Dom {
    header
        .children
        .as_ref()
        .iter()
        .find(|c| classes_of(c).iter().any(|c| c == "__azul-native-tabs-touch-strip"))
        .expect("a touch header holds its strip")
}

/// The pills of a touch strip: its children that carry a tab class (not the two spacers).
fn pills_of(strip: &Dom) -> Vec<&Dom> {
    strip
        .children
        .as_ref()
        .iter()
        .filter(|n| {
            let c = classes_of(n);
            c.iter()
                .any(|c| c == TAB_ACTIVE_CLASS_NAME || c == TAB_NOT_ACTIVE_CLASS_NAME)
        })
        .collect()
}

#[test]
fn auto_is_the_buttons_at_touch_sizes_on_a_phone_and_the_buttons_everywhere_else() {
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        assert_eq!(
            TabStyle::Auto.resolved_for(theme, true),
            TabStyle::TouchPills
        );
        assert_eq!(
            TabStyle::Auto.resolved_for(theme, false),
            TabStyle::Auto.resolved(theme)
        );
        // A style the app named stays, on a phone too.
        for named in [
            TabStyle::Rectangular,
            TabStyle::Pills,
            TabStyle::TouchPills,
        ] {
            assert_eq!(named.resolved_for(theme, true), named);
            assert_eq!(named.resolved_for(theme, false), named);
        }
    }
    // One look in every theme: the theme's buttons floating on the leather.
    for theme in [UiTheme::Flat, UiTheme::Flora] {
        assert_eq!(TabStyle::Auto.resolved_for(theme, false), TabStyle::Pills);
    }
}

#[test]
fn a_desktop_build_is_never_a_phone() {
    // Outside a window's build (and on any desktop target) nothing says phone.
    assert!(!phone_build());
}

#[test]
fn a_touch_pill_is_a_finger_tall_at_the_house_radius_and_a_target_of_the_whole_row() {
    assert_eq!(PILL_TOUCH.height, 32);
    assert_eq!(PILL_TOUCH.radius, PILL_RADIUS, "the house radius: slightly rounded, no capsule");
    assert_eq!(PILL_TOUCH.radius, PILL_DESKTOP.radius, "one look at two sizes");
    // The target is the pill and 8 px above and below: at least 44 pt / 48 dp.
    assert!(PILL_TOUCH.height + 2 * 8 >= 48);
    assert!(PILL_TOUCH.height > PILL_DESKTOP.height);
}

#[test]
fn the_touch_header_is_a_header_around_one_scrolling_row_of_pills() {
    let dom = touch_header(5, 2).dom();
    assert_eq!(
        classes_of(&dom),
        vec!["__azul-native-tabs-touch-header".to_string()]
    );
    // No band: the leather the row lies on (one stretch, with the golden line at its foot), the row, and
    // the two fades at the ends of the lane.
    assert_eq!(dom.children.as_ref().len(), 1 + 1 + 2);
    let leather = &dom.children.as_ref()[0];
    assert_eq!(leather.children.as_ref().len(), 1, "the golden line along the leather's foot");
    let strip = strip_of(&dom);
    assert_eq!(
        classes_of(strip),
        vec!["__azul-native-tabs-touch-strip".to_string()]
    );
    // The tab list is the row: the tabs are its direct children (two spacers around them).
    assert_eq!(
        strip.root.get_accessibility_info().map(|a| a.role),
        Some(AccessibilityRole::PageTabList)
    );
    assert_eq!(strip.children.as_ref().len(), 5 + 2);
    assert_eq!(pills_of(strip).len(), 5);
}

#[test]
fn every_pill_is_a_tab_with_its_name_and_exactly_the_selected_one_says_so() {
    let dom = touch_header(4, 1).dom();
    let pills = pills_of(strip_of(&dom));
    for (i, pill) in pills.iter().enumerate() {
        let info = pill
            .root
            .get_accessibility_info()
            .expect("a tab has accessibility info");
        assert_eq!(info.role, AccessibilityRole::PageTab);
        assert_eq!(
            info.accessibility_name
                .as_ref()
                .map(|n| n.as_str().to_string()),
            Some(format!("tab {i}")),
            "the name is given: the label is not the node's own text"
        );
        let selected = info.states.as_ref().contains(&AccessibilityState::Selected);
        assert_eq!(selected, i == 1, "pill {i}");
        let classes = classes_of(pill);
        assert!(classes.contains(&"__azul-native-tabs-touch-pill".to_string()));
        assert_eq!(
            classes.contains(&TAB_ACTIVE_CLASS_NAME.to_string()),
            i == 1,
            "the arrow keys find the siblings by these classes"
        );
    }
}

#[test]
fn a_pill_holds_its_label_first_then_the_hit_area_and_only_the_selected_one_the_marker() {
    let dom = touch_header(3, 0).dom();
    let pills = pills_of(strip_of(&dom));
    for (i, pill) in pills.iter().enumerate() {
        let children = pill.children.as_ref();
        match children[0].root.get_node_type() {
            NodeType::Text(text) => assert_eq!(text.as_ref().as_str(), format!("tab {i}")),
            other => panic!("the label comes first, got {other:?}"),
        }
        let expected = if i == 0 { 3 } else { 2 };
        assert_eq!(
            children.len(),
            expected,
            "pill {i}: label, hit area{}",
            if i == 0 { ", marker" } else { "" }
        );
    }
}

#[test]
fn the_marker_is_keyed_by_its_tab_so_a_new_selection_mounts_a_new_one() {
    let at = |active: usize| {
        let dom = touch_header(3, active).dom();
        let pills = pills_of(strip_of(&dom));
        let marker = pills[active]
            .children
            .as_ref()
            .last()
            .expect("the marker")
            .root
            .get_key();
        marker
    };
    assert_ne!(at(0), at(1));
    assert_ne!(at(1), at(2));
    assert_eq!(at(1), at(1));
}

#[test]
fn a_header_with_no_selection_has_no_marker() {
    let dom = touch_header(3, 7).dom();
    for pill in pills_of(strip_of(&dom)) {
        assert_eq!(pill.children.as_ref().len(), 2, "label and hit area only");
    }
}

#[test]
fn with_a_click_callback_every_pill_is_a_tab_stop_candidate_and_takes_the_click_and_the_keys() {
    extern "C" fn on_click(_: RefAny, _: CallbackInfo, _: TabHeaderState) -> Update {
        Update::DoNothing
    }
    let dom = touch_header(3, 1)
        .with_on_click(RefAny::new(0usize), on_click as TabOnClickCallbackType)
        .dom();
    for pill in pills_of(strip_of(&dom)) {
        assert_eq!(
            pill.root.get_callbacks().as_ref().len(),
            2,
            "click and arrow keys"
        );
        assert!(pill.root.get_tab_index().is_some(), "on the tab order");
    }
    // Without one the tabs stay out of the tab order and take nothing.
    let inert = touch_header(3, 1).dom();
    for pill in pills_of(strip_of(&inert)) {
        assert!(pill.root.get_callbacks().as_ref().is_empty());
        assert!(pill.root.get_tab_index().is_none());
    }
}

#[test]
fn in_the_pixel_8_band_the_header_is_the_band_and_covers_the_gap_of_the_camera() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let dom = touch_header(6, 0).with_status_bar(band).dom();
    // The leather of the two lanes beside the camera, the row, the two fades of each lane, and one cover
    // for the one gap (the camera and its margins).
    assert_eq!(dom.children.as_ref().len(), 2 + 1 + 2 * 2 + 1);
    assert_eq!(pills_of(strip_of(&dom)).len(), 6);
}

#[test]
fn every_lane_ends_in_a_fade_and_the_first_and_last_pill_stand_outside_of_it() {
    // A pill that slides out of a lane fades out at the lane's edge ...
    for preset in [DisplayPreset::Pixel8, DisplayPreset::DoubleHole] {
        let band = band_of(preset, 360.0, 720.0);
        let lanes = band.lane_spans().len();
        assert_eq!(super::lane_fades(Some(&band), 48.0, 0.0).len(), 2 * lanes, "{preset:?}: two fades per lane");
        // ... and at rest the first and the last pill stand beyond the fade, so it dims nothing.
        let scroll = super::StripScrollData::for_band(Some(&band), Vec::new());
        let first_lane_left = band.lane_spans().first().map_or(0.0, |lane| lane.0);
        let last_lane_right = band.lane_spans().last().map_or(0.0, |lane| lane.1);
        assert!(scroll.lead >= first_lane_left + super::LANE_FADE_PX, "{preset:?}: the first pill is past the fade");
        assert!(
            scroll.trail >= band.width - last_lane_right + super::LANE_FADE_PX,
            "{preset:?}: the last pill is past the fade"
        );
    }
    // The whole row without a band: one lane, two fades.
    assert_eq!(super::lane_fades(None, 48.0, 0.0).len(), 2);
}

#[test]
fn a_double_hole_band_covers_both_gaps() {
    let band = band_of(DisplayPreset::DoubleHole, 360.0, 720.0);
    let dom = touch_header(6, 0).with_status_bar(band).dom();
    // Three lanes of leather, the row, the two fades of each lane, two covers.
    assert_eq!(dom.children.as_ref().len(), 3 + 1 + 3 * 2 + 2);
}

#[test]
fn a_window_without_a_band_gets_an_ordinary_row_even_if_a_band_was_asked_for() {
    let band = crate::widgets::tab_strip::StatusBarBand::from_display(
        &DisplayFeatures::none(),
        &SafeAreaInsets::default(),
        LogicalSize {
            width: 900.0,
            height: 700.0,
        },
    );
    let dom = touch_header(4, 0).with_status_bar(band).dom();
    // The leather, the row and the two fades of the one lane.
    assert_eq!(dom.children.as_ref().len(), 1 + 1 + 2);
}

#[test]
fn a_band_on_a_style_that_is_not_the_touch_pills_is_ignored() {
    let band = band_of(DisplayPreset::Pixel8, 360.0, 720.0);
    let dom = TabHeader::create(labels(3))
        .with_style(TabStyle::Rectangular)
        .with_theme(UiTheme::Flat)
        .with_status_bar(band)
        .dom();
    // The header of the theme's look: spacer, three tabs, spacer.
    assert_eq!(dom.children.as_ref().len(), 3 + 2);
}

#[test]
fn the_other_styles_are_built_as_they_were() {
    for style in [TabStyle::Rectangular, TabStyle::Pills] {
        let dom = TabHeader::create(labels(3))
            .with_style(style)
            .with_theme(UiTheme::Flat)
            .dom();
        assert!(
            !classes_of(&dom).contains(&"__azul-native-tabs-touch-header".to_string()),
            "{style:?} has no touch header"
        );
    }
}

#[test]
fn a_header_gets_no_band_by_default() {
    assert!(TabHeader::default().status_bar.is_none());
    assert!(TabHeader::create(labels(2)).status_bar.is_none());
}

#[test]
fn the_selected_marker_names_itself_so_a_resize_finds_the_selected_pill_again() {
    let dom = touch_header(5, 3).dom();
    let strip = strip_of(&dom);
    let pills = pills_of(strip);
    for (i, pill) in pills.iter().enumerate() {
        let marked = pill
            .children
            .as_ref()
            .iter()
            .any(|c| classes_of(c).iter().any(|c| c == "__azul-tabs-selected-marker"));
        assert_eq!(marked, i == 3, "pill {i}: only the selected one carries the marker");
    }
}

#[test]
fn the_strip_listens_to_its_own_resize() {
    use azul_core::{dom::ComponentEventFilter, dom::EventFilter};

    let resize = |dom: &Dom| {
        dom.root
            .get_callbacks()
            .as_ref()
            .iter()
            .filter(|cb| {
                matches!(cb.event, EventFilter::Component(ComponentEventFilter::NodeResized))
            })
            .count()
    };
    let dom = touch_header(5, 1).dom();
    assert_eq!(resize(strip_of(&dom)), 1, "a turn of the phone moves the lanes: the strip hears it");
    assert_eq!(resize(&dom), 0, "the header around it is not the strip");
}

#[test]
fn a_flora_pill_fades_its_face_together_and_its_label_steps_where_both_inks_read_whatever_it_changes_to() {
    use crate::widgets::{button::ButtonType, themes::flora::FloraButtonKind};
    use azul_css::props::property::CssProperty;
    for button_type in [ButtonType::Default, ButtonType::Primary] {
        let ink = FloraButtonKind::of(button_type).label_ink().expect("a pill's label has an ink");
        for metrics in [&super::PILL_DESKTOP, &super::PILL_TOUCH] {
            let css = super::pill_sized(UiTheme::Flora, button_type, metrics);
            // The animation that governs the pill at rest: the last one declared with no condition.
            let list = css
                .as_ref()
                .iter()
                .rev()
                .filter(|p| p.apply_if.as_ref().is_empty())
                .find_map(|p| match &p.property {
                    CssProperty::Animation(value) => value.get_property().cloned(),
                    _ => None,
                })
                .expect("the pill declares its fade");
            let entries = list.as_ref();
            let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
            assert!(names.contains(&"color") && names.contains(&"background"), "{button_type:?}: {names:?}");
            for entry in entries {
                if entry.name.as_str() == "color" {
                    // The label does not fade with the face (dark to light ink under a face that goes light to
                    // dark crosses it at one frame): it holds, then takes its new colour at once.
                    assert!(entry.duration.inner <= 5, "{button_type:?}: the label's colour steps: {} ms", entry.duration.inner);
                    assert_eq!(
                        entry.delay.inner,
                        super::PILL_FADE_MS * ink.held_permille() / 1000,
                        "{button_type:?}: a {ink:?} label is held for its share of the face's time"
                    );
                } else {
                    assert_eq!(
                        entry.duration.inner,
                        super::PILL_FADE_MS,
                        "{button_type:?}: {} fades over the same time as the rest",
                        entry.name.as_str()
                    );
                }
            }
        }
    }
}

// ---- one rule for the phone's forms: a phone's size, on a phone or a stand-in for one ----

fn is_touch_pills(dom: &azul_core::dom::Dom) -> bool {
    fn has_touch_header(dom: &azul_core::dom::Dom) -> bool {
        dom.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .any(|c| matches!(c, azul_core::dom::IdOrClass::Class(n) if n.as_str() == "__azul-native-tabs-touch-header"))
    }
    has_touch_header(dom)
}

#[test]
fn auto_is_the_touch_pills_in_a_window_under_600_dp_that_sits_on_a_phone_and_the_buttons_elsewhere() {
    use azul_core::callbacks::WindowSizeScope;
    // (width in dp, sits on a simulated phone): the phone's forms only for a phone narrower than 600 dp.
    for (width, preset, touch) in [
        (412.0, true, true),     // a phone, portrait
        (599.0, true, true),
        (640.0, true, false),    // a phone on its side (852 dp in life) and a tablet: the desktop form
        (852.0, true, false),
        (1280.0, true, false),
        (412.0, false, false),   // a desktop window that is narrow: never the phone's form
        (1280.0, false, false),
    ] {
        let _scope = WindowSizeScope::enter_for_window(LogicalSize { width, height: 900.0 }, false, preset);
        let dom = TabHeader::create(labels(4)).with_theme(UiTheme::Flora).dom();
        assert_eq!(
            is_touch_pills(&dom),
            touch,
            "{width} dp, preset {preset}: {}",
            if touch { "touch pills" } else { "the desktop form" }
        );
    }
}

// ---- at rest, no pill stands under the camera: the marker's callback writes the room ----

/// A header of long tab names in the Pixel 8's band, laid out in a phone's width with the system's fonts:
/// the window, the strip's numbers as the build made them, and the node ids of the strip, the selected
/// pill's marker and the pills.
struct BandStrip {
    window: crate::window::LayoutWindow,
    data: StripScrollData,
    marker: azul_core::dom::NodeId,
    pills: Vec<azul_core::dom::NodeId>,
}

fn band_strip() -> BandStrip {
    use azul_core::{
        dom::{DomId, NodeId},
        resources::RendererResources,
        styled_dom::StyledDom,
    };
    use rust_fontconfig::FcFontCache;

    use crate::{callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState};

    let size = LogicalSize { width: 411.0, height: 800.0 };
    let band = band_of(DisplayPreset::Pixel8, size.width, size.height);
    let names = StringVec::from_vec(
        ["Documents", "Reviewing", "Insertion", "Viewing"].iter().map(|s| AzString::from(*s)).collect(),
    );
    let header = TabHeader::create(names)
        .with_active_tab(1)
        .with_style(TabStyle::TouchPills)
        .with_theme(UiTheme::Flat)
        .with_status_bar(band.clone());
    let mut dom = Dom::create_body().with_child(header.dom());
    let (css, _) = azul_css::parser2::new_from_str("* { margin: 0; padding: 0; }");
    let styled = StyledDom::create(&mut dom, css);
    let mut window = LayoutWindow::new(FcFontCache::build()).expect("a layout window");
    let mut state = FullWindowState::default();
    state.size.dimensions = size;
    window.current_window_state = state.clone();
    window
        .layout_and_generate_display_list(
            styled,
            &state,
            &RendererResources::default(),
            &ExternalSystemCallbacks::rust_internal(),
            &mut None,
        )
        .expect("the header lays out");
    let with_class = |class: &str| -> Vec<NodeId> {
        let styled = &window.layout_results[&DomId::ROOT_ID].styled_dom;
        (0..styled.node_data.as_ref().len())
            .map(NodeId::new)
            .filter(|n| {
                styled.node_data.as_container()[*n]
                    .get_ids_and_classes()
                    .iter()
                    .any(|c| matches!(c.as_class(), Some(s) if s == class))
            })
            .collect()
    };
    let marker = with_class(SELECTED_MARKER_CLASS).first().copied().expect("the selected pill carries the marker");
    let pills = with_class("__azul-native-tabs-touch-pill");
    let data = StripScrollData::for_band(
        Some(&band),
        vec![TAB_ACTIVE_CLASS_NAME.to_string(), TAB_NOT_ACTIVE_CLASS_NAME.to_string()],
    );
    BandStrip { window, data, marker, pills }
}

/// The left margins the marker's `AfterMount` writes on the pills of `strip` (node id, px).
fn margins_written(strip: &BandStrip) -> Vec<(azul_core::dom::NodeId, f32)> {
    use std::{collections::BTreeMap, sync::{Arc, Mutex}};

    use azul_core::{
        dom::{DomId, DomNodeId},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        refany::OptionRefAny,
        resources::RendererResources,
        styled_dom::NodeHierarchyItemId,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;

    use crate::{
        callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        window_state::FullWindowState,
    };

    let renderer_resources = RendererResources::default();
    let previous_window_state: Option<FullWindowState> = None;
    let current_window_state = FullWindowState::default();
    let gl_context = OptionGlContextPtr::None;
    let scroll_states = BTreeMap::new();
    let window_handle = RawWindowHandle::Unsupported;
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let ref_data = CallbackInfoRefData {
        layout_window: &strip.window,
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
        icu_localizer: crate::icu::IcuLocalizerHandle::default(),
        ctx: core::cell::RefCell::new(OptionRefAny::None),
    };
    let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
    let info = CallbackInfo::new(
        &ref_data,
        &changes,
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(strip.marker)),
        },
        OptionLogicalPosition::None,
        OptionLogicalPosition::None,
    );
    let _ = on_selected_pill_mounted(RefAny::new(strip.data.clone()), info);
    let written = changes.lock().expect("the changes").clone();
    written
        .into_iter()
        .filter_map(|change| match change {
            CallbackChange::ChangeNodeCssProperties { node_id, properties, .. } => {
                properties.as_ref().iter().find_map(|p| match p {
                    azul_css::props::property::CssProperty::MarginLeft(v) => {
                        v.get_property().map(|m| (node_id, m.inner.to_pixels_internal(0.0, 0.0, 0.0)))
                    }
                    _ => None,
                })
            }
            _ => None,
        })
        .collect()
}

#[test]
fn the_marker_of_the_selected_pill_writes_room_before_the_pill_that_would_stand_under_the_camera() {
    use azul_core::dom::{DomId, DomNodeId};
    use azul_core::styled_dom::NodeHierarchyItemId;

    let strip = band_strip();
    assert_eq!(strip.pills.len(), 4, "the four pills");
    let widths: Vec<f32> = strip
        .pills
        .iter()
        .map(|pill| {
            strip
                .window
                .get_node_layout_rect(DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(Some(*pill)),
                })
                .expect("the pill is laid out")
                .size
                .width
        })
        .collect();
    let expected = crate::widgets::tab_strip::room_around_cutouts(
        &widths,
        strip.data.lead,
        strip.data.gap,
        &strip.data.lanes,
        TOUCH_LEAD as f32,
    );
    assert!(
        expected.iter().any(|room| *room > 1.0),
        "harness: a pill of {widths:?} would stand under the camera (lanes {:?}): {expected:?}",
        strip.data.lanes
    );
    let written = margins_written(&strip);
    assert_eq!(written.len(), 4, "one margin for every pill: {written:?}");
    for ((pill, extra), (written_for, written)) in strip.pills.iter().zip(&expected).zip(&written) {
        assert_eq!(pill, written_for, "the margins are written in the order of the pills");
        assert!((extra - written).abs() < 0.01, "pill {pill:?}: wrote {written}, the plan is {extra}");
    }
}

#[test]
fn a_strip_that_places_its_pills_itself_writes_no_room() {
    let mut strip = band_strip();
    strip.data.camera_room = false;
    assert!(margins_written(&strip).is_empty(), "no margin is written for a strip that opted out");
}
