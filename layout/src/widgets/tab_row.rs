//! The desktop's row of tab buttons ([`TabStyle::Pills`]): a fixed box on leather with a row of pills
//! inside it that scrolls.
//!
//! A row of 22 px buttons that is longer than the window used to be cut at the window's edge (or squeezed
//! until the labels broke). It is the touch strip's build at a pointer's sizes: the header is a FIXED
//! box (`overflow: hidden`) that holds the leather with its golden line, the window's chevron, and the
//! row of pills, which is the part that scrolls - the wheel (a vertical wheel over a box that only
//! scrolls sideways moves it sideways), the arrow keys (the engine's roving focus moves the selection and
//! the selected pill comes into view by itself), a trackpad, a drag. The first and the last pill stand
//! [`PILL_LEAD`] in, and a fade of that width in the leather's colour lies over each end of the row: a
//! pill that slides out is dimmed instead of cut, and with nothing more to scroll to nothing shows. No
//! "more" pill, no scrollbar.
//!
//! What the pills keep from the row they replace: the geometry (22 px tall, centred in a 28 px row, 6 px
//! apart, the first 10 px in from the edge, the golden line of 2 px under the row), the classes the tests
//! and the arrow keys find the tabs by, the tab role and the selected state, the dataset, the click and
//! arrow-key callbacks and the roving tab stop. A bar with a reveal keeps the row it had: the reveal's
//! hover areas hang on the bar's own nodes.

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::{CoreCallback, CoreCallbackData, CoreCallbackDataVec},
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, IdOrClassVec},
    refany::RefAny,
};
use azul_css::{AzString, OptionString};

use super::{
    chevron_over_leather, lane_fades_sized, leather_under, on_tab_click, on_tab_key, pill_sized, selected_marker,
    strip_resized_callbacks, StripScrollData, TabHeader, TabHeaderLook, TabLocalDataset, PILL_DESKTOP, PILL_GAP,
    PILL_LEAD, PILL_ROW, PILL_RULE, TAB_ACTIVE_CLASS_NAME, TAB_NOT_ACTIVE_CLASS_NAME, TEXT_SCALE_CAP_CSS,
};
use crate::widgets::{button::ButtonType, themes::UiTheme};

/// The class of the desktop row's header, next to the one every tab bar has: what a test or a scenario
/// finds a scrolling row by.
pub(super) const ROW_HEADER_CLASS: &str = "__azul-native-tabs-pill-row";
/// The class of the row that scrolls, inside the header: not the touch strip's (a page with both finds
/// each by its own).
pub(super) const ROW_STRIP_CLASS: &str = "__azul-native-tabs-pill-strip";

static ROW_STRIP_CLASSES: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str(ROW_STRIP_CLASS))];

static ACTIVE_CLASSES: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str(TAB_ACTIVE_CLASS_NAME))];
static INACTIVE_CLASSES: &[IdOrClass] = &[IdOrClass::Class(AzString::from_const_str(TAB_NOT_ACTIVE_CLASS_NAME))];

/// A desktop pill: [`pill_sized`] at [`PILL_DESKTOP`], which does not shrink in a row longer than the
/// window and is the box the pill's hit area hangs on.
fn desktop_pill(theme: UiTheme, button_type: ButtonType) -> azul_css::dynamic_selector::CssPropertyWithConditionsVec {
    use azul_css::{
        dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
        props::{
            basic::FloatValue,
            layout::{LayoutFlexShrink, LayoutPosition},
            property::CssProperty,
        },
    };

    use crate::widgets::themes::decl;

    let mut v = pill_sized(theme, button_type, &PILL_DESKTOP).into_library_owned_vec();
    v.push(CssPropertyWithConditions::simple(CssProperty::const_flex_shrink(LayoutFlexShrink {
        inner: FloatValue::const_new(0),
    })));
    v.push(decl::position(LayoutPosition::Relative));
    CssPropertyWithConditionsVec::from_vec(v)
}

/// The transparent box that makes a desktop pill's target a little more than the picture: 3 px beside it
/// (half the gap to the next pill) and 3 px above and below (the row is 28 px, the pill 22).
fn hit_extender() -> Dom {
    Dom::create_div().with_css("position: absolute; left: -3px; right: -3px; top: -3px; bottom: -3px;")
}

