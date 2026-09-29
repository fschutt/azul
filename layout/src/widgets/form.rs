//! `<form>`: named controls, a submit that collects them, a reset that
//! restores them.

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::Update,
        dom::{AttributeType, Dom, DomId, DomNodeId, DomVec, EventFilter, HoverEventFilter, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        refany::{OptionRefAny, RefAny},
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, VirtualKeyCode},
    };
    use azul_css::{AzString, StringVec};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        widgets::{
            button::{Button, ButtonFormAction},
            date_picker::DatePicker,
            datetime_local::DateTimeLocalPicker,
            text_input::{TextInput, TextInputStateWrapper},
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Harness
    // ------------------------------------------------------------------

    fn dom_node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    /// Runs `f` with a real `CallbackInfo` over a window holding `styled_dom`
    /// (no layout: the form handlers only walk the node hierarchy and read
    /// datasets and attributes). `key` is the pressed key, if any.
    fn run<R>(
        styled_dom: StyledDom,
        hit: DomNodeId,
        key: Option<VirtualKeyCode>,
        f: impl FnOnce(CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        layout_window.layout_results.insert(
            DomId::ROOT_ID,
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
            },
        );
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let mut current_window_state = FullWindowState::default();
        current_window_state.keyboard_state.current_virtual_keycode = key.into();
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
        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            hit,
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );
        let r = f(info);
        let pushed = info.take_changes();
        (r, pushed)
    }

    /// What the app's form callbacks were handed, in call order.
    #[derive(Default)]
    struct Log {
        submitted: Vec<FormData>,
        reset: Vec<FormData>,
    }

    extern "C" fn record_submit(mut data: RefAny, _: CallbackInfo, form_data: FormData) -> Update {
        if let Some(mut log) = data.downcast_mut::<Log>() {
            log.submitted.push(form_data);
        }
        Update::RefreshDom
    }

    extern "C" fn record_reset(mut data: RefAny, _: CallbackInfo, form_data: FormData) -> Update {
        if let Some(mut log) = data.downcast_mut::<Log>() {
            log.reset.push(form_data);
        }
        Update::RefreshDomAllWindows
    }

    fn submitted(log: &RefAny) -> Vec<FormData> {
        let mut log = log.clone();
        log.downcast_ref::<Log>().expect("log").submitted.clone()
    }

    fn resets(log: &RefAny) -> Vec<FormData> {
        let mut log = log.clone();
        log.downcast_ref::<Log>().expect("log").reset.clone()
    }

    fn pairs(data: &FormData) -> Vec<(String, String)> {
        data.entries
            .as_ref()
            .iter()
            .map(|e| (e.name.as_str().to_string(), e.value.as_str().to_string()))
            .collect()
    }

    fn invalid_names(data: &FormData) -> Vec<String> {
        data.invalid
            .as_ref()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect()
    }

    /// The flattened index of the first node carrying a callback to `cb`.
    fn node_with_callback(sd: &StyledDom, cb: usize) -> usize {
        sd.node_data
            .as_ref()
            .iter()
            .position(|nd| nd.callbacks.as_ref().iter().any(|c| c.callback.cb == cb))
            .expect("no node carries that callback")
    }

    /// The flattened index of the node named `name`.
    fn named(sd: &StyledDom, name: &str) -> usize {
        sd.node_data
            .as_ref()
            .iter()
            .position(|nd| {
                nd.attributes()
                    .as_ref()
                    .iter()
                    .any(|a| matches!(a, AttributeType::Name(n) if n.as_str() == name))
            })
            .unwrap_or_else(|| panic!("no node is named {name:?}"))
    }

    /// Overwrite the text a rendered TextInput holds, the way typing would.
    fn type_into(sd: &StyledDom, name: &str, text: &str) {
        let idx = named(sd, name);
        let mut ds = sd.node_data.as_ref()[idx]
            .get_dataset()
            .cloned()
            .expect("a TextInput carries its state");
        let mut w = ds
            .downcast_mut::<TextInputStateWrapper>()
            .expect("the dataset is a TextInput's state");
        w.inner.text = text.chars().map(|c| c as u32).collect::<Vec<_>>().into();
    }

    fn text_of(sd: &StyledDom, name: &str) -> String {
        let idx = named(sd, name);
        let mut ds = sd.node_data.as_ref()[idx].get_dataset().cloned().expect("state");
        let w = ds.downcast_ref::<TextInputStateWrapper>().expect("TextInput state");
        w.inner.get_text()
    }

    /// user (text), mail (email, invalid at build), period (month), when
    /// (datetime-local), an UNNAMED field, and the three buttons.
    fn sample_form(log: &RefAny) -> Form {
        Form::create(DomVec::from_vec(vec![
            TextInput::create()
                .with_name("user".into())
                .with_text("ann".into())
                .dom(),
            TextInput::create_email()
                .with_name("mail".into())
                .with_text("not-an-email".into())
                .dom(),
            DatePicker::create_month(2026, 9)
                .with_name("period".into())
                .dom(),
            DateTimeLocalPicker::create(2026, 9, 29, 14, 5)
                .with_name("when".into())
                .dom(),
            TextInput::create().with_text("ignored".into()).dom(),
            Button::create_submit("Send".into()).dom(),
            Button::create_reset("Clear".into()).dom(),
        ]))
        .with_on_submit(log.clone(), record_submit as FormOnSubmitCallbackType)
        .with_on_reset(log.clone(), record_reset as FormOnResetCallbackType)
    }

    // ------------------------------------------------------------------
    // FormData
    // ------------------------------------------------------------------

    #[test]
    fn form_data_is_a_multimap_of_named_values() {
        let data = FormData {
            entries: vec![
                FormEntry {
                    name: "tag".into(),
                    value: "a".into(),
                },
                FormEntry {
                    name: "user".into(),
                    value: "ann".into(),
                },
                FormEntry {
                    name: "tag".into(),
                    value: "b".into(),
                },
            ]
            .into(),
            invalid: StringVec::from_const_slice(&[]),
        };
        assert_eq!(data.get("tag".into()).into_option().as_ref().map(AzString::as_str), Some("a"));
        assert_eq!(data.get("nope".into()).into_option(), None);
        let all: Vec<String> = data
            .get_all("tag".into())
            .as_ref()
            .iter()
            .map(|s| s.as_str().to_string())
            .collect();
        assert_eq!(all, vec!["a".to_string(), "b".to_string()]);
        assert!(data.has("user".into()));
        assert!(!data.has("nope".into()));
        assert!(data.is_valid());
    }

    // ------------------------------------------------------------------
    // Building
    // ------------------------------------------------------------------

    #[test]
    fn a_form_is_a_form_node_around_its_children() {
        let log = RefAny::new(Log::default());
        let dom = sample_form(&log).dom();
        assert!(matches!(dom.root.get_node_type(), NodeType::Form));
        assert_eq!(dom.children.as_ref().len(), 7);
        let events: Vec<EventFilter> = dom.root.callbacks.as_ref().iter().map(|c| c.event).collect();
        assert!(
            events.contains(&EventFilter::Hover(HoverEventFilter::Submit)),
            "the engine's Submit (Enter in a control) must reach the form"
        );
        assert!(events.contains(&EventFilter::Hover(HoverEventFilter::Reset)));
    }

    #[test]
    fn a_form_records_each_named_controls_initial_value_at_build() {
        let log = RefAny::new(Log::default());
        let dom = sample_form(&log).dom();
        let mut ds = dom.root.get_dataset().cloned().expect("the form carries its state");
        let state = ds.downcast_ref::<FormStateWrapper>().expect("form state");
        assert_eq!(
            pairs(&state.initial),
            vec![
                ("user".to_string(), "ann".to_string()),
                ("mail".to_string(), "not-an-email".to_string()),
                ("period".to_string(), "2026-09".to_string()),
                ("when".to_string(), "2026-09-29T14:05".to_string()),
            ],
            "only NAMED controls, in document order"
        );
        assert_eq!(invalid_names(&state.initial), vec!["mail".to_string()]);
    }

    #[test]
    fn submit_reset_and_image_buttons_declare_their_html_type() {
        for (button, ty) in [
            (Button::create_submit("Send".into()), "submit"),
            (Button::create_reset("Clear".into()), "reset"),
            (
                Button::create_image(
                    azul_core::resources::ImageRef::null_image(
                        1,
                        1,
                        azul_core::resources::RawImageFormat::RGBA8,
                        Vec::new(),
                    ),
                    "Go".into(),
                ),
                "image",
            ),
        ] {
            let dom = button.dom();
            assert!(
                dom.root.attributes().as_ref().iter().any(
                    |a| matches!(a, AttributeType::InputType(t) if t.as_str() == ty)
                ),
                "no type={ty}"
            );
            assert!(
                dom.root
                    .callbacks
                    .as_ref()
                    .iter()
                    .any(|c| c.callback.cb == default_on_form_button_click as usize),
                "a {ty} button must act on its form"
            );
        }
        let image = Button::create_image(
            azul_core::resources::ImageRef::null_image(
                1,
                1,
                azul_core::resources::RawImageFormat::RGBA8,
                Vec::new(),
            ),
            "Go".into(),
        );
        assert_eq!(image.form_action, ButtonFormAction::Submit);
        let a11y = image.dom();
        let name = a11y
            .root
            .get_accessibility_info()
            .and_then(|i| i.accessibility_name.as_ref().map(|n| n.as_str().to_string()));
        assert_eq!(name.as_deref(), Some("Go"), "an image button is named by its alt text");
    }

    #[test]
    fn a_plain_button_has_no_form_action() {
        let dom = Button::create("Hi".into()).dom();
        assert!(!dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .any(|c| c.callback.cb == default_on_form_button_click as usize));
    }

    // ------------------------------------------------------------------
    // Submit
    // ------------------------------------------------------------------

    #[test]
    fn a_submit_button_hands_the_app_the_current_values() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        type_into(&sd, "user", "bob");
        let button = node_with_callback(&sd, default_on_form_button_click as usize);
        let payload = sd.node_data.as_ref()[button]
            .callbacks
            .as_ref()
            .iter()
            .find(|c| c.callback.cb == default_on_form_button_click as usize)
            .map(|c| c.refany.clone())
            .expect("payload");
        let (update, _) = run(sd, dom_node(button), None, |info| {
            default_on_form_button_click(payload.clone(), info)
        });
        assert_eq!(update, Update::RefreshDom, "the app's verdict");
        let got = submitted(&log);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].get("user".into()).into_option().map(|s| s.as_str().to_string()),
            Some("bob".to_string()),
            "a submit reads the CURRENT value, not the initial one"
        );
        assert_eq!(invalid_names(&got[0]), vec!["mail".to_string()]);
        assert!(!got[0].is_valid());
    }

    #[test]
    fn a_failed_submit_paints_the_invalid_look_on_the_invalid_fields() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let mail = named(&sd, "mail");
        let (_, changes) = run(sd, dom_node(0), None, |mut info| submit_form(&mut info, dom_node(0)));
        assert!(
            changes.iter().any(|c| matches!(
                c,
                CallbackChange::OverrideNodeCssProperties { node_id, .. } if *node_id == NodeId::new(mail)
            )),
            "the invalid e-mail field was not marked: {changes:?}"
        );
    }

    #[test]
    fn the_engines_submit_event_on_the_form_runs_the_same_submit() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let state = sd.node_data.as_ref()[0].get_dataset().cloned().expect("form state");
        let (_, _) = run(sd, dom_node(0), None, |info| {
            default_on_form_submit_event(state.clone(), info)
        });
        assert_eq!(submitted(&log).len(), 1);
    }

    #[test]
    fn enter_in_a_text_field_submits_its_form() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        let user = named(&sd, "user");
        let state = sd.node_data.as_ref()[user].get_dataset().cloned().expect("field state");
        let _ = run(sd, dom_node(user), Some(VirtualKeyCode::Return), |info| {
            crate::widgets::text_input::default_on_virtual_key_down(state.clone(), info)
        });
        assert_eq!(submitted(&log).len(), 1, "HTML's implicit submission");
    }

    #[test]
    fn a_submit_button_outside_a_form_does_nothing() {
        let dom = Button::create_submit("Send".into()).dom();
        let payload = dom
            .root
            .callbacks
            .as_ref()
            .iter()
            .find(|c| c.callback.cb == default_on_form_button_click as usize)
            .map(|c| c.refany.clone())
            .expect("payload");
        let sd = StyledDom::create_from_dom(dom);
        let (update, _) = run(sd, dom_node(0), None, |info| {
            default_on_form_button_click(payload.clone(), info)
        });
        assert_eq!(update, Update::DoNothing);
    }

    // ------------------------------------------------------------------
    // Reset
    // ------------------------------------------------------------------

    #[test]
    fn a_reset_restores_each_text_field_and_hands_the_app_the_initial_values() {
        let log = RefAny::new(Log::default());
        let sd = StyledDom::create_from_dom(sample_form(&log).dom());
        type_into(&sd, "user", "bob");
        let user = named(&sd, "user");
        let (update, changes) = run(sd.clone(), dom_node(0), None, |mut info| {
            reset_form(&mut info, dom_node(0))
        });
        assert_eq!(update, Update::RefreshDomAllWindows, "the app's verdict");
        assert_eq!(text_of(&sd, "user"), "ann", "the field is back at its initial value");
        let got = resets(&log);
        assert_eq!(got.len(), 1);
        assert_eq!(
            got[0].get("user".into()).into_option().map(|s| s.as_str().to_string()),
            Some("ann".to_string())
        );
        // The field's line is re-texted: user(container) > line <p> > text.
        assert!(
            changes.iter().any(|c| matches!(
                c,
                CallbackChange::ChangeNodeText { node_id, text }
                    if *node_id == dom_node(user + 2) && text.as_str() == "ann"
            )),
            "the field still shows the edited text: {changes:?}"
        );
    }
}
