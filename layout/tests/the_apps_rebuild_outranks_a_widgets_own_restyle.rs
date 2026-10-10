//! What the app rebuilds is what a widget shows - not what the widget wrote
//! on itself before the rebuild.
//!
//! A widget that restyles itself in a callback with `set_css_property` writes
//! a USER OVERRIDE. The engine carries an override onto the matching node of
//! every later rebuild (`StyledDom::migrate_user_overrides_from`), where it
//! outranks the rebuilt DOM's own style - so whatever the app's next build
//! says for that property never shows (the preflight contract
//! `[override-latch]`, after the Accordion latch and the TextInput
//! placeholder that never came back).
//!
//! - The ribbon's File menu lit (or dimmed) a place's pin on the click, then
//!   told the app, which pins or unpins the place and rebuilds. The pin kept
//!   the click's guess whatever the app decided, and an unpinned pin's dim
//!   outranked its own `:hover`: it no longer lit under the pointer.
//! - A token input hid its list of suggestions on Escape "until the next
//!   build", without asking the app. The user typed on, the app rebuilt the
//!   field with the suggestions for the new text - and the list stayed
//!   hidden for good, while Down moved a highlight nobody could see.
//!
//! Each check runs the window the way the shell does: the handler through
//! `invoke_single_callback_at`, its style writes through the content
//! chokepoint, the app's rebuild through the reconciliation - and compares
//! the result with a TWIN, a window built straight from the app's new state.

use std::sync::{Arc, Mutex};

use azul_core::{
    callbacks::Update,
    dom::{Dom, DomId, DomNodeId, EventFilter, HoverEventFilter, NodeId},
    events::FocusEventFilter,
    geom::LogicalSize,
    gl::OptionGlContextPtr,
    refany::RefAny,
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::Instant,
    window::{RawWindowHandle, VirtualKeyCode},
};
use azul_css::{
    props::{
        layout::LayoutDisplay,
        property::{CssProperty, CssPropertyType},
    },
    system::SystemStyle,
    AzString, StringVec,
};
use azul_layout::{
    callbacks::{Callback, CallbackChange, CallbackInfo, ExternalSystemCallbacks},
    overlay::ContentChange,
    widgets::{
        ribbon_file_menu::{
            RibbonFileMenu, RibbonFileMenuCommand, RibbonFileMenuCommandVec, RibbonFileMenuEvent,
            RibbonFileMenuOnEventCallbackType, RibbonFileMenuPlace, RibbonFileMenuPlaceVec,
            PIN_CLASS,
        },
        text_input::{TextInputOnTextInput, TextInputState, TextInputStateWrapper},
        themes::UiTheme,
        token_input::{TokenInput, TokenInputEvent, TokenInputOnEventCallbackType},
    },
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

// ---- the window, as the shell runs it ----

fn state() -> FullWindowState {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(480.0, 360.0);
    ws
}

/// A window showing `content`.
fn window(content: StyledDom) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let ws = state();
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        content,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the window lays out");
    lw
}

/// The app's rebuild, installed as the shells install one: the
/// reconciliation, the layout, its completion.
fn rebuild(lw: &mut LayoutWindow, content: StyledDom) {
    let ws = state();
    let mut next = content;
    let pending = lw.begin_reconciliation(DomId::ROOT_ID, &mut next, Instant::now());
    lw.layout_new_generation(
        next,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the rebuilt window lays out");
    lw.finish_reconciliation(DomId::ROOT_ID, &pending);
}

fn styled(lw: &LayoutWindow) -> &StyledDom {
    &lw.layout_results[&DomId::ROOT_ID].styled_dom
}

/// Every node carrying `class`, in document order.
fn with_class(lw: &LayoutWindow, class: &str) -> Vec<NodeId> {
    let node_data = styled(lw).node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .filter(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| matches!(c.as_class(), Some(s) if s == class))
        })
        .collect()
}

/// The handler `node` registered for `event`, with its payload.
fn handler(lw: &LayoutWindow, node: NodeId, event: EventFilter) -> (Callback, RefAny) {
    styled(lw).node_data.as_container()[node]
        .get_callbacks()
        .as_ref()
        .iter()
        .find(|cb| cb.event == event)
        .map(|cb| (Callback::from_core(cb.callback.clone()), cb.refany.clone()))
        .expect("harness: the node handles the event")
}