impl TabHeader {
    /// The tab bar as a scrolling row of desktop pills: see the module's text. `look` is the Pills look
    /// the bar was asked for (its theme marker goes on the header).
    #[allow(clippy::too_many_lines)] // one build: the pills, the strip, the leather, the fades, the chevron
    pub(super) fn dom_row(self, theme: UiTheme, look: &TabHeaderLook) -> Dom {
        let tab_count = self.tabs.as_ref().len();
        let active = (self.active_tab < tab_count).then_some(self.active_tab);
        let on_click_is_some = self.on_click.is_some();
        let tab_stop = crate::widgets::roving::stop_index(Some(self.active_tab), tab_count);
        #[allow(clippy::cast_precision_loss)] // a few px
        let (gap_px, lead_px, row, rule) = (
            PILL_GAP as f32,
            PILL_LEAD as f32,
            (PILL_ROW + PILL_RULE) as f32,
            PILL_RULE as f32,
        );
        // The window's one chevron, at the left end of the row: the pills start after it.
        let chevron = if self.content_toggle {
            crate::widgets::tab_content_toggle::chevron_for_row(row - rule)
        } else {
            None
        };
        let chevron_room = if chevron.is_some() {
            crate::widgets::tab_content_toggle::CONTENT_TOGGLE_PX
        } else {
            0.0
        };
        // The numbers the strip's scroll-into-view works from: no band, so one lane the width of the view.
        let mut scroll = StripScrollData::for_band(
            None,
            vec![TAB_ACTIVE_CLASS_NAME.to_string(), TAB_NOT_ACTIVE_CLASS_NAME.to_string()],
        );
        scroll.lead = lead_px + chevron_room;
        scroll.trail = lead_px;
        scroll.gap = gap_px;
        scroll.camera_room = false;
        // The first pill stands 10 px in at rest: a selected first (or last) pill is in its place, and the
        // strip does not scroll it out of it.
        scroll.inset = lead_px.min(scroll.inset);

        let dataset = TabLocalDataset {
            tab_idx: 0,
            on_click: self.on_click,
        };
        let mut nodes: Vec<Dom> = Vec::with_capacity(tab_count + 2);
        nodes.push(
            Dom::create_div().with_css(&format!(
                "flex-shrink: 0; width: {}px; height: 1px;",
                (scroll.lead - gap_px).max(0.0)
            )),
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
                        event: EventFilter::Focus(azul_core::events::FocusEventFilter::VirtualKeyDown),
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
            // The pill is a box with its label in a block of its own (the hit area and the marker hang on the box
            // too: a raw text run among flex items has no box, the text lint says so).
            let mut pill = crate::widgets::widget_p_chrome()
                .with_child(crate::widgets::widget_p_with_text(label.clone()))
                .with_callbacks(callbacks)
                .with_dataset(Some(tab_dataset).into())
                .with_css_props(desktop_pill(
                    theme,
                    if selected {
                        ButtonType::Primary
                    } else {
                        ButtonType::Default
                    },
                ))
                .with_ids_and_classes(IdOrClassVec::from_const_slice(if selected {
                    ACTIVE_CLASSES
                } else {
                    INACTIVE_CLASSES
                }))
                // A tab, and whether it is the active one. The pill's text is not the node's own (the hit
                // area hangs on it too), so the name is given.
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
                pill = pill.with_tab_index(crate::widgets::roving::item_tab_index(tab_idx, tab_stop));
            }
            pill.add_child(hit_extender());
            if selected {
                pill.add_child(selected_marker(scroll.clone(), tab_idx));
            }
            nodes.push(pill);
        }
        nodes.push(
            Dom::create_div().with_css(&format!("flex-shrink: 0; width: {}px; height: 1px;", (scroll.trail - gap_px).max(0.0))),
        );

        // The row: longer than the window, it scrolls sideways (the wheel, the keys, a drag) and shows
        // no scrollbar. As tall as the row of the pills: the golden line under it is the leather's.
        let strip = Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_const_slice(ROW_STRIP_CLASSES))
            .with_css(&format!(
                "display: flex; flex-direction: row; align-items: center; box-sizing: border-box; \
                 width: 100%; height: {}px; overflow-x: auto; overflow-y: hidden; \
                 scrollbar-width: none; column-gap: {gap_px}px;",
                row - rule
            ))
            // The tab list its tabs belong to: the tabs are its direct children.
            .with_accessibility_info(AccessibilityInfo {
                role: AccessibilityRole::PageTabList,
                ..Default::default()
            })
            // A window of another width moves the room the row has: the selected pill is brought into view.
            .with_callbacks(strip_resized_callbacks(scroll))
            .with_children(DomVec::from_vec(nodes));

        // The leather under the row with the golden line at its foot; the strip's own pills lie over it,
        // and the fades over them.
        let mut children = leather_under(look, None);
        children.push(strip);
        children.extend(lane_fades_sized(lead_px, None, row, chevron_room));
        // The window's one chevron at the left end of the row, over the pills and their fade.
        if let Some(chevron) = chevron {
            children.extend(chevron_over_leather(look, 0.0, chevron));
        }
        let mut classes = vec![
            IdOrClass::Class(AzString::from_const_str("__azul-native-tabs-header")),
            IdOrClass::Class(AzString::from_const_str(ROW_HEADER_CLASS)),
        ];
        if let Some(marker) = look.marker {
            classes.push(IdOrClass::Class(AzString::from_const_str(marker)));
        }
        Dom::create_div()
            .with_ids_and_classes(IdOrClassVec::from_vec(classes))
            .with_css(&format!(
                "position: relative; box-sizing: border-box; width: 100%; height: {row}px; \
                 flex-shrink: 0; overflow: hidden; {TEXT_SCALE_CAP_CSS}"
            ))
            .with_children(DomVec::from_vec(children))
    }
}
