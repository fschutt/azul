//! `<input type=datetime-local>`: a date and a time in one control.

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::Update,
        dom::{AttributeType, Dom, DomId, DomNodeId, IdOrClass, NodeId},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::{OptionRefAny, RefAny},
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle},
    };
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::{
            date_picker::DatePickerState,
            theme_probe,
            themes::{OptionUiTheme, UiTheme},
            time_picker::TimePickerState,
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

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

    /// Runs `f` with a real `CallbackInfo` over an empty window: the part
    /// handlers never look at the DOM, only at their payload.
    fn with_info<R>(f: impl FnOnce(CallbackInfo) -> R) -> R {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window.layout_results.insert(
            DomId::ROOT_ID,
            DomLayoutResult {
                styled_dom: StyledDom::default(),
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
            },
        );
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
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: core::cell::RefCell::new(OptionRefAny::None),
        };
        let changes = Arc::new(Mutex::new(Vec::new()));
        let hit = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
        };
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        f(info)
    }

    struct Seen(Vec<DateTimeLocalPickerState>);

    extern "C" fn record(
        mut data: RefAny,
        _: CallbackInfo,
        state: DateTimeLocalPickerState,
    ) -> Update {
        if let Some(mut seen) = data.downcast_mut::<Seen>() {
            seen.0.push(state);
        }
        Update::RefreshDom
    }

    fn seen(log: &RefAny) -> Vec<DateTimeLocalPickerState> {
        let mut log = log.clone();
        log.downcast_ref::<Seen>().expect("the log changed type").0.clone()
    }

    fn shared_of(dom: &Dom) -> RefAny {
        dom.root
            .get_dataset()
            .cloned()
            .expect("the picker carries its state as its dataset")
    }

    fn state_of(shared: &RefAny) -> DateTimeLocalPickerState {
        let mut shared = shared.clone();
        shared
            .downcast_ref::<DateTimeLocalPickerStateWrapper>()
            .expect("the dataset is the picker's state")
            .inner
    }

    #[test]
    fn a_datetime_local_picker_is_a_date_picker_and_a_time_picker_in_one_row() {
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5).dom();
        let kids = dom.children.as_ref();
        assert_eq!(kids.len(), 2, "one date part, one time part");
        assert!(classes(&kids[0]).contains(&"__azul-native-date-picker".to_string()));
        assert!(classes(&kids[1]).contains(&"__azul-native-time-picker".to_string()));
        assert!(classes(&dom).contains(&DATETIME_LOCAL_CLASS.to_string()));
    }

    #[test]
    fn its_value_is_the_html_datetime_local_string() {
        let state = state_of(&shared_of(&DateTimeLocalPicker::create(2026, 9, 29, 14, 5).dom()));
        assert_eq!(state.to_html_value(), "2026-09-29T14:05");

        // A 12-hour display still submits the canonical 24-hour time.
        let mut s = state;
        s.time = TimePickerState {
            hour: 2,
            minute: 5,
            is_pm: true,
            is_24h: false,
        };
        assert_eq!(s.to_html_value(), "2026-09-29T14:05");
    }

    #[test]
    fn it_carries_its_state_name_and_type_for_a_form() {
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_name("when".into())
            .dom();
        let _ = state_of(&shared_of(&dom));
        let attrs = dom.root.attributes();
        assert!(attrs
            .as_ref()
            .iter()
            .any(|a| matches!(a, AttributeType::Name(n) if n.as_str() == "when")));
        assert!(attrs.as_ref().iter().any(
            |a| matches!(a, AttributeType::InputType(t) if t.as_str() == "datetime-local")
        ));
    }

    #[test]
    fn a_date_part_change_updates_the_combined_state_and_reports_it() {
        let log = RefAny::new(Seen(Vec::new()));
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_on_change(log.clone(), record as DateTimeLocalPickerOnChangeCallbackType)
            .dom();
        let shared = shared_of(&dom);
        let update = with_info(|info| {
            on_date_part_change(
                shared.clone(),
                info,
                DatePickerState {
                    year: 2026,
                    month: 10,
                    day: 1,
                },
            )
        });
        assert_eq!(update, Update::RefreshDom, "the app's verdict");
        let s = state_of(&shared);
        assert_eq!((s.date.year, s.date.month, s.date.day), (2026, 10, 1));
        assert_eq!((s.time.hour, s.time.minute), (14, 5), "the time part is kept");
        assert_eq!(
            seen(&log).last().map(DateTimeLocalPickerState::to_html_value).as_deref(),
            Some("2026-10-01T14:05")
        );
    }

    #[test]
    fn a_time_part_change_keeps_the_date() {
        let log = RefAny::new(Seen(Vec::new()));
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_on_change(log.clone(), record as DateTimeLocalPickerOnChangeCallbackType)
            .dom();
        let shared = shared_of(&dom);
        let _ = with_info(|info| {
            on_time_part_change(
                shared.clone(),
                info,
                TimePickerState {
                    hour: 9,
                    minute: 30,
                    is_pm: false,
                    is_24h: true,
                },
            )
        });
        assert_eq!(state_of(&shared).to_html_value(), "2026-09-29T09:30");
        assert_eq!(seen(&log).len(), 1);
    }

    #[test]
    fn both_themes_style_the_control_in_light_and_dark() {
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let mut picker = DateTimeLocalPicker::create(2026, 9, 29, 14, 5);
            picker.theme = OptionUiTheme::Some(theme);
            let dom = picker.dom();
            assert!(
                !theme_probe::dark(&dom).is_empty(),
                "{theme:?}: no dark-mode declarations on the datetime-local row"
            );
        }
        let dom = DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
            .with_theme(UiTheme::Flora)
            .dom();
        assert_eq!(
            state_of(&shared_of(&dom)).to_html_value(),
            "2026-09-29T14:05"
        );
    }
}