/// Runs `callback` on `node` the way the shell runs a handler, in a window
/// whose state is `ws`, and lands its style writes through the content
/// chokepoint the shell applies them through. What the handler returned.
fn run(
    lw: &mut LayoutWindow,
    node: NodeId,
    callback: &mut Callback,
    data: &mut RefAny,
    ws: &FullWindowState,
) -> Update {
    let hit = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(node)),
    };
    let (changes, update) = lw.invoke_single_callback_at(
        hit,
        callback,
        data,
        &RawWindowHandle::Unsupported,
        &OptionGlContextPtr::None,
        Arc::new(SystemStyle::default()),
        &ExternalSystemCallbacks::rust_internal(),
        &None,
        ws,
        &RendererResources::default(),
    );
    for change in changes {
        let content = match change {
            CallbackChange::ChangeNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => ContentChange::NodeCss {
                dom_id,
                node_id,
                props: properties.as_ref().to_vec(),
                override_only: false,
            },
            CallbackChange::OverrideNodeCssProperties {
                dom_id,
                node_id,
                properties,
            } => ContentChange::NodeCss {
                dom_id,
                node_id,
                props: properties.as_ref().to_vec(),
                override_only: true,
            },
            CallbackChange::SetNodeStyle {
                dom_id,
                node_id,
                style,
            } => ContentChange::NodeStyle {
                dom_id,
                node_id,
                style,
            },
            // Focus, propagation, a11y, text: not styles.
            _ => continue,
        };
        let _ = lw.apply_content_change(content);
    }
    update
}

/// A click on `node`.
fn click(lw: &mut LayoutWindow, node: NodeId) -> Update {
    let (mut callback, mut data) = handler(lw, node, EventFilter::Hover(HoverEventFilter::Click));
    let ws = lw.current_window_state.clone();
    run(lw, node, &mut callback, &mut data, &ws)
}

/// `ty` of `node` as the cascade resolves it - user overrides first, which is
/// what the layout and the display list read - at rest or under the pointer.
fn resolved(lw: &LayoutWindow, node: NodeId, ty: CssPropertyType, hovered: bool) -> Option<CssProperty> {
    let sd = styled(lw);
    let node_data = sd.node_data.as_container();
    let mut node_state = sd.get_styled_node_state(&node);
    node_state.hover = hovered;
    sd.get_css_property_cache()
        .get_property(&node_data[node], &node, &node_state, &ty)
        .cloned()
}

// ---- the ribbon's File menu: a place's pin ----

/// Every pick the app heard.
type Picks = Arc<Mutex<Vec<RibbonFileMenuEvent>>>;

/// The app's callback: it hears the pick and rebuilds - the menu is its state.
extern "C" fn rebuild_on_every_pick(
    mut data: RefAny,
    _info: CallbackInfo,
    event: RibbonFileMenuEvent,
) -> Update {
    if let Some(picks) = data.downcast_ref::<Picks>() {
        picks.lock().expect("picks").push(event);
    }
    Update::RefreshDom
}

/// The app's File menu: Close, and three places, `pinned` saying which are.
fn file_menu(picks: &Picks, pinned: [bool; 3]) -> RibbonFileMenu {
    let places: Vec<RibbonFileMenuPlace> = ["Downloads", "Pictures", "Music"]
        .iter()
        .zip(pinned)
        .map(|(label, pinned)| RibbonFileMenuPlace::create(AzString::from(*label)).with_pinned(pinned))
        .collect();
    RibbonFileMenu::create(RibbonFileMenuCommandVec::from_vec(vec![RibbonFileMenuCommand::create(
        AzString::from("close"),
        AzString::from("Close"),
    )]))
    .with_places(AzString::from("Frequent places"), RibbonFileMenuPlaceVec::from_vec(places))
    .with_on_event(
        RefAny::new(picks.clone()),
        rebuild_on_every_pick as RibbonFileMenuOnEventCallbackType,
    )
    .with_theme(UiTheme::Flat)
}

/// The menu's popup as its own window shows it: the subtree of the menu's
/// `<transient-window>`, handed over the way the shell hands it
/// (`transient::extract_subtree_as_dom`).
fn popup(menu: RibbonFileMenu) -> StyledDom {
    let app = StyledDom::create_from_dom(menu.dom());
    let content = azul_core::transient::extract_subtree_as_dom(&app, NodeId::new(0))
        .expect("harness: the menu's window holds the menu");
    StyledDom::create_from_dom(content)
}

/// The opacity of pin `place` of `lw`, at rest or under the pointer.
fn pin_opacity(lw: &LayoutWindow, place: usize, hovered: bool) -> Option<CssProperty> {
    let pins = with_class(lw, PIN_CLASS);
    assert_eq!(pins.len(), 3, "harness: a pin per place");
    resolved(lw, pins[place], CssPropertyType::Opacity, hovered)
}

/// A click on pin `place` of the open menu `lw`.
fn click_pin(lw: &mut LayoutWindow, place: usize) -> Update {
    let pin = with_class(lw, PIN_CLASS)[place];
    click(lw, pin)
}

#[test]
fn a_file_menu_pin_shows_the_pin_state_of_the_apps_rebuild() {
    let picks: Picks = Arc::new(Mutex::new(Vec::new()));
    // Downloads is pinned, Pictures and Music are not.
    let app = [true, false, false];
    let mut lw = window(popup(file_menu(&picks, app)));
    assert_ne!(
        pin_opacity(&lw, 2, false),
        pin_opacity(&lw, 0, false),
        "harness: an unpinned pin looks unlike a pinned one"
    );

    // The user pins Music. The app hears it and rebuilds - but it does not pin
    // Music (its list of pinned places is full): Music is still unpinned.
    let update = click_pin(&mut lw, 2);
    assert_eq!(picks.lock().expect("picks").len(), 1, "harness: the app heard the pin");
    assert!(
        matches!(update, Update::RefreshDom | Update::RefreshDomAllWindows),
        "harness: the app's answer is a rebuild, got {update:?}"
    );
    rebuild(&mut lw, popup(file_menu(&picks, app)));

    let twin = window(popup(file_menu(&picks, app)));
    assert_eq!(
        pin_opacity(&lw, 2, false),
        pin_opacity(&twin, 2, false),
        "after the app's rebuild Music's pin must show what the app says (unpinned), not what \
         the click guessed"
    );
}

#[test]
fn an_unpinned_file_menu_pin_lights_under_the_pointer_after_the_apps_rebuild() {
    let picks: Picks = Arc::new(Mutex::new(Vec::new()));
    let mut lw = window(popup(file_menu(&picks, [true, false, false])));

    // The user unpins Downloads, and the app unpins it and rebuilds.
    let _ = click_pin(&mut lw, 0);
    let app = [false, false, false];
    rebuild(&mut lw, popup(file_menu(&picks, app)));

    let twin = window(popup(file_menu(&picks, app)));
    assert_ne!(
        pin_opacity(&twin, 0, true),
        pin_opacity(&twin, 0, false),
        "harness: an unpinned pin lights under the pointer"
    );
    assert_eq!(
        pin_opacity(&lw, 0, false),
        pin_opacity(&twin, 0, false),
        "at rest, the unpinned pin is dim"
    );
    assert_eq!(
        pin_opacity(&lw, 0, true),
        pin_opacity(&twin, 0, true),
        "under the pointer the unpinned pin lights like any other: the dim the click wrote must \
         not outrank its :hover"
    );
}

// ---- the token input: its list of suggestions ----

/// The classes the token input gives its entry and its list of suggestions
/// (`widgets/token_input.rs`).
const ENTRY_CLASS: &str = "__azul-native-token-input-entry";
const LIST_CLASS: &str = "__azul-native-token-input-suggestions";

/// Every event the app heard.
type Heard = Arc<Mutex<Vec<TokenInputEvent>>>;

/// The app's callback: it keeps the event's state and rebuilds - the field
/// is its state.
extern "C" fn keep_and_rebuild(mut data: RefAny, _info: CallbackInfo, event: TokenInputEvent) -> Update {
    if let Some(heard) = data.downcast_ref::<Heard>() {
        heard.lock().expect("heard").push(event);
    }
    Update::RefreshDom
}

/// The app's page: a "To" field holding one recipient, `text` typed, and the
/// address book it suggests from.
fn recipients(heard: &Heard, text: &str) -> StyledDom {
    let field = TokenInput::create(
        StringVec::from_vec(vec![AzString::from("bob@example.org")]),
        AzString::from("To"),
    )
    .with_text(AzString::from(text))
    .with_suggestions(StringVec::from_vec(vec![
        AzString::from("Alan Turing <alan@example.org>"),
        AzString::from("Albert Camus <albert@example.org>"),
        AzString::from("Malcolm X <malcolm@example.org>"),
    ]))
    .with_on_event(RefAny::new(heard.clone()), keep_and_rebuild as TokenInputOnEventCallbackType)
    .with_theme(UiTheme::Flat);
    StyledDom::create_from_dom(Dom::create_body().with_css("margin: 0;").with_child(field.dom()))
}

/// A key pressed in the field's entry: the key handler the entry registered,
/// with the key down in the window's state.
fn press(lw: &mut LayoutWindow, entry: NodeId, key: VirtualKeyCode) -> Update {
    let (mut callback, mut data) =
        handler(lw, entry, EventFilter::Focus(FocusEventFilter::VirtualKeyDown));
    let mut ws = lw.current_window_state.clone();
    ws.keyboard_state.current_virtual_keycode = Some(key).into();
    ws.keyboard_state.pressed_virtual_keycodes = vec![key].into();
    run(lw, entry, &mut callback, &mut data, &ws)
}

/// What the entry's edit handler hands its hook once the engine has taken a
/// keystroke (`text_input::default_on_text_input`): the field's state, with
/// the new text.
struct Typed {
    hook: TextInputOnTextInput,
    state: TextInputState,
}

/// The edit handler's last step: the entry's hook hears the new text.
extern "C" fn edit_handler(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some((hook, state)) = data
        .downcast_ref::<Typed>()
        .map(|t| (t.hook.clone(), t.state.clone()))
    else {
        return Update::DoNothing;
    };
    hook.callback.invoke(hook.refany.clone(), info, state).update
}

/// The user types on until the entry reads `text`: its hook hears it, as the
/// entry's edit handler hands it on.
fn type_until(lw: &mut LayoutWindow, entry: NodeId, text: &str) -> Update {
    let mut data = styled(lw).node_data.as_container()[entry]
        .get_dataset()
        .cloned()
        .expect("harness: the entry keeps its state");
    let hook = {
        let wrapper = data
            .downcast_ref::<TextInputStateWrapper>()
            .expect("harness: the entry is a text input");
        wrapper
            .on_text_input
            .as_ref()
            .cloned()
            .expect("harness: the entry hands its text to the token input")
    };
    let state = TextInputState {
        text: text.chars().map(|c| c as u32).collect::<Vec<_>>().into(),
        ..TextInputState::default()
    };
    let mut callback = Callback::from_ptr(edit_handler);
    let mut payload = RefAny::new(Typed { hook, state });
    let ws = lw.current_window_state.clone();
    run(lw, entry, &mut callback, &mut payload, &ws)
}

#[test]
fn an_escaped_suggestion_list_comes_back_with_the_next_character() {
    let heard: Heard = Arc::new(Mutex::new(Vec::new()));
    let mut lw = window(recipients(&heard, "a"));
    let entry = with_class(&lw, ENTRY_CLASS)[0];
    let list = with_class(&lw, LIST_CLASS)[0];
    let hidden = Some(CssProperty::const_display(LayoutDisplay::None));
    assert_ne!(
        resolved(&lw, list, CssPropertyType::Display, false),
        hidden,
        "harness: \"a\" shows suggestions"
    );

    // Escape: the list goes, and the app is not asked.
    let _ = press(&mut lw, entry, VirtualKeyCode::Escape);
    assert_eq!(
        resolved(&lw, list, CssPropertyType::Display, false),
        hidden,
        "harness: Escape hides the list"
    );
    assert!(heard.lock().expect("heard").is_empty(), "harness: Escape tells the app nothing");

    // The next character: the app hears the new text and rebuilds the field
    // with it - and with the suggestions for it.
    let update = type_until(&mut lw, entry, "al");
    assert_eq!(update, Update::RefreshDom, "harness: the app rebuilds for the new text");
    rebuild(&mut lw, recipients(&heard, "al"));

    let twin = window(recipients(&heard, "al"));
    let twin_list = with_class(&twin, LIST_CLASS);
    assert_eq!(twin_list.len(), 1, "harness: \"al\" has suggestions");
    let list = with_class(&lw, LIST_CLASS);
    assert_eq!(list.len(), 1, "the rebuilt field has its list");
    assert_eq!(
        resolved(&lw, list[0], CssPropertyType::Display, false),
        resolved(&twin, twin_list[0], CssPropertyType::Display, false),
        "the field the app rebuilt for the next character shows its suggestions again: the list \
         Escape hid must not stay hidden for good"
    );
}
